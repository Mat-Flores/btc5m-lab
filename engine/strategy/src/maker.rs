//! Maker strategy of the maker backtest (`engine/maker-bt/sim.mjs` of btc5m-lag-sim): post-only bids on Up and Down
//! of one window, decided at every event; fair value of the reference engine. Ported operation for operation (same
//! f64 order of evaluation) from `simWindow`: its parameters (`DEFAULTS`), the Up-book replica (`Book`), the fair value
//! (`fairAt` and what feeds it) and `decide`. The simulated exchange (fills, queue, retention, look-ahead book of the
//! exits) is not here: `decide` emits acts, the exchange answers with fills.
//!
//! One `Maker` per window, driven like `simWindow`'s loop, times in ms from the open (END = 300 000):
//! `second(t)` at each whole second, `cl`, `px`, `book` per record; ties in that order (second, cl, px, book).
//! Exits (tp/sl) differ by design: here a FAK sell is emitted and the position moves when its fill comes back
//! (`exit_done`), where `sim.mjs` books the result at the decision with a look-ahead book (decision D6).
use crate::model::{fair_up, jmax, jmin, jround, Vol};
use serde::{Deserialize, Serialize};

const END: f64 = 300e3;
const T_END: f64 = END + 2500.0;
const K_TAPE: usize = 361;

fn nan() -> f64 { f64::NAN }
fn infinity() -> f64 { f64::INFINITY }

/// {J, S}: a spot move of more than J $ over S s.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct Move { #[serde(rename = "J")] pub j: f64, #[serde(rename = "S")] pub s: f64 }
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct CancelLoser { #[serde(rename = "X")] pub x: f64, #[serde(rename = "Y")] pub y: f64 }
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct NoKnife { #[serde(rename = "T")] pub t: f64, #[serde(rename = "M")] pub m: f64 }
/// validate.mjs: one order live on side `s` at `q` for `r` shares from book record `j` until server time `end`.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Script { pub s: usize, pub q: f64, pub r: f64, pub j: f64, #[serde(default)] pub end: Option<f64> }

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Price { pub mode: String, pub offset: f64, pub fixed: f64 }
impl Default for Price { fn default() -> Self { Price { mode: "offset".into(), offset: 0.03, fixed: 0.45 } } }

/// `sim.mjs` DEFAULTS (lines 42-77), same names and values; then the constants of `sim.mjs` it reads (`minSize`,
/// `silenceMs`) and the exchange values `reserveInFlight` needs (`oneWay`, `delay`), which must be the exchange's.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Params {
    pub name: String,
    /// Labels of a candidate in a batch file (`id`, `desc`); read and ignored.
    pub id: Option<String>,
    pub desc: Option<String>,
    pub f: f64,
    pub shares: Option<f64>,
    pub prob: String,
    pub sigma_mult: f64,
    pub price: Price,
    pub requote: String,
    pub pull: Option<Move>,
    pub pull_side: Option<Move>,
    /// Which spot feeds the fair value: chosen by the source configuration, read and kept here for the record.
    pub spot: String,
    pub spot_latency: f64,
    pub cancel_loser: Option<CancelLoser>,
    pub tp: Option<f64>,
    pub sl: Option<f64>,
    pub exit_slip: f64,
    pub merge_hold: bool,
    pub entry_from: f64,
    pub quote_end: f64,
    pub vol_min: f64,
    pub vol_max: Option<f64>,
    pub min_prev_volume: f64,
    pub daily_stop: Option<f64>,
    pub liq_cap: bool,
    pub reserve_in_flight: bool,
    pub join: Option<f64>,
    pub side_prob: Option<[f64; 2]>,
    pub max_diverge: Option<f64>,
    pub no_knife: Option<NoKnife>,
    pub max_imbalance: Option<f64>,
    pub pair_edge: Option<f64>,
    pub order_shares: Option<f64>,
    pub life: Option<f64>,
    /// validate.mjs: one scripted order, no decision (placed by the test shell).
    pub script: Option<Script>,
    /// `MIN_SIZE` (orderMinSize), shares.
    pub min_size: f64,
    /// `SILENCE_MS`: no decision while the book is silent for longer, ms.
    pub silence_ms: f64,
    /// `ONE_WAY` (ms) and `opts.delay` (s) of the exchange: how long a replaced order may still fill.
    pub one_way: f64,
    pub delay: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params { name: String::new(), id: None, desc: None, f: 0.05, shares: None, prob: "fair".into(), sigma_mult: 1.5, price: Price::default(), requote: "follow".into(),
            pull: None, pull_side: None, spot: "okx".into(), spot_latency: 120.0, cancel_loser: None, tp: None, sl: None, exit_slip: 0.05, merge_hold: true,
            entry_from: 10.0, quote_end: 10.0, vol_min: 0.0, vol_max: None, min_prev_volume: 0.0, daily_stop: None, liq_cap: true, reserve_in_flight: false,
            join: None, side_prob: None, max_diverge: None, no_knife: None, max_imbalance: None, pair_edge: None, order_shares: None, life: None, script: None,
            min_size: 5.0, silence_ms: 3000.0, one_way: 12.2, delay: 0.15 }
    }
}

