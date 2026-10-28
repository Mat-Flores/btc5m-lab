//! Engine binary: the live shell around the engine (`engine_app::engine`). Sources come from the configuration; an
//! unlisted source is neither loaded nor connected. Hot thread: feeds, health, connection choice, the engine (books,
//! strategies, commands), the venue (paper or live). Cold thread: window discovery (HTTP). Ledger thread: facts.
//! Usage: engine <config.json>
//! Config: every key, with its default, in `config.example.json` (market: BTC 5 minutes by default).
//!
//! A book source may run several connections: the delay of a connection depends on the path it lands on (measured:
//! 82 ms against 9 ms at the same moment). Each connection keeps its own books; the engine uses the books of the
//! freshest valid connection, so a slow path is left aside and a cut costs no wait.
use adapter_polymarket::account::{self as venue, Holding, Redemption};
use adapter_polymarket::exec::{Clob, Creds, ALLOWED, HOST};
use engine_app::engine::{Command, Decision, Engine, EngineInput, Venue as _};
use engine_core::guard::NetGuard;
use adapter_polymarket::sign::{self, Signer};
use adapter_polymarket::windows;
use engine_core::market::{MarketSpec, Window};
use engine_core::account::{self, Account};
use engine_core::book::{Books, Integrity};
use engine_core::clock::Clock;
use engine_core::event::{Payload, SourceId};
use engine_core::feed::{self, Cmd, Out};
use engine_core::ledger::{read_all, spawn_writer, Ledger, Stamped, SyncPolicy};
use engine_core::money::Micro;
use engine_core::order::{Input, Phase};
use engine_core::reconcile::{reconcile, Venue, Verdict};
use engine_core::supervisor::Health;
use engine_core::value::{value, StopLoss, Trigger, Valuation};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

mod live;
mod paper;

fn one() -> usize { 1 }
fn ten() -> u64 { 10 }
#[derive(Deserialize)]
struct SourceCfg { name: String, adapter: String, #[serde(default)] topics: Vec<String>, #[serde(default = "one")] connections: usize }
/// The account the engine keeps the books of: `address` holds the shares (read in public at data-api).
#[derive(Deserialize)]
struct AccountCfg { #[serde(default)] address: String, #[serde(default)] stop_loss: StopLoss, #[serde(default = "ten")] reconcile_s: u64 }
/// The market of the original application: Polymarket BTC "Up or Down" 5 minutes.
fn btc_5m() -> MarketSpec { MarketSpec { venue: "polymarket".into(), asset: "btc".into(), horizon_s: 300 } }
#[derive(Deserialize)]
struct Config { #[serde(default = "btc_5m")] market: MarketSpec, ledger: String, seconds: u64, sources: Vec<SourceCfg>, #[serde(default)] bench_orders: bool, #[serde(default)] account: Option<AccountCfg>,
    /// Paper mode (phase 9): the reference strategy on the live feeds, nothing sent (`paper.rs`).
    #[serde(default)] paper: Option<paper::PaperCfg>,
    /// Live mode (phase 10): the paper strategy's decisions sent for real (`live.rs`); needs `paper` and `account`.
    #[serde(default)] live: Option<live::LiveCfg> }

/// What the cold thread read about the account. `None`: not read.
struct Reads { holdings: Option<Vec<Holding>>, redemptions: Option<Vec<Redemption>>, balance: Option<Micro>, open_orders: Option<BTreeSet<String>> }
enum Cold { Account(Reads), Resolved { market: String, winner: String, up_won: bool, price_to_beat: Option<Micro> } }

/// One ledger line: an accounting fact (read back at startup) or any other fact.
#[derive(Serialize)]
#[serde(untagged)]
enum Line { Account(account::Fact), Engine(Fact) }

/// The only sender of the ledger: it knows the seq each fact gets, so a figure can name the facts it comes from.
struct Rec { tx: std::sync::mpsc::Sender<Stamped<Line>>, clock: Arc<Clock>, next: Cell<u64> }
impl Rec {
    fn send(&self, l: Line) -> u64 { let s = self.next.get(); self.next.set(s + 1); let _ = self.tx.send(Stamped { stamp: self.clock.now(), fact: l }); s }
    /// Writes an accounting fact only if it changes the account (a read repeated every few seconds is not a fact).
    fn keep(&self, a: &mut Account, f: account::Fact) -> bool { a.apply(self.next.get(), &f) && { self.send(Line::Account(f)); true } }
}

/// A connection whose data arrives this late is replaced (new path) rather than drained: measured, a slow path built
/// up 27 s of backlog, and draining it delayed the other connections.
const SLOW_MS: f64 = 1_000.0;
const RECYCLE_COOLDOWN_NS: u64 = 10_000_000_000;
/// A requested snapshot that has not come is asked for again after this long.
const RESYNC_RETRY_NS: u64 = 5_000_000_000;
/// A finished window's books are dropped this long after its end.
const AFTER_END_S: u64 = 15;
/// Smoothing of each connection's age, and the advantage needed to switch to another connection.
const EWMA: f64 = 0.02;
const SWITCH_MS: f64 = 5.0;

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Fact {
    Boot { connections: Vec<String> },
    SourceState { connection: String, health: Health },
    SourceRecovered { connection: String, ms_invalid: f64 },
    ConnectionRecycled { connection: String, age_ms: f64 },
    Selected { source: String, connection: String, age_ms: f64 },
    Window { slug: String, start: u64, up: String, down: String, tick: i64, min_size: i64, fee_rate: i64, fee_exponent: u32, taker_delay: bool },
    WindowDropped { slug: String },
    BookBroken { connection: String, asset: String, why: String },
    BookRecovered { connection: String, asset: String, ms_unusable: f64 },
    DecodeError { connection: String, error: String, head: String },
    BookUsable { asset: String, ms_after_open: i64 },
    /// Latent value of a held position, written when it changes.
    Valuation { valuation: Valuation },
    /// A stop loss decision and its inputs. `exit_sent`: no order loop is assembled before phase 9, so nothing is sent.
    Defense { trigger: Trigger, exit_sent: bool },
    Reconcile { verdict: Verdict },
    /// The venue's own value of a holding, against ours (TODO §4.2).
    VenueValue { token: String, venue: Micro, ours: Option<Micro> },
    /// Live mode armed: the signer (EOA) and the wallet it signs for; never the key.
    Armed { eoa: String, wallet: String, clock_synced: bool },
    /// Emergency stop asked (SIGUSR1): no new opening; held positions run to their settlement.
    EmergencyStop {},
    /// A strategy decision the guards refused, or that the live mode does not send (`why`).
    LiveRefused { slug: String, why: String },
    /// A real order, signed and handed to the sender; `hash` is its EIP-712 hash.
    LiveOrder { n: u32, slug: String, token: String, limit: Micro, maker: u64, taker: u64, hash: String },
    /// What the venue answered (and queries learnt): the machine's phase, the venue's id, the instants.
    LiveAnswer { n: u32, phase: Phase, id: Option<String>, decision_to_written_us: Option<f64>, written_to_answer_ms: Option<f64>, late: bool, status: Option<u16>, body: Option<String>, back_off_s: Option<u64> },
    Report { report: serde_json::Value },
}

/// Why a book is unusable on a connection, and since when (monotonic ns).
struct Broken { since: u64, last_resync: u64 }

/// One connection of one source.
struct Conn {
    label: String,
    source: usize,
    cmd: mpsc::UnboundedSender<Cmd>,
    broken: HashMap<String, Broken>,
    valid: bool,
    invalid_since: Option<u64>,
    age_ewma: Option<f64>,
    recycled_at: Option<u64>,
    recycles: u64,
    msgs: u64,
    last_recv: Option<u64>,
    gaps_ms: Hist,
    invalid: u64,
    invalid_ms: Vec<f64>,
    ages_ms: HashMap<&'static str, Hist>,
    failures: HashMap<String, u64>,
}

impl Conn {
    fn resync(&self, a: &str) { let _ = self.cmd.send(Cmd::Unsubscribe(vec![a.to_string()])); let _ = self.cmd.send(Cmd::Subscribe(vec![a.to_string()])); }

