//! The backtest configuration: `config/default.json` holds every parameter with its default (E2); an override file
//! holds only what changes and is merged in depth over it (objects key by key, anything else replaced). Each strategy
//! has its own commented TOML file (`strategy.file`), its defaults those of the code; `strategy.base` and the
//! candidates are applied over it as `explore.mjs` did (`build`: keys replaced, dotted keys set by path).
use crate::venue::VenueCfg;
use engine_core::market::MarketSpec;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Period { pub from: Option<String>, pub to: Option<String>, pub split: String }

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capital { pub start_usd: f64, pub compound: bool, pub claim_s: f64 }

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceCfg { pub root: String, pub formats: Vec<String> }

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MixLatency { pub perp: f64, pub coinbase: f64 }

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackCfg { pub source_root: String, pub data_dir: String, pub packs_dir: String, pub mix_latency_ms: MixLatency }

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceInput { pub trace: String, pub recv_lag_ms: f64 }

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCfg { pub name: String, pub adapter: String, pub input: SourceInput }

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signal { pub name: String, pub sources: Vec<String>, pub field: String, pub combine: String }

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineCfg { pub book_quiet_ms: u64, pub windows_kept_s: u64, pub tick_ms: u64 }

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyCfg {
    pub kind: String,
    pub file: String,
    pub sources: Map<String, Value>,
    /// null: no override of the strategy file.
    pub base: Option<Map<String, Value>>,
    pub candidates: Vec<Map<String, Value>>,
    pub grid: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub name: String,
    pub market: MarketSpec,
    pub period: Period,
    pub capital: Capital,
    pub input: String,
    pub trace: TraceCfg,
    pub pack: PackCfg,
    pub sources: Vec<SourceCfg>,
    pub signal: Signal,
    pub venue: VenueCfg,
    pub variants: Vec<Map<String, Value>>,
    pub engine: EngineCfg,
    pub strategy: StrategyCfg,
    pub out: String,
    /// Directory of the configuration files (strategy files are found from it).
    #[serde(skip)]
    pub dir: PathBuf,
}

/// In-depth merge: objects key by key, anything else (arrays included) replaced.
pub fn merge(a: &mut Value, b: Value) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => for (k, v) in y { merge(x.entry(k).or_insert(Value::Null), v); },
        (a, b) => *a = b,
    }
}

/// `setPath(o, "a.b", v)` of explore.mjs.
pub fn set_path(o: &mut Map<String, Value>, k: &str, v: Value) {
    let mut ks: Vec<&str> = k.split('.').collect();
    let last = ks.pop().unwrap();
    let mut x = o;
    for p in ks {
        let e = x.entry(p).or_insert_with(|| Value::Object(Map::new()));
        if !e.is_object() { *e = Value::Object(Map::new()); }
        x = e.as_object_mut().unwrap();
    }
    x.insert(last.to_string(), v);
}

/// `build(c)` of explore.mjs over `base`: each key of the candidate replaces, a dotted key is set by path
/// (keys taken in alphabetical order: a dotted key after its parent).
pub fn build(base: &Map<String, Value>, c: &Map<String, Value>) -> Map<String, Value> {
    let mut s = base.clone();
    for (k, v) in c { if k.contains('.') { set_path(&mut s, k, v.clone()); } else { s.insert(k.clone(), v.clone()); } }
    s
}

fn read(p: &Path) -> Result<Value, String> {
    let s = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(&s).map_err(|e| format!("{}: {e}", p.display()))
}

impl Config {
    /// `default.json`, then each override file in order, then `extra` (an override already read).
    pub fn load(default: &Path, overrides: &[PathBuf], extra: Option<Value>) -> Result<Config, String> {
        let mut v = read(default)?;
        for o in overrides { merge(&mut v, read(o)?); }
        if let Some(x) = extra { merge(&mut v, x); }
        let mut c: Config = serde_json::from_value(v).map_err(|e| format!("configuration: {e}"))?;
        c.dir = default.parent().unwrap_or(Path::new(".")).to_path_buf();
        c.check()?;
        Ok(c)
    }