/// The window's fee schedule as `sim.mjs` reads it (`w.fee ?? CRYPTO_FEE`).
#[derive(Clone, Copy, Debug)]
pub struct Fee { pub rate: f64, pub exponent: f64, pub rebate_rate: f64 }
impl Default for Fee { fn default() -> Self { Fee { rate: 0.07, exponent: 1.0, rebate_rate: 0.0 } } }

/// `matchFee` (src/core/fees.js): taker fee of one match, rounded to 5 decimals.
pub fn match_fee(shares: f64, price: f64, f: &Fee) -> f64 {
    if f.rate == 0.0 || f.rate.is_nan() { return 0.0; }
    let raw = shares * f.rate * (price * (1.0 - price)).powf(f.exponent);
    jround(raw * 1e5 + 1e-9) / 1e5
}

pub fn floor_tick(p: f64, tick: f64) -> f64 { jround((p / tick + 1e-6).floor() * tick * 1e6) / 1e6 }
pub fn floor2(x: f64) -> f64 { (x * 100.0 + 1e-9).floor() / 100.0 }

/// Up-token book replica, prices in thousandths (`Book` of sim.mjs); Down bids at x are Up asks at 1 - x.
#[derive(Clone)]
pub struct Book { bid: Vec<f64>, ask: Vec<f64>, pub bb: i64, pub ba: i64, pub fine: bool }
impl Default for Book { fn default() -> Self { Book { bid: vec![0.0; 1001], ask: vec![0.0; 1001], bb: -1, ba: 1001, fine: false } } }
impl Book {
    pub fn clear(&mut self) { self.bid.fill(0.0); self.ask.fill(0.0); self.bb = -1; self.ba = 1001; }
    /// type 1 bid, 2 ask; returns the previous size.
    pub fn set(&mut self, typ: u8, price: f64, size: f64) -> f64 {
        let i = jround(price * 1000.0);
        if !(0.0..=1000.0).contains(&i) { return 0.0; }
        let i = i as i64;
        if i % 10 != 0 { self.fine = true; }
        let arr = if typ == 1 { &mut self.bid } else { &mut self.ask };
        let old = arr[i as usize];
        arr[i as usize] = size;
        if typ == 1 {
            if size > 0.0 && i > self.bb { self.bb = i; }
            else if size <= 0.0 && i == self.bb { while self.bb >= 0 && !(self.bid[self.bb as usize] > 0.0) { self.bb -= 1; } }
        } else if size > 0.0 && i < self.ba { self.ba = i; }
        else if size <= 0.0 && i == self.ba { while self.ba <= 1000 && !(self.ask[self.ba as usize] > 0.0) { self.ba += 1; } }
        old
    }
    fn at(v: &[f64], i: i64) -> f64 { if (0..=1000).contains(&i) { v[i as usize] } else { f64::NAN } }
    pub fn bid_size(&self, s: usize, i: i64) -> f64 { if s == 0 { Self::at(&self.bid, i) } else { Self::at(&self.ask, 1000 - i) } }
    pub fn best_bid(&self, s: usize) -> i64 { if s == 0 { self.bb } else { 1000 - self.ba } }
    pub fn best_ask(&self, s: usize) -> i64 { if s == 0 { self.ba } else { 1000 - self.bb } }
    pub fn ask_size(&self, s: usize, i: i64) -> f64 { if s == 0 { Self::at(&self.ask, i) } else { Self::at(&self.bid, 1000 - i) } }
}

