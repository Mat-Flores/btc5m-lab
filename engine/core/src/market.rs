//! The traded market as configuration (venue, asset, horizon), and one window of it as the venue describes it.
//! BTC 5 minutes is the only market configured and tested; another asset or horizon is a configuration line. The
//! default market is the application's (engine-app), not the core's.
use crate::money::Micro;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MarketSpec { pub venue: String, pub asset: String, pub horizon_s: u64 }

impl MarketSpec {
    /// "{asset}-updown-{horizon in minutes}m-{start}": "btc-updown-5m-1792359300" for BTC 5 minutes.
    pub fn slug(&self, start: u64) -> String { format!("{}-updown-{}m-{start}", self.asset, self.horizon_s / 60) }
    /// Start of the window holding `unix_s` (windows are aligned on multiples of the horizon).
    pub fn window_start(&self, unix_s: u64) -> u64 { unix_s - unix_s % self.horizon_s }
    pub fn end(&self, start: u64) -> u64 { start + self.horizon_s }
}

/// Fee schedule of a market: `rate × size × (p(1−p))^exponent` for a taker; `rebate_rate` of it back to the maker.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FeeSchedule { pub rate: Micro, pub exponent: u32, pub rebate_rate: Micro }

/// One window of the market, as its venue adapter read it ahead of time.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub start: u64,
    pub end: u64,
    pub slug: String,
    /// The venue's market id (condition id).
    pub market: String,
    /// Token ids, [up, down], whatever order the market lists them.
    pub tokens: [String; 2],
    pub tick: Micro,
    pub min_size: Micro,
    pub fee: FeeSchedule,
    /// Marketable orders are held by the venue before matching.
    pub taker_delay: bool,
    pub twap_s: Option<u64>,
    /// Published only after the window has ended.
    pub price_to_beat: Option<Micro>,
    /// Index of the winning token (0 up, 1 down), once the market is resolved.
    pub winner: Option<usize>,
}

impl Window {
    pub fn up(&self) -> &String { &self.tokens[0] }
    pub fn down(&self) -> &String { &self.tokens[1] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn btc_5m_slugs_and_bounds_are_unchanged() {
        let m = MarketSpec { venue: "v".into(), asset: "btc".into(), horizon_s: 300 };
        assert_eq!(m.slug(1_792_359_300), "btc-updown-5m-1792359300");
        assert_eq!(m.window_start(1_792_359_399), 1_792_359_300);
        assert_eq!(m.end(1_792_359_300), 1_792_359_600);
        // another market is configuration only (not traded nor tested beyond its names)
        let eth15 = MarketSpec { venue: "v".into(), asset: "eth".into(), horizon_s: 900 };
        assert_eq!((eth15.slug(1_792_359_000), eth15.window_start(1_792_359_899)), ("eth-updown-15m-1792359000".into(), 1_792_359_000));
        assert!(serde_json::from_str::<MarketSpec>(r#"{"venue":"v","asset":"btc","horizon_s":300,"x":1}"#).is_err());
    }
}