    /// A book became unusable: count it, record it, and ask for a new snapshot at once (measured: a forced snapshot
    /// restores a book in 76 ms at p50; waiting for the source's own took 0.9 s).
    fn broke(&mut self, asset: String, why: Integrity, now_ns: u64, fact: &impl Fn(Fact)) {
        *self.failures.entry(format!("{why:?}").split([' ', '{']).next().unwrap_or("").to_string()).or_default() += 1;
        fact(Fact::BookBroken { connection: self.label.clone(), asset: asset.clone(), why: format!("{why:?}") });
        if !self.broken.contains_key(&asset) { self.broken.insert(asset.clone(), Broken { since: now_ns, last_resync: now_ns }); self.resync(&asset); }
    }

    /// Usable for trading: valid, and every book of the live windows usable (`books`: this connection's, in the engine).
    fn ready(&self, books: Option<&Books>, assets: &[&String]) -> bool { self.valid && books.is_some_and(|b| assets.iter().all(|a| b.get(a).is_some_and(|x| x.usable()))) }
}

/// Order path bench (phase 6): from the decision instant to the last byte of a signed order written, under the
/// real feed load. Throwaway key and credentials; the sink is a local socket that discards: nothing reaches a venue.
struct Bench { signer: Signer, creds: Creds, sink: engine_core::http::Conn, next_ns: u64, salt: u64, last_token: String, total_us: Hist, sign_us: Hist, first_us: Vec<f64>, value_us: Hist }
const BENCH_EVERY_NS: u64 = 100_000_000;

impl Bench {
    async fn new() -> Bench {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || { use std::io::Read; let (mut s, _) = l.accept().unwrap(); let mut b = [0u8; 65536]; while s.read(&mut b).map(|k| k > 0).unwrap_or(false) {} });
        let sink = engine_core::http::Conn::new(engine_core::http::dial("127.0.0.1", port, false).await.unwrap());
        let signer = Signer::from_bytes(&[0x11; 32], "0x7a57144fafde87f5ce4a880d570bae58eebc83f2").unwrap();
        let creds = Creds { key: "00000000-0000-4000-8000-000000000000".into(), secret: "c2VjcmV0c2VjcmV0c2VjcmV0c2VjcmV0c2VjcmV0MDA=".into(), passphrase: "bench".into() };
        Bench { signer, creds, sink, next_ns: 0, salt: 1, last_token: String::new(), total_us: Hist::default(), sign_us: Hist::default(), first_us: vec![], value_us: Hist::default() }
    }

