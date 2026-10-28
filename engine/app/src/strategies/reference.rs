//! The reference strategy (`engine_strategy::reference`, unchanged) behind the `Strategy` contract. The conversion of
//! engine events into the old engine's events is the paper mode's (phase 9), moved here as it was; its orders become
//! FAK intents. For the paper shell, the events fed and the strategy's own records are kept when `keep` is set.
use crate::engine::{Amount, Decision, Intent, Strategy, Tif, VenueEvent};
use engine_core::account::Dir;
use engine_core::book::Books;
use engine_core::clock::Stamp;
use engine_core::event::{Payload, Side};
use engine_core::market::Window;
use engine_core::money::{Micro, MICRO};
use engine_strategy::reference::{Config, Event, Fee, Meta, Rec, Reference};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct RefStrategy {
    strat: Reference,
    /// The configured price source it reads, and the oracle source (Chainlink).
    source: String,
    oracle: String,
    last_mid: f64,
    /// slug → [up, down] tokens; market id → slug.
    tokens: HashMap<String, [String; 2]>,
    slugs: HashMap<String, String>,
    next_id: u64,
    decisions: u64,
    pending: Vec<Decision>,
    /// Kept for the paper shell (`keep`): each event fed with its source timestamp, and each record of the strategy
    /// with, for an order, the client id of its intent (None: market unknown, no intent).
    pub keep: bool,
    pub fed: Vec<(Event, Option<u64>)>,
    pub records: Vec<(Rec, Option<u64>)>,
}

impl RefStrategy {
    pub fn new(cfg: Config, oracle: String) -> RefStrategy {
        RefStrategy { source: cfg.source.clone(), strat: Reference::new(cfg), oracle, last_mid: f64::NAN, tokens: HashMap::new(), slugs: HashMap::new(),
            next_id: 0, decisions: 0, pending: vec![], keep: false, fed: vec![], records: vec![] }
    }

    /// Feeds one strategy event, then turns its orders into intents.
    fn feed(&mut self, ev: Event, ts: Option<u64>) {
        self.strat.process(&ev);
        if self.keep { self.fed.push((ev, ts)); }
        for r in std::mem::take(&mut self.strat.records) {
            let id = match &r {
                Rec::Order { slug, side, buy, limit, maker, .. } => self.tokens.get(slug).map(|t| {
                    self.next_id += 1;
                    self.decisions += 1;
                    let token = t[if *side == engine_strategy::reference::Side::Up { 0 } else { 1 }].clone();
                    let amount = if *buy { Amount::Usd(*maker as Micro) } else { Amount::Shares(*maker as Micro) };
                    self.pending.push(Decision::Place(Intent { client_id: self.next_id, token, side: if *buy { Dir::Buy } else { Dir::Sell },
                        limit: (limit * MICRO as f64).round() as Micro, amount, tif: Tif::Fak, post_only: false, reason: "reference" }));
                    self.next_id
                }),
                _ => None,
            };
            if self.keep { self.records.push((r, id)); }
        }
    }

    fn flush(&mut self, out: &mut Vec<Decision>) { out.append(&mut self.pending); }

    /// The connection in use changed (live shell): the strategy gets that connection's books whole. Its decisions
    /// go out with the next input.
    pub fn switched(&mut self, books: &Books, assets: &[String], t: f64) {
        for a in assets {
            let Some(b) = books.get(a) else { continue };
            let bids = b.bids().map(|(p, s)| (p as f64 / 1e6, s as f64 / 1e6)).collect();
            let asks = b.asks().map(|(p, s)| (p as f64 / 1e6, s as f64 / 1e6)).collect();
            self.feed(Event::Book { t, a: a.clone(), bids, asks }, None);
        }
    }
}

impl Strategy for RefStrategy {
    fn sources(&self) -> Vec<String> { vec![self.source.clone(), self.oracle.clone()] }

