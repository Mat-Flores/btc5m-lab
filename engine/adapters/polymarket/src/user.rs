//! Polymarket `user` channel: trades and order closings of our own orders. Format from the official SDK's schemas
//! (`@polymarket/bindings` 0.13, `UserTradeEventSchema`, `UserOrderEventSchema`) `[non vérifié sur message réel]`:
//! no real order has been sent yet. Topics are condition ids. Only the taker side is read: the engine sends FAK
//! orders, which are always takers.
use crate::exec::Creds;
use engine_core::clock::Stamp;
use engine_core::event::{BookKind, Caps, Event, Payload, SourceId};
use engine_core::feed::{Adapter, DecodeError};
use engine_core::money::parse_micro;
use engine_core::order::TradeStatus;
use serde::Deserialize;
use std::time::Duration;

pub const URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/user";

pub struct User { pub creds: Creds }

#[derive(Deserialize)]
struct Msg<'a> {
    event_type: &'a str,
    id: String,
    #[serde(default)] taker_order_id: Option<String>,
    #[serde(default)] size: Option<&'a str>,
    #[serde(default)] price: Option<&'a str>,
    #[serde(default)] status: Option<&'a str>,
    #[serde(default, rename = "type")] kind: Option<&'a str>,
    timestamp: &'a str,
}

fn need<'a>(x: Option<&'a str>, field: &str) -> Result<&'a str, DecodeError> { x.ok_or_else(|| DecodeError(format!("no {field}"))) }

fn status(s: &str) -> Option<TradeStatus> {
    Some(match s.to_ascii_uppercase().trim_start_matches("TRADE_STATUS_") {
        "MATCHED" | "MATCHED_NOT_BROADCASTED" => TradeStatus::Matched,
        "MINED" => TradeStatus::Mined,
        "RETRYING" => TradeStatus::Retrying,
        "CONFIRMED" => TradeStatus::Confirmed,
        "FAILED" => TradeStatus::Failed,
        _ => return None,
    })
}

impl Adapter for User {
    fn url(&self) -> String { URL.into() }
    // a quiet account sends nothing: the heartbeat answers keep it alive, so silence means three missed answers
    fn caps(&self) -> Caps { Caps { sequence: false, event_time: true, book: BookKind::None, silence: Duration::from_secs(30) } }
    fn subscribe_all(&self, topics: &[String]) -> Vec<String> {
        vec![serde_json::json!({ "auth": { "apiKey": self.creds.key, "passphrase": self.creds.passphrase, "secret": self.creds.secret }, "type": "user", "markets": topics }).to_string()]
    }
    fn subscribe_more(&self, topics: &[String]) -> Vec<String> { vec![serde_json::json!({ "markets": topics, "operation": "subscribe" }).to_string()] }
    fn unsubscribe(&self, topics: &[String]) -> Vec<String> { vec![serde_json::json!({ "markets": topics, "operation": "unsubscribe" }).to_string()] }
    fn heartbeat(&self) -> Option<(Duration, String)> { Some((Duration::from_secs(10), "PING".into())) }

    fn decode(&mut self, text: &str, recv: Stamp, source: SourceId, out: &mut Vec<Event>) -> Result<Option<u64>, DecodeError> {
        if text == "PONG" { return Ok(None); }
        let one = |m: Msg, out: &mut Vec<Event>| -> Result<(), DecodeError> {
            let ts = m.timestamp.parse().map_err(|e| DecodeError(format!("timestamp: {e}")))?;
            let payload = match (m.event_type, m.kind) {
                ("trade", _) => {
                    let st = need(m.status, "status")?;
                    Payload::Fill {
                        order: m.taker_order_id.ok_or_else(|| DecodeError("no taker_order_id".into()))?, trade: m.id,
                        status: status(st).ok_or_else(|| DecodeError(format!("status {st}")))?,
                        size: parse_micro(need(m.size, "size")?).map_err(|e| DecodeError(format!("size: {e:?}")))?,
                        price: parse_micro(need(m.price, "price")?).map_err(|e| DecodeError(format!("price: {e:?}")))?,
                    }
                }
                ("order", Some("CANCELLATION")) => Payload::OrderClosed { order: m.id },
                _ => return Ok(()), // placements and updates: the answer and the trades already say it
            };
            out.push(Event { source, recv, event_ms: Some(ts), seq: None, payload });
            Ok(())
        };
        // one event or an array of them
        if text.trim_start().starts_with('[') {
            let v: Vec<Msg> = serde_json::from_str(text).map_err(|e| DecodeError(format!("json: {e}")))?;
            let start = out.len();
            for m in v { if let Err(e) = one(m, out) { out.truncate(start); return Err(e); } }
        } else {
            one(serde_json::from_str(text).map_err(|e| DecodeError(format!("json: {e}")))?, out)?;
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::conformance;
    use engine_core::order::{AckStatus, Input, Order, Phase};
    use proptest::prelude::*;

    fn user() -> User { User { creds: Creds { key: "k".into(), secret: "s".into(), passphrase: "p".into() } } }
    fn frames() -> Vec<String> {
        let path = format!("{}/../../fixtures/sdk/user-events.json", env!("CARGO_MANIFEST_DIR"));
        let v: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        v.into_iter().map(|m| m.to_string()).collect()
    }

    #[test]
    fn schema_frames_decode() {
        let ev = conformance::decodes(&mut user(), &frames());
        assert_eq!(ev.len(), 4);
        assert_eq!(ev[0].payload, Payload::Fill { order: "0xabc".into(), trade: "t1".into(), status: TradeStatus::Matched, size: 9_090_900, price: 550_000 });
        assert!(matches!(&ev[1].payload, Payload::Fill { status: TradeStatus::Confirmed, .. }));
        assert_eq!(ev[3].payload, Payload::OrderClosed { order: "0xabd".into() });
        // the same events in one array frame
        let arr = format!("[{}]", frames().join(","));
        assert_eq!(conformance::decodes(&mut user(), &[arr]).len(), 4);
    }

    /// A duplicated user message (here: CONFIRMED twice, the second in the other status spelling) changes nothing.
    #[test]
    fn duplicate_user_messages_change_nothing() {
        let mut o = Order::default();
        o.apply(Input::Signed).unwrap();
        o.apply(Input::Sent).unwrap();
        o.apply(Input::Accepted { id: "0xabc".into(), status: AckStatus::Matched, trades: vec!["t1".into()] }).unwrap();
        let mut changes = vec![];
        for e in conformance::decodes(&mut user(), &frames()[..3]) {
            if let Payload::Fill { trade, status, size, price, .. } = e.payload { changes.push(o.apply(Input::Trade { id: trade, status, size, price }).unwrap()); }
        }
        assert_eq!(changes, vec![false, true, false]);
        assert_eq!((o.phase, o.matched_size()), (Phase::Confirmed, 9_090_900));
    }

    #[test]
    fn truncated_frames_are_refused() { for f in frames() { conformance::truncations_refused(&mut user(), &f); } }

    #[test]
    fn subscription_frames_match_the_sdk() {
        assert_eq!(user().subscribe_all(&["0xc".into()]), vec![r#"{"auth":{"apiKey":"k","passphrase":"p","secret":"s"},"markets":["0xc"],"type":"user"}"#]);
        assert_eq!(user().subscribe_more(&["0xc".into()]), vec![r#"{"markets":["0xc"],"operation":"subscribe"}"#]);
    }

    proptest! {
        #[test]
        fn garbage_never_panics(s in ".{0,200}") { conformance::never_panics(&mut user(), &s); }
    }
}