/// Up VWAP of the public taker trades of the last 5 s (`tapePath(...).vwap`), per second -60..300.
/// rows: [t (s from the open), hit side, price on that side, size] × n, in time order.
pub fn tape_vwap(rows: &[f64]) -> Vec<f64> {
    let n = rows.len() / 4;
    let o = 60i64;
    let mut vwap = vec![f64::NAN; K_TAPE];
    let (mut lo, mut last) = (0usize, f64::NAN);
    for (k, slot) in vwap.iter_mut().enumerate() {
        let sec = (k as i64 - o) as f64;
        while lo < n && rows[4 * lo] < sec - 5.0 { lo += 1; }
        let (mut pv, mut v) = (0.0, 0.0);
        let mut j = lo;
        while j < n && rows[4 * j] <= sec - 1.0 {
            let up = if rows[4 * j + 1] == 0.0 { rows[4 * j + 2] } else { 1.0 - rows[4 * j + 2] };
            pv += up * rows[4 * j + 3];
            v += rows[4 * j + 3];
            j += 1;
        }
        if v > 0.0 { last = pv / v; }
        *slot = last;
    }
    vwap
}

/// What `decide` does that the exchange must carry out. `id`: the order's index in `Maker::orders`.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// A new bid on side `s` (0 Up, 1 Down) at `q` for `r` shares, `qa` = size of level q in the book at the decision.
    Place { s: usize, id: usize, q: f64, r: f64, qa: f64, t: f64 },
    /// The order stops (replaced or withdrawn).
    Cancel { s: usize, id: usize, t: f64 },
    /// A taker exit: FAK sell of `shares` on side `s`, limit `limit`.
    Exit { s: usize, shares: f64, limit: f64, why: &'static str, mark: f64, t: f64 },
}

/// One book record as seen before and after it: `before` the best Up bid and ask, `old` the level's previous size.
#[derive(Clone, Copy, Debug)]
pub struct BookStep { pub t: f64, pub before: (i64, i64), pub old: f64, pub silent_before: bool, live: bool }

#[derive(Clone, Debug)]
pub struct Order { pub s: usize, pub q: f64, pub r: f64, pub qa: f64, pub at: f64 }

#[derive(Clone, Copy, Debug, Default)]
pub struct Pos { pub n: f64, pub cost: f64 }

