//! Polymarket execution: one warm connection to the CLOB, `POST /order` written whole, its answer turned into a fact
//! of the order state machine. What the answer means follows the official SDK (`@polymarket/bindings` 0.13,
//! `OrderResponseSchema`): accepted only if `success`, empty `errorMsg`, an `orderID` and a status live, matched or
//! delayed. A FAK that finds nothing comes back as an HTTP 400 with the CLOB's text (seen on a real order,
//! 2026-10-17) or, per the SDK, as status `unmatched`.
//! Anything that does not say what became of the order (no answer in time, a cut after the write, a 5xx, an
//! unreadable body) leaves it unknown: it may have been matched, so it is never sent again, only queried.
use crate::sign::{l2_signature, Signer};
use engine_core::clock::{Clock, Stamp};
use engine_core::guard::{Allowed, NetGuard};
use engine_core::http::{dial, Conn, HttpError, Response};
use engine_core::order::{AckStatus, Input, Order, Phase};
use std::time::Duration;

pub const HOST: &str = "clob.polymarket.com";
/// Every write the engine may make to Polymarket; anything else is refused by the network guard.
pub static ALLOWED: [Allowed; 3] = [
    Allowed { method: "POST", host: HOST, path: "/order" },
    Allowed { method: "DELETE", host: HOST, path: "/order" },
    Allowed { method: "POST", host: HOST, path: "/auth/api-key" },
];
const FAK_KILLED: &str = "no orders found to match with FAK order";

/// API credentials: they let anyone trade the account, so they are wiped when dropped and never logged.
pub struct Creds { pub key: String, pub secret: String, pub passphrase: String }
impl Drop for Creds { fn drop(&mut self) { use zeroize::Zeroize; self.key.zeroize(); self.secret.zeroize(); self.passphrase.zeroize(); } }

