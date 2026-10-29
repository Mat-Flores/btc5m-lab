//! `input: "trace"`: the engine itself (`engine_app::engine::Engine`, the registry's strategies, its books) on the
//! traces of source-tracing, merged by receive time (ties: order of the sources in the configuration), with the
//! simulated venue below. Times are `t_recv` + the source's `recv_lag_ms`.
//! - `.old` traces (converted sessions): mapped from their `legacy` line (the old session format); a window's market
//!   is its slug (the old recordings have no condition id); its rebate rate 0.2 (WALLETS.md:42, not recorded then).
//! - Current traces: each frame redecoded by the source's own adapter (`raw`); gamma's answers by `windows::parse`.
//! - `signal`: a source of its own, the mean of the last values of `signal.sources` (their mid, or their trade price
//!   with `field: "trade"`), emitted when one of them changes and all are known. A `.old` trace has one price per
//!   source (`px`), used for both fields.
//! - Gamma is polled: a window is given to the engine once (a window announced before the period, at its start),
//!   its result once.
//! - Venue (`TraceVenue`): a FAK order meets the book `one_way_ms` + a hold drawn from `taker_hold` after its decision;
//!   a post-only bid is refused if it would cross, else joins its level `one_way_ms` after the decision with the
//!   level's size at the decision ahead of it (sim.mjs:396), and fills by `fill_bound` on the changes of its level
//!   (fills before the strategy hears of the change, sim.mjs:446-448). `exact` reads the venue's trades of the traces:
//!   a change carries the trade volume seen at its price since the level's previous change [à vérifier : appariement
//!   des trades `last_trade_price` aux deltas] ; the old Node recordings have no trade, so no `exact` fill there.
//!   A cancel takes effect `one_way_ms` after the decision. No fill on a snapshot, on a level silent longer than
//!   `silence_ms`, or past the window's end + `trading_cutoff_ms_after_end`.
use crate::config::Config;
use crate::packs::parse_utc;
use crate::venue::{exact_fill, hold_quantile, Bound, Order, Rng, VenueCfg};
use engine_app::engine::{Amount, Command, Engine, EngineInput, Tif, Venue, VenueEvent};
use engine_core::account::Dir;
use engine_core::book::{Book, Books};
use engine_core::clock::Stamp;
use engine_core::event::{Change, Event, Level, Payload, Side};
use engine_core::market::{FeeSchedule, MarketSpec, Window};
use engine_core::money::Micro;
use engine_core::order::{Input, TradeStatus};
use serde_json::Value;
use std::collections::{BinaryHeap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command as Proc, Stdio};

const MICRO: f64 = 1e6;
fn m(x: f64) -> Micro { (x * MICRO).round() as Micro }

/// What one trace line becomes.
#[derive(Debug)]
pub enum Item { Feed(Payload, Option<u64>, bool), Window(Window), Resolved { slug: String, up_won: bool }, PriceToBeat { slug: String, value: Micro } }

fn levels(v: &Value) -> Vec<Level> { v.as_array().map_or(vec![], |a| a.iter().filter_map(|l| Some(Level { price: m(l[0].as_f64()?), size: m(l[1].as_f64()?) })).collect()) }
fn ms(v: &Value) -> Option<u64> { v.as_f64().filter(|x| *x > 0.0).map(|x| x as u64) }

/// A `.old` line (`kind`, `legacy` = the old session line) as engine input. None: not market data (clock, gap…).
pub fn map_old(kind: &str, l: &Value) -> Option<Item> {
    Some(match kind {
        "mid" => { let p = m(l["p"].as_f64()?); Item::Feed(Payload::Quote { bid: p, ask: p }, ms(&l["et"]), true) }
        "oracle" => Item::Feed(Payload::Oracle { value: m(l["p"].as_f64()?) }, ms(&l["ts"]), true),
        "snapshot" => Item::Feed(Payload::BookSnapshot { asset: l["a"].as_str()?.into(), bids: levels(&l["bids"]), asks: levels(&l["asks"]), tick: None }, ms(&l["ts"]), true),
        "delta" => {
            let changes = l["ch"].as_array()?.iter().filter_map(|c| Some(Change { asset: c[0].as_str()?.into(), side: if c[1] == "A" { Side::Ask } else { Side::Bid },
                price: m(c[2].as_f64()?), size: m(c[3].as_f64()?), best_bid: None, best_ask: None })).collect();
            Item::Feed(Payload::BookDelta { changes }, ms(&l["ts"]), true)
        }
        "trade" => Item::Feed(Payload::Trade { asset: l["a"].as_str()?.into(), price: m(l["p"].as_f64()?), size: m(l["s"].as_f64()?),
            side: if l["side"] == "BUY" { Side::Bid } else { Side::Ask }, id: l["id"].as_str().unwrap_or("").into() }, ms(&l["ts"]), true),
        "window" => {
            let w = &l["w"];
            let start = w["start"].as_u64()?;
            let tokens = w["tokens"].as_array()?;
            let fee = &w["fee"];
            Item::Window(Window { start, end: start + w["len"].as_u64()?, slug: w["slug"].as_str()?.into(), market: w["slug"].as_str()?.into(),
                tokens: [tokens.first()?.as_str()?.into(), tokens.get(1)?.as_str()?.into()], tick: m(w["tick"].as_f64()?), min_size: m(w["minSize"].as_f64().unwrap_or(5.0)),
                fee: FeeSchedule { rate: m(fee["rate"].as_f64().unwrap_or(0.0)), exponent: fee["exponent"].as_u64().unwrap_or(1) as u32, rebate_rate: 200_000 },
                taker_delay: true, twap_s: w["twap"].as_u64(), price_to_beat: w["priceToBeat"].as_f64().map(m), winner: None })
        }
        "resolution" => Item::Resolved { slug: l["slug"].as_str()?.into(), up_won: l["winner"] == "Up" },
        "price_to_beat" => Item::PriceToBeat { slug: l["slug"].as_str()?.into(), value: m(l["priceToBeat"].as_f64()?) },
        _ => return None,
    })
}