    fn on_window(&mut self, w: &Window, at: Stamp, out: &mut Vec<Decision>) {
        let t = at.wall_ns as f64 / 1e6;
        self.tokens.insert(w.slug.clone(), w.tokens.clone());
        self.slugs.insert(w.market.clone(), w.slug.clone());
        let fee = (w.fee.rate != 0).then(|| Fee { rate: w.fee.rate as f64 / 1e6, exponent: w.fee.exponent as f64 });
        let m = Meta { slug: w.slug.clone(), start: w.start as f64, len: (w.end - w.start) as f64, tokens: [w.up().clone(), w.down().clone()], tick: w.tick as f64 / 1e6,
            min_size: w.min_size as f64 / 1e6, fee, twap: w.twap_s.unwrap_or(60) as f64, price_to_beat: w.price_to_beat.map_or(f64::NAN, |p| p as f64 / 1e6) };
        self.feed(Event::Meta { t, w: m }, None);
        self.flush(out);
    }

    /// The strategy sees prices of its source, the oracle, and the books in use.
    fn on_event(&mut self, source: &str, e: &engine_core::event::Event, books: Option<&Books>, out: &mut Vec<Decision>) {
        let used_books = books.is_some();
        let t = e.recv.wall_ns as f64 / 1e6;
        match &e.payload {
            Payload::Quote { bid, ask } if source == self.source => {
                // the old application sent a price only when its mid changed (src/feeds/live.js)
                let mid = (*bid as f64 / 1e6 + *ask as f64 / 1e6) / 2.0;
                if mid != self.last_mid { self.last_mid = mid; self.feed(Event::Px { t, src: source.into(), p: mid }, e.event_ms); }
            }
            Payload::Oracle { value } if source == self.oracle => {
                if let Some(ts) = e.event_ms { self.feed(Event::Cl { t, p: *value as f64 / 1e6, ts: ts as f64 }, Some(ts)); }
            }
            Payload::BookSnapshot { asset, bids, asks, .. } if used_books => {
                let l = |v: &[engine_core::event::Level]| v.iter().map(|x| (x.price as f64 / 1e6, x.size as f64 / 1e6)).collect();
                self.feed(Event::Book { t, a: asset.clone(), bids: l(bids), asks: l(asks) }, e.event_ms);
            }
            Payload::BookDelta { changes } if used_books => {
                let ch = changes.iter().map(|c| (c.asset.clone(), c.side == Side::Ask, c.price as f64 / 1e6, c.size as f64 / 1e6)).collect();
                self.feed(Event::Delta { t, ch }, e.event_ms);
            }
            _ => self.feed(Event::Other { t }, None),
        }
        self.flush(out);
    }

    fn on_resolved(&mut self, market: &str, up_won: bool, ptb: Option<Micro>, at: Stamp, out: &mut Vec<Decision>) {
        let t = at.wall_ns as f64 / 1e6;
        // only windows it was given have a slug, and only they can hold its positions
        if let Some(slug) = self.slugs.get(market).cloned() {
            if let Some(p) = ptb { self.feed(Event::Ptb { t, slug: slug.clone(), price_to_beat: p as f64 / 1e6 }, None); }
            self.feed(Event::Res { t, slug, winner: if up_won { "Up" } else { "Down" }.into() }, None);
        }
        self.flush(out);
    }

    /// Its fills are its own model's (the old engine's realistic account): the venue's do not reach its decisions.
    fn on_venue(&mut self, _: &VenueEvent, _: &mut Vec<Decision>) {}

    fn on_tick(&mut self, at: Stamp, out: &mut Vec<Decision>) { self.feed(Event::Other { t: at.wall_ns as f64 / 1e6 }, None); self.flush(out); }

