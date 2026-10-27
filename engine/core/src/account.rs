//! Accounting from ledger facts only (TODO §4, objective 4). Positions, costs, fees, settlements and results are
//! derived from what the venue reported: its trades (size and price), its fees, its balance, its official results and
//! the redemptions seen at it. Every figure keeps the seqs of the facts it comes from. Two figures are computed:
//! the entry fees, estimated from the market's fee schedule until the venue's are read (02-conception.md §3.6), and
//! the latent values (`value.rs`). A trade whose size the venue has not given yet is not a holding.
use crate::money::Micro;
use crate::order::TradeStatus;
use crate::value::{amount, fee};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dir { Buy, Sell }

/// Facts the accounting reads. `market` is the venue's market id, `token` the traded outcome.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Fact {
    /// A trade of one of our orders as the venue reported it; a later fact on the same `trade` updates it. The fee
    /// schedule and the window end are the market's, read with it.
    Trade { trade: String, order: String, market: String, token: String, dir: Dir, status: TradeStatus, size: Micro, price: Micro, fee_rate: Micro, fee_exponent: u32, end_ms: u64 },
    /// Entry fees the venue attributes to a token, cumulative.
    Fees { token: String, amount: Micro },
    /// Collateral balance read at the venue.
    Balance { amount: Micro },
    /// Official result of a market.
    Resolved { market: String, winner: String },
    /// A redemption seen at the venue, with its transaction (one per transaction).
    Redeemed { market: String, amount: Micro, tx: String },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Position {
    pub market: String,
    pub token: String,
    /// Bought − sold, failed trades excluded.
    pub shares: Micro,
    /// size × price of each buy.
    pub paid: Micro,
    /// The venue's once read (`fees_measured`), else estimated from the fee schedule.
    pub fees: Micro,
    pub fees_measured: bool,
    /// size × price of each sell, minus its estimated taker fee.
    pub sold: Micro,
    /// Every trade confirmed; else matched, not confirmed.
    pub confirmed: bool,
    pub won: Option<bool>,
    pub fee_rate: Micro,
    pub fee_exponent: u32,
    pub end_ms: u64,
    pub seqs: Vec<u64>,
}

impl Position { pub fn cost(&self) -> Micro { self.paid + self.fees } }

/// A closed market: everything it brought (sales, redemptions) minus everything it cost.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Realized { pub market: String, pub amount: Micro, pub seqs: Vec<u64> }

#[derive(Clone, Debug)]
struct Trade { market: String, token: String, dir: Dir, status: TradeStatus, size: Micro, price: Micro, fee_rate: Micro, fee_exponent: u32, end_ms: u64, seqs: Vec<u64> }

#[derive(Default, Debug)]
pub struct Account {
    trades: BTreeMap<String, Trade>,
    fees: BTreeMap<String, (Micro, u64)>,
    resolved: BTreeMap<String, (String, u64)>,
    redeemed: BTreeMap<String, (Micro, u64)>,
    redeem_txs: std::collections::BTreeSet<String>,
    /// Last balance read, with its seq; the first one is the anchor of the expected balance.
    pub balance: Option<(Micro, u64)>,
    pub anchor: Option<(Micro, u64)>,
    /// Derived after each fact.
    pub open: BTreeMap<String, Position>,
    pub realized: Vec<Realized>,
    /// Seq of the last fact that moves money (trade, fees, redemption).
    pub last_flow_seq: Option<u64>,
}