/// Kinds of a current trace that are not a received frame of the source.
const NOT_DATA: [&str; 8] = ["header", "connect", "sent", "disconnect", "end", "gap", "decode_error", "recorder_start"];

/// The files of one source in the period: canonical day files only (a secondary host's file, D5, is another
/// recording of the same day), `<day>.jsonl.gz` (current) and/or `<day>.old.jsonl.gz`, by day.
pub fn files(root: &Path, trace: &str, formats: &[String], from_day: &str, to_day: &str) -> Vec<PathBuf> {
    let mut out: Vec<(String, PathBuf)> = vec![];
    let Ok(years) = std::fs::read_dir(root.join(trace)) else { return vec![] };
    for y in years.flatten() {
        for mo in std::fs::read_dir(y.path()).into_iter().flatten().flatten() {
            for f in std::fs::read_dir(mo.path()).into_iter().flatten().flatten() {
                let name = f.file_name().to_string_lossy().to_string();
                let (day, fmt) = if let Some(d) = name.strip_suffix(".old.jsonl.gz") { (d, "old") } else if let Some(d) = name.strip_suffix(".jsonl.gz") { (d, "current") } else { continue };
                if day.len() != 10 || !formats.iter().any(|x| x == fmt) || day < from_day || day > to_day { continue; }
                out.push((format!("{day}{}", if fmt == "old" { 0 } else { 1 }), f.path()));
            }
        }
    }
    out.sort();
    out.into_iter().map(|x| x.1).collect()
}

/// One source's lines, file after file (`gzip -dc`: no new dependency).
struct Reader { files: std::vec::IntoIter<PathBuf>, cur: Option<(Child, BufReader<ChildStdout>)>, buf: String }
impl Reader {
    fn next_line(&mut self) -> Result<Option<&str>, String> {
        loop {
            if self.cur.is_none() {
                let Some(f) = self.files.next() else { return Ok(None) };
                let mut c = Proc::new("gzip").arg("-dc").arg(&f).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("gzip {}: {e}", f.display()))?;
                let out = c.stdout.take().unwrap();
                self.cur = Some((c, BufReader::with_capacity(1 << 20, out)));
            }
            self.buf.clear();
            let (_, r) = self.cur.as_mut().unwrap();
            if r.read_line(&mut self.buf).map_err(|e| e.to_string())? == 0 { let (mut c, _) = self.cur.take().unwrap(); let _ = c.wait(); continue; }
            return Ok(Some(self.buf.trim_end()));
        }
    }
}

/// One source of the run: its reader, its adapter (current traces), what it is in the engine.
struct Src { id: u16, reader: Reader, adapter: Option<Box<dyn engine_core::feed::Adapter>>, gamma: bool, lag_us: i64 }

/// Next engine items of a source with their receive time (µs), from its next line(s).
fn read_next(s: &mut Src, spec: &MarketSpec, out: &mut Vec<(i64, Item)>) -> Result<bool, String> {
    loop {
        let Some(line) = s.reader.next_line()? else { return Ok(false) };
        let v: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
        let (Some(kind), Some(t)) = (v["kind"].as_str(), v["t_recv"].as_i64()) else { continue };
        let t = t + s.lag_us;
        if !v["legacy"].is_null() {
            if let Some(it) = map_old(kind, &v["legacy"]) { out.push((t, it)); return Ok(true); }
            continue;
        }
        let Some(raw) = v["raw"].as_str() else { continue };
        if NOT_DATA.contains(&kind) { continue; }
        if s.gamma {
            // gamma `events?slug=…`: the window, and its result once closed
            let Ok(Value::Array(evs)) = serde_json::from_str::<Value>(raw) else { continue };
            let Some(ev) = evs.first() else { continue };
            let Some(start) = ev["slug"].as_str().and_then(|x| x.rsplit('-').next()).and_then(|x| x.parse().ok()) else { continue };
            let Ok(w) = adapter_polymarket::windows::parse(spec, start, ev, None) else { continue };
            let (slug, winner, ptb) = (w.slug.clone(), w.winner, w.price_to_beat);
            out.push((t, Item::Window(w)));
            if let Some(p) = ptb { out.push((t, Item::PriceToBeat { slug: slug.clone(), value: p })); }
            if let Some(i) = winner { out.push((t, Item::Resolved { slug, up_won: i == 0 })); }
            return Ok(true);
        }
        let Some(a) = s.adapter.as_mut() else { continue };
        let recv = stamp(t);
        let mut evs = vec![];
        if a.decode(raw, recv, s.id, &mut evs).is_err() { continue; }
        if evs.is_empty() { continue; }
        for e in evs { out.push((t, Item::Feed(e.payload, e.event_ms, false))); }
        return Ok(true);
    }
}

