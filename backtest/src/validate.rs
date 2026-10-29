//! Ground truth of the fill model (`engine/maker-bt/validate.mjs` of btc5m-lag-sim, `--delay d --sim exact`, ported
//! operation for operation): real maker orders of the recorded windows (on-chain `matchOrders`: size, price, creation
//! time) replayed as twins through the book, then through `sim_window` itself (strategy `script`), and compared with
//! their real fills. An order's arrival is the book increase of exactly its size at its level from its creation on;
//! a partly filled order ends at the decrease of exactly its remainder that is no trade.
use crate::labels::{label_trades, load_fills};
use crate::packs::{num, read_json, Source, Win};
use crate::venue::{exact_fill, Order, VenueCfg};
use crate::window::{sim_window, Input, Script};
use engine_strategy::maker::Params;
use engine_strategy::model::jround;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;

const COLS: usize = 5;

/// One twin (`twins()` of validate.mjs).
#[derive(Clone, Debug)]
pub struct Twin {
    pub start: u64, pub j: usize, pub s: usize, pub q: f64, pub size: f64, pub qa0: f64, pub arrive: f64, pub end: f64,
    pub real: Vec<(f64, f64)>, pub pred: Vec<(f64, f64)>, pub real_sh: f64, pub pred_sh: f64,
}

/// `Number.prototype.toFixed` (half away from zero on exact decimal ties, unlike Rust's formatting).
pub fn to_fixed(x: f64, p: usize) -> String {
    if !x.is_finite() { return format!("{x}"); }
    let long = format!("{:.*}", p + 30, x.abs());
    let (int, frac) = long.split_once('.').unwrap();
    let mut digits: Vec<u8> = format!("{int}{}", &frac[..p]).into_bytes();
    if frac.as_bytes()[p] >= b'5' {
        let mut i = digits.len();
        loop {
            if i == 0 { digits.insert(0, b'1'); break; }
            i -= 1;
            if digits[i] == b'9' { digits[i] = b'0'; } else { digits[i] += 1; break; }
        }
    }
    let s = String::from_utf8(digits).unwrap();
    let (a, b) = s.split_at(s.len() - p);
    let body = if p == 0 { a.to_string() } else { format!("{a}.{b}") };
    if x < 0.0 { format!("-{body}") } else { body }
}

/// `String(x)` of a JavaScript number, for the values validate.mjs prints that way (ms, finite, < 1e21).
fn js_str(x: f64) -> String { if x == 0.0 { "0".into() } else { format!("{x}") } }

