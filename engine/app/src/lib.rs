//! Engine application: the engine shared by the live shell (`main.rs`) and the test shell (`backtest/`), its
//! strategies, and the registry that maps configuration names to adapters and strategies.
pub mod arm;
pub mod engine;
pub mod registry;
pub mod strategies;
