//! Polymarket orders, built and signed as the official SDK does (`@polymarket/client` 0.13.0), byte for byte: CTF
//! Exchange V2 order, signed by a Deposit Wallet (signature type 3, ERC-7739 `TypedDataSign` wrapping), the
//! `POST /order` body and its L2 HMAC headers. Checked against the SDK's own output (`fixtures/sign/vectors.json`).
//! Only what the account in use needs: FAK market orders of a Deposit Wallet, on 0.01 and 0.001 price grids.
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig, URL_SAFE};
use base64::engine::{DecodePaddingMode, Engine};
use engine_core::money::{Micro, MICRO};
use hmac::{Hmac, Mac};
use k256::ecdsa::SigningKey;
use sha2::Sha256;
use sha3::{Digest, Keccak256};

pub const CHAIN_ID: u64 = 137;
/// CTF Exchange V2, from the SDK's signed domain.
pub const EXCHANGE: [u8; 20] = [0xe1, 0x11, 0x18, 0x00, 0x00, 0xd2, 0x66, 0x3c, 0x00, 0x91, 0xe4, 0xf4, 0x00, 0x23, 0x75, 0x45, 0xb8, 0x7b, 0x99, 0x6b]; // 0xE111…996B
const ORDER_TYPE: &str = "Order(uint256 salt,address maker,address signer,uint256 tokenId,uint256 makerAmount,uint256 takerAmount,uint8 side,uint8 signatureType,uint256 timestamp,bytes32 metadata,bytes32 builder)";
const SIGNATURE_TYPE: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side { Buy, Sell }

#[derive(Debug, PartialEq, Eq)]
pub enum OrderError { Tick(Micro), OffGrid, Size, Price, Secret }

/// The SDK's amounts for a market order. `size`: pUSD to spend (buy) or shares to sell, 2 decimals. `price`: the limit
/// (max for a buy, min for a sell) on the grid. Returns (makerAmount, takerAmount) in micro-units: the maker side is the
/// size itself, the taker side size / price (buy) or size × price (sell), rounded UP to 4 decimals on a 0.01 grid and
/// 5 on a 0.001 grid (fitted on 80 SDK vectors; rounding to nearest fails 12 of them, down 36).
pub fn amounts(side: Side, size: Micro, price: Micro, tick: Micro) -> Result<(u64, u64), OrderError> {
    let unit: u128 = match tick { 10_000 => 100, 1_000 => 10, t => return Err(OrderError::Tick(t)) };
    if size <= 0 || size % 10_000 != 0 { return Err(OrderError::Size); }
    if price <= 0 || price >= MICRO { return Err(OrderError::Price); }
    if price % tick != 0 { return Err(OrderError::OffGrid); }
    let (s, p, m) = (size as u128, price as u128, MICRO as u128);
    // exact ratio n / d in micro-units, then up to the next multiple of `unit`
    let (n, d) = match side { Side::Buy => (s * m, p), Side::Sell => (s * p, m) };
    let taker = n.div_ceil(d * unit) * unit;
    Ok((size as u64, u64::try_from(taker).map_err(|_| OrderError::Size)?))
}

pub struct Order { pub salt: u64, pub token_id: String, pub maker_amount: u64, pub taker_amount: u64, pub side: Side, pub timestamp_ms: u64 }

/// One wallet's signer: the key (EOA) that controls the Deposit Wallet `wallet`. Every constant is prepared once.
pub struct Signer {
    key: SigningKey,
    /// The EOA, EIP-55 checksummed (header `POLY_ADDRESS`).
    pub address: String,
    /// The Deposit Wallet, as configured (order `maker` and `signer`).
    pub wallet: String,
    wallet_word: [u8; 32],
    exchange_domain: [u8; 32],
    order_typehash: [u8; 32],
    sign_typehash: [u8; 32],
    wallet_name: [u8; 32],
    wallet_version: [u8; 32],
}

impl Signer {
    /// From the decrypted key bytes (`engine_core::key::decrypt_v1`).
    pub fn from_bytes(key: &[u8; 32], wallet: &str) -> Option<Signer> { Signer::new(SigningKey::from_slice(key).ok()?, wallet) }

