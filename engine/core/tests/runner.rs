// Fault injection on the runner, against a local WebSocket server playing a fake source: silence, gap, duplicate,
// truncated frame and a server-side close must each make the source invalid, then reconnect and resubscribe.
use engine_core::clock::{Clock, Stamp};
use engine_core::event::{BookKind, Caps, Event, Payload, SourceId};
use engine_core::feed::{run, Adapter, Cmd, DecodeError, Out};
use engine_core::supervisor::{Health, Reason};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

struct Fake { url: String }

impl Adapter for Fake {
    fn url(&self) -> String { self.url.clone() }
    fn caps(&self) -> Caps { Caps { sequence: true, event_time: false, book: BookKind::None, silence: Duration::from_millis(300) } }
    fn subscribe_all(&self, topics: &[String]) -> Vec<String> { vec![format!("SUB {}", topics.join(","))] }
    fn unsubscribe(&self, topics: &[String]) -> Vec<String> { vec![format!("UNSUB {}", topics.join(","))] }
    fn heartbeat(&self) -> Option<(Duration, String)> { None }
    fn decode(&mut self, text: &str, recv: Stamp, source: SourceId, out: &mut Vec<Event>) -> Result<Option<u64>, DecodeError> {
        #[derive(serde::Deserialize)] struct F { seq: u64, v: i64 }
        let f: F = serde_json::from_str(text).map_err(|e| DecodeError(e.to_string()))?;
        out.push(Event { source, recv, event_ms: None, seq: Some(f.seq), payload: Payload::Oracle { value: f.v } });
        Ok(Some(f.seq))
    }
}

/// What the fake server does on each successive connection.
#[derive(Clone, Copy)]
enum Script { SilenceAfter3, GapAt3, DuplicateThenClose, TruncatedAt2, Steady }

async fn serve(listener: TcpListener, scripts: Vec<Script>, subs: mpsc::UnboundedSender<String>) {
    for s in scripts {
        let (tcp, _) = listener.accept().await.unwrap();
        let subs = subs.clone();
        tokio::spawn(async move { play(tcp, s, subs).await });
    }
}

