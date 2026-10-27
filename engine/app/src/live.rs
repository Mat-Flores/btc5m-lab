//! Live mode (phase 10): the operator's wallet armed, the reference strategy's decisions sent for real, within the
//! guards and a count of orders. Arming reads the wallet line of the current application (`POLY_WALLET_<ID>=<address>:
//! <v1 encrypted key>`, `src/exec/wallets.js`) from a file only its owner can read, and the passphrase from the first
//! line of stdin; the key lives in memory only. Orders go out on a task of their own, so the hot thread never waits
//! for the venue.
use adapter_polymarket::exec::{self, Clob, Creds, Sent, ALLOWED, HOST};
use adapter_polymarket::sign::Signer;
use engine_core::clock::Clock;
use engine_core::guard::NetGuard;
use engine_core::money::Micro;
use engine_core::order::{Order, Phase};
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