    fn check(&self) -> Result<(), String> {
        self.venue.check()?;
        if !self.capital.compound { return Err("capital.compound = false: not implemented (the stake is f × free capital) [à vérifier]".into()); }
        if !matches!(self.input.as_str(), "trace" | "pack") { return Err(format!("input {}: trace or pack", self.input)); }
        if !matches!(self.signal.field.as_str(), "mid" | "trade") { return Err(format!("signal.field {}: mid or trade", self.signal.field)); }
        if self.signal.combine != "mean" { return Err(format!("signal.combine {}: mean", self.signal.combine)); }
        for s in &self.signal.sources { if !self.sources.iter().any(|x| &x.name == s) { return Err(format!("signal source {s} not configured")); } }
        if self.trace.formats.iter().any(|f| f != "current" && f != "old") { return Err("trace.formats: current, old".into()); }
        for v in &self.variants { if !v.contains_key("id") { return Err("variants: each has an id".into()); } }
        Ok(())
    }

    /// The strategy's own file (TOML), as a JSON object: its parameters with their defaults.
    pub fn strategy_defaults(&self) -> Result<Map<String, Value>, String> {
        let p = self.dir.join(&self.strategy.file);
        let s = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let t: toml::Table = toml::from_str(&s).map_err(|e| format!("{}: {e}", p.display()))?;
        match serde_json::to_value(t).map_err(|e| e.to_string())? { Value::Object(m) => Ok(m), _ => Err("strategy file: not a table".into()) }
    }

    /// Every run of the configuration: (candidate id, variant id, strategy parameters, venue). Candidates (none: the
    /// base alone) × grid combinations × variants (none: the venue as configured).
    pub fn runs(&self) -> Result<Vec<(String, String, Map<String, Value>, VenueCfg)>, String> {
        let defaults = self.strategy_defaults()?;
        let mut cands: Vec<Map<String, Value>> = if self.strategy.candidates.is_empty() { vec![Map::new()] } else { self.strategy.candidates.clone() };
        // grid (grid.mjs `combos`): cartesian product of the listed values, each combination a dotted override
        for (k, vals) in &self.strategy.grid {
            let vals = vals.as_array().ok_or(format!("strategy.grid.{k}: a list"))?;
            cands = cands.iter().flat_map(|c| vals.iter().map(move |v| {
                let mut c = c.clone();
                let id = format!("{}{}{k}={v}", c.get("id").and_then(Value::as_str).unwrap_or(""), if c.contains_key("id") { "," } else { "" });
                c.insert(k.clone(), v.clone());
                c.insert("id".into(), Value::String(id));
                c
            })).collect();
        }
        let variants: Vec<Map<String, Value>> = if self.variants.is_empty() { vec![Map::from_iter([("id".to_string(), Value::String(String::new()))])] } else { self.variants.clone() };
        let mut out = vec![];
        for c in &cands {
            let mut p = Value::Object(defaults.clone());
            merge(&mut p, Value::Object(build(self.strategy.base.as_ref().unwrap_or(&Map::new()), c)));
            let Value::Object(mut p) = p else { unreachable!() };
            let id = p.get("id").and_then(Value::as_str).unwrap_or("").to_string();
            // the sources it reads: the maker names them by role; the reference by `source` (price) and `oracle`
            if self.strategy.kind == "reference" {
                for (role, key) in [("price", "source"), ("oracle", "oracle")] { if let Some(n) = self.strategy.sources.get(role) { p.insert(key.into(), n.clone()); } }
            } else if !self.strategy.sources.is_empty() {
                let mut s = Value::Object(p.get("sources").and_then(Value::as_object).cloned().unwrap_or_default());
                merge(&mut s, Value::Object(self.strategy.sources.clone()));
                p.insert("sources".into(), s);
            }
            for v in &variants {
                let mut venue = serde_json::to_value(&self.venue).unwrap();
                for (k, x) in v { if k != "id" { let k = k.strip_prefix("venue.").ok_or(format!("variant key {k}: venue.<param>"))?; let Value::Object(m) = &mut venue else { unreachable!() }; set_path(m, k, x.clone()); } }
                let venue: VenueCfg = serde_json::from_value(venue).map_err(|e| format!("variant {}: {e}", v["id"]))?;
                venue.check()?;
                let mut q = p.clone();
                self.inject(&mut q, &venue);
                out.push((id.clone(), v["id"].as_str().unwrap_or("").to_string(), q, venue));
            }
        }
        Ok(out)
    }

