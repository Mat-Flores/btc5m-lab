//! Live value of a position and the stop loss (02-conception.md §6, TODO §4.2). The only figure the engine computes
//! rather than reads: what selling the shares now on the observed book would bring, bids walked from the best, each
//! level net of the market's taker fee. It is labelled latent and carries the age of the book; a position without a
//! usable book is not valued (never guessed). Exact integers, rounded half up to the micro-unit once per amount.
use crate::account::Position;
use crate::book::Book;
use crate::money::{Micro, MICRO};
use serde::{Deserialize, Serialize};

const M: i128 = MICRO as i128;

fn round(num: i128, den: i128) -> Micro { ((num + den / 2) / den) as Micro }

/// Collateral for `size` shares at `price`.
pub fn amount(size: Micro, price: Micro) -> Micro { round(size as i128 * price as i128, M) }

/// Taker fee of the market's schedule: size × rate × p × (1 − p)^e. Exponent ≤ 2 (checked when a market is read),
/// which keeps the exact product within i128 up to 10^12 shares.
pub fn fee(size: Micro, price: Micro, rate: Micro, exponent: u32) -> Micro {
    round(size as i128 * rate as i128 * price as i128 * (M - price as i128).pow(exponent), M.pow(2 + exponent))
}

/// A sale walked on the bids.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Exit {
    /// What the sale would bring, fees deducted.
    pub net: Micro,
    pub fees: Micro,
    /// Shares the book could not take; they count for nothing.
    pub unfilled: Micro,
    /// Levels walked: price, shares taken.
    pub levels: Vec<(Micro, Micro)>,
}

pub fn exit(shares: Micro, bids: impl Iterator<Item = (Micro, Micro)>, rate: Micro, exponent: u32) -> Exit {
    let mut x = Exit { net: 0, fees: 0, unfilled: shares.max(0), levels: vec![] };
    for (p, s) in bids {
        if x.unfilled == 0 { break; }
        let take = s.min(x.unfilled);
        let f = fee(take, p, rate, exponent);
        x.net += amount(take, p) - f;
        x.fees += f;
        x.unfilled -= take;
        x.levels.push((p, take));
    }
    x
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "label", rename_all = "snake_case")]
pub enum Label {
    /// A sale on the observed book, whose age (wall now − the book's last source timestamp) goes with it.
    Latent { book_age_ms: i64 },
    /// The market is resolved: the payout is known, not yet redeemed.
    Settled,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Valuation {
    #[serde(flatten)]
    pub label: Label,
    pub token: String,
    pub shares: Micro,
    pub cost: Micro,
    /// The shares now: sale net of fees, or the payout (one unit of collateral a winning share) once resolved.
    pub value: Micro,
    /// value + sold − cost.
    pub latent: Micro,
    /// Every trade confirmed; else matched by the venue, not yet confirmed.
    pub confirmed: bool,
    pub fees_measured: bool,
    pub exit: Option<Exit>,
    /// Ledger facts the position comes from.
    pub seqs: Vec<u64>,
}

/// Values a held position. None while its market is open and no usable book is known.
pub fn value(p: &Position, book: Option<&Book>, book_age_ms: i64) -> Option<Valuation> {
    let (label, value, exit) = match p.won {
        Some(won) => (Label::Settled, if won { p.shares } else { 0 }, None),
        None => {
            let x = exit(p.shares, book.filter(|b| b.usable())?.bids(), p.fee_rate, p.fee_exponent);
            (Label::Latent { book_age_ms }, x.net, Some(x))
        }
    };
    Some(Valuation { label, token: p.token.clone(), shares: p.shares, cost: p.cost(), value, latent: value + p.sold - p.cost(), confirmed: p.confirmed, fees_measured: p.fees_measured, exit, seqs: p.seqs.clone() })
}

/// Sells a position whose latent result falls to `loss` of its cost, except in the last `quiet_s` of its window
/// (02-conception.md §10, decision 5). Acts from the venue's match, before confirmation (decision 8).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StopLoss {
    /// Millionths of the cost: 500 000 = −50 %.
    pub loss: Micro,
    pub quiet_s: u64,
}

impl Default for StopLoss { fn default() -> Self { StopLoss { loss: 500_000, quiet_s: 60 } } }

