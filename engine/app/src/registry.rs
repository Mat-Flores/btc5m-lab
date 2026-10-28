//! The only place that maps a configuration line to an implementation: adding a source is one adapter crate and one
//! line here.
use engine_core::feed::Adapter;

/// The adapter of a configured `adapter` kind. `true`: the adapter carries books.
pub fn feed_adapter(kind: &str) -> Option<(Box<dyn Adapter>, bool)> {
    match kind {
        "polymarket-market" => Some((Box::new(adapter_polymarket::market::Market), true)),
        "chainlink-rtds" => Some((Box::new(adapter_chainlink::Rtds::default()), false)),
        "okx-bbo-tbt" => Some((Box::new(adapter_okx::BboTbt::default()), false)),
        _ => None,
    }
}
