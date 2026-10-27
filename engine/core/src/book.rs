//! Order book of one asset, in integer micro-units, with integrity checks. A book is usable only once a snapshot
//! has arrived and no check has failed since; a failed check or a tick change waits for a new snapshot.
use crate::event::{Level, Side};
use crate::money::Micro;
use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Integrity {
    /// Best bid at or above best ask.
    Crossed { bid: Micro, ask: Micro },
    /// Our best price differs from the one the source announced with the change.
    Mismatch { side: Side, ours: Option<Micro>, theirs: Micro },
    /// A level off the current price grid.
    OffTick { price: Micro, tick: Micro },
    /// The grid changed; a new snapshot is needed.
    TickChanged,
    /// A change arrived before any snapshot.
    NoSnapshot,
}

#[derive(Default, Debug)]
pub struct Book {
    bids: BTreeMap<Micro, Micro>,
    asks: BTreeMap<Micro, Micro>,
    pub tick: Micro,
    /// None while usable; Some(reason) until the next snapshot.
    pub broken: Option<Integrity>,
    pub has_snapshot: bool,
}

impl Book {
    pub fn usable(&self) -> bool { self.has_snapshot && self.broken.is_none() }
    pub fn best_bid(&self) -> Option<Micro> { self.bids.keys().next_back().copied() }
    pub fn best_ask(&self) -> Option<Micro> { self.asks.keys().next().copied() }
    /// Bid levels from best to worst (exit valuation walks them).
    pub fn bids(&self) -> impl Iterator<Item = (Micro, Micro)> + '_ { self.bids.iter().rev().map(|(p, s)| (*p, *s)) }
    /// Ask levels from best to worst.
    pub fn asks(&self) -> impl Iterator<Item = (Micro, Micro)> + '_ { self.asks.iter().map(|(p, s)| (*p, *s)) }

    pub fn snapshot(&mut self, bids: &[Level], asks: &[Level], tick: Option<Micro>) -> Result<(), Integrity> {
        self.bids.clear(); self.asks.clear();
        if let Some(t) = tick { self.tick = t; } self.has_snapshot = true; self.broken = None;
        for l in bids { if l.size > 0 { self.bids.insert(l.price, l.size); } }
        for l in asks { if l.size > 0 { self.asks.insert(l.price, l.size); } }
        let r = self.check_grid(bids.iter().chain(asks)).and_then(|_| self.check_crossed());
        if let Err(e) = &r { self.broken = Some(e.clone()); }
        r
    }

    /// Applies one level change. Checks that need the whole message (announced best prices) are done by `verify`.
    pub fn apply(&mut self, side: Side, price: Micro, size: Micro) -> Result<(), Integrity> {
        if !self.has_snapshot { return self.fail(Integrity::NoSnapshot); }
        if let Some(b) = &self.broken { return Err(b.clone()); }
        if self.tick > 0 && price % self.tick != 0 { return self.fail(Integrity::OffTick { price, tick: self.tick }); }
        let m = match side { Side::Bid => &mut self.bids, Side::Ask => &mut self.asks };
        if size > 0 { m.insert(price, size); } else { m.remove(&price); }
        Ok(())
    }

    /// Checks the book against the best prices the source announced once a message is fully applied. `empty_bid` and
    /// `empty_ask` are how the source writes an empty side (a probability book writes 0 and 1).
    pub fn verify(&mut self, best_bid: Option<Micro>, best_ask: Option<Micro>, empty_bid: Micro, empty_ask: Micro) -> Result<(), Integrity> {
        if let Some(b) = &self.broken { return Err(b.clone()); }
        if let Some(b) = best_bid { let ours = self.best_bid(); if ours.unwrap_or(empty_bid) != b { return self.fail(Integrity::Mismatch { side: Side::Bid, ours, theirs: b }); } }
        if let Some(a) = best_ask { let ours = self.best_ask(); if ours.unwrap_or(empty_ask) != a { return self.fail(Integrity::Mismatch { side: Side::Ask, ours, theirs: a }); } }
        self.check_crossed()
    }

