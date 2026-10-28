//! Live mode (phase 10): the operator's wallet armed, the reference strategy's decisions sent for real, within the
//! guards and a count of orders. Arming reads the wallet line of the current application (`POLY_WALLET_<ID>=<address>:
//! <v1 encrypted key>`, `src/exec/wallets.js`) from a file only its owner can read, and the passphrase from the first
//! line of stdin; the key lives in memory only. Orders go out on a task of their own, so the hot thread never waits
//! for the venue.
use super::{Fact, Rec};
use adapter_polymarket::exec::{self, Clob, Creds, Sent, ALLOWED, HOST};
use adapter_polymarket::sign::{self, Signer};
use engine_app::engine::{Amount, Command, Tif, Venue, VenueEvent};
use engine_core::account::{self, Account, Dir};
use engine_core::clock::{Clock, Stamp};
use engine_core::guard::{Context, Guards, Limits, NetGuard};
use engine_core::market::Window;
use engine_core::money::Micro;
use engine_core::order::{Input, Order, Phase, TradeStatus};
use std::collections::BTreeSet;
use serde::Deserialize;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use zeroize::Zeroizing;

fn five() -> Micro { 5_000_000 }
fn one() -> u32 { 1 }
#[derive(Deserialize)]
pub struct LiveCfg {
    /// File holding the wallet line; refused if its group or others may read it.
    pub wallet_file: String,
    /// Largest opening order, pUSD micro-units; above, the order is refused, not reduced.
    #[serde(default = "five")]
    pub max_order: Micro,
    /// Session loss at which openings stop, pUSD micro-units.
    #[serde(default = "five")]
    pub session_loss: Micro,
    /// Orders sent in this run, at most; then no more openings.
    #[serde(default = "one")]
    pub orders: u32,
}

pub struct Armed { pub signer: Arc<Signer>, pub creds: Arc<Creds>, pub clock_synced: bool }

/// `POLY_WALLET_<ID>=<address>:<payload>` or `<address>:<payload>` → (address, payload).
pub fn wallet_line(text: &str) -> Result<(String, String), &'static str> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')).ok_or("empty wallet file")?;
    let value = line.split_once('=').map_or(line, |(_, v)| v).trim().trim_matches('"');
    let (addr, payload) = value.split_once(':').ok_or("no encrypted key after the address")?;
    let ok = addr.len() == 42 && addr.starts_with("0x") && addr[2..].bytes().all(|b| b.is_ascii_hexdigit());
    if !ok || !payload.starts_with("v1.") { return Err("not <address>:v1.<…>"); }
    Ok((addr.to_string(), payload.to_string()))
}

/// Decrypts the key and gets the account's API credentials. Panics with a message that says nothing of the key.
pub fn arm(cfg: &LiveCfg) -> Armed {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&cfg.wallet_file).expect("live: wallet file").permissions().mode();
        assert!(mode & 0o077 == 0, "live: wallet file readable by others (chmod 600)");
    }
    let (wallet, payload) = wallet_line(&Zeroizing::new(std::fs::read_to_string(&cfg.wallet_file).expect("live: wallet file"))).expect("live: wallet file");
    // reserved ahead: a buffer that grew while reading would leave unwiped copies of the passphrase behind
    let mut pass = Zeroizing::new(String::with_capacity(4096));
    std::io::stdin().read_line(&mut pass).expect("live: passphrase on stdin");
    let key = engine_core::key::decrypt_v1(&payload, pass.trim_end_matches(['\r', '\n'])).unwrap_or_else(|e| panic!("live: key not decrypted ({e:?})"));
    drop(pass);
    let signer = Arc::new(Signer::from_bytes(&key, &wallet).expect("live: key"));
    drop(key);
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let creds = rt.block_on(derive_creds(&signer)).unwrap_or_else(|e| panic!("live: API credentials: {e}"));
    let clock_synced = std::process::Command::new("timedatectl").args(["show", "-p", "NTPSynchronized", "--value"]).output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "yes");
    Armed { signer, creds: Arc::new(creds), clock_synced }
}