/// The window record of `simWindow` (the parts its strategy produces).
#[derive(Clone, Debug)]
pub struct Rec {
    pub spent: f64, pub proceeds: f64, pub fees: f64, pub rebates: f64, pub payout: f64,
    pub capped: [bool; 2], pub why: Option<&'static str>,
    /// [t, shares, price, fair of the side] per fill.
    pub fills: [Vec<[f64; 4]>; 2],
    pub posted: u64,
    /// [price, queue ahead, ticks under the best bid] per post.
    pub joins: Vec<[f64; 3]>,
    pub cancel_loser_at: Option<f64>, pub loser_filled_before: bool, pub pulled: u64, pub silent_skips: u64, pub sigma0: f64,
    /// [t, why, side, want, sold, px, mark] per exit.
    pub exits: Vec<(f64, &'static str, usize, f64, f64, f64, f64)>,
    pub drift: [f64; 2], pub pos_n: [f64; 2], pub pnl: f64,
}

pub struct Maker {
    pub p: Params,
    fee: Fee,
    tick0: f64,
    rebate: f64,
    reb_exp: f64,
    pub rec: Rec,
    pub pos: [Pos; 2],
    pub orders: Vec<Order>,
    cur: [Option<usize>; 2],
    frozen: [Option<f64>; 2],
    side_done: [bool; 2],
    spent_side: [f64; 2],
    cap: [f64; 2],
    last_exit_sec: [f64; 2],
    pulled_until: f64,
    inflight: [Vec<(f64, usize)>; 2],
    // fair value
    vol: Vol,
    okx: f64,
    basis: f64,
    strike: f64,
    px_hist: Vec<(f64, f64)>,
    cl_buf: Vec<(f64, f64)>,
    cl_after_open: bool,
    cl_before_twap: bool,
    pub fair_now: f64,
    pub fair_sec: Vec<f64>,
    tape: Option<Vec<f64>>,
    // book
    pub book: Book,
    last_book_t: f64,
    snap_t: f64,
    // loop
    pub next_sec: f64,
    /// The window stopped (volatility filter) or its loop is over (past END + 2.5 s).
    pub stopped: bool,
    pub over: bool,
    /// None until the first decision past `entryFrom`; then whether the window is played and its budget.
    played: Option<Option<Option<f64>>>,
}

/// Asked once per window, at its first decision past `entryFrom` (the latest instant before any order): None, the
/// window is not played; Some(budget) ($ per side, None with `shares`).
pub type Start<'a> = &'a mut dyn FnMut() -> Option<Option<f64>>;

impl Maker {
    pub fn new(p: Params, fee: Fee, tick0: f64) -> Maker {
        let rebate = fee.rebate_rate * fee.rate;
        Maker {
            p, fee, tick0, rebate, reb_exp: fee.exponent,
            rec: Rec { spent: 0.0, proceeds: 0.0, fees: 0.0, rebates: 0.0, payout: 0.0, capped: [false; 2], why: None, fills: [vec![], vec![]], posted: 0, joins: vec![],
                cancel_loser_at: None, loser_filled_before: false, pulled: 0, silent_skips: 0, sigma0: f64::NAN, exits: vec![], drift: [0.0; 2], pos_n: [0.0; 2], pnl: 0.0 },
            pos: [Pos::default(); 2], orders: vec![], cur: [None; 2], frozen: [None; 2], side_done: [false; 2], spent_side: [0.0; 2], cap: [infinity(); 2],
            last_exit_sec: [-1.0; 2], pulled_until: f64::NEG_INFINITY, inflight: [vec![], vec![]],
            vol: Vol::new(), okx: nan(), basis: nan(), strike: nan(), px_hist: vec![], cl_buf: vec![], cl_after_open: false, cl_before_twap: false,
            fair_now: nan(), fair_sec: vec![f64::NAN; 312], tape: None,
            book: Book::default(), last_book_t: f64::NEG_INFINITY, snap_t: nan(), next_sec: nan(), stopped: false, over: false, played: None,
        }
    }

    /// Public taker rows of the window (`w.trades`), for `prob: 'vwap'`.
    pub fn set_trades(&mut self, rows: &[f64]) { self.tape = Some(tape_vwap(rows)); }

    fn tape_at(&self, sec: f64) -> f64 {
        let Some(v) = &self.tape else { return f64::NAN };
        let i = (sec.floor() + 60.0).clamp(0.0, (K_TAPE - 1) as f64);
        v[i as usize]
    }

    // ---- fair value, as the engine
    fn px_at(&self, t: f64) -> f64 { let i = self.px_hist.partition_point(|x| x.0 <= t); if i == 0 { f64::NAN } else { self.px_hist[i - 1].1 } }
    fn spot(&self) -> f64 { self.okx - self.basis }
    fn twap_known(&self, mut from: f64, to: f64) -> (f64, f64, f64, f64) {
        let (mut sum, mut last_ts, mut v) = (0.0, from, f64::NAN);
        for &(ts, p) in &self.cl_buf {
            if ts <= from { v = p; continue; }
            if ts > to { break; }
            if v.is_finite() { sum += v * (ts - last_ts) / 1000.0; } else { from = ts; }
            last_ts = ts;
            v = p;
        }
        (sum, (last_ts - from) / 1000.0, last_ts, v)
    }
    fn fair_at(&self, t: f64) -> f64 {
        let sp = self.spot();
        if !sp.is_finite() || !self.strike.is_finite() { return f64::NAN; }
        let sigma = self.vol.sigma() * self.p.sigma_mult;
        if t < END - 60e3 { return fair_up(sp, self.strike, (END - t) / 1000.0, sigma, 60.0, 0.0, 0.0); }
        let (sum, secs, last_ts, _) = self.twap_known(END - 60e3, jmin(t, END));
        fair_up(sp, self.strike, jmax(0.0, (END - last_ts) / 1000.0), sigma, 60.0, sum, secs)
    }

