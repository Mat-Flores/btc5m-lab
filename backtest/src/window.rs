//! One window replayed from the packs: `simWindow` (engine/maker-bt/sim.mjs) rebuilt from the engine's maker strategy
//! (`engine_strategy::maker::Maker`, decisions and fair value) and the simulated exchange (`venue::WindowVenue`),
//! driven by the same merged loop: per instant a whole second first, then a Chainlink print, an OKX price, a book
//! record; on a book record the exchange fills before the strategy decides (sim.mjs:446-448).
use crate::venue::{Bound, VenueCfg, WindowVenue};
use engine_strategy::maker::{Act, Fee, Maker, Params, Rec};

const COLS: usize = 5;

/// validate.mjs `script`: one order live on side `s` at `q` for `r` shares from book record `j` (its server time;
/// queue ahead = its level just before) until server time `end` (ms from the open).
#[derive(Clone, Copy, Debug)]
pub struct Script { pub s: usize, pub q: f64, pub r: f64, pub j: usize, pub end: f64 }

/// What `simWindow` gives: the window record (`why`: "no-spot", "no-fills", "vol" or None) and, for a script, the
/// fills of its order [server time, shares].
pub struct Out { pub rec: Rec, pub script_fills: Vec<(f64, f64)> }

pub struct Input<'a> {
    pub start: u64,
    /// 0 Up, 1 Down, anything else: no payout.
    pub winner: i64,
    pub fee: Fee,
    pub tick0: f64,
    /// Public taker rows (A.trades.f32), for `prob: 'vwap'`.
    pub trades: &'a [f32],
    pub book: &'a [f32],
    /// Spot of the fair value [t, p] (None: the requested series is not on disk).
    pub px: Option<&'a [f64]>,
    pub cl: &'a [f64],
    /// Traded volume of each book record (`exact`).
    pub labels: Option<&'a [f32]>,
}

fn empty(p: &Params, why: &'static str) -> Out {
    let mut m = Maker::new(p.clone(), Fee::default(), 0.01);
    m.finish(None);
    m.rec.why = Some(why);
    Out { rec: m.rec, script_fills: vec![] }
}

