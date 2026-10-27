//! Reconciliation of the ledger with the venue (TODO §9 phase 7, 02-conception.md §2.3): shares per token, open orders
//! and the collateral balance. Only an agreement lets openings go; a disagreement blocks them and raises the alert,
//! and so does anything that cannot be compared yet (pending), in doubt.
use crate::account::Account;
use crate::money::Micro;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// What the venue says, read off the hot path. `None`: not read (no credentials, or the read failed).
#[derive(Clone, Debug, Default)]
pub struct Venue {
    /// Shares per token, as the venue lists them.
    pub shares: Option<BTreeMap<String, Micro>>,
    pub balance: Option<Micro>,
    pub open_orders: Option<BTreeSet<String>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "gap", rename_all = "snake_case")]
pub enum Gap {
    Shares { token: String, ledger: Micro, venue: Micro },
    Balance { expected: Micro, venue: Micro },
    /// An order open at the venue that the engine does not know as open.
    Order { id: String },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum Verdict { Agree, Pending { why: String }, Disagree { gaps: Vec<Gap> } }

impl Verdict { pub fn agrees(&self) -> bool { *self == Verdict::Agree } }

/// `ours_open`: orders the engine knows as resting at the venue. The balance is compared only when every trade is
/// final, the venue's fees are read, and the balance was read after the last money fact (`balance_seq`).
pub fn reconcile(a: &Account, v: &Venue, ours_open: &BTreeSet<String>) -> Verdict {
    let mut gaps = vec![];
    let Some(shares) = &v.shares else { return Verdict::Pending { why: "positions not read".into() } };
    let ours = a.shares();
    for token in ours.keys().chain(shares.keys()).collect::<BTreeSet<_>>() {
        let (l, r) = (ours.get(token).copied().unwrap_or(0), shares.get(token).copied().unwrap_or(0));
        if l != r { gaps.push(Gap::Shares { token: token.clone(), ledger: l, venue: r }); }
    }
    let Some(open) = &v.open_orders else { return pending_or(gaps, "open orders not read") };
    gaps.extend(open.difference(ours_open).map(|id| Gap::Order { id: id.clone() }));
    let Some(balance) = v.balance else { return pending_or(gaps, "balance not read") };
    if let Err(why) = a.settled() { return pending_or(gaps, &why); }
    // the venue's balance must be the one recorded last, and recorded after every money fact
    match (a.balance, a.last_flow_seq) {
        (Some((b, s)), flow) if b == balance && flow.is_none_or(|f| f < s) => {}
        _ => return pending_or(gaps, "balance not recorded after the last trade"),
    }
    match a.expected_balance() {
        Some(e) if e != balance => gaps.push(Gap::Balance { expected: e, venue: balance }),
        Some(_) => {}
        None => return pending_or(gaps, "no anchor balance"),
    }
    if gaps.is_empty() { Verdict::Agree } else { Verdict::Disagree { gaps } }
}

fn pending_or(gaps: Vec<Gap>, why: &str) -> Verdict { if gaps.is_empty() { Verdict::Pending { why: why.into() } } else { Verdict::Disagree { gaps } } }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{Dir, Fact};
    use crate::order::TradeStatus;

    fn trade(status: TradeStatus, size: Micro) -> Fact {
        Fact::Trade { trade: "t1".into(), order: "o".into(), market: "m".into(), token: "up".into(), dir: Dir::Buy, status, size, price: 500_000, fee_rate: 70_000, fee_exponent: 1, end_ms: 0 }
    }
    fn venue(shares: Micro, balance: Micro) -> Venue {
        Venue { shares: Some([("up".to_string(), shares)].into()), balance: Some(balance), open_orders: Some(BTreeSet::new()) }
    }

    #[test]
    fn agreement_needs_every_figure() {
        let mut a = Account::default();
        a.apply(0, &Fact::Balance { amount: 50_000_000 });
        a.apply(1, &trade(TradeStatus::Matched, 10_000_000));
        assert_eq!(reconcile(&a, &Venue::default(), &BTreeSet::new()), Verdict::Pending { why: "positions not read".into() });
        assert!(matches!(reconcile(&a, &venue(10_000_000, 50_000_000), &BTreeSet::new()), Verdict::Pending { .. })); // trade not final
        a.apply(2, &trade(TradeStatus::Confirmed, 0));
        a.apply(3, &Fact::Fees { token: "up".into(), amount: 175_000 });
        // the balance read before the trade settled says nothing yet
        assert!(matches!(reconcile(&a, &venue(10_000_000, 50_000_000), &BTreeSet::new()), Verdict::Pending { .. }));
        a.apply(4, &Fact::Balance { amount: 44_825_000 });
        assert_eq!(reconcile(&a, &venue(10_000_000, 44_825_000), &BTreeSet::new()), Verdict::Agree);
    }

    #[test]
    fn every_gap_is_named() {
        let mut a = Account::default();
        a.apply(0, &Fact::Balance { amount: 50_000_000 });
        a.apply(1, &trade(TradeStatus::Confirmed, 10_000_000));
        a.apply(2, &Fact::Fees { token: "up".into(), amount: 175_000 });
        a.apply(3, &Fact::Balance { amount: 44_800_000 });
        let mut v = venue(9_000_000, 44_800_000);
        v.shares.as_mut().unwrap().insert("other".into(), 1);
        v.open_orders = Some(["0xstranger".to_string()].into());
        assert_eq!(reconcile(&a, &v, &BTreeSet::new()), Verdict::Disagree { gaps: vec![
            Gap::Shares { token: "other".into(), ledger: 0, venue: 1 },
            Gap::Shares { token: "up".into(), ledger: 10_000_000, venue: 9_000_000 },
            Gap::Order { id: "0xstranger".into() },
            Gap::Balance { expected: 44_825_000, venue: 44_800_000 },
        ] });
        // a share gap is reported even while the balance cannot be compared yet
        assert!(matches!(reconcile(&a, &Venue { balance: None, ..v }, &BTreeSet::new()), Verdict::Disagree { .. }));
    }
}
