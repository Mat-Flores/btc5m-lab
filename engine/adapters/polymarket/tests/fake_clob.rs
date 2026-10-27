// The execution path against a local fake CLOB, one scripted behaviour per connection: success, partial fill, FAK
// killed, 4xx, 5xx, no answer, cut during the send, late answer, rate limit, network guard. Plain TCP on 127.0.0.1:
// nothing leaves the machine.
use adapter_polymarket::exec::{Clob, ALLOWED};
use engine_core::clock::Clock;
use engine_core::guard::NetGuard;
use engine_core::order::{Input, Order, Phase};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone, Copy)]
enum Script { Answer(u16, &'static str, &'static str), Silent, CutAfterHead, LateAnswer(u64) }

const MATCHED: &str = r#"{"errorMsg":"","makingAmount":"5","orderID":"0xabc","status":"matched","success":true,"takingAmount":"9.0909","tradeIDs":["t1"],"transactionsHashes":[]}"#;
const PARTIAL: &str = r#"{"errorMsg":"","makingAmount":"2.5","orderID":"0xabc","status":"matched","success":true,"takingAmount":"4.5454","tradeIDs":["t1"],"transactionsHashes":[]}"#;
const KILLED: &str = r#"{"error":"no orders found to match with FAK order. FAK orders are partially filled or killed if no match is found."}"#;
const REQ: &[u8] = b"POST /order HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\n\r\n{}";

/// Reads one whole request (head, then Content-Length bytes); false if the client went away first.
async fn read_request(s: &mut TcpStream) -> bool {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(h) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..h]).to_ascii_lowercase();
            let n: usize = head.lines().find_map(|l| l.strip_prefix("content-length:")).map_or(0, |v| v.trim().parse().unwrap());
            if buf.len() >= h + 4 + n { return true; }
        }
        match s.read(&mut chunk).await { Ok(0) | Err(_) => return false, Ok(k) => buf.extend_from_slice(&chunk[..k]) }
    }
}

fn answer(status: u16, extra: &str, body: &str) -> Vec<u8> { format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes() }

async fn serve(script: Script) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut s, _) = l.accept().await.unwrap();
        match script {
            Script::CutAfterHead => { let mut b = [0u8; 16]; let _ = s.read(&mut b).await; return; }
            _ if !read_request(&mut s).await => return,
            Script::Answer(st, extra, body) => { s.write_all(&answer(st, extra, body)).await.unwrap(); }
            Script::LateAnswer(ms) => { tokio::time::sleep(Duration::from_millis(ms)).await; let _ = s.write_all(&answer(200, "", MATCHED)).await; }
            Script::Silent => {}
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    });
    port
}

async fn run(script: Script, armed: bool) -> (Order, adapter_polymarket::exec::Sent) {
    let port = serve(script).await;
    // the connection goes to the fake server; the guard is still asked about the venue
    let mut clob = Clob::new("127.0.0.1", port, false, NetGuard::new(&ALLOWED, Arc::new(AtomicBool::new(armed))));
    clob.deadline = Duration::from_millis(150);
    clob.grace = Duration::from_millis(600);
    let mut o = Order::default();
    o.apply(Input::Signed).unwrap();
    let sent = clob.post(REQ, &mut o, &Clock::new()).await;
    (o, sent)
}

#[tokio::test(flavor = "current_thread")]
async fn success_partial_killed_refused() {
    let (o, s) = run(Script::Answer(200, "", MATCHED), true).await;
    assert_eq!((o.phase, o.id.as_deref(), o.trades.len()), (Phase::Matched, Some("0xabc"), 1));
    assert!(s.written.is_some() && s.answered.is_some() && !s.late);
    // a partial fill is matched for what the venue says it matched; sizes come from its trades
    let (o, s) = run(Script::Answer(200, "", PARTIAL), true).await;
    assert_eq!(o.phase, Phase::Matched);
    assert!(String::from_utf8_lossy(&s.raw.unwrap().body).contains("\"makingAmount\":\"2.5\""));
    let (o, _) = run(Script::Answer(400, "", KILLED), true).await;
    assert_eq!(o.phase, Phase::Killed);
    let (o, _) = run(Script::Answer(400, "", r#"{"error":"invalid signature"}"#), true).await;
    assert_eq!(o.phase, Phase::Refused);
}

#[tokio::test(flavor = "current_thread")]
async fn what_says_nothing_leaves_the_order_unknown() {
    for script in [Script::Answer(502, "", "<html>bad gateway</html>"), Script::Answer(500, "", "{}"), Script::Silent, Script::CutAfterHead] {
        let (o, s) = run(script, true).await;
        assert_eq!(o.phase, Phase::Unknown);
        assert!(o.blocks_openings());
        assert!(s.written.is_some());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_late_answer_settles_the_unknown_order() {
    let (o, s) = run(Script::LateAnswer(400), true).await;
    assert!(s.late);
    assert_eq!(o.phase, Phase::Matched);
    // beyond the grace period, it stays unknown
    let (o, s) = run(Script::LateAnswer(1_500), true).await;
    assert!(s.late && s.answered.is_none());
    assert_eq!(o.phase, Phase::Unknown);
}

#[tokio::test(flavor = "current_thread")]
async fn rate_limit_asks_to_wait() {
    let (o, s) = run(Script::Answer(429, "Retry-After: 4\r\n", r#"{"error":"too many requests"}"#), true).await;
    assert_eq!((o.phase, s.back_off_s), (Phase::Refused, Some(4)));
}

/// The same order handed twice is written once: the second call finds it no longer signed.
#[tokio::test(flavor = "current_thread")]
async fn an_order_is_never_written_twice() {
    for script in [Script::Answer(200, "", MATCHED), Script::Silent] {
        let port = serve(script).await;
        let mut clob = Clob::new("127.0.0.1", port, false, NetGuard::new(&ALLOWED, Arc::new(AtomicBool::new(true))));
        clob.deadline = Duration::from_millis(100);
        clob.grace = Duration::from_millis(100);
        let mut o = Order::default();
        o.apply(Input::Signed).unwrap();
        let first = clob.post(REQ, &mut o, &Clock::new()).await;
        let phase = o.phase;
        let second = clob.post(REQ, &mut o, &Clock::new()).await;
        assert!(first.written.is_some() && second.written.is_none());
        assert_eq!(o.phase, phase);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn disarmed_nothing_is_written() {
    let (o, s) = run(Script::Answer(200, "", MATCHED), false).await;
    assert_eq!(o.phase, Phase::Refused);
    assert!(s.written.is_none());
}
