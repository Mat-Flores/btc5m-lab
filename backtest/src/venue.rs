//! The simulated exchange, one window at a time, on the Up-token book (Down bids are Up asks mirrored): the exchange
//! half of `simWindow` (engine/maker-bt/sim.mjs of btc5m-lag-sim), ported operation for operation. Every value comes
//! from the configuration (`venue` of config/default.json); none is written here.
//! - Our posts and cancels take effect `one_way_ms` after the decision. A book change stamped `ts` (server) that fills
//!   a bid came from a taker submitted at `ts − fill_lookback_ms`: it fills the bid live at that instant only.
//! - No fill on a snapshot, a re-listed snapshot level, a book silent for longer than `silence_ms` before the change,
//!   before the open, or past the end + `trading_cutoff_ms_after_end`.
//! - Fill bounds: `exact` (real queue, real trades from `labels`), `prudent`, `optimistic` (sim.mjs:14-29).
//! - Taker exits (FAK sells) arrive after `one_way_ms` + a sampled hold and take the bids present then (decision D6:
//!   sim.mjs booked them at the decision with a look-ahead book).
use engine_strategy::maker::Book;
use engine_strategy::model::{jmin, jround};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Bound { Exact, Prudent, Optimistic }

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelQueue { ProRata, Behind }

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TakerHold { pub quantiles_s: Vec<[f64; 2]>, pub seed: u32 }

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MinSizeRule { pub min_shares: f64 }

/// `venue` of the configuration. Values other than the validated behaviour that are not implemented are refused
/// when the configuration is read (`check`), never ignored.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VenueCfg {
    pub one_way_ms: f64,
    pub ack_ms: f64,
    pub taker_hold: TakerHold,
    pub fill_bound: Bound,
    pub fill_lookback_ms: f64,
    pub cancel_queue: CancelQueue,
    /// E1: share of the pro-rata thinning a cancel at our level applies to the queue ahead (1: pro rata, validated;
    /// 0: as `behind`). Unused with `behind`.
    pub cancel_ahead_factor: f64,
    pub post_only_cross_check: String,
    pub silence_ms: f64,
    pub trading_cutoff_ms_after_end: f64,
    pub rebates: bool,
    pub trade_fail_rate: f64,
    pub consume_own_liquidity: bool,
    pub min_size_rule: MinSizeRule,
}

impl VenueCfg {
    pub fn check(&self) -> Result<(), String> {
        let no = |what: &str| Err(format!("venue.{what}: not implemented (phase B, E1) [à vérifier]"));
        if self.ack_ms != 0.0 { return no("ack_ms ≠ 0"); }
        if self.post_only_cross_check != "decision" { return no("post_only_cross_check ≠ decision"); }
        if self.trade_fail_rate != 0.0 { return no("trade_fail_rate ≠ 0"); }
        if self.consume_own_liquidity { return no("consume_own_liquidity"); }
        if !(0.0..=1.0).contains(&self.cancel_ahead_factor) { return Err("venue.cancel_ahead_factor: between 0 and 1".into()); }
        let q = &self.taker_hold.quantiles_s;
        if q.len() < 2 || q[0][0] != 0.0 || q[q.len() - 1][0] != 1.0 || q.windows(2).any(|w| !(w[1][0] > w[0][0])) {
            return Err("venue.taker_hold.quantiles_s: [[0, s], …, [1, s]] with increasing quantiles".into());
        }
        Ok(())
    }

    /// The `ahead` share of `exact_fill`.
    pub fn cancel_ahead(&self) -> f64 { match self.cancel_queue { CancelQueue::ProRata => self.cancel_ahead_factor, CancelQueue::Behind => 0.0 } }
}

/// `rng(seed)` of sim.mjs (mulberry32).
pub struct Rng(u32);
impl Rng {
    pub fn new(seed: u32) -> Rng { Rng(seed) }
    pub fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        (t ^ (t >> 14)) as f64 / 4_294_967_296.0
    }
}

