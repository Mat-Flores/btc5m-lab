//! Order state machine (02-conception.md §3.5), pure: decided → signed → sent → acked → matched → confirmed, with the
//! outcomes refused, killed (FAK without a match), failed (every trade failed after matching) and unknown (no answer
//! in time, or connection lost after the write). Each transition is driven by one measured fact, given as an `Input`.
//! There is no way to send an order twice: the machine has no resend transition, and an unknown order blocks every
//! new opening until its fate is learnt (a late answer, a trade of the user channel, or a query).
use crate::money::Micro;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Phase { Decided, Signed, Sent, Acked, Matched, Confirmed, Refused, Killed, Failed, Unknown }

/// What the venue said the order became when it accepted it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum AckStatus { Live, Matched, Delayed }

/// Life of one trade after matching. Ordered: a status never goes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TradeStatus { Matched, Mined, Retrying, Confirmed, Failed }

impl TradeStatus { fn is_final(self) -> bool { matches!(self, TradeStatus::Confirmed | TradeStatus::Failed) } }

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Input {
    Signed,
    /// Last byte written on the socket.
    Sent,
    Accepted { id: String, status: AckStatus, trades: Vec<String> },
    /// `killed`: a fill-and-kill order that found nothing to match.
    Rejected { killed: bool, reason: String },
    /// No answer before the deadline, or the connection broke after the write.
    NoAnswer,
    /// A trade of this order, with the venue's own size and price.
    Trade { id: String, status: TradeStatus, size: Micro, price: Micro },
    /// The venue closed the order (delayed or partially filled order done): nothing more will match.
    Closed,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Trade { pub status: TradeStatus, pub size: Micro, pub price: Micro }

#[derive(Debug, PartialEq, Eq)]
pub struct Invalid { pub phase: Phase, pub input: &'static str }

#[derive(Clone, Debug, Serialize)]
pub struct Order {
    pub phase: Phase,
    /// The venue's order id, once known.
    pub id: Option<String>,
    pub trades: BTreeMap<String, Trade>,
    closed: bool,
}

impl Default for Order { fn default() -> Self { Order { phase: Phase::Decided, id: None, trades: BTreeMap::new(), closed: false } } }

impl Order {
    /// Applies one fact. Ok(true): the phase changed. Ok(false): nothing new (a duplicate, an older status).
    /// Err: a fact that cannot happen in this phase; the order is left as it was.
    pub fn apply(&mut self, input: Input) -> Result<bool, Invalid> {
        use Phase::*;
        let before = self.phase;
        let bad = |input: &'static str| Err(Invalid { phase: before, input });
        match input {
            Input::Signed => match before { Decided => self.phase = Signed, _ => return bad("signed") },
            Input::Sent => match before { Signed => self.phase = Sent, _ => return bad("sent") },
            // a late answer settles an unknown order like a timely one
            Input::Accepted { id, status, trades } => match before {
                Sent | Unknown => {
                    if self.id.as_ref().is_some_and(|x| *x != id) { return bad("accepted with another id"); }
                    self.id = Some(id);
                    for t in trades { self.trades.entry(t).or_insert(Trade { status: TradeStatus::Matched, size: 0, price: 0 }); }
                    self.phase = if status == AckStatus::Matched || !self.trades.is_empty() { Matched } else { Acked };
                    self.settle();
                }
                // the user channel may tell the match before the answer comes
                Matched | Confirmed | Failed if self.id.is_none() || self.id.as_ref() == Some(&id) => { self.id = Some(id); return Ok(false); }
                _ => return bad("accepted"),
            },
            Input::Rejected { killed, .. } => match before {
                Sent | Unknown => self.phase = if killed { Killed } else { Refused },
                // refused before the first byte left (network guard, no connection)
                Signed if !killed => self.phase = Refused,
                _ => return bad("rejected"),
            },
            Input::NoAnswer => match before { Sent => self.phase = Unknown, Unknown => {} _ => return bad("no answer") },
            Input::Trade { id, status, size, price } => match before {
                Sent | Unknown | Acked | Matched | Confirmed | Failed => {
                    let t = self.trades.entry(id).or_insert(Trade { status, size, price });
                    if status < t.status { return Ok(false); }
                    if size != 0 { t.size = size; t.price = price; }
                    t.status = status;
                    if matches!(before, Sent | Unknown | Acked) { self.phase = Matched; }
                    self.settle();
                }
                _ => return bad("trade"),
            },
            Input::Closed => match before {
                Acked if self.trades.is_empty() => self.phase = Killed,
                Acked | Matched | Confirmed | Failed => { self.closed = true; self.settle(); }
                _ => return bad("closed"),
            },
        }
        Ok(self.phase != before)
    }

    /// Matched becomes final once every trade is: confirmed if one was, failed if all failed.
    fn settle(&mut self) {
        if !matches!(self.phase, Phase::Matched | Phase::Confirmed | Phase::Failed) || self.trades.is_empty() { return; }
        if self.trades.values().all(|t| t.status.is_final()) {
            self.phase = if self.trades.values().any(|t| t.status == TradeStatus::Confirmed) { Phase::Confirmed } else { Phase::Failed };
        } else { self.phase = Phase::Matched; }
    }

    /// While true, no new order may open: its fate is not known.
    pub fn blocks_openings(&self) -> bool { matches!(self.phase, Phase::Signed | Phase::Sent | Phase::Unknown) }

    /// Shares (or pUSD) matched so far, per the venue, failed trades excluded.
    pub fn matched_size(&self) -> Micro { self.trades.values().filter(|t| t.status != TradeStatus::Failed).map(|t| t.size).sum() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Phase::*;

    fn sent() -> Order { let mut o = Order::default(); o.apply(Input::Signed).unwrap(); o.apply(Input::Sent).unwrap(); o }
    fn acc(status: AckStatus, trades: &[&str]) -> Input { Input::Accepted { id: "0xabc".into(), status, trades: trades.iter().map(|t| t.to_string()).collect() } }
    fn trade(id: &str, status: TradeStatus) -> Input { Input::Trade { id: id.into(), status, size: 9_091_000, price: 550_000 } }

    #[test]
    fn success_path() {
        let mut o = sent();
        assert!(o.blocks_openings());
        assert_eq!(o.apply(acc(AckStatus::Matched, &["t1"])), Ok(true));
        assert_eq!(o.phase, Matched);
        assert!(!o.blocks_openings());
        o.apply(trade("t1", TradeStatus::Mined)).unwrap();
        assert_eq!(o.phase, Matched);
        assert_eq!(o.apply(trade("t1", TradeStatus::Confirmed)), Ok(true));
        assert_eq!((o.phase, o.matched_size()), (Confirmed, 9_091_000));
    }

    #[test]
    fn partial_fill_two_trades_one_fails() {
        let mut o = sent();
        o.apply(acc(AckStatus::Matched, &["t1", "t2"])).unwrap();
        o.apply(trade("t1", TradeStatus::Confirmed)).unwrap();
        assert_eq!(o.phase, Matched);
        o.apply(trade("t2", TradeStatus::Failed)).unwrap();
        assert_eq!((o.phase, o.matched_size()), (Confirmed, 9_091_000));
    }

    #[test]
    fn every_trade_failed_is_failed() {
        let mut o = sent();
        o.apply(acc(AckStatus::Matched, &["t1"])).unwrap();
        o.apply(trade("t1", TradeStatus::Failed)).unwrap();
        assert_eq!((o.phase, o.matched_size()), (Failed, 0));
    }

    #[test]
    fn killed_refused_and_delayed_then_closed() {
        let mut o = sent();
        o.apply(Input::Rejected { killed: true, reason: "no match".into() }).unwrap();
        assert_eq!(o.phase, Killed);
        let mut o = sent();
        o.apply(Input::Rejected { killed: false, reason: "400".into() }).unwrap();
        assert_eq!(o.phase, Refused);
        let mut o = Order::default();
        o.apply(Input::Signed).unwrap();
        o.apply(Input::Rejected { killed: false, reason: "network guard".into() }).unwrap();
        assert_eq!((o.phase, o.blocks_openings()), (Refused, false));
        let mut o = sent();
        o.apply(acc(AckStatus::Delayed, &[])).unwrap();
        assert_eq!(o.phase, Acked);
        o.apply(Input::Closed).unwrap();
        assert_eq!(o.phase, Killed);
    }

    #[test]
    fn no_answer_is_unknown_and_blocks_until_learnt() {
        let mut o = sent();
        o.apply(Input::NoAnswer).unwrap();
        assert_eq!(o.phase, Unknown);
        assert!(o.blocks_openings());
        // a late answer settles it
        o.apply(acc(AckStatus::Matched, &["t1"])).unwrap();
        assert_eq!(o.phase, Matched);
        // or a trade of the user channel, before any answer
        let mut o = sent();
        o.apply(Input::NoAnswer).unwrap();
        o.apply(trade("t9", TradeStatus::Matched)).unwrap();
        assert_eq!(o.phase, Matched);
        assert_eq!(o.apply(acc(AckStatus::Matched, &["t9"])), Ok(false));
        assert_eq!(o.id.as_deref(), Some("0xabc"));
    }

    #[test]
    fn duplicates_and_older_statuses_change_nothing() {
        let mut o = sent();
        o.apply(acc(AckStatus::Matched, &["t1"])).unwrap();
        o.apply(trade("t1", TradeStatus::Confirmed)).unwrap();
        assert_eq!(o.apply(trade("t1", TradeStatus::Confirmed)), Ok(false));
        assert_eq!(o.apply(trade("t1", TradeStatus::Matched)), Ok(false));
        assert_eq!(o.phase, Confirmed);
        assert_eq!(o.trades.len(), 1);
    }

    fn input() -> impl proptest::strategy::Strategy<Value = Input> {
        use proptest::prelude::*;
        let st = prop_oneof![Just(TradeStatus::Matched), Just(TradeStatus::Mined), Just(TradeStatus::Retrying), Just(TradeStatus::Confirmed), Just(TradeStatus::Failed)];
        let ack = prop_oneof![Just(AckStatus::Live), Just(AckStatus::Matched), Just(AckStatus::Delayed)];
        prop_oneof![
            Just(Input::Signed), Just(Input::Sent), Just(Input::NoAnswer), Just(Input::Closed),
            (ack, prop::collection::vec("t[0-2]", 0..3), "0x[ab]").prop_map(|(status, trades, id)| Input::Accepted { id, status, trades }),
            any::<bool>().prop_map(|killed| Input::Rejected { killed, reason: String::new() }),
            ("t[0-2]", st, 0i64..10_000_000).prop_map(|(id, status, size)| Input::Trade { id, status, size, price: 500_000 }),
        ]
    }

    proptest::proptest! {
        /// Whatever the facts and their order: refused facts change nothing, terminal outcomes stay terminal, a
        /// trade status never goes back, confirmed means a confirmed trade, and only an order whose fate is open blocks.
        #[test]
        fn invariants_hold_for_any_sequence(inputs in proptest::collection::vec(input(), 0..30)) {
            let mut o = Order::default();
            for i in inputs {
                let (before, trades) = (o.phase, o.trades.clone());
                match o.apply(i) {
                    Err(_) => { proptest::prop_assert_eq!(o.phase, before); proptest::prop_assert_eq!(&o.trades, &trades); }
                    Ok(_) => {
                        if matches!(before, Refused | Killed) { proptest::prop_assert_eq!(o.phase, before); }
                        for (id, t) in &trades { proptest::prop_assert!(o.trades[id].status >= t.status); }
                    }
                }
                if o.phase == Confirmed { proptest::prop_assert!(o.trades.values().any(|t| t.status == TradeStatus::Confirmed)); }
                proptest::prop_assert_eq!(o.blocks_openings(), matches!(o.phase, Signed | Sent | Unknown));
            }
        }
    }

    #[test]
    fn impossible_facts_are_refused_and_change_nothing() {
        let mut o = Order::default();
        assert!(o.apply(Input::Sent).is_err());
        assert!(o.apply(acc(AckStatus::Matched, &[])).is_err());
        let mut o = sent();
        o.apply(Input::Rejected { killed: true, reason: String::new() }).unwrap();
        assert!(o.apply(acc(AckStatus::Matched, &[])).is_err());
        assert!(o.apply(trade("t", TradeStatus::Matched)).is_err());
        assert!(o.apply(Input::Signed).is_err());
        assert_eq!(o.phase, Killed);
        let mut o = sent();
        o.apply(acc(AckStatus::Matched, &["t1"])).unwrap();
        assert!(o.apply(Input::Accepted { id: "0xother".into(), status: AckStatus::Matched, trades: vec![] }).is_err());
    }
}
