//! CLOB `market` channel: book snapshots, level changes (with the best prices Polymarket announces after each one),
//! tick size changes and trades. No sequence numbers on this channel; topics are token (asset) ids.
use engine_core::clock::Stamp;
use engine_core::event::{BookKind, Caps, Change, Event, Level, Payload, Side, SourceId};
use engine_core::feed::{Adapter, DecodeError};
use engine_core::money::{parse_micro, Micro};
use serde::Deserialize;
use std::time::Duration;

pub const URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";
/// Calibrated on 4 recorded sessions (~17 M messages): p99.99 <= 405 ms, max outside outages 2.8 s (01-audit.md, phase 4).
pub const SILENCE: Duration = Duration::from_secs(3);

pub struct Market;

#[derive(Deserialize)]
struct Lvl { price: String, size: String }

#[derive(Deserialize)]
struct Pc { asset_id: String, price: String, size: String, side: String, best_bid: Option<String>, best_ask: Option<String> }

#[derive(Deserialize)]
struct Msg {
    event_type: String,
    asset_id: Option<String>,
    timestamp: Option<String>,
    bids: Option<Vec<Lvl>>,
    asks: Option<Vec<Lvl>>,
    tick_size: Option<String>,
    price_changes: Option<Vec<Pc>>,
    new_tick_size: Option<String>,
    price: Option<String>,
    size: Option<String>,
    side: Option<String>,
    transaction_hash: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Frame { Many(Vec<Msg>), One(Msg) }

fn err<E: std::fmt::Debug>(what: &'static str) -> impl FnOnce(E) -> DecodeError { move |e| DecodeError(format!("{what}: {e:?}")) }
fn num(s: &Option<String>, what: &'static str) -> Result<Micro, DecodeError> { parse_micro(s.as_deref().ok_or_else(|| DecodeError(format!("missing {what}")))?).map_err(err(what)) }
fn side(s: &str) -> Result<Side, DecodeError> { match s { "BUY" => Ok(Side::Bid), "SELL" => Ok(Side::Ask), x => Err(DecodeError(format!("side {x}"))) } }
fn levels(v: &Option<Vec<Lvl>>) -> Result<Vec<Level>, DecodeError> {
    v.as_ref().map_or(Ok(vec![]), |v| v.iter().map(|l| Ok(Level { price: parse_micro(&l.price).map_err(err("level price"))?, size: parse_micro(&l.size).map_err(err("level size"))? })).collect())
}

impl Adapter for Market {
    fn url(&self) -> String { URL.into() }
    fn caps(&self) -> Caps { Caps { sequence: false, event_time: true, book: BookKind::Full, silence: SILENCE } }
    fn subscribe_all(&self, topics: &[String]) -> Vec<String> {
        if topics.is_empty() { return vec![]; }
        vec![serde_json::json!({ "assets_ids": topics, "type": "market" }).to_string()]
    }
    fn subscribe_more(&self, topics: &[String]) -> Vec<String> { vec![serde_json::json!({ "assets_ids": topics, "operation": "subscribe" }).to_string()] }
    fn unsubscribe(&self, topics: &[String]) -> Vec<String> { vec![serde_json::json!({ "assets_ids": topics, "operation": "unsubscribe" }).to_string()] }
    fn heartbeat(&self) -> Option<(Duration, String)> { Some((Duration::from_secs(10), "PING".into())) }

    fn decode(&mut self, text: &str, recv: Stamp, source: SourceId, out: &mut Vec<Event>) -> Result<Option<u64>, DecodeError> {
        if text == "PONG" { return Ok(None); }
        let msgs = match serde_json::from_str::<Frame>(text).map_err(err("json"))? { Frame::Many(v) => v, Frame::One(m) => vec![m] };
        let start = out.len();
        let r = (|| {
            for m in msgs {
                let event_ms = m.timestamp.as_deref().map(|t| t.parse::<u64>().map_err(err("timestamp"))).transpose()?;
                let asset = || m.asset_id.clone().ok_or_else(|| DecodeError("missing asset_id".into()));
                let payload = match m.event_type.as_str() {
                    "book" => Payload::BookSnapshot { asset: asset()?, bids: levels(&m.bids)?, asks: levels(&m.asks)?, tick: m.tick_size.as_deref().map(parse_micro).transpose().map_err(err("tick_size"))? },
                    "price_change" => Payload::BookDelta { changes: m.price_changes.as_ref().ok_or_else(|| DecodeError("missing price_changes".into()))?.iter().map(|c| Ok(Change {
                        asset: c.asset_id.clone(), side: side(&c.side)?, price: parse_micro(&c.price).map_err(err("price"))?, size: parse_micro(&c.size).map_err(err("size"))?,
                        best_bid: c.best_bid.as_deref().map(parse_micro).transpose().map_err(err("best_bid"))?,
                        best_ask: c.best_ask.as_deref().map(parse_micro).transpose().map_err(err("best_ask"))?,
                    })).collect::<Result<_, DecodeError>>()? },
                    "tick_size_change" => Payload::TickChange { asset: asset()?, tick: num(&m.new_tick_size, "new_tick_size")? },
                    "last_trade_price" => Payload::Trade { asset: asset()?, price: num(&m.price, "price")?, size: num(&m.size, "size")?, side: side(m.side.as_deref().unwrap_or(""))?, id: m.transaction_hash.clone().unwrap_or_default() },
                    _ => continue, // other event types carry nothing the engine uses
                };
                out.push(Event { source, recv, event_ms, seq: None, payload });
            }
            Ok(())
        })();
        if r.is_err() { out.truncate(start); } // a frame is used whole or not at all
        r.map(|_| None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::conformance;
    use proptest::prelude::*;

    fn fixture(name: &str) -> Vec<String> {
        let path = format!("{}/../../fixtures/raw/{name}", env!("CARGO_MANIFEST_DIR"));
        let v: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        v.into_iter().map(|m| m.to_string()).collect()
    }
    fn all() -> Vec<String> { ["polymarket-book.json", "polymarket-book-initial-array.json", "polymarket-price_change.json", "polymarket-last_trade_price.json", "polymarket-tick_size_change.json"].iter().flat_map(|f| fixture(f)).collect() }

    #[test]
    fn real_frames_decode_to_the_right_events() {
        let ev = conformance::decodes(&mut Market, &all());
        let book = ev.iter().find_map(|e| match &e.payload { Payload::BookSnapshot { bids, tick, .. } => Some((bids.len(), *tick, e.event_ms)), _ => None }).unwrap();
        assert_eq!(book.1, Some(10_000)); // tick "0.01"
        assert_eq!(book.2, Some(1_792_359_572_072));
        assert!(book.0 > 10);
        let ch = ev.iter().find_map(|e| match &e.payload { Payload::BookDelta { changes } => Some(changes[0].clone()), _ => None }).unwrap();
        assert_eq!((ch.side, ch.price, ch.size, ch.best_bid, ch.best_ask), (Side::Bid, 350_000, 222_030_000, Some(430_000), Some(440_000)));
        assert!(ev.iter().any(|e| matches!(&e.payload, Payload::TickChange { tick: 1_000, .. })));
        assert!(ev.iter().any(|e| matches!(&e.payload, Payload::Trade { price: 570_000, size: 17_543_860, side: Side::Bid, .. })));
        // arrays of events are decoded too
        let objects: Vec<String> = all().into_iter().filter(|f| f.starts_with('{')).collect();
        let joined = format!("[{}]", objects.join(","));
        assert_eq!(conformance::decodes(&mut Market, &[joined]).len(), conformance::decodes(&mut Market, &objects).len());
    }

    #[test]
    fn truncated_frames_are_refused() { for f in all() { conformance::truncations_refused(&mut Market, &f); } }

    #[test]
    fn book_without_tick_size_keeps_the_known_grid() {
        let f = r#"{"market":"0x1", "asset_id":"7", "bids":[{"price":"0.01", "size":"9376.25"}], "asks":[], "timestamp":"1", "event_type":"book"}"#;
        let ev = conformance::decodes(&mut Market, &[f.into()]);
        assert!(matches!(&ev[0].payload, Payload::BookSnapshot { tick: None, .. }));
    }

    #[test]
    fn pong_and_unknown_events_carry_nothing() {
        assert!(conformance::decodes(&mut Market, &["PONG".into(), r#"{"event_type":"best_bid_ask","asset_id":"1"}"#.into()]).is_empty());
    }

    #[test]
    fn bad_numbers_refuse_the_whole_frame() {
        let mut out = vec![];
        let f = r#"[{"event_type":"tick_size_change","asset_id":"1","new_tick_size":"0.001","timestamp":"1"},{"event_type":"tick_size_change","asset_id":"1","new_tick_size":"abc"}]"#;
        assert!(Market.decode(f, Stamp { mono_ns: 0, wall_ns: 0 }, 0, &mut out).is_err());
        assert!(out.is_empty());
    }

    #[test]
    fn subscription_frames() {
        let t = vec!["11".to_string(), "22".to_string()];
        assert_eq!(Market.subscribe_all(&t), vec![r#"{"assets_ids":["11","22"],"type":"market"}"#]);
        assert_eq!(Market.subscribe_more(&t), vec![r#"{"assets_ids":["11","22"],"operation":"subscribe"}"#]);
        assert!(Market.subscribe_all(&[]).is_empty());
    }

    proptest! {
        #[test]
        fn garbage_never_panics(s in ".{0,200}") { conformance::never_panics(&mut Market, &s); }
    }
}