    pub fn tick_changed(&mut self, tick: Micro) { self.tick = tick; self.broken = Some(Integrity::TickChanged); }

    fn fail(&mut self, e: Integrity) -> Result<(), Integrity> { self.broken = Some(e.clone()); Err(e) }
    fn check_crossed(&mut self) -> Result<(), Integrity> {
        match (self.best_bid(), self.best_ask()) { (Some(b), Some(a)) if b >= a => self.fail(Integrity::Crossed { bid: b, ask: a }), _ => Ok(()) }
    }
    fn check_grid<'a>(&self, mut levels: impl Iterator<Item = &'a Level>) -> Result<(), Integrity> {
        if self.tick <= 0 { return Ok(()); }
        match levels.find(|l| l.price % self.tick != 0) { Some(l) => Err(Integrity::OffTick { price: l.price, tick: self.tick }), None => Ok(()) }
    }
}

/// Best prices a source announced for an asset, waiting for its event to be complete before they are checked.
#[derive(Debug, Clone, Copy)]
struct Pending { event_ms: u64, best_bid: Option<Micro>, best_ask: Option<Micro>, at_ns: u64 }

/// All books of one source. A source may spread one event over several frames that share its timestamp, and
/// announce the best prices of the complete event in each of them: an asset is checked only once a later timestamp
/// touches it, or after `quiet_ns` without news (measured live: 0 false alarm on 57 018 checks, phase 4).
pub struct Books {
    pub books: std::collections::HashMap<String, Book>,
    pending: std::collections::HashMap<String, Pending>,
    empty_bid: Micro,
    empty_ask: Micro,
    quiet_ns: u64,
}

impl Books {
    pub fn new(empty_bid: Micro, empty_ask: Micro, quiet_ns: u64) -> Self { Books { books: Default::default(), pending: Default::default(), empty_bid, empty_ask, quiet_ns } }

    pub fn get(&self, asset: &str) -> Option<&Book> { self.books.get(asset) }
    pub fn entry(&mut self, asset: &str) -> &mut Book { self.books.entry(asset.to_string()).or_default() }

    pub fn snapshot(&mut self, asset: &str, bids: &[Level], asks: &[Level], tick: Option<Micro>) -> Result<(), Integrity> {
        self.pending.remove(asset);
        self.entry(asset).snapshot(bids, asks, tick)
    }

    pub fn tick_changed(&mut self, asset: &str, tick: Micro) { self.pending.remove(asset); self.entry(asset).tick_changed(tick); }

    /// Forgets an asset (its market is over).
    pub fn remove(&mut self, asset: &str) { self.pending.remove(asset); self.books.remove(asset); }

    /// One message of level changes. Returns the assets whose book broke (each needs a new snapshot).
    pub fn delta(&mut self, event_ms: u64, changes: &[crate::event::Change], now_ns: u64) -> Vec<(String, Integrity)> {
        let mut broken = vec![];
        // first close the events this message ends: their books are complete before these changes
        for c in changes {
            if let Some(p) = self.pending.get(&c.asset).copied() {
                if p.event_ms != event_ms { self.pending.remove(&c.asset); if let Err(e) = self.check(&c.asset, p) { broken.push((c.asset.clone(), e)); } }
            }
        }
        for c in changes {
            let Some(b) = self.books.get_mut(&c.asset) else { continue };
            if !b.usable() { continue; }
            if let Err(e) = b.apply(c.side, c.price, c.size) { self.pending.remove(&c.asset); broken.push((c.asset.clone(), e)); continue; }
            self.pending.insert(c.asset.clone(), Pending { event_ms, best_bid: c.best_bid, best_ask: c.best_ask, at_ns: now_ns });
        }
        broken
    }