    /// Whole seconds due before an input at `t` (a second comes before a record of the same instant).
    fn seconds(&mut self, t: f64, out: &mut Vec<Act>, start: Start) {
        if self.next_sec.is_nan() { self.next_sec = (t / 1000.0).ceil() * 1000.0; }
        while !self.over && self.next_sec <= t {
            if self.next_sec > T_END { self.over = true; break; }
            self.second(out, start);
        }
        if t > T_END { self.over = true; }
    }

    fn second(&mut self, out: &mut Vec<Act>, start: Start) {
        let ns = self.next_sec;
        let sp = self.spot();
        if sp.is_finite() { self.vol.push(sp); }
        if ns == 0.0 { self.rec.sigma0 = self.vol.sigma(); }
        if (0.0..=310e3).contains(&ns) { self.fair_sec[(ns / 1000.0) as usize] = self.fair_now; }
        if ns == 0.0 && (self.vol.sigma() < self.p.vol_min || self.p.vol_max.is_some_and(|m| self.vol.sigma() > m)) { self.rec.why = Some("vol"); self.stopped = true; self.over = true; return; }
        if ns >= 0.0 { self.decide(ns, true, out, start); }
        self.next_sec += 1000.0;
    }

    fn live(&self) -> bool { !self.stopped && !self.over }

    /// Time passes (the test shell's tick, or any input): runs the seconds due by `t`.
    pub fn tick(&mut self, t: f64, out: &mut Vec<Act>, start: Start) { if self.live() { self.seconds(t, out, start); } }

    /// A Chainlink print received at `t`, observed at `ts`.
    pub fn cl(&mut self, t: f64, ts: f64, p: f64, out: &mut Vec<Act>, start: Start) {
        self.seconds(t, out, start);
        if !self.live() { return; }
        self.cl_buf.push((ts, p));
        self.cl_after_open |= ts >= 0.0;
        self.cl_before_twap |= ts <= -60e3;
        let then = self.px_at(ts);
        // basis to Chainlink, as the engine: OKX at the print's observation time, untouched when OKX is stale (> 5 s)
        if then.is_finite() && t - self.px_hist.last().map_or(f64::NEG_INFINITY, |x| x.0) <= 5000.0 { let b = then - p; self.basis = if self.basis.is_finite() { self.basis + (b - self.basis) / 30.0 } else { b }; }
        if !self.strike.is_finite() && self.cl_after_open && self.cl_before_twap {
            let (sum, _, last_ts, v) = self.twap_known(-60e3, 0.0);
            self.strike = (sum + v * (0.0 - last_ts) / 1000.0) / 60.0;
        }
        self.fair_now = self.fair_at(t);
        if t >= 0.0 { self.decide(t, false, out, start); }
    }

    /// A spot price received at `t`.
    pub fn px(&mut self, t: f64, p: f64, out: &mut Vec<Act>, start: Start) {
        self.seconds(t, out, start);
        if !self.live() { return; }
        self.okx = p;
        self.px_hist.push((t, p));
        self.fair_now = self.fair_at(t);
        if t >= 0.0 { self.decide(t, false, out, start); }
    }

    /// One Up-book record received at `t`: type 0 a snapshot begins (its levels follow, same `t`), 1 bid, 2 ask.
    /// Updates the book only; the exchange then fills what this change fills, and `after_book` decides.
    pub fn book(&mut self, t: f64, typ: u8, price: f64, size: f64, out: &mut Vec<Act>, start: Start) -> BookStep {
        self.seconds(t, out, start);
        let before = (self.book.best_bid(0), self.book.best_ask(0));
        if !self.live() { return BookStep { t, before, old: 0.0, silent_before: false, live: false }; }
        let silent_before = t - self.last_book_t > self.p.silence_ms;
        self.last_book_t = t;
        let old = if typ == 0 { self.book.clear(); self.snap_t = t; 0.0 } else { self.book.set(typ, price, size) };
        BookStep { t, before, old, silent_before, live: true }
    }