/// `twins(start, { delay })`: None when the window has no on-chain orders or no full fills.
pub fn twins(src: &Source, start: u64, book: &[f32], delay: f64) -> Option<(Vec<Twin>, usize)> {
    let win = src.window_json(start)?;
    let cid = win["conditionId"].as_str()?;
    let of = src.data.join("polymarket").join("orders").join(format!("{cid}.json"));
    let (mut fills, _) = load_fills(src, start)?;
    if !of.exists() { return None; }
    let (ev, n, o0) = (book, book.len() / COLS, start as f64 * 1000.0);
    let vol = label_trades(ev, start, &mut fills, 0.0);
    let book_ts: HashMap<&str, f64> = fills.iter().filter_map(|x| x.book_ts.filter(|&t| t != 0.0).map(|t| (x.tx.as_str(), t))).collect();
    // orders and their real fills, in first-seen order (a JS Map)
    let matches = read_json(&of).ok()?;
    let mut keys: HashMap<String, usize> = HashMap::new();
    let mut orders: Vec<(Value, Vec<(f64, f64)>)> = vec![];
    for m in matches.as_array()? {
        let Some(&t) = m["tx"].as_str().and_then(|tx| book_ts.get(tx)) else { continue };
        for x in m["makers"].as_array().map_or(&[][..], |v| v.as_slice()) {
            if !num(&x["ts"]).is_some_and(|ts| ts > 0.0) { continue; }
            let k = match x["hash"].as_str() {
                Some(h) => h.to_string(),
                None => format!("{}|{}|{}|{:x}|{:.4}|{:x}", x["maker"], x["token"], x["side"], num(&x["size"]).unwrap_or(f64::NAN).to_bits(), num(&x["price"]).unwrap_or(f64::NAN), num(&x["ts"]).unwrap().to_bits()),
            };
            let i = *keys.entry(k).or_insert_with(|| { orders.push((x.clone(), vec![])); orders.len() - 1 });
            orders[i].1.push((t, num(&x["filled"]).unwrap_or(f64::NAN)));
        }
    }
    // per Up-book level: records [j, ts, old, new, in a snapshot] in receive order
    let mut lv: HashMap<usize, Vec<(usize, f64, f64, f64, bool)>> = HashMap::new();
    let mut arr = vec![0f64; 3 * 1001];
    let mut snap_t = f64::NAN;
    for j in 0..n {
        let (t, typ) = (ev[COLS * j] as f64, ev[COLS * j + 2] as f64);
        if typ == 0.0 { arr.fill(0.0); snap_t = t; continue; }
        let k = typ * 1001.0 + jround(ev[COLS * j + 3] as f64 * 1000.0);
        if !(0.0..3003.0).contains(&k) { continue; }
        let (k, s) = (k as usize, ev[COLS * j + 4] as f64);
        lv.entry(k).or_default().push((j, ev[COLS * j + 1] as f64 + o0, arr[k], s, t == snap_t));
        arr[k] = s;
    }
    let tokens = [win["tokens"][0].as_str().unwrap_or(""), win["tokens"][1].as_str().unwrap_or("")];
    let (mut out, mut missing) = (vec![], 0usize);
    for (x, real) in &orders {
        let token = x["token"].as_str().unwrap_or("");
        let up = token == tokens[0];
        if !up && token != tokens[1] { continue; }
        let (price, size, side) = (num(&x["price"]).unwrap_or(f64::NAN), num(&x["size"]).unwrap_or(f64::NAN), num(&x["side"]).unwrap_or(f64::NAN));
        let p = jround(price * 1000.0);
        let s = if (side == 0.0) == up { 0 } else { 1 };
        let q = if side == 0.0 { p } else { 1000.0 - p };
        let (typ, i) = if s == 0 { (1.0, q) } else { (2.0, 1000.0 - q) };
        let recs = lv.get(&((typ * 1001.0 + i) as usize)).map_or(&[][..], |v| v.as_slice());
        let ts = num(&x["ts"]).unwrap();
        let created = if ts < 1e12 { ts * 1000.0 } else { ts };
        let first_real = real.iter().fold(f64::INFINITY, |a, r| a.min(r.0));
        let Some(a) = recs.iter().find(|r| !r.4 && r.1 >= created - 1000.0 && r.1 <= first_real && (r.3 - r.2 - size).abs() < 0.011) else { missing += 1; continue };
        let real_sh = real.iter().fold(0.0, |t, r| t + r.1);
        let last_real = real.iter().fold(f64::NEG_INFINITY, |a, r| a.max(r.0));
        let mut end = f64::INFINITY;
        if real_sh < size - 0.011 {
            if let Some(c) = recs.iter().find(|r| r.1 >= last_real && r.0 > a.0 && vol[r.0] == 0.0 && (r.2 - r.3 - (size - real_sh)).abs() < 0.011) { end = c.1; }
        }
        // replay from the arrival with the simulator's rule
        let mut o = Order { s, q: q / 1000.0, r: size, qa: a.2 };
        let mut pred = vec![];
        let mut bk = vec![0f64; 3 * 1001];
        let mut snap = f64::NAN;
        let mut j = 0;
        while j < n && o.r > 1e-9 {
            let (t, typ2, ts) = (ev[COLS * j] as f64, ev[COLS * j + 2] as f64, ev[COLS * j + 1] as f64 + o0);
            if typ2 == 0.0 { bk.fill(0.0); snap = t; j += 1; continue; }
            let ci = jround(ev[COLS * j + 3] as f64 * 1000.0);
            let k = typ2 * 1001.0 + ci;
            let sz = ev[COLS * j + 4] as f64;
            let old = if (0.0..3003.0).contains(&k) { let k = k as usize; let old = bk[k]; bk[k] = sz; old } else { f64::NAN };
            if j <= a.0 || t == snap || ts - delay >= end { j += 1; continue; }
            let take = engine_strategy::model::jmin(o.r, exact_fill(&mut o, s, typ2 as u8, ci, old, sz, vol[j] as f64, 1.0));
            if take > 1e-9 { o.r -= take; pred.push((ts, take)); }
            j += 1;
        }
        let pred_sh = pred.iter().fold(0.0, |t, r| t + r.1);
        out.push(Twin { start, j: a.0, s, q, size, qa0: a.2, arrive: a.1, end, real: real.clone(), pred, real_sh, pred_sh });
    }
    Some((out, missing))
}

