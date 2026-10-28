//! Sources HTTP : fenêtres gamma du marché configuré, `GET /time` du CLOB, et découverte des tokens à abonner.
use crate::source::HttpRes;
use engine_core::clock::Now;
use engine_core::market::MarketSpec;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

const GAMMA: &str = "https://gamma-api.polymarket.com/events?slug=";
pub const CLOB_TIME: &str = "https://clob.polymarket.com/time";
const TIMEOUT: Duration = Duration::from_secs(5);
/// Les tokens d'une fenêtre terminée restent abonnés ce temps après sa fin (comme engine/app : AFTER_END_S).
const TOKENS_AFTER_END_S: u64 = 15;

/// Débuts des fenêtres à enregistrer : la précédente (tant qu'elle est dans `prev_grace_s` après sa fin), la courante,
/// puis `ahead` suivantes.
pub fn gamma_starts(spec: &MarketSpec, now_s: u64, ahead: u64, prev_grace_s: u64) -> Vec<u64> {
    window_list(spec, now_s, ahead, prev_grace_s)
}

/// Fenêtres dont les tokens doivent rester abonnés.
pub fn token_starts(spec: &MarketSpec, now_s: u64, ahead: u64) -> Vec<u64> {
    window_list(spec, now_s, ahead, TOKENS_AFTER_END_S)
}

fn window_list(spec: &MarketSpec, now_s: u64, ahead: u64, prev_grace_s: u64) -> Vec<u64> {
    let cur = spec.window_start(now_s);
    let mut v = vec![];
    if cur >= spec.horizon_s && now_s - cur < prev_grace_s { v.push(cur - spec.horizon_s); }
    v.extend((0..=ahead).map(|i| cur + i * spec.horizon_s));
    v
}

pub fn gamma_url(spec: &MarketSpec, start: u64) -> String { format!("{GAMMA}{}", spec.slug(start)) }

/// Les deux tokens (up, down dans l'ordre de gamma) de la réponse `events?slug=…`. None : fenêtre pas encore listée.
pub fn tokens(body: &str) -> Option<Vec<String>> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let ids = v.as_array()?.first()?["markets"][0]["clobTokenIds"].as_str()?;
    serde_json::from_str(ids).ok()
}

pub async fn get(url: &str, now: &Arc<dyn Now + Send + Sync>) -> HttpRes {
    let t_sent = (now.now().wall_ns / 1000) as i64;
    let r = adapter_polymarket::http_get(url, TIMEOUT).await;
    let at = (now.now().wall_ns / 1000) as i64;
    match r {
        Ok((status, raw)) => HttpRes { at, t_sent, url: url.into(), status: Some(status), raw: Some(raw), error: None },
        Err(e) => HttpRes { at, t_sent, url: url.into(), status: None, raw: None, error: Some(e) },
    }
}

fn now_s(now: &Arc<dyn Now + Send + Sync>) -> u64 { now.now().wall_ns / 1_000_000_000 }

/// Enregistre gamma : une requête par fenêtre suivie, toutes les `every`.
pub async fn poll_gamma(spec: MarketSpec, every: Duration, ahead: u64, grace_s: u64, now: Arc<dyn Now + Send + Sync>, tx: UnboundedSender<HttpRes>) {
    loop {
        let started = tokio::time::Instant::now();
        for s in gamma_starts(&spec, now_s(&now), ahead, grace_s) {
            if tx.send(get(&gamma_url(&spec, s), &now).await).is_err() { return; }
        }
        tokio::time::sleep(every.saturating_sub(started.elapsed())).await;
    }
}

pub async fn poll_clob_time(every: Duration, now: Arc<dyn Now + Send + Sync>, tx: UnboundedSender<HttpRes>) {
    loop {
        let started = tokio::time::Instant::now();
        if tx.send(get(CLOB_TIME, &now).await).is_err() { return; }
        tokio::time::sleep(every.saturating_sub(started.elapsed())).await;
    }
}

/// Tokens des fenêtres suivies (sans rien enregistrer). Une fenêtre non listée ou en erreur est ignorée.
pub async fn discover(spec: &MarketSpec, ahead: u64, now: &Arc<dyn Now + Send + Sync>) -> Vec<String> {
    let mut out = vec![];
    for s in token_starts(spec, now_s(now), ahead) {
        if let Some(t) = get(&gamma_url(spec, s), now).await.raw.as_deref().and_then(tokens) { out.extend(t); }
    }
    out
}

/// Redécouvre les tokens toutes les `every` ; n'envoie que les changements.
pub async fn discover_loop(spec: MarketSpec, every: Duration, ahead: u64, now: Arc<dyn Now + Send + Sync>, tx: UnboundedSender<Vec<String>>) {
    let mut last: Vec<String> = vec![];
    loop {
        tokio::time::sleep(every).await;
        let t = discover(&spec, ahead, &now).await;
        // une découverte vide (réseau, fenêtre non listée) ne désabonne rien
        if !t.is_empty() && t != last { last = t.clone(); if tx.send(t).is_err() { return; } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn btc() -> MarketSpec { MarketSpec { venue: "polymarket".into(), asset: "btc".into(), horizon_s: 300 } }

    #[test]
    fn windows_followed() {
        let s = btc();
        assert_eq!(gamma_starts(&s, 1_792_359_450, 2, 120), vec![1_792_359_300, 1_792_359_600, 1_792_359_900]);
        // 30 s après le début de la fenêtre : la précédente est encore suivie (résolution)
        assert_eq!(gamma_starts(&s, 1_792_359_330, 1, 120), vec![1_792_359_000, 1_792_359_300, 1_792_359_600]);
        // les tokens de la précédente ne restent que 15 s après sa fin
        assert_eq!(token_starts(&s, 1_792_359_330, 1), vec![1_792_359_300, 1_792_359_600]);
        assert_eq!(token_starts(&s, 1_792_359_305, 1), vec![1_792_359_000, 1_792_359_300, 1_792_359_600]);
        assert_eq!(gamma_url(&s, 1_792_359_300), "https://gamma-api.polymarket.com/events?slug=btc-updown-5m-1792359300");
    }

    #[test]
    fn tokens_from_gamma_body() {
        assert_eq!(tokens(r#"[{"markets":[{"clobTokenIds":"[\"111\",\"222\"]"}]}]"#), Some(vec!["111".into(), "222".into()]));
        assert_eq!(tokens("[]"), None);
        assert_eq!(tokens("pas du json"), None);
    }
}