fn stamp(t_us: i64) -> Stamp { let ns = (t_us.max(0) as u64) * 1000; Stamp { mono_ns: ns, wall_ns: ns } }

/// A resting bid or a FAK on its way, at the venue.
enum Live { Bid { token: String, o: Order, from_us: i64, until_us: i64, end_us: i64 }, Fak { token: String, buy: bool, amount: Micro, limit: Micro, at_us: i64 } }

/// The simulated venue of the trace runner (the `Venue` of the engine's contract).
pub struct TraceVenue {
    cfg: VenueCfg,
    pub books: Books,
    rng: Rng,
    live: HashMap<u32, Live>,
    /// token → (fee schedule, window end s).
    markets: HashMap<String, (FeeSchedule, u64)>,
    /// token → the other token of its market (the books are mirrors: a Down bid at p is an Up ask at 1 − p).
    other: HashMap<String, String>,
    /// Trade volume seen per (token, price) since its level's last change (`exact`).
    traded: HashMap<(String, Micro), Micro>,
    /// Last change of each token's book (µs), for `silence_ms`.
    last_change: HashMap<String, i64>,
    events: Vec<(i64, VenueEvent)>,
    trades: u64,
    /// Shares held per token (bid and FAK buy fills less sales): a sale beyond them is refused, as the venue does.
    held: HashMap<String, Micro>,
}

impl TraceVenue {
    pub fn new(cfg: VenueCfg) -> TraceVenue {
        let seed = cfg.taker_hold.seed;
        TraceVenue { cfg, books: engine_app::engine::new_books(u64::MAX), rng: Rng::new(seed), live: HashMap::new(), markets: HashMap::new(), other: HashMap::new(), traded: HashMap::new(),
            last_change: HashMap::new(), events: vec![], trades: 0, held: HashMap::new() }
    }

    pub fn window(&mut self, w: &Window) {
        for t in &w.tokens { self.markets.insert(t.clone(), (w.fee, w.end)); }
        self.other.insert(w.tokens[0].clone(), w.tokens[1].clone());
        self.other.insert(w.tokens[1].clone(), w.tokens[0].clone());
    }

    fn fill(&mut self, n: u32, at: i64, token: &str, price: Micro, size: Micro, maker: bool) {
        let (fee, _) = self.markets.get(token).copied().unwrap_or((FeeSchedule::default(), 0));
        let f = engine_core::value::fee(size, price, fee.rate, fee.exponent);
        let (fee_paid, rebate) = if maker { (0, self.cfg.rebates.then(|| (f as i128 * fee.rebate_rate as i128 / 1_000_000) as Micro)) } else { (f, None) };
        self.trades += 1;
        self.events.push((at, VenueEvent::Fill { n, trade: format!("sim:{}", self.trades), status: TradeStatus::Confirmed, size, price, maker, fee: fee_paid, rebate }));
    }

    /// FAK orders due by `until` (µs) meet the book as it is (just before the next input).
    fn due(&mut self, until: i64) {
        let mut due: Vec<(i64, u32)> = self.live.iter().filter_map(|(n, l)| match l { Live::Fak { at_us, .. } if *at_us <= until => Some((*at_us, *n)), _ => None }).collect();
        due.sort();
        for (at, n) in due {
            let Some(Live::Fak { token, buy, amount, limit, .. }) = self.live.remove(&n) else { continue };
            let held = self.held.get(&token).copied().unwrap_or(0);
            if !buy && amount > held + 10_000 { self.events.push((at, VenueEvent::Answer { n, input: Input::Rejected { killed: false, reason: "not enough balance".into() } })); continue; }
            let lv = match self.books.get(&token).filter(|b| b.usable()) {
                Some(b) if buy => adapter_polymarket::paper::buy(b, amount, limit),
                Some(b) => adapter_polymarket::paper::sell(b, amount, limit),
                None => vec![],
            };
            for (p, s) in lv { *self.held.entry(token.clone()).or_insert(0) += if buy { s } else { -s }; self.fill(n, at, &token, p, s, false); }
            self.events.push((at, VenueEvent::Closed { n }));
        }
        // bids whose cancel took effect
        let gone: Vec<u32> = self.live.iter().filter_map(|(n, l)| matches!(l, Live::Bid { until_us, .. } if *until_us <= until).then_some(*n)).collect();
        for n in gone { let Some(Live::Bid { until_us, .. }) = self.live.remove(&n) else { continue }; self.events.push((until_us, VenueEvent::Closed { n })); }
    }

