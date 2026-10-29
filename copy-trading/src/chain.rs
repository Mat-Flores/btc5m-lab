//! Détection : logs `OrderFilled` des échanges CTF V2 sur Polygon (websocket `eth_subscribe`), la source la plus rapide
//! mesurée (étape 1 : 1,77 s médiane après le match, avant RTDS dans 96 % des cas ; couvre le wallet 28/28).
//! Disposition des données vérifiée sur 6 logs réels du wallet rapprochés de data-api /activity (2026-10-28) :
//! topics [sig, orderHash, maker, taker], data [side (0 achat), tokenId, montant maker, montant taker, frais…].
//! Seuls les logs où le wallet est `maker` sont lus : ceux où il est `taker` répètent ses fills côté contrepartie.
use engine_core::clock::Stamp;
use engine_core::event::{BookKind, Caps, Event, SourceId};
use engine_core::feed::{Adapter, DecodeError};
use serde_json::Value;
use std::time::Duration;

pub const ORDER_FILLED: &str = "0xd543adfd945773f1a62f74f0ee55a5e3b9b1a28262980ba90b1a89f2ea84d8ee";
/// Les deux échanges qui émettent ces logs (relevés sur la chaîne) : standard et neg-risk.
pub const EXCHANGES: [&str; 2] = ["0xe111180000d2663c0091e4f400237545b87b996b", "0xe2222d279d744050d28e00520010520000310f59"];

/// Abonnement : les fills où `wallet` est maker, et les en-têtes de blocs (leur horodatage date l'ordre suivi).
pub struct Logs { pub url: String, pub wallet: String }

impl Adapter for Logs {
    fn url(&self) -> String { self.url.clone() }
    // le wallet peut rester muet longtemps : le silence se juge sur les blocs (~2 s), pas sur ses fills
    fn caps(&self) -> Caps { Caps { sequence: false, event_time: false, book: BookKind::None, silence: Duration::from_secs(20) } }
    fn subscribe_all(&self, _: &[String]) -> Vec<String> {
        let maker = format!("0x{:0>64}", self.wallet.trim_start_matches("0x").to_lowercase());
        vec![
            serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "eth_subscribe", "params": ["logs", { "address": EXCHANGES, "topics": [ORDER_FILLED, null, maker] }] }).to_string(),
            serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "eth_subscribe", "params": ["newHeads"] }).to_string(),
        ]
    }
    fn unsubscribe(&self, _: &[String]) -> Vec<String> { vec![] }
    fn heartbeat(&self) -> Option<(Duration, String)> { None }
    // les trames sont lues brutes (`run_with(.., trace: true)`, `Out::Frame`) : rien à décoder en événements
    fn decode(&mut self, _: &str, _: Stamp, _: SourceId, _: &mut Vec<Event>) -> Result<Option<u64>, DecodeError> { Ok(None) }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fill { pub tx: String, pub log_index: u64, pub block: u64, pub token: String, pub buy: bool, pub usd: f64, pub shares: f64 }

#[derive(Debug, PartialEq)]
pub enum Frame { Fill(Fill), Head { block: u64, ts_s: u64 }, Other }

fn hex_u64(v: &Value) -> Option<u64> { u64::from_str_radix(v.as_str()?.trim_start_matches("0x"), 16).ok() }
/// Mot de 32 octets (hex) → décimal, pour un tokenId (jusqu'à 78 chiffres).
fn word_dec(w: &str) -> String {
    let mut digits = vec![0u8]; // petit-boutiste, base 10
    for c in w.chars() {
        let mut carry = c.to_digit(16).unwrap_or(0);
        for d in digits.iter_mut() { let v = *d as u32 * 16 + carry; *d = (v % 10) as u8; carry = v / 10; }
        while carry > 0 { digits.push((carry % 10) as u8); carry /= 10; }
    }
    let s: String = digits.iter().rev().map(|d| (b'0' + d) as char).collect();
    match s.trim_start_matches('0') { "" => "0".into(), t => t.into() }
}
fn word_amount(w: &str) -> Option<f64> { u128::from_str_radix(w, 16).ok().map(|x| x as f64 / 1e6) }