/// `holdQuantile(u)` (sim.mjs:88-94), seconds.
pub fn hold_quantile(q: &[[f64; 2]], u: f64) -> f64 {
    for i in 1..q.len() {
        let ([u0, h0], [u1, h1]) = (q[i - 1], q[i]);
        if u <= u1 { return h0 + (h1 - h0) * (u - u0) / (u1 - u0); }
    }
    q[q.len() - 1][1]
}

/// A resting bid as the exchange sees it: side, price, shares left, queue ahead.
#[derive(Clone, Debug)]
pub struct Order { pub s: usize, pub q: f64, pub r: f64, pub qa: f64 }

/// `exactFill` (sim.mjs:148-156): shares a change of the Up book fills for bid `o` on side `s`; `o.qa` updated.
/// typ 1 bid / 2 ask, level `ch_i` (thousandths), size `ch_old` → `ch_size`, `v` traded volume of the change; `ahead`:
/// share of the pro-rata thinning a cancel applies to the queue ahead (1 pro rata, 0 behind: `VenueCfg::cancel_ahead`).
pub fn exact_fill(o: &mut Order, s: usize, typ: u8, ch_i: f64, ch_old: f64, ch_size: f64, v: f64, ahead: f64) -> f64 {
    let qi = jround(o.q * 1000.0);
    let is_bid = (s == 0) == (typ == 1);
    let lvl = if s == 0 { ch_i } else { 1000.0 - ch_i };
    if !is_bid { return if lvl <= qi && ch_size > ch_old { ch_size - ch_old } else { 0.0 }; }
    if v > 0.0 && lvl < qi { return v; }
    if lvl != qi { return 0.0; }
    if v > 0.0 { let used = jmin(o.qa, v); o.qa -= used; return v - used; }
    if ahead > 0.0 && ch_old > ch_size && ch_old > 0.0 { o.qa -= ahead * (ch_old - ch_size) * o.qa / ch_old; }
    0.0
}

/// A taker exit on its way: arrival instant, side, shares, limit (thousandths are read from it at arrival).
#[derive(Clone, Copy, Debug)]
pub struct Exit { pub at: f64, pub s: usize, pub shares: f64, pub limit: f64, pub id: usize }

/// The exchange of one window. Times in ms from the open (local receive time `t`, server time `ts`).
pub struct WindowVenue<'a> {
    cfg: &'a VenueCfg,
    end_ms: f64,
    labels: Option<&'a [f32]>,
    pub book: Book,
    last_book_t: f64,
    snap_t: f64,
    prev_bid: [i64; 2],
    prev_size: [f64; 2],
    /// Order changes per side [effective time, order], in time order.
    hist: [Vec<(f64, Option<usize>)>; 2],
    pub orders: Vec<Order>,
    exits: Vec<Exit>,
    next_exit: usize,
    rng: Rng,
}

