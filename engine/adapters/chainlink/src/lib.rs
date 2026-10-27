//! Chainlink reference price through Polymarket's RTDS (`crypto_prices_chainlink`). RTDS is legacy for prices since
//! 2026-09-15; PolyBolt (`price.crypto`, `price.crypto.twap`) replaces it once API credentials exist, as another
//! adapter behind the same configuration name. Topics are symbols ("btc/usd").
use engine_core::clock::Stamp;
use engine_core::event::{BookKind, Caps, Event, Payload, SourceId};
use engine_core::feed::{Adapter, DecodeError};
use engine_core::money::scaled_int_to_micro;
use serde::Deserialize;
use std::time::Duration;

pub const URL: &str = "wss://ws-live-data.polymarket.com";
/// Calibrated on 4 recorded sessions: ~1 message/s, p99.99 ~8.5 s, max outside outages 8.9 s.
pub const SILENCE: Duration = Duration::from_secs(10);

pub struct Rtds;

#[derive(Deserialize)]
struct P { full_accuracy_value: String, symbol: String, timestamp: u64 }
// the payload is read only once the topic is known: other topics (e.g. the history sent at subscription) differ
#[derive(Deserialize)]
struct Msg { topic: Option<String>, payload: Option<serde_json::Value> }

impl Adapter for Rtds {
    fn url(&self) -> String { URL.into() }
    fn caps(&self) -> Caps { Caps { sequence: false, event_time: true, book: BookKind::None, silence: SILENCE } }
    fn subscribe_all(&self, topics: &[String]) -> Vec<String> {
        let subs: Vec<_> = topics.iter().map(|t| serde_json::json!({ "topic": "crypto_prices_chainlink", "type": "*", "filters": serde_json::json!({ "symbol": t }).to_string() })).collect();
        vec![serde_json::json!({ "action": "subscribe", "subscriptions": subs }).to_string()]
    }
    fn unsubscribe(&self, topics: &[String]) -> Vec<String> {
        let subs: Vec<_> = topics.iter().map(|t| serde_json::json!({ "topic": "crypto_prices_chainlink", "type": "*", "filters": serde_json::json!({ "symbol": t }).to_string() })).collect();
        vec![serde_json::json!({ "action": "unsubscribe", "subscriptions": subs }).to_string()]
    }
    fn heartbeat(&self) -> Option<(Duration, String)> { Some((Duration::from_secs(5), "PING".into())) }

    fn decode(&mut self, text: &str, recv: Stamp, source: SourceId, out: &mut Vec<Event>) -> Result<Option<u64>, DecodeError> {
        if text.eq_ignore_ascii_case("pong") || text.is_empty() { return Ok(None); }
        let m: Msg = serde_json::from_str(text).map_err(|e| DecodeError(format!("json: {e}")))?;
        if m.topic.as_deref() != Some("crypto_prices_chainlink") { return Ok(None); }
        let Some(p) = m.payload else { return Ok(None) };
        let p: P = serde_json::from_value(p).map_err(|e| DecodeError(format!("payload: {e}")))?;
        if !p.symbol.eq_ignore_ascii_case("btc/usd") { return Ok(None); }
        // full_accuracy_value carries 18 implied decimals; rounded to the micro-dollar (reference price, not money)
        let value = scaled_int_to_micro(&p.full_accuracy_value, 18).map_err(|e| DecodeError(format!("value: {e:?}")))?;
        out.push(Event { source, recv, event_ms: Some(p.timestamp), seq: None, payload: Payload::Oracle { value } });
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::conformance;
    use proptest::prelude::*;

    fn frames() -> Vec<String> {
        let path = format!("{}/../../fixtures/raw/rtds-chainlink.json", env!("CARGO_MANIFEST_DIR"));
        let v: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        v.into_iter().map(|m| m.to_string()).collect()
    }

    #[test]
    fn real_frames_decode() {
        let ev = conformance::decodes(&mut Rtds, &frames());
        assert_eq!(ev.len(), frames().len());
        assert_eq!(ev[0].payload, Payload::Oracle { value: 80_985_877_816 });
        assert_eq!(ev[0].event_ms, Some(1_792_359_572_000));
    }

    #[test]
    fn truncated_frames_are_refused() { for f in frames() { conformance::truncations_refused(&mut Rtds, &f); } }

    #[test]
    fn history_sent_at_subscription_carries_nothing() {
        let path = format!("{}/../../fixtures/raw/rtds-subscribe-history.json", env!("CARGO_MANIFEST_DIR"));
        let v: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(conformance::decodes(&mut Rtds, &[v[0].to_string(), String::new()]).is_empty());
    }

    #[test]
    fn subscription_frame_matches_the_one_in_use() {
        assert_eq!(Rtds.subscribe_all(&["btc/usd".into()]), vec![r#"{"action":"subscribe","subscriptions":[{"filters":"{\"symbol\":\"btc/usd\"}","topic":"crypto_prices_chainlink","type":"*"}]}"#]);
    }

    proptest! {
        #[test]
        fn garbage_never_panics(s in ".{0,200}") { conformance::never_panics(&mut Rtds, &s); }
    }
}
