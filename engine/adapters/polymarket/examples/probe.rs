//! Latency probe of the order path (phase 10 investigation): on one warm connection, GET /time round trips, then
//! POST /order of FAK buys priced 0.01 (far under the asks: they cannot match), timed from the last byte written to
//! the answer. Key from PROBE_KEY (hex), wallet from PROBE_WALLET; default: the public test key, whose balance is 0.
//! Usage: cargo run --release --example probe -- <orders>
use adapter_polymarket::exec::{self, Clob, ALLOWED, HOST};
use adapter_polymarket::sign::{self, Signer};
use adapter_polymarket::windows;
use engine_core::clock::Clock;
use engine_core::guard::NetGuard;
use engine_core::order::{Input, Order};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(5);
    let key_hex = std::env::var("PROBE_KEY").unwrap_or_else(|_| "11".repeat(32));
    let wallet = std::env::var("PROBE_WALLET").unwrap_or_else(|_| "0x7a57144fafde87f5ce4a880d570bae58eebc83f2".into());
    let mut key = [0u8; 32];
    for (i, b) in key.iter_mut().enumerate() { *b = u8::from_str_radix(&key_hex[2 * i..2 * i + 2], 16).unwrap(); }
    let s = Signer::from_bytes(&key, &wallet).unwrap();
    let clock = Clock::new();
    let now_s = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let mut clob = Clob::new(HOST, 443, true, NetGuard::new(&ALLOWED, Arc::new(AtomicBool::new(true))));
    clob.deadline = Duration::from_secs(15);
    let req = format!("GET /auth/derive-api-key HTTP/1.1\r\nHost: {HOST}\r\n{}\r\n", s.l1_headers(now_s()));
    let creds = exec::read_creds(&clob.get(req.as_bytes(), Duration::from_secs(5)).await.unwrap()).expect("creds");
    let w = windows::fetch(windows::window_start(now_s())).await.unwrap().expect("window");
    println!("window {} tick {}", w.slug, w.tick);
    for _ in 0..5 {
        let t0 = clock.now().mono_ns;
        let r = clob.get(format!("GET /time HTTP/1.1\r\nHost: {HOST}\r\n\r\n").as_bytes(), Duration::from_secs(5)).await.unwrap();
        println!("GET /time {} in {:.1} ms", r.status, (clock.now().mono_ns - t0) as f64 / 1e6);
    }
    for i in 0..n {
        let (maker, taker) = sign::amounts(sign::Side::Buy, 1_000_000, w.tick.max(10_000), w.tick).unwrap();
        let ts = clock.now().wall_ns;
        let o = sign::Order { salt: (ts / 1000) & ((1 << 53) - 1), token_id: w.up.clone(), maker_amount: maker, taker_amount: taker, side: sign::Side::Buy, timestamp_ms: ts / 1_000_000 };
        let body = s.body(&o, &creds.key);
        let req = exec::order_request(HOST, &s, &creds, &body, ts / 1_000_000_000).unwrap();
        let mut order = Order::default();
        order.apply(Input::Signed).unwrap();
        let sent = clob.post(&req, &mut order, &clock).await;
        let ms = sent.written.zip(sent.answered).map(|(a, b)| (b.mono_ns - a.mono_ns) as f64 / 1e6);
        let body = sent.raw.map(|r| format!("{} {}", r.status, String::from_utf8_lossy(&r.body).chars().take(160).collect::<String>()));
        println!("order {i}: written→answer {ms:?} ms, {:?}, {body:?}", order.phase);
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