impl<'a> WindowVenue<'a> {
    /// `start`: the window's open (s), seeds the exit holds as sim.mjs (`rng((start ^ seed) >>> 0)`).
    pub fn new(cfg: &'a VenueCfg, start: u64, end_ms: f64, labels: Option<&'a [f32]>) -> WindowVenue<'a> {
        WindowVenue { cfg, end_ms, labels, book: Book::default(), last_book_t: f64::NEG_INFINITY, snap_t: f64::NAN, prev_bid: [-1, -1], prev_size: [0.0, 0.0],
            hist: [vec![], vec![]], orders: vec![], exits: vec![], next_exit: 0, rng: Rng::new(((start as u32) as i32 ^ cfg.taker_hold.seed as i32) as u32) }
    }

    /// A post-only bid decided at `t` (`qa` = its level's size at the decision): live from `t + one_way_ms`.
    pub fn place(&mut self, s: usize, q: f64, r: f64, qa: f64, t: f64) -> usize {
        self.orders.push(Order { s, q, r, qa });
        let id = self.orders.len() - 1;
        self.hist[s].push((t + self.cfg.one_way_ms, Some(id)));
        id
    }

    /// The bid of side `s` stops, decided at `t`.
    pub fn cancel(&mut self, s: usize, t: f64) { self.hist[s].push((t + self.cfg.one_way_ms, None)); }

    /// validate.mjs: an order live on side `s` from server time `from` (queue ahead: its level now) until `until`.
    pub fn script(&mut self, s: usize, q: f64, r: f64, from: f64, until: f64) -> usize {
        let qa = self.book.bid_size(s, jround(q * 1000.0) as i64);
        self.orders.push(Order { s, q, r, qa });
        let id = self.orders.len() - 1;
        self.hist[s].push((from, Some(id)));
        self.hist[s].push((until, None));
        id
    }

    fn live_at(&self, s: usize, t: f64) -> Option<usize> {
        let h = &self.hist[s];
        let i = h.partition_point(|x| x.0 <= t);
        if i == 0 { None } else { h[i - 1].1 }
    }

    fn fill(&mut self, id: usize, shares: f64, ts: f64, out: &mut Vec<(usize, f64, f64)>) {
        let o = &mut self.orders[id];
        let shares = jmin(shares, o.r);
        if !(shares > 1e-9) { return; }
        o.r -= shares;
        out.push((id, shares, ts));
    }

    /// Book record `j` [t, ts, type, price, size] (type 0: a snapshot begins): updates the book and pushes the fills
    /// it makes (order, shares, server time) — before the strategy decides on it (sim.mjs:446-448).
    pub fn record(&mut self, j: usize, t: f64, ts: f64, typ: u8, price: f64, size: f64, out: &mut Vec<(usize, f64, f64)>) {
        let silent_before = t - self.last_book_t > self.cfg.silence_ms;
        self.last_book_t = t;
        let (mut ch_i, mut ch_old) = (0.0, 0.0);
        if typ == 0 { self.book.clear(); self.snap_t = t; } else { ch_i = jround(price * 1000.0); ch_old = self.book.set(typ, price, size); }
        let skip = typ == 0 || t == self.snap_t || silent_before || t < 0.0 || ts > self.end_ms + self.cfg.trading_cutoff_ms_after_end;
        for s in 0..2 {
            let bb = self.book.best_bid(s);
            let b_size = if bb >= 0 { self.book.bid_size(s, bb) } else { 0.0 };
            let (before_bid, before_size) = (self.prev_bid[s], self.prev_size[s]);
            self.prev_bid[s] = bb;
            self.prev_size[s] = b_size;
            if skip { continue; }
            let Some(id) = self.live_at(s, ts - self.cfg.fill_lookback_ms) else { continue };
            if self.orders[id].r <= 0.0 { continue; }
            let qi = jround(self.orders[id].q * 1000.0);
            let take = match self.cfg.fill_bound {
                Bound::Exact => {
                    let v = self.labels.map_or(0.0, |l| l[j] as f64);
                    exact_fill(&mut self.orders[id], s, typ, ch_i, ch_old, size, v, self.cfg.cancel_ahead())
                }
                b => {
                    let ba = self.book.best_ask(s);
                    let mut take = 0.0;
                    if (ba as f64) <= qi { take = self.book.ask_size(s, ba); }
                    if take == 0.0 && b == Bound::Optimistic && bb == before_bid && bb >= 0 && (bb as f64) <= qi && b_size < before_size { take = before_size - b_size; }
                    take
                }
            };
            if take > 0.0 { self.fill(id, take, ts, out); }
        }
    }

    /// A FAK sell decided at `t`: it arrives after `one_way_ms` and a hold drawn from `taker_hold`.
    pub fn exit(&mut self, s: usize, shares: f64, limit: f64, t: f64) -> usize {
        let hold = 1000.0 * hold_quantile(&self.cfg.taker_hold.quantiles_s, self.rng.next());
        let id = self.exits.len();
        self.exits.push(Exit { at: t + self.cfg.one_way_ms + hold, s, shares, limit, id });
        id
    }

    /// The earliest exit still on its way.
    pub fn next_exit(&self) -> Option<Exit> { self.exits[self.next_exit..].iter().min_by(|a, b| a.at.total_cmp(&b.at)).copied() }

    /// Carries out exit `id` against the bids present now: (shares sold, $ received), as sim.mjs:332-335.
    pub fn take_exit(&mut self, id: usize) -> (f64, f64) {
        let x = self.exits[id];
        let k = self.next_exit + self.exits[self.next_exit..].iter().position(|e| e.id == id).unwrap();
        self.exits.swap(self.next_exit, k);
        self.next_exit += 1;
        let (mut sold, mut got) = (0.0, 0.0);
        let mut i = self.book.best_bid(x.s);
        while (i as f64) >= jround(x.limit * 1000.0) && sold < x.shares {
            let sz = self.book.bid_size(x.s, i);
            if sz > 0.0 { let q = jmin(sz, x.shares - sold); sold += q; got += q * i as f64 / 1000.0; }
            i -= 1;
        }
        (sold, got)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// sim.test.mjs « exact: trades take the queue first in first out, cancels thin it pro rata, a trade below our bid meets us ».
    #[test]
    fn exact_fill_is_fifo_pro_rata_and_met_below() {
        let pr = 1.0;
        let mut o = Order { s: 0, q: 0.45, r: 50.0, qa: 100.0 };
        assert_eq!(exact_fill(&mut o, 0, 1, 450.0, 200.0, 160.0, 40.0, pr), 0.0);
        assert_eq!(o.qa, 60.0);
        assert_eq!(exact_fill(&mut o, 0, 1, 450.0, 160.0, 80.0, 0.0, pr), 0.0);
        assert_eq!(o.qa, 30.0);
        assert_eq!(exact_fill(&mut o, 0, 1, 450.0, 80.0, 40.0, 40.0, pr), 10.0);
        assert_eq!(exact_fill(&mut o, 0, 1, 440.0, 90.0, 70.0, 20.0, pr), 20.0);
        assert_eq!(exact_fill(&mut o, 0, 1, 460.0, 90.0, 70.0, 20.0, pr), 0.0);
        assert_eq!(exact_fill(&mut Order { s: 1, q: 0.45, r: 50.0, qa: 0.0 }, 1, 2, 550.0, 90.0, 70.0, 20.0, pr), 20.0);
        // `behind`: a cancel leaves the queue ahead as it was
        let mut b = Order { s: 0, q: 0.45, r: 50.0, qa: 100.0 };
        exact_fill(&mut b, 0, 1, 450.0, 160.0, 80.0, 0.0, 0.0);
        assert_eq!(b.qa, 100.0);
        // in between: half the pro-rata thinning (100 − 0.5 × 80 × 100 / 160)
        exact_fill(&mut b, 0, 1, 450.0, 160.0, 80.0, 0.0, 0.5);
        assert_eq!(b.qa, 75.0);
    }

    #[test]
    fn hold_quantiles_and_rng_follow_sim_mjs() {
        let q = [[0.0, 0.15], [0.1, 0.19], [0.5, 0.36], [0.9, 1.5], [1.0, 3.5]];
        assert_eq!(hold_quantile(&q, 0.0), 0.15);
        assert_eq!(hold_quantile(&q, 1.0), 3.5);
        assert!((hold_quantile(&q, 0.3) - 0.275).abs() < 1e-15);
        // mulberry32(7): first draws of `rng(7)` in Node 24
        let mut r = Rng::new(7);
        let v: Vec<f64> = (0..3).map(|_| r.next()).collect();
        assert_eq!(v, vec![0.011704753153026104, 0.06195825757458806, 0.97690763277933]);
    }
}
