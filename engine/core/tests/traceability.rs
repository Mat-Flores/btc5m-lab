//! Phase 7 exit criteria: every figure the accounting exposes goes back to ledger facts, and is rebuilt from those
//! facts alone; every unrealized value is labelled latent with the age of its book; a position the venue has not
//! reported (no trade, or a trade without its size) is never valued.
use engine_core::account::{Account, Dir, Fact};
use engine_core::book::Book;
use engine_core::clock::Stamp;
use engine_core::event::Level;
use engine_core::ledger::{read_all, Ledger, Stamped};
use engine_core::order::TradeStatus;
use engine_core::value::{value, Label};

fn trade(id: &str, market: &str, token: &str, dir: Dir, status: TradeStatus, size: i64, price: i64) -> Fact {
    Fact::Trade { trade: id.into(), order: format!("o-{id}"), market: market.into(), token: token.into(), dir, status, size, price, fee_rate: 70_000, fee_exponent: 1, end_ms: 9_000_000 }
}

fn ledger(facts: &[Fact]) -> Vec<(u64, Fact)> {
    let p = std::env::temp_dir().join(format!("trace-{}-{}", std::process::id(), facts.len()));
    let _ = std::fs::remove_file(&p);
    let (mut l, _) = Ledger::open(&p).unwrap();
    for (i, f) in facts.iter().enumerate() { l.append(&Stamped { stamp: Stamp { mono_ns: i as u64, wall_ns: i as u64 }, fact: f.clone() }).unwrap(); }
    l.sync().unwrap();
    let back = read_all::<Fact>(&p).unwrap().into_iter().map(|e| (e.seq, e.fact)).collect();
    std::fs::remove_file(&p).unwrap();
    back
}

/// The account rebuilt from the listed seqs only.
fn only(entries: &[(u64, Fact)], seqs: &[u64]) -> Account { Account::fold(entries.iter().filter(|(s, _)| seqs.contains(s)).map(|(s, f)| (*s, f))) }

#[test]
fn every_figure_goes_back_to_the_ledger() {
    let entries = ledger(&[
        Fact::Balance { amount: 100_000_000 },
        trade("a1", "m1", "up1", Dir::Buy, TradeStatus::Matched, 0, 0), // answer with a trade id, size not given yet
        trade("a1", "m1", "up1", Dir::Buy, TradeStatus::Matched, 10_000_000, 560_000),
        trade("b1", "m2", "up2", Dir::Buy, TradeStatus::Confirmed, 8_000_000, 400_000),
        Fact::Fees { token: "up2".into(), amount: 134_400 },
        trade("b2", "m2", "up2", Dir::Sell, TradeStatus::Confirmed, 8_000_000, 600_000),
        trade("c1", "m3", "dn3", Dir::Buy, TradeStatus::Confirmed, 5_000_000, 300_000),
        Fact::Resolved { market: "m3".into(), winner: "dn3".into() },
        Fact::Redeemed { market: "m3".into(), amount: 5_000_000, tx: "0xredeem".into() },
        trade("d1", "m4", "up4", Dir::Buy, TradeStatus::Confirmed, 2_000_000, 500_000),
        Fact::Resolved { market: "m4".into(), winner: "dn4".into() },
    ]);
    let a = Account::fold(entries.iter().map(|(s, f)| (*s, f)));
    let all: Vec<u64> = entries.iter().map(|e| e.0).collect();

    // open positions: m1 (matched, not confirmed) and m4 (resolved, lost, not redeemed)
    assert_eq!(a.open.keys().collect::<Vec<_>>(), ["up1", "up4"]);
    for p in a.open.values() {
        assert!(!p.seqs.is_empty() && p.seqs.iter().all(|s| all.contains(s)));
        assert_eq!(&only(&entries, &p.seqs).open[&p.token], p, "{} rebuilt from its own facts", p.token);
    }
    // realized: m2 sold, m3 redeemed
    assert_eq!(a.realized.iter().map(|r| (r.market.as_str(), r.amount)).collect::<Vec<_>>(), [("m2", (4_800_000 - 134_400) - (3_200_000 + 134_400)), ("m3", 5_000_000 - 1_500_000 - 73_500)]);
    for r in &a.realized {
        assert!(r.seqs.iter().all(|s| all.contains(s)));
        assert_eq!(only(&entries, &r.seqs).realized, vec![r.clone()]);
    }
    assert_eq!(a.balance, Some((100_000_000, 0)));

    // values: latent with the book's age, or settled; each carries its position's seqs
    let mut book = Book::default();
    book.snapshot(&[Level { price: 540_000, size: 50_000_000 }], &[Level { price: 560_000, size: 1 }], None).unwrap();
    let v = value(&a.open["up1"], Some(&book), 12).unwrap();
    assert_eq!((v.label, v.value, v.cost, v.latent, v.confirmed, &v.seqs), (Label::Latent { book_age_ms: 12 }, 5_400_000 - 173_880, 5_600_000 + 172_480, 5_226_120 - 5_772_480, false, &vec![1, 2]));
    let json = serde_json::to_value(&v).unwrap();
    assert_eq!((json["label"].as_str(), json["book_age_ms"].as_i64()), (Some("latent"), Some(12)));
    let s = value(&a.open["up4"], None, 0).unwrap();
    assert_eq!((s.label, s.value, s.latent), (Label::Settled, 0, -(1_000_000 + 35_000)));
    // no usable book: no value, never a guess
    assert!(value(&a.open["up1"], None, 0).is_none());
    assert!(value(&a.open["up1"], Some(&Book::default()), 0).is_none());
}

#[test]
fn a_position_the_venue_has_not_reported_is_never_valued() {
    // an order sent, unknown or acknowledged without any trade writes no trade fact; a trade id without its size
    // (the order answer) is not a holding either
    let entries = ledger(&[trade("a1", "m1", "up1", Dir::Buy, TradeStatus::Matched, 0, 0)]);
    let a = Account::fold(entries.iter().map(|(s, f)| (*s, f)));
    assert!(a.open.is_empty() && a.realized.is_empty());
    // and a failed trade takes the position away
    let entries = ledger(&[trade("a1", "m1", "up1", Dir::Buy, TradeStatus::Matched, 10_000_000, 500_000), trade("a1", "m1", "up1", Dir::Buy, TradeStatus::Failed, 0, 0)]);
    let a = Account::fold(entries.iter().map(|(s, f)| (*s, f)));
    assert!(a.open.is_empty() && a.realized.is_empty());
}