    /// A feed event at `t` (µs): the books move, resting bids fill (before the strategy hears of it).
    pub fn feed(&mut self, t: i64, e: &Event) {
        self.due(t);
        match &e.payload {
            Payload::BookSnapshot { asset, bids, asks, tick } => { let _ = self.books.snapshot(asset, bids, asks, *tick); self.last_change.insert(asset.clone(), t); }
            // a trade empties its level and the mirror level of the other token (measured 2026-10-28: 99.5 % of
            // data-api's taker transactions are in last_trade_price, same size; per level 90 % recall, 92 % precision)
            Payload::Trade { asset, price, size, .. } => {
                *self.traded.entry((asset.clone(), *price)).or_insert(0) += size;
                if let Some(o) = self.other.get(asset) { *self.traded.entry((o.clone(), 1_000_000 - price)).or_insert(0) += size; }
            }
            Payload::BookDelta { changes } => {
                for c in changes {
                    let old = self.books.get(&c.asset).and_then(|b| level(b, c.side, c.price)).unwrap_or(0);
                    let silent = self.last_change.get(&c.asset).is_none_or(|l| t - l > (self.cfg.silence_ms * 1000.0) as i64);
                    self.last_change.insert(c.asset.clone(), t);
                    if let Some(b) = self.books.get(&c.asset) { if b.usable() { let _ = self.books.entry(&c.asset).apply(c.side, c.price, c.size); } }
                    let v = if c.size < old { self.traded.remove(&(c.asset.clone(), c.price)).map_or(0, |x| x.min(old - c.size)) } else { 0 };
                    if !silent { self.match_change(t, c, old, v); }
                }
            }
            _ => {}
        }
    }

    fn match_change(&mut self, t: i64, c: &Change, old: Micro, v: Micro) {
        let cutoff = (self.cfg.trading_cutoff_ms_after_end * 1000.0) as i64;
        let mut fills = vec![];
        for (n, l) in self.live.iter_mut() {
            let Live::Bid { token, o, from_us, until_us, end_us } = l else { continue };
            if *token != c.asset || t < *from_us || t >= *until_us || t > end_us.saturating_add(cutoff) || o.r <= 0.0 { continue; }
            let qi = (o.q * 1000.0).round() as i64;
            let ci = c.price / 1000;
            let take = match self.cfg.fill_bound {
                Bound::Exact => exact_fill(o, 0, if c.side == Side::Bid { 1 } else { 2 }, ci as f64, old as f64 / MICRO, c.size as f64 / MICRO, v as f64 / MICRO, self.cfg.cancel_ahead()),
                b => {
                    let Some(book) = self.books.get(token) else { continue };
                    let ask = book.best_ask().map_or(i64::MAX, |p| p / 1000);
                    let mut take = if ask <= qi { book.asks().next().map_or(0.0, |x| x.1 as f64 / MICRO) } else { 0.0 };
                    if take == 0.0 && b == Bound::Optimistic && c.side == Side::Bid && ci <= qi && c.size < old && book.best_bid() == Some(c.price) { take = (old - c.size) as f64 / MICRO; }
                    take
                }
            };
            let take = take.min(o.r);
            if take > 1e-9 { o.r -= take; fills.push((*n, token.clone(), m(o.q), m(take))); }
        }
        for (n, token, p, s) in fills { *self.held.entry(token.clone()).or_insert(0) += s; self.fill(n, t, &token, p, s, true); }
    }
}

/// Size of a token's level, if any.
fn level(b: &Book, side: Side, price: Micro) -> Option<Micro> {
    if side == Side::Bid { b.bids().find(|x| x.0 == price).map(|x| x.1) } else { b.asks().find(|x| x.0 == price).map(|x| x.1) }
}