async fn play(tcp: tokio::net::TcpStream, s: Script, subs: mpsc::UnboundedSender<String>) {
    {
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        if let Some(Ok(m)) = ws.next().await { subs.send(m.to_text().unwrap().to_string()).unwrap(); }
        let frame = |seq: u64| Message::text(format!("{{\"seq\":{seq},\"v\":{}}}", seq * 10));
        // sends may fail once the client has dropped the connection: that is the point of these scripts
        match s {
            Script::SilenceAfter3 => { for i in 1..=3 { let _ = ws.send(frame(i)).await; } tokio::time::sleep(Duration::from_secs(5)).await; }
            Script::GapAt3 => { for i in [1, 2, 4, 5] { let _ = ws.send(frame(i)).await; } tokio::time::sleep(Duration::from_secs(5)).await; }
            Script::DuplicateThenClose => { for i in [1, 2, 2, 1, 3] { let _ = ws.send(frame(i)).await; } let _ = ws.close(None).await; }
            Script::TruncatedAt2 => { let _ = ws.send(frame(1)).await; let _ = ws.send(Message::text("{\"seq\":2,\"v\"")).await; tokio::time::sleep(Duration::from_secs(5)).await; }
            Script::Steady => { for i in 1..=5 { let _ = ws.send(frame(i)).await; tokio::time::sleep(Duration::from_millis(20)).await; } tokio::time::sleep(Duration::from_secs(30)).await; }
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn faults_make_the_source_invalid_then_it_recovers() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let (sub_tx, mut sub_rx) = mpsc::unbounded_channel();
    tokio::spawn(serve(listener, vec![Script::SilenceAfter3, Script::GapAt3, Script::DuplicateThenClose, Script::TruncatedAt2, Script::Steady], sub_tx));
    let (out_tx, mut out_rx) = mpsc::unbounded_channel();
    let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Cmd>();
    tokio::spawn(run(Box::new(Fake { url }), 7, vec!["t1".into(), "t2".into()], Arc::new(Clock::new()), cmd_rx, out_tx));

    let mut values = Vec::new();
    let mut reasons = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while values.len() < 3 + 2 + 3 + 1 + 5 {
        let Ok(Some(o)) = tokio::time::timeout_at(deadline, out_rx.recv()).await else { panic!("runner stalled: values {values:?}, reasons {reasons:?}") };
        match o {
            Out::Event(e) => { assert_eq!(e.source, 7); if let Payload::Oracle { value } = e.payload { values.push(value); } }
            Out::Health { health: Health::Invalid(r), .. } => reasons.push(r),
            Out::Health { .. } | Out::DecodeError { .. } => {}
            Out::Frame { .. } | Out::Connected { .. } | Out::Sent { .. } => panic!("traced output without the flag"),
        }
    }
    // every connection re-sent the full subscription
    let mut subs = Vec::new();
    while let Ok(s) = sub_rx.try_recv() { subs.push(s); }
    assert_eq!(subs.len(), 5, "{subs:?}");
    assert!(subs.iter().all(|s| s == "SUB t1,t2"));
    // values: 3 before the silence, 2 before the gap (4 and 5 dropped), 1,2,3 with duplicates dropped, 1 before the
    // truncated frame, then the steady 5
    assert_eq!(values, vec![10, 20, 30, 10, 20, 10, 20, 30, 10, 10, 20, 30, 40, 50]);
    assert!(matches!(reasons[0], Reason::Silence { .. }), "{reasons:?}");
    assert!(reasons.iter().any(|r| matches!(r, Reason::SeqGap { expected: 3, got: 4 })), "{reasons:?}");
    assert!(reasons.contains(&Reason::Disconnected), "{reasons:?}");
    assert!(reasons.contains(&Reason::Decode), "{reasons:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn subscriptions_follow_commands_on_the_open_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let (got_tx, mut got_rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        let mut n = 0u64;
        loop {
            tokio::select! {
                m = ws.next() => match m { Some(Ok(m)) if m.is_text() => got_tx.send(m.to_text().unwrap().to_string()).unwrap(), _ => return },
                _ = tokio::time::sleep(Duration::from_millis(50)) => { n += 1; if ws.send(Message::text(format!("{{\"seq\":{n},\"v\":1}}"))).await.is_err() { return; } }
            }
        }
    });
    let (out_tx, _out_rx) = mpsc::unbounded_channel();
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    tokio::spawn(run(Box::new(Fake { url }), 1, vec!["a".into()], Arc::new(Clock::new()), cmd_rx, out_tx));
    assert_eq!(got_rx.recv().await.unwrap(), "SUB a");
    cmd_tx.send(Cmd::Subscribe(vec!["a".into(), "b".into()])).unwrap();
    assert_eq!(got_rx.recv().await.unwrap(), "SUB b"); // only the new topic
    cmd_tx.send(Cmd::Unsubscribe(vec!["a".into()])).unwrap();
    assert_eq!(got_rx.recv().await.unwrap(), "UNSUB a");
}

/// Traced: the connection, the frames written and every frame received (an unreadable one whole), in order.
#[tokio::test(flavor = "current_thread")]
async fn traced_run_reports_connections_and_every_frame() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let long_bad = format!("{{\"seq\":2,\"v\":\"{}\"", "x".repeat(400));
    let bad = long_bad.clone();
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        let _ = ws.next().await;
        let _ = ws.send(Message::text("{\"seq\":1,\"v\":5}")).await;
        let _ = ws.send(Message::text(bad)).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
    });
    let (out_tx, mut out_rx) = mpsc::unbounded_channel();
    let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Cmd>();
    tokio::spawn(engine_core::feed::run_with(Box::new(Fake { url: url.clone() }), 3, 2, vec!["a".into()], Arc::new(Clock::new()), cmd_rx, out_tx, true));
    let mut seen = Vec::new();
    while seen.len() < 6 {
        let o = tokio::time::timeout(Duration::from_secs(5), out_rx.recv()).await.unwrap().unwrap();
        let k = match o {
            Out::Connected { source: 3, conn: 2, url: u, .. } => { assert_eq!(u, url); "connected".to_string() }
            Out::Sent { conn: 2, text, .. } => format!("sent {text}"),
            Out::Frame { conn: 2, text, .. } => format!("frame {}", text.len()),
            Out::Event(_) => "event".into(),
            Out::DecodeError { head, .. } => { assert_eq!(head, long_bad); "decode error".into() }
            Out::Health { .. } => continue,
            o => panic!("{o:?}"),
        };
        seen.push(k);
    }
    assert_eq!(seen, vec!["connected".to_string(), "sent SUB a".into(), "frame 15".into(), "event".into(), format!("frame {}", long_bad.len()), "decode error".into()]);
}