    /// `wallet`: 0x-prefixed hex address of the Deposit Wallet.
    pub fn new(key: SigningKey, wallet: &str) -> Option<Signer> {
        let w = parse_addr(wallet)?;
        let pubk = key.verifying_key().to_encoded_point(false);
        let mut eoa = [0u8; 20];
        eoa.copy_from_slice(&keccak(&[&pubk.as_bytes()[1..]])[12..]);
        let exchange_domain = keccak(&[&keccak(&[b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"]),
            &keccak(&[b"Polymarket CTF Exchange"]), &keccak(&[b"2"]), &u256(CHAIN_ID as u128), &addr_word(&EXCHANGE)]);
        Some(Signer {
            key, address: checksum(&eoa), wallet: wallet.to_string(), wallet_word: addr_word(&w), exchange_domain,
            order_typehash: keccak(&[ORDER_TYPE.as_bytes()]),
            sign_typehash: keccak(&[format!("TypedDataSign(Order contents,string name,string version,uint256 chainId,address verifyingContract,bytes32 salt){ORDER_TYPE}").as_bytes()]),
            wallet_name: keccak(&[b"DepositWallet"]), wallet_version: keccak(&[b"1"]),
        })
    }

    /// The order's `signature` field: ERC-7739 nested signature, `sig ‖ appDomainSeparator ‖ contentsHash ‖
    /// contentsType ‖ uint16(len(contentsType))`.
    pub fn sign(&self, o: &Order) -> String {
        let contents = self.contents(o);
        let wrapped = keccak(&[&self.sign_typehash, &contents, &self.wallet_name, &self.wallet_version, &u256(CHAIN_ID as u128), &self.wallet_word, &[0u8; 32]]);
        let digest = keccak(&[&[0x19, 0x01], &self.exchange_domain, &wrapped]);
        let (sig, rec) = self.key.sign_prehash_recoverable(&digest).expect("signing a 32-byte digest cannot fail");
        let mut out = Vec::with_capacity(65 + 64 + ORDER_TYPE.len() + 2);
        out.extend_from_slice(&sig.to_bytes());
        out.push(rec.to_byte() + 27);
        out.extend_from_slice(&self.exchange_domain);
        out.extend_from_slice(&contents);
        out.extend_from_slice(ORDER_TYPE.as_bytes());
        out.extend_from_slice(&(ORDER_TYPE.len() as u16).to_be_bytes());
        format!("0x{}", hex::encode(out))
    }

    /// L1 headers that create the API credentials (`POST /auth/api-key`): the EOA signs `ClobAuth` (EIP-712).
    pub fn l1_headers(&self, ts_s: u64) -> String {
        const MSG: &str = "This message attests that I control the given wallet";
        let domain = keccak(&[&keccak(&[b"EIP712Domain(string name,string version,uint256 chainId)"]), &keccak(&[b"ClobAuthDomain"]), &keccak(&[b"1"]), &u256(CHAIN_ID as u128)]);
        let eoa = parse_addr(&self.address.to_lowercase()).expect("own address");
        let msg = keccak(&[&keccak(&[b"ClobAuth(address address,string timestamp,uint256 nonce,string message)"]), &addr_word(&eoa), &keccak(&[ts_s.to_string().as_bytes()]), &u256(0), &keccak(&[MSG.as_bytes()])]);
        let (sig, rec) = self.key.sign_prehash_recoverable(&keccak(&[&[0x19, 0x01], &domain, &msg])).expect("signing a 32-byte digest cannot fail");
        format!("POLY_ADDRESS: {}\r\nPOLY_SIGNATURE: 0x{}{:02x}\r\nPOLY_TIMESTAMP: {ts_s}\r\nPOLY_NONCE: 0\r\n", self.address, hex::encode(sig.to_bytes()), rec.to_byte() + 27)
    }

    /// EIP-712 struct hash of the order (the ERC-7739 `contentsHash`).
    fn contents(&self, o: &Order) -> [u8; 32] {
        keccak(&[&self.order_typehash, &u256(o.salt as u128), &self.wallet_word, &self.wallet_word, &dec_word(&o.token_id),
            &u256(o.maker_amount as u128), &u256(o.taker_amount as u128), &u256(matches!(o.side, Side::Sell) as u128),
            &u256(SIGNATURE_TYPE as u128), &u256(o.timestamp_ms as u128), &[0u8; 32], &[0u8; 32]])
    }

    /// The order's EIP-712 hash on the exchange domain: the id to query when no answer gave one. That the CLOB's
    /// `orderID` is this hash is `[non vérifié]` until a real order (phase 10); a miss leaves the order unknown.
    pub fn order_hash(&self, o: &Order) -> String { format!("0x{}", hex::encode(keccak(&[&[0x19, 0x01], &self.exchange_domain, &self.contents(o)]))) }

    /// The `POST /order` body of a FAK order, field for field as the SDK writes it.
    pub fn body(&self, o: &Order, owner: &str) -> String { self.body_typed(o, owner, "FAK") }

    /// The same body for another order type (`GTC`: a resting limit order) `[GTC non vérifié sur ordre réel]`.
    pub fn body_typed(&self, o: &Order, owner: &str, order_type: &str) -> String {
        let z = "0x0000000000000000000000000000000000000000000000000000000000000000";
        format!(r#"{{"deferExec":false,"order":{{"builder":"{z}","expiration":"0","maker":"{w}","makerAmount":"{}","metadata":"{z}","salt":{},"side":"{}","signature":"{}","signatureType":{SIGNATURE_TYPE},"signer":"{w}","takerAmount":"{}","timestamp":"{}","tokenId":"{}"}},"orderType":"{order_type}","owner":"{owner}"}}"#,
            o.maker_amount, o.salt, if o.side == Side::Buy { "BUY" } else { "SELL" }, self.sign(o), o.taker_amount, o.timestamp_ms, o.token_id, w = self.wallet)
    }
}

/// `POLY_SIGNATURE` of an L2 request: HMAC-SHA256 of `ts ‖ method ‖ path ‖ body` under the API secret, URL-safe
/// base64. The secret is decoded as the SDK does (URL-safe or standard alphabet, other characters dropped).
pub fn l2_signature(secret: &str, ts_s: u64, method: &str, path: &str, body: &str) -> Result<String, OrderError> {
    let clean: String = secret.chars().map(|c| match c { '-' => '+', '_' => '/', c => c }).filter(|c| c.is_ascii_alphanumeric() || "+/=".contains(*c)).collect();
    let lenient = GeneralPurpose::new(&base64::alphabet::STANDARD, GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent));
    let key = lenient.decode(clean).map_err(|_| OrderError::Secret)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).map_err(|_| OrderError::Secret)?;
    mac.update(format!("{ts_s}{method}{path}").as_bytes());
    mac.update(body.as_bytes());
    Ok(URL_SAFE.encode(mac.finalize().into_bytes()))
}

