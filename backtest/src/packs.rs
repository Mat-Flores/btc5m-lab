//! Data of the maker backtest of btc5m-lag-sim, read in place (READ ONLY): its packs (`engine/maker-bt/packs`, made
//! by `data.mjs`) and its public data (`data/polymarket`, `data/binance/aggTrades`). Same arrays as `loadA`, `loadB`,
//! `readB`, `windows()` (run.mjs), `binancePx` and `mixPx` (binance-trades.mjs), values bit for bit.
use engine_strategy::maker::Fee;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn read_json(p: &Path) -> Result<Value, String> {
    let s = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(&s).map_err(|e| format!("{}: {e}", p.display()))
}
fn bytes(p: &Path) -> Result<Vec<u8>, String> { std::fs::read(p).map_err(|e| format!("{}: {e}", p.display())) }
pub fn f32s(p: &Path) -> Result<Vec<f32>, String> { Ok(bytes(p)?.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()) }
pub fn f64s(p: &Path) -> Result<Vec<f64>, String> { Ok(bytes(p)?.chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect()) }

/// `Number(x)` of a JSON value as the simulator reads it (absent → None).
pub fn num(v: &Value) -> Option<f64> { match v { Value::Number(n) => n.as_f64(), Value::String(s) => s.trim().parse().ok(), _ => None } }

/// `w.fee ?? CRYPTO_FEE` (sim.mjs:167, fees.js:5): rate, exponent, rebateRate (absent → 0, `Number(x ?? 0)`).
pub fn fee_of(v: &Value) -> Fee {
    if v.is_null() { return Fee { rate: 0.07, exponent: 1.0, rebate_rate: 0.0 }; }
    Fee { rate: num(&v["rate"]).unwrap_or(0.0), exponent: num(&v["exponent"]).unwrap_or(1.0), rebate_rate: num(&v["rebateRate"]).unwrap_or(0.0) }
}

/// One recorded window joined with its public data (run.mjs `windows()`).
#[derive(Clone)]
pub struct Win {
    pub start: u64,
    /// `w.tick0 ?? 0.01`.
    pub tick0: f64,
    /// 0 Up, 1 Down, -1 unknown.
    pub winner: i64,
    pub fee: Fee,
    /// Public taker tape (A.trades.f32 rows [t, hit side, price, size]).
    pub trades: Vec<f32>,
    pub prev_volume: f64,
}

/// One window's arrays (`readB`): book [t, ts, type, price, size] (f32), OKX mid [t, p], Chainlink [t, ts, p].
pub struct Pack { pub book: Vec<f32>, pub px: Vec<f64>, pub cl: Vec<f64> }

pub struct Source { pub data: PathBuf, pub packs: PathBuf, days: HashMap<String, Option<Vec<f64>>> }

impl Source {
    pub fn new(data: PathBuf, packs: PathBuf) -> Source { Source { data, packs, days: HashMap::new() } }

    /// Starts and `tick0` of B (B.json), in its order.
    pub fn load_b(&self) -> Result<Vec<(u64, f64)>, String> {
        let b = read_json(&self.packs.join("B.json"))?;
        Ok(b["windows"].as_array().ok_or("B.json: windows")?.iter().map(|w| (w["start"].as_u64().unwrap_or(0), num(&w["tick0"]).unwrap_or(0.01))).collect())
    }

    pub fn read_b(&self, start: u64) -> Result<Pack, String> {
        let f = |ext: &str| self.packs.join("B").join(format!("{start}.{ext}"));
        Ok(Pack { book: f32s(&f("book.f32"))?, px: f64s(&f("px.f64"))?, cl: f64s(&f("cl.f64"))? })
    }

    /// run.mjs `windows()`: B joined with A (winner, fee, tape, previous window's taker volume).
    pub fn windows(&self) -> Result<Vec<Win>, String> {
        let a = read_json(&self.packs.join("A.json"))?;
        let tr = f32s(&self.packs.join("A.trades.f32"))?;
        let mut by: HashMap<u64, (i64, Fee, Vec<f32>)> = HashMap::new();
        for w in a["windows"].as_array().ok_or("A.json: windows")? {
            let (off, n) = (w["off"].as_u64().unwrap_or(0) as usize, w["n"].as_u64().unwrap_or(0) as usize);
            let rows = tr.get(4 * off..4 * (off + n)).ok_or("A.trades.f32 too short")?.to_vec();
            by.insert(w["start"].as_u64().unwrap_or(0), (w["winner"].as_i64().unwrap_or(-1), fee_of(&w["fee"]), rows));
        }
        let vol = |t: &[f32]| { let mut v = 0.0; for j in 0..t.len() / 4 { v += t[4 * j + 3] as f64; } v };
        Ok(self.load_b()?.into_iter().filter_map(|(start, tick0)| {
            let (winner, fee, trades) = by.get(&start)?.clone();
            let prev_volume = by.get(&(start - 300)).map_or(f64::NAN, |p| vol(&p.2));
            Some(Win { start, tick0, winner, fee, trades, prev_volume })
        }).collect())
    }