    /// One order on `token` at a mid-grid price: amounts, signature, body, headers, write.
    async fn order(&mut self, token: &str, tick: i64, clock: &Clock) {
        let t0 = clock.now();
        let Ok((maker, taker)) = sign::amounts(sign::Side::Buy, 5_000_000, 500_000, tick) else { return };
        self.salt += 1;
        let o = sign::Order { salt: self.salt, token_id: token.into(), maker_amount: maker, taker_amount: taker, side: sign::Side::Buy, timestamp_ms: t0.wall_ns / 1_000_000 };
        let body = self.signer.body(&o, &self.creds.key);
        let t1 = clock.now();
        let req = adapter_polymarket::exec::order_request(adapter_polymarket::exec::HOST, &self.signer, &self.creds, &body, t0.wall_ns / 1_000_000_000).unwrap();
        self.sink.write(&req).await.unwrap();
        let us = (clock.now().mono_ns - t0.mono_ns) as f64 / 1000.0;
        self.total_us.push(us);
        self.sign_us.push((t1.mono_ns - t0.mono_ns) as f64 / 1000.0);
        if token != self.last_token { self.first_us.push(us); self.last_token = token.into(); }
        self.next_ns = t0.mono_ns + BENCH_EVERY_NS;
    }
}

async fn recv<T>(r: &mut Option<mpsc::UnboundedReceiver<T>>) -> Option<T> { match r { Some(r) => r.recv().await, None => std::future::pending().await } }

fn kind(p: &Payload) -> &'static str {
    match p { Payload::Quote { .. } => "quote", Payload::BookSnapshot { .. } => "book", Payload::BookDelta { .. } => "delta", Payload::TickChange { .. } => "tick", Payload::Trade { .. } => "trade", Payload::Oracle { .. } => "oracle", Payload::Twap { .. } => "twap", Payload::Fill { .. } => "fill", Payload::OrderClosed { .. } => "closed" }
}
fn q(v: &mut [f64], p: f64) -> Option<f64> { if v.is_empty() { return None; } v.sort_by(|a, b| a.partial_cmp(b).unwrap()); Some((v[((p * v.len() as f64) as usize).min(v.len() - 1)] * 10.0).round() / 10.0) }
fn dist(v: &mut [f64]) -> serde_json::Value { serde_json::json!({ "n": v.len(), "p50": q(v, 0.5), "p99": q(v, 0.99), "p999": q(v, 0.999), "max": q(v, 1.0) }) }

/// Distribution of a per-message measure in bounded memory (a Vec of every sample would grow by ~1 GB a day).
/// Logarithmic buckets of 1 %: a quantile is within 0.5 % of the true value; max is exact.
#[derive(Default)]
struct Hist { n: u64, max: Option<f64>, buckets: std::collections::BTreeMap<i32, u64> }
const FLOOR: f64 = 0.001;
impl Hist {
    fn key(v: f64) -> i32 { if v.abs() < FLOOR { 0 } else { (1 + ((v.abs() / FLOOR).ln() / 1.01f64.ln()) as i32) * v.signum() as i32 } }
    fn value(k: i32) -> f64 { if k == 0 { 0.0 } else { FLOOR * 1.01f64.powf(k.abs() as f64 - 0.5) * k.signum() as f64 } }
    fn push(&mut self, v: f64) { self.n += 1; self.max = Some(self.max.map_or(v, |m| m.max(v))); *self.buckets.entry(Self::key(v)).or_default() += 1; }
    fn q(&self, p: f64) -> Option<f64> {
        let rank = ((p * self.n as f64) as u64).min(self.n.checked_sub(1)?);
        let mut seen = 0;
        self.buckets.iter().find(|(_, c)| { seen += **c; seen > rank }).map(|(k, _)| (Self::value(*k) * 10.0).round() / 10.0)
    }
    fn dist(&self) -> serde_json::Value { serde_json::json!({ "n": self.n, "p50": self.q(0.5), "p99": self.q(0.99), "p999": self.q(0.999), "max": self.max.map(|m| (m * 10.0).round() / 10.0) }) }
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: engine <config.json>");
    let mut cfg: Config = serde_json::from_str(&std::fs::read_to_string(&path).expect("config")).expect("config json");
    let clock = Arc::new(Clock::new());
    let (ledger, rep) = Ledger::open(&cfg.ledger).expect("ledger");
    if rep.cut_bytes > 0 { eprintln!("ledger: torn tail of {} bytes cut", rep.cut_bytes); }
    // restart: the account is what the ledger says, before anything else (02-conception.md §2.3)
    let mut book_keeping = Account::default();
    for e in read_all::<serde_json::Value>(&cfg.ledger).expect("ledger read") {
        if let Ok(f) = serde_json::from_value::<account::Fact>(e.fact) { book_keeping.apply(e.seq, &f); }
    }
    let (facts, _durable, writer) = spawn_writer::<Line>(ledger, SyncPolicy::default());
    let recorder = Rec { tx: facts.clone(), clock: clock.clone(), next: Cell::new(rep.entries) };
    let rec = &recorder;
    let fact = move |f: Fact| { rec.send(Line::Engine(f)); };

    // live mode: armed before anything connects (passphrase on stdin), or the run stops here
    let armed = cfg.live.as_ref().map(|l| {
        let acc = cfg.account.as_ref().expect("live: needs \"account\" (reconciliation)");
        assert!(cfg.paper.is_some(), "live: needs \"paper\" (its strategy decides)");
        let a = live::arm(l);
        assert!(acc.address.is_empty() || a.signer.wallet.eq_ignore_ascii_case(&acc.address), "live: account.address is not the wallet of the key");
        fact(Fact::Armed { eoa: a.signer.address.clone(), wallet: a.signer.wallet.clone(), clock_synced: a.clock_synced });
        a
    });
    let auth = armed.as_ref().map(|a| (a.signer.clone(), a.creds.clone()));
    let strategy_source = cfg.paper.as_ref().map(|p| p.strategy.source.clone());