/// `simWindow(w, d, strat, opts, budget)`. `budget`: $ per side (None with `shares`).
pub fn sim_window(w: &Input, p: &Params, cfg: &VenueCfg, budget: Option<f64>, script: Option<Script>) -> Out {
    // a scripted window makes no decision (`decide` returns at once: sim.mjs:312)
    let scripted;
    let p = match script {
        Some(sc) => { scripted = Params { script: Some(engine_strategy::maker::Script { s: sc.s, q: sc.q, r: sc.r, j: sc.j as f64, end: Some(sc.end) }), ..p.clone() }; &scripted }
        None => p,
    };
    let Some(px) = w.px else { return empty(p, "no-spot") };
    if cfg.fill_bound == Bound::Exact && w.labels.is_none() { return empty(p, "no-fills"); }
    let (ev, cl) = (w.book, w.cl);
    let (n_ev, n_px, n_cl) = (ev.len() / COLS, px.len() / 2, cl.len() / 3);
    let mut m = Maker::new(p.clone(), w.fee, w.tick0);
    if p.prob == "vwap" { m.set_trades(&w.trades.iter().map(|&x| x as f64).collect::<Vec<_>>()); }
    let mut v = WindowVenue::new(cfg, w.start, p.end_ms, w.labels);
    let t_end = p.end_ms + p.loop_after_end_ms;
    // nextSec = ceil(min(first book, first price, first print) / 1000) × 1000, an empty series counting as 0
    let first = |a: f64, n: usize| if n > 0 { a } else { 0.0 };
    let (b0, p0, c0) = (ev.first().map_or(0.0, |&x| x as f64), px.first().copied().unwrap_or(0.0), cl.first().copied().unwrap_or(0.0));
    m.next_sec = (first(b0, n_ev).min(first(p0, n_px)).min(first(c0, n_cl)) / 1000.0).ceil() * 1000.0;
    let mut start = || Some(budget);
    let mut acts: Vec<Act> = vec![];
    // venue order → maker order (None: the script's)
    let mut of: Vec<Option<usize>> = vec![];
    let mut fills: Vec<(usize, f64, f64)> = vec![];
    let mut script_id = None;
    let mut script_fills = vec![];
    let send = |acts: &mut Vec<Act>, v: &mut WindowVenue, of: &mut Vec<Option<usize>>| {
        for a in acts.drain(..) {
            match a {
                Act::Place { s, id, q, r, qa, t } => { let k = v.place(s, q, r, qa, t); of.resize(k + 1, None); of[k] = Some(id); }
                Act::Cancel { s, t, .. } => v.cancel(s, t),
                Act::Exit { s, shares, limit, t, .. } => { v.exit(s, shares, limit, t); }
            }
        }
    };
    let (mut jb, mut jp, mut jc) = (0usize, 0usize, 0usize);
    loop {
        let tb = if jb < n_ev { ev[COLS * jb] as f64 } else { f64::INFINITY };
        let tp = if jp < n_px { px[2 * jp] } else { f64::INFINITY };
        let tc = if jc < n_cl { cl[3 * jc] } else { f64::INFINITY };
        let t = tb.min(tp).min(tc);
        // taker exits that arrived before this input, on the book as it was then
        while let Some(x) = v.next_exit().filter(|x| x.at < t.min(t_end)) {
            m.tick(x.at, &mut acts, &mut start);
            send(&mut acts, &mut v, &mut of);
            let (sold, got) = v.take_exit(x.id);
            m.exit_done(x.s, sold, got, x.at, &mut acts);
            send(&mut acts, &mut v, &mut of);
        }
        if t > t_end || m.stopped { break; }
        if t == tc {
            m.cl(t, cl[3 * jc + 1], cl[3 * jc + 2], &mut acts, &mut start);
            jc += 1;
        } else if t == tp {
            m.px(t, px[2 * jp + 1], &mut acts, &mut start);
            jp += 1;
        } else {
            let (ts, typ, price, size) = (ev[COLS * jb + 1] as f64, ev[COLS * jb + 2] as u8, ev[COLS * jb + 3] as f64, ev[COLS * jb + 4] as f64);
            if let Some(sc) = script.filter(|sc| sc.j == jb) {
                let id = v.script(sc.s, sc.q, sc.r, ts, sc.end);
                of.resize(id + 1, None);
                script_id = Some(id);
            }
            let step = m.book(t, typ, price, size, &mut acts, &mut start);
            send(&mut acts, &mut v, &mut of);
            if m.stopped { break; }
            v.record(jb, t, ts, typ, price, size, &mut fills);
            for (k, sh, at) in fills.drain(..) {
                match of[k] { Some(id) => m.filled(id, sh, at, cfg.rebates), None => if Some(k) == script_id { script_fills.push((at, sh)); } }
            }
            m.after_book(step, &mut acts, &mut start);
            jb += 1;
        }
        send(&mut acts, &mut v, &mut of);
        if m.stopped { break; }
    }
    if !m.stopped {
        while let Some(x) = v.next_exit().filter(|x| x.at <= t_end) {
            m.tick(x.at, &mut acts, &mut start);
            send(&mut acts, &mut v, &mut of);
            let (sold, got) = v.take_exit(x.id);
            m.exit_done(x.s, sold, got, x.at, &mut acts);
            send(&mut acts, &mut v, &mut of);
        }
        m.tick(t_end, &mut acts, &mut start);
        send(&mut acts, &mut v, &mut of);
    }
    m.finish(if w.winner == 0 || w.winner == 1 { Some(w.winner as usize) } else { None });
    Out { rec: m.rec, script_fills }
}
