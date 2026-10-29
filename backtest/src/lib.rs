//! Backtest: the engine's strategies on recorded data (traces of source-tracing, or the packs of the maker backtest
//! of btc5m-lag-sim), against the simulated exchange. No network, no clock: every instant comes from the data.
pub mod batch;
pub mod compare;
pub mod config;
pub mod labels;
pub mod packs;
pub mod run;
pub mod validate;
pub mod venue;
pub mod window;
