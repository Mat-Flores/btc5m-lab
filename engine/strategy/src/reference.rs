//! Reference strategy: the old engine (`src/core/engine.js`) for one source and one account, ported without changing
//! its logic, so that its decisions (side, limit, amount, instant) match the old realistic account on recorded sessions.
//!
//! `Reference::process` is a deterministic function of (state, event): no clock, no I/O, no randomness. The decision
//! itself is `evaluate` (fair value, edge, `minFair`, `jump`, cooldown) followed by `enter` (risk, size, limit), and
//! `check_exits` for the `converge` exit. The fills come from the old engine's paper model (`fill_buy`, `run_sell`):
//! a FAK matched against the recorded book after `signMs + RTT p50 + matchMs (+ extra)`, which is what its realistic
//! account assumes. Every float operation keeps the JavaScript order of evaluation.
use crate::model::{fair_up, jmax, jmin, jround, Vol};
use std::collections::{BTreeMap, HashMap, VecDeque};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side { Up, Down }
impl Side {
    pub fn name(self) -> &'static str { if self == Side::Up { "Up" } else { "Down" } }
}
const SIDES: [Side; 2] = [Side::Up, Side::Down];

#[derive(Clone, Copy, Debug)]
pub struct Fee { pub rate: f64, pub exponent: f64 }

/// Window metadata as recorded (`meta.w`).
#[derive(Clone, Debug)]
pub struct Meta {
    pub slug: String,
    pub start: f64,
    pub len: f64,
    pub tokens: [String; 2],
    pub tick: f64,
    pub min_size: f64,
    pub fee: Option<Fee>,
    pub twap: f64,
    pub price_to_beat: f64, // NaN when absent
}