    /// The decision after a book record (once the exchange has filled what it fills).
    pub fn after_book(&mut self, b: BookStep, out: &mut Vec<Act>, start: Start) {
        // no decision while a snapshot is being re-listed (half a book)
        if b.live && self.live() && b.t >= 0.0 && b.t != self.snap_t && (self.book.best_bid(0) != b.before.0 || self.book.best_ask(0) != b.before.1) { self.decide(b.t, false, out, start); }
    }

    /// Time of the last snapshot (records of a snapshot share it).
    pub fn snap_t(&self) -> f64 { self.snap_t }

    /// A fill of order `id` (`doFill`), at `t`; `rebate`: the exchange pays the maker rebate.
    pub fn filled(&mut self, id: usize, shares: f64, t: f64, rebate: bool) {
        let s = self.orders[id].s;
        let o = &mut self.orders[id];
        let shares = jmin(shares, o.r);
        if !(shares > 1e-9) { return; }
        o.r -= shares;
        let q = o.q;
        self.pos[s].n += shares; self.pos[s].cost += shares * q; self.rec.spent += shares * q; self.spent_side[s] += shares * q;
        if rebate { self.rec.rebates += self.rebate * shares * (q * (1.0 - q)).powf(self.reb_exp); }
        self.rec.fills[s].push([t, shares, q, if s == 0 { self.fair_now } else { 1.0 - self.fair_now }]);
    }

    /// What an exit sold (`sold` shares for `got` $), at `t`.
    pub fn exit_done(&mut self, s: usize, sold: f64, got: f64, t: f64, out: &mut Vec<Act>) {
        let sold = floor2(sold);
        let px_s = if sold > 0.0 { got / sold } else { f64::NAN };
        if let Some(e) = self.rec.exits.iter_mut().rev().find(|e| e.2 == s && e.4.is_nan()) { e.4 = sold; e.5 = px_s; }
        if sold > 0.0 {
            let avg = self.pos[s].cost / self.pos[s].n;
            self.pos[s].n -= sold; self.pos[s].cost -= avg * sold; self.rec.proceeds += sold * px_s; self.rec.fees += match_fee(sold, px_s, &self.fee);
            self.side_done[s] = true;
            if self.cur[s].is_some() { self.post(s, None, 0.0, t, out); }
        }
    }

    fn post(&mut self, s: usize, q: Option<f64>, r: f64, t: f64, out: &mut Vec<Act>) -> Option<usize> {
        if let Some(c) = self.cur[s] {
            if self.orders[c].r > 0.0 { self.inflight[s].push((t + self.p.one_way + 1000.0 * self.p.delay + 1000.0, c)); }
            out.push(Act::Cancel { s, id: c, t });
        }
        let id = q.map(|q| { self.orders.push(Order { s, q, r, qa: 0.0, at: t }); self.rec.posted += 1; self.orders.len() - 1 });
        self.cur[s] = id;
        id
    }