    pub fn window_json(&self, start: u64) -> Option<Value> {
        let f = self.data.join("polymarket").join("windows").join(format!("btc-updown-5m-{start}.json"));
        f.exists().then(|| read_json(&f).ok()).flatten()
    }

    /// A cached day of Binance or Coinbase trades [time ms, price] (data/binance/aggTrades/<market>-<day>.f64).
    fn day(&mut self, market: &str, day: i64) -> Option<&Vec<f64>> {
        let key = format!("{market}-{}", ymd(day));
        if !self.days.contains_key(&key) {
            let f = self.data.join("binance").join("aggTrades").join(format!("{key}.f64"));
            self.days.insert(key.clone(), f64s(&f).ok());
        }
        self.days[&key].as_ref()
    }

    /// binance-trades.mjs `binancePx`: [t ms from the open, price] at each price change, each trade seen `latency` ms
    /// after its exchange time, from 360 s before the open to 310 s after; None when a day is not on disk.
    pub fn binance_px(&mut self, start: u64, market: &str, latency: f64) -> Option<Vec<f64>> {
        let o = start as f64 * 1000.0;
        let mut out: Vec<f64> = vec![];
        let (d0, d1) = (((o - 360e3) / 86400e3).floor() as i64, ((o + 310e3) / 86400e3).floor() as i64);
        for d in d0..=d1 {
            let a = self.day(market, d)?;
            let (mut lo, mut hi) = (0usize, a.len() / 2);
            while lo < hi { let m = (lo + hi) >> 1; if a[2 * m] < o - 360e3 - latency { lo = m + 1; } else { hi = m; } }
            let mut i = lo;
            while i < a.len() / 2 && a[2 * i] <= o + 310e3 - latency {
                let t = a[2 * i] + latency - o;
                if out.last() != Some(&a[2 * i + 1]) { out.push(t); out.push(a[2 * i + 1]); }
                i += 1;
            }
        }
        Some(out)
    }

    /// binance-trades.mjs `mixPx`: mean of Binance perp and Coinbase at their arrivals, Coinbase shifted by the median
    /// perp − Coinbase gap over the 300 s before the open.
    pub fn mix_px(&mut self, start: u64, perp_lat: f64, cb_lat: f64) -> Option<Vec<f64>> {
        let a = self.binance_px(start, "perp", perp_lat)?;
        let b = self.binance_px(start, "coinbase", cb_lat)?;
        let mut gaps = vec![];
        let mut j = 0;
        for i in 0..a.len() / 2 {
            if a[2 * i] < -300e3 || a[2 * i] > 0.0 { continue; }
            while j + 1 < b.len() / 2 && b[2 * j + 2] <= a[2 * i] { j += 1; }
            gaps.push(a[2 * i + 1] - b[2 * j + 1]);
        }
        gaps.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let off = if gaps.is_empty() { 0.0 } else { gaps[gaps.len() >> 1] };
        let (mut out, mut i, mut j, mut pa, mut pb) = (vec![], 0, 0, f64::NAN, f64::NAN);
        while i < a.len() / 2 || j < b.len() / 2 {
            let ta = if i < a.len() / 2 { a[2 * i] } else { f64::INFINITY };
            let tb = if j < b.len() / 2 { b[2 * j] } else { f64::INFINITY };
            let t = ta.min(tb);
            if ta == t { pa = a[2 * i + 1]; i += 1; }
            if tb == t { pb = b[2 * j + 1] + off; j += 1; }
            if pa.is_finite() && pb.is_finite() { out.push(t); out.push((pa + pb) / 2.0); }
        }
        Some(out)
    }
}

/// "YYYY-MM-DD" of a day number since 1970-01-01 (UTC).
pub fn ymd(day: i64) -> String {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Unix seconds of "YYYY-MM-DDTHH:MM:SSZ" (the only form the configuration takes).
pub fn parse_utc(s: &str) -> Result<u64, String> {
    let b = s.as_bytes();
    let n = |a: usize, l: usize| s.get(a..a + l).and_then(|x| x.parse::<i64>().ok()).ok_or_else(|| format!("date {s}: expected YYYY-MM-DDTHH:MM:SSZ"));
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z' { return Err(format!("date {s}: expected YYYY-MM-DDTHH:MM:SSZ")); }
    let (y, m, d) = (n(0, 4)?, n(5, 2)?, n(8, 2)?);
    let (y2, m2) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * m2 + 2) / 5 + d - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    Ok((days * 86400 + n(11, 2)? * 3600 + n(14, 2)? * 60 + n(17, 2)?) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dates_round_trip() {
        assert_eq!(parse_utc("2026-10-20T00:00:00Z").unwrap(), 1_792_454_400);
        assert_eq!(ymd(1_792_454_400 / 86400), "2026-10-20");
        assert_eq!(ymd(0), "1970-01-01");
        assert!(parse_utc("2026-10-20").is_err());
    }
}
