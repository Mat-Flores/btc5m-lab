//! A strategy over the recorded windows of the packs with compounded capital: `makeRun`/`runMany` of
//! engine/maker-bt/run.mjs (btc5m-lag-sim), ported operation for operation. Stake = f × FREE capital at the window's
//! open, half per side; spent capital stays locked until the end; the payout comes back `claim_s` after the end
//! (0 by default [à vérifier]); unspent stake and exit proceeds come back at the end. Settlement by the official
//! winner only (no result: the window is not played).
use crate::labels::labels;
use crate::packs::{Source, Win};
use crate::venue::{Bound, VenueCfg};
use crate::window::{sim_window, Input};
use engine_strategy::maker::Params;
use std::collections::HashMap;

/// One window of one run (the record `makeRun.step` keeps, with `summarizeWindow`'s fields when played).
#[derive(Clone, Debug)]
pub struct WinRec {
    pub start: u64, pub why: Option<&'static str>, pub played: bool, pub eq_before: f64, pub eq_after: f64, pub pnl: f64,
    /// spent, payout, proceeds, fees, shares [Up, Down], fills [Up, Down], driftS, driftN (None: not simulated).
    pub sum: Option<(f64, f64, f64, f64, [f64; 2], [usize; 2], f64, f64)>,
}

pub struct Run { pub p: Params, pub v: VenueCfg, pub capital0: f64, pub claim_s: f64, free: f64, day: Option<f64>, day_start: f64, pending: Vec<(f64, f64)>, pub out: Vec<WinRec> }

impl Run {
    pub fn new(p: Params, v: VenueCfg, capital0: f64, claim_s: f64) -> Run { Run { p, v, capital0, claim_s, free: capital0, day: None, day_start: 0.0, pending: vec![], out: vec![] } }

    fn eq(&self) -> f64 { self.free + self.pending.iter().fold(0.0, |x, p| x + p.1) }

    /// `makeRun.step(w, data)`: `input` holds the window's data (its spot series chosen by the strategy's `spot`).
    pub fn step(&mut self, w: &Win, input: &Input, end_s: f64) {
        let mut j = self.pending.len();
        while j > 0 { j -= 1; if self.pending[j].0 <= w.start as f64 { self.free += self.pending[j].1; self.pending.remove(j); } }
        let eq_before = self.eq();
        let d = (w.start as f64 / 86400.0).floor();
        if self.day != Some(d) { self.day = Some(d); self.day_start = eq_before; }
        let p = &self.p;
        let budget = if p.shares.is_some() { None } else { Some((p.f * self.free) / 2.0) };
        let why = if w.winner != 0 && w.winner != 1 { Some("no-result") }
            else if p.daily_stop.is_some_and(|ds| eq_before - self.day_start <= -ds * self.day_start) { Some("daily-stop") }
            else if p.min_prev_volume > 0.0 && !(w.prev_volume >= p.min_prev_volume) { Some("volume") }
            else if budget.is_some_and(|b| b < p.min_size * 0.01) { Some("stake") }
            else { None };
        let mut r = WinRec { start: w.start, why, played: false, eq_before, eq_after: 0.0, pnl: 0.0, sum: None };
        if why.is_none() {
            let mut o = sim_window(input, &self.p, &self.v, budget, None);
            let rec = &mut o.rec;
            r.why = rec.why;
            self.free += -rec.spent + rec.proceeds - rec.fees + rec.rebates;
            if rec.payout > 0.0 { if self.claim_s > 0.0 { self.pending.push((w.start as f64 + end_s + self.claim_s, rec.payout)); } else { self.free += rec.payout; } }
            let sh = |s: usize| rec.fills[s].iter().fold(0.0, |a, f| a + f[1]);
            r.sum = Some((rec.spent, rec.payout, rec.proceeds, rec.fees, [sh(0), sh(1)], [rec.fills[0].len(), rec.fills[1].len()], rec.drift[0], rec.drift[1]));
            r.played = rec.posted > 0;
            r.pnl = rec.pnl;
        }
        r.eq_after = self.eq();
        self.out.push(r);
    }
}

/// `runMany(wins, configs)`: every run over the windows in one pass, each window's data read once; the spot series
/// of a window (`spot` of the strategy: okx | spot | perp | perp+coinbase) and its trade labels (`exact`) are computed
/// once per window and shared.
pub fn run_many(src: &mut Source, wins: &[Win], runs: &mut [Run], mix: (f64, f64), end_s: f64, log: &mut dyn FnMut(usize, u64)) -> Result<(), String> {
    for (k, w) in wins.iter().enumerate() {
        let d = src.read_b(w.start)?;
        let mut spots: HashMap<String, Option<Vec<f64>>> = HashMap::new();
        let mut lab: Option<Option<Vec<f32>>> = None;
        for r in runs.iter_mut() {
            let key = format!("{}|{}", r.p.spot, r.p.spot_latency);
            if !spots.contains_key(&key) {
                let px = match r.p.spot.as_str() {
                    "" | "okx" => Some(d.px.clone()),
                    "perp+coinbase" => src.mix_px(w.start, mix.0, mix.1),
                    m => src.binance_px(w.start, m, r.p.spot_latency),
                };
                spots.insert(key.clone(), px);
            }
            if r.v.fill_bound == Bound::Exact && lab.is_none() { lab = Some(labels(src, w.start, &d.book)); }
            let px = spots[&key].as_deref();
            let labels = lab.as_ref().and_then(|l| l.as_deref());
            r.step(w, &Input { start: w.start, winner: w.winner, fee: w.fee, tick0: w.tick0, trades: &w.trades, book: &d.book, px, cl: &d.cl, labels }, end_s);
        }
        log(k, w.start);
    }
    Ok(())
}
