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
    /// An unreadable frame: counted and kept (its beginning) for the ledger, never guessed.
    DecodeError { source: SourceId, at: Stamp, error: String, head: String },
}

const TIMER: Duration = Duration::from_millis(100);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_BACKOFF: Duration = Duration::from_secs(2);
const STABLE: Duration = Duration::from_secs(10);

/// Runs one source until `out` is closed. Reconnects at once after a drop; backs off only while connections keep
/// failing quickly.
pub async fn run(mut a: Box<dyn Adapter>, source: SourceId, mut topics: Vec<String>, clock: Arc<Clock>, mut cmd: UnboundedReceiver<Cmd>, out: UnboundedSender<Out>) {
    let mut sup = Supervisor::new(a.caps());
    let mut backoff = Duration::ZERO;
    let mut events = Vec::with_capacity(64);
    let mut cmd_open = true;
    let health = |h: Option<Health>, out: &UnboundedSender<Out>, at: Stamp| if let Some(health) = h { let _ = out.send(Out::Health { source, at, health }); };
    loop {
        if out.is_closed() { return; }
        if !backoff.is_zero() { tokio::time::sleep(backoff).await; }
        let started = tokio::time::Instant::now();
        let ws = match tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(a.url())).await {
            Ok(Ok((ws, _))) => ws,
            _ => { health(sup.on_disconnect(), &out, clock.now()); backoff = (backoff * 2).clamp(Duration::from_millis(100), MAX_BACKOFF); continue; }
        };
        let (mut tx, mut rx) = ws.split();
        sup.on_connect(clock.now().mono_ns);
        let mut ok = true;
        for f in a.subscribe_all(&topics) { ok &= tx.send(Message::text(f)).await.is_ok(); }
        let hb = a.heartbeat();
        let mut hb_tick = tokio::time::interval(hb.as_ref().map_or(Duration::from_secs(3600), |h| h.0));
        hb_tick.tick().await;
        let mut timer = tokio::time::interval(TIMER);
        while ok {
            tokio::select! {
                m = rx.next() => match m {
                    Some(Ok(msg)) if msg.is_text() => {
                        let recv = clock.now();
                        events.clear();
                        let text = msg.to_text().unwrap_or("");
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
                                let _ = out.send(Out::DecodeError { source, at: recv, error: e.0, head: text.chars().take(300).collect() });
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
                    Some(Cmd::Subscribe(t)) => { let new: Vec<String> = t.into_iter().filter(|x| !topics.contains(x)).collect(); if !new.is_empty() { for f in a.subscribe_more(&new) { ok &= tx.send(Message::text(f)).await.is_ok(); } topics.extend(new); } }
                    Some(Cmd::Unsubscribe(t)) => { topics.retain(|x| !t.contains(x)); for f in a.unsubscribe(&t) { ok &= tx.send(Message::text(f)).await.is_ok(); } }
                    Some(Cmd::Resync) => ok = false,
                    None => cmd_open = false,
                },
            }
        }
        let _ = tx.close().await;
        // a drop while valid is reported now; an already reported reason (silence, gap, decode) is kept as the cause
        if matches!(sup.health, Health::Valid) { health(sup.on_disconnect(), &out, clock.now()); } else { sup.reconnects += 1; }
        backoff = if started.elapsed() >= STABLE { Duration::ZERO } else { (backoff * 2).clamp(Duration::from_millis(100), MAX_BACKOFF) };
    }
}