impl Venue for TraceVenue {
    fn submit(&mut self, cmd: Command, at: Stamp) {
        let t = (at.wall_ns / 1000) as i64;
        let one_way = (self.cfg.one_way_ms * 1000.0) as i64;
        match cmd {
            Command::Place { n, intent, .. } => match (intent.tif, intent.amount) {
                (Tif::Fak | Tif::Fok, amount) => {
                    let hold = (1e6 * hold_quantile(&self.cfg.taker_hold.quantiles_s, self.rng.next())) as i64;
                    let amount = match amount { Amount::Usd(u) | Amount::Shares(u) => u };
                    self.live.insert(n, Live::Fak { token: intent.token, buy: intent.side == Dir::Buy, amount, limit: intent.limit, at_us: t + one_way + hold });
                }
                (Tif::Gtc | Tif::Gtd { .. }, Amount::Shares(sh)) if intent.side == Dir::Buy => {
                    let book = self.books.get(&intent.token);
                    if intent.post_only && book.and_then(|b| b.best_ask()).is_some_and(|a| a <= intent.limit) {
                        self.events.push((t, VenueEvent::Answer { n, input: Input::Rejected { killed: false, reason: "post-only would cross".into() } }));
                        return;
                    }
                    let qa = book.and_then(|b| level(b, Side::Bid, intent.limit)).unwrap_or(0) as f64 / MICRO;
                    let end_us = self.markets.get(&intent.token).map_or(i64::MAX, |x| x.1 as i64 * 1_000_000);
                    self.live.insert(n, Live::Bid { token: intent.token, o: Order { s: 0, q: intent.limit as f64 / MICRO, r: sh as f64 / MICRO, qa }, from_us: t + one_way, until_us: i64::MAX, end_us });
                }
                _ => self.events.push((t, VenueEvent::Answer { n, input: Input::Rejected { killed: false, reason: "not simulated: resting sell or amount in $".into() } })),
            },
            Command::Cancel { n } => if let Some(Live::Bid { until_us, .. }) = self.live.get_mut(&n) { *until_us = (*until_us).min(t + one_way); },
        }
    }

    fn poll(&mut self, until: Stamp, out: &mut Vec<(Stamp, VenueEvent)>) {
        self.due((until.wall_ns / 1000) as i64);
        self.events.sort_by_key(|e| e.0);
        for (t, e) in self.events.drain(..) { out.push((stamp(t), e)); }
    }
}

/// Money of one market, from the venue's fills and its result.
#[derive(Default, Clone)]
struct Money { cash: i128, shares: [i128; 2], fees: i128, rebates: i128, volume: i128, fills: u64 }

/// A run's result.
pub struct Res { pub id: String, pub variant: String, pub windows: u64, pub orders: u64, pub refused: u64, pub why: Vec<(String, u64)>, pub fills: u64, pub volume: f64, pub fees: f64, pub rebates: f64, pub pnl: f64, pub settled: u64 }

