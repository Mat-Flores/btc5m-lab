//! BTC 5-minute windows: deterministic slugs, metadata read ahead of time (gamma for the event, clob-markets for
//! the trading parameters, which take precedence), official resolution. Runs on the cold thread only.
use engine_core::http;
use engine_core::money::{parse_micro, parse_micro_rounded, Micro};
use serde_json::Value;
use std::time::Duration;

pub const LEN_S: u64 = 300;
const GAMMA: &str = "https://gamma-api.polymarket.com";
const CLOB: &str = "https://clob.polymarket.com";
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome { Up, Down }

#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub start: u64,
    pub slug: String,
    pub condition_id: String,
    /// Token ids, in this order whatever order the market lists them.
    pub up: String,
    pub down: String,
    pub tick: Micro,
    pub min_size: Micro,
    pub fee_rate: Micro,
    pub fee_exponent: u32,
    /// `itode`: marketable orders are held by Polymarket before matching. Read by the execution adapter only.
    pub taker_delay: bool,
    /// Published only after the window has ended.
    pub price_to_beat: Option<Micro>,
    pub closed: bool,
    pub winner: Option<Outcome>,
    pub twap_seconds: Option<u64>,
}

impl Window { pub fn end(&self) -> u64 { self.start + LEN_S } }

pub fn slug(start: u64) -> String { format!("btc-updown-5m-{start}") }
pub fn window_start(unix_s: u64) -> u64 { unix_s - unix_s % LEN_S }

fn s<'a>(v: &'a Value, k: &str) -> Result<&'a str, String> { v[k].as_str().ok_or_else(|| format!("missing {k}")) }
fn list(v: &Value, k: &str) -> Result<Vec<String>, String> { serde_json::from_str(s(v, k)?).map_err(|e| format!("{k}: {e}")) }
fn dec(v: &Value) -> Option<Micro> { match v { Value::String(x) => parse_micro_rounded(x).ok(), Value::Number(n) => parse_micro_rounded(&n.to_string()).ok(), _ => None } }

/// Builds a window from gamma's event and clob-markets' answer (None if the clob read failed: gamma values then).
pub fn parse(start: u64, event: &Value, clob: Option<&Value>) -> Result<Window, String> {
    let m = &event["markets"][0];
    let tokens = list(m, "clobTokenIds")?;
    let outcomes = list(m, "outcomes")?;
    if tokens.len() != 2 || outcomes.len() != 2 { return Err("expected two outcomes".into()); }
    // a token id is signed as a uint256: only decimal digits that fit are accepted
    const MAX: &str = "115792089237316195423570985008687907853269984665640564039457584007913129639935"; // 2^256 - 1
    let fits = |t: &String| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()) && (t.len() < MAX.len() || (t.len() == MAX.len() && t.as_str() <= MAX));
    if !tokens.iter().all(fits) { return Err("token id not a uint256".into()); }
    let up_i = outcomes.iter().position(|o| o == "Up").ok_or("no Up outcome")?;
    let (mut tick, mut min_size) = (dec(&m["orderPriceMinTickSize"]).ok_or("tick")?, dec(&m["orderMinSize"]).ok_or("min size")?);
    let fs = &m["feeSchedule"];
    let (mut fee_rate, mut fee_exponent) = (dec(&fs["rate"]).unwrap_or(0), fs["exponent"].as_u64().unwrap_or(1) as u32);
    let mut taker_delay = false;
    if let Some(c) = clob {
        if let Some(t) = dec(&c["mts"]) { tick = t; }
        if let Some(x) = dec(&c["mos"]) { min_size = x; }
        if let Some(r) = dec(&c["fd"]["r"]) { fee_rate = r; }
        if let Some(e) = c["fd"]["e"].as_u64() { fee_exponent = e as u32; }
        taker_delay = c["itode"].as_bool().unwrap_or(false);
    }
    // the fee formula is computed exactly up to this exponent (engine_core::value::fee); measured: 1
    if fee_exponent > 2 { return Err(format!("fee exponent {fee_exponent} not supported")); }
    let closed = m["closed"].as_bool().unwrap_or(false);
    let winner = if closed {
        let prices = list(m, "outcomePrices").unwrap_or_default();
        match prices.iter().position(|p| parse_micro(p) == Ok(1_000_000)) { Some(i) if i == up_i => Some(Outcome::Up), Some(_) => Some(Outcome::Down), None => None }
    } else { None };
    Ok(Window {
        start, slug: slug(start), condition_id: s(m, "conditionId")?.to_string(),
        up: tokens[up_i].clone(), down: tokens[1 - up_i].clone(),
        tick, min_size, fee_rate, fee_exponent, taker_delay,
        price_to_beat: dec(&event["eventMetadata"]["priceToBeat"]),
        closed, winner,
        twap_seconds: m["cryptoMarketConfig"]["twapLookbackSeconds"].as_u64(),
    })
}