/// `GET /auth/derive-api-key` (L1): the account's existing credentials, as the SDK's `createOrDeriveApiKey` falls back to.
// shortcut: derive only, no creation (POST /auth/api-key); an account that never had credentials stops here
async fn derive_creds(s: &Signer) -> Result<Creds, String> {
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let req = format!("GET /auth/derive-api-key HTTP/1.1\r\nHost: {HOST}\r\n{}\r\n", s.l1_headers(ts));
    let mut clob = Clob::new(HOST, 443, true, NetGuard::new(&ALLOWED, Arc::new(AtomicBool::new(false))));
    let r = clob.get(req.as_bytes(), Duration::from_secs(5)).await.map_err(|e| format!("{e:?}"))?;
    exec::read_creds(&r).ok_or_else(|| format!("HTTP {}", r.status))
}

/// One order handed to the sender: its number, the machine (signed), the request, and the hash to query it by.
pub struct ToSend { pub n: u32, pub order: Order, pub req: Vec<u8>, pub hash: String }
/// What became of it: the machine after the answer (and queries), the instants, and inputs learnt later.
pub struct Done { pub n: u32, pub order: Order, pub sent: Sent }

/// The sender task: one warm connection, kept alive off the hot path; an unknown order is queried, never resent.
pub fn spawn_sender(a: &Armed, armed: Arc<AtomicBool>, clock: Arc<Clock>) -> (mpsc::UnboundedSender<ToSend>, mpsc::UnboundedReceiver<Done>) {
    let (tx, mut rx) = mpsc::unbounded_channel::<ToSend>();
    let (done_tx, done_rx) = mpsc::unbounded_channel::<Done>();
    let (signer, creds) = (a.signer.clone(), a.creds.clone());
    tokio::spawn(async move {
        let mut clob = Clob::new(HOST, 443, true, NetGuard::new(&ALLOWED, armed));
        // measured on real orders (2026-10-21/22): the venue answers 0.87 to 3.5 s after the write, past the 300 ms of
        // the design; waiting longer keeps a merely slow order from passing through "unknown"
        clob.deadline = Duration::from_secs(5);
        let _ = clob.warm().await;
        let mut keep = tokio::time::interval(Duration::from_secs(20));
        loop {
            tokio::select! {
                m = rx.recv() => {
                    let Some(ToSend { n, mut order, req, hash }) = m else { return };
                    let sent = clob.post(&req, &mut order, &clock).await;
                    let id = order.id.clone().unwrap_or(hash);
                    // what the answer did not say, a query may: every 2 s for a minute, then the order stays unknown
                    for _ in 0..30 {
                        if order.phase != Phase::Unknown { break; }
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        let ts = clock.now().wall_ns / 1_000_000_000;
                        let Ok(q) = exec::query_request(HOST, &signer, &creds, &id, ts) else { break };
                        if let Ok(r) = clob.get(&q, Duration::from_secs(2)).await { for i in exec::read_order(&r, &id) { let _ = order.apply(i); } }
                    }
                    if done_tx.send(Done { n, order, sent }).is_err() { return; }
                }
                _ = keep.tick() => { let _ = clob.warm().await; }
            }
        }
    });
    (tx, done_rx)
}

/// What the ledger needs of a real order's market to write its trades.
pub struct Meta { pub token: String, pub market: String, pub fee_rate: Micro, pub fee_exponent: u32, pub end_ms: u64, pub decided_ns: u64 }

/// The real venue: the guards, the orders of this run, and the sender task. Only FAK buys are sent (the reference
/// holds to settlement); anything else is refused and recorded. What the shell knows of the run (valid sources,
/// reconciliation, result, balance) is set before each `submit`.
pub struct LiveVenue {
    pub armed: Armed,
    pub guards: Guards,
    max_orders: u32,
    tx: mpsc::UnboundedSender<ToSend>,
    /// This run's orders, by their live number (the ledger's `n`); `engine_n`: the engine's number of each.
    pub orders: Vec<(Meta, Order)>,
    engine_n: Vec<u32>,
    /// User channel trades of an order whose id is not known yet (the channel may beat the answer), replayed after it.
    pub early: Vec<(String, String, TradeStatus, Micro, Micro)>,
    windows: Vec<Window>,
    pub sources_valid: bool,
    pub reconciled: bool,
    pub session_pnl: Micro,
    pub balance: Option<Micro>,
    /// Facts to write, drained by the shell after each call.
    pub facts: Vec<Fact>,
    /// Answers the sender gave back (`Done`), handed in by the shell, read by `poll`.
    pub inbox: Vec<Done>,
}

