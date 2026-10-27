//! Candidate C4 (simulated): post-only bids on both sides around the reference fair value, merged pairs (C2 maker)
//! included. Trades were not recorded, so a fill is bounded, never asserted:
//! - `Prudent`: filled only when a recorded ask reaches our price (a seller provably stood there), up to its size;
//! - `Optimistic`: also first in the queue, and any size taken off a recorded best bid at or below our price is a
//!   sale into us.
//! A quote change takes effect one way later (`ONE_WAY_MS`); at most `SHARES` per side and window; no fee (maker).
use engine_strategy::reference::Reference;
use std::collections::VecDeque;

#[derive(Clone, Copy, PartialEq)]
pub enum Bound { Prudent, Optimistic }

/// Half the Dublin RTT p50 to the CLOB (24.4 ms, `01-audit.md`); symmetry of the path [non vérifié].
const ONE_WAY_MS: f64 = 12.2;
const SHARES: f64 = 10.0;
const MARK_MS: f64 = 10_000.0;

pub struct Fill { pub slug: String, pub side: usize, pub t: f64, pub shares: f64, pub price: f64, pub fair: f64, pub fair_later: f64 }

pub struct Maker {
    bound: Bound,
    half: f64,
    slug: String,
    target: [Option<f64>; 2],
    changes: [VecDeque<(f64, Option<f64>)>; 2],
    live: [Option<f64>; 2],
    filled: [f64; 2],
    last_bid: [(f64, f64); 2],
    marking: Vec<Fill>,
    pub done: Vec<Fill>,
}

fn floor_tick(p: f64, tick: f64) -> f64 { format!("{:.6}", (p / tick + 1e-9).floor() * tick).parse().unwrap() }

impl Maker {
    pub fn new(bound: Bound, half: f64) -> Self {
        Maker { bound, half, slug: String::new(), target: [None; 2], changes: Default::default(), live: [None; 2], filled: [0.0; 2],
            last_bid: [(f64::NAN, 0.0); 2], marking: Vec::new(), done: Vec::new() }
    }

    /// Called after every event the strategy processed.
    pub fn step(&mut self, r: &Reference) {
        let t = r.time();
        let Some((slug, tokens, tick, tau, fair)) = r.now() else { return };
        if slug != self.slug {
            *self = Maker { bound: self.bound, half: self.half, slug: slug.to_string(), marking: std::mem::take(&mut self.marking),
                done: std::mem::take(&mut self.done), ..Maker::new(self.bound, self.half) };
        }
        let up = fair;
        let mut i = 0;
        while i < self.marking.len() {
            if t - self.marking[i].t >= MARK_MS {
                let mut f = self.marking.remove(i);
                if f.slug == slug && up.is_finite() { f.fair_later = if f.side == 0 { up } else { 1.0 - up }; }
                self.done.push(f);
            } else { i += 1; }
        }
        for s in 0..2 {
            let (bid, ask) = r.best(&tokens[s]);
            // quote: fair minus the half spread, never marketable (post-only), none in the last 10 s
            let want = if up.is_finite() && (10.0..=290.0).contains(&tau) && self.filled[s] < SHARES {
                let fs = if s == 0 { up } else { 1.0 - up };
                let q = floor_tick(fs - self.half, tick);
                (q >= tick && q < ask).then_some(q)
            } else { None };
            if want != self.target[s] { self.target[s] = want; self.changes[s].push_back((t + ONE_WAY_MS, want)); }
            while self.changes[s].front().is_some_and(|c| c.0 <= t) { self.live[s] = self.changes[s].pop_front().unwrap().1; }
            let size_at_bid = r.size_at(&tokens[s], false, bid);
            let before = self.last_bid[s];
            self.last_bid[s] = (bid, size_at_bid);
            let Some(q) = self.live[s] else { continue };
            let room = SHARES - self.filled[s];
            let mut take = 0.0;
            if ask <= q + 1e-12 { take = r.size_at(&tokens[s], true, ask).min(room); }
            if take == 0.0 && self.bound == Bound::Optimistic && bid == before.0 && bid <= q + 1e-12 && size_at_bid < before.1 {
                take = (before.1 - size_at_bid).min(room);
            }
            if take > 0.0 {
                self.filled[s] += take;
                let fs = if s == 0 { up } else { 1.0 - up };
                self.marking.push(Fill { slug: slug.to_string(), side: s, t, shares: take, price: q, fair: fs, fair_later: f64::NAN });
            }
        }
    }

    pub fn finish(&mut self) { self.done.append(&mut self.marking); }
}