/// The recorded events the old engine reads; anything else only moves the clock (`Other`).
#[derive(Clone, Debug)]
pub enum Event {
    Px { t: f64, src: String, p: f64 },
    Cl { t: f64, p: f64, ts: f64 },
    Book { t: f64, a: String, bids: Vec<(f64, f64)>, asks: Vec<(f64, f64)> },
    /// (asset, is ask, price, size)
    Delta { t: f64, ch: Vec<(String, bool, f64, f64)> },
    Meta { t: f64, w: Meta },
    Res { t: f64, slug: String, winner: String },
    Ptb { t: f64, slug: String, price_to_beat: f64 },
    Rtt { t: f64, ms: f64 },
    Clock { t: f64, offset: f64 },
    Other { t: f64 },
}
impl Event {
    pub fn t(&self) -> f64 {
        match self {
            Event::Px { t, .. } | Event::Cl { t, .. } | Event::Book { t, .. } | Event::Delta { t, .. } | Event::Meta { t, .. }
            | Event::Res { t, .. } | Event::Ptb { t, .. } | Event::Rtt { t, .. } | Event::Clock { t, .. } | Event::Other { t } => *t,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Exit { Hold, Converge }

/// The old engine's configuration keys the decision reads (`DEFAULT_CONFIG` names, in their units).
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Config {
    pub source: String,
    pub paused: bool,
    pub off: bool,
    pub min_edge: f64,
    pub min_fair: f64,
    pub jump: f64,
    pub sigma_mult: f64,
    pub lookback: f64,
    pub min_tau: f64,
    pub max_tau: f64,
    pub cooldown_ms: f64,
    pub max_entries_per_window: f64,
    pub bankroll: f64,
    pub stake_usd: f64,
    pub max_open: f64,
    pub max_open_frac: f64,
    pub daily_stop_frac: f64,
    pub exit: Exit,
    pub take_profit: f64,
    pub max_hold_sec: f64,
    pub exit_slippage: f64,
    pub sign_ms: f64,
    pub match_ms: f64,
    pub default_rtt_ms: f64,
    /// Share of the RTT median counted before the match: 1 in the old engine (a whole round trip); 0.5 for the
    /// design's model (one way, `00-recherche.md` §5.9), with the market's taker delay in `match_ms`.
    pub rtt_share: f64,
    /// `extraMs` of the simulated account's latency profile (0 for the realistic account).
    pub extra_ms: f64,
}

/// What the old engine emits for its account, in order (the fields the parity check compares).
#[derive(Clone, Debug)]
pub enum Rec {
    Order { t: f64, slug: String, side: Side, buy: bool, limit: f64, maker: u128, taker: u128, latency_ms: f64 },
    Fill { t: f64, slug: String, side: Side, buy: bool, shares: f64, usd: f64, fees: f64 },
    Miss { t: f64, slug: String, side: Side, buy: bool },
    Skip { t: f64, slug: String, side: Side, reason: &'static str },
    Close { t: f64, slug: String, side: Side, pnl: f64, how: &'static str },
}

/// `src/core/engine.js` DEFAULT_CONFIG, realistic account (`extraMs` 0), source to be named.
impl Default for Config {
    fn default() -> Self {
        Config { source: String::new(), paused: false, off: false, min_edge: 0.10, min_fair: 0.60, jump: 0.20, sigma_mult: 1.5, lookback: 5.0,
            min_tau: 5.0, max_tau: 300.0, cooldown_ms: 2000.0, max_entries_per_window: 1.0, bankroll: 1000.0, stake_usd: 100.0, max_open: 0.0,
            max_open_frac: 0.5, daily_stop_frac: 0.3, exit: Exit::Hold, take_profit: 0.02, max_hold_sec: 60.0, exit_slippage: 0.03, sign_ms: 3.0,
            match_ms: 10.0, default_rtt_ms: 150.0, rtt_share: 1.0, extra_ms: 0.0 }
    }
}

// ------------------------------------------------------------------ order amounts (src/core/orders.js)
const U: u128 = 1_000_000;

/// (price, size, amount) decimals by tick size.
fn precision(tick: f64) -> (u32, u32, u32) {
    if tick == 0.1 { (1, 2, 3) } else if tick == 0.01 { (2, 2, 4) } else if tick == 0.005 || tick == 0.001 { (3, 2, 5) }
    else if tick == 0.0025 || tick == 0.0001 { (4, 2, 6) } else { panic!("unsupported tick {tick}") }
}
// shortcut: Rust rounds exact decimal ties to even where `toFixed` rounds them up; a tie needs a dyadic value with
// exactly d+1 decimals ending in 5, which none of the callers below can produce at a digit they keep
fn to_fixed(x: f64, d: usize) -> String { format!("{x:.d$}") }
/// Decimal string -> base units, truncated after 6 decimals (`toUnits`).
fn units_of(s: &str) -> u128 {
    let (i, f) = s.split_once('.').unwrap_or((s, ""));
    let f = format!("{f:0<6}");
    i.parse::<u128>().expect("non-negative amount") * U + f[..6].parse::<u128>().unwrap()
}
fn to_units(x: f64) -> u128 { units_of(&to_fixed(x, 9)) }
fn from_units(u: u128) -> f64 { u as f64 / 1e6 }
fn floor_to(u: u128, dec: u32) -> u128 { let q = 10u128.pow(6 - dec); u / q * q }
fn price_units(price: f64, dec: u32) -> u128 { jround(price * 10f64.powi(dec as i32)) as u128 * 10u128.pow(6 - dec) }

/// (makerAmount, takerAmount, price) of a market BUY spending `amount` (decimal string) at worst `max_price`.
fn market_buy(amount: &str, max_price: f64, tick: f64) -> (u128, u128, f64) {
    let (pd, sd, ad) = precision(tick);
    let maker = floor_to(units_of(amount), sd);
    let pu = price_units(max_price, pd);
    let q = 10u128.pow(6 - ad);
    let taker = (maker * U).div_ceil(pu * q) * q;
    (maker, taker, pu as f64 / 1e6)
}
/// (makerAmount, takerAmount, price) of a market SELL of `shares` (decimal string) at worst `min_price`.
fn market_sell(shares: &str, min_price: f64, tick: f64) -> (u128, u128, f64) {
    let (pd, sd, ad) = precision(tick);
    let maker = floor_to(units_of(shares), sd);
    let pu = price_units(min_price, pd);
    let q = 10u128.pow(6 - ad);
    (maker, maker * pu / U / q * q, pu as f64 / 1e6)
}

/// FAK against levels sorted best first; returns (price, shares, usd) per level (`simulateFak`).
fn simulate_fak(buy: bool, maker: u128, limit: f64, levels: &[(f64, f64)], consumed: Option<&HashMap<u64, f64>>) -> Vec<(f64, u128, u128)> {
    let mut remaining = maker;
    let mut out = Vec::new();
    for &(price, size) in levels {
        if remaining == 0 { break; }
        if if buy { price > limit + 1e-12 } else { price < limit - 1e-12 } { break; }
        let taken = consumed.and_then(|c| c.get(&price.to_bits())).copied().unwrap_or(0.0);
        let avail = to_units(jmax(0.0, size - taken));
        if avail == 0 { continue; }
        let pu = jround(price * 1e6) as u128;
        if buy {
            let shares = (remaining * U / pu).min(avail);
            let usd = shares * pu / U;
            if shares == 0 || usd == 0 { break; }
            remaining -= usd;
            out.push((price, shares, usd));
        } else {
            let shares = remaining.min(avail);
            remaining -= shares;
            out.push((price, shares, shares * pu / U));
        }
    }
    out
}

/// Taker fee of one match, rounded to 5 decimals (`matchFee`).
fn match_fee(shares: f64, price: f64, fee: Option<Fee>) -> f64 {
    match fee {
        Some(f) if f.rate != 0.0 => {
            let raw = shares * f.rate * (price * (1.0 - price)).powf(f.exponent);
            jround(raw * 1e5 + 1e-9) / 1e5
        }
        _ => 0.0,
    }
}
pub fn fee_per_share(p: f64, fee: Option<Fee>) -> f64 {
    match fee { Some(f) if f.rate != 0.0 => f.rate * (p * (1.0 - p)).powf(f.exponent), _ => 0.0 }
}

/// Highest tick price at which buying still leaves >= `min_edge` per share after the taker fee (`limitPrice`).
pub fn limit_price(fair_side: f64, tick: f64, fee: Option<Fee>, min_edge: f64) -> f64 {
    let mut p = (fair_side / tick + 1e-9).floor() * tick;
    while p >= tick - 1e-12 {
        if fair_side - p - fee_per_share(p, fee) >= min_edge - 1e-12 { return to_fixed(p, 6).parse().unwrap(); }
        p = to_fixed(p - tick, 6).parse().unwrap();
    }
    f64::NAN
}

// ------------------------------------------------------------------ market state
#[derive(Default)]
struct Book { bids: BTreeMap<u64, f64>, asks: BTreeMap<u64, f64> }
impl Book {
    // prices are >= 0, so the order of their bits is the numeric order
    fn best_ask(&self) -> f64 { self.asks.keys().next().map_or(f64::INFINITY, |b| f64::from_bits(*b)) }
    fn best_bid(&self) -> f64 { self.bids.keys().next_back().map_or(f64::NEG_INFINITY, |b| f64::from_bits(*b)) }
    fn set(&mut self, ask: bool, price: f64, size: f64) {
        let m = if ask { &mut self.asks } else { &mut self.bids };
        if size > 0.0 { m.insert(price.to_bits(), size); } else { m.remove(&price.to_bits()); }
    }
    fn ask_levels(&self, limit: f64) -> Vec<(f64, f64)> {
        self.asks.iter().map(|(p, s)| (f64::from_bits(*p), *s)).filter(|(p, _)| *p <= limit + 1e-12).collect()
    }
    fn bid_levels(&self, limit: f64) -> Vec<(f64, f64)> {
        self.bids.iter().rev().map(|(p, s)| (f64::from_bits(*p), *s)).filter(|(p, _)| *p >= limit - 1e-12).collect()
    }
}

struct Window {
    m: Meta,
    end: f64,
    strike: f64,
    official: bool,
    provisional: Option<Side>,
    resolved: Option<String>,
}

struct Position {
    slug: String,
    start: f64,
    side: Side,
    token: String,
    shares: f64,
    shares_bought: f64,
    cost: f64,
    fees: f64,
    proceeds: f64,
    t: f64,
    fair: f64,
    exiting: bool,
}

struct Buy { slug: String, start: f64, side: Side, token: String, fair: f64, fee: Option<Fee>, maker: u128, price: f64 }
struct Sell { key: (String, Side), token: String, fee: Option<Fee>, maker: u128, price: f64 }
enum Due { Provisional(f64), Buy(Buy), Sell(Sell) }
struct Timer { at: f64, seq: u64, due: Due }

/// RTT samples (`Rolling(50)`), median as the old engine takes it.
struct Rolling { a: VecDeque<f64> }
impl Rolling {
    fn push(&mut self, x: f64) { if x.is_finite() { self.a.push_back(x); if self.a.len() > 50 { self.a.pop_front(); } } }
    fn median(&self) -> f64 {
        let mut s: Vec<f64> = self.a.iter().copied().collect();
        s.sort_by(|x, y| x.partial_cmp(y).unwrap());
        s[(s.len() - 1).min((0.5 * s.len() as f64).floor() as usize)]
    }
}

pub struct Reference {
    cfg: Config,
    t: f64,
    offset: f64,
    first_t: f64,
    next_sec: f64,
    seq: u64,
    timers: Vec<Timer>,
    // price source
    px: f64,
    px_t: f64,
    basis: f64,
    px_seen: bool,
    hist: VecDeque<(f64, f64)>,
    cl_buf: VecDeque<(f64, f64)>,
    vol: Vol,
    rtt: Rolling,
    books: HashMap<String, Book>,
    windows: Vec<Window>,
    cur: Option<f64>,
    ring: Vec<(f64, f64)>,
    ring_head: usize,
    ring_sorted: bool,
    last_signal: [f64; 2],
    fair_now: f64,
    // the simulated account
    cash: f64,
    day: Option<i64>,
    day_start: f64,
    inflight: u32,
    consumed: HashMap<String, HashMap<u64, f64>>,
    positions: Vec<Position>,
    entries: HashMap<String, f64>,
    pub records: Vec<Rec>,
}

impl Reference {
    pub fn new(cfg: Config) -> Self {
        let bankroll = cfg.bankroll;
        Reference {
            cfg, t: f64::NEG_INFINITY, offset: 0.0, first_t: f64::NAN, next_sec: f64::NAN, seq: 0, timers: Vec::new(),
            px: f64::NAN, px_t: f64::NAN, basis: f64::NAN, px_seen: false, hist: VecDeque::new(), cl_buf: VecDeque::new(),
            vol: Vol::new(), rtt: Rolling { a: VecDeque::new() }, books: HashMap::new(), windows: Vec::new(), cur: None,
            ring: Vec::new(), ring_head: 0, ring_sorted: true, last_signal: [f64::NEG_INFINITY; 2], fair_now: f64::NAN,
            cash: bankroll, day: None, day_start: bankroll, inflight: 0, consumed: HashMap::new(), positions: Vec::new(),
            entries: HashMap::new(), records: Vec::new(),
        }
    }

    // ------------------------------------------------------------------ read-only views (candidate studies)
    /// Current window: (slug, tokens, tick, seconds to its end, latest fair value of Up or NaN).
    pub fn now(&self) -> Option<(&str, &[String; 2], f64, f64, f64)> {
        let w = self.cur_window()?;
        Some((&w.m.slug, &w.m.tokens, w.m.tick, w.end - self.tc() / 1000.0, self.fair_now))
    }
    /// Best bid and best ask of a token (-inf / +inf when empty).
    pub fn best(&self, token: &str) -> (f64, f64) {
        self.books.get(token).map_or((f64::NEG_INFINITY, f64::INFINITY), |b| (b.best_bid(), b.best_ask()))
    }
    /// Recorded size at a price on one side of a token's book (0 when absent).
    pub fn size_at(&self, token: &str, ask: bool, price: f64) -> f64 {
        self.books.get(token).and_then(|b| if ask { &b.asks } else { &b.bids }.get(&price.to_bits()).copied()).unwrap_or(0.0)
    }
    pub fn time(&self) -> f64 { self.t }

    // ------------------------------------------------------------------ clock & timers
    fn tc(&self) -> f64 { self.t - self.offset }
    fn schedule(&mut self, at: f64, due: Due) {
        let seq = self.seq;
        self.seq += 1;
        let i = self.timers.partition_point(|x| x.at < at || (x.at == at && x.seq < seq));
        self.timers.insert(i, Timer { at, seq, due });
    }

    /// Runs every timer and whole-second task due at or before `t` (local time).
    pub fn advance(&mut self, t: f64) {
        loop {
            let timer_at = self.timers.first().map_or(f64::INFINITY, |x| x.at);
            let sec_at = if self.next_sec.is_finite() { self.next_sec + self.offset } else { f64::INFINITY };
            let at = jmin(timer_at, sec_at);
            if at > t { break; }
            self.t = jmax(self.t, at);
            if sec_at <= timer_at {
                let s = self.next_sec;
                self.next_sec += 1000.0;
                self.every_second(s);
            } else {
                let timer = self.timers.remove(0);
                match timer.due {
                    Due::Provisional(start) => self.provisional(start),
                    Due::Buy(b) => self.fill_buy(b),
                    Due::Sell(s) => self.run_sell(s),
                }
            }
        }
        self.t = jmax(self.t, t);
    }

    pub fn process(&mut self, ev: &Event) {
        self.advance(ev.t());
        if !self.first_t.is_finite() {
            self.first_t = ev.t();
            self.next_sec = ((ev.t() - self.offset) / 1000.0).ceil() * 1000.0;
        }
        match ev {
            &Event::Px { t, ref src, p } => {
                if *src != self.cfg.source { return; }
                self.px_seen = true;
                self.px = p;
                self.px_t = t;
                let tc = self.tc();
                self.hist.push_back((tc, p));
                while self.hist.len() > 2 && self.hist[1].0 < self.hist[self.hist.len() - 1].0 - 10_000.0 { self.hist.pop_front(); }
                self.evaluate();
            }
            &Event::Cl { t, p, ts } => self.on_cl(t, p, ts),
            Event::Book { a, bids, asks, .. } => {
                let b = self.books.entry(a.clone()).or_default();
                b.bids = bids.iter().map(|(p, s)| (p.to_bits(), *s)).collect();
                b.asks = asks.iter().map(|(p, s)| (p.to_bits(), *s)).collect();
                self.consumed.remove(a);
                self.after_book(a);
            }
            Event::Delta { ch, .. } => {
                let mut touched: Vec<&str> = Vec::new();
                for (a, ask, price, size) in ch {
                    self.books.entry(a.clone()).or_default().set(*ask, *price, *size);
                    if let Some(c) = self.consumed.get_mut(a) { c.remove(&price.to_bits()); }
                    if !touched.contains(&a.as_str()) { touched.push(a); }
                }
                for a in touched { self.after_book(a); }
            }
            Event::Meta { w, .. } => self.on_meta(w.clone()),
            Event::Res { slug, winner, .. } => self.on_resolution(slug, winner),
            Event::Ptb { slug, price_to_beat, .. } => {
                if let Some(w) = self.windows.iter_mut().find(|w| w.m.slug == *slug) {
                    if price_to_beat.is_finite() && !w.official { w.strike = *price_to_beat; w.official = true; }
                }
            }
            Event::Rtt { ms, .. } => self.rtt.push(*ms),
            Event::Clock { offset, .. } => self.offset = *offset,
            Event::Other { .. } => {}
        }
    }

    // ------------------------------------------------------------------ market data
    fn px_at(&self, ts: f64) -> f64 { self.hist.iter().rev().find(|h| h.0 <= ts).map_or(f64::NAN, |h| h.1) }

    fn on_cl(&mut self, t: f64, p: f64, ts: f64) {
        self.cl_buf.push_back((ts, p));
        if self.cl_buf.len() > 120 { self.cl_buf.pop_front(); }
        // Chainlink lags its observation time: compare it with the exchange price at that time
        if self.px_seen {
            let then = self.px_at(ts);
            if then.is_finite() && !(t - self.px_t > 5000.0) {
                let b = then - p;
                self.basis = if self.basis.is_finite() { self.basis + (b - self.basis) / 30.0 } else { b };
            }
        }
        for i in 0..self.windows.len() { self.capture_strike(i); }
    }

    fn after_book(&mut self, a: &str) {
        let Some(w) = self.cur_window() else { return };
        if a == w.m.tokens[0] || a == w.m.tokens[1] { self.evaluate(); self.check_exits(); }
    }

    // ------------------------------------------------------------------ windows
    fn window(&self, start: f64) -> Option<&Window> { self.windows.iter().find(|w| w.m.start == start) }
    fn cur_window(&self) -> Option<&Window> { self.cur.and_then(|s| self.window(s)) }

    fn on_meta(&mut self, m: Meta) {
        if self.window(m.start).is_some() { return; }
        let (strike, official) = if m.price_to_beat.is_finite() { (m.price_to_beat, true) } else { (f64::NAN, false) };
        let end = m.start + m.len;
        self.windows.push(Window { m, end, strike, official, provisional: None, resolved: None });
        self.capture_strike(self.windows.len() - 1);
        self.update_current();
    }

    /// Strike = the 60 s Chainlink TWAP ending at the open, once the path up to the open is complete.
    fn capture_strike(&mut self, i: usize) {
        let w = &self.windows[i];
        if w.strike.is_finite() { return; }
        let open = w.m.start * 1000.0;
        let len = w.m.twap * 1000.0;
        if !self.cl_buf.iter().any(|x| x.0 >= open) || !self.cl_buf.iter().any(|x| x.0 <= open - len) { return; }
        let v = self.twap_full(open - len, open);
        if v.is_finite() { self.windows[i].strike = v; }
    }

    fn twap_full(&self, from: f64, to: f64) -> f64 {
        let (sum, _, last_ts, v) = self.twap_known(from, to);
        if v.is_finite() { (sum + v * (to - last_ts) / 1000.0) / ((to - from) / 1000.0) } else { f64::NAN }
    }

    /// (integral $.s, seconds covered, last print time, last value) of the Chainlink path over [from, last print <= to].
    fn twap_known(&self, mut from: f64, to: f64) -> (f64, f64, f64, f64) {
        let (mut sum, mut last_ts, mut v) = (0.0, from, f64::NAN);
        for &(ts, p) in &self.cl_buf {
            if ts <= from { v = p; continue; }
            if ts > to { break; }
            if v.is_finite() { sum += v * (ts - last_ts) / 1000.0; } else { from = ts; }
            last_ts = ts;
            v = p;
        }
        (sum, (last_ts - from) / 1000.0, last_ts, v)
    }

    fn update_current(&mut self) {
        let s = (self.tc() / 1000.0 / 300.0).floor() * 300.0;
        let next = self.window(s).map(|w| w.m.start);
        if next != self.cur {
            if let Some(c) = self.cur { self.close_window(c); }
            self.cur = next;
            self.ring.clear();
            self.ring_head = 0;
            self.ring_sorted = true;
            self.last_signal = [f64::NEG_INFINITY; 2];
            self.fair_now = f64::NAN;
        }
    }

    fn close_window(&mut self, start: f64) {
        let tokens = self.window(start).unwrap().m.tokens.clone();
        for t in &tokens { self.consumed.remove(t); }
        self.schedule(self.t + 5000.0, Due::Provisional(start));
        self.windows.retain(|w| !(w.m.start < start - 3600.0));
    }

    fn provisional(&mut self, start: f64) {
        let Some(w) = self.window(start) else { return };
        if w.strike.is_finite() {
            let twap = self.twap_full((w.end - w.m.twap) * 1000.0, w.end * 1000.0);
            if twap.is_finite() {
                let side = if twap >= w.strike { Side::Up } else { Side::Down };
                self.windows.iter_mut().find(|w| w.m.start == start).unwrap().provisional = Some(side);
            }
        }
    }

    fn on_resolution(&mut self, slug: &str, winner: &str) {
        if let Some(w) = self.windows.iter_mut().find(|w| w.m.slug == slug) { w.resolved = Some(winner.to_string()); }
        let mut i = 0;
        while i < self.positions.len() {
            if self.positions[i].slug != slug { i += 1; continue; }
            let pos = self.positions.remove(i);
            let payout = if pos.side.name() == winner { pos.shares } else { 0.0 };
            self.cash += payout;
            self.close(pos, payout, "resolution");
        }
    }

    fn close(&mut self, pos: Position, payout: f64, how: &'static str) {
        let pnl = pos.proceeds + payout - pos.cost - pos.fees;
        self.records.push(Rec::Close { t: self.t, slug: pos.slug, side: pos.side, pnl, how });
    }

    // ------------------------------------------------------------------ per second
    fn every_second(&mut self, sec_ms: f64) {
        self.update_current();
        let spot = self.spot();
        if spot.is_finite() { self.vol.push(spot); }
        if self.cur.is_some() { self.check_exits(); }
        let day = (sec_ms / 86_400_000.0).floor() as i64;
        if self.day != Some(day) { self.day = Some(day); self.day_start = self.equity(); }
    }

    // ------------------------------------------------------------------ pricing
    fn spot(&self) -> f64 { if self.px.is_finite() && self.basis.is_finite() { self.px - self.basis } else { f64::NAN } }

    fn fair(&self, w: &Window) -> f64 {
        let spot = self.spot();
        if !spot.is_finite() || !w.strike.is_finite() { return f64::NAN; }
        let tc = self.tc();
        let sigma = self.vol.sigma() * self.cfg.sigma_mult;
        let tw_start = (w.end - w.m.twap) * 1000.0;
        if tc < tw_start { return fair_up(spot, w.strike, w.end - tc / 1000.0, sigma, w.m.twap, 0.0, 0.0); }
        // inside the averaging window: the part Chainlink already printed is known exactly
        let (sum, secs, last_ts, _) = self.twap_known(tw_start, jmin(tc, w.end * 1000.0));
        fair_up(spot, w.strike, jmax(0.0, w.end - last_ts / 1000.0), sigma, w.m.twap, sum, secs)
    }

    fn mark(&self, pos: &Position) -> f64 {
        let w = self.window(pos.start);
        let side = |f: f64| if pos.side == Side::Up { f } else { 1.0 - f };
        if let Some(w) = w {
            if let Some(r) = &w.resolved { return if pos.side.name() == r { 1.0 } else { 0.0 }; }
            if let Some(p) = w.provisional { return if pos.side == p { 1.0 } else { 0.0 }; }
            if self.cur == Some(w.m.start) && self.fair_now.is_finite() { return side(self.fair_now); }
            let f = if self.tc() >= w.end * 1000.0 { self.fair(w) } else { f64::NAN };
            if f.is_finite() { return side(f); }
        }
        pos.fair
    }
    fn equity(&self) -> f64 {
        let mut e = self.cash;
        for p in &self.positions { e += p.shares * self.mark(p); }
        e
    }
    fn open_cost(&self) -> f64 {
        let mut s = 0.0;
        for p in &self.positions { s += p.cost + p.fees - p.proceeds; }
        jmax(0.0, s)
    }

    // ------------------------------------------------------------------ fair-value history
    fn ring_before(&self, target: f64) -> f64 {
        let r = &self.ring;
        if !self.ring_sorted {
            let mut past = f64::NAN;
            for x in &r[self.ring_head..] { if x.0 > target { break; } past = x.1; }
            return past;
        }
        let (mut lo, mut hi) = (self.ring_head, r.len() - 1);
        while lo < hi { let mid = (lo + hi + 1) >> 1; if r[mid].0 <= target { lo = mid; } else { hi = mid - 1; } }
        r[lo].1
    }

    // ------------------------------------------------------------------ signal
    fn evaluate(&mut self) {
        let Some(w) = self.cur_window() else { return };
        if !w.strike.is_finite() { return; }
        let f = self.fair(w);
        if !f.is_finite() { return; }
        let (slug, start, end, tokens, fee) = (w.m.slug.clone(), w.m.start, w.end, w.m.tokens.clone(), w.m.fee);
        self.fair_now = f;
        let tc = self.tc();
        let cutoff = tc - (self.cfg.lookback + 2.0) * 1000.0;
        if self.ring.len() > self.ring_head && self.ring[self.ring.len() - 1].0 > tc { self.ring_sorted = false; }
        self.ring.push((tc, f));
        while self.ring_head < self.ring.len() && self.ring[self.ring_head].0 < cutoff { self.ring_head += 1; }
        if self.ring_head > 4096 && self.ring_head * 2 > self.ring.len() { self.ring.drain(..self.ring_head); self.ring_head = 0; }
        let tau = end - tc / 1000.0;
        let c = &self.cfg;
        if c.paused || c.off || tau < c.min_tau || tau > c.max_tau { return; }
        let mut past = f64::NAN;
        if c.jump > 0.0 {
            let target = tc - c.lookback * 1000.0;
            if self.ring[self.ring_head].0 > target { return; }
            past = self.ring_before(target);
        }
        for i in 0..2 {
            let side = SIDES[i];
            let Some(book) = self.books.get(&tokens[i]) else { continue };
            let ask = book.best_ask();
            if !ask.is_finite() { continue; }
            let fs = if side == Side::Up { f } else { 1.0 - f };
            let c = &self.cfg;
            if fs - ask - fee_per_share(ask, fee) < c.min_edge { continue; }
            if c.min_fair > 0.0 && fs < c.min_fair { continue; }
            if c.jump > 0.0 && !(fs - (if side == Side::Up { past } else { 1.0 - past }) >= c.jump) { continue; }
            if self.t - self.last_signal[i] < c.cooldown_ms { continue; }
            self.last_signal[i] = self.t;
            self.enter(&slug, start, side, tokens[i].clone(), fs);
        }
    }

    fn latency(&self) -> f64 {
        let rtt = if self.rtt.a.is_empty() { self.cfg.default_rtt_ms } else { self.rtt.median() };
        self.cfg.sign_ms + rtt * self.cfg.rtt_share + self.cfg.match_ms + self.cfg.extra_ms
    }

    fn enter(&mut self, slug: &str, start: f64, side: Side, token: String, fair: f64) {
        let used = self.entries.get(slug).copied().unwrap_or(0.0);
        if used >= self.cfg.max_entries_per_window { return; }
        let skip = |me: &mut Self, reason| me.records.push(Rec::Skip { t: me.t, slug: slug.to_string(), side, reason });
        if self.cfg.max_open > 0.0 && (self.inflight as usize + self.positions.len()) as f64 >= self.cfg.max_open {
            return skip(self, "places");
        }
        let w = self.window(start).unwrap();
        let (tick, fee, min_size) = (w.m.tick, w.m.fee, w.m.min_size);
        let eq = self.equity();
        let c = &self.cfg;
        let blocked = eq < self.day_start * (1.0 - c.daily_stop_frac) || self.open_cost() + c.stake_usd > c.max_open_frac * eq;
        let rate = fee.map_or(0.0, |f| f.rate);
        let amount = jmin(c.stake_usd, self.cash / (1.0 + rate));
        if blocked || amount < 1.0 { return skip(self, "risque"); }
        let limit = limit_price(fair, tick, fee, c.min_edge);
        if !limit.is_finite() { return; }
        let (maker, taker, price) = market_buy(&to_fixed(amount, 6), limit, tick);
        if from_units(taker) < min_size { return skip(self, "taille < minimum"); }
        self.entries.insert(slug.to_string(), used + 1.0);
        let lat = self.latency();
        self.seq += 1; // the old engine numbers its orders on the timers' counter
        self.records.push(Rec::Order { t: self.t, slug: slug.to_string(), side, buy: true, limit, maker, taker, latency_ms: lat });
        self.inflight += 1;
        self.schedule(self.t + lat, Due::Buy(Buy { slug: slug.to_string(), start, side, token, fair, fee, maker, price }));
    }

    fn fill_buy(&mut self, b: Buy) {
        self.inflight = self.inflight.saturating_sub(1);
        let levels = self.books.entry(b.token.clone()).or_default().ask_levels(b.price);
        let matches = simulate_fak(true, b.maker, b.price, &levels, self.consumed.get(&b.token));
        if matches.is_empty() {
            let e = self.entries.get(&b.slug).copied().unwrap_or(0.0);
            self.entries.insert(b.slug.clone(), (if e == 0.0 { 1.0 } else { e }) - 1.0);
            self.records.push(Rec::Miss { t: self.t, slug: b.slug, side: b.side, buy: true });
            return;
        }
        let (mut shares, mut usd, mut fees) = (0.0, 0.0, 0.0);
        let ov = self.consumed.entry(b.token.clone()).or_default();
        for (price, sh, u) in matches {
            let (sh, u) = (from_units(sh), from_units(u));
            shares += sh;
            usd += u;
            fees += match_fee(sh, price, b.fee);
            *ov.entry(price.to_bits()).or_insert(0.0) += sh;
        }
        self.cash -= usd + fees;
        self.records.push(Rec::Fill { t: self.t, slug: b.slug.clone(), side: b.side, buy: true, shares, usd, fees });
        let t = self.t;
        let i = match self.positions.iter().position(|p| p.slug == b.slug && p.side == b.side) {
            Some(i) => i,
            None => {
                self.positions.push(Position { slug: b.slug, start: b.start, side: b.side, token: b.token, shares: 0.0, shares_bought: 0.0,
                    cost: 0.0, fees: 0.0, proceeds: 0.0, t, fair: b.fair, exiting: false });
                self.positions.len() - 1
            }
        };
        let p = &mut self.positions[i];
        p.shares += shares;
        p.shares_bought += shares;
        p.cost += usd;
        p.fees += fees;
    }

    // ------------------------------------------------------------------ convergence exits
    fn check_exits(&mut self) {
        if self.cfg.exit != Exit::Converge { return; }
        let Some(w) = self.cur_window() else { return };
        let (slug, tick, fee) = (w.m.slug.clone(), w.m.tick, w.m.fee);
        for i in 0..self.positions.len() {
            let pos = &self.positions[i];
            if pos.exiting || pos.shares <= 0.0 || pos.slug != slug { continue; }
            let bid = self.books.get(&pos.token).map_or(f64::NEG_INFINITY, |b| b.best_bid());
            if !bid.is_finite() { continue; }
            let avg = pos.cost / pos.shares_bought;
            let entry_fee = pos.fees / pos.shares_bought;
            let net = bid - avg - entry_fee - fee_per_share(bid, fee);
            let held = (self.t - pos.t) / 1000.0;
            if net >= self.cfg.take_profit || held >= self.cfg.max_hold_sec { self.exit(i, tick, fee, bid); }
        }
    }

    fn exit(&mut self, i: usize, tick: f64, fee: Option<Fee>, bid: f64) {
        let min_price = jmax(tick, jround((bid - self.cfg.exit_slippage) / tick) * tick);
        let (maker, taker, price) = market_sell(&to_fixed(self.positions[i].shares, 6), min_price, tick);
        if maker == 0 { return; }
        let lat = self.latency();
        let pos = &mut self.positions[i];
        pos.exiting = true;
        let (key, token) = ((pos.slug.clone(), pos.side), pos.token.clone());
        self.seq += 1;
        self.records.push(Rec::Order { t: self.t, slug: key.0.clone(), side: key.1, buy: false, limit: min_price, maker, taker, latency_ms: lat });
        self.schedule(self.t + lat, Due::Sell(Sell { key, token, fee, maker, price }));
    }

    fn run_sell(&mut self, s: Sell) {
        let levels = self.books.entry(s.token.clone()).or_default().bid_levels(s.price);
        let matches = simulate_fak(false, s.maker, s.price, &levels, None);
        let (mut sold, mut usd, mut fees) = (0.0, 0.0, 0.0);
        for (price, sh, u) in matches {
            let (sh, u) = (from_units(sh), from_units(u));
            sold += sh;
            usd += u;
            fees += match_fee(sh, price, s.fee);
        }
        // the old engine sells only within the current window, minutes before any resolution can close the position
        let i = self.positions.iter().position(|p| (p.slug.as_str(), p.side) == (s.key.0.as_str(), s.key.1))
            .expect("sell of a position already closed");
        self.positions[i].exiting = false;
        if sold == 0.0 {
            self.records.push(Rec::Miss { t: self.t, slug: s.key.0, side: s.key.1, buy: false });
            return;
        }
        let p = &mut self.positions[i];
        p.shares = jmax(0.0, p.shares - sold);
        p.proceeds += usd;
        p.fees += fees;
        self.cash += usd - fees;
        self.records.push(Rec::Fill { t: self.t, slug: s.key.0.clone(), side: s.key.1, buy: false, shares: sold, usd, fees });
        if self.positions[i].shares < 1e-6 {
            let pos = self.positions.remove(i);
            self.close(pos, 0.0, "vente");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // expected values from the old engine (node: src/core/orders.js and engine.js limitPrice)
    #[test]
    fn amounts_match_the_old_engine() {
        assert_eq!(market_buy("93.457943", 0.55, 0.01), (93_450_000, 169_909_100, 0.55));
        assert_eq!(market_sell("12.345678", 0.47, 0.01), (12_340_000, 5_799_800, 0.47));
        let fee = Some(Fee { rate: 0.07, exponent: 1.0 });
        assert_eq!(limit_price(0.83, 0.01, fee, 0.1), 0.71);
        assert_eq!(match_fee(10.0, 0.5, fee), 0.175);
    }

    #[test]
    fn fak_takes_levels_in_order_and_skips_what_was_consumed() {
        let levels = [(0.5, 10.0), (0.51, 100.0)];
        let mut c = HashMap::new();
        c.insert(0.5f64.to_bits(), 4.0);
        let m = simulate_fak(true, 10_000_000, 0.51, &levels, Some(&c));
        assert_eq!(m, vec![(0.5, 6_000_000, 3_000_000), (0.51, 13_725_490, 6_999_999)]);
    }
}
