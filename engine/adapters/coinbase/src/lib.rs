//! Coinbase Exchange public `ticker` channel, parsing of the old application (`src/feeds/live.js`): best bid and ask
//! after each trade, stamped by the exchange (`time`, RFC 3339). Topics are product ids ("BTC-USD"), one per source
//! line: a quote carries no instrument. `sequence` numbers the product's full feed, which the ticker samples: it jumps,
//! so no sequence is declared.
use engine_core::clock::Stamp;
use engine_core::event::{BookKind, Caps, Event, Payload, SourceId};
use engine_core::feed::{Adapter, DecodeError};
use engine_core::money::parse_micro;
use serde_json::Value;
use std::time::Duration;

pub const URL: &str = "wss://ws-feed.exchange.coinbase.com";
/// [à vérifier] not calibrated on recorded data yet; the ticker only speaks after a trade.
pub const SILENCE: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Ticker { inst: Option<String> }

fn frame(kind: &str, topics: &[String]) -> Vec<String> {
    if topics.is_empty() { return vec![]; }
    vec![serde_json::json!({ "type": kind, "product_ids": topics, "channels": ["ticker"] }).to_string()]
}

/// "2026-10-28T15:35:14.846847Z" → Unix ms (UTC only, fraction truncated to the ms).
pub fn rfc3339_ms(t: &str) -> Option<u64> {
    let b = t.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || *b.last()? != b'Z' { return None; }
    let n = |r: std::ops::Range<usize>| -> Option<i64> { let s = t.get(r)?; s.bytes().all(|c| c.is_ascii_digit()).then(|| s.parse().ok())? };
    let (y, mo, d, h, mi, s) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 { return None; }
    let frac = &t[19..t.len() - 1];
    let ms = match frac.strip_prefix('.') {
        None if frac.is_empty() => 0,
        Some(f) if !f.is_empty() && f.bytes().all(|c| c.is_ascii_digit()) => format!("{f:0<3}")[..3].parse::<i64>().ok()?,
        _ => return None,
    };
    // days from the civil date (Howard Hinnant's algorithm)
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(((days * 24 + h) * 60 + mi) * 60 + s).ok().map(|x| x * 1000 + ms as u64)
}

impl Adapter for Ticker {
    fn url(&self) -> String { URL.into() }
    fn caps(&self) -> Caps { Caps { sequence: false, event_time: true, book: BookKind::TopOfBook, silence: SILENCE } }
    fn subscribe_all(&self, topics: &[String]) -> Vec<String> { frame("subscribe", topics) }
    fn unsubscribe(&self, topics: &[String]) -> Vec<String> { frame("unsubscribe", topics) }
    fn heartbeat(&self) -> Option<(Duration, String)> { None }

    fn decode(&mut self, text: &str, recv: Stamp, source: SourceId, out: &mut Vec<Event>) -> Result<Option<u64>, DecodeError> {
        let m: Value = serde_json::from_str(text).map_err(|e| DecodeError(format!("json: {e}")))?;
        match m["type"].as_str() {
            // a refused subscription would leave the connection silent: reported as unreadable instead
            Some("error") => return Err(DecodeError(format!("error {}", m["message"]))),
            Some("ticker") => {}
            Some(_) => return Ok(None), // subscriptions, heartbeats
            None => return Err(DecodeError("no type".into())),
        }
        let s = |k: &str| m[k].as_str().ok_or_else(|| DecodeError(format!("missing {k}")));
        let product = s("product_id")?;
        if self.inst.get_or_insert_with(|| product.to_string()) != product { return Err(DecodeError(format!("second instrument {product}"))); }
        // as the old application: a ticker without a book side carries no quote
        let (Some(b), Some(a)) = (m["best_bid"].as_str(), m["best_ask"].as_str()) else { return Ok(None) };
        let bid = parse_micro(b).map_err(|e| DecodeError(format!("bid: {e:?}")))?;
        let ask = parse_micro(a).map_err(|e| DecodeError(format!("ask: {e:?}")))?;
        let event_ms = Some(rfc3339_ms(s("time")?).ok_or_else(|| DecodeError("time".into()))?);
        out.push(Event { source, recv, event_ms, seq: None, payload: Payload::Quote { bid, ask } });
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::conformance;
    use proptest::prelude::*;

    fn frames() -> Vec<String> {
        let path = format!("{}/../../fixtures/raw/coinbase-ticker-BTC-USD.json", env!("CARGO_MANIFEST_DIR"));
        let v: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        v.into_iter().map(|m| m.to_string()).collect()
    }
    const R: Stamp = Stamp { mono_ns: 0, wall_ns: 0 };

    #[test]
    fn real_frames_decode() {
        let ev = conformance::decodes(&mut Ticker::default(), &frames());
        assert_eq!(ev.len(), frames().len() - 1); // the subscription answer carries nothing
        assert_eq!(ev[0].payload, Payload::Quote { bid: 80_451_660_000, ask: 80_451_670_000 });
        // Date.parse("2026-10-28T15:35:14.846847Z") of the old application
        assert_eq!(ev[0].event_ms, Some(1_793_201_714_846));
    }

    #[test]
    fn times_are_read_as_utc() {
        assert_eq!(rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(rfc3339_ms("2000-03-01T00:00:00.5Z"), Some(951_868_800_500));
        assert_eq!(rfc3339_ms("2026-10-28T15:35:14.846847"), None);
        assert_eq!(rfc3339_ms("2026-13-28T15:35:14Z"), None);
    }

    #[test]
    fn truncated_frames_are_refused() { for f in frames() { conformance::truncations_refused(&mut Ticker::default(), &f); } }

    #[test]
    fn errors_and_a_second_instrument_are_refused() {
        assert!(Ticker::default().decode(r#"{"type":"error","message":"Failed to subscribe"}"#, R, 0, &mut vec![]).is_err());
        let mut a = Ticker::default();
        conformance::decodes(&mut a, &frames());
        assert!(a.decode(&frames()[1].replace("BTC-USD", "ETH-USD"), R, 0, &mut vec![]).is_err());
    }

    #[test]
    fn subscription_frame_matches_the_old_application() {
        assert_eq!(Ticker::default().subscribe_all(&["BTC-USD".into()]), vec![r#"{"channels":["ticker"],"product_ids":["BTC-USD"],"type":"subscribe"}"#]);
    }

    proptest! {
        #[test]
        fn garbage_never_panics(s in ".{0,200}") { conformance::never_panics(&mut Ticker::default(), &s); }
    }
}
