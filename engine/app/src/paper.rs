//! Paper mode (phase 9): the reference strategy on the live feeds. The strategy runs in the engine; each order it
//! decides (an engine command) is built and signed here with a throwaway key and never sent (no execution adapter
//! exists in this mode); the simulated Polymarket execution matches it after one way plus the taker delay, on the book
//! observed at that instant; the real accounting (`engine_core::account`, `value`) keeps its books, in a ledger of its
//! own. Every figure here is simulated.
//! The feeds are also captured in the old application's session format (`src/feeds/session.js`), so the old
//! backtester, `read-session` and `replay/` read a phase 9 run as they read a recorded session.
use super::Hist;
use adapter_polymarket::paper::{self as sim, TAKER_DELAY_MS};
use adapter_polymarket::sign::{self, Signer};
use engine_app::engine::{Command, Strategy};
use engine_app::strategies::reference::{RefStrategy, Shared};
use engine_core::market::Window;
use engine_core::account::{self, Account, Dir};
use engine_core::book::Books;
use engine_core::clock::Clock;
use engine_core::event::{Payload, Side};
use engine_core::ledger::{read_all, spawn_writer, Ledger, Stamped, SyncPolicy};
use engine_core::money::Micro;
use engine_core::order::TradeStatus;
use engine_core::value::{value, Valuation};
use engine_strategy::reference::{Config, Event, Rec};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;
use std::sync::Arc;

fn one_way() -> f64 { 12.2 }
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaperCfg {
    pub ledger: String,
    /// Gzip'd session file (`gzip` on the PATH).
    pub capture: String,
    /// The reference's configuration, old engine names; `source` is the configured price source it reads.
    pub strategy: Config,
    /// Node → CLOB, one way: half the RTT p50 measured from Dublin (24.4 ms, 01-audit.md); symmetry [non vérifié].
    #[serde(default = "one_way")]
    pub one_way_ms: f64,
    /// Venue hold before the match: 150 ms announced; real orders were not matched before 0.5 s (2026-10-21/22).
    #[serde(default = "taker_delay")]
    pub taker_delay_ms: u64,
}
fn taker_delay() -> u64 { TAKER_DELAY_MS }

/// Facts of the paper ledger besides the accounting ones; all simulated.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Fact {
    Config { strategy: Value, one_way_ms: f64, taker_delay_ms: u64, signer: String },
    /// The strategy's order, built and signed; `id` is its EIP-712 hash. Never sent.
    Decision { id: String, slug: String, token: String, buy: bool, limit: Micro, maker: u64, taker: u64, sign_us: f64, match_after_ms: f64 },
    /// The simulated match, on the book of the connection used at that instant.
    Match { id: String, levels: Vec<(Micro, Micro)>, book_age_ms: Option<i64>, best_ask: Option<Micro>, best_bid: Option<Micro> },
    /// What the strategy's own model (the old engine's paper fill) said, for comparison.
    Strategy { rec: Value },
    Valuation { valuation: Valuation },
    Report { report: Value },
}
#[derive(Serialize)]
#[serde(untagged)]
enum Line { Account(account::Fact), Paper(Fact) }

/// One recorded line of the capture, serialised on its own thread.
pub enum Cap { Ev(Event, Option<u64>), Trade { t: f64, a: String, p: Micro, s: Micro, buy: bool, id: String, ts: Option<u64> }, Header(Value) }

struct Pending { id: String, due_ns: u64, token: String, buy: bool, maker: Micro, limit: Micro }
struct Market { condition: String, fee_rate: Micro, fee_exponent: u32, end_ms: u64 }

pub struct Paper {
    /// The strategy the engine runs; the paper mode reads what it fed and recorded, and hands it connection switches.
    strat: Rc<RefCell<RefStrategy>>,
    source: String,
    one_way_ms: f64,
    taker_delay_ms: u64,
    signer: Signer,
    salt: u64,
    clock: Arc<Clock>,
    facts: std::sync::mpsc::Sender<Stamped<Line>>,
    writer: Option<std::thread::JoinHandle<std::io::Result<()>>>,
    next_seq: u64,
    cap: Option<std::sync::mpsc::Sender<Cap>>,
    capture: Option<std::thread::JoinHandle<()>>,
    pub acct: Account,
    markets: HashMap<String, Market>,
    /// Source timestamp of each token's last book change, for the age of a match's book.
    book_ms: HashMap<String, u64>,
    pending: Vec<Pending>,
    last_value: HashMap<String, Micro>,
    /// Last valuation written per token: at most one a second goes to the ledger (measured: 73 451 in 8 min otherwise).
    written_ms: HashMap<String, u64>,
    decisions: u64,
    matched: u64,
    missed: u64,
    sign_us: Hist,
    delay_ms: Hist,
    strat_us: Hist,
    strat_pnl: f64,
    strat_closes: u64,
}

