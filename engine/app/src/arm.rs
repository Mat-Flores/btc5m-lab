//! Arming a wallet for live orders, shared by the engine's live mode and copy-trading. Reads the wallet line of the
//! current application (`POLY_WALLET_<ID>=<address>:<v1 encrypted key>`, `src/exec/wallets.js`) from a file only its
//! owner can read, and the passphrase from the first line of stdin; the key lives in memory only.
use adapter_polymarket::exec::{self, Clob, Creds, ALLOWED, HOST};
use adapter_polymarket::sign::Signer;
use engine_core::guard::NetGuard;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use zeroize::Zeroizing;

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
pub fn arm(wallet_file: &str, http_timeout: Duration) -> Armed {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(wallet_file).expect("live: wallet file").permissions().mode();
        assert!(mode & 0o077 == 0, "live: wallet file readable by others (chmod 600)");
    }
    let (wallet, payload) = wallet_line(&Zeroizing::new(std::fs::read_to_string(wallet_file).expect("live: wallet file"))).expect("live: wallet file");
    // reserved ahead: a buffer that grew while reading would leave unwiped copies of the passphrase behind
    let mut pass = Zeroizing::new(String::with_capacity(4096));
    std::io::stdin().read_line(&mut pass).expect("live: passphrase on stdin");
    let key = engine_core::key::decrypt_v1(&payload, pass.trim_end_matches(['\r', '\n'])).unwrap_or_else(|e| panic!("live: key not decrypted ({e:?})"));
    drop(pass);
    let signer = Arc::new(Signer::from_bytes(&key, &wallet).expect("live: key"));
    drop(key);
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let creds = rt.block_on(derive_creds(&signer, http_timeout)).unwrap_or_else(|e| panic!("live: API credentials: {e}"));
    let clock_synced = std::process::Command::new("timedatectl").args(["show", "-p", "NTPSynchronized", "--value"]).output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "yes");
    Armed { signer, creds: Arc::new(creds), clock_synced }
}

/// `GET /auth/derive-api-key` (L1): the account's existing credentials, as the SDK's `createOrDeriveApiKey` falls back to.
// shortcut: derive only, no creation (POST /auth/api-key); an account that never had credentials stops here
async fn derive_creds(s: &Signer, timeout: Duration) -> Result<Creds, String> {
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let req = format!("GET /auth/derive-api-key HTTP/1.1\r\nHost: {HOST}\r\n{}\r\n", s.l1_headers(ts));
    let mut clob = Clob::new(HOST, 443, true, NetGuard::new(&ALLOWED, Arc::new(AtomicBool::new(false))));
    let r = clob.get(req.as_bytes(), timeout).await.map_err(|e| format!("{e:?}"))?;
    exec::read_creds(&r).ok_or_else(|| format!("HTTP {}", r.status))
}