impl StopLoss {
    pub fn triggers(&self, v: &Valuation, now_ms: u64, end_ms: u64) -> bool {
        matches!(v.label, Label::Latent { .. }) && v.cost > 0 && now_ms + self.quiet_s * 1000 < end_ms
            && v.latent as i128 * M <= -(self.loss as i128 * v.cost as i128)
    }
}

/// A stop loss decision with every input needed to replay it.
#[derive(Clone, Debug, Serialize)]
pub struct Trigger { pub rule: StopLoss, pub at_ms: u64, pub end_ms: u64, pub valuation: Valuation }

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn examples() {
        // 10 shares at 0.56, fee 0.07 x p(1-p): 5.6 + 0.17248
        assert_eq!(amount(10_000_000, 560_000), 5_600_000);
        assert_eq!(fee(10_000_000, 560_000, 70_000, 1), 172_480);
        assert_eq!(fee(1, 500_000, 70_000, 1), 0); // 0.0175 micro rounds to 0
        assert_eq!(fee(100, 500_000, 70_000, 1), 2); // 1.75 rounds to 2
        let x = exit(10_000_000, [(300_000, 4_000_000), (290_000, 100_000_000)].into_iter(), 70_000, 1);
        assert_eq!(x.levels, vec![(300_000, 4_000_000), (290_000, 6_000_000)]);
        assert_eq!(x.net, 1_200_000 - 58_800 + 1_740_000 - 86_478);
        assert_eq!(x.unfilled, 0);
        assert_eq!(exit(5, std::iter::empty(), 70_000, 1), Exit { net: 0, fees: 0, unfilled: 5, levels: vec![] });
    }

    proptest! {
        // a sale never brings more than its gross, never takes more shares than held or offered, and walking the
        // book in two halves never beats walking it once (no rounding gain from splitting)
        #[test]
        fn exit_is_bounded(shares in 0i64..1_000_000_000, levels in proptest::collection::vec((1i64..1_000, 0i64..1_000_000_000), 0..20), rate in 0i64..200_000) {
            let mut bids: Vec<(Micro, Micro)> = levels.into_iter().map(|(p, s)| (p * 1_000, s)).collect();
            bids.sort_by(|a, b| b.0.cmp(&a.0));
            bids.dedup_by_key(|l| l.0);
            let x = exit(shares, bids.iter().copied(), rate, 1);
            let taken: Micro = x.levels.iter().map(|l| l.1).sum();
            prop_assert_eq!(taken + x.unfilled, shares);
            prop_assert!(taken <= bids.iter().map(|l| l.1).sum::<Micro>());
            let gross: Micro = x.levels.iter().map(|&(p, s)| amount(s, p)).sum();
            prop_assert!(x.net <= gross && x.net >= 0 && x.fees >= 0);
            prop_assert!(x.levels.windows(2).all(|w| w[0].0 > w[1].0));
        }
        // the fee is the exact product rounded: within half a micro-unit of the real value, and never negative
        #[test]
        fn fee_is_exact(size in 0i64..1_000_000_000_000, p in 0i64..=1_000_000, rate in 0i64..1_000_000, e in 0u32..=2) {
            let f = fee(size, p, rate, e) as f64;
            let real = size as f64 / 1e6 * rate as f64 / 1e6 * p as f64 / 1e6 * (1.0 - p as f64 / 1e6).powi(e as i32) * 1e6;
            prop_assert!((f - real).abs() <= 0.5 + real * 1e-12);
        }
    }

    fn val(latent: Micro, cost: Micro) -> Valuation {
        Valuation { label: Label::Latent { book_age_ms: 10 }, token: "t".into(), shares: 1, cost, value: cost + latent, latent, confirmed: false, fees_measured: false, exit: None, seqs: vec![] }
    }

    #[test]
    fn stop_loss_at_half_the_cost_outside_the_last_minute() {
        let s = StopLoss::default();
        assert!(s.triggers(&val(-2_000_000, 4_000_000), 0, 61_000));
        assert!(!s.triggers(&val(-1_999_999, 4_000_000), 0, 61_000));
        assert!(!s.triggers(&val(-3_000_000, 4_000_000), 1_000, 61_000)); // last minute
        assert!(!s.triggers(&Valuation { label: Label::Settled, ..val(-4_000_000, 4_000_000) }, 0, 61_000));
    }
}
