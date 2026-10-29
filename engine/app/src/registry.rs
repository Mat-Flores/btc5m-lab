//! The only place that maps a configuration name to an implementation: adding a source is one adapter crate and one
//! line in `feed_adapter`; adding a strategy, one line in `strategy`.
use crate::engine::Strategy;
use crate::strategies::{maker::{MakerStrategy, Roles}, reference::RefStrategy};
use engine_core::feed::Adapter;
use engine_core::market::MarketSpec;
use serde_json::Value;

/// Every kind `feed_adapter` knows.
pub const FEED_KINDS: [&str; 6] = ["polymarket-market", "chainlink-rtds", "okx-bbo-tbt", "binance-spot", "binance-perp", "coinbase"];

/// The adapter of a configured `adapter` kind. `true`: the adapter carries books.
pub fn feed_adapter(kind: &str) -> Option<(Box<dyn Adapter>, bool)> {
    match kind {
        "polymarket-market" => Some((Box::new(adapter_polymarket::market::Market), true)),
        "chainlink-rtds" => Some((Box::new(adapter_chainlink::Rtds::default()), false)),
        "okx-bbo-tbt" => Some((Box::new(adapter_okx::BboTbt::default()), false)),
        "binance-spot" => Some((Box::new(adapter_binance::Spot::default()), false)),
        "binance-perp" => Some((Box::new(adapter_binance::Perp::default()), false)),
        "coinbase" => Some((Box::new(adapter_coinbase::Ticker::default()), false)),
        _ => None,
    }
}

/// Both strategies keep the 5-minute window's constants of their origin (reference.rs: windows of 300 s; maker:
/// END = 300 s, TWAP 60 s): another horizon than the one they are built for (`horizonS`, default 300 s) is refused
/// rather than traded wrong.
fn built_for(kind: &str, p: &mut serde_json::Map<String, Value>, m: &MarketSpec) -> Result<(), String> {
    let h = match p.remove("horizonS") { None => 300, Some(v) => v.as_u64().ok_or(format!("{kind}: horizonS {v}"))? };
    if m.horizon_s != h { return Err(format!("{kind}: horizon {} s not supported (built for {h} s)", m.horizon_s)); }
    Ok(())
}

