//! Feed contract and runner. An adapter only knows its source: URL, subscription frames, heartbeat, decoding.
//! The runner does the rest for every source alike: connection, reconnection, silence and gap detection (through
//! the supervisor), duplicate dropping, health reporting.
use crate::clock::{Clock, Stamp};
use crate::event::{Caps, Event, SourceId};
use crate::supervisor::{Health, Supervisor, Verdict};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, PartialEq)]
pub struct DecodeError(pub String);

pub trait Adapter: Send {
    fn url(&self) -> String;
    fn caps(&self) -> Caps;
    /// Frames that subscribe `topics` on a fresh connection.
    fn subscribe_all(&self, topics: &[String]) -> Vec<String>;
    /// Frames that add `topics` to an open connection.
    fn subscribe_more(&self, topics: &[String]) -> Vec<String> { self.subscribe_all(topics) }
    fn unsubscribe(&self, topics: &[String]) -> Vec<String>;
    /// Application-level keep-alive the source requires, if any.
    fn heartbeat(&self) -> Option<(Duration, String)>;
    /// Decodes one text frame into events. Returns the frame's sequence number when the source numbers frames.
    /// Ok with nothing pushed: a frame without data (ack, pong). Err: unreadable, never guessed.
    fn decode(&mut self, text: &str, recv: Stamp, source: SourceId, out: &mut Vec<Event>) -> Result<Option<u64>, DecodeError>;
}

#[derive(Debug)]
pub enum Cmd { Subscribe(Vec<String>), Unsubscribe(Vec<String>), Resync }

#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Event(Event),
    Health { source: SourceId, at: Stamp, health: Health },
    /// An unreadable frame: counted and kept (its beginning, or the whole frame when traced) for the ledger, never guessed.
    DecodeError { source: SourceId, at: Stamp, error: String, head: String },
    // the three below only when traced (`run_with(.., trace: true)`), for source-tracing
    /// Every text frame received, before it is decoded.
    Frame { source: SourceId, conn: u16, at: Stamp, text: String },
    /// A connection opened (before its subscriptions are written).
    Connected { source: SourceId, conn: u16, at: Stamp, url: String },
    /// A subscription or unsubscription frame written (heartbeats are not).
    Sent { source: SourceId, conn: u16, at: Stamp, text: String },
}

/// The runner's settings, the same for every source (engine config `runtime.feed`).
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Timing {
    /// Period of the silence check.
    pub timer_ms: u64,
    pub connect_timeout_ms: u64,
    /// Wait before a reconnection while connections keep failing quickly: doubles from `min_backoff_ms` up to `max_backoff_ms`.
    pub min_backoff_ms: u64,
    pub max_backoff_ms: u64,
    /// A connection that lived this long reconnects at once when it drops.
    pub stable_ms: u64,
}
impl Default for Timing { fn default() -> Self { Timing { timer_ms: 100, connect_timeout_ms: 10_000, min_backoff_ms: 100, max_backoff_ms: 2_000, stable_ms: 10_000 } } }

/// Runs one source until `out` is closed. Reconnects at once after a drop; backs off only while connections keep
/// failing quickly.
pub async fn run(a: Box<dyn Adapter>, source: SourceId, topics: Vec<String>, clock: Arc<Clock>, cmd: UnboundedReceiver<Cmd>, out: UnboundedSender<Out>) {
    run_with(a, source, 0, topics, clock, cmd, out, false).await
}

/// `run`, plus, when `trace`, every frame received and written and every connection (`Out::Frame`, `Sent`,
/// `Connected`), and unreadable frames whole. `conn`: index of this connection within its source, as traced.
#[allow(clippy::too_many_arguments)]
pub async fn run_with(a: Box<dyn Adapter>, source: SourceId, conn: u16, topics: Vec<String>, clock: Arc<Clock>, cmd: UnboundedReceiver<Cmd>, out: UnboundedSender<Out>, trace: bool) {
    let (silence, hb) = (a.caps().silence, a.heartbeat().map(|h| h.0));
    run_tuned(a, source, conn, topics, clock, cmd, out, trace, Timing::default(), silence, hb).await
}