impl Paper {
    /// The paper mode and the strategy the engine is to run. `oracle`: the configured Chainlink source.
    pub fn open(cfg: PaperCfg, clock: Arc<Clock>, oracle: String) -> (Paper, Box<dyn Strategy>) {
        let (ledger, rep) = Ledger::open(&cfg.ledger).expect("paper ledger");
        // restart: the paper account is what its ledger says
        let mut acct = Account::default();
        for e in read_all::<Value>(&cfg.ledger).expect("paper ledger read") {
            if let Ok(f) = serde_json::from_value::<account::Fact>(e.fact) { acct.apply(e.seq, &f); }
        }
        let (facts, _durable, writer) = spawn_writer::<Line>(ledger, SyncPolicy::default());
        let signer = Signer::from_bytes(&[0x11; 32], "0x7a57144fafde87f5ce4a880d570bae58eebc83f2").unwrap();
        let (cap_tx, cap_rx) = std::sync::mpsc::channel::<Cap>();
        let path = cfg.capture.clone();
        let capture = std::thread::Builder::new().name("capture".into()).spawn(move || write_capture(&path, cap_rx)).unwrap();
        // the whole configuration, old engine names: a replay of the capture reads it back as is
        let strategy = serde_json::to_value(&cfg.strategy).unwrap();
        let source = cfg.strategy.source.clone();
        let mut r = RefStrategy::new(cfg.strategy, oracle);
        r.keep = true;
        let strat = Rc::new(RefCell::new(r));
        let mut p = Paper {
            strat: strat.clone(), source, one_way_ms: cfg.one_way_ms, taker_delay_ms: cfg.taker_delay_ms, signer, salt: 0, clock,
            facts, writer: Some(writer), next_seq: rep.entries, cap: Some(cap_tx), capture: Some(capture), acct, markets: HashMap::new(), book_ms: HashMap::new(), pending: vec![],
            last_value: HashMap::new(), written_ms: HashMap::new(), decisions: 0, matched: 0, missed: 0, sign_us: Hist::default(), delay_ms: Hist::default(),
            strat_us: Hist::default(), strat_pnl: 0.0, strat_closes: 0,
        };
        let id = format!("{}_phase9", chrono_id(p.clock.now().wall_ns));
        p.capture(Cap::Header(json!({ "k": "session", "v": 1, "id": id, "config": strategy, "sources": [p.source], "engine": "engine/app paper",
            "note": "books of the connection the engine used; trades from last_trade_price" })));
        p.fact(Line::Paper(Fact::Config { strategy, one_way_ms: p.one_way_ms, taker_delay_ms: p.taker_delay_ms, signer: "throwaway".into() }));
        (p, Box::new(Shared(strat)))
    }

    fn fact(&mut self, l: Line) -> u64 { let s = self.next_seq; self.next_seq += 1; let _ = self.facts.send(Stamped { stamp: self.clock.now(), fact: l }); s }
    fn capture(&self, c: Cap) { if let Some(tx) = &self.cap { let _ = tx.send(c); } }

