//! Config du tracer (`config.json`, Contrat 1 + 3), relue au changement de mtime.
use engine_core::market::MarketSpec;
use serde::Deserialize;

/// Sources HTTP interrogées par le tracer lui-même (les autres passent par `registry::feed_adapter`).
pub const HTTP_GAMMA: &str = "polymarket-gamma";
pub const HTTP_CLOB_TIME: &str = "polymarket-clob-time";
/// Adapter WebSocket dont les topics (tokens des fenêtres) sont découverts via gamma.
pub const CLOB_MARKET: &str = "polymarket-market";

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SourceCfg {
    pub name: String,
    pub adapter: String,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default = "one")]
    pub connections: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub host_id: String,
    /// Racine des traces (`<root>/<source>/<AAAA>/<MM>/…`, `<root>/status.json`).
    pub root: String,
    #[serde(default = "btc5m")]
    pub market: MarketSpec,
    /// Période de sondage de gamma (une requête par fenêtre suivie à chaque période).
    #[serde(default = "gamma_poll_ms")]
    pub gamma_poll_ms: u64,
    #[serde(default = "clob_time_poll_ms")]
    pub clob_time_poll_ms: u64,
    /// Nombre de fenêtres suivant la courante (gamma enregistré, tokens CLOB abonnés).
    #[serde(default = "windows_ahead")]
    pub windows_ahead: u64,
    /// Une fenêtre finie reste sondée (résolution, price to beat) ce nombre de secondes après sa fin. La résolution
    /// arrive ≈ 5 min après la fin (314 s mesurées) : 120 s la manquait.
    #[serde(default = "prev_grace_s")]
    pub gamma_prev_grace_s: u64,
    /// Garde disque (D3) : arrêt d'écriture sous ce seuil, reprise au-dessus de l'autre.
    #[serde(default = "disk_min")]
    pub disk_min_free_gb: f64,
    #[serde(default = "disk_resume")]
    pub disk_resume_free_gb: f64,
    pub sources: Vec<SourceCfg>,
}

fn one() -> u32 { 1 }
fn btc5m() -> MarketSpec { MarketSpec { venue: "polymarket".into(), asset: "btc".into(), horizon_s: 300 } }
fn gamma_poll_ms() -> u64 { 10_000 }
fn clob_time_poll_ms() -> u64 { 2_000 }
fn windows_ahead() -> u64 { 2 }
fn prev_grace_s() -> u64 { 900 }
fn disk_min() -> f64 { 2.0 }
fn disk_resume() -> f64 { 3.0 }

pub fn is_http(adapter: &str) -> bool { adapter == HTTP_GAMMA || adapter == HTTP_CLOB_TIME }

impl Config {
    pub fn parse(text: &str) -> Result<Config, String> {
        let c: Config = serde_json::from_str(text).map_err(|e| format!("config : {e}"))?;
        c.validate()?;
        Ok(c)
    }

    pub fn load(path: &str) -> Result<Config, String> {
        Config::parse(&std::fs::read_to_string(path).map_err(|e| format!("{path} : {e}"))?)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.host_id.is_empty() || self.root.is_empty() { return Err("host_id et root sont obligatoires".into()); }
        if self.market.horizon_s == 0 || self.market.horizon_s % 60 != 0 { return Err("market.horizon_s doit être un multiple de 60".into()); }
        if self.gamma_poll_ms == 0 || self.clob_time_poll_ms == 0 { return Err("périodes de sondage nulles".into()); }
        if !(self.disk_min_free_gb < self.disk_resume_free_gb) { return Err("disk_min_free_gb doit être < disk_resume_free_gb".into()); }
        for (i, s) in self.sources.iter().enumerate() {
            if s.name.is_empty() || s.name.contains(['/', '\\']) || s.name.starts_with('.') { return Err(format!("nom de source invalide : {:?}", s.name)); }
            if self.sources[..i].iter().any(|o| o.name == s.name) { return Err(format!("source en double : {}", s.name)); }
            if s.connections != 1 { return Err(format!("{} : connections doit valoir 1 (une seule connexion par source)", s.name)); }
            if !is_http(&s.adapter) && engine_app::registry::feed_adapter(&s.adapter).is_none() { return Err(format!("{} : adapter inconnu {}", s.name, s.adapter)); }
        }
        Ok(())
    }
}
