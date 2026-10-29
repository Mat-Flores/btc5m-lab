//! The maker strategy (`engine_strategy::maker`, one `Maker` per window) behind the `Strategy` contract.
//! - Inputs as the maker backtest's packs hold them (maker-bt/data.mjs `packB`), per window, times from its open:
//!   the price source's mid (on change) from -360 s to +310 s, the oracle's prints from -360 s to +360 s, the Up
//!   book of the books source from -60 s to +305 s. A window learnt late gets the prices and prints kept since.
//! - Its acts become intents: bids are post-only GTC of `Shares`, exits FAK sells.
//! - Capital as `maker-bt/run.mjs` compounds it: budget = f × free capital / 2 per side, taken at the window's first
//!   decision past `entryFrom`; a window's flows come back at its end, its payout at its result (+ `claim` s).
//!   Not done online: skipping a window without an official result (`no-result`, a look-ahead), `minPrevVolume`
//!   (needs the public trade tape: such a window is not played), `prob: 'vwap'` (idem: no tape, no decision).
//!   [à vérifier] the time of a fill given to the strategy is the last instant it saw (the venue event has none).
use crate::engine::{Amount, Decision, Intent, Strategy, Tif, VenueEvent};
use engine_core::account::Dir;
use engine_core::book::Books;
use engine_core::clock::Stamp;
use engine_core::event::{Event, Payload, Side};
use engine_core::market::Window;
use engine_core::money::{Micro, MICRO};
use engine_core::order::Input;
use engine_strategy::maker::{Act, Fee, Maker, Params};
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};