impl LiveVenue {
    pub fn new(a: Armed, l: &LiveCfg, armed: Arc<AtomicBool>, clock: Arc<Clock>) -> (LiveVenue, mpsc::UnboundedReceiver<Done>) {
        let (tx, rx) = spawn_sender(&a, armed.clone(), clock);
        let guards = Guards::new(Limits { max_order: l.max_order, session_loss: l.session_loss }, armed);
        // "orders": 0 arms for the reads only (credentials, balance, user channel): nothing may open
        guards.stop.store(l.orders == 0, std::sync::atomic::Ordering::Release);
        (LiveVenue { armed: a, guards, max_orders: l.orders, tx, orders: vec![], engine_n: vec![], early: vec![], windows: vec![], sources_valid: false, reconciled: false,
            session_pnl: 0, balance: None, facts: vec![], inbox: vec![] }, rx)
    }

    pub fn window(&mut self, w: &Window) { self.windows.push(w.clone()); let last = w.start; self.windows.retain(|x| x.start + 3600 > last); }

    /// Orders that may rest at the venue (accepted, nothing matched yet), for the reconciliation.
    pub fn resting(&self) -> BTreeSet<String> { self.orders.iter().filter(|(_, o)| o.phase == Phase::Acked).filter_map(|(_, o)| o.id.clone()).collect() }

    /// A trade of the user channel: to its order, and as an accounting fact with the venue's size and price.
    pub fn trade(&mut self, order: String, trade: String, status: TradeStatus, size: Micro, price: Micro, rec: &Rec, acct: &mut Account) {
        let Some((m, o)) = self.orders.iter_mut().find(|(_, o)| o.id.as_deref() == Some(order.as_str())) else {
            // not ours, or ours before its answer: kept a while (an account trades rarely)
            if self.early.len() < 1000 { self.early.push((order, trade, status, size, price)); }
            return;
        };
        let _ = o.apply(Input::Trade { id: trade.clone(), status, size, price });
        rec.keep(acct, account::Fact::Trade { trade, order, market: m.market.clone(), token: m.token.clone(), dir: account::Dir::Buy, status, size, price,
            fee_rate: m.fee_rate, fee_exponent: m.fee_exponent, end_ms: m.end_ms });
    }

    fn refuse(&mut self, slug: String, why: String) { self.facts.push(Fact::LiveRefused { slug, why }); }
}

