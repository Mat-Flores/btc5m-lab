//! Line by line comparison of two JSONL exports (oracle Node vs this backtest): every key of the oracle's line must
//! hold the same value in ours (numbers compared as numbers, f64 bits written in hexadecimal compared as text).
use serde_json::Value;
use std::path::Path;

fn lines(p: &Path) -> Result<Vec<Value>, String> {
    std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?.lines().filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).map_err(|e| format!("{}: {e}", p.display()))).collect()
}

fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b)),
        _ => a == b,
    }
}

/// (lines compared, differences "line k: key: oracle vs ours").
pub fn files(oracle: &Path, ours: &Path) -> Result<(usize, Vec<String>), String> {
    let (a, b) = (lines(oracle)?, lines(ours)?);
    let mut d = vec![];
    if a.len() != b.len() { d.push(format!("line count: oracle {} vs ours {}", a.len(), b.len())); }
    for (k, (x, y)) in a.iter().zip(&b).enumerate() {
        for (key, v) in x.as_object().into_iter().flatten() {
            let w = &y[key];
            if !same(v, w) { d.push(format!("line {}: {key}: oracle {v} vs ours {w} ({})", k + 1, x)); }
        }
    }
    Ok((a.len(), d))
}
