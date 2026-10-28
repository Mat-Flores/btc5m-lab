//! Binance public streams (combined `/stream` endpoint), parsing of the old application (`src/feeds/live.js`):
//! - `binance-spot` (`Spot`): `<symbol>@bookTicker` (top of book, no event time) and `<symbol>@aggTrade` (trades, with
//!   their event time: the age of the feed);
//! - `binance-perp` (`Perp`, USDⓈ-M futures): `<symbol>@bookTicker`, stamped (`E`).
//! Topics are lowercase symbols ("btcusdt"), one per source line: a quote carries no instrument. `u` is the book's
//! update id, not a sequence of the stream (it jumps): no sequence is declared.
use engine_core::clock::Stamp;
use engine_core::event::{BookKind, Caps, Event, Payload, Side, SourceId};
use engine_core::feed::{Adapter, DecodeError};
use engine_core::money::{parse_micro, parse_micro_rounded};
use serde_json::Value;
use std::time::Duration;

pub const SPOT_URL: &str = "wss://stream.binance.com:9443/stream";
pub const PERP_URL: &str = "wss://fstream.binance.com/stream";
/// [à vérifier] not calibrated on recorded data yet: BTCUSDT's book ticker changes many times a second.
pub const SILENCE: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct Spot { inst: Option<String> }
#[derive(Default)]
pub struct Perp { inst: Option<String> }

fn frame(method: &str, streams: Vec<String>, id: u32) -> Vec<String> {
    if streams.is_empty() { return vec![]; }
    vec![serde_json::json!({ "method": method, "params": streams, "id": id }).to_string()]
}

fn err(what: &str) -> impl FnOnce(engine_core::money::ParseError) -> DecodeError + '_ { move |e| DecodeError(format!("{what}: {e:?}")) }
fn s<'a>(d: &'a Value, k: &str) -> Result<&'a str, DecodeError> { d[k].as_str().ok_or_else(|| DecodeError(format!("missing {k}"))) }

/// One frame of either market. `inst`: the instrument this source carries, learnt from its first quote.
fn decode(inst: &mut Option<String>, text: &str, recv: Stamp, source: SourceId, out: &mut Vec<Event>) -> Result<Option<u64>, DecodeError> {
    let m: Value = serde_json::from_str(text).map_err(|e| DecodeError(format!("json: {e}")))?;
    // a refused subscription would leave the connection silent: reported as unreadable instead
    if !m["error"].is_null() { return Err(DecodeError(format!("error {}", m["error"]))); }
    if m.get("result").is_some() && m.get("id").is_some() { return Ok(None); } // subscription answer
    let (Some(stream), d) = (m["stream"].as_str(), &m["data"]) else { return Err(DecodeError("no stream".into())) };
    let sym = s(d, "s")?;
    if inst.get_or_insert_with(|| sym.to_string()) != sym { return Err(DecodeError(format!("second instrument {sym}"))); }
    let event_ms = d["E"].as_u64();
    let payload = if stream.ends_with("@bookTicker") {
        Payload::Quote { bid: parse_micro(s(d, "b")?).map_err(err("bid"))?, ask: parse_micro(s(d, "a")?).map_err(err("ask"))? }
    } else if stream.ends_with("@aggTrade") {
        // m: the buyer was the maker, so the taker sold; sizes carry 8 decimals (a reference market's size, rounded)
        let side = match d["m"].as_bool() { Some(true) => Side::Ask, Some(false) => Side::Bid, None => return Err(DecodeError("missing m".into())) };
        let id = d["a"].as_u64().ok_or_else(|| DecodeError("missing a".into()))?.to_string();
        Payload::Trade { asset: sym.to_string(), price: parse_micro(s(d, "p")?).map_err(err("price"))?, size: parse_micro_rounded(s(d, "q")?).map_err(err("size"))?, side, id }
    } else {
        return Err(DecodeError(format!("stream {stream}")));
    };
    out.push(Event { source, recv, event_ms, seq: None, payload });
    Ok(None)
}

impl Adapter for Spot {
    fn url(&self) -> String { SPOT_URL.into() }
    fn caps(&self) -> Caps { Caps { sequence: false, event_time: true, book: BookKind::TopOfBook, silence: SILENCE } }
    fn subscribe_all(&self, topics: &[String]) -> Vec<String> { frame("SUBSCRIBE", topics.iter().flat_map(|t| [format!("{t}@bookTicker"), format!("{t}@aggTrade")]).collect(), 1) }
    fn unsubscribe(&self, topics: &[String]) -> Vec<String> { frame("UNSUBSCRIBE", topics.iter().flat_map(|t| [format!("{t}@bookTicker"), format!("{t}@aggTrade")]).collect(), 2) }
    // Binance pings at the WebSocket level; the pongs are answered by the WebSocket layer
    fn heartbeat(&self) -> Option<(Duration, String)> { None }
    fn decode(&mut self, text: &str, recv: Stamp, source: SourceId, out: &mut Vec<Event>) -> Result<Option<u64>, DecodeError> { decode(&mut self.inst, text, recv, source, out) }
}