    fn decide(&mut self, t: f64, every_sec: bool, out: &mut Vec<Act>, start: Start) {
        if self.p.script.is_some() { return; }
        if t - self.last_book_t > self.p.silence_ms { if every_sec { self.rec.silent_skips += 1; } return; }
        let p_up = match self.p.prob.as_str() {
            "vwap" => self.tape_at(t / 1000.0),
            "mid" => if self.book.bb >= 0 && self.book.ba <= 1000 { (self.book.bb + self.book.ba) as f64 / 2000.0 } else { f64::NAN },
            _ => self.fair_now,
        };
        if !p_up.is_finite() { return; }
        let tick = if self.book.fine { 0.001 } else { self.tick0 };
        let sec = (t / 1000.0).floor();
        // exits (taker, FAK), at most one attempt per side and second
        if self.p.tp.is_some() || self.p.sl.is_some() {
            let mut paired = 0.0;
            if self.p.merge_hold && self.pos[0].n > 0.0 && self.pos[1].n > 0.0 && self.pos[0].cost / self.pos[0].n + self.pos[1].cost / self.pos[1].n < 1.0 { paired = jmin(self.pos[0].n, self.pos[1].n); }
            for s in 0..2 {
                let u = floor2(self.pos[s].n - paired);
                if u < 0.01 || self.last_exit_sec[s] == sec { continue; }
                let mark = if s == 0 { p_up } else { 1.0 - p_up };
                let entry = self.pos[s].cost / self.pos[s].n;
                let why = if self.p.tp.is_some_and(|tp| mark >= entry + tp) { "tp" } else if self.p.sl.is_some_and(|sl| mark <= entry - sl) { "sl" } else { continue };
                self.last_exit_sec[s] = sec;
                let limit = jmax(tick, floor_tick(mark - self.p.exit_slip, tick));
                self.rec.exits.push((t, why, s, u, f64::NAN, f64::NAN, mark));
                out.push(Act::Exit { s, shares: u, limit, why, mark, t });
            }
        }
        if t < 1000.0 * self.p.entry_from { return; }
        let budget = match self.played { Some(b) => b, None => { let b = start(); self.played = Some(b); b } };
        let Some(budget) = budget else { return };
        let mut pull = false;
        if let Some(m) = self.p.pull {
            if (self.okx - self.px_at(t - 1000.0 * m.s)).abs() > m.j { self.pulled_until = t + 1000.0 * m.s; }
            pull = t < self.pulled_until;
            if pull && every_sec && (self.cur[0].is_some() || self.cur[1].is_some()) { self.rec.pulled += 1; }
        }
        if let Some(c) = self.p.cancel_loser {
            if t >= END - 1000.0 * c.x {
                let loser = if p_up > c.y { 1 } else if 1.0 - p_up > c.y { 0 } else { -1 };
                if loser >= 0 && !self.side_done[loser as usize] {
                    let l = loser as usize;
                    self.side_done[l] = true;
                    self.rec.cancel_loser_at.get_or_insert(t);
                    if self.pos[l].n > 0.0 { self.rec.loser_filled_before = true; }
                }
            }
        }
        let q_end = END - 1000.0 * self.p.quote_end;
        for s in 0..2 {
            let o = self.cur[s];
            let mut want: Option<f64> = None;
            if !pull && !self.side_done[s] && t < q_end {
                let ps = if s == 0 { p_up } else { 1.0 - p_up };
                let bb = self.book.best_bid(s) as f64 / 1000.0;
                let ba = self.book.best_ask(s) as f64 / 1000.0;
                let pr = &self.p.price;
                let mut q = match pr.mode.as_str() {
                    "fixed" => pr.fixed,
                    "penny" => if bb > 0.0 { jmin(floor_tick(ps - pr.offset, tick), floor_tick(bb + tick, tick)) } else { f64::NAN },
                    "book" => {
                        let j = self.p.join.unwrap_or(0.0);
                        if bb > 0.0 && floor_tick(bb - j * tick, tick) <= ps - pr.offset + 1e-9 { floor_tick(bb - j * tick, tick) } else { f64::NAN }
                    }
                    _ => floor_tick(ps - pr.offset, tick),
                };
                if self.p.requote == "freeze" { q = *self.frozen[s].get_or_insert(q); }
                if let Some([lo, hi]) = self.p.side_prob { if ps < lo || ps > hi { q = f64::NAN; } }
                if let Some(md) = self.p.max_diverge { if !(bb > 0.0 && ba <= 1.0 && (ps - (bb + ba) / 2.0).abs() <= md) { q = f64::NAN; } }
                if let Some(m) = self.p.pull_side { let mv = self.okx - self.px_at(t - 1000.0 * m.s); if (if s == 0 { mv } else { -mv }) < -m.j { q = f64::NAN; } }
                if let Some(k) = self.p.no_knife {
                    let past = self.fair_at_sec(jmax(0.0, sec - k.t));
                    let now = self.fair_at_sec(jmax(0.0, sec));
                    if past.is_finite() && now.is_finite() && (if s == 0 { now - past } else { past - now }) < -k.m { q = f64::NAN; }
                }
                if let Some(mi) = self.p.max_imbalance { if self.pos[s].n - self.pos[1 - s].n >= mi { q = f64::NAN; } }
                if let Some(pe) = self.p.pair_edge { if self.pos[1 - s].n > 0.0 { q = jmin(q, floor_tick(1.0 - self.pos[1 - s].cost / self.pos[1 - s].n - pe, tick)); } }
                // post-only, never marketable: no bid while it would cross the ask (as maker.rs)
                if q < ba - 1e-9 && q >= tick - 1e-9 && q <= 1.0 - tick + 1e-9 { want = Some(q); }
            }
            let Some(want) = want else { if o.is_some() { self.post(s, None, 0.0, t, out); } continue };
            if let Some(c) = o {
                let x = &self.orders[c];
                // resting at the right price: keep the queue place (until `life`)
                if x.q == want && x.r > 0.0 && self.p.life.is_none_or(|l| t - x.at < l) { continue; }
            }
            let shares = if let Some(sh) = self.p.shares { floor2(jmin(self.p.order_shares.unwrap_or(f64::INFINITY), sh - self.pos[s].n)) } else {
                let budget = budget.unwrap_or(f64::NAN);
                if self.cap[s] == f64::INFINITY && self.p.liq_cap {
                    let mut liq = 0.0;
                    let mut i = jround(want * 1000.0) as i64;
                    let lo = jround((want - 0.05) * 1000.0) as i64;
                    while i >= lo && i >= 0 { liq += self.book.bid_size(s, i); i -= 1; }
                    self.cap[s] = liq * want;
                    if self.cap[s] < budget { self.rec.capped[s] = true; }
                }
                let mut reserved = 0.0;
                if self.p.reserve_in_flight {
                    let orders = &self.orders;
                    self.inflight[s].retain(|&(u, x)| u > t && orders[x].r > 0.0);
                    for &(_, x) in &self.inflight[s] { reserved += self.orders[x].r * self.orders[x].q; }
                    if let Some(c) = o { if self.orders[c].r > 0.0 { reserved += self.orders[c].r * self.orders[c].q; } } // o is about to be replaced
                }
                floor2((jmin(budget, self.cap[s]) - self.spent_side[s] - reserved) / want)
            };
            if shares < self.p.min_size { if o.is_some() { self.post(s, None, 0.0, t, out); } continue; }
            let qa = self.book.bid_size(s, jround(want * 1000.0) as i64);
            let id = self.post(s, Some(want), shares, t, out).unwrap();
            self.orders[id].qa = qa;
            self.rec.joins.push([want, qa, (jround(want * 1000.0) as i64 - self.book.best_bid(s)) as f64]);
            out.push(Act::Place { s, id, q: want, r: shares, qa, t });
        }
    }

