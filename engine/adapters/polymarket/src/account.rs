//! Account reads at Polymarket, on the cold thread only: positions and redemptions (data-api, public by address),
//! the pUSD balance and the open orders (CLOB, L2-authenticated). Formats from the official SDK (`@polymarket/bindings`
//! 0.13: `PositionSchema`, `ActivitySchema`, `BalanceAllowanceResponseSchema`, `OpenOrdersPageSchema`); positions and
//! redemptions checked on real answers (fixtures/raw/data-api-*.json, 2026-10-20), the CLOB reads `[non vérifié sur
//! réponse réelle]` (they need the account's credentials). data-api writes its figures as JSON numbers.
use crate::exec::{get_request, Creds};
use crate::sign::{OrderError, Signer};
use engine_core::http::{self, Response};
use engine_core::money::{parse_micro_rounded, scaled_int_to_micro, Micro};
use serde_json::Value;
use std::time::Duration;

pub const DATA: &str = "https://data-api.polymarket.com";

#[derive(Clone, Debug, PartialEq)]
pub struct Holding {
    pub token: String,
    pub market: String,
    /// 0 for a closed row (the venue leaves a ~0 residual there).
    pub shares: Micro,
    /// Entry fees the venue attributes to the token, in pUSD.
    pub fees: Micro,
    /// The venue's own value of the holding (`current_value`), compared with ours.
    pub value: Micro,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Redemption { pub market: String, pub amount: Micro, pub tx: String }

fn dec(v: &Value, k: &str) -> Result<Micro, String> {
    match &v[k] { Value::Number(n) => parse_micro_rounded(&n.to_string()), Value::String(s) => parse_micro_rounded(s), _ => return Err(format!("missing {k}")) }.map_err(|e| format!("{k}: {e:?}"))
}
fn text(v: &Value, k: &str) -> Result<String, String> { v[k].as_str().filter(|s| !s.is_empty()).map(String::from).ok_or_else(|| format!("missing {k}")) }

/// One page of data-api: its rows and the next cursor.
fn page(body: &[u8]) -> Result<(Vec<Value>, Option<String>), String> {
    let mut v: Value = serde_json::from_slice(body).map_err(|e| format!("json: {e}"))?;
    let next = v["pagination"]["next_cursor"].as_str().map(String::from);
    match v["data"].take() { Value::Array(rows) => Ok((rows, next)), _ => Err("no data".into()) }
}

pub fn read_holdings(body: &[u8]) -> Result<(Vec<Holding>, Option<String>), String> {
    let (rows, next) = page(body)?;
    let out = rows.iter().map(|r| Ok(Holding {
        token: text(r, "token_id")?, market: text(r, "condition_id")?,
        shares: if r["status"] == "CLOSED" { 0 } else { dec(r, "current_size")? },
        fees: dec(r, "entry_fees_usdc")?, value: dec(r, "current_value")?,
    })).collect::<Result<_, String>>()?;
    Ok((out, next))
}

pub fn read_redemptions(body: &[u8]) -> Result<(Vec<Redemption>, Option<String>), String> {
    let (rows, next) = page(body)?;
    let out = rows.iter().filter(|r| r["type"] == "REDEEM")
        .map(|r| Ok(Redemption { market: text(r, "condition_id")?, amount: dec(r, "usdc_size")?, tx: text(r, "transaction_hash")? }))
        .collect::<Result<_, String>>()?;
    Ok((out, next))
}

/// `max_pages`: a cursor that never ends would loop; an account holds far fewer rows than that many pages.
async fn all<T>(url: impl Fn(Option<&str>) -> String, read: fn(&[u8]) -> Result<(Vec<T>, Option<String>), String>, timeout: Duration, max_pages: usize) -> Result<Vec<T>, String> {
    let (mut out, mut cursor) = (vec![], None);
    for _ in 0..max_pages {
        let body = http::get(&url(cursor.as_deref()), timeout).await.map_err(|e| format!("data-api: {e:?}"))?;
        let (rows, next) = read(&body)?;
        out.extend(rows);
        match next { Some(c) => cursor = Some(c), None => return Ok(out) }
    }
    Err("too many pages".into())
}

fn cursor(c: Option<&str>) -> String { c.map(|c| format!("&cursor={c}")).unwrap_or_default() }

/// Every holding of the address (`user`: the wallet that holds the shares).
pub async fn holdings(user: &str, timeout: Duration, max_pages: usize) -> Result<Vec<Holding>, String> { all(|c| format!("{DATA}/v2/positions?user={user}&limit=500{}", cursor(c)), read_holdings, timeout, max_pages).await }

/// Every redemption of the address.
pub async fn redemptions(user: &str, timeout: Duration, max_pages: usize) -> Result<Vec<Redemption>, String> { all(|c| format!("{DATA}/v2/activity?user={user}&type=REDEEM&limit=500{}", cursor(c)), read_redemptions, timeout, max_pages).await }

/// `GET /balance-allowance` for the collateral of a Deposit Wallet (signature type 3), as the SDK asks it.
pub fn balance_request(host: &str, s: &Signer, c: &Creds, ts_s: u64) -> Result<Vec<u8>, OrderError> {
    get_request(host, s, c, "/balance-allowance", "?asset_type=COLLATERAL&signature_type=3", ts_s)
}

/// The balance, in base units of a 6-decimal token.
pub fn read_balance(r: &Response) -> Option<Micro> {
    if r.status != 200 { return None; }
    let v: Value = serde_json::from_slice(&r.body).ok()?;
    scaled_int_to_micro(v["balance"].as_str()?, 6).ok()
}

/// `GET /data/orders`, one page.
pub fn open_orders_request(host: &str, s: &Signer, c: &Creds, cursor: Option<&str>, ts_s: u64) -> Result<Vec<u8>, OrderError> {
    get_request(host, s, c, "/data/orders", &cursor.map(|c| format!("?next_cursor={c}")).unwrap_or_default(), ts_s)
}

/// Order ids of one page, and the next cursor ("LTE=" marks the end).
pub fn read_open_orders(r: &Response) -> Option<(Vec<String>, Option<String>)> {
    if r.status != 200 { return None; }
    let v: Value = serde_json::from_slice(&r.body).ok()?;
    let ids = v["data"].as_array()?.iter().map(|o| o["id"].as_str().map(String::from)).collect::<Option<Vec<_>>>()?;
    Some((ids, v["next_cursor"].as_str().filter(|c| *c != "LTE=").map(String::from)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> { std::fs::read(format!("{}/../../fixtures/raw/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap() }
    fn r(status: u16, body: &str) -> Response { Response { status, body: body.as_bytes().to_vec(), retry_after_s: None, close: false } }

    #[test]
    fn real_holdings_and_redemptions_decode() {
        let (h, next) = read_holdings(&fixture("data-api-positions.json")).unwrap();
        assert_eq!(next, None);
        assert_eq!(h[0].shares, 2_905_930_000);
        assert_eq!((h[0].fees, h[0].value), (687_935, 4_358_800));
        assert!(h[0].market.starts_with("0xefa1"));
        let (rd, next) = read_redemptions(&fixture("data-api-activity-redeem.json")).unwrap();
        assert!(next.is_some());
        // a losing redemption pays 0 and still closes the market
        assert_eq!(rd.iter().map(|x| x.amount).collect::<Vec<_>>(), [0, 130_000_000, 0]);
        assert!(rd[1].tx.starts_with("0x43c4"));
    }

    #[test]
    fn closed_rows_hold_nothing_and_bad_rows_refuse_the_page() {
        let closed = br#"{"data":[{"token_id":"1","condition_id":"0xc","current_size":0.000001,"entry_fees_usdc":0,"current_value":0,"status":"CLOSED"}],"pagination":{"next_cursor":null}}"#;
        assert_eq!(read_holdings(closed).unwrap().0[0].shares, 0);
        assert!(read_holdings(br#"{"data":[{"token_id":"1","current_size":1}]}"#).is_err());
        assert!(read_holdings(b"not json").is_err());
    }

    #[test]
    fn balance_and_open_orders_as_the_sdk_reads_them() {
        assert_eq!(read_balance(&r(200, r#"{"balance":"44825000","allowances":{}}"#)), Some(44_825_000));
        assert_eq!(read_balance(&r(401, "")), None);
        assert_eq!(read_open_orders(&r(200, r#"{"count":1,"data":[{"id":"0xabc"}],"next_cursor":"LTE="}"#)), Some((vec!["0xabc".into()], None)));
        assert_eq!(read_open_orders(&r(200, r#"{"data":[{"nope":1}],"next_cursor":"LTE="}"#)), None);
        let s = Signer::from_bytes(&[0x11; 32], "0x7a57144fafde87f5ce4a880d570bae58eebc83f2").unwrap();
        let c = Creds { key: "k".into(), secret: "c2VjcmV0".into(), passphrase: "p".into() };
        let req = String::from_utf8(balance_request("clob.polymarket.com", &s, &c, 1).unwrap()).unwrap();
        assert!(req.starts_with("GET /balance-allowance?asset_type=COLLATERAL&signature_type=3 HTTP/1.1\r\n"));
    }
}