impl Adapter for Perp {
    fn url(&self) -> String { PERP_URL.into() }
    fn caps(&self) -> Caps { Caps { sequence: false, event_time: true, book: BookKind::TopOfBook, silence: SILENCE } }
    fn subscribe_all(&self, topics: &[String]) -> Vec<String> { frame("SUBSCRIBE", topics.iter().map(|t| format!("{t}@bookTicker")).collect(), 1) }
    fn unsubscribe(&self, topics: &[String]) -> Vec<String> { frame("UNSUBSCRIBE", topics.iter().map(|t| format!("{t}@bookTicker")).collect(), 2) }
    fn heartbeat(&self) -> Option<(Duration, String)> { None }
    fn decode(&mut self, text: &str, recv: Stamp, source: SourceId, out: &mut Vec<Event>) -> Result<Option<u64>, DecodeError> { decode(&mut self.inst, text, recv, source, out) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::conformance;
    use proptest::prelude::*;

    fn fixture(name: &str) -> Vec<String> {
        let path = format!("{}/../../fixtures/raw/{name}", env!("CARGO_MANIFEST_DIR"));
        let v: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        v.into_iter().map(|m| m.to_string()).collect()
    }
    const R: Stamp = Stamp { mono_ns: 0, wall_ns: 0 };

    #[test]
    fn real_spot_frames_decode() {
        let f = fixture("binance-spot-btcusdt.json");
        let ev = conformance::decodes(&mut Spot::default(), &f);
        assert_eq!(ev.len(), f.len() - 1); // the subscription answer carries nothing
        assert_eq!((&ev[0].payload, ev[0].event_ms), (&Payload::Quote { bid: 80_584_080_000, ask: 80_584_090_000 }, None));
        let t = ev.iter().find(|e| matches!(e.payload, Payload::Trade { .. })).unwrap();
        assert_eq!(t.payload, Payload::Trade { asset: "BTCUSDT".into(), price: 80_584_090_000, size: 340, side: Side::Bid, id: "4100268169".into() });
        assert_eq!(t.event_ms, Some(1_793_201_712_930));
    }

    #[test]
    fn real_perp_frames_decode() {
        let f = fixture("binance-perp-btcusdt.json");
        let ev = conformance::decodes(&mut Perp::default(), &f);
        assert_eq!(ev.len(), f.len() - 1);
        assert_eq!((&ev[0].payload, ev[0].event_ms), (&Payload::Quote { bid: 80_548_300_000, ask: 80_548_400_000 }, Some(1_793_201_714_337)));
    }

    #[test]
    fn truncated_frames_are_refused() {
        for f in fixture("binance-spot-btcusdt.json") { conformance::truncations_refused(&mut Spot::default(), &f); }
        for f in fixture("binance-perp-btcusdt.json") { conformance::truncations_refused(&mut Perp::default(), &f); }
    }

    #[test]
    fn errors_other_streams_and_a_second_instrument_are_refused() {
        assert!(Spot::default().decode(r#"{"error":{"code":2,"msg":"Invalid request"},"id":1}"#, R, 0, &mut vec![]).is_err());
        assert!(Spot::default().decode(r#"{"stream":"btcusdt@depth","data":{"s":"BTCUSDT"}}"#, R, 0, &mut vec![]).is_err());
        let mut a = Perp::default();
        conformance::decodes(&mut a, &fixture("binance-perp-btcusdt.json"));
        assert!(a.decode(&fixture("binance-perp-btcusdt.json")[1].replace("BTCUSDT", "ETHUSDT"), R, 0, &mut vec![]).is_err());
    }

    #[test]
    fn subscription_frames() {
        assert_eq!(Spot::default().subscribe_all(&["btcusdt".into()]), vec![r#"{"id":1,"method":"SUBSCRIBE","params":["btcusdt@bookTicker","btcusdt@aggTrade"]}"#]);
        assert_eq!(Perp::default().unsubscribe(&["btcusdt".into()]), vec![r#"{"id":2,"method":"UNSUBSCRIBE","params":["btcusdt@bookTicker"]}"#]);
        assert!(Perp::default().subscribe_all(&[]).is_empty());
    }

    proptest! {
        #[test]
        fn garbage_never_panics(s in ".{0,200}") { conformance::never_panics(&mut Spot::default(), &s); conformance::never_panics(&mut Perp::default(), &s); }
    }
}
