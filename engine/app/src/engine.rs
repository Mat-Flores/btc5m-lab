//! The engine shared by the live shell (`main.rs`) and the test shell (`backtest/`): strategies, their decisions
//! turned into numbered commands, the books they read. Synchronous; it reads no clock: every input comes with its
//! instant. What surrounds it is the shell's: feeds or traces, the venue (real or simulated), the clock.
use adapter_polymarket::sign;
use engine_core::account::Dir;
use engine_core::book::{Books, Integrity};
use engine_core::clock::Stamp;
use engine_core::event::{Event, Payload};
use engine_core::market::Window;
use engine_core::money::Micro;
use engine_core::order::{Input, TradeStatus};
use engine_core::value::amount;

pub enum EngineInput {
    Feed(Event),
    Window(Window),
    Resolved { market: String, winner_token: String, up_won: bool, price_to_beat: Option<Micro> },
    Venue(VenueEvent),
    Tick,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tif { Fak, Fok, Gtc, Gtd { expire_s: u64 } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Amount { Usd(Micro), Shares(Micro) }

/// An order a strategy wants. `client_id`: the strategy's own number, unique within the strategy.
#[derive(Clone, Debug, PartialEq)]
pub struct Intent { pub client_id: u64, pub token: String, pub side: Dir, pub limit: Micro, pub amount: Amount, pub tif: Tif, pub post_only: bool, pub reason: &'static str }

#[derive(Clone, Debug, PartialEq)]
pub enum Decision { Place(Intent), Cancel { client_id: u64 } }

/// A decision the engine accepted, numbered: `n` names the order for the venue and in every later event.
#[derive(Clone, Debug, PartialEq)]
pub enum Command { Place { n: u32, strategy: u16, intent: Intent, maker: u64, taker: u64 }, Cancel { n: u32 } }

#[derive(Clone, Debug, PartialEq)]
pub enum VenueEvent {
    Answer { n: u32, input: Input },
    Fill { n: u32, trade: String, status: TradeStatus, size: Micro, price: Micro, maker: bool, fee: Micro, rebate: Option<Micro> },
    Closed { n: u32 },
}
impl VenueEvent { pub fn n(&self) -> u32 { match self { VenueEvent::Answer { n, .. } | VenueEvent::Fill { n, .. } | VenueEvent::Closed { n } => *n } } }

/// Where commands go: the real venue (live) or a simulated one (backtest).
pub trait Venue {
    fn submit(&mut self, cmd: Command, at: Stamp);
    /// What happened at the venue up to `until`, in order.
    fn poll(&mut self, until: Stamp, out: &mut Vec<(Stamp, VenueEvent)>);
}

pub trait Strategy {
    /// Names of the configured sources it reads (checked at startup); crossing N sources is naming N.
    fn sources(&self) -> Vec<String>;
    fn on_window(&mut self, w: &Window, at: Stamp, out: &mut Vec<Decision>);
    /// `books`: the books of `source` when they are the ones in use, else None.
    fn on_event(&mut self, source: &str, ev: &Event, books: Option<&Books>, out: &mut Vec<Decision>);
    fn on_resolved(&mut self, market: &str, up_won: bool, ptb: Option<Micro>, at: Stamp, out: &mut Vec<Decision>);
    fn on_venue(&mut self, ev: &VenueEvent, out: &mut Vec<Decision>);
    fn stats(&self) -> Vec<(&'static str, f64)>;
    // two additions to the frozen contract, both optional:
    /// Time passes without an event (the live shell's 50 ms timer, the test shell's ticks).
    fn on_tick(&mut self, _at: Stamp, _out: &mut Vec<Decision>) {}
    /// The engine numbered a Place decision: the venue events of the order will carry `n`.
    fn placed(&mut self, _client_id: u64, _n: u32) {}
}

/// One accepted order: its strategy, the strategy's id, the intent.
pub struct Placed { pub strategy: u16, pub client_id: u64, pub intent: Intent }

pub struct Engine {
    /// Source name of each SourceId.
    sources: Vec<String>,
    /// Books of each SourceId that carries books.
    pub books: Vec<Option<Books>>,
    /// The books of this SourceId are the ones strategies read (the live shell picks one connection per source).
    pub used: Vec<bool>,
    strategies: Vec<Box<dyn Strategy>>,
    pub windows: Vec<Window>,
    /// Every accepted order, by its number.
    pub orders: Vec<Placed>,
    /// What the last feed event did to the books: per asset, applied or broken (the shell resyncs those).
    pub book_changes: Vec<(String, Result<(), Integrity>)>,
    /// Decisions refused here (unknown market, amounts not computable), with why.
    pub refused: Vec<(u16, Decision, String)>,
    buf: Vec<Decision>,
    /// A book change left unconfirmed this long is checked (`Books::quiet`).
    pub book_quiet_ns: u64,
    /// A window's metadata is kept this long after the start of the latest one, for its late orders and its result.
    pub windows_kept_s: u64,
}

/// Polymarket books are probability books: an empty side is announced as 0 (bids) and 1 (asks).
pub fn new_books(quiet_ns: u64) -> Books { Books::new(0, engine_core::money::MICRO, quiet_ns) }

impl Engine {
    /// `sources`: per SourceId, its name and whether it carries books.
    /// `book_quiet_ns`, `windows_kept_s`: see the fields (engine config `runtime`: 200 ms, 3600 s).
    pub fn new(sources: Vec<(String, bool)>, strategies: Vec<Box<dyn Strategy>>, book_quiet_ns: u64, windows_kept_s: u64) -> Result<Engine, String> {
        for s in &strategies { for n in s.sources() { if !sources.iter().any(|(x, _)| *x == n) { return Err(format!("strategy source {n} not configured")); } } }
        let books = sources.iter().map(|(_, b)| b.then(|| new_books(book_quiet_ns))).collect();
        let used = vec![false; sources.len()];
        Ok(Engine { sources: sources.into_iter().map(|s| s.0).collect(), books, used, strategies, windows: vec![], orders: vec![], book_changes: vec![], refused: vec![], buf: vec![], book_quiet_ns, windows_kept_s })
    }

    pub fn stats(&self) -> Vec<Vec<(&'static str, f64)>> { self.strategies.iter().map(|s| s.stats()).collect() }

    pub fn on(&mut self, input: EngineInput, at: Stamp, out: &mut Vec<Command>) {
        match input {
            EngineInput::Feed(e) => {
                self.book_changes.clear();
                let src = e.source as usize;
                if let Some(books) = self.books.get_mut(src).and_then(|b| b.as_mut()) { apply(books, &e, &mut self.book_changes); }
                for i in 0..self.strategies.len() {
                    let books = if self.used.get(src) == Some(&true) { self.books[src].as_ref() } else { None };
                    let name = self.sources.get(src).map_or("", |s| s.as_str());
                    self.strategies[i].on_event(name, &e, books, &mut self.buf);
                    self.emit(i, out);
                }
            }
            EngineInput::Window(w) => {
                // known before the strategies hear of it: an order decided now names its market
                if !self.windows.iter().any(|x| x.start == w.start) { self.windows.push(w.clone()); }
                        // a finished window's metadata is kept a while, for its late orders and its result
                if let Some(last) = self.windows.iter().map(|x| x.start).max() { self.windows.retain(|x| x.start + self.windows_kept_s > last); }
                for i in 0..self.strategies.len() { self.strategies[i].on_window(&w, at, &mut self.buf); self.emit(i, out); }
            }
            EngineInput::Resolved { market, up_won, price_to_beat, .. } => {
                for i in 0..self.strategies.len() { self.strategies[i].on_resolved(&market, up_won, price_to_beat, at, &mut self.buf); self.emit(i, out); }
            }
            EngineInput::Venue(ev) => {
                let Some(p) = self.orders.get(ev.n() as usize) else { return };
                let i = p.strategy as usize;
                self.strategies[i].on_venue(&ev, &mut self.buf);
                self.emit(i, out);
            }
            EngineInput::Tick => {
                for i in 0..self.strategies.len() { self.strategies[i].on_tick(at, &mut self.buf); self.emit(i, out); }
            }
        }
    }

    /// The decisions strategy `i` just made, numbered and priced; refusals are kept with their reason.
    fn emit(&mut self, i: usize, out: &mut Vec<Command>) {
        let s = i as u16;
        for d in std::mem::take(&mut self.buf) {
            match d {
                Decision::Place(intent) => {
                    let Some(w) = self.windows.iter().find(|w| w.tokens.contains(&intent.token)) else { self.refused.push((s, Decision::Place(intent), "market unknown".into())); continue };
                    let (maker, taker) = match amounts(&intent, w.tick) { Ok(x) => x, Err(why) => { self.refused.push((s, Decision::Place(intent), why)); continue } };
                    let n = self.orders.len() as u32;
                    self.orders.push(Placed { strategy: s, client_id: intent.client_id, intent: intent.clone() });
                    self.strategies[i].placed(intent.client_id, n);
                    out.push(Command::Place { n, strategy: s, intent, maker, taker });
                }
                Decision::Cancel { client_id } => match self.orders.iter().rposition(|p| p.strategy == s && p.client_id == client_id) {
                    Some(n) => out.push(Command::Cancel { n: n as u32 }),
                    None => self.refused.push((s, d, "order unknown".into())),
                },
            }
        }
    }
}

/// One feed event on the books of its source (what the live shell did inline before the engine existed).
fn apply(books: &mut Books, e: &Event, changes: &mut Vec<(String, Result<(), Integrity>)>) {
    match &e.payload {
        Payload::BookSnapshot { asset, bids, asks, tick } => { let r = books.snapshot(asset, bids, asks, *tick); changes.push((asset.clone(), r)); }
        Payload::BookDelta { changes: c } => for (asset, why) in books.delta(e.event_ms.unwrap_or(0), c, e.recv.mono_ns) { changes.push((asset, Err(why))); },
        Payload::TickChange { asset, tick } => { books.tick_changed(asset, *tick); changes.push((asset.clone(), Err(Integrity::TickChanged))); }
        _ => {}
    }
}

/// (makerAmount, takerAmount) of an intent. Marketable orders (FAK, FOK) follow the signer (`sign::amounts`, checked
/// against the SDK): a buy spends `Usd`, a sell gives `Shares`. Resting orders (GTC, GTD) of `Shares` at the limit:
/// a buy pays size × price, a sell asks it [à vérifier : arrondi des montants d'un ordre limite chez Polymarket].
pub fn amounts(i: &Intent, tick: Micro) -> Result<(u64, u64), String> {
    let side = if i.side == Dir::Buy { sign::Side::Buy } else { sign::Side::Sell };
    match (i.tif, i.side, i.amount) {
        (Tif::Fak | Tif::Fok, Dir::Buy, Amount::Usd(u)) | (Tif::Fak | Tif::Fok, Dir::Sell, Amount::Shares(u)) => sign::amounts(side, u, i.limit, tick).map_err(|e| format!("{e:?}")),
        (Tif::Gtc | Tif::Gtd { .. }, d, Amount::Shares(sh)) if sh > 0 && i.limit > 0 && i.limit % tick == 0 => {
            let usd = amount(sh, i.limit) as u64;
            Ok(if d == Dir::Buy { (usd, sh as u64) } else { (sh as u64, usd) })
        }
        _ => Err(format!("amount {:?} not supported for {:?} {:?} at {}", i.amount, i.tif, i.side, i.limit)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::market::FeeSchedule;

    const AT: Stamp = Stamp { mono_ns: 0, wall_ns: 0 };

    fn window() -> Window {
        Window { start: 300, end: 600, slug: "btc-updown-5m-300".into(), market: "m".into(), tokens: ["up".into(), "down".into()], tick: 10_000, min_size: 5_000_000,
            fee: FeeSchedule::default(), taker_delay: false, twap_s: Some(60), price_to_beat: None, winner: None }
    }
    fn intent(id: u64, token: &str) -> Intent { Intent { client_id: id, token: token.into(), side: Dir::Buy, limit: 550_000, amount: Amount::Usd(93_450_000), tif: Tif::Fak, post_only: false, reason: "t" } }

    /// Places on each window, cancels its first order on the first venue event, and records what it was told.
    struct Probe { placed: Vec<(u64, u32)>, venue: Vec<u32>, next: u64 }
    impl Strategy for Probe {
        fn sources(&self) -> Vec<String> { vec!["a".into()] }
        fn on_window(&mut self, w: &Window, _: Stamp, out: &mut Vec<Decision>) {
            self.next += 1;
            out.push(Decision::Place(intent(self.next, w.up())));
            out.push(Decision::Place(intent(99, "unknown")));
        }
        fn on_event(&mut self, _: &str, _: &Event, _: Option<&Books>, _: &mut Vec<Decision>) {}
        fn on_resolved(&mut self, _: &str, _: bool, _: Option<Micro>, _: Stamp, _: &mut Vec<Decision>) {}
        fn on_venue(&mut self, ev: &VenueEvent, out: &mut Vec<Decision>) { self.venue.push(ev.n()); out.push(Decision::Cancel { client_id: 1 }); }
        fn stats(&self) -> Vec<(&'static str, f64)> { vec![] }
        fn placed(&mut self, c: u64, n: u32) { self.placed.push((c, n)); }
    }

    #[test]
    fn decisions_become_numbered_priced_commands() {
        assert!(Engine::new(vec![("b".into(), false)], vec![Box::new(Probe { placed: vec![], venue: vec![], next: 0 })], 200_000_000, 3600).is_err());
        let mut e = Engine::new(vec![("a".into(), true)], vec![Box::new(Probe { placed: vec![], venue: vec![], next: 0 })], 200_000_000, 3600).unwrap();
        let mut out = vec![];
        e.on(EngineInput::Window(window()), AT, &mut out);
        // the reference's own amounts for 93.457943 $ at 0.55 (strategy/src/reference.rs test): the signer's are equal
        assert_eq!(out, vec![Command::Place { n: 0, strategy: 0, intent: intent(1, "up"), maker: 93_450_000, taker: 169_909_100 }]);
        assert_eq!(e.refused.len(), 1);
        out.clear();
        e.on(EngineInput::Venue(VenueEvent::Closed { n: 0 }), AT, &mut out);
        assert_eq!(out, vec![Command::Cancel { n: 0 }]);
        e.on(EngineInput::Venue(VenueEvent::Closed { n: 7 }), AT, &mut out); // not an order of ours: nobody is told
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn resting_order_amounts() {
        let i = Intent { tif: Tif::Gtc, amount: Amount::Shares(12_340_000), limit: 450_000, post_only: true, ..intent(1, "up") };
        assert_eq!(amounts(&i, 10_000), Ok((5_553_000, 12_340_000)));
        assert!(amounts(&Intent { limit: 455_000, ..i.clone() }, 10_000).is_err());
        assert!(amounts(&Intent { amount: Amount::Usd(1), ..i }, 10_000).is_err());
    }
}
