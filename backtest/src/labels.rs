//! Which book decreases of a window were trades (`engine/maker-bt/fills.mjs` of btc5m-lag-sim, ported operation for
//! operation): the full fills of data-api give, per taker order, the volume taken at each Up-book level; each order is
//! anchored on the decrease of its largest level equal to that volume inside [block − 4.5 s, block + 1 s], nearest to
//! a 2 s lag; its other levels are matched within 150 ms of the anchor. The `exact` fill bound reads these volumes.
use crate::packs::{num, read_json, Source};
use engine_strategy::model::jround;
use std::collections::HashMap;

const COLS: usize = 5;

/// One taker order of the full fills: block time (ms), levels [type 1 bid / 2 ask of the Up book, level, shares]
/// largest first; `book_ts`: server time of its match in the book, once anchored.
#[derive(Clone, Debug)]
pub struct Taker { pub tx: String, pub ts: f64, pub levels: Vec<(u32, u32, f64)>, pub book_ts: Option<f64> }

/// `loadFills(start)`: the window's taker orders in time order, and whether data-api truncated them. None: no file.
pub fn load_fills(src: &Source, start: u64) -> Option<(Vec<Taker>, bool)> {
    let w = src.window_json(start)?;
    let cid = w["conditionId"].as_str()?;
    let f = src.data.join("polymarket").join("fills").join(format!("{cid}.json"));
    if !f.exists() { return None; }
    let j = read_json(&f).ok()?;
    let truncated = j["truncated"].as_bool().unwrap_or(false);
    // insertion order of transactions and of levels within one, as the JS Maps
    let mut order: Vec<(String, f64, Vec<(u32, f64)>)> = vec![];
    let mut at: HashMap<String, usize> = HashMap::new();
    for r in j["rows"].as_array()? {
        let maker = match &r[5] { serde_json::Value::Bool(b) => *b, v => num(v).is_some_and(|m| m != 0.0 && !m.is_nan()) };
        if !maker { continue; }
        let (ts, side, outcome, price, size, hash) = (num(&r[0])?, r[1].as_str()?, r[2].as_str()?, num(&r[3])?, num(&r[4])?, r[7].as_str()?);
        let (up, sell, p) = (outcome == "Up", side == "SELL", jround(price * 1000.0) as i64);
        let typ: i64 = if sell == up { 2 } else { 1 };
        let i = if up { p } else { 1000 - p };
        let k = (typ * 2000 + i) as u32;
        let x = *at.entry(hash.to_string()).or_insert_with(|| { order.push((hash.to_string(), ts * 1000.0, vec![])); order.len() - 1 });
        let lv = &mut order[x].2;
        match lv.iter_mut().find(|e| e.0 == k) { Some(e) => e.1 += size, None => lv.push((k, size)) }
    }
    let mut out: Vec<Taker> = order.into_iter().map(|(tx, ts, lv)| {
        let mut levels: Vec<(u32, u32, f64)> = lv.into_iter().map(|(k, v)| (k / 2000, k % 2000, v)).collect();
        levels.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        Taker { tx, ts, levels, book_ts: None }
    }).collect();
    out.sort_by(|a, b| a.ts.partial_cmp(&b.ts).unwrap_or(std::cmp::Ordering::Equal));
    Some((out, truncated))
}

/// `labelTrades(ev, start, fills, shift)`: traded volume carried by each book record (f32, 0 = a cancel or a post);
/// sets `book_ts` of each anchored taker order.
pub fn label_trades(ev: &[f32], start: u64, fills: &mut [Taker], shift: f64) -> Vec<f32> {
    let n = ev.len() / COLS;
    let o = start as f64 * 1000.0;
    let mut vol = vec![0f32; n];
    let mut lvl = vec![0f64; 3 * 1001];
    let mut dec: HashMap<usize, Vec<(f64, usize, f64)>> = HashMap::new();
    let mut snap_t = f64::NAN;
    for j in 0..n {
        let (t, typ) = (ev[COLS * j] as f64, ev[COLS * j + 2] as f64);
        if typ == 0.0 { lvl.fill(0.0); snap_t = t; continue; }
        let i = jround(ev[COLS * j + 3] as f64 * 1000.0);
        let k = typ * 1001.0 + i;
        if !(0.0..3003.0).contains(&k) { continue; } // out of the typed array: no level (JS reads undefined)
        let (k, s) = (k as usize, ev[COLS * j + 4] as f64);
        let old = lvl[k];
        lvl[k] = s;
        if t == snap_t || !(old - s > 1e-6) { continue; }
        dec.entry(k).or_default().push((ev[COLS * j + 1] as f64 + o, j, old - s));
    }
    let mut used = vec![false; n];
    let find = |used: &[bool], typ: u32, i: u32, v: f64, lo: f64, hi: f64, target: f64| -> Option<(f64, usize, f64)> {
        let mut best: Option<(f64, usize, f64)> = None;
        for &c in dec.get(&(typ as usize * 1001 + i as usize)).map_or(&[][..], |x| x.as_slice()) {
            if c.0 < lo || c.0 > hi { continue; }
            if used[c.1] || (c.2 - v).abs() > 0.011 { continue; }
            if best.is_none_or(|b| (c.0 - target).abs() < (b.0 - target).abs()) { best = Some(c); }
        }
        best
    };
    for x in fills.iter_mut() {
        let t = x.ts + shift;
        let (t0, i0, v0) = x.levels[0];
        let Some(a) = find(&used, t0, i0, v0, t - 4500.0, t + 999.0, t - 2000.0) else { continue };
        used[a.1] = true;
        vol[a.1] = v0 as f32;
        x.book_ts = Some(a.0);
        for &(typ, i, v) in &x.levels[1..] {
            if let Some(c) = find(&used, typ, i, v, a.0 - 150.0, a.0 + 150.0, a.0) { used[c.1] = true; vol[c.1] = v as f32; }
        }
    }
    vol
}

/// `labels` of the `exact` bound for one window (`simWindow`: none when the fills are missing or truncated).
pub fn labels(src: &Source, start: u64, book: &[f32]) -> Option<Vec<f32>> {
    let (mut fills, truncated) = load_fills(src, start)?;
    if truncated { return None; }
    Some(label_trades(book, start, &mut fills, 0.0))
}