/// A strategy of a configured `kind`, from its parameters (JSON object).
/// - "reference": the old engine's configuration keys (`engine_strategy::reference::Config`, camelCase, `source` =
///   the price source), plus `oracle` (Chainlink source name, default "chainlink"); `maxTau` defaults to the horizon.
/// - "maker": `sim.mjs` DEFAULTS names (`engine_strategy::maker::Params`), plus `sources` {price, oracle, book},
///   `capital0` ($, default 1000) and `claim` (s, default 0) of the capital rule (maker-bt/run.mjs), `keepMs` (ms,
///   default 360 000: prices and prints kept for a window learnt after they came).
/// Both: `horizonS`, the horizon they are built for (s, default 300).
pub fn strategy(kind: &str, params: Value, m: &MarketSpec) -> Result<Box<dyn Strategy>, String> {
    let Value::Object(mut p) = params else { return Err(format!("{kind}: parameters are not an object")) };
    if matches!(kind, "reference" | "maker") { built_for(kind, &mut p, m)?; }
    match kind {
        "reference" => {
            let oracle = match p.remove("oracle") { Some(Value::String(s)) => s, None => "chainlink".into(), Some(v) => return Err(format!("reference: oracle {v}")) };
            p.entry("maxTau").or_insert(Value::from(m.horizon_s));
            let cfg: engine_strategy::reference::Config = serde_json::from_value(Value::Object(p)).map_err(|e| format!("reference: {e}"))?;
            if cfg.source.is_empty() { return Err("reference: no price source (\"source\")".into()); }
            Ok(Box::new(RefStrategy::new(cfg, oracle)))
        }
        "maker" => {
            let roles: Roles = p.remove("sources").map_or(Ok(Roles::default()), serde_json::from_value).map_err(|e| format!("maker: sources: {e}"))?;
            let num = |v: Option<Value>, d: f64, k: &str| v.map_or(Ok(d), |v| v.as_f64().ok_or(format!("maker: {k} not a number")));
            let capital0 = num(p.remove("capital0"), 1000.0, "capital0")?;
            let claim = num(p.remove("claim"), 0.0, "claim")?;
            let keep_ms = num(p.remove("keepMs"), 360_000.0, "keepMs")?;
            let params: engine_strategy::maker::Params = serde_json::from_value(Value::Object(p)).map_err(|e| format!("maker: {e}"))?;
            Ok(Box::new(MakerStrategy::new(params, roles, capital0, claim, (keep_ms * 1e6) as u64)))
        }
        _ => Err(format!("unknown strategy {kind}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategies_are_built_from_configuration_and_other_horizons_refused() {
        let btc = MarketSpec { venue: "polymarket".into(), asset: "btc".into(), horizon_s: 300 };
        let eth15 = MarketSpec { asset: "eth".into(), horizon_s: 900, ..btc.clone() };
        let r = strategy("reference", serde_json::json!({ "source": "okx", "minEdge": 0.12 }), &btc).unwrap();
        assert_eq!(r.sources(), vec!["okx", "chainlink"]);
        assert!(strategy("reference", serde_json::json!({ "source": "okx" }), &eth15).is_err());
        // the horizon a strategy is built for is its configuration (`horizonS`), 300 s unless said
        assert!(strategy("reference", serde_json::json!({ "source": "okx", "horizonS": 900 }), &eth15).is_ok());
        assert!(strategy("reference", serde_json::json!({ "source": "okx", "horizonS": 900 }), &btc).is_err());
        assert!(strategy("maker", serde_json::json!({ "keepMs": 360000, "horizonS": 300, "volWindow": 300, "endMs": 300000 }), &btc).is_ok());
        assert!(strategy("reference", serde_json::json!({ "source": "okx", "typo": 1 }), &btc).is_err());
        assert!(strategy("reference", serde_json::json!({}), &btc).is_err());
        // a candidate of a maker batch file (maker-bt/batches/x01-retest-exact.json), as is
        let x01 = serde_json::json!({ "id": "X-P88-Dno", "desc": "…", "f": 0.05, "prob": "fair", "price": { "mode": "offset", "offset": 0.01 }, "requote": "follow",
            "reserveInFlight": true, "liqCap": false, "entryFrom": 270, "quoteEnd": 0, "sideProb": [0.88, 0.999], "maxDiverge": null,
            "sources": { "price": "okx-btc-usdt-swap", "oracle": "chainlink-rtds", "book": "polymarket-clob" } });
        assert_eq!(strategy("maker", x01.clone(), &btc).unwrap().sources(), vec!["okx-btc-usdt-swap", "chainlink-rtds", "polymarket-clob"]);
        assert!(strategy("maker", x01, &eth15).is_err());
        assert!(strategy("maker", serde_json::json!({ "offset": 0.01 }), &btc).is_err());
        assert!(strategy("other", serde_json::json!({}), &btc).is_err());
    }

    /// The price sources added in the migration resolve by their configured name and decode their real frames.
    #[test]
    fn new_price_sources_are_registered_and_decode_their_fixtures() {
        for (kind, file) in [("binance-spot", "binance-spot-btcusdt.json"), ("binance-perp", "binance-perp-btcusdt.json"), ("coinbase", "coinbase-ticker-BTC-USD.json")] {
            let (mut a, books) = feed_adapter(kind).unwrap();
            assert!(!books, "{kind}");
            let path = format!("{}/../fixtures/raw/{file}", env!("CARGO_MANIFEST_DIR"));
            let frames: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            let frames: Vec<String> = frames.iter().map(Value::to_string).collect();
            // the subscription answer carries nothing; every other frame is one event at least
            assert!(engine_core::conformance::decodes(a.as_mut(), &frames).len() >= frames.len() - 1, "{kind}");
        }
        assert!(feed_adapter("binance").is_none());
        assert!(FEED_KINDS.iter().all(|k| feed_adapter(k).is_some()));
    }
}