impl Account {
    pub fn fold<'a>(facts: impl IntoIterator<Item = (u64, &'a Fact)>) -> Account {
        let mut a = Account::default();
        for (seq, f) in facts { a.apply(seq, f); }
        a
    }

    /// Applies one fact. False: nothing changed (a duplicate, an older trade status).
    pub fn apply(&mut self, seq: u64, f: &Fact) -> bool {
        match f {
            Fact::Trade { trade, market, token, dir, status, size, price, fee_rate, fee_exponent, end_ms, .. } => {
                let t = self.trades.entry(trade.clone()).or_insert_with(|| Trade { market: market.clone(), token: token.clone(), dir: *dir, status: *status, size: 0, price: 0, fee_rate: *fee_rate, fee_exponent: *fee_exponent, end_ms: *end_ms, seqs: vec![] });
                // like the order machine: a status never goes back, and a size is the venue's last non-zero one
                if *status < t.status || (*status == t.status && (*size == 0 || (*size, *price) == (t.size, t.price)) && !t.seqs.is_empty()) { return false; }
                t.status = *status;
                if *size != 0 { t.size = *size; t.price = *price; }
                t.seqs.push(seq);
                self.last_flow_seq = Some(seq);
            }
            Fact::Fees { token, amount } => {
                if self.fees.get(token).is_some_and(|f| f.0 == *amount) { return false; }
                self.fees.insert(token.clone(), (*amount, seq));
                self.last_flow_seq = Some(seq);
            }
            Fact::Balance { amount } => {
                self.balance = Some((*amount, seq));
                self.anchor.get_or_insert((*amount, seq));
                return true;
            }
            Fact::Resolved { market, winner } => {
                if self.resolved.contains_key(market) { return false; }
                self.resolved.insert(market.clone(), (winner.clone(), seq));
            }
            Fact::Redeemed { market, amount, tx } => {
                if !self.redeem_txs.insert(tx.clone()) { return false; }
                let r = self.redeemed.entry(market.clone()).or_insert((0, seq));
                *r = (r.0 + amount, seq);
                self.last_flow_seq = Some(seq);
            }
        }
        self.derive();
        true
    }

    fn derive(&mut self) {
        let mut by_token: BTreeMap<&str, Position> = BTreeMap::new();
        let mut estimated: BTreeMap<&str, Micro> = BTreeMap::new();
        let mut effective: std::collections::BTreeSet<&str> = Default::default();
        for t in self.trades.values() {
            let p = by_token.entry(&t.token).or_insert_with(|| Position { market: t.market.clone(), token: t.token.clone(), shares: 0, paid: 0, fees: 0, fees_measured: false, sold: 0, confirmed: true,
                won: None, fee_rate: t.fee_rate, fee_exponent: t.fee_exponent, end_ms: t.end_ms, seqs: vec![] });
            p.seqs.extend(&t.seqs);
            // a failed trade never happened; a trade without the venue's size is not a holding yet
            if t.status == TradeStatus::Failed || t.size == 0 { continue; }
            effective.insert(&t.token);
            p.confirmed &= t.status == TradeStatus::Confirmed;
            match t.dir {
                Dir::Buy => { p.shares += t.size; p.paid += amount(t.size, t.price); *estimated.entry(&t.token).or_default() += fee(t.size, t.price, t.fee_rate, t.fee_exponent); }
                Dir::Sell => { p.shares -= t.size; p.sold += amount(t.size, t.price) - fee(t.size, t.price, t.fee_rate, t.fee_exponent); }
            }
        }
        let mut markets: BTreeMap<String, Vec<Position>> = BTreeMap::new();
        // a token whose every trade failed or has no size yet is neither held nor closed; its facts stay in the ledger
        for (token, mut p) in by_token.into_iter().filter(|(t, _)| effective.contains(t)) {
            match self.fees.get(token) {
                Some(&(f, seq)) => { p.fees = f; p.fees_measured = true; p.seqs.push(seq); }
                None => p.fees = estimated.get(token).copied().unwrap_or(0),
            }
            if let Some((winner, seq)) = self.resolved.get(&p.market) { p.won = Some(*winner == p.token); p.seqs.push(*seq); }
            if p.seqs.len() > 1 { p.seqs.sort_unstable(); p.seqs.dedup(); }
            markets.entry(p.market.clone()).or_default().push(p);
        }
        self.open.clear();
        self.realized.clear();
        for (market, ps) in markets {
            let all_confirmed = ps.iter().all(|p| p.confirmed);
            let redeemed = self.redeemed.get(&market);
            // closed: redeemed at the venue, or every share sold and confirmed
            if redeemed.is_some() || (all_confirmed && ps.iter().all(|p| p.shares == 0)) {
                let mut seqs: Vec<u64> = ps.iter().flat_map(|p| p.seqs.iter().copied()).chain(redeemed.map(|r| r.1)).collect();
                seqs.sort_unstable();
                seqs.dedup();
                let amount = redeemed.map_or(0, |r| r.0) + ps.iter().map(|p| p.sold - p.cost()).sum::<Micro>();
                self.realized.push(Realized { market, amount, seqs });
            } else {
                for p in ps { self.open.insert(p.token.clone(), p); }
            }
        }
    }

    pub fn traded(&self, market: &str) -> bool { self.trades.values().any(|t| t.market == market) }

    pub fn realized_total(&self) -> Micro { self.realized.iter().map(|r| r.amount).sum() }

    /// Every trade final (confirmed or failed) and every bought token's fees read from the venue: only then can the
    /// expected balance be compared with the venue's.
    pub fn settled(&self) -> Result<(), String> {
        if let Some((id, _)) = self.trades.iter().find(|(_, t)| !matches!(t.status, TradeStatus::Confirmed | TradeStatus::Failed)) { return Err(format!("trade {id} not final")); }
        if let Some(t) = self.trades.values().find(|t| t.dir == Dir::Buy && t.status == TradeStatus::Confirmed && !self.fees.contains_key(&t.token)) { return Err(format!("fees of {} not read", t.token)); }
        Ok(())
    }

    /// The anchor balance plus every flow since: buys (paid and fees), sales (net of estimated fees), redemptions.
    /// Assumes the anchor was read before any trade (startup order, 02-conception.md §2.3).
    pub fn expected_balance(&self) -> Option<Micro> {
        let (anchor, _) = self.anchor?;
        let mut paid_by_token: BTreeMap<&str, Micro> = BTreeMap::new();
        let mut v = anchor;
        for t in self.trades.values().filter(|t| t.status == TradeStatus::Confirmed) {
            match t.dir {
                Dir::Buy => { let a = amount(t.size, t.price); v -= a; *paid_by_token.entry(&t.token).or_default() += a; }
                Dir::Sell => v += amount(t.size, t.price) - fee(t.size, t.price, t.fee_rate, t.fee_exponent),
            }
        }
        for token in paid_by_token.keys() { v -= self.fees.get(*token).map_or(0, |f| f.0); }
        Some(v + self.redeemed.values().map(|r| r.0).sum::<Micro>())
    }

    /// Shares held per token, closed markets included (0 after a redemption), for comparison with the venue.
    pub fn shares(&self) -> BTreeMap<String, Micro> {
        let mut out: BTreeMap<String, Micro> = BTreeMap::new();
        for t in self.trades.values().filter(|t| t.status != TradeStatus::Failed) {
            let s = out.entry(t.token.clone()).or_default();
            if self.redeemed.contains_key(&t.market) { continue; }
            *s += match t.dir { Dir::Buy => t.size, Dir::Sell => -t.size };
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn trade(id: &str, token: &str, dir: Dir, status: TradeStatus, size: Micro, price: Micro) -> Fact {
        Fact::Trade { trade: id.into(), order: "o".into(), market: "m".into(), token: token.into(), dir, status, size, price, fee_rate: 70_000, fee_exponent: 1, end_ms: 1_000_000 }
    }

    #[test]
    fn a_trade_becomes_a_position_at_the_venues_size_and_price() {
        let mut a = Account::default();
        a.apply(0, &Fact::Balance { amount: 100_000_000 });
        // the answer gave a trade id without its size: no holding yet
        a.apply(1, &trade("t1", "up", Dir::Buy, TradeStatus::Matched, 0, 0));
        assert!(a.open.is_empty());
        a.apply(2, &trade("t1", "up", Dir::Buy, TradeStatus::Matched, 10_000_000, 560_000));
        let p = &a.open["up"];
        assert_eq!((p.shares, p.paid, p.fees, p.fees_measured, p.confirmed), (10_000_000, 5_600_000, 172_480, false, false));
        assert_eq!(p.seqs, vec![1, 2]);
        // a duplicate changes nothing; the confirmation and the venue's fees replace the estimate
        assert!(!a.apply(3, &trade("t1", "up", Dir::Buy, TradeStatus::Matched, 10_000_000, 560_000)));
        a.apply(4, &trade("t1", "up", Dir::Buy, TradeStatus::Confirmed, 0, 0));
        a.apply(5, &Fact::Fees { token: "up".into(), amount: 172_500 });
        let p = &a.open["up"];
        assert_eq!((p.paid, p.fees, p.fees_measured, p.confirmed, p.cost()), (5_600_000, 172_500, true, true, 5_772_500));
        assert_eq!(p.seqs, vec![1, 2, 4, 5]);
        assert_eq!(a.settled(), Ok(()));
        assert_eq!(a.expected_balance(), Some(100_000_000 - 5_772_500));
    }

    #[test]
    fn failed_trades_vanish_and_a_sale_closes_the_market() {
        let mut a = Account::default();
        a.apply(0, &trade("t1", "up", Dir::Buy, TradeStatus::Matched, 10_000_000, 500_000));
        a.apply(1, &trade("t1", "up", Dir::Buy, TradeStatus::Failed, 0, 0));
        assert!(a.open.is_empty() && a.realized.is_empty());
        let mut a = Account::default();
        a.apply(0, &trade("t1", "up", Dir::Buy, TradeStatus::Confirmed, 10_000_000, 500_000));
        a.apply(1, &trade("t2", "up", Dir::Sell, TradeStatus::Matched, 10_000_000, 300_000));
        assert_eq!(a.open["up"].shares, 0); // sold, not yet confirmed: still open, nothing realized
        assert!(a.realized.is_empty());
        a.apply(2, &trade("t2", "up", Dir::Sell, TradeStatus::Confirmed, 0, 0));
        assert!(a.open.is_empty());
        assert_eq!(a.realized, vec![Realized { market: "m".into(), amount: (3_000_000 - 147_000) - (5_000_000 + 175_000), seqs: vec![0, 1, 2] }]);
    }

    #[test]
    fn resolution_settles_and_the_redemption_realizes() {
        let mut a = Account::default();
        a.apply(0, &trade("t1", "up", Dir::Buy, TradeStatus::Confirmed, 10_000_000, 500_000));
        a.apply(1, &Fact::Fees { token: "up".into(), amount: 175_000 });
        a.apply(2, &Fact::Resolved { market: "m".into(), winner: "up".into() });
        assert_eq!(a.open["up"].won, Some(true));
        assert!(a.realized.is_empty());
        a.apply(3, &Fact::Redeemed { market: "m".into(), amount: 10_000_000, tx: "0xt".into() });
        assert!(!a.apply(4, &Fact::Redeemed { market: "m".into(), amount: 10_000_000, tx: "0xt".into() }));
        assert_eq!(a.realized, vec![Realized { market: "m".into(), amount: 10_000_000 - 5_175_000, seqs: vec![0, 1, 2, 3] }]);
        assert_eq!(a.shares()["up"], 0);
    }

    #[test]
    fn balance_is_compared_only_once_everything_is_final() {
        let mut a = Account::default();
        a.apply(0, &Fact::Balance { amount: 50_000_000 });
        a.apply(1, &trade("t1", "up", Dir::Buy, TradeStatus::Mined, 10_000_000, 500_000));
        assert!(a.settled().is_err());
        a.apply(2, &trade("t1", "up", Dir::Buy, TradeStatus::Confirmed, 0, 0));
        assert_eq!(a.settled(), Err("fees of up not read".into()));
        a.apply(3, &Fact::Fees { token: "up".into(), amount: 175_000 });
        a.apply(4, &Fact::Balance { amount: 44_825_000 });
        assert_eq!((a.expected_balance(), a.balance, a.anchor), (Some(44_825_000), Some((44_825_000, 4)), Some((50_000_000, 0))));
    }
}