pub fn read(text: &str) -> Frame {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return Frame::Other };
    let r = &v["params"]["result"];
    if r["topics"].is_array() {
        let data = r["data"].as_str().unwrap_or("").trim_start_matches("0x");
        let w = |i: usize| data.get(64 * i..64 * (i + 1));
        let (Some(side), Some(tok), Some(a), Some(b)) = (w(0).and_then(word_amount), w(1), w(2).and_then(word_amount), w(3).and_then(word_amount)) else { return Frame::Other };
        let buy = side == 0.0;
        // achat : il donne des USDC (maker) contre des parts (taker) ; vente : l'inverse
        let (usd, shares) = if buy { (a, b) } else { (b, a) };
        return match (r["transactionHash"].as_str(), hex_u64(&r["logIndex"]), hex_u64(&r["blockNumber"])) {
            (Some(tx), Some(log_index), Some(block)) => Frame::Fill(Fill { tx: tx.to_lowercase(), log_index, block, token: word_dec(tok), buy, usd, shares }),
            _ => Frame::Other,
        };
    }
    match (hex_u64(&r["number"]), hex_u64(&r["timestamp"])) { (Some(block), Some(ts_s)) => Frame::Head { block, ts_s }, _ => Frame::Other }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "18048113644252055228047575277787633022068076468097449060148334743204784347285";

    fn log(side: u8, a: u64, b: u64) -> String {
        let data = format!("{:064x}{}{:064x}{:064x}{}", side, "27e6decd602acaa1b7efe3f3d258f00a1ead2c24c95af099ebe2e7b910759895", a, b, "0".repeat(192));
        serde_json::json!({ "jsonrpc": "2.0", "method": "eth_subscription", "params": { "subscription": "0x1", "result": {
            "address": EXCHANGES[0], "topics": [ORDER_FILLED, "0xaa", "0x000000000000000000000000f0318c32136c2db7fec88b84869aee6a1106c80c", "0x0000000000000000000000002ed88860"],
            "data": format!("0x{data}"), "blockNumber": "0x5be4a71", "transactionHash": "0xE36C", "logIndex": "0x1a" } } }).to_string()
    }

    /// Trame refaite avec les valeurs d'un log réel du wallet : achat de 200 parts pour 100 USDC (data-api : BUY 200 @ 0,5 = 100 $).
    #[test]
    fn real_buy_log_decodes_as_data_api_reports_it() {
        assert_eq!(read(&log(0, 100_000_000, 200_000_000)), Frame::Fill(Fill { tx: "0xe36c".into(), log_index: 26, block: 96_356_977, token: TOKEN.into(), buy: true, usd: 100.0, shares: 200.0 }));
    }

    /// Vente : les montants s'inversent `[non vérifié sur log réel de vente : contrôlé à chaque run par data-api]`.
    #[test]
    fn sell_log_swaps_amounts() {
        let Frame::Fill(f) = read(&log(1, 30_000_000, 15_000_000)) else { panic!() };
        assert_eq!((f.buy, f.usd, f.shares), (false, 15.0, 30.0));
    }

    #[test]
    fn heads_acks_and_garbage() {
        assert_eq!(read(r#"{"jsonrpc":"2.0","method":"eth_subscription","params":{"subscription":"0x2","result":{"number":"0x10","timestamp":"0x6a1b2c3d","hash":"0x1"}}}"#), Frame::Head { block: 16, ts_s: 0x6a1b2c3d });
        assert_eq!(read(r#"{"jsonrpc":"2.0","id":1,"result":"0xabc"}"#), Frame::Other);
        assert_eq!(read("pas du json"), Frame::Other);
        assert_eq!(read(&log(0, 1, 2).replace("\"data\":\"0x", "\"data\":\"0xzz")), Frame::Other);
    }

    #[test]
    fn subscription_targets_the_wallet_as_maker() {
        let s = Logs { url: "wss://x".into(), wallet: "0xF0318c32136C2db7fec88b84869aee6a1106c80c".into() }.subscribe_all(&[]);
        assert!(s[0].contains("0x000000000000000000000000f0318c32136c2db7fec88b84869aee6a1106c80c") && s[0].contains(EXCHANGES[1]));
        assert!(s[1].contains("newHeads"));
    }
}
