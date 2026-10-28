//! Strategies: deterministic decisions over the market state, no I/O, no clock. They name their sources by their
//! configuration name; the core never sees them.
pub mod maker;
pub mod model;
pub mod reference;