    /// `fairSec[i]`: undefined (NaN) outside the array or at a fractional index.
    fn fair_at_sec(&self, i: f64) -> f64 { if i.fract() == 0.0 && i >= 0.0 && (i as usize) < self.fair_sec.len() { self.fair_sec[i as usize] } else { f64::NAN } }

    /// The window record once the loop is over: drift of the fair value 10 s after each fill, positions, payout of
    /// `winner` (0 Up, 1 Down), PnL.
    pub fn finish(&mut self, winner: Option<usize>) -> &Rec {
        self.rec.drift = [0.0, 0.0];
        for s in 0..2 {
            for f in &self.rec.fills[s] {
                let k = jmax(0.0, jmin(300.0, (f[0] / 1000.0).floor())) as usize;
                let (a, b) = (self.fair_sec[k], self.fair_sec[(k + 10).min(300)]);
                if a.is_finite() && b.is_finite() { let sgn = if s == 0 { 1.0 } else { -1.0 }; self.rec.drift[0] += f[1] * sgn * (b - a); self.rec.drift[1] += f[1]; }
            }
        }
        self.rec.pos_n = [self.pos[0].n, self.pos[1].n];
        self.rec.payout = winner.map_or(0.0, |w| self.pos[w].n);
        self.rec.pnl = self.rec.proceeds + self.rec.payout - self.rec.spent - self.rec.fees + self.rec.rebates;
        &self.rec
    }
}