    // cold thread: window discovery ahead of time, official results, account reads
    let (win_tx, mut win_rx) = mpsc::unbounded_channel::<Window>();
    let (cold_tx, mut cold_rx) = mpsc::unbounded_channel::<Cold>();
    // armed, the account is the key's wallet
    let address = cfg.account.as_ref().map(|a| (armed.as_ref().map_or(a.address.clone(), |x| x.signer.wallet.clone()), a.reconcile_s));
    // windows of positions held at startup still need their result
    let mut unresolved: Vec<u64> = book_keeping.open.values().filter(|p| p.won.is_none()).map(|p| (p.end_ms / 1000).saturating_sub(cfg.market.horizon_s)).collect();
    let market = cfg.market.clone();
    std::thread::Builder::new().name("cold".into()).spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            let mut known: Vec<u64> = vec![];
            let (mut next_result, mut next_account) = (0u64, 0u64);
            let mut clob_reads: Option<Clob> = None;
            loop {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                let s = market.window_start(now);
                // 3 windows ahead: gamma lists them an hour early, and a 7 min gamma outage (2026-10-21) cost a window with 1
                for start in (0..=3).map(|k| s + k * market.horizon_s) {
                    if known.contains(&start) { continue; }
                    if let Ok(Some(w)) = windows::fetch(&market, start).await { known.push(start); unresolved.push(start); if win_tx.send(w).is_err() { return; } }
                }
                known.retain(|x| *x + 3600 > s);
                // results: gamma publishes them some time after the end; asked every 15 s, given up after an hour
                if now >= next_result {
                    next_result = now + 15;
                    for start in unresolved.clone().into_iter().filter(|x| x + market.horizon_s + 30 < now) {
                        if let Ok(Some(w)) = windows::fetch(&market, start).await {
                            if let Some(o) = w.winner {
                                unresolved.retain(|x| *x != start);
                                let up_won = o == 0;
                                let winner = w.tokens[o].clone();
                                if cold_tx.send(Cold::Resolved { market: w.market, winner, up_won, price_to_beat: w.price_to_beat }).is_err() { return; }
                            }
                        }
                    }
                    unresolved.retain(|x| x + 3600 > now);
                }
                if let Some((addr, every)) = address.as_ref().filter(|_| now >= next_account) {
                    next_account = now + every;
                    // balance and open orders need the account's API credentials, which only arming gives
                    let (mut balance, mut open_orders) = (None, None);
                    if let Some((s, c)) = auth.as_ref() {
                        let reads = clob_reads.get_or_insert_with(|| Clob::new(HOST, 443, true, NetGuard::new(&ALLOWED, Arc::new(std::sync::atomic::AtomicBool::new(false)))));
                        if let Ok(q) = venue::balance_request(HOST, s, c, now) { balance = reads.get(&q, Duration::from_secs(5)).await.ok().and_then(|r| venue::read_balance(&r)); }
                        let (mut ids, mut cursor) = (BTreeSet::new(), None::<String>);
                        for _ in 0..20 {
                            let Ok(q) = venue::open_orders_request(HOST, s, c, cursor.as_deref(), now) else { break };
                            let Some((page, next)) = reads.get(&q, Duration::from_secs(5)).await.ok().and_then(|r| venue::read_open_orders(&r)) else { break };
                            ids.extend(page);
                            match next { Some(n) => cursor = Some(n), None => { open_orders = Some(ids); break; } }
                        }
                    }
                    let r = Reads { holdings: venue::holdings(addr).await.ok(), redemptions: venue::redemptions(addr).await.ok(), balance, open_orders };
                    if cold_tx.send(Cold::Account(r)).is_err() { return; }
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }).unwrap();

