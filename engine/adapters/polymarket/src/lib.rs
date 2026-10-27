//! Polymarket: market data (CLOB `market` WebSocket), BTC 5-minute windows (gamma + clob-markets over HTTP), and
//! order signing (`sign`).
//! Formats checked against real messages (engine/fixtures/raw) and live reads (01-audit.md §4).
pub mod account;
pub mod exec;
pub mod market;
pub mod paper;
pub mod sign;
pub mod user;
pub mod windows;