fn keccak(parts: &[&[u8]]) -> [u8; 32] { let mut h = Keccak256::new(); for p in parts { h.update(p); } h.finalize().into() }
fn u256(n: u128) -> [u8; 32] { let mut b = [0u8; 32]; b[16..].copy_from_slice(&n.to_be_bytes()); b }
fn addr_word(a: &[u8; 20]) -> [u8; 32] { let mut b = [0u8; 32]; b[12..].copy_from_slice(a); b }
/// Decimal string (a token id, up to 78 digits) to a big-endian uint256.
fn dec_word(s: &str) -> [u8; 32] {
    let mut b = [0u8; 32];
    for c in s.bytes() { let mut carry = (c - b'0') as u32; for x in b.iter_mut().rev() { let v = *x as u32 * 10 + carry; *x = v as u8; carry = v >> 8; } }
    b
}
fn parse_addr(s: &str) -> Option<[u8; 20]> { let v = hex::decode(s.strip_prefix("0x")?).ok()?; v.try_into().ok() }
/// EIP-55 mixed-case checksum.
fn checksum(a: &[u8; 20]) -> String {
    let h = hex::encode(a);
    let k = keccak(&[h.as_bytes()]);
    let mixed: String = h.chars().enumerate().map(|(i, c)| if c.is_ascii_alphabetic() && (k[i / 2] >> (if i % 2 == 0 { 4 } else { 0 })) & 0xf >= 8 { c.to_ascii_uppercase() } else { c }).collect();
    format!("0x{mixed}")
}
#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::money::parse_micro;

    fn vectors() -> serde_json::Value {
        let path = format!("{}/../../fixtures/sign/vectors.json", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }
    fn signer(v: &serde_json::Value) -> Signer {
        let key = SigningKey::from_slice(&hex::decode(v["key"].as_str().unwrap().trim_start_matches("0x")).unwrap()).unwrap();
        Signer::new(key, v["wallet"].as_str().unwrap()).unwrap()
    }

    /// Every SDK order is rebuilt identically: amounts, signature, body, HMAC header, signer address.
    #[test]
    fn sdk_vectors_are_reproduced_byte_for_byte() {
        let v = vectors();
        let s = signer(&v);
        assert_eq!(s.address, v["signer"].as_str().unwrap());
        let cases = v["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 80);
        for c in cases {
            let m = &c["typed"]["message"]["contents"];
            let side = if c["side"] == "BUY" { Side::Buy } else { Side::Sell };
            let tick = parse_micro(&c["tick"].to_string()).unwrap();
            let (maker, taker) = amounts(side, parse_micro(c["size"].as_str().unwrap()).unwrap(), parse_micro(&c["price"].to_string()).unwrap(), tick).unwrap();
            assert_eq!((maker.to_string(), taker.to_string()), (m["makerAmount"].as_str().unwrap().to_string(), m["takerAmount"].as_str().unwrap().to_string()), "{c}");
            let o = Order { salt: m["salt"].as_str().unwrap().parse().unwrap(), token_id: m["tokenId"].as_str().unwrap().into(), maker_amount: maker, taker_amount: taker, side, timestamp_ms: m["timestamp"].as_str().unwrap().parse().unwrap() };
            assert_eq!(s.sign(&o), c["signature"].as_str().unwrap());
            let body = s.body(&o, v["creds"]["apiKey"].as_str().unwrap());
            let req = &c["request"];
            assert_eq!(body, req["body"].as_str().unwrap());
            let ts: u64 = req["headers"]["poly_timestamp"].as_str().unwrap().parse().unwrap();
            assert_eq!(l2_signature(v["creds"]["secret"].as_str().unwrap(), ts, "POST", "/order", &body).unwrap(), req["headers"]["poly_signature"].as_str().unwrap());
            assert_eq!(req["headers"]["poly_address"].as_str().unwrap(), s.address);
        }
    }

    #[test]
    fn l1_auth_headers_match_the_sdk() {
        let v = vectors();
        let h = &v["auth"]["request"]["headers"];
        let ts: u64 = h["poly_timestamp"].as_str().unwrap().parse().unwrap();
        let want = format!("POLY_ADDRESS: {}\r\nPOLY_SIGNATURE: {}\r\nPOLY_TIMESTAMP: {ts}\r\nPOLY_NONCE: 0\r\n", h["poly_address"].as_str().unwrap(), h["poly_signature"].as_str().unwrap());
        assert_eq!(signer(&v).l1_headers(ts), want);
    }

    #[test]
    fn amounts_refuse_what_the_sdk_would_not_build() {
        assert_eq!(amounts(Side::Buy, 5_000_000, 555_000, 1_000), Ok((5_000_000, 9_009_010)));
        assert_eq!(amounts(Side::Buy, 5_000_000, 555_000, 10_000), Err(OrderError::OffGrid));
        assert_eq!(amounts(Side::Buy, 5_000_000, 550_000, 100), Err(OrderError::Tick(100)));
        assert_eq!(amounts(Side::Buy, 5_001_000, 550_000, 10_000), Err(OrderError::Size));
        assert_eq!(amounts(Side::Sell, 0, 550_000, 10_000), Err(OrderError::Size));
        assert_eq!(amounts(Side::Sell, 5_000_000, MICRO, 10_000), Err(OrderError::Price));
        assert_eq!(amounts(Side::Sell, 5_000_000, 0, 10_000), Err(OrderError::Price));
    }

    #[test]
    fn checksum_matches_eip55_examples() {
        // examples from the EIP-55 specification
        for a in ["0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed", "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359", "0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB"] {
            assert_eq!(checksum(&parse_addr(&a.to_lowercase()).unwrap()), a);
        }
    }
}
