//! Simulated execution (paper mode, 02-conception.md §10 question 3; 00-recherche.md §5.9): a marketable order is
//! held for the market's taker delay, then matched FAK against the book observed at that instant. Exact integers, as
//! the accounting computes amounts. Every figure it produces is simulated, never the venue's.
use engine_core::book::Book;
use engine_core::money::{Micro, MICRO};
use engine_core::value::amount;

/// Hold Polymarket announces for marketable orders on markets flagged `itode` (changelog, 2026-09-04); not measured
/// on our own orders yet (phase 10).
pub const TAKER_DELAY_MS: u64 = 150;

/// FAK buy: spends up to `spend` on the asks at or below `limit`, best first. (price, shares) per level taken.
pub fn buy(book: &Book, spend: Micro, limit: Micro) -> Vec<(Micro, Micro)> {
    let mut left = spend;
    let mut out = vec![];
    for (p, s) in book.asks() {
        if p > limit || p <= 0 || left <= 0 { break; }
        // take × p ≤ left × 1e6, so the rounded amount never exceeds what is left
        let take = s.min((left as i128 * MICRO as i128 / p as i128) as Micro);
        if take <= 0 { break; }
        left -= amount(take, p);
        out.push((p, take));
    }
    out
}

/// FAK sell: sells up to `shares` on the bids at or above `limit`, best first.
pub fn sell(book: &Book, shares: Micro, limit: Micro) -> Vec<(Micro, Micro)> {
    let mut left = shares;
    let mut out = vec![];
    for (p, s) in book.bids() {
        if p < limit || left <= 0 { break; }
        let take = s.min(left);
        left -= take;
        out.push((p, take));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::event::Level;

    #[test]
    fn fak_walks_the_book_within_the_limit() {
        let mut b = Book::default();
        let l = |price, size| Level { price, size };
        b.snapshot(&[l(400_000, 50_000_000), l(450_000, 3_000_000)], &[l(500_000, 10_000_000), l(510_000, 100_000_000), l(600_000, 1_000_000)], Some(10_000)).unwrap();
        // 10 $: 10 shares at 0.50 (5 $), then 5 / 0.51 = 9.803921 shares at 0.51; 0.60 is above the limit
        assert_eq!(buy(&b, 10_000_000, 510_000), vec![(500_000, 10_000_000), (510_000, 9_803_921)]);
        assert_eq!(buy(&b, 10_000_000, 490_000), vec![]);
        assert_eq!(sell(&b, 5_000_000, 420_000), vec![(450_000, 3_000_000)]);
        assert_eq!(sell(&b, 60_000_000, 0), vec![(450_000, 3_000_000), (400_000, 50_000_000)]);
    }
}