impl Venue for LiveVenue {
    /// Each command past the guards becomes a real order, signed here, written by the sender.
    fn submit(&mut self, cmd: Command, now: Stamp) {
        let Command::Place { n: engine_n, intent: d, maker, taker, .. } = cmd else { return self.refuse(String::new(), "cancel not sent (FAK only)".into()) };
        let Some(w) = self.windows.iter().find(|w| w.tokens.contains(&d.token)) else { return self.refuse(String::new(), "market unknown".into()) };
        let (slug, market, fee, end) = (w.slug.clone(), w.market.clone(), w.fee, w.end);
        if d.tif != Tif::Fak { return self.refuse(slug, format!("{:?} not sent (FAK only)", d.tif)); }
        // shortcut: buys only (the reference holds to settlement, exit "hold"); a sell would need the held size checked
        if d.side != Dir::Buy || !matches!(d.amount, Amount::Usd(_)) { return self.refuse(slug, "sell not sent".into()); }
        let ctx = Context {
            sources_valid: self.sources_valid,
            order_pending: self.orders.iter().any(|(_, o)| o.blocks_openings()),
            reconciled: self.reconciled,
            clock_synced: self.armed.clock_synced,
            prepared: end * 1_000_000_000 > now.wall_ns,
            // shortcut: realized only; the latent part waits for a run with more than a few orders
            session_pnl: self.session_pnl,
            now_ns: now.mono_ns,
        };
        if let Err(r) = self.guards.check_open(maker as Micro, &ctx) { return self.refuse(slug, format!("{r:?}")); }
        // trading goes on while the wallet can pay: the last balance read must cover the order
        if self.balance.is_none_or(|b| b < maker as Micro) { let why = format!("balance {:?} < {}", self.balance, maker); return self.refuse(slug, why); }
        let o = sign::Order { salt: (now.wall_ns / 1000) & ((1 << 53) - 1), token_id: d.token.clone(), maker_amount: maker, taker_amount: taker, side: sign::Side::Buy, timestamp_ms: now.wall_ns / 1_000_000 };
        let hash = self.armed.signer.order_hash(&o);
        let body = self.armed.signer.body(&o, &self.armed.creds.key);
        let req = match exec::order_request(HOST, &self.armed.signer, &self.armed.creds, &body, now.wall_ns / 1_000_000_000) {
            Ok(r) => r,
            Err(e) => return self.refuse(slug, format!("{e:?}")),
        };
        let mut order = Order::default();
        let _ = order.apply(Input::Signed);
        let n = self.orders.len() as u32;
        self.facts.push(Fact::LiveOrder { n, slug, token: d.token.clone(), limit: d.limit, maker, taker, hash: hash.clone() });
        self.orders.push((Meta { token: d.token, market, fee_rate: fee.rate, fee_exponent: fee.exponent, end_ms: end * 1000, decided_ns: now.mono_ns }, order.clone()));
        self.engine_n.push(engine_n);
        // the count of this run: once reached, the stop refuses every further opening
        if self.orders.len() as u32 >= self.max_orders { self.guards.stop.store(true, std::sync::atomic::Ordering::Release); }
        let _ = self.tx.send(ToSend { n, order, req, hash });
    }

    /// What the venue answered (and queries learnt), from the answers handed in.
    fn poll(&mut self, until: Stamp, out: &mut Vec<(Stamp, VenueEvent)>) {
        for Done { n, order, sent } in std::mem::take(&mut self.inbox) {
            let m = &self.orders[n as usize].0;
            self.facts.push(Fact::LiveAnswer { n, phase: order.phase, id: order.id.clone(),
                decision_to_written_us: sent.written.map(|w| (w.mono_ns - m.decided_ns) as f64 / 1000.0),
                written_to_answer_ms: sent.written.zip(sent.answered).map(|(w, a)| (a.mono_ns - w.mono_ns) as f64 / 1e6), late: sent.late,
                status: sent.raw.as_ref().map(|r| r.status), body: sent.raw.as_ref().map(|r| String::from_utf8_lossy(&r.body).chars().take(2000).collect()), back_off_s: sent.back_off_s });
            if let Some(s) = sent.back_off_s { self.guards.back_off_until_ns = until.mono_ns + s * 1_000_000_000; }
            let input = sent.raw.as_ref().map_or(Input::NoAnswer, |r| exec::read_answer(r).input);
            self.orders[n as usize].1 = order;
            out.push((until, VenueEvent::Answer { n: self.engine_n[n as usize], input }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_lines_of_the_current_application_are_read() {
        let a = "0x515a00000000000000000000000000000000610c";
        assert_eq!(wallet_line(&format!("POLY_WALLET_MAIN={a}:v1.s.i.t.c\n")), Ok((a.into(), "v1.s.i.t.c".into())));
        assert_eq!(wallet_line(&format!("# comment\n\n\"{a}:v1.s.i.t.c\"")), Ok((a.into(), "v1.s.i.t.c".into())));
        assert!(wallet_line(a).is_err());
        assert!(wallet_line(&format!("{a}:v2.x")).is_err());
        assert!(wallet_line("0x12:v1.s.i.t.c").is_err());
        assert!(wallet_line("").is_err());
    }
}