/// validate.mjs `q(a, p)`: the element at floor(p × (n − 1)) of the sorted values.
fn quant(a: &[f64], p: f64) -> Option<f64> {
    if a.is_empty() { return None; }
    let mut v = a.to_vec();
    v.sort_by(|x, y| x.partial_cmp(y).unwrap());
    Some(v[(p * (v.len() - 1) as f64).floor() as usize])
}

/// Report of the run: the lines validate.mjs prints, then a line of bits (f64 in hexadecimal) for the comparison.
pub struct Report { pub lines: Vec<String> }

/// The whole of `validate.mjs --delay <delay s> --sim exact`. `only`: restrict to these windows. `export`: one line
/// per twin of the simulator part (same fields as backtest/oracle/validate-export.mjs).
pub fn run(src: &Source, wins: &[Win], base: &Params, cfg: &VenueCfg, delay_s: f64, only: Option<&[u64]>, mut export: Option<&mut dyn Write>) -> Result<Report, String> {
    let delay = delay_s; // validate.mjs passes `--delay` (s) to twins as is, compared with ms stamps
    let mut all: Vec<Twin> = vec![];
    let mut n_all = 0usize;
    let b = src.load_b()?;
    let mut books: HashMap<u64, crate::packs::Pack> = HashMap::new();
    for &(start, _) in &b {
        if only.is_some_and(|o| !o.contains(&start)) { continue; }
        let Some(w) = src.window_json(start) else { continue };
        if w["conditionId"].as_str().is_none_or(|c| !src.data.join("polymarket").join("orders").join(format!("{c}.json")).exists()) { continue; }
        let pack = src.read_b(start)?;
        if let Some((t, missing)) = twins(src, start, &pack.book, delay) { n_all += t.len() + missing; all.extend(t); books.insert(start, pack); }
    }
    let ok = &all;
    let mut lines = vec![];
    let sum = |v: &mut dyn Iterator<Item = f64>| v.fold(0.0, |t, x| t + x);
    let (r, p) = (sum(&mut ok.iter().map(|x| x.real_sh)), sum(&mut ok.iter().map(|x| x.pred_sh)));
    lines.push(format!("orders {n_all}, arrival found {}; real filled {} sh, predicted {} sh (ratio {})", ok.len(), to_fixed(r, 0), to_fixed(p, 0), to_fixed(p / r, 3)));
    let first: Vec<f64> = ok.iter().filter(|x| !x.pred.is_empty()).map(|x| x.pred[0].0 - x.real.iter().fold(f64::INFINITY, |a, r| a.min(r.0))).collect();
    let qs = |a: &[f64], f: &dyn Fn(f64) -> String| [0.1, 0.5, 0.9].iter().map(|&q| quant(a, q).map_or(String::new(), f)).collect::<Vec<_>>().join(",");
    lines.push(format!("predicted at least one fill: {}; first fill predicted - real (ms) q10/50/90 {}", to_fixed(first.len() as f64 / ok.len() as f64, 3), qs(&first, &js_str)));
    let err: Vec<f64> = ok.iter().map(|x| x.pred_sh - x.real_sh).collect();
    lines.push(format!("per order predicted - real (sh) q10/50/90 {}, exact (±0.02) {}", qs(&err, &|x| to_fixed(x, 2)), to_fixed(err.iter().filter(|e| e.abs() < 0.02).count() as f64 / ok.len() as f64, 3)));
    let known: Vec<&Twin> = ok.iter().filter(|x| x.end != f64::INFINITY || x.real_sh >= x.size - 0.011).collect();
    let (rk, pk) = (sum(&mut known.iter().map(|x| x.real_sh)), sum(&mut known.iter().map(|x| x.pred_sh)));
    lines.push(format!("orders with a known end (filled or cancel found): {}; real {} predicted {} (ratio {})", known.len(), to_fixed(rk, 0), to_fixed(pk, 0), to_fixed(pk / rk, 3)));

    let w_by: HashMap<u64, &Win> = wins.iter().map(|w| (w.start, w)).collect();
    let tw: Vec<&Twin> = ok.iter().filter(|x| x.arrive >= x.start as f64 * 1000.0 && w_by.get(&x.start).is_some_and(|w| w.winner == 0 || w.winner == 1)
        && (x.end != f64::INFINITY || x.real_sh >= x.size - 0.011)).collect();
    let (mut pred, mut exact, mut pnl, mut real, mut real_pnl) = (0.0, 0usize, 0.0, 0.0, 0.0);
    let mut days: Vec<(String, f64, f64)> = vec![];
    let mut labels: HashMap<u64, Option<Vec<f32>>> = HashMap::new();
    for x in &tw {
        let w = w_by[&x.start];
        let d = &books[&x.start];
        if !labels.contains_key(&x.start) { labels.clear(); labels.insert(x.start, crate::labels::labels(src, x.start, &d.book)); }
        let pay = (if w.winner == x.s as i64 { 1.0 } else { 0.0 }) - x.q / 1000.0;
        real += x.real_sh;
        real_pnl += x.real_sh * pay;
        let dk = crate::packs::ymd((x.start / 86400) as i64)[5..].to_string();
        if !days.iter().any(|d| d.0 == dk) { days.push((dk.clone(), 0.0, 0.0)); }
        let di = days.iter().position(|d| d.0 == dk).unwrap();
        days[di].1 += x.real_sh * pay;
        let input = Input { start: x.start, winner: w.winner, fee: w.fee, tick0: w.tick0, trades: &w.trades, book: &d.book, px: Some(&d.px), cl: &d.cl, labels: labels[&x.start].as_deref() };
        let end = if x.end == f64::INFINITY { f64::INFINITY } else { x.end - x.start as f64 * 1000.0 };
        let o = sim_window(&input, base, cfg, None, Some(Script { s: x.s, q: x.q / 1000.0, r: x.size, j: x.j, end }));
        let sh = engine_strategy::model::jmin(x.size, o.script_fills.iter().fold(0.0, |t, f| t + f.1));
        pred += sh;
        pnl += sh * pay;
        days[di].2 += sh * pay;
        if (sh - x.real_sh).abs() < 0.02 { exact += 1; }
        if let Some(e) = export.as_mut() {
            let h = |v: f64| format!("{:016x}", v.to_bits());
            writeln!(e, "{}", serde_json::json!({ "start": x.start, "j": x.j, "s": x.s, "q": x.q, "size": h(x.size), "qa0": h(x.qa0), "arrive": h(x.arrive), "end": h(x.end),
                "realSh": h(x.real_sh), "sh": h(sh), "fills": o.script_fills.len(), "winner": w.winner })).map_err(|e| e.to_string())?;
        }
    }
    lines.push(format!("simWindow on {} twins after the open with a known end (real {} sh, PnL {} $):", tw.len(), to_fixed(real, 0), to_fixed(real_pnl, 0)));
    lines.push(format!("  {:<10} predicted {} sh, ratio {}, orders exact {}, PnL {} $", "exact", to_fixed(pred, 0), to_fixed(pred / real, 3), to_fixed(exact as f64 / tw.len() as f64, 3), to_fixed(pnl, 0)));
    for (d, r, e) in &days { lines.push(format!("  {d} real {} $ | exact {} $", to_fixed(*r, 0), to_fixed(*e, 0))); }
    let h = |v: f64| format!("{:016x}", v.to_bits());
    lines.push(format!("bits: pred {} pnl {} real {} realPnl {} exact {exact}", h(pred), h(pnl), h(real), h(real_pnl)));
    Ok(Report { lines })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn to_fixed_is_javascripts() {
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(-6556.6, 0), "-6557");
        assert_eq!(to_fixed(1.0444, 3), "1.044");
        assert_eq!(to_fixed(9.9996, 3), "10.000");
        assert_eq!(to_fixed(-0.4, 0), "-0");
    }
}