/// `run_with`, with the runner's settings, the source's silence and its heartbeat period given (the adapter's own
/// are its defaults). A heartbeat period for a source without heartbeat is ignored.
#[allow(clippy::too_many_arguments)]
pub async fn run_tuned(mut a: Box<dyn Adapter>, source: SourceId, conn: u16, mut topics: Vec<String>, clock: Arc<Clock>, mut cmd: UnboundedReceiver<Cmd>, out: UnboundedSender<Out>, trace: bool,
    t: Timing, silence: Duration, heartbeat: Option<Duration>) {
    let ms = Duration::from_millis;
    let mut sup = Supervisor::new(Caps { silence, ..a.caps() });
    let mut backoff = Duration::ZERO;
    let mut events = Vec::with_capacity(64);
    let mut cmd_open = true;
    let health = |h: Option<Health>, out: &UnboundedSender<Out>, at: Stamp| if let Some(health) = h { let _ = out.send(Out::Health { source, at, health }); };
    loop {
        if out.is_closed() { return; }
        if !backoff.is_zero() { tokio::time::sleep(backoff).await; }
        let started = tokio::time::Instant::now();
        let ws = match tokio::time::timeout(ms(t.connect_timeout_ms), tokio_tungstenite::connect_async(a.url())).await {
            Ok(Ok((ws, _))) => ws,
            _ => { health(sup.on_disconnect(), &out, clock.now()); backoff = (backoff * 2).clamp(ms(t.min_backoff_ms), ms(t.max_backoff_ms)); continue; }
        };
        let (mut tx, mut rx) = ws.split();
        sup.on_connect(clock.now().mono_ns);
        if trace { let _ = out.send(Out::Connected { source, conn, at: clock.now(), url: a.url() }); }
        let sent = |f: &String, out: &UnboundedSender<Out>| if trace { let _ = out.send(Out::Sent { source, conn, at: clock.now(), text: f.clone() }); };
        let mut ok = true;
        for f in a.subscribe_all(&topics) { sent(&f, &out); ok &= tx.send(Message::text(f)).await.is_ok(); }
        let hb = a.heartbeat().map(|(p, text)| (heartbeat.unwrap_or(p), text));
        let mut hb_tick = tokio::time::interval(hb.as_ref().map_or(Duration::from_secs(3600), |h| h.0));
        hb_tick.tick().await;
        let mut timer = tokio::time::interval(ms(t.timer_ms));
        while ok {
            tokio::select! {
                m = rx.next() => match m {
                    Some(Ok(msg)) if msg.is_text() => {
                        let recv = clock.now();
                        events.clear();
                        let text = msg.to_text().unwrap_or("");
                        if trace { let _ = out.send(Out::Frame { source, conn, at: recv, text: text.to_string() }); }
                        match a.decode(text, recv, source, &mut events) {
                            Ok(seq) => {
                                let (v, h) = sup.on_message(recv.mono_ns, seq);
                                health(h, &out, recv);
                                match v {
                                    Verdict::Accept => for e in events.drain(..) { if out.send(Out::Event(e)).is_err() { return; } },
                                    Verdict::Duplicate => {}
                                    Verdict::Resync => ok = false,
                                }
                            }
                            Err(e) => {
                                let _ = out.send(Out::DecodeError { source, at: recv, error: e.0, head: if trace { text.to_string() } else { text.chars().take(300).collect() } });
                                health(sup.on_decode_error(), &out, recv);
                                ok = false;
                            }
                        }
                    }
                    Some(Ok(msg)) if msg.is_close() => ok = false,
                    Some(Ok(_)) => {} // ping/pong/binary: pongs are answered by the WebSocket layer
                    Some(Err(_)) | None => ok = false,
                },
                _ = timer.tick() => if let Some(h) = sup.on_timer(clock.now().mono_ns) { health(Some(h), &out, clock.now()); ok = false; },
                _ = hb_tick.tick(), if hb.is_some() => { ok &= tx.send(Message::text(hb.as_ref().unwrap().1.clone())).await.is_ok(); }
                c = cmd.recv(), if cmd_open => match c {
                    Some(Cmd::Subscribe(t)) => { let new: Vec<String> = t.into_iter().filter(|x| !topics.contains(x)).collect(); if !new.is_empty() { for f in a.subscribe_more(&new) { sent(&f, &out); ok &= tx.send(Message::text(f)).await.is_ok(); } topics.extend(new); } }
                    Some(Cmd::Unsubscribe(t)) => { topics.retain(|x| !t.contains(x)); for f in a.unsubscribe(&t) { sent(&f, &out); ok &= tx.send(Message::text(f)).await.is_ok(); } }
                    Some(Cmd::Resync) => ok = false,
                    None => cmd_open = false,
                },
            }
        }
        let _ = tx.close().await;
        // a drop while valid is reported now; an already reported reason (silence, gap, decode) is kept as the cause
        if matches!(sup.health, Health::Valid) { health(sup.on_disconnect(), &out, clock.now()); } else { sup.reconnects += 1; }
        backoff = if started.elapsed() >= ms(t.stable_ms) { Duration::ZERO } else { (backoff * 2).clamp(ms(t.min_backoff_ms), ms(t.max_backoff_ms)) };
    }
}