/// Reads the answer of `POST /auth/api-key` (SDK `ApiKeyCredsSchema`).
pub fn read_creds(r: &Response) -> Option<Creds> {
    #[derive(serde::Deserialize)]
    struct C { #[serde(rename = "apiKey")] api_key: String, secret: String, passphrase: String }
    if r.status != 200 { return None; }
    let c: C = serde_json::from_slice(&r.body).ok()?;
    // they go into request headers: a control character would let the answer inject headers into every order
    if [&c.api_key, &c.secret, &c.passphrase].iter().any(|v| v.is_empty() || v.chars().any(|ch| ch.is_control())) { return None; }
    Some(Creds { key: c.api_key, secret: c.secret, passphrase: c.passphrase })
}

/// What the venue said, in the state machine's terms. `back_off_s`: the venue asked to wait before the next request.
#[derive(Debug, PartialEq)]
pub struct Answer { pub input: Input, pub back_off_s: Option<u64> }

/// Reads one `POST /order` answer.
pub fn read_answer(r: &Response) -> Answer {
    #[derive(serde::Deserialize)]
    struct Body { #[serde(default)] success: bool, #[serde(default, rename = "errorMsg")] error_msg: String, #[serde(default, rename = "orderID")] order_id: String,
        #[serde(default)] status: String, #[serde(default, rename = "tradeIDs")] trade_ids: Vec<String>, #[serde(default)] error: Option<String> }
    let back_off_s = matches!(r.status, 425 | 429).then_some(r.retry_after_s.unwrap_or(1));
    // the raw answer is kept by the caller (`Sent::raw`) for the ledger
    let unknown = || Answer { input: Input::NoAnswer, back_off_s: None };
    // a 5xx says nothing about the order: a gateway may fail after the CLOB matched it
    if r.status >= 500 { return unknown(); }
    let Ok(b) = serde_json::from_slice::<Body>(&r.body) else {
        return if r.status >= 400 { Answer { input: Input::Rejected { killed: false, reason: format!("{}: {}", r.status, String::from_utf8_lossy(&r.body)) }, back_off_s } } else { unknown() };
    };
    if r.status >= 400 {
        let msg = b.error.unwrap_or(b.error_msg);
        return Answer { input: Input::Rejected { killed: msg.contains(FAK_KILLED), reason: format!("{}: {msg}", r.status) }, back_off_s };
    }
    let status = match b.status.as_str() { "live" => Some(AckStatus::Live), "matched" => Some(AckStatus::Matched), "delayed" => Some(AckStatus::Delayed), _ => None };
    match status {
        Some(status) if b.success && b.error_msg.is_empty() && !b.order_id.is_empty() => Answer { input: Input::Accepted { id: b.order_id, status, trades: b.trade_ids }, back_off_s: None },
        _ if b.status == "unmatched" || b.error_msg.contains(FAK_KILLED) => Answer { input: Input::Rejected { killed: true, reason: b.error_msg }, back_off_s: None },
        _ if !b.success => Answer { input: Input::Rejected { killed: false, reason: format!("{}: {}", b.status, b.error_msg) }, back_off_s: None },
        _ => unknown(),
    }
}

/// The full `POST /order` request, ready to write.
pub fn order_request(host: &str, s: &Signer, c: &Creds, body: &str, ts_s: u64) -> Result<Vec<u8>, crate::sign::OrderError> {
    let sig = l2_signature(&c.secret, ts_s, "POST", "/order", body)?;
    Ok(format!("POST /order HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nPOLY_ADDRESS: {}\r\nPOLY_API_KEY: {}\r\nPOLY_PASSPHRASE: {}\r\nPOLY_SIGNATURE: {sig}\r\nPOLY_TIMESTAMP: {ts_s}\r\nContent-Length: {}\r\n\r\n{body}",
        s.address, c.key, c.passphrase, body.len()).into_bytes())
}

/// `POST /auth/api-key`: creates the account's API credentials (L1, signed by the EOA). Off the hot path, at arming.
pub fn api_key_request(host: &str, s: &Signer, ts_s: u64) -> Vec<u8> {
    format!("POST /auth/api-key HTTP/1.1\r\nHost: {host}\r\n{}Content-Length: 0\r\n\r\n", s.l1_headers(ts_s)).into_bytes()
}

/// `GET /data/order/{id}`, L2-authenticated: what became of an order whose answer was lost.
pub fn query_request(host: &str, s: &Signer, c: &Creds, id: &str, ts_s: u64) -> Result<Vec<u8>, crate::sign::OrderError> { get_request(host, s, c, &format!("/data/order/{id}"), "", ts_s) }

/// An L2-authenticated GET. The signature covers the path without its query `[non vérifié sur requête réelle]`.
pub fn get_request(host: &str, s: &Signer, c: &Creds, path: &str, query: &str, ts_s: u64) -> Result<Vec<u8>, crate::sign::OrderError> {
    let sig = l2_signature(&c.secret, ts_s, "GET", path, "")?;
    Ok(format!("GET {path}{query} HTTP/1.1\r\nHost: {host}\r\nPOLY_ADDRESS: {}\r\nPOLY_API_KEY: {}\r\nPOLY_PASSPHRASE: {}\r\nPOLY_SIGNATURE: {sig}\r\nPOLY_TIMESTAMP: {ts_s}\r\n\r\n",
        s.address, c.key, c.passphrase).into_bytes())
}

/// Facts learnt from a queried order (SDK `OpenOrderSchema`: `id`, `status`, `associate_trades`). Empty: not found
/// (yet), and the order stays unknown; it is never presumed absent.
pub fn read_order(r: &Response, id: &str) -> Vec<Input> {
    #[derive(serde::Deserialize)]
    struct O { id: String, #[serde(default)] status: String, #[serde(default)] associate_trades: Vec<String> }
    if r.status != 200 { return vec![]; }
    let Ok(o) = serde_json::from_slice::<O>(&r.body) else { return vec![] };
    // an answer about another order says nothing about this one
    if o.id != id { return vec![]; }
    let closed = { let s = o.status.to_ascii_uppercase(); s.contains("CANCEL") || s.contains("UNMATCHED") };
    let status = if o.associate_trades.is_empty() { AckStatus::Delayed } else { AckStatus::Matched };
    let mut v = vec![Input::Accepted { id: o.id, status, trades: o.associate_trades }];
    if closed { v.push(Input::Closed); }
    v
}

/// Instants of one send, for the ledger and the latency measures.
#[derive(Debug, Default)]
pub struct Sent { pub written: Option<Stamp>, pub answered: Option<Stamp>, pub raw: Option<Response>, pub late: bool, pub back_off_s: Option<u64> }

pub struct Clob {
    /// The venue the guard checks and the requests name (`Host`); `host:port` is where the connection goes.
    pub venue: &'static str,
    pub host: String,
    pub port: u16,
    pub tls: bool,
    /// Time for an answer before the order is unknown: round trip p99 + market delay + margin (§3.5).
    pub deadline: Duration,
    /// After the deadline, how long a late answer is still awaited on the same connection.
    pub grace: Duration,
    pub net: NetGuard,
    conn: Option<Conn>,
}

impl Clob {
    pub fn new(host: &str, port: u16, tls: bool, net: NetGuard) -> Self {
        Clob { venue: HOST, host: host.into(), port, tls, deadline: Duration::from_millis(300), grace: Duration::from_secs(10), net, conn: None }
    }

    /// Opens the connection if needed and keeps it alive with a cheap read; off the hot path.
    pub async fn warm(&mut self) -> Result<(), HttpError> {
        if self.conn.is_none() { self.conn = Some(Conn::new(dial(&self.host, self.port, self.tls).await?)); }
        let c = self.conn.as_mut().unwrap();
        let r = tokio::time::timeout(Duration::from_secs(2), async { c.write(format!("GET /time HTTP/1.1\r\nHost: {}\r\n\r\n", self.venue).as_bytes()).await?; c.read().await }).await.unwrap_or(Err(HttpError::Timeout));
        if r.as_ref().map_or(true, |r| r.close) { self.conn = None; }
        r.map(|_| ())
    }

    /// A read on the warm connection (a query); never an order.
    pub async fn get(&mut self, req: &[u8], timeout: Duration) -> Result<Response, HttpError> {
        // orders go through `post` only, past the network guard and the signed-once check
        if !req.starts_with(b"GET ") { return Err(HttpError::Url); }
        if self.conn.is_none() { self.conn = Some(Conn::new(dial(&self.host, self.port, self.tls).await?)); }
        let c = self.conn.as_mut().unwrap();
        let r = tokio::time::timeout(timeout, async { c.write(req).await?; c.read().await }).await.unwrap_or(Err(HttpError::Timeout));
        if r.as_ref().map_or(true, |r| r.close) { self.conn = None; }
        r
    }

    /// Writes a signed order and applies to `order` what the venue answered. The order must be `Signed`.
    /// Never retries: whatever fails after the first byte may have reached the venue.
    pub async fn post(&mut self, req: &[u8], order: &mut Order, clock: &Clock) -> Sent {
        let mut sent = Sent::default();
        // the only guard against writing the same order twice: once written it is no longer `Signed`
        if order.phase != Phase::Signed { return sent; }
        if !self.net.permits("POST", self.venue, "/order") {
            let _ = order.apply(Input::Rejected { killed: false, reason: "network guard".into() });
            return sent;
        }
        if self.conn.is_none() {
            match dial(&self.host, self.port, self.tls).await {
                Ok(io) => self.conn = Some(Conn::new(io)),
                // nothing was written: the order never left
                Err(e) => { let _ = order.apply(Input::Rejected { killed: false, reason: format!("connect: {e:?}") }); return sent; }
            }
        }
        let c = self.conn.as_mut().unwrap();
        let wrote = c.write(req).await;
        sent.written = Some(clock.now());
        let _ = order.apply(Input::Sent);
        if wrote.is_err() { self.conn = None; let _ = order.apply(Input::NoAnswer); return sent; }
        let first = tokio::time::timeout(self.deadline, c.read()).await;
        let answer = match first {
            Ok(r) => r,
            Err(_) => {
                let _ = order.apply(Input::NoAnswer);
                sent.late = true;
                tokio::time::timeout(self.grace, c.read()).await.unwrap_or(Err(HttpError::Timeout))
            }
        };
        match answer {
            Ok(r) => {
                sent.answered = Some(clock.now());
                let a = read_answer(&r);
                sent.back_off_s = a.back_off_s;
                let _ = order.apply(a.input);
                if r.close { self.conn = None; }
                sent.raw = Some(r);
            }
            Err(_) => { self.conn = None; let _ = order.apply(Input::NoAnswer); }
        }
        sent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(status: u16, body: &str) -> Response { Response { status, body: body.as_bytes().to_vec(), retry_after_s: None, close: false } }

    #[test]
    fn answers_mean_what_the_sdk_says() {
        let ok = r#"{"errorMsg":"","makingAmount":"5","orderID":"0xabc","status":"matched","success":true,"takingAmount":"9.09","tradeIDs":["t1"],"transactionsHashes":[]}"#;
        assert_eq!(read_answer(&r(200, ok)).input, Input::Accepted { id: "0xabc".into(), status: AckStatus::Matched, trades: vec!["t1".into()] });
        let delayed = r#"{"errorMsg":"","makingAmount":"0","orderID":"0xabc","status":"delayed","success":true,"takingAmount":"0"}"#;
        assert_eq!(read_answer(&r(200, delayed)).input, Input::Accepted { id: "0xabc".into(), status: AckStatus::Delayed, trades: vec![] });
        // the real answer of a killed FAK (2026-10-17), and the SDK's other form
        let killed = r#"{"error":"no orders found to match with FAK order. FAK orders are partially filled or killed if no match is found."}"#;
        assert!(matches!(read_answer(&r(400, killed)).input, Input::Rejected { killed: true, .. }));
        assert!(matches!(read_answer(&r(200, r#"{"errorMsg":"","orderID":"0xabc","status":"unmatched","success":false}"#)).input, Input::Rejected { killed: true, .. }));
        assert!(matches!(read_answer(&r(400, r#"{"error":"not enough balance / allowance"}"#)).input, Input::Rejected { killed: false, .. }));
        assert!(matches!(read_answer(&r(200, r#"{"errorMsg":"invalid nonce","orderID":"","status":"","success":false}"#)).input, Input::Rejected { killed: false, .. }));
        assert!(matches!(read_answer(&r(401, "Unauthorized")).input, Input::Rejected { killed: false, .. }));
    }

    #[test]
    fn what_says_nothing_about_the_order_leaves_it_unknown() {
        for (s, b) in [(500, "{}"), (502, "<html>bad gateway</html>"), (503, ""), (504, "timeout"), (200, "not json"), (200, r#"{"success":true,"errorMsg":"","orderID":"0xabc","status":"weird"}"#)] {
            assert_eq!(read_answer(&r(s, b)).input, Input::NoAnswer, "{s} {b}");
        }
    }

    #[test]
    fn a_queried_order_settles_the_unknown_one_or_leaves_it() {
        let mut o = Order::default();
        for i in [Input::Signed, Input::Sent, Input::NoAnswer] { o.apply(i).unwrap(); }
        assert!(read_order(&r(404, ""), "0xabc").is_empty());
        assert!(read_order(&r(200, "null"), "0xabc").is_empty());
        assert!(read_order(&r(200, r#"{"id":"0xother","status":"MATCHED","associate_trades":["t1"]}"#), "0xabc").is_empty());
        assert_eq!(o.phase, engine_core::order::Phase::Unknown);
        let killed = read_order(&r(200, r#"{"id":"0xabc","status":"CANCELED","associate_trades":[],"size_matched":"0"}"#), "0xabc");
        let mut k = o.clone();
        for i in killed { k.apply(i).unwrap(); }
        assert_eq!(k.phase, engine_core::order::Phase::Killed);
        for i in read_order(&r(200, r#"{"id":"0xabc","status":"MATCHED","associate_trades":["t1"],"size_matched":"9.09"}"#), "0xabc") { o.apply(i).unwrap(); }
        assert_eq!((o.phase, o.id.as_deref()), (engine_core::order::Phase::Matched, Some("0xabc")));
    }

    #[test]
    fn credentials_that_could_inject_headers_are_refused() {
        assert!(read_creds(&r(200, r#"{"apiKey":"k","secret":"s","passphrase":"p"}"#)).is_some());
        // valid JSON whose value decodes to CR LF
        assert!(read_creds(&r(200, r#"{"apiKey":"k\r\nX-Evil: 1","secret":"s","passphrase":"p"}"#)).is_none());
        assert!(read_creds(&r(200, r#"{"apiKey":"","secret":"s","passphrase":"p"}"#)).is_none());
    }

    #[test]
    fn rate_limits_ask_to_wait() {
        let a = read_answer(&Response { status: 429, body: b"{\"error\":\"too many requests\"}".to_vec(), retry_after_s: Some(7), close: false });
        assert!(matches!(a.input, Input::Rejected { killed: false, .. }));
        assert_eq!(a.back_off_s, Some(7));
        assert_eq!(read_answer(&r(425, "")).back_off_s, Some(1));
    }
}