    fn stats(&self) -> Vec<(&'static str, f64)> { vec![("decisions", self.decisions as f64)] }
}

/// A reference strategy the paper shell keeps a handle on (switches, records), run by the engine.
pub struct Shared(pub Rc<RefCell<RefStrategy>>);

impl Strategy for Shared {
    fn sources(&self) -> Vec<String> { self.0.borrow().sources() }
    fn on_window(&mut self, w: &Window, at: Stamp, out: &mut Vec<Decision>) { self.0.borrow_mut().on_window(w, at, out) }
    fn on_event(&mut self, source: &str, ev: &engine_core::event::Event, books: Option<&Books>, out: &mut Vec<Decision>) { self.0.borrow_mut().on_event(source, ev, books, out) }
    fn on_resolved(&mut self, market: &str, up_won: bool, ptb: Option<Micro>, at: Stamp, out: &mut Vec<Decision>) { self.0.borrow_mut().on_resolved(market, up_won, ptb, at, out) }
    fn on_venue(&mut self, ev: &VenueEvent, out: &mut Vec<Decision>) { self.0.borrow_mut().on_venue(ev, out) }
    fn on_tick(&mut self, at: Stamp, out: &mut Vec<Decision>) { self.0.borrow_mut().on_tick(at, out) }
    fn stats(&self) -> Vec<(&'static str, f64)> { self.0.borrow().stats() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Command, Engine, EngineInput};
    use engine_core::event::Level;
    use engine_core::market::FeeSchedule;

    const S: u64 = 1_792_359_300;
    fn at(ms: u64) -> Stamp { Stamp { mono_ns: ms * 1_000_000, wall_ns: (S * 1000 + ms) * 1_000_000 } }
    fn ev(source: u16, ms: u64, event_ms: Option<u64>, payload: Payload) -> engine_core::event::Event { engine_core::event::Event { source, recv: at(ms), event_ms, seq: None, payload } }

    /// An order of the strategy becomes a FAK command whose amounts are the strategy's own (what the live mode sent).
    #[test]
    fn an_order_of_the_reference_is_a_fak_command_with_its_amounts() {
        let cfg: Config = serde_json::from_value(serde_json::json!({ "source": "okx", "jump": 0, "minFair": 0, "minEdge": 0.05 })).unwrap();
        let mut r = RefStrategy::new(cfg, "chainlink".into());
        r.keep = true;
        let shared = Rc::new(RefCell::new(r));
        let mut e = Engine::new(vec![("okx".into(), false), ("chainlink".into(), false), ("polymarket".into(), true)], vec![Box::new(Shared(shared.clone()))], 200_000_000, 3600).unwrap();
        e.used[2] = true;
        let w = Window { start: S, end: S + 300, slug: format!("btc-updown-5m-{S}"), market: "m".into(), tokens: ["up".into(), "down".into()], tick: 10_000, min_size: 5_000_000,
            fee: FeeSchedule { rate: 70_000, exponent: 1, rebate_rate: 200_000 }, taker_delay: true, twap_s: Some(60), price_to_beat: Some(81_000_000_000), winner: None };
        let mut out = vec![];
        e.on(EngineInput::Window(w), at(0), &mut out);
        // spot 81 050 against a strike of 81 000 with 100 s to go: Up is worth far more than an ask of 0.30
        for k in 0..5u64 {
            e.on(EngineInput::Feed(ev(0, 200_000 + k * 500, Some(S * 1000 + 200_000 + k * 500), Payload::Quote { bid: 81_050_000_000 + k as i64, ask: 81_050_100_000 + k as i64 })), at(200_000 + k * 500), &mut out);
            e.on(EngineInput::Feed(ev(1, 200_100 + k * 500, Some(S * 1000 + 199_900 + k * 500), Payload::Oracle { value: 81_050_000_000 })), at(200_100 + k * 500), &mut out);
        }
        let lv = |p, s| Level { price: p, size: s };
        e.on(EngineInput::Feed(ev(2, 203_000, Some(S * 1000 + 202_990), Payload::BookSnapshot { asset: "up".into(), bids: vec![lv(290_000, 50_000_000)], asks: vec![lv(300_000, 400_000_000)], tick: Some(10_000) })), at(203_000), &mut out);
        let orders: Vec<_> = shared.borrow().records.iter().filter_map(|(r, id)| match r { Rec::Order { maker, taker, limit, .. } => Some((*maker as u64, *taker as u64, *limit, *id)), _ => None }).collect();
        assert_eq!(orders.len(), 1, "{:?}", shared.borrow().records.len());
        let (maker, taker, limit, id) = orders[0];
        let Command::Place { intent, maker: m, taker: t, .. } = &out[0] else { panic!("{out:?}") };
        assert_eq!((intent.client_id, intent.tif, intent.side, *m, *t, intent.limit), (id.unwrap(), Tif::Fak, Dir::Buy, maker, taker, (limit * 1e6).round() as Micro));
        assert_eq!(intent.amount, Amount::Usd(maker as Micro));
    }
}