fn d_price() -> String { "okx".into() }
fn d_oracle() -> String { "chainlink".into() }
fn d_book() -> String { "polymarket".into() }
/// The configured sources it reads, by role.
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Roles { #[serde(default = "d_price")] pub price: String, #[serde(default = "d_oracle")] pub oracle: String, #[serde(default = "d_book")] pub book: String }
impl Default for Roles { fn default() -> Self { Roles { price: d_price(), oracle: d_oracle(), book: d_book() } } }

#[derive(Clone, Copy)]
enum Kind { Order(usize), Exit(usize) }

struct Win { w: Window, core: Maker, client: HashMap<usize, u64>, booked: bool, winner: Option<usize>, paid: bool }

pub struct MakerStrategy {
    p: Params,
    roles: Roles,
    free: f64,
    /// Payouts waiting for their claim: (release instant, s; amount).
    pending: Vec<(f64, f64)>,
    claim_s: f64,
    day: Option<f64>,
    day_start: f64,
    wins: Vec<Win>,
    last_mid: f64,
    /// Prices and prints of the last 360 s (wall ns), for a window learnt after they came.
    px: VecDeque<(u64, f64)>,
    cl: VecDeque<(u64, f64, f64)>,
    next_id: u64,
    ids: HashMap<u64, (u64, Kind)>,
    by_n: HashMap<u32, u64>,
    exits: HashMap<u64, (f64, f64)>,
    last_ns: u64,
    /// Prices and prints are kept this long (ns), for a window learnt after they came (`keepMs`).
    keep_ns: u64,
    played: u64,
}

fn rel(wall_ns: u64, start: u64) -> f64 { (wall_ns as i128 - start as i128 * 1_000_000_000) as f64 / 1e6 }

impl MakerStrategy {
    /// `capital0`: free capital at the start, $ (run.mjs `capital0`, 1000); `claim`: s between a window's end and its
    /// payout being free (run.mjs `claim`, 0); `keep_ns`: see the field (360 s: the packs' 360 s before the open).
    pub fn new(p: Params, roles: Roles, capital0: f64, claim_s: f64, keep_ns: u64) -> MakerStrategy {
        MakerStrategy { p, roles, free: capital0, pending: vec![], claim_s, day: None, day_start: 0.0, wins: vec![], last_mid: f64::NAN, px: VecDeque::new(), cl: VecDeque::new(),
            next_id: 0, ids: HashMap::new(), by_n: HashMap::new(), exits: HashMap::new(), last_ns: 0, keep_ns, played: 0 }
    }

    /// Runs `f` on window `i`'s core with the capital rule, then turns its acts into decisions.
    fn drive(&mut self, i: usize, out: &mut Vec<Decision>, f: impl FnOnce(&mut Maker, &mut Vec<Act>, &mut dyn FnMut() -> Option<Option<f64>>)) {
        let mut acts = vec![];
        {
            let MakerStrategy { wins, p, free, pending, day, day_start, played, .. } = self;
            let start_s = wins[i].w.start as f64;
            // run.mjs makeRun.step, at the window's first decision past entryFrom
            let mut start = || {
                let mut j = pending.len();
                while j > 0 { j -= 1; if pending[j].0 <= start_s { *free += pending[j].1; pending.remove(j); } }
                let eq_before = *free + pending.iter().map(|x| x.1).sum::<f64>();
                let d = (start_s / 86400.0).floor();
                if *day != Some(d) { *day = Some(d); *day_start = eq_before; }
                let budget = if p.shares.is_some() { None } else { Some(p.f * *free / 2.0) };
                if p.daily_stop.is_some_and(|ds| eq_before - *day_start <= -ds * *day_start) { return None; }
                if p.min_prev_volume > 0.0 { return None; }
                if budget.is_some_and(|b| b < p.min_size * 0.01) { return None; }
                *played += 1;
                Some(budget)
            };
            f(&mut wins[i].core, &mut acts, &mut start);
        }
        for a in acts { self.act(i, a, out); }
        self.settle(i);
    }

    fn act(&mut self, i: usize, a: Act, out: &mut Vec<Decision>) {
        let w = &mut self.wins[i];
        let mut id = || { self.next_id += 1; self.next_id };
        match a {
            Act::Place { s, id: o, q, r, .. } => {
                let c = id();
                w.client.insert(o, c);
                self.ids.insert(c, (w.w.start, Kind::Order(o)));
                out.push(Decision::Place(Intent { client_id: c, token: w.w.tokens[s].clone(), side: Dir::Buy, limit: (q * MICRO as f64).round() as Micro,
                    amount: Amount::Shares((r * MICRO as f64).round() as Micro), tif: Tif::Gtc, post_only: true, reason: "maker" }));
            }
            Act::Cancel { id: o, .. } => if let Some(c) = w.client.get(&o) { out.push(Decision::Cancel { client_id: *c }); },
            Act::Exit { s, shares, limit, why, .. } => {
                let c = id();
                self.ids.insert(c, (w.w.start, Kind::Exit(s)));
                self.exits.insert(c, (0.0, 0.0));
                out.push(Decision::Place(Intent { client_id: c, token: w.w.tokens[s].clone(), side: Dir::Sell, limit: (limit * MICRO as f64).round() as Micro,
                    amount: Amount::Shares((shares * MICRO as f64).round() as Micro), tif: Tif::Fak, post_only: false, reason: why }));
            }
        }
    }

    /// A window whose loop is over gives its flows back; its payout once its result is known.
    fn settle(&mut self, i: usize) {
        let w = &mut self.wins[i];
        if w.core.over && !w.booked {
            w.booked = true;
            let r = w.core.finish(w.winner);
            self.free += -r.spent + r.proceeds - r.fees + r.rebates;
        }
        if w.booked && !w.paid {
            if let Some(win) = w.winner {
                w.paid = true;
                let payout = w.core.finish(Some(win)).payout;
                if payout > 0.0 { if self.claim_s > 0.0 { self.pending.push((w.w.end as f64 + self.claim_s, payout)); } else { self.free += payout; } }
            }
        }
    }

    fn tick_all(&mut self, wall_ns: u64, out: &mut Vec<Decision>) {
        self.last_ns = self.last_ns.max(wall_ns);
        for i in 0..self.wins.len() { let t = rel(wall_ns, self.wins[i].w.start); self.drive(i, out, |c, a, s| c.tick(t, a, s)); }
        // a window is forgotten an hour after its end, paid or not
        let now_s = wall_ns / 1_000_000_000;
        self.wins.retain(|w| w.w.end + 3600 > now_s || !(w.booked));
    }

    fn book_records(&mut self, i: usize, t: f64, recs: &[(u8, f64, f64)], out: &mut Vec<Decision>) {
        for &(typ, price, size) in recs {
            self.drive(i, out, |c, a, s| { let b = c.book(t, typ, price, size, a, s); c.after_book(b, a, s); });
        }
    }
}

impl Strategy for MakerStrategy {
    fn sources(&self) -> Vec<String> { vec![self.roles.price.clone(), self.roles.oracle.clone(), self.roles.book.clone()] }

    fn on_window(&mut self, w: &Window, at: Stamp, out: &mut Vec<Decision>) {
        if self.wins.iter().any(|x| x.w.start == w.start) { return; }
        let fee = Fee { rate: w.fee.rate as f64 / 1e6, exponent: w.fee.exponent as f64, rebate_rate: w.fee.rebate_rate as f64 / 1e6 };
        self.wins.push(Win { w: w.clone(), core: Maker::new(self.p.clone(), fee, w.tick as f64 / 1e6), client: HashMap::new(), booked: false, winner: w.winner, paid: false });
        let i = self.wins.len() - 1;
        // what came before it was known, in time order (a print before a price at the same instant, as sim.mjs)
        let mut seed: Vec<(u64, u8, f64, f64)> = self.cl.iter().map(|&(n, ts, p)| (n, 0, ts, p)).chain(self.px.iter().map(|&(n, p)| (n, 1, 0.0, p))).collect();
        seed.sort_by_key(|x| (x.0, x.1));
        let start = w.start;
        for (n, k, ts, p) in seed {
            let t = rel(n, start);
            if k == 0 && t >= -360e3 { self.drive(i, out, |c, a, s| c.cl(t, ts - start as f64 * 1000.0, p, a, s)); }
            if k == 1 && (-360e3..=310e3).contains(&t) { self.drive(i, out, |c, a, s| c.px(t, p, a, s)); }
        }
        self.tick_all(at.wall_ns, out);
    }

    fn on_event(&mut self, source: &str, e: &Event, books: Option<&Books>, out: &mut Vec<Decision>) {
        let n = e.recv.wall_ns;
        self.tick_all(n, out);
        match &e.payload {
            Payload::Quote { bid, ask } if source == self.roles.price => {
                // the recorded prices were the mid, sent when it changed (src/feeds/live.js)
                let mid = (*bid as f64 / 1e6 + *ask as f64 / 1e6) / 2.0;
                if mid == self.last_mid { return; }
                self.last_mid = mid;
                self.px.push_back((n, mid));
                while self.px.front().is_some_and(|x| x.0 + self.keep_ns < n) { self.px.pop_front(); }
                for i in 0..self.wins.len() { let t = rel(n, self.wins[i].w.start); if (-360e3..=310e3).contains(&t) { self.drive(i, out, |c, a, s| c.px(t, mid, a, s)); } }
            }
            Payload::Oracle { value } if source == self.roles.oracle => {
                let Some(ms) = e.event_ms else { return };
                let p = *value as f64 / 1e6;
                self.cl.push_back((n, ms as f64, p));
                while self.cl.front().is_some_and(|x| x.0 + self.keep_ns < n) { self.cl.pop_front(); }
                for i in 0..self.wins.len() {
                    let start = self.wins[i].w.start;
                    let t = rel(n, start);
                    if (-360e3..=360e3).contains(&t) { self.drive(i, out, |c, a, s| c.cl(t, ms as f64 - start as f64 * 1000.0, p, a, s)); }
                }
            }
            Payload::BookSnapshot { asset, bids, asks, .. } if source == self.roles.book && books.is_some() => {
                let Some(i) = self.wins.iter().position(|w| w.w.up() == asset) else { return };
                let t = rel(n, self.wins[i].w.start);
                if !(-60e3..=305e3).contains(&t) { return; }
                let l = |typ: u8, v: &[engine_core::event::Level]| v.iter().map(move |x| (typ, x.price as f64 / 1e6, x.size as f64 / 1e6)).collect::<Vec<_>>();
                let recs: Vec<_> = std::iter::once((0, 0.0, 0.0)).chain(l(1, bids)).chain(l(2, asks)).collect();
                self.book_records(i, t, &recs, out);
            }
            Payload::BookDelta { changes } if source == self.roles.book && books.is_some() => {
                for c in changes {
                    let Some(i) = self.wins.iter().position(|w| *w.w.up() == c.asset) else { continue };
                    let t = rel(n, self.wins[i].w.start);
                    if !(-60e3..=305e3).contains(&t) { continue; }
                    self.book_records(i, t, &[(if c.side == Side::Bid { 1 } else { 2 }, c.price as f64 / 1e6, c.size as f64 / 1e6)], out);
                }
            }
            _ => {}
        }
    }

    fn on_resolved(&mut self, market: &str, up_won: bool, _: Option<Micro>, _: Stamp, _: &mut Vec<Decision>) {
        let Some(i) = self.wins.iter().position(|w| w.w.market == market) else { return };
        self.wins[i].winner = Some(if up_won { 0 } else { 1 });
        self.settle(i);
    }

    fn on_venue(&mut self, ev: &VenueEvent, out: &mut Vec<Decision>) {
        let Some(&c) = self.by_n.get(&ev.n()) else { return };
        let Some(&(start, kind)) = self.ids.get(&c) else { return };
        let Some(i) = self.wins.iter().position(|w| w.w.start == start) else { return };
        let t = rel(self.last_ns, start);
        match (kind, ev) {
            (Kind::Order(o), VenueEvent::Fill { size, rebate, .. }) => { let sh = *size as f64 / 1e6; self.wins[i].core.filled(o, sh, t, rebate.is_some()); }
            // a refused or closed bid is dead: nothing more fills it
            (Kind::Order(o), VenueEvent::Answer { input: Input::Rejected { .. }, .. } | VenueEvent::Closed { .. }) => self.wins[i].core.orders[o].r = 0.0,
            (Kind::Exit(_), VenueEvent::Fill { size, price, .. }) => if let Some(x) = self.exits.get_mut(&c) { let sh = *size as f64 / 1e6; x.0 += sh; x.1 += sh * (*price as f64 / 1e6); },
            (Kind::Exit(s), VenueEvent::Closed { .. } | VenueEvent::Answer { input: Input::Rejected { .. }, .. }) => {
                let (sold, got) = self.exits.remove(&c).unwrap_or((0.0, 0.0));
                self.drive(i, out, |core, a, _| core.exit_done(s, sold, got, t, a));
            }
            _ => {}
        }
    }

    fn on_tick(&mut self, at: Stamp, out: &mut Vec<Decision>) { self.tick_all(at.wall_ns, out); }

    fn placed(&mut self, client_id: u64, n: u32) { self.by_n.insert(n, client_id); }

    fn stats(&self) -> Vec<(&'static str, f64)> {
        let posted: u64 = self.wins.iter().map(|w| w.core.rec.posted).sum();
        vec![("free_usd", self.free), ("windows_played", self.played as f64), ("posted_open_windows", posted as f64)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Command, Engine, EngineInput};
    use engine_core::event::Level;
    use engine_core::market::{FeeSchedule, MarketSpec};

    const S: u64 = 1_792_359_300;
    fn at(ms: i64) -> Stamp { let n = ((S as i64 * 1000 + ms) * 1_000_000) as u64; Stamp { mono_ns: n, wall_ns: n } }
    fn ev(source: u16, ms: i64, event_ms: Option<u64>, payload: Payload) -> Event { Event { source, recv: at(ms), event_ms, seq: None, payload } }

    /// Through the engine: bids on both sides, post-only GTC of shares; a fill reaches the window's position.
    #[test]
    fn quotes_both_sides_and_takes_its_fills() {
        let m = MarketSpec { venue: "polymarket".into(), asset: "btc".into(), horizon_s: 300 };
        let s = crate::registry::strategy("maker", serde_json::json!({ "sources": { "price": "okx", "oracle": "chainlink", "book": "polymarket" }, "liqCap": false }), &m).unwrap();
        let mut e = Engine::new(vec![("okx".into(), false), ("chainlink".into(), false), ("polymarket".into(), true)], vec![s], 200_000_000, 3600).unwrap();
        e.used[2] = true;
        let w = Window { start: S, end: S + 300, slug: format!("btc-updown-5m-{S}"), market: "m".into(), tokens: ["up".into(), "down".into()], tick: 10_000, min_size: 5_000_000,
            fee: FeeSchedule { rate: 70_000, exponent: 1, rebate_rate: 200_000 }, taker_delay: true, twap_s: Some(60), price_to_beat: None, winner: None };
        let mut out = vec![];
        e.on(EngineInput::Window(w), at(-400_000), &mut out);
        let lv = |p, s| Level { price: p, size: s };
        // prices and prints from 6 min before the open, then the book, then 20 s of the window
        for k in 0..=760i64 {
            let t = -360_000 + k * 500;
            e.on(EngineInput::Feed(ev(0, t, None, Payload::Quote { bid: 81_000_000_000 + (k % 7) * 1_000_000, ask: 81_000_100_000 + (k % 7) * 1_000_000 })), at(t), &mut out);
            e.on(EngineInput::Feed(ev(1, t + 100, Some((S as i64 * 1000 + t - 800) as u64), Payload::Oracle { value: 80_975_000_000 + (k % 5) * 1_000_000 })), at(t + 100), &mut out);
            if t >= -60_000 && k % 2 == 0 {
                e.on(EngineInput::Feed(ev(2, t + 200, Some((S as i64 * 1000 + t) as u64), Payload::BookSnapshot { asset: "up".into(),
                    bids: vec![lv(400_000 + (k % 3) * 10_000, 100_000_000)], asks: vec![lv(600_000, 100_000_000)], tick: Some(10_000) })), at(t + 200), &mut out);
            }
            e.on(EngineInput::Tick, at(t + 300), &mut out);
        }
        let places: Vec<_> = out.iter().filter_map(|c| match c { Command::Place { n, intent, .. } => Some((*n, intent.clone())), _ => None }).collect();
        assert!(places.iter().any(|(_, i)| i.token == "up") && places.iter().any(|(_, i)| i.token == "down"), "{out:?}");
        assert!(places.iter().all(|(_, i)| i.tif == Tif::Gtc && i.post_only && i.side == Dir::Buy && matches!(i.amount, Amount::Shares(_))));
        assert!(out.iter().any(|c| matches!(c, Command::Cancel { .. })));
        // a fill of the last Up bid
        let (n, i) = places.iter().rev().find(|(_, i)| i.token == "up").unwrap().clone();
        e.on(EngineInput::Venue(VenueEvent::Fill { n, trade: "t".into(), status: engine_core::order::TradeStatus::Matched, size: 5_000_000, price: i.limit, maker: true, fee: 0, rebate: Some(1) }), at(20_000), &mut out);
        let stats = e.stats();
        assert_eq!(stats[0][1], ("windows_played", 1.0));
    }
}