    // the strategies the engine runs: the reference of the paper mode (refused on another horizon than its 300 s)
    let mut strategies: Vec<Box<dyn engine_app::engine::Strategy>> = vec![];
    let mut paper = cfg.paper.take().map(|p| {
        assert!(cfg.sources.iter().any(|s| s.name == p.strategy.source), "paper: strategy source {} not configured", p.strategy.source);
        assert_eq!(cfg.market.horizon_s, 300, "paper: the reference strategy is built for 5-minute windows");
        let oracle = cfg.sources.iter().find(|s| s.adapter == "chainlink-rtds").map_or_else(|| "chainlink".to_string(), |s| s.name.clone());
        let (p, s) = paper::Paper::open(p, clock.clone(), oracle);
        strategies.push(s);
        p
    });
    // hot thread: every configured source and connection, books, health, live value of held positions
    let stop_loss = cfg.account.as_ref().map(|a| a.stop_loss).unwrap_or_default();
    let mut acct = book_keeping;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let report = rt.block_on(async {
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Out>();
        let mut conns: Vec<Conn> = vec![];
        // one engine source per connection (SourceId = connection index), named after its configured source
        let mut engine_sources: Vec<(String, bool)> = vec![];
        for (si, s) in cfg.sources.iter().enumerate() {
            for k in 0..s.connections.max(1) {
                let (a, has_books) = engine_app::registry::feed_adapter(&s.adapter).unwrap_or_else(|| panic!("unknown adapter {}", s.adapter));
                let (cmd, cmd_rx) = mpsc::unbounded_channel();
                let label = if s.connections > 1 { format!("{}#{}", s.name, k + 1) } else { s.name.clone() };
                tokio::spawn(feed::run(a, conns.len() as SourceId, s.topics.clone(), clock.clone(), cmd_rx, out_tx.clone()));
                engine_sources.push((s.name.clone(), has_books));
                conns.push(Conn { label, source: si, cmd, broken: HashMap::new(),
                    valid: false, invalid_since: None, age_ewma: None, recycled_at: None, recycles: 0, msgs: 0, last_recv: None, gaps_ms: Hist::default(), invalid: 0, invalid_ms: vec![], ages_ms: HashMap::new(), failures: HashMap::new() });
            }
        }
        // live: the user channel (our trades) as one more connection, outside the configured sources; and the sender
        let user_conn = armed.as_ref().map(|a| {
            let (cmd, cmd_rx) = mpsc::unbounded_channel();
            let creds = Creds { key: a.creds.key.clone(), secret: a.creds.secret.clone(), passphrase: a.creds.passphrase.clone() };
            tokio::spawn(feed::run(Box::new(adapter_polymarket::user::User { creds }), conns.len() as SourceId, vec![], clock.clone(), cmd_rx, out_tx.clone()));
            engine_sources.push(("user".into(), false));
            conns.push(Conn { label: "user".into(), source: cfg.sources.len(), cmd, broken: HashMap::new(),
                valid: false, invalid_since: None, age_ewma: None, recycled_at: None, recycles: 0, msgs: 0, last_recv: None, gaps_ms: Hist::default(), invalid: 0, invalid_ms: vec![], ages_ms: HashMap::new(), failures: HashMap::new() });
            conns.len() - 1
        });
        let mut eng = Engine::new(engine_sources, std::mem::take(&mut strategies)).unwrap_or_else(|e| panic!("engine: {e}"));
        let mut cmds: Vec<Command> = vec![];
        let armed_flag = Arc::new(std::sync::atomic::AtomicBool::new(armed.is_some()));
        let mut seen_tokens: HashSet<String> = HashSet::new();
        let (mut real, mut done_rx) = match (armed, cfg.live.as_ref()) {
            (Some(a), Some(l)) => { let (v, rx) = live::LiveVenue::new(a, l, armed_flag.clone(), clock.clone()); (Some(v), Some(rx)) }
            _ => (None, None),
        };
        fact(Fact::Boot { connections: conns.iter().map(|c| c.label.clone()).collect() });
        let book_conns: Vec<usize> = (0..conns.len()).filter(|&i| eng.books[i].is_some()).collect();
        let mut selected: Option<usize> = None;
        let mut used_ages: HashMap<&'static str, Hist> = HashMap::new(); // ages of what the engine uses
        let mut used_ms_without_ready: f64 = 0.0;
        let mut switches = 0u64;
        let mut engine_us = Hist::default();
        let mut bench = if cfg.bench_orders { Some(Bench::new().await) } else { None };
        let mut live: Vec<Window> = vec![];
        let mut opens_while_running: HashMap<String, u64> = HashMap::new();
        let mut recovered: Vec<f64> = vec![];
        let mut first_usable_ms: Vec<f64> = vec![];
        let mut last_value: HashMap<String, Micro> = HashMap::new();
        let mut fired: HashSet<String> = HashSet::new();
        let mut verdict: Option<Verdict> = None;
        let mut verdicts: HashMap<&'static str, u64> = HashMap::new();
        let mut value_us = Hist::default();
        let boot_s = clock.now().wall_ns / 1_000_000_000;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(cfg.seconds);
        // SIGTERM ends the run like its deadline (report, ledgers and capture flushed); SIGUSR1 is the emergency stop
        use tokio::signal::unix::{signal, SignalKind};
        let (mut term, mut usr1) = (signal(SignalKind::terminate()).expect("SIGTERM"), signal(SignalKind::user_defined1()).expect("SIGUSR1"));
        let mut tick = tokio::time::interval(Duration::from_millis(50));
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => break,
                _ = term.recv() => break,
                _ = usr1.recv() => if let Some(l) = real.as_mut() {
                    l.guards.stop.store(true, std::sync::atomic::Ordering::Release);
                    fact(Fact::EmergencyStop {});
                },
                _ = tick.tick() => {
                    let now = clock.now();
                    let now_s = now.wall_ns / 1_000_000_000;
                    for &i in &book_conns {
                        let c = &mut conns[i];
                        let quiet = eng.books[i].as_mut().unwrap().quiet(now.mono_ns);
                        for (asset, why) in quiet { c.broke(asset, why, now.mono_ns, &fact); }
                        // a requested snapshot that did not come is asked for again
                        let due: Vec<String> = c.broken.iter().filter(|(_, b)| now.mono_ns - b.last_resync >= RESYNC_RETRY_NS).map(|(a, _)| a.clone()).collect();
                        for a in due { c.broken.get_mut(&a).unwrap().last_resync = now.mono_ns; c.resync(&a); }
                        // finished windows: unsubscribe and forget their books
                        for w in live.iter().filter(|w| w.end + AFTER_END_S < now_s) {
                            let _ = c.cmd.send(Cmd::Unsubscribe(vec![w.up().clone(), w.down().clone()]));
                            for a in [w.up(), w.down()] { eng.books[i].as_mut().unwrap().remove(a); c.broken.remove(a); }
                        }
                    }
                    for w in live.iter().filter(|w| w.end + AFTER_END_S < now_s) { fact(Fact::WindowDropped { slug: w.slug.clone() }); }
                    live.retain(|w| w.end + AFTER_END_S >= now_s);
                    // the freshest ready connection is used; switching needs a clear advantage
                    let assets: Vec<&String> = live.iter().filter(|w| w.end > now_s).flat_map(|w| [w.up(), w.down()]).collect();
                    let best = book_conns.iter().copied().filter(|&i| conns[i].ready(eng.books[i].as_ref(), &assets)).min_by(|&a, &b| conns[a].age_ewma.unwrap_or(f64::MAX).total_cmp(&conns[b].age_ewma.unwrap_or(f64::MAX)));
                    let keep = selected.is_some_and(|s| conns[s].ready(eng.books[s].as_ref(), &assets));
                    let switch = match (selected, best) {
                        (_, None) => None,
                        (None, Some(b)) => Some(b),
                        (Some(s), Some(b)) if !keep => Some(b).filter(|&b| b != s),
                        (Some(s), Some(b)) => Some(b).filter(|&b| b != s && conns[b].age_ewma.unwrap_or(f64::MAX) + SWITCH_MS < conns[s].age_ewma.unwrap_or(f64::MAX)),
                    };
                    if let Some(b) = switch {
                        if selected.is_some() { switches += 1; }
                        selected = Some(b);
                        // the books strategies read are those of the connection in use
                        for (i, u) in eng.used.iter_mut().enumerate() { *u = i == b; }
                        fact(Fact::Selected { source: cfg.sources[conns[b].source].name.clone(), connection: conns[b].label.clone(), age_ms: conns[b].age_ewma.unwrap_or(0.0) });
                        if let Some(p) = paper.as_mut() {
                            let owned: Vec<String> = assets.iter().map(|a| a.to_string()).collect();
                            p.switched(eng.books[b].as_ref().unwrap(), &owned, now.wall_ns as f64 / 1e6);
                        }
                    }
                    if let Some(p) = paper.as_mut() { p.due(now.mono_ns, selected.and_then(|s| eng.books[s].as_ref()), now.wall_ns / 1_000_000); }
                    let t0 = clock.now().mono_ns;
                    eng.on(EngineInput::Tick, now, &mut cmds);
                    if let Some(p) = paper.as_mut() { p.after(&cmds, now.mono_ns, (clock.now().mono_ns - t0) as f64 / 1000.0); }
                    if !book_conns.is_empty() && !assets.is_empty() && best.is_none() { used_ms_without_ready += 50.0; }
                }
                Some(w) = win_rx.recv() => {
                    fact(Fact::Window { slug: w.slug.clone(), start: w.start, up: w.up().clone(), down: w.down().clone(), tick: w.tick, min_size: w.min_size, fee_rate: w.fee.rate, fee_exponent: w.fee.exponent, taker_delay: w.taker_delay });
                    // the opening delay is measured only for windows that open while the engine runs
                    if w.start > boot_s { opens_while_running.insert(w.up().clone(), w.start); opens_while_running.insert(w.down().clone(), w.start); }
                    for &i in &book_conns {
                        for a in [w.up(), w.down()] { let b = eng.books[i].as_mut().unwrap().entry(a); if b.tick == 0 { b.tick = w.tick; } }
                        let _ = conns[i].cmd.send(Cmd::Subscribe(vec![w.up().clone(), w.down().clone()]));
                    }
                    if let Some(p) = paper.as_mut() { p.window(&w); }
                    if let Some(l) = real.as_mut() { l.window(&w); }
                    let now = clock.now();
                    eng.on(EngineInput::Window(w.clone()), now, &mut cmds);
                    if let Some(p) = paper.as_mut() { p.after(&cmds, now.mono_ns, (clock.now().mono_ns - now.mono_ns) as f64 / 1000.0); }
                    if let Some(u) = user_conn { let _ = conns[u].cmd.send(Cmd::Subscribe(vec![w.market.clone()])); }
                    seen_tokens.insert(w.up().clone());
                    seen_tokens.insert(w.down().clone());
                    live.push(w);
                }
                Some(c) = cold_rx.recv() => match c {
                    // only markets the engine traded
                    Cold::Resolved { market, winner, up_won, price_to_beat } => {
                        let now = clock.now();
                        eng.on(EngineInput::Resolved { market: market.clone(), winner_token: winner.clone(), up_won, price_to_beat }, now, &mut cmds);
                        if let Some(p) = paper.as_mut() { p.after(&cmds, now.mono_ns, (clock.now().mono_ns - now.mono_ns) as f64 / 1000.0); p.resolved(&market, &winner); }
                        if acct.traded(&market) { rec.keep(&mut acct, account::Fact::Resolved { market, winner }); }
                    }
                    Cold::Account(r) => {
                        let traded = acct.shares();
                        for h in r.holdings.iter().flatten().filter(|h| traded.contains_key(&h.token)) {
                            rec.keep(&mut acct, account::Fact::Fees { token: h.token.clone(), amount: h.fees });
                            if acct.open.contains_key(&h.token) { fact(Fact::VenueValue { token: h.token.clone(), venue: h.value, ours: last_value.get(&h.token).copied() }); }
                        }
                        for x in r.redemptions.iter().flatten() {
                            if acct.traded(&x.market) { rec.keep(&mut acct, account::Fact::Redeemed { market: x.market.clone(), amount: x.amount, tx: x.tx.clone() }); }
                        }
                        // a balance read is a fact when it changes, or when a money fact came since the last one (the
                        // reconciliation needs a balance recorded after the last flow)
                        if let Some(b) = r.balance.filter(|b| acct.balance.is_none_or(|(x, s)| x != *b || acct.last_flow_seq.is_some_and(|f| f > s))) {
                            rec.keep(&mut acct, account::Fact::Balance { amount: b });
                        }
                        // the engine answers for its own markets only: holdings of older markets (the wallet's past) are not its books
                        let v = Venue { shares: r.holdings.map(|hs| hs.into_iter().filter(|h| seen_tokens.contains(&h.token) || traded.contains_key(&h.token)).map(|h| (h.token, h.shares)).collect()),
                            balance: r.balance, open_orders: r.open_orders };
                        let now = reconcile(&acct, &v, &real.as_ref().map(|l| l.resting()).unwrap_or_default());
                        *verdicts.entry(match now { Verdict::Agree => "agree", Verdict::Pending { .. } => "pending", Verdict::Disagree { .. } => "disagree" }).or_default() += 1;
                        if verdict.as_ref() != Some(&now) {
                            if matches!(now, Verdict::Disagree { .. }) { eprintln!("ALERT reconciliation: {}", serde_json::to_string(&now).unwrap()); }
                            fact(Fact::Reconcile { verdict: now.clone() });
                            verdict = Some(now);
                        }
                    }
                },
                Some(d) = recv(&mut done_rx) => if let Some(l) = real.as_mut() {
                    l.inbox.push(d);
                    let now = clock.now();
                    let mut answers = vec![];
                    l.poll(now, &mut answers);
                    for f in l.facts.drain(..) { fact(f); }
                    // trades the user channel told before the answer
                    for (o, t, st, size, price) in std::mem::take(&mut l.early) { l.trade(o, t, st, size, price, rec, &mut acct); }
                    for (at, ev) in answers { eng.on(EngineInput::Venue(ev), at, &mut cmds); }
                    if let Some(p) = paper.as_mut() { p.after(&cmds, now.mono_ns, 0.0); }
                },
                Some(o) = out_rx.recv() => match o {
                    Out::Event(e) if matches!(e.payload, Payload::Fill { .. } | Payload::OrderClosed { .. }) => match (real.as_mut(), e.payload) {
                        (Some(l), Payload::Fill { order, trade, status, size, price }) => l.trade(order, trade, status, size, price, rec, &mut acct),
                        (Some(l), Payload::OrderClosed { order }) => if let Some((_, o)) = l.orders.iter_mut().find(|(_, o)| o.id.as_deref() == Some(order.as_str())) { let _ = o.apply(Input::Closed); },
                        _ => {}
                    },
                    Out::Frame { .. } | Out::Connected { .. } | Out::Sent { .. } => {} // traced runs only (source-tracing)
                    Out::DecodeError { source, error, head, .. } => fact(Fact::DecodeError { connection: conns[source as usize].label.clone(), error, head }),
                    Out::Health { source, health, at } => {
                        let c = &mut conns[source as usize];
                        match &health {
                            Health::Invalid(_) => {
                                c.valid = false; c.invalid += 1; c.invalid_since.get_or_insert(at.mono_ns);
                                // changes may have been lost while the connection was down: books restart empty and
                                // become usable again only with their new snapshot
                                if eng.books[source as usize].is_some() {
                                    let mut fresh = engine_app::engine::new_books();
                                    for w in &live { for a in [w.up(), w.down()] { fresh.entry(a).tick = w.tick; } }
                                    eng.books[source as usize] = Some(fresh); c.broken.clear();
                                }
                            }
                            Health::Valid => { c.valid = true; if let Some(t) = c.invalid_since.take() {
                                let ms = (at.mono_ns - t) as f64 / 1e6;
                                c.invalid_ms.push(ms);
                                fact(Fact::SourceRecovered { connection: c.label.clone(), ms_invalid: ms });
                            } }
                        }
                        fact(Fact::SourceState { connection: c.label.clone(), health });
                    }
                    Out::Event(e) => {
                        let ci = e.source as usize;
                        let k = kind(&e.payload);
                        let is_quote = k == "quote";
                        let age = e.event_ms.filter(|_| k != "book").map(|ms| (e.recv.wall_ns as f64 / 1e6) - ms as f64);
                        let has_books = eng.books[ci].is_some();
                        let used = !has_books || selected == Some(ci);
                        // books of held positions (or of the bench's) this event changes; nothing is allocated otherwise
                        let mut touched: Vec<String> = if used && (!acct.open.is_empty() || bench.is_some()) { match &e.payload {
                            Payload::BookSnapshot { asset, .. } | Payload::TickChange { asset, .. } => vec![asset.clone()],
                            Payload::BookDelta { changes } => changes.iter().map(|c| c.asset.clone()).collect(),
                            _ => vec![],
                        } } else { vec![] };
                        touched.sort_unstable();
                        touched.dedup();
                        // late alone means a slow path; late together means the source itself (measured: all three
                        // connections 1.86 s late at once): only the first is worth a new connection
                        let other_fresh = conns.iter().enumerate().any(|(j, o)| j != ci && o.source == conns[ci].source && o.valid && o.age_ewma.is_some_and(|x| x < SLOW_MS / 4.0));
                        let c = &mut conns[ci];
                        c.msgs += 1;
                        // interval between frames (events of one frame share its stamp), to calibrate silence thresholds
                        if c.last_recv != Some(e.recv.mono_ns) { if let Some(l) = c.last_recv.replace(e.recv.mono_ns) { c.gaps_ms.push((e.recv.mono_ns - l) as f64 / 1e6); } }
                        // a snapshot's timestamp is its last change, not its sending time: it is not an age
                        if let Some(a) = age {
                            c.ages_ms.entry(k).or_default().push(a);
                            if k == "delta" { c.age_ewma = Some(c.age_ewma.map_or(a, |x| x + EWMA * (a - x))); }
                            // far behind: replace the connection instead of draining its backlog
                            if a > SLOW_MS && other_fresh && has_books && c.recycled_at.is_none_or(|t| e.recv.mono_ns - t >= RECYCLE_COOLDOWN_NS) {
                                c.recycled_at = Some(e.recv.mono_ns); c.recycles += 1; c.valid = false; c.age_ewma = None;
                                let _ = c.cmd.send(Cmd::Resync);
                                fact(Fact::ConnectionRecycled { connection: c.label.clone(), age_ms: a });
                            }
                            if used { used_ages.entry(k).or_default().push(a); }
                        }
                        let now_ns = e.recv.mono_ns;
                        // paper: orders due are matched on the books as they were just before this event
                        let paper_ev = paper.as_mut().map(|p| { p.due(now_ns, selected.and_then(|s| eng.books[s].as_ref()), e.recv.wall_ns / 1_000_000); p.observe(&e, used && has_books); e.clone() });
                        let (recv, event_ms) = (e.recv, e.event_ms);
                        // the engine: books of this connection, then the strategies (on the books in use)
                        eng.on(EngineInput::Feed(e), recv, &mut cmds);
                        let us = (clock.now().mono_ns - recv.mono_ns) as f64 / 1000.0;
                        let c = &mut conns[ci];
                        for (asset, r) in std::mem::take(&mut eng.book_changes) {
                            match r {
                                Err(why) => c.broke(asset, why, now_ns, &fact),
                                Ok(()) => {
                                    if let Some(b) = c.broken.remove(&asset) {
                                        let ms = (clock.now().mono_ns - b.since) as f64 / 1e6;
                                        recovered.push(ms);
                                        fact(Fact::BookRecovered { connection: c.label.clone(), asset: asset.clone(), ms_unusable: ms });
                                    }
                                    if let Some(start) = opens_while_running.remove(&asset) {
                                        let ms = (recv.wall_ns / 1_000_000) as i64 - (start as i64) * 1000;
                                        first_usable_ms.push(ms as f64);
                                        fact(Fact::BookUsable { asset, ms_after_open: ms });
                                    }
                                }
                            }
                        }
                        if let (Some(p), Some(pe)) = (paper.as_mut(), paper_ev) {
                            p.after(&cmds, now_ns, us);
                            p.revalue(&pe, eng.books[ci].as_ref());
                        }
                        // live value of held positions on each change of their book, and the stop loss on it (02-conception.md §6)
                        let now_ms = recv.wall_ns / 1_000_000;
                        let age = event_ms.map_or(0, |ms| now_ms as i64 - ms as i64);
                        for token in &touched {
                            let Some(p) = acct.open.get(token) else { continue };
                            let t0 = clock.now().mono_ns;
                            let Some(v) = value(p, eng.books[ci].as_ref().and_then(|b| b.get(token)), age) else { continue };
                            let trig = !fired.contains(token) && stop_loss.triggers(&v, now_ms, p.end_ms);
                            value_us.push((clock.now().mono_ns - t0) as f64 / 1000.0);
                            if trig {
                                fired.insert(token.clone());
                                fact(Fact::Defense { trigger: Trigger { rule: stop_loss, at_ms: now_ms, end_ms: p.end_ms, valuation: v.clone() }, exit_sent: false });
                            }
                            if last_value.insert(token.clone(), v.value) != Some(v.value) { fact(Fact::Valuation { valuation: v }); }
                        }
                        // engine latency: from the frame's receive stamp to the state (live values included) being up to date
                        engine_us.push((clock.now().mono_ns - recv.mono_ns) as f64 / 1000.0);
                        // bench: the same valuation on a 10-share position of the bench's token, measured under the real load
                        if let (Some(b), Some(w)) = (bench.as_mut(), live.iter().find(|w| w.end * 1000 > now_ms)) {
                            if touched.contains(w.up()) {
                                let p = account::Position { market: w.market.clone(), token: w.up().clone(), shares: 10_000_000, paid: 5_000_000, fees: 0, fees_measured: false, sold: 0, confirmed: false,
                                    won: None, fee_rate: w.fee.rate, fee_exponent: w.fee.exponent, end_ms: w.end * 1000, seqs: vec![] };
                                let t0 = clock.now().mono_ns;
                                if let Some(v) = value(&p, eng.books[ci].as_ref().and_then(|x| x.get(w.up())), age) { let _ = stop_loss.triggers(&v, now_ms, p.end_ms); b.value_us.push((clock.now().mono_ns - t0) as f64 / 1000.0); }
                            }
                        }
                        if let (Some(b), Some(w)) = (bench.as_mut(), live.iter().find(|w| w.end > clock.now().wall_ns / 1_000_000_000)) {
                            if is_quote && recv.mono_ns >= b.next_ns { b.order(&w.up().clone(), w.tick, &clock).await; }
                        }
                    }
                },
            }
            // live: each engine command goes to the real venue, which refuses all but FAK buys past the guards
            if let Some(l) = real.as_mut() {
                l.sources_valid = selected.is_some() && conns.iter().filter(|c| c.source < cfg.sources.len() && Some(&cfg.sources[c.source].name) == strategy_source.as_ref()).all(|c| c.valid);
                l.reconciled = verdict.as_ref().is_some_and(|v| v.agrees());
                l.session_pnl = acct.realized_total();
                l.balance = acct.balance.map(|b| b.0);
                for (_, d, why) in eng.refused.drain(..) { if let Decision::Place(i) = d { fact(Fact::LiveRefused { slug: String::new(), why: format!("{why} (token {})", i.token) }); } }
                for c in cmds.drain(..) { l.submit(c, clock.now()); for f in l.facts.drain(..) { fact(f); } }
            }
            cmds.clear();
            eng.refused.clear();
        }
        let per_conn: serde_json::Map<String, serde_json::Value> = conns.iter_mut().map(|c| (c.label.clone(), serde_json::json!({
            "msgs": c.msgs, "interval_ms": c.gaps_ms.dist(), "invalid_transitions": c.invalid, "recycles": c.recycles, "invalid_ms": dist(&mut c.invalid_ms), "integrity_failures": c.failures,
            "age_ms": c.ages_ms.iter().map(|(k, v)| (k.to_string(), v.dist())).collect::<serde_json::Map<_, _>>(),
        }))).collect();
        serde_json::json!({ "seconds": cfg.seconds, "connections": per_conn,
            "used_age_ms": used_ages.iter().map(|(k, v)| (k.to_string(), v.dist())).collect::<serde_json::Map<_, _>>(),
            "connection_switches": switches, "ms_without_a_ready_book_connection": used_ms_without_ready,
            "engine_us": engine_us.dist(),
            "order_bench_us": bench.as_mut().map(|b| serde_json::json!({ "decision_to_written": b.total_us.dist(), "build_and_sign": b.sign_us.dist(), "first_of_window": dist(&mut b.first_us), "valuation": b.value_us.dist() })),
            "book_recovery_ms": dist(&mut recovered),
            "first_usable_book_ms_after_open": dist(&mut first_usable_ms),
            "account": { "open": acct.open.len(), "realized": acct.realized_total(), "valuation_us": value_us.dist(), "stop_losses": fired.len(), "reconciliations": verdicts, "last_verdict": verdict },
            "paper": paper.as_mut().map(|p| p.report()),
            "live": real.as_ref().map(|l| serde_json::json!({ "orders": l.orders.iter().map(|(_, o)| serde_json::json!({ "phase": o.phase, "id": o.id, "matched": o.matched_size() })).collect::<Vec<_>>() })) })
    });
    if let Some(p) = paper { p.close(); }
    rec.send(Line::Engine(Fact::Report { report: report.clone() }));
    drop(recorder);
    drop(facts);
    writer.join().unwrap().expect("ledger writer");
    // one line, after the capture's last one: the server reads stdout line by line
    println!("{}", serde_json::json!({ "k": "report", "report": report }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The histogram's quantiles stay within 0.5 % (plus rounding) of the exact ones, negatives and zero included.
    #[test]
    fn hist_matches_exact_quantiles() {
        let mut v: Vec<f64> = (0..20_000).map(|i| ((i * 7919) % 20_000) as f64 * 0.37 - 300.0).collect();
        let mut h = Hist::default();
        for x in &v { h.push(*x); }
        for p in [0.0, 0.5, 0.99, 0.999] {
            let exact = q(&mut v, p).unwrap();
            let got = h.q(p).unwrap();
            assert!((got - exact).abs() <= exact.abs() * 0.006 + 0.11, "p{p}: {got} vs {exact}");
        }
        assert_eq!(h.dist()["max"], serde_json::json!(q(&mut v, 1.0)));
        assert_eq!(Hist::default().dist()["p50"], serde_json::Value::Null);
    }
}