    /// What the strategy reads of the exchange and of the capital is the configuration's, not its file's.
    fn inject(&self, p: &mut Map<String, Value>, v: &VenueCfg) {
        let n = |x: f64| Value::from(x);
        if self.strategy.kind == "maker" {
            p.insert("oneWay".into(), n(v.one_way_ms));
            p.insert("delay".into(), n(v.fill_lookback_ms / 1000.0));
            p.insert("silenceMs".into(), n(v.silence_ms));
            p.insert("minSize".into(), n(v.min_size_rule.min_shares));
            p.insert("capital0".into(), n(self.capital.start_usd));
            p.insert("claim".into(), n(self.capital.claim_s));
        } else if self.strategy.kind == "reference" {
            p.insert("bankroll".into(), n(self.capital.start_usd));
        }
    }

    pub fn path(&self, p: &str) -> PathBuf { let p = Path::new(p); if p.is_absolute() { p.to_path_buf() } else { self.dir.join(p) } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_and_build_follow_explore() {
        let mut a = json!({ "venue": { "one_way_ms": 12.2, "taker_hold": { "seed": 7, "quantiles_s": [[0, 1], [1, 2]] } }, "x": [1, 2] });
        merge(&mut a, json!({ "venue": { "taker_hold": { "seed": 8 } }, "x": [3] }));
        assert_eq!(a, json!({ "venue": { "one_way_ms": 12.2, "taker_hold": { "seed": 8, "quantiles_s": [[0, 1], [1, 2]] } }, "x": [3] }));
        // x01 candidate T-P90-E270-O0.01: a whole `price`, then "price.offset"
        let c = json!({ "id": "T", "price": { "mode": "offset", "offset": 0.02 }, "price.offset": 0.01, "sideProb": [0.9, 0.999] });
        let b = build(&Map::new(), c.as_object().unwrap());
        assert_eq!(Value::Object(b), json!({ "id": "T", "price": { "mode": "offset", "offset": 0.01 }, "sideProb": [0.9, 0.999] }));
    }

    #[test]
    fn default_configuration_is_valid_and_its_strategy_files_read() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("config");
        let c = Config::load(&dir.join("default.json"), &[], None).unwrap();
        let runs = c.runs().unwrap();
        assert_eq!(runs.len(), 1);
        let p = &runs[0].2;
        assert_eq!((p["spot"].as_str(), p["sigmaMult"].as_f64(), p["delay"].as_f64(), p["oneWay"].as_f64()), (Some("spot"), Some(1.3), Some(0.0), Some(12.2)));
        // the maker file holds the code's defaults, strictly
        let mut file = c.strategy_defaults().unwrap();
        for k in ["sources", "capital0", "claim", "keepMs", "horizonS"] { file.remove(k); }
        let from_file: engine_strategy::maker::Params = serde_json::from_value(Value::Object(file)).unwrap();
        assert_eq!(from_file, engine_strategy::maker::Params::default());
        let r = Config::load(&dir.join("default.json"), &[dir.join("reference.json")], None).unwrap();
        let mut rf = r.strategy_defaults().unwrap();
        for k in ["oracle", "horizonS"] { rf.remove(k); }
        rf.insert("source".into(), json!(""));
        let from_file: engine_strategy::reference::Config = serde_json::from_value(Value::Object(rf)).unwrap();
        assert_eq!(serde_json::to_value(from_file).unwrap(), serde_json::to_value(engine_strategy::reference::Config::default()).unwrap());
    }
}