    /// Checks the events that went quiet. Call it on a timer.
    pub fn quiet(&mut self, now_ns: u64) -> Vec<(String, Integrity)> {
        let due: Vec<String> = self.pending.iter().filter(|(_, p)| now_ns.saturating_sub(p.at_ns) >= self.quiet_ns).map(|(a, _)| a.clone()).collect();
        let mut broken = vec![];
        for a in due { let p = self.pending.remove(&a).unwrap(); if let Err(e) = self.check(&a, p) { broken.push((a, e)); } }
        broken
    }

    fn check(&mut self, asset: &str, p: Pending) -> Result<(), Integrity> {
        let (eb, ea) = (self.empty_bid, self.empty_ask);
        match self.books.get_mut(asset) { Some(b) if b.usable() => b.verify(p.best_bid, p.best_ask, eb, ea), _ => Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::MICRO;
    use proptest::prelude::*;
    const T: Micro = 10_000; // 0.01

    fn lv(p: Micro, s: Micro) -> Level { Level { price: p, size: s } }

    #[test]
    fn snapshot_then_deltas_track_best_prices() {
        let mut b = Book::default();
        b.snapshot(&[lv(400_000, 5), lv(430_000, 7)], &[lv(440_000, 3), lv(500_000, 1)], Some(T)).unwrap();
        assert_eq!((b.best_bid(), b.best_ask()), (Some(430_000), Some(440_000)));
        b.apply(Side::Bid, 430_000, 0).unwrap();
        b.verify(Some(400_000), Some(440_000), 0, MICRO).unwrap();
        assert_eq!(b.best_bid(), Some(400_000));
        assert!(b.usable());
    }

    #[test]
    fn mismatch_with_announced_best_breaks_until_snapshot() {
        let mut b = Book::default();
        b.snapshot(&[lv(430_000, 7)], &[lv(440_000, 3)], Some(T)).unwrap();
        b.apply(Side::Bid, 420_000, 1).unwrap();
        assert!(matches!(b.verify(Some(420_000), None, 0, MICRO), Err(Integrity::Mismatch { .. })));
        assert!(!b.usable());
        assert!(b.apply(Side::Bid, 410_000, 1).is_err());
        b.snapshot(&[lv(430_000, 7)], &[lv(440_000, 3)], Some(T)).unwrap();
        assert!(b.usable());
    }

    #[test]
    fn crossed_off_tick_no_snapshot_and_tick_change() {
        let mut b = Book::default();
        assert_eq!(b.apply(Side::Bid, 400_000, 1), Err(Integrity::NoSnapshot));
        assert!(matches!(b.snapshot(&[lv(450_000, 1)], &[lv(440_000, 1)], Some(T)), Err(Integrity::Crossed { .. })));
        b.snapshot(&[lv(430_000, 1)], &[lv(440_000, 1)], Some(T)).unwrap();
        assert!(matches!(b.apply(Side::Bid, 431_000, 1), Err(Integrity::OffTick { .. })));
        b.snapshot(&[lv(430_000, 1)], &[lv(440_000, 1)], Some(T)).unwrap();
        b.tick_changed(1_000);
        assert!(!b.usable());
        b.snapshot(&[lv(431_000, 1)], &[lv(440_000, 1)], Some(1_000)).unwrap();
        assert!(b.usable());
    }

    #[test]
    fn empty_sides_written_as_the_source_writes_them() {
        let mut b = Book::default();
        b.snapshot(&[lv(430_000, 1)], &[], Some(T)).unwrap();
        b.apply(Side::Bid, 420_000, 2).unwrap();
        b.verify(Some(430_000), Some(MICRO), 0, MICRO).unwrap(); // empty ask written as 1.00
        assert!(b.usable());
    }

    #[test]
    fn announced_best_is_checked_after_the_whole_message() {
        // one message moves the best bid twice: checking after the first change would be a false alarm
        let mut b = Book::default();
        b.snapshot(&[lv(430_000, 1)], &[lv(440_000, 1)], Some(T)).unwrap();
        b.apply(Side::Bid, 430_000, 0).unwrap();
        b.apply(Side::Bid, 420_000, 3).unwrap();
        b.verify(Some(420_000), Some(440_000), 0, MICRO).unwrap();
        assert!(b.usable());
    }

    fn ch(asset: &str, side: Side, price: Micro, size: Micro, bb: Micro, ba: Micro) -> crate::event::Change {
        crate::event::Change { asset: asset.into(), side, price, size, best_bid: Some(bb), best_ask: Some(ba) }
    }

    #[test]
    fn an_event_split_over_two_frames_is_checked_once_complete() {
        // live case of 2026-10-19: "SELL 0.90 x 48 -> 0.89/0.90" then, same timestamp, "BUY 0.91 x 0 -> 0.89/0.90"
        let mut bs = Books::new(0, MICRO, 200_000_000);
        bs.snapshot("a", &[lv(910_000, 5), lv(890_000, 5)], &[lv(920_000, 5)], Some(T)).unwrap();
        assert!(bs.delta(83, &[ch("a", Side::Ask, 900_000, 48, 890_000, 900_000)], 0).is_empty());
        assert!(bs.delta(83, &[ch("a", Side::Bid, 910_000, 0, 890_000, 900_000)], 1).is_empty());
        // a later timestamp closes the event: the book matches what was announced
        assert!(bs.delta(84, &[ch("a", Side::Bid, 880_000, 1, 890_000, 900_000)], 2).is_empty());
        assert!(bs.quiet(u64::MAX).is_empty());
        assert!(bs.get("a").unwrap().usable());
    }

    #[test]
    fn a_real_divergence_is_caught_when_the_event_closes_or_goes_quiet() {
        let mut bs = Books::new(0, MICRO, 200_000_000);
        bs.snapshot("a", &[lv(910_000, 5)], &[lv(920_000, 5)], Some(T)).unwrap();
        bs.delta(1, &[ch("a", Side::Bid, 900_000, 5, 900_000, 920_000)], 0); // claims 0.90 is best while 0.91 stays
        assert!(bs.quiet(100_000_000).is_empty()); // not quiet yet
        let broken = bs.quiet(250_000_000);
        assert!(matches!(broken[..], [(_, Integrity::Mismatch { side: Side::Bid, ours: Some(910_000), theirs: 900_000 })]));
        assert!(!bs.get("a").unwrap().usable());
    }

    proptest! {
        // invariant: after any sequence of changes the book equals a naive model, and best prices are its extremes
        #[test]
        fn matches_naive_model(ops in proptest::collection::vec((any::<bool>(), 1i64..100, 0i64..5), 0..200)) {
            let mut b = Book::default();
            b.snapshot(&[], &[], Some(T)).unwrap();
            let (mut bids, mut asks) = (std::collections::HashMap::new(), std::collections::HashMap::new());
            for (bid, p, s) in ops {
                // bids below 0.50, asks above: never crossed, so every change must be accepted
                let price = if bid { p * T / 2 } else { 500_000 + p * T };
                let price = price - price % T;
                let (side, m) = if bid { (Side::Bid, &mut bids) } else { (Side::Ask, &mut asks) };
                if s > 0 { m.insert(price, s); } else { m.remove(&price); }
                prop_assert!(b.apply(side, price, s).is_ok());
            }
            prop_assert_eq!(b.best_bid(), bids.keys().max().copied());
            prop_assert_eq!(b.best_ask(), asks.keys().min().copied());
            prop_assert_eq!(b.bids().count(), bids.len());
            let walked: Vec<_> = b.bids().map(|x| x.0).collect();
            prop_assert!(walked.windows(2).all(|w| w[0] > w[1]));
        }
    }
}
