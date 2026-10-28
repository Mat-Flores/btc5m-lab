//! Polymarket: market data (CLOB `market` WebSocket), windows of the configured market (gamma + clob-markets over
//! HTTP), and order signing (`sign`).
//! Formats checked against real messages (engine/fixtures/raw) and live reads (01-audit.md §4).
pub mod account;
pub mod exec;
pub mod market;
pub mod paper;
pub mod sign;
pub mod user;
pub mod windows;

/// One HTTPS GET (the engine's client, `engine_core::http`): the status and the raw body, whatever the status.
/// Used by the window reads, and by source-tracing to record them.
pub async fn http_get(url: &str, timeout: std::time::Duration) -> Result<(u16, String), String> {
    let (status, body) = engine_core::http::get_any(url, timeout).await.map_err(|e| format!("{e:?}"))?;
    Ok((status, String::from_utf8_lossy(&body).into_owned()))
}