/// Reads one window. Ok(None): gamma does not list it yet.
pub async fn fetch(start: u64) -> Result<Option<Window>, String> {
    let body = http::get(&format!("{GAMMA}/events?slug={}", slug(start)), TIMEOUT).await.map_err(|e| format!("gamma: {e:?}"))?;
    let events: Value = serde_json::from_slice(&body).map_err(|e| format!("gamma json: {e}"))?;
    let Some(ev) = events.as_array().and_then(|a| a.first()) else { return Ok(None) };
    let cid = s(&ev["markets"][0], "conditionId")?.to_string();
    let clob = match http::get(&format!("{CLOB}/clob-markets/{cid}"), TIMEOUT).await { Ok(b) => serde_json::from_slice::<Value>(&b).ok(), Err(_) => None };
    parse(start, ev, clob.as_ref()).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    // shapes as read live on 2026-10-18 (01-audit.md §4)
    fn event(closed: bool, prices: &str) -> Value {
        serde_json::json!({ "eventMetadata": { "priceToBeat": 81039.441385 }, "markets": [{
            "conditionId": "0xabc", "clobTokenIds": "[\"111\",\"222\"]", "outcomes": "[\"Down\",\"Up\"]",
            "orderPriceMinTickSize": 0.01, "orderMinSize": 5, "closed": closed, "outcomePrices": prices,
            "feeSchedule": { "exponent": 1, "rate": 0.07, "takerOnly": true, "rebateRate": 0.2 },
            "cryptoMarketConfig": { "twapEnabled": true, "twapLookbackSeconds": 60 } }] })
    }

    #[test]
    fn clob_values_take_precedence_and_tokens_follow_outcomes() {
        let clob = serde_json::json!({ "mos": 5, "mts": 0.001, "itode": true, "fd": { "r": 0.07, "e": 1, "to": true } });
        let w = parse(1_792_359_300, &event(false, "[\"0.5\",\"0.5\"]"), Some(&clob)).unwrap();
        assert_eq!((w.up.as_str(), w.down.as_str()), ("222", "111"));
        assert_eq!((w.tick, w.min_size, w.fee_rate, w.fee_exponent, w.taker_delay), (1_000, 5_000_000, 70_000, 1, true));
        assert_eq!(w.slug, "btc-updown-5m-1792359300");
        assert_eq!(w.end(), 1_792_359_600);
        assert_eq!(w.twap_seconds, Some(60));
        assert_eq!(w.price_to_beat, Some(81_039_441_385));
        assert_eq!(w.winner, None);
    }

    #[test]
    fn gamma_values_when_clob_is_missing_and_resolution() {
        let w = parse(0, &event(true, "[\"0\",\"1\"]"), None).unwrap();
        assert_eq!((w.tick, w.taker_delay), (10_000, false));
        assert_eq!(w.winner, Some(Outcome::Up)); // "1" is at the Up index
        assert_eq!(parse(0, &event(true, "[\"1\",\"0\"]"), None).unwrap().winner, Some(Outcome::Down));
        assert_eq!(window_start(1_792_359_399), 1_792_359_300);
    }

    /// Token ids are signed as uint256: any that fits passes (78 digits included), anything else is refused.
    #[test]
    fn token_ids_must_fit_a_uint256() {
        let with = |a: &str| { let mut e = event(false, "[\"0.5\",\"0.5\"]"); e["markets"][0]["clobTokenIds"] = serde_json::json!(format!("[\"{a}\",\"222\"]")); parse(0, &e, None) };
        assert!(with("115792089237316195423570985008687907853269984665640564039457584007913129639935").is_ok());
        assert!(with("115792089237316195423570985008687907853269984665640564039457584007913129639936").is_err());
        assert!(with("1157920892373161954235709850086879078532699846656405640394575840079131296399350").is_err());
        assert!(with("12a").is_err());
        assert!(with("").is_err());
    }
}
