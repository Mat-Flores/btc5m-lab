//! Phase 7 exit criterion: a book drop taken from a recorded session (fixtures/sessions/book-drop.json, built by
//! bench/book-drop.mjs from the live session of 2026-10-17) is replayed event by event; the stop loss must trigger
//! at the instant, and on the values, that the generator computed on its own (BigInt, in JavaScript).
use engine_core::account::{Account, Dir, Fact};
use engine_core::book::Book;
use engine_core::event::{Level, Side};
use engine_core::money::parse_micro;
use engine_core::order::TradeStatus;
use engine_core::value::{value, StopLoss};
use serde_json::Value;

fn m(v: &Value) -> i64 { parse_micro(v.as_str().unwrap()).unwrap() }
fn levels(v: &Value) -> Vec<Level> { v.as_array().unwrap().iter().map(|l| Level { price: m(&l[0]), size: m(&l[1]) }).collect() }

#[test]
fn a_recorded_book_drop_triggers_the_stop_loss_at_the_right_instant_on_the_right_value() {
    let fx: Value = serde_json::from_str(&std::fs::read_to_string(format!("{}/../fixtures/sessions/book-drop.json", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap();
    let end_ms = fx["end_ms"].as_u64().unwrap();
    let rule = StopLoss { loss: fx["stop_loss"]["loss"].as_i64().unwrap(), quiet_s: fx["stop_loss"]["quiet_s"].as_u64().unwrap() };
    // the test position: a trade the venue matched, not yet confirmed (the stop loss acts before confirmation)
    let mut a = Account::default();
    a.apply(0, &Fact::Trade { trade: "t".into(), order: "o".into(), market: "m".into(), token: fx["token"].as_str().unwrap().into(), dir: Dir::Buy, status: TradeStatus::Matched,
        size: m(&fx["entry"]["size"]), price: m(&fx["entry"]["price"]), fee_rate: m(&fx["fee_rate"]), fee_exponent: fx["fee_exponent"].as_u64().unwrap() as u32, end_ms });
    let p = a.open.values().next().unwrap().clone();

    let mut book = Book::default();
    let mut fired = None;
    for (i, e) in fx["events"].as_array().unwrap().iter().enumerate() {
        if e.get("bids").is_some() { book.snapshot(&levels(&e["bids"]), &levels(&e["asks"]), None).unwrap(); }
        else { for c in e["changes"].as_array().unwrap() { book.apply(if c[0] == "B" { Side::Bid } else { Side::Ask }, m(&c[1]), m(&c[2])).unwrap(); } }
        let v = value(&p, Some(&book), 0).unwrap();
        if rule.triggers(&v, e["t"].as_f64().unwrap() as u64, end_ms) { fired = Some((i, e["t"].as_f64().unwrap(), v)); break; }
    }
    let (i, t, v) = fired.expect("the stop loss never triggered");
    let x = &fx["expected"];
    assert_eq!(i as u64, x["index"].as_u64().unwrap());
    assert_eq!(t, x["t"].as_f64().unwrap());
    assert_eq!((v.cost, v.value, v.latent, v.exit.as_ref().unwrap().unfilled), (x["cost"].as_i64().unwrap(), x["net"].as_i64().unwrap(), x["latent"].as_i64().unwrap(), x["unfilled"].as_i64().unwrap()));
    // the inputs recorded with the decision: the bid levels walked, best first
    assert!(v.exit.unwrap().levels.windows(2).all(|w| w[0].0 > w[1].0));
}