/// `backtest run` with `input: "trace"`.
pub fn run_traces(c: &Config, mut export: Option<&mut dyn Write>) -> Result<(), String> {
    let day = |s: Option<&String>, d: &str| -> Result<String, String> { Ok(match s { Some(x) => crate::packs::ymd((parse_utc(x)? / 86_400) as i64), None => d.into() }) };
    let (from_day, to_day) = (day(c.period.from.as_ref(), "0000-00-00")?, day(c.period.to.as_ref(), "9999-99-99")?);
    let (from_us, to_us) = (c.period.from.as_deref().map(parse_utc).transpose()?.map_or(i64::MIN, |s| s as i64 * 1_000_000), c.period.to.as_deref().map(parse_utc).transpose()?.map_or(i64::MAX, |s| s as i64 * 1_000_000));
    let root = c.path(&c.trace.root);
    let t0 = std::time::Instant::now();
    for (id, variant, params, vcfg) in c.runs()? {
        let r = run_one(c, &root, &from_day, &to_day, from_us, to_us, &params, vcfg, &mut export, &id, &variant)?;
        for (why, n) in &r.why { eprintln!("refused ×{n}: {why}"); }
        println!("{}{}: {} windows, {} orders ({} refused), {} fills, {} sh, fees {} $, rebates {} $, PnL {} $ ({} markets settled)",
            r.id, if r.variant.is_empty() { String::new() } else { format!(" [{}]", r.variant) }, r.windows, r.orders, r.refused, r.fills,
            crate::validate::to_fixed(r.volume, 0), crate::validate::to_fixed(r.fees, 2), crate::validate::to_fixed(r.rebates, 2), crate::validate::to_fixed(r.pnl, 2), r.settled);
    }
    println!("({} s)", t0.elapsed().as_secs());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_one(c: &Config, root: &Path, from_day: &str, to_day: &str, from_us: i64, to_us: i64, params: &serde_json::Map<String, Value>, vcfg: VenueCfg,
    export: &mut Option<&mut dyn Write>, id: &str, variant: &str) -> Result<Res, String> {
    // engine sources: the configured ones, then the signal
    let mut srcs: Vec<Src> = vec![];
    let mut names: Vec<(String, bool)> = vec![];
    for (i, s) in c.sources.iter().enumerate() {
        let gamma = s.adapter == "polymarket-gamma";
        let fa = if gamma { None } else { Some(engine_app::registry::feed_adapter(&s.adapter).ok_or(format!("source {}: adapter {} unknown", s.name, s.adapter))?) };
        let books = fa.as_ref().is_some_and(|x| x.1);
        let fl = files(root, &s.input.trace, &c.trace.formats, from_day, to_day);
        srcs.push(Src { id: i as u16, reader: Reader { files: fl.into_iter(), cur: None, buf: String::new() }, adapter: fa.map(|x| x.0), gamma, lag_us: (s.input.recv_lag_ms * 1000.0) as i64 });
        names.push((s.name.clone(), books));
    }
    let sig_id = names.len() as u16;
    names.push((c.signal.name.clone(), false));
    let sig_of: Vec<Option<usize>> = c.sources.iter().map(|s| c.signal.sources.iter().position(|x| *x == s.name)).collect();
    let mut sig_val: Vec<Option<Micro>> = vec![None; c.signal.sources.len()];
    let mut last_sig: Option<Micro> = None;

    let strat = engine_app::registry::strategy(&c.strategy.kind, Value::Object(params.clone()), &c.market)?;
    let mut eng = Engine::new(names.clone(), vec![strat], c.engine.book_quiet_ms * 1_000_000, c.engine.windows_kept_s)?;
    for (i, n) in names.iter().enumerate() { eng.used[i] = n.1; }
    let mut venue = TraceVenue::new(vcfg);

    // merge: (time, source order, sequence) → item
    let mut heap: BinaryHeap<std::cmp::Reverse<(i64, u16, u64)>> = BinaryHeap::new();
    let mut pending: HashMap<u64, (u16, Item)> = HashMap::new();
    let mut seq = 0u64;
    let mut buf = vec![];
    let mut refill = |s: &mut Src, heap: &mut BinaryHeap<std::cmp::Reverse<(i64, u16, u64)>>, pending: &mut HashMap<u64, (u16, Item)>, seq: &mut u64| -> Result<(), String> {
        buf.clear();
        if read_next(s, &c.market, &mut buf)? { for (t, it) in buf.drain(..) { heap.push(std::cmp::Reverse((t, s.id, *seq))); pending.insert(*seq, (s.id, it)); *seq += 1; } }
        Ok(())
    };
    for s in srcs.iter_mut() { refill(s, &mut heap, &mut pending, &mut seq)?; }

    let tick_us = (c.engine.tick_ms * 1000) as i64;
    let mut next_tick = i64::MIN;
    let mut cmds: Vec<Command> = vec![];
    let mut vev = vec![];
    let mut slugs: HashMap<String, Window> = HashMap::new();
    let mut early: Vec<Window> = vec![];
    let mut resolved: std::collections::HashSet<String> = Default::default();
    let mut ptb: HashMap<String, Micro> = HashMap::new();
    let mut money: HashMap<String, Money> = HashMap::new();
    let mut res = Res { id: id.into(), variant: variant.into(), windows: 0, orders: 0, refused: 0, why: vec![], fills: 0, volume: 0.0, fees: 0.0, rebates: 0.0, pnl: 0.0, settled: 0 };

    // engine input (if any), then its commands to the venue, then the venue's answers back, until quiet
    let step = |eng: &mut Engine, venue: &mut TraceVenue, input: Option<EngineInput>, at: Stamp, cmds: &mut Vec<Command>, vev: &mut Vec<(Stamp, VenueEvent)>, money: &mut HashMap<String, Money>, res: &mut Res, slugs: &HashMap<String, Window>| {
        if let Some(input) = input { eng.on(input, at, cmds); }
        loop {
            for cmd in cmds.drain(..) { if matches!(cmd, Command::Place { .. }) { res.orders += 1; } venue.submit(cmd, at); }
            venue.poll(at, vev);
            if vev.is_empty() { break; }
            for (t, e) in vev.drain(..) {
                if let VenueEvent::Fill { n, size, price, fee, rebate, .. } = &e {
                    if let Some(p) = eng.orders.get(*n as usize) {
                        if let Some(w) = slugs.values().find(|w| w.tokens.contains(&p.intent.token)) {
                            let k = if w.tokens[0] == p.intent.token { 0 } else { 1 };
                            let b = money.entry(w.slug.clone()).or_default();
                            let usd = engine_core::value::amount(*size, *price) as i128;
                            if p.intent.side == Dir::Buy { b.cash -= usd; b.shares[k] += *size as i128; } else { b.cash += usd; b.shares[k] -= *size as i128; }
                            b.cash -= *fee as i128;
                            b.fees += *fee as i128;
                            if let Some(r) = rebate { b.cash += *r as i128; b.rebates += *r as i128; }
                            b.volume += *size as i128;
                            b.fills += 1;
                        }
                    }
                }
                eng.on(EngineInput::Venue(e), t, cmds);
            }
        }
    };

    while let Some(std::cmp::Reverse((t, src, k))) = heap.pop() {
        let (_, item) = pending.remove(&k).unwrap();
        refill(&mut srcs[src as usize], &mut heap, &mut pending, &mut seq)?;
        if t < from_us { if let Item::Window(w) = item { early.retain(|x| x.slug != w.slug); early.push(w); } continue; }
        if t >= to_us { break; }
        // windows announced before the period and still to trade, at its start
        for w in std::mem::take(&mut early) {
            if (w.end as i64) * 1_000_000 <= t || slugs.contains_key(&w.slug) { continue; }
            slugs.insert(w.slug.clone(), w.clone());
            res.windows += 1;
            venue.window(&w);
            step(&mut eng, &mut venue, Some(EngineInput::Window(w)), stamp(t), &mut cmds, &mut vev, &mut money, &mut res, &slugs);
        }
        // whole ticks first, then the venue up to now
        if next_tick == i64::MIN { next_tick = t - t.rem_euclid(tick_us) + tick_us; }
        while next_tick <= t {
            step(&mut eng, &mut venue, Some(EngineInput::Tick), stamp(next_tick), &mut cmds, &mut vev, &mut money, &mut res, &slugs);
            next_tick += tick_us;
        }
        let at = stamp(t);
        match item {
            Item::Feed(payload, event_ms, legacy) => {
                let e = Event { source: src, recv: at, event_ms, seq: None, payload };
                // the signal: the mean of its sources' last values
                if let Some(i) = sig_of[src as usize] {
                    let v = match (&e.payload, c.signal.field.as_str()) {
                        (Payload::Quote { bid, ask }, "mid") => Some((bid + ask) / 2),
                        (Payload::Quote { bid, ask }, _) if legacy => Some((bid + ask) / 2),
                        (Payload::Trade { price, .. }, "trade") => Some(*price),
                        _ => None,
                    };
                    if let Some(v) = v { sig_val[i] = Some(v); }
                }
                // the venue first: fills of this change reach the strategy before the change itself (sim.mjs:446-448)
                venue.feed(t, &e);
                step(&mut eng, &mut venue, None, at, &mut cmds, &mut vev, &mut money, &mut res, &slugs);
                let fed_signal = sig_of[src as usize].is_some();
                step(&mut eng, &mut venue, Some(EngineInput::Feed(e)), at, &mut cmds, &mut vev, &mut money, &mut res, &slugs);
                if fed_signal && sig_val.iter().all(|x| x.is_some()) {
                    let mean = (sig_val.iter().map(|x| x.unwrap() as i128).sum::<i128>() / sig_val.len() as i128) as Micro;
                    if last_sig != Some(mean) {
                        last_sig = Some(mean);
                        let e = Event { source: sig_id, recv: at, event_ms: None, seq: None, payload: Payload::Quote { bid: mean, ask: mean } };
                        step(&mut eng, &mut venue, Some(EngineInput::Feed(e)), at, &mut cmds, &mut vev, &mut money, &mut res, &slugs);
                    }
                }
            }
            Item::Window(w) => {
                if slugs.contains_key(&w.slug) { continue; }
                slugs.insert(w.slug.clone(), w.clone());
                res.windows += 1;
                venue.window(&w);
                step(&mut eng, &mut venue, Some(EngineInput::Window(w)), at, &mut cmds, &mut vev, &mut money, &mut res, &slugs);
            }
            Item::PriceToBeat { slug, value } => { ptb.insert(slug, value); }
            Item::Resolved { slug, up_won } => {
                let Some(w) = slugs.get(&slug).cloned() else { continue };
                if !resolved.insert(slug.clone()) { continue; }
                if let Some(b) = money.get(&slug) {
                    let pnl = b.cash + b.shares[if up_won { 0 } else { 1 }];
                    res.pnl += pnl as f64 / MICRO;
                    res.fees += b.fees as f64 / MICRO;
                    res.rebates += b.rebates as f64 / MICRO;
                    res.volume += b.volume as f64 / MICRO;
                    res.fills += b.fills;
                    res.settled += 1;
                    if let Some(e) = export.as_mut() {
                        writeln!(e, "{}", serde_json::json!({ "id": id, "variant": variant, "slug": slug, "upWon": up_won, "pnl": pnl as f64 / MICRO, "fees": b.fees as f64 / MICRO,
                            "rebates": b.rebates as f64 / MICRO, "volume": b.volume as f64 / MICRO, "fills": b.fills })).map_err(|e| e.to_string())?;
                    }
                    money.remove(&slug);
                }
                let input = EngineInput::Resolved { market: w.market.clone(), winner_token: w.tokens[if up_won { 0 } else { 1 }].clone(), up_won, price_to_beat: ptb.get(&slug).copied().or(w.price_to_beat) };
                step(&mut eng, &mut venue, Some(input), at, &mut cmds, &mut vev, &mut money, &mut res, &slugs);
            }
        }
    }
    res.refused = eng.refused.len() as u64;
    // grouped by kind: numbers replaced by #
    for (_, _, why) in &eng.refused {
        let k: String = why.split(|c: char| c.is_ascii_digit()).filter(|x| !x.is_empty()).collect::<Vec<_>>().join("#");
        match res.why.iter_mut().find(|x| x.0 == k) { Some(x) => x.1 += 1, None => res.why.push((k, 1)) }
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn old_lines_map_to_engine_inputs() {
        let Some(Item::Feed(Payload::Quote { bid, ask }, Some(831), true)) = map_old("mid", &json!({ "k": "px", "t": 1.0, "src": "okx", "p": 82983.35, "et": 831 })) else { panic!() };
        assert_eq!((bid, ask), (82_983_350_000, 82_983_350_000));
        let Some(Item::Feed(Payload::BookDelta { changes }, Some(1000), true)) = map_old("delta", &json!({ "ch": [["a", "B", 0.01, 9650.01], ["b", "A", 0.99, 1.5]], "ts": 1000 })) else { panic!() };
        assert_eq!((changes[0].side, changes[0].price, changes[0].size, changes[1].side), (Side::Bid, 10_000, 9_650_010_000, Side::Ask));
        let Some(Item::Window(w)) = map_old("window", &json!({ "w": { "slug": "btc-updown-5m-300", "start": 300, "len": 300, "tokens": ["u", "d"], "tick": 0.01, "minSize": 5,
            "fee": { "rate": 0.07, "exponent": 1 }, "twap": 60, "priceToBeat": null } })) else { panic!() };
        assert_eq!((w.end, w.market.as_str(), w.tick, w.fee.rate, w.price_to_beat), (600, "btc-updown-5m-300", 10_000, 70_000, None));
        assert!(map_old("clock", &json!({})).is_none());
    }

    fn venue() -> TraceVenue {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("config");
        let c = Config::load(&dir.join("default.json"), &[], None).unwrap();
        TraceVenue::new(c.venue)
    }
    fn ev(payload: Payload) -> Event { Event { source: 0, recv: stamp(0), event_ms: None, seq: None, payload } }
    fn place(n: u32, tif: Tif, amount: Amount, limit: Micro) -> Command {
        Command::Place { n, strategy: 0, maker: 0, taker: 0, intent: engine_app::engine::Intent { client_id: n as u64, token: "u".into(), side: Dir::Buy, limit, amount, tif, post_only: true, reason: "t" } }
    }

    /// A FAK buy meets the asks after one way + hold; a post-only bid that would cross is refused; a resting bid
    /// fills by `exact` when a trade at its level eats the queue ahead of it.
    #[test]
    fn trace_venue_matches_fak_and_resting_bids() {
        let mut v = venue();
        let lv = |p, s| Level { price: p, size: s };
        v.feed(0, &ev(Payload::BookSnapshot { asset: "u".into(), bids: vec![lv(400_000, 3_000_000)], asks: vec![lv(500_000, 10_000_000)], tick: Some(10_000) }));
        v.submit(place(0, Tif::Fak, Amount::Usd(5_000_000), 550_000), stamp(1_000));
        let mut out = vec![];
        v.poll(stamp(2_000), &mut out);
        assert!(out.is_empty(), "not yet arrived");
        v.poll(stamp(10_000_000), &mut out);
        assert!(matches!(out[0].1, VenueEvent::Fill { n: 0, size: 10_000_000, price: 500_000, maker: false, .. }), "{out:?}");
        assert!(matches!(out[1].1, VenueEvent::Closed { n: 0 }));
        out.clear();
        v.submit(place(1, Tif::Gtc, Amount::Shares(5_000_000), 500_000), stamp(10_000_000));
        v.poll(stamp(10_000_000), &mut out);
        assert!(matches!(out[0].1, VenueEvent::Answer { n: 1, input: Input::Rejected { .. } }));
        out.clear();
        // bid at 0.40 behind 3 shares; 7 more join behind it; a trade of 6 eats the 3 ahead, then 3 of ours
        let bid = |size| ev(Payload::BookDelta { changes: vec![Change { asset: "u".into(), side: Side::Bid, price: 400_000, size, best_bid: None, best_ask: None }] });
        v.submit(place(2, Tif::Gtc, Amount::Shares(5_000_000), 400_000), stamp(10_000_000));
        v.feed(10_100_000, &bid(10_000_000));
        v.feed(10_200_000, &ev(Payload::Trade { asset: "u".into(), price: 400_000, size: 6_000_000, side: Side::Ask, id: "x".into() }));
        v.feed(10_200_000, &bid(4_000_000));
        v.poll(stamp(10_200_000), &mut out);
        assert!(matches!(out[0].1, VenueEvent::Fill { n: 2, size: 3_000_000, price: 400_000, maker: true, rebate: Some(_), .. }), "{out:?}");
        // the mirror: a sale of the other token at 0.60 is a trade at our 0.40 level
        v.window(&Window { start: 0, end: 600, slug: "s".into(), market: "m".into(), tokens: ["u".into(), "d".into()], tick: 10_000, min_size: 5_000_000,
            fee: FeeSchedule::default(), taker_delay: true, twap_s: None, price_to_beat: None, winner: None });
        out.clear();
        v.feed(10_300_000, &bid(9_000_000));
        v.feed(10_400_000, &ev(Payload::Trade { asset: "d".into(), price: 600_000, size: 7_000_000, side: Side::Bid, id: "y".into() }));
        v.feed(10_400_000, &bid(2_000_000));
        v.poll(stamp(10_400_000), &mut out);
        assert!(matches!(out[0].1, VenueEvent::Fill { n: 2, size: 2_000_000, .. }), "{out:?}");
    }
}