    /// After an engine input: what the strategy was fed goes to the capture, what it recorded to the ledger in its
    /// order, its orders (the engine's `cmds`) become signed simulated orders. `us`: time the engine took.
    pub fn after(&mut self, cmds: &[Command], now_ns: u64, us: f64) {
        self.strat_us.push(us);
        let (fed, records) = { let mut s = self.strat.borrow_mut(); (std::mem::take(&mut s.fed), std::mem::take(&mut s.records)) };
        for (ev, ts) in fed { self.capture(Cap::Ev(ev, ts)); }
        for (r, id) in records {
            match r {
                Rec::Order { slug, .. } => {
                    let Some(id) = id else { continue }; // market unknown: no order (as before)
                    if let Some(Command::Place { intent, maker, taker, .. }) = cmds.iter().find(|c| matches!(c, Command::Place { intent, .. } if intent.client_id == id)) {
                        self.decide(&slug, &intent.token, intent.side == Dir::Buy, intent.limit, *maker, *taker, now_ns);
                    }
                }
                other => {
                    if let Rec::Close { pnl, .. } = &other { self.strat_pnl += pnl; self.strat_closes += 1; }
                    let rec = rec_json(&other);
                    self.fact(Line::Paper(Fact::Strategy { rec }));
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn decide(&mut self, slug: &str, token: &str, buy: bool, limit: Micro, maker: u64, taker: u64, now_ns: u64) {
        let t0 = self.clock.now();
        self.salt += 1;
        let o = sign::Order { salt: self.salt, token_id: token.into(), maker_amount: maker, taker_amount: taker,
            side: if buy { sign::Side::Buy } else { sign::Side::Sell }, timestamp_ms: t0.wall_ns / 1_000_000 };
        let _body = self.signer.body(&o, "00000000-0000-4000-8000-000000000000");
        let id = self.signer.order_hash(&o);
        let sign_us = (self.clock.now().mono_ns - t0.mono_ns) as f64 / 1000.0;
        self.sign_us.push(sign_us);
        self.decisions += 1;
        let match_after_ms = sign_us / 1000.0 + self.one_way_ms + self.taker_delay_ms as f64;
        self.fact(Line::Paper(Fact::Decision { id: id.clone(), slug: slug.into(), token: token.into(), buy, limit, maker, taker, sign_us, match_after_ms }));
        self.pending.push(Pending { id, due_ns: now_ns + (match_after_ms * 1e6) as u64, token: token.into(), buy, maker: maker as Micro, limit });
    }

    /// Matches every order due by `now_ns` on the books of the connection in use (state just before the next event).
    pub fn due(&mut self, now_ns: u64, books: Option<&Books>, now_wall_ms: u64) {
        let (due, rest): (Vec<Pending>, Vec<Pending>) = std::mem::take(&mut self.pending).into_iter().partition(|p| p.due_ns <= now_ns);
        self.pending = rest;
        for p in due {
            let book = books.and_then(|b| b.get(&p.token)).filter(|b| b.usable());
            let levels = match book { Some(b) if p.buy => sim::buy(b, p.maker, p.limit), Some(b) => sim::sell(b, p.maker, p.limit), None => vec![] };
            self.delay_ms.push((now_ns.saturating_sub(p.due_ns)) as f64 / 1e6);
            let age = book.and(self.book_ms.get(&p.token)).map(|ms| now_wall_ms as i64 - *ms as i64);
            let (best_ask, best_bid) = (book.and_then(|b| b.best_ask()), book.and_then(|b| b.best_bid()));
            self.fact(Line::Paper(Fact::Match { id: p.id.clone(), levels: levels.clone(), book_age_ms: age, best_ask, best_bid }));
            if levels.is_empty() { self.missed += 1; continue; }
            self.matched += 1;
            let m = &self.markets[&p.token];
            let (market, fee_rate, fee_exponent, end_ms) = (m.condition.clone(), m.fee_rate, m.fee_exponent, m.end_ms);
            for (i, (price, size)) in levels.into_iter().enumerate() {
                let f = account::Fact::Trade { trade: format!("{}:{i}", p.id), order: p.id.clone(), market: market.clone(), token: p.token.clone(),
                    dir: if p.buy { Dir::Buy } else { Dir::Sell }, status: TradeStatus::Confirmed, size, price, fee_rate, fee_exponent, end_ms };
                let seq = self.next_seq;
                self.acct.apply(seq, &f);
                self.fact(Line::Account(f));
            }
        }
    }

    /// A window's markets, for the simulated matches and the account.
    pub fn window(&mut self, w: &Window) {
        for tok in &w.tokens {
            self.markets.insert(tok.clone(), Market { condition: w.market.clone(), fee_rate: w.fee.rate, fee_exponent: w.fee.exponent, end_ms: w.end * 1000 });
        }
    }

    /// Official result, to the paper account, settled by a simulated redemption (the strategy is told by the engine).
    pub fn resolved(&mut self, market: &str, winner_token: &str) {
        if !self.acct.traded(market) { return; }
        let f = account::Fact::Resolved { market: market.into(), winner: winner_token.into() };
        let seq = self.next_seq;
        if self.acct.apply(seq, &f) { self.fact(Line::Account(f)); }
        let won: Micro = self.acct.open.values().filter(|p| p.market == market && p.token == winner_token).map(|p| p.shares).sum();
        let f = account::Fact::Redeemed { market: market.into(), amount: won, tx: format!("simulated:{market}") };
        let seq = self.next_seq;
        if self.acct.apply(seq, &f) { self.fact(Line::Account(f)); }
    }

    /// One engine event, before the strategy sees it: the age of the books in use, and the trades captured.
    pub fn observe(&mut self, e: &engine_core::event::Event, used_books: bool) {
        if !used_books { return; }
        match &e.payload {
            Payload::BookSnapshot { asset, .. } => { self.book_ms.insert(asset.clone(), e.event_ms.unwrap_or(0)); }
            Payload::BookDelta { changes } => for c in changes { self.book_ms.insert(c.asset.clone(), e.event_ms.unwrap_or(0)); },
            Payload::Trade { asset, price, size, side, id } => {
                self.capture(Cap::Trade { t: e.recv.wall_ns as f64 / 1e6, a: asset.clone(), p: *price, s: *size, buy: *side == Side::Bid, id: id.clone(), ts: e.event_ms });
            }
            _ => {}
        }
    }

    /// The connection in use changed: the strategy gets that connection's books whole (its decisions go out with
    /// the engine's next input).
    pub fn switched(&mut self, books: &Books, assets: &[String], t: f64) { self.strat.borrow_mut().switched(books, assets, t); }

    /// Latent value of each held paper position whose book this event changed (02-conception.md §6).
    pub fn revalue(&mut self, e: &engine_core::event::Event, books: Option<&Books>) {
        if self.acct.open.is_empty() { return; }
        let touched: Vec<String> = match &e.payload {
            Payload::BookSnapshot { asset, .. } => vec![asset.clone()],
            Payload::BookDelta { changes } => changes.iter().map(|c| c.asset.clone()).collect(),
            _ => return,
        };
        let now_ms = e.recv.wall_ns / 1_000_000;
        let age = e.event_ms.map_or(0, |ms| now_ms as i64 - ms as i64);
        for token in touched {
            let Some(p) = self.acct.open.get(&token) else { continue };
            let Some(v) = value(p, books.and_then(|b| b.get(&token)), age) else { continue };
            if self.last_value.insert(token.clone(), v.value) == Some(v.value) { continue; }
            if self.written_ms.get(&token).is_some_and(|w| now_ms < w + 1000) { continue; }
            self.written_ms.insert(token, now_ms);
            self.fact(Line::Paper(Fact::Valuation { valuation: v }));
        }
    }

    pub fn report(&mut self) -> Value {
        let latent: Micro = self.last_value.iter().filter(|(t, _)| self.acct.open.contains_key(*t)).map(|(t, v)| v - self.acct.open[t].cost()).sum();
        let r = json!({ "simulated": true, "decisions": self.decisions, "matched": self.matched, "missed": self.missed, "pending": self.pending.len(),
            "realized_micro": self.acct.realized_total(), "closed_markets": self.acct.realized.len(), "open_positions": self.acct.open.len(), "latent_micro": latent,
            "strategy_model": { "closes": self.strat_closes, "pnl_usd": self.strat_pnl },
            "sign_us": self.sign_us.dist(), "match_late_ms": self.delay_ms.dist(), "strategy_us": self.strat_us.dist() });
        self.fact(Line::Paper(Fact::Report { report: r.clone() }));
        r
    }

    /// Flushes the paper ledger and the capture.
    pub fn close(mut self) {
        drop(self.cap.take());
        if let Some(h) = self.capture.take() { let _ = h.join(); }
        let (tx, _) = std::sync::mpsc::channel();
        drop(std::mem::replace(&mut self.facts, tx));
        if let Some(w) = self.writer.take() { w.join().unwrap().expect("paper ledger writer"); }
    }
}

fn chrono_id(wall_ns: u64) -> String {
    let s = wall_ns / 1_000_000_000;
    let (d, r) = (s / 86_400, s % 86_400);
    // civil date from days since 1970-01-01 (Howard Hinnant's algorithm)
    let z = d as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (day, month) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02}T{:02}-{:02}-{:02}", r / 3600, r % 3600 / 60, r % 60)
}

fn rec_json(r: &Rec) -> Value {
    let op = |b: bool| if b { "BUY" } else { "SELL" };
    match r {
        Rec::Fill { t, slug, side, buy, shares, usd, fees } => json!({ "type": "fill", "t": t, "slug": slug, "side": side.name(), "op": op(*buy), "shares": shares, "usd": usd, "fees": fees }),
        Rec::Miss { t, slug, side, buy } => json!({ "type": "miss", "t": t, "slug": slug, "side": side.name(), "op": op(*buy) }),
        Rec::Skip { t, slug, side, reason } => json!({ "type": "skip", "t": t, "slug": slug, "side": side.name(), "reason": reason }),
        Rec::Close { t, slug, side, pnl, how } => json!({ "type": "close", "t": t, "slug": slug, "side": side.name(), "pnl": pnl, "how": how }),
        Rec::Order { .. } => unreachable!("orders become decisions"),
    }
}

/// The capture thread: old session format lines into `gzip -1`, off the hot thread; `-`: plain lines on stdout, one
/// write per event, for the server that records the session and draws the interface from it.
fn write_capture(path: &str, rx: std::sync::mpsc::Receiver<Cap>) {
    let mut gz = (path != "-").then(|| {
        let file = std::fs::File::create(path).expect("capture file");
        std::process::Command::new("gzip").arg("-1").arg("-c").stdin(std::process::Stdio::piped()).stdout(file).spawn().expect("gzip")
    });
    {
        let mut w: Box<dyn Write> = match gz.as_mut() {
            Some(g) => Box::new(std::io::BufWriter::with_capacity(1 << 20, g.stdin.take().unwrap())),
            None => Box::new(std::io::LineWriter::new(std::io::stdout())),
        };
        let n = |m: Micro| m as f64 / 1e6;
        for c in rx {
            let v = match c {
                Cap::Header(h) => h,
                Cap::Trade { t, a, p, s, buy, id, ts } => json!({ "k": "trade", "t": t, "a": a, "p": n(p), "s": n(s), "side": if buy { "BUY" } else { "SELL" }, "id": id, "ts": ts }),
                Cap::Ev(ev, ts) => match ev {
                    Event::Px { t, src, p } => json!({ "k": "px", "t": t, "src": src, "p": p, "et": ts }),
                    Event::Cl { t, p, ts } => json!({ "k": "cl", "t": t, "p": p, "ts": ts }),
                    Event::Book { t, a, bids, asks } => json!({ "k": "book", "t": t, "a": a, "bids": bids, "asks": asks, "ts": ts }),
                    Event::Delta { t, ch } => json!({ "k": "delta", "t": t, "ch": ch.iter().map(|(a, ask, p, s)| json!([a, if *ask { "A" } else { "B" }, p, s])).collect::<Vec<_>>(), "ts": ts }),
                    Event::Meta { t, w } => json!({ "k": "meta", "t": t, "w": { "slug": w.slug, "start": w.start, "len": w.len, "tokens": w.tokens, "tick": w.tick, "minSize": w.min_size,
                        "fee": w.fee.map(|f| json!({ "rate": f.rate, "exponent": f.exponent, "takerOnly": true })), "twap": w.twap, "priceToBeat": if w.price_to_beat.is_finite() { json!(w.price_to_beat) } else { Value::Null } } }),
                    Event::Res { t, slug, winner } => json!({ "k": "res", "t": t, "slug": slug, "winner": winner }),
                    Event::Ptb { t, slug, price_to_beat } => json!({ "k": "ptb", "t": t, "slug": slug, "priceToBeat": price_to_beat }),
                    Event::Rtt { .. } | Event::Clock { .. } | Event::Other { .. } => continue,
                },
            };
            if writeln!(w, "{v}").is_err() { return; }
        }
        let _ = w.flush();
    }
    if let Some(mut g) = gz { let _ = g.wait(); }
}
