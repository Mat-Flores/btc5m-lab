//! copy-trading : suit UN wallet public Polymarket et recopie ses trades (spec : copy-trading/SPEC.md).
//!   copy-trading run <config.json>      paper ou live selon la config ; en live, phrase secrète sur la 1re ligne de stdin
//!   copy-trading encrypt <adresse>      clé (1re ligne) + phrase (2e ligne) sur stdin → `<adresse>:v1.…` sur stdout
//!   copy-trading report <journal.jsonl> synthèse ≤ 15 lignes
mod book;
mod chain;
mod maker;
mod pnl;
mod report;
mod sizing;
mod strategy;

use adapter_polymarket::exec::{self, Clob, ALLOWED, HOST};
use adapter_polymarket::market::Market;
use adapter_polymarket::sign::{self, Side, Signer};
use engine_app::arm::Armed;
use engine_core::clock::Clock;
use engine_core::event::Payload;
use engine_core::feed::{self, Cmd, Out, Timing};
use engine_core::guard::NetGuard;
use engine_core::http::{self, dial, Conn};
use engine_core::order::{Input, Order, Phase};
use pnl::Account;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

const DATA: &str = "https://data-api.polymarket.com";
const CLOB: &str = "https://clob.polymarket.com";
/// pUSD, le collatéral des échanges V2 (symbol « pUSD », 6 décimales, lu sur la chaîne le 2026-10-28).
const PUSD: &str = "0xc011a7e12a19f7b1f670d46f03b03f3342e82dfb";
const T: Duration = Duration::from_secs(10);

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    /// Wallet suivi (proxy wallet 0x…).
    wallet: String,
    /// `paper` (défaut) ou `live`.
    #[serde(default = "paper")]
    mode: String,
    /// Fichier `<adresse>:v1.…` (chmod 600), lu en live seulement.
    wallet_file: String,
    journal: String,
    /// Paliers [seuil de p, m] croissants : m = part de mon portefeuille quand sa mise atteint ce seuil de son portefeuille.
    tiers: Vec<(f64, f64)>,
    /// Capital de départ du compte paper ($).
    paper_cash_usd: f64,
    /// Plafond d'un ordre ($).
    max_order_usd: f64,
    /// Écart maximal du prix limite au meilleur prix observé (0.10 = 10 %).
    max_slippage: f64,
    rpc_ws: String,
    rpc_http: String,
    /// Stratégies interchangeables (strategy.rs) ; absent : maker, achat au cumul, ventes par lots, annulation à 6 h.
    #[serde(default)]
    strategy: strategy::StrategyCfg,
}
fn paper() -> String {
    "paper".into()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match (args.get(1).map(String::as_str), args.get(2)) {
        (Some("run"), Some(cfg)) => run(cfg),
        (Some("encrypt"), Some(addr)) => encrypt(addr),
        (Some("report"), Some(j)) => println!(
            "{}",
            report::report(&std::fs::read_to_string(j).expect("journal illisible"))
        ),
        _ => {
            eprintln!("usage : copy-trading run <config.json> | encrypt <adresse> | report <journal.jsonl>");
            std::process::exit(2);
        }
    }
}

// ---- clé : chiffrement local, jamais en clair ailleurs qu'en mémoire --------------------------------------------------

/// Efface un tampon de façon que le compilateur ne puisse pas l'omettre.
fn wipe(b: &mut [u8]) {
    for x in b.iter_mut() {
        unsafe { std::ptr::write_volatile(x, 0) }
    }
}
fn urandom<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .expect("/dev/urandom");
    b
}

fn encrypt(addr: &str) {
    let ok = addr.len() == 42
        && addr.starts_with("0x")
        && addr[2..].bytes().all(|b| b.is_ascii_hexdigit());
    if !ok {
        eprintln!("adresse attendue : 0x + 40 hex (le Deposit Wallet)");
        std::process::exit(2);
    }
    let mut input = String::with_capacity(4096);
    std::io::stdin().read_to_string(&mut input).expect("stdin");
    let mut lines = input.lines();
    let (Some(k), Some(pass)) = (lines.next(), lines.next()) else {
        eprintln!("stdin : clé sur la 1re ligne, phrase sur la 2e");
        std::process::exit(2)
    };
    let hex = k.trim().trim_start_matches("0x");
    let mut key = [0u8; 32];
    let parsed = hex.len() == 64
        && (0..32).all(|i| {
            u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)
                .map(|b| key[i] = b)
                .is_ok()
        });
    let out = parsed
        .then(|| engine_core::key::encrypt_v1(&key, pass, &urandom::<16>(), &urandom::<12>()));
    wipe(&mut key);
    // SAFETY: on n'écrit que des zéros, l'UTF-8 reste valide
    wipe(unsafe { input.as_bytes_mut() });
    match out {
        Some(Ok(p)) => println!("{addr}:{p}"),
        _ => {
            eprintln!("clé attendue : 64 hex, avec ou sans 0x");
            std::process::exit(2);
        }
    }
}

// ---- lectures publiques ---------------------------------------------------------------------------------------------

async fn get_json(url: &str) -> Result<Value, String> {
    let (s, b) = http::get_any(url, T).await.map_err(|e| format!("{e:?}"))?;
    if s != 200 {
        return Err(format!("HTTP {s}"));
    }
    serde_json::from_slice(&b).map_err(|e| format!("json: {e}"))
}

/// Un appel JSON-RPC (POST) au nœud Polygon.
async fn rpc(url: &str, method: &str, params: Value) -> Result<Value, String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or("rpc_http doit être https://")?;
    let (host, path) = rest
        .split_once('/')
        .map_or((rest, "/".to_string()), |(h, p)| (h, format!("/{p}")));
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    let req = format!("POST {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: copy-trading\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
    let r = tokio::time::timeout(T, async {
        let mut c = Conn::new(dial(host, 443, true).await?);
        c.write(req.as_bytes()).await?;
        c.read().await
    })
    .await
    .map_err(|_| "rpc: délai".to_string())?
    .map_err(|e| format!("rpc: {e:?}"))?;
    let v: Value = serde_json::from_slice(&r.body).map_err(|e| format!("rpc json: {e}"))?;
    v.get("result")
        .cloned()
        .ok_or_else(|| format!("rpc: {}", v["error"]))
}

/// Valeur totale du wallet suivi : positions au prix du marché (data-api /value) + cash pUSD (balanceOf).
async fn his_value(cfg: &Config) -> Result<f64, String> {
    let pos = get_json(&format!("{DATA}/value?user={}", cfg.wallet)).await?[0]["value"]
        .as_f64()
        .ok_or("value: format")?;
    let call = json!([{ "to": PUSD, "data": format!("0x70a08231{:0>64}", cfg.wallet.trim_start_matches("0x")) }, "latest"]);
    let cash = rpc(&cfg.rpc_http, "eth_call", call).await?;
    let cash = u128::from_str_radix(
        cash.as_str()
            .ok_or("balanceOf: format")?
            .trim_start_matches("0x"),
        16,
    )
    .map_err(|e| format!("balanceOf: {e}"))? as f64
        / 1e6;
    Ok(pos + cash)
}

// ---- ordres réels -----------------------------------------------------------------------------------------------------

/// La requête `POST /order` signée d'un ordre FAK : achat de `size` $ ou vente de `size` parts, au prix limite `limit`.
fn order_request(
    s: &Signer,
    c: &exec::Creds,
    token: &str,
    buy: bool,
    size: f64,
    limit: f64,
    tick: f64,
    salt: u64,
    now_ms: u64,
    order_type: &str,
) -> Result<Vec<u8>, String> {
    let micro = |x: f64| (x * 1e6).round() as i64;
    let size = ((size * 100.0 + 1e-9).floor() as i64) * 10_000; // au cent (ou centième de part) inférieur
    let side = if buy { Side::Buy } else { Side::Sell };
    let (maker_amount, taker_amount) = sign::amounts(side, size, micro(limit), micro(tick))
        .map_err(|e| format!("montants : {e:?}"))?;
    let o = sign::Order {
        salt,
        token_id: token.into(),
        maker_amount,
        taker_amount,
        side,
        timestamp_ms: now_ms,
    };
    exec::order_request(HOST, s, c, &s.body_typed(&o, &c.key, order_type), now_ms / 1000)
        .map_err(|e| format!("signature : {e:?}"))
}

struct Live {
    armed: Armed,
    clob: Clob,
}

/// Ce que devient un ordre réel : (parts, $) remplis, et les instants d'envoi, d'accusé et de fill (ms).
struct Sent {
    shares: f64,
    usd: f64,
    sent: Option<u64>,
    ack: Option<u64>,
    fill: Option<u64>,
    phase: Phase,
    raw: String,
}

async fn send(
    l: &mut Live,
    clock: &Clock,
    token: &str,
    buy: bool,
    size: f64,
    limit: f64,
    tick: f64,
) -> Result<Sent, String> {
    let now_ms = clock.now().wall_ns / 1_000_000;
    let salt = u64::from_le_bytes(urandom::<8>()) >> 11; // ≤ 2^53, comme les sels du SDK
    let req = order_request(
        &l.armed.signer,
        &l.armed.creds,
        token,
        buy,
        size,
        limit,
        tick,
        salt,
        now_ms,
        "FAK",
    )?;
    let mut o = Order::default();
    o.apply(Input::Signed).map_err(|e| format!("{e:?}"))?;
    let s = l.clob.post(&req, &mut o, clock).await;
    let ms = |x: Option<engine_core::clock::Stamp>| x.map(|s| s.wall_ns / 1_000_000);
    let raw = s
        .raw
        .as_ref()
        .map(|r| String::from_utf8_lossy(&r.body).into_owned())
        .unwrap_or_default();
    // un FAK est rempli au moment où le CLOB l'apparie : l'accusé « matched » date le fill
    let (mut shares, mut usd) = (0.0, 0.0);
    if o.phase == Phase::Matched {
        let v: Value = serde_json::from_str(&raw).unwrap_or_default();
        let f = |k: &str| {
            v[k].as_str()
                .and_then(|x| x.parse::<f64>().ok())
                .unwrap_or(0.0)
        };
        // `[non vérifié sur ordre réel de copy-trading]` : achat, making = $ donnés, taking = parts reçues ; vente, l'inverse
        (shares, usd) = if buy {
            (f("takingAmount"), f("makingAmount"))
        } else {
            (f("makingAmount"), f("takingAmount"))
        };
    }
    let matched = o.phase == Phase::Matched;
    Ok(Sent {
        shares,
        usd,
        sent: ms(s.written),
        ack: ms(s.answered),
        fill: if matched { ms(s.answered) } else { None },
        phase: o.phase,
        raw,
    })
}

// ---- le bot -----------------------------------------------------------------------------------------------------------

struct Bot {
    cfg: Config,
    journal: std::fs::File,
    clock: Arc<Clock>,
    his_value: f64,
    his_pos: HashMap<String, f64>,
    obs: Account,
    dem: Account,
    real: Account,
    marks: BTreeMap<String, f64>,
    market_of: HashMap<String, String>,
    /// Heure du match CLOB (ms) par hash de transaction, lue sur le websocket market : elle date l'ordre suivi.
    matched: HashMap<String, u64>,
    /// Tokens suivis sur le websocket market, et de quoi en ajouter.
    subscribed: HashSet<String>,
    market_cmd: tokio::sync::mpsc::UnboundedSender<Cmd>,
    /// Relance du flux on-chain : publicnode peut couper l'abonnement aux logs sans couper les en-têtes de blocs
    /// (vu le 2026-10-29 : 4 h 20 sans log, flux « Valid »).
    chain_cmd: tokio::sync::mpsc::UnboundedSender<Cmd>,
    last_resync_ms: u64,
    /// Txs rattrapées par data-api : la chaîne ne doit plus les traiter si elle les livre en retard.
    recovered: HashSet<String>,
    seen_logs: HashSet<(String, u64)>,
    /// Fills de la chaîne par tx : (token, achat, parts), pour le contrôle par data-api.
    by_tx: HashMap<String, (String, bool, f64)>,
    checked: HashSet<String>,
    /// Tokens où j'ai déjà pris mon entrée (une seule par token).
    entered: HashSet<String>,
    start_ms: u64,
    live: Option<Live>,
    strat: strategy::Strategy,
    /// Mes ordres posés (maker) et le prochain identifiant local.
    rests: Vec<maker::Rest>,
    next_id: u64,
    /// Ses ordres déduits de ses remplissages, et ceux des miens annulés faute d'activité : (annulé à, son dernier fill).
    his_orders: HashMap<maker::Key, maker::HisOrder>,
    cancelled: HashMap<maker::Key, (u64, u64)>,
    /// Par token : son 1er achat et son cumul d'achats ($), la part de sa position qu'il garde depuis mon entrée.
    his_first: HashMap<String, f64>,
    his_cum: HashMap<String, f64>,
    his_kept: HashMap<String, f64>,
    /// Par token, compte réel : $ et parts achetés, parts vendues depuis l'entrée.
    bought: HashMap<String, (f64, f64)>,
    sold: HashMap<String, f64>,
    /// Trades publics déjà appliqués à mes ordres posés (paper), par hash de transaction.
    applied: HashSet<String>,
    side_checks: u32,
}

fn now_ms(c: &Clock) -> u64 {
    c.now().wall_ns / 1_000_000
}

impl Bot {
    fn done(&mut self, mut line: Value, lat: Value) {
        line["lat"] = lat;
        self.log(line)
    }

    fn log(&mut self, mut v: Value) {
        v["at"] = json!(now_ms(&self.clock));
        let _ = writeln!(self.journal, "{v}");
        let _ = self.journal.flush();
    }

    async fn book(&self, token: &str) -> Result<book::Book, String> {
        let (s, b) = http::get_any(&format!("{CLOB}/book?token_id={token}"), T)
            .await
            .map_err(|e| format!("{e:?}"))?;
        if s != 200 {
            return Err(format!("HTTP {s}"));
        }
        book::parse(&b)
    }

    async fn on_fill(&mut self, f: chain::Fill, src: &str, obs_ms: u64) {
        // un token encore jamais vu : son premier match est déjà passé (non mesuré), les suivants seront datés
        if self.subscribed.insert(f.token.clone()) {
            let _ = self.market_cmd.send(Cmd::Subscribe(vec![f.token.clone()]));
        }
        let src_ms = self.matched.get(&f.tx).copied();
        let his_before = self.his_pos.get(&f.token).copied().unwrap_or(0.0);
        *self.his_pos.entry(f.token.clone()).or_default() +=
            if f.buy { f.shares } else { -f.shares };
        let his_price = if f.shares > 0.0 {
            f.usd / f.shares
        } else {
            0.0
        };
        let mut line = json!({ "t": "trade", "src": src, "tx": f.tx, "token": f.token, "side": if f.buy { "BUY" } else { "SELL" },
            "his_usd": f.usd, "his_shares": f.shares, "his_price": his_price, "his_value": self.his_value });
        let bk = self.book(&f.token).await;
        let dec_ms = now_ms(&self.clock);
        let mut lat = json!({ "src": src_ms, "obs": obs_ms, "dec": dec_ms, "sent": null, "ack": null, "fill": null });
        let bk = match bk {
            Ok(b) => b,
            Err(e) => {
                line["decision"] = json!("skip");
                line["reason"] = json!(format!("carnet illisible : {e}"));
                return self.done(line, lat);
            }
        };
        line["book_ts"] = json!(bk.ts_ms);
        self.market_of.insert(f.token.clone(), bk.market.clone());
        if let (Some(b), Some(a)) = (bk.bids.first(), bk.asks.first()) {
            self.marks.insert(f.token.clone(), (b.0 + a.0) / 2.0);
        }
        let skip = |line: &mut Value, why: &str| {
            line["decision"] = json!("skip");
            line["reason"] = json!(why);
        };
        let price = ((his_price / bk.tick).round() * bk.tick * 1e6).round() / 1e6;
        self.his_order_stats(&f, price, &mut line);
        if self.live.is_some() && bk.neg_risk {
            // shortcut: sign.rs ne signe que pour l'échange standard ; le domaine neg-risk demande d'étendre Signer (accord requis)
            skip(
                &mut line,
                "neg-risk : signature non prise en charge en live",
            );
        } else if self.strat.maker {
            if self.live.is_none() {
                // son fill est aussi un trade public : il sert d'abord mes ordres déjà posés
                self.on_public_trade(&f.token, price, f.shares, !f.buy, &f.tx);
            }
            self.maker(&f, his_before, his_price, price, &bk, &mut line, &mut lat)
                .await;
        } else if f.buy && self.his_value <= 0.0 {
            skip(&mut line, "valeur du wallet suivi inconnue");
        } else if f.buy && self.entered.contains(&f.token) {
            // choix de l'utilisateur : une seule entrée par token, ses renforts ne sont pas recopiés
            skip(&mut line, "renfort (déjà entré)");
        } else if f.buy {
            let p = f.usd / self.his_value;
            let m = sizing::stake_frac(p, &self.cfg.tiers);
            line["p"] = json!(p);
            line["m"] = json!(m);
            match bk.asks.first() {
                None => skip(&mut line, "aucun ask"),
                Some(_) if m <= 0.0 => skip(&mut line, "sous le premier palier"),
                Some(&(ask, _)) => {
                    let usd = sizing::buy_usd(
                        f.usd,
                        self.his_value,
                        self.real.value(&self.marks).0,
                        self.real.cash,
                        &self.cfg.tiers,
                        bk.min_size * ask,
                        self.cfg.max_order_usd,
                    );
                    if usd <= 0.0 {
                        skip(&mut line, "cash insuffisant pour le minimum");
                        return self.done(line, lat);
                    }
                    line["my_usd"] = json!(usd);
                    self.entered.insert(f.token.clone());
                    let limit = book::buy_limit(ask, self.cfg.max_slippage, bk.tick);
                    line["decision"] = json!("buy");
                    line["asked"] = json!(ask);
                    line["limit"] = json!(limit);
                    self.obs.buy(&f.token, usd / his_price, usd);
                    self.dem.buy(&f.token, usd / ask, usd);
                    let (sh, cost) = self
                        .fill(
                            &f.token, true, usd, limit, bk.tick, &bk, &mut lat, &mut line,
                        )
                        .await;
                    self.real.buy(&f.token, sh, cost);
                }
            }
        } else {
            let mine = sizing::sell_shares(f.shares, his_before, self.real.shares(&f.token));
            line["x"] = json!(if his_before > 0.0 {
                (f.shares / his_before).min(1.0)
            } else {
                0.0
            });
            line["my_shares"] = json!(mine);
            let (o, d) = (
                sizing::sell_shares(f.shares, his_before, self.obs.shares(&f.token)),
                sizing::sell_shares(f.shares, his_before, self.dem.shares(&f.token)),
            );
            if mine + o + d <= 0.0 {
                skip(&mut line, "aucune position à vendre");
            } else {
                line["decision"] = json!("sell");
                self.obs.sell(&f.token, o, o * his_price);
                if let Some(&(bid, _)) = bk.bids.first() {
                    self.dem.sell(&f.token, d, d * bid);
                    line["asked"] = json!(bid);
                }
                match bk.bids.first() {
                    None => line["reason"] = json!("aucun bid"),
                    Some(_) if mine < bk.min_size => line["reason"] = json!("réel sous le minimum"),
                    Some(&(bid, _)) => {
                        let limit = book::sell_limit(bid, self.cfg.max_slippage, bk.tick);
                        line["limit"] = json!(limit);
                        let (sh, got) = self
                            .fill(
                                &f.token, false, mine, limit, bk.tick, &bk, &mut lat, &mut line,
                            )
                            .await;
                        self.real.sell(&f.token, sh, got);
                    }
                }
            }
        }
        self.done(line, lat);
    }

    /// Statistiques de son ordre déduit (token, sens, prix) ; une annulation de mon côté démentie par un de ses fills est
    /// tracée (`t: stat`) pour l'analyse de sa stratégie.
    fn his_order_stats(&mut self, f: &chain::Fill, price: f64, line: &mut Value) {
        let at = now_ms(&self.clock);
        let k = maker::key(&f.token, f.buy, price);
        let h = self
            .his_orders
            .entry(k.clone())
            .or_insert_with(|| maker::HisOrder::new(at));
        let gap = h.fill(at, f.usd, f.shares);
        line["his_order"] = json!({ "price": price, "fills": h.fills, "usd": h.usd, "shares": h.shares, "age_ms": at - h.first_ms, "gap_ms": gap });
        if let Some((cancelled_at, last_ms)) = self.cancelled.remove(&k) {
            self.log(json!({ "t": "stat", "kind": "annulation à tort", "token": f.token, "side": if f.buy { "BUY" } else { "SELL" },
                "price": price, "cancelled_at": cancelled_at, "idle_before_cancel_ms": cancelled_at - last_ms, "revived_after_ms": at - cancelled_at }));
        }
    }

    fn committed_usd(&self, token: &str) -> f64 {
        self.bought.get(token).map_or(0.0, |b| b.0)
            + self
                .rests
                .iter()
                .filter(|r| r.buy && r.token == token)
                .map(|r| r.left() * r.price)
                .sum::<f64>()
    }

    /// Stratégie maker : je pose à son prix, du côté qu'il a posé ; mes ordres à un autre prix sur ce token et ce sens
    /// sont déplacés (annulés, la stratégie redimensionne la suite).
    #[allow(clippy::too_many_arguments)]
    async fn maker(
        &mut self,
        f: &chain::Fill,
        his_before: f64,
        his_price: f64,
        price: f64,
        bk: &book::Book,
        line: &mut Value,
        lat: &mut Value,
    ) {
        let moved: Vec<u64> = self
            .rests
            .iter()
            .filter(|r| r.token == f.token && r.buy == f.buy && (r.price - price).abs() > 1e-9)
            .map(|r| r.id)
            .collect();
        for id in moved {
            self.cancel(id, "déplacé").await;
        }
        let at = now_ms(&self.clock);
        if f.buy {
            if self.his_value <= 0.0 {
                line["decision"] = json!("skip");
                line["reason"] = json!("valeur du wallet suivi inconnue");
                return;
            }
            let first = *self.his_first.entry(f.token.clone()).or_insert(f.usd);
            let cum = {
                let c = self.his_cum.entry(f.token.clone()).or_default();
                *c += f.usd;
                *c
            };
            let resting_buys: f64 = self.rests.iter().filter(|r| r.buy).map(|r| r.left() * r.price).sum();
            let ctx = strategy::BuyCtx {
                his_first_usd: first,
                his_cum_usd: cum,
                his_value: self.his_value,
                my_value: self.real.value(&self.marks).0,
                // le cash déjà promis à mes achats posés n'est plus disponible
                my_cash: self.real.cash - resting_buys,
                committed_usd: self.committed_usd(&f.token),
                tiers: &self.cfg.tiers,
                min_usd: bk.min_size * price,
                max_usd: self.cfg.max_order_usd,
            };
            let usd = self.strat.buy.add_usd(&ctx);
            line["p_cum"] = json!(cum / self.his_value);
            if usd <= 0.0 {
                let below = sizing::stake_frac(cum / self.his_value, &self.cfg.tiers) <= 0.0;
                line["decision"] = json!("skip");
                line["reason"] = json!(if below { "sous le premier palier" } else { "engagement atteint" });
                return;
            }
            self.his_kept.entry(f.token.clone()).or_insert(1.0);
            self.obs.buy(&f.token, usd / his_price, usd);
            if let Some(&(ask, _)) = bk.asks.first() {
                self.dem.buy(&f.token, usd / ask, usd);
                line["asked"] = json!(ask);
            }
            line["decision"] = json!("buy");
            line["my_usd"] = json!(usd);
            let shares = ((usd / price) * 100.0 + 1e-9).floor() / 100.0;
            self.post(&f.token, true, price, shares, bk, line, lat, at).await;
        } else {
            let frac = if his_before > 0.0 { (f.shares / his_before).min(1.0) } else { 0.0 };
            let kept = self.his_kept.get(&f.token).copied().map(|k| k * (1.0 - frac));
            if let Some(k) = kept {
                self.his_kept.insert(f.token.clone(), k);
            }
            line["x"] = json!(frac);
            let held = self.real.shares(&f.token);
            let listed: f64 = self.rests.iter().filter(|r| !r.buy && r.token == f.token).map(|r| r.left()).sum();
            let (cost, bought) = self.bought.get(&f.token).copied().unwrap_or((0.0, 0.0));
            let ctx = strategy::SellCtx {
                his_sold_frac: 1.0 - kept.unwrap_or(1.0),
                his_price: price,
                my_avg_cost: if bought > 0.0 { cost / bought } else { 0.0 },
                held,
                sold: self.sold.get(&f.token).copied().unwrap_or(0.0),
                listed,
                min_shares: bk.min_size,
            };
            let mine = if held > 0.0 { self.strat.sell.add_shares(&ctx) } else { 0.0 };
            // comptes observé et demandé : la même part de leurs propres parts
            let ratio = if held > 0.0 { mine / held } else { 0.0 };
            let (o, d) = (self.obs.shares(&f.token) * ratio, self.dem.shares(&f.token) * ratio);
            self.obs.sell(&f.token, o, o * his_price);
            if let Some(&(bid, _)) = bk.bids.first() {
                self.dem.sell(&f.token, d, d * bid);
                line["asked"] = json!(bid);
            }
            line["my_shares"] = json!(mine);
            if mine <= 0.0 {
                line["decision"] = json!("skip");
                line["reason"] = json!(if held <= 0.0 { "aucune position à vendre" } else { "vente déjà posée ou en attente d'un lot" });
                return;
            }
            line["decision"] = json!("sell");
            self.post(&f.token, false, price, mine, bk, line, lat, at).await;
        }
    }

    /// Pose mon ordre : la partie qui croise déjà le carnet est prise tout de suite, le reste attend derrière la file.
    #[allow(clippy::too_many_arguments)]
    async fn post(
        &mut self,
        token: &str,
        buy: bool,
        price: f64,
        shares: f64,
        bk: &book::Book,
        line: &mut Value,
        lat: &mut Value,
        at: u64,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        let mut r = maker::Rest {
            id,
            token: token.into(),
            buy,
            price,
            size: shares,
            filled: 0.0,
            usd: 0.0,
            ahead: 0.0,
            posted_ms: at,
            live_id: None,
        };
        let side = if buy { "BUY" } else { "SELL" };
        match self.live.as_mut() {
            None => {
                let same = |l: &&(f64, f64)| (l.0 - price).abs() < 1e-9;
                let level = if buy { bk.bids.iter().find(same) } else { bk.asks.iter().find(same) };
                r.ahead = level.map_or(0.0, |l| l.1);
                let (sh, usd) = if buy {
                    book::walk_buy(&bk.asks, price, shares * price)
                } else {
                    book::walk_sell(&bk.bids, price, shares)
                };
                if sh > 0.0 {
                    r.filled = sh.min(shares);
                    r.usd = usd;
                    self.book_fill(token, buy, sh, usd);
                }
            }
            Some(l) => {
                let now = now_ms(&self.clock);
                let salt = u64::from_le_bytes(urandom::<8>()) >> 11;
                // achat : en $ (au cent), vente : en parts
                let size = if buy { (shares * price * 100.0).floor() / 100.0 } else { shares };
                let req = order_request(&l.armed.signer, &l.armed.creds, token, buy, size, price, bk.tick, salt, now, "GTC");
                let sent = match req {
                    Ok(req) => l.clob.get(&req, T).await.map_err(|e| format!("{e:?}")),
                    Err(e) => Err(e),
                };
                lat["sent"] = json!(now);
                lat["ack"] = json!(now_ms(&self.clock));
                match sent {
                    Ok(resp) => {
                        let raw = String::from_utf8_lossy(&resp.body).into_owned();
                        let v: Value = serde_json::from_str(&raw).unwrap_or_default();
                        r.live_id = v["orderID"].as_str().filter(|s| !s.is_empty()).map(String::from);
                        line["answer"] = json!(raw.chars().take(300).collect::<String>());
                        if r.live_id.is_none() {
                            line["reason"] = json!("ordre refusé par le CLOB");
                            let answer = line["answer"].clone();
                            self.log(json!({ "t": "order", "ev": "rejected", "token": token, "side": side, "price": price, "size": shares, "answer": answer }));
                            return;
                        }
                    }
                    Err(e) => {
                        line["reason"] = json!(format!("envoi : {e}"));
                        return;
                    }
                }
            }
        }
        line["order"] = json!({ "id": id, "price": price, "size": shares, "ahead": r.ahead, "filled_now": r.filled, "live_id": r.live_id });
        self.log(json!({ "t": "order", "ev": "post", "id": id, "token": token, "side": side, "price": price,
            "size": shares, "ahead": r.ahead, "filled": r.filled, "live_id": r.live_id }));
        if r.left() > 1e-9 {
            self.rests.push(r);
        } else {
            self.log(json!({ "t": "order", "ev": "done", "id": id }));
        }
    }

    /// Un fill de mon compte réel, et ce qu'il faut aux stratégies (coût moyen, parts vendues).
    fn book_fill(&mut self, token: &str, buy: bool, shares: f64, usd: f64) {
        if buy {
            self.real.buy(token, shares, usd);
            let b = self.bought.entry(token.into()).or_default();
            (b.0, b.1) = (b.0 + usd, b.1 + shares);
        } else {
            self.real.sell(token, shares, usd);
            *self.sold.entry(token.into()).or_default() += shares;
        }
    }

    async fn cancel(&mut self, id: u64, why: &str) {
        let Some(i) = self.rests.iter().position(|r| r.id == id) else {
            return;
        };
        let r = self.rests.remove(i);
        if let (Some(l), Some(live_id)) = (self.live.as_mut(), r.live_id.as_ref()) {
            let ts = now_ms(&self.clock) / 1000;
            if let Ok(req) = exec::cancel_request(HOST, &l.armed.signer, &l.armed.creds, live_id, ts) {
                let _ = l.clob.get(&req, T).await;
            }
        }
        self.log(json!({ "t": "order", "ev": "cancel", "id": id, "reason": why, "filled": r.filled, "left": r.left() }));
    }

    /// Un trade public (paper) : il remplit ceux de mes ordres posés sur ce token qu'il atteint.
    fn on_public_trade(&mut self, token: &str, price: f64, size: f64, taker_buy: bool, tx: &str) {
        if !tx.is_empty() && !self.applied.insert(tx.to_string()) {
            return;
        }
        if self.applied.len() > 200_000 {
            self.applied.clear();
        }
        let mut fills = vec![];
        for r in self.rests.iter_mut().filter(|r| r.token == token) {
            let q = r.on_trade(price, size, taker_buy);
            if q > 0.0 {
                fills.push((r.id, r.buy, q, q * r.price, r.left()));
            }
        }
        for (id, buy, q, usd, left) in fills {
            self.book_fill(token, buy, q, usd);
            self.log(json!({ "t": "order", "ev": "fill", "id": id, "token": token, "side": if buy { "BUY" } else { "SELL" },
                "price": usd / q, "shares": q, "left": left, "tx": tx }));
            if left <= 1e-9 {
                self.rests.retain(|r| r.id != id);
                self.log(json!({ "t": "order", "ev": "done", "id": id }));
            }
        }
    }

    /// Live : ce que le CLOB a rempli de mes ordres posés (`size_matched`), toutes les 5 s `[non vérifié sur ordre réel]`.
    async fn poll_rests(&mut self) {
        let ids: Vec<(u64, String)> = self
            .rests
            .iter()
            .filter_map(|r| r.live_id.clone().map(|l| (r.id, l)))
            .collect();
        for (id, live_id) in ids {
            let Some(l) = self.live.as_mut() else { return };
            let ts = now_ms(&self.clock) / 1000;
            let Ok(req) = exec::query_request(HOST, &l.armed.signer, &l.armed.creds, &live_id, ts) else {
                continue;
            };
            let Ok(resp) = l.clob.get(&req, T).await else {
                continue;
            };
            let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
            let matched = v["size_matched"]
                .as_str()
                .and_then(|x| x.parse::<f64>().ok())
                .unwrap_or(0.0);
            let status = v["status"].as_str().unwrap_or("").to_ascii_uppercase();
            let Some(r) = self.rests.iter_mut().find(|r| r.id == id) else {
                continue;
            };
            let q = matched - r.filled;
            let (token, buy, price) = (r.token.clone(), r.buy, r.price);
            if q > 1e-9 {
                r.filled = matched;
                r.usd += q * price;
                let left = r.left();
                self.book_fill(&token, buy, q, q * price);
                self.log(json!({ "t": "order", "ev": "fill", "id": id, "token": token, "side": if buy { "BUY" } else { "SELL" },
                    "price": price, "shares": q, "left": left, "status": status }));
            }
            let left = self.rests.iter().find(|r| r.id == id).map_or(0.0, |r| r.left());
            if left <= 1e-9 || status.contains("CANCEL") {
                self.rests.retain(|r| r.id != id);
                self.log(json!({ "t": "order", "ev": "done", "id": id, "status": status }));
            }
        }
    }

    /// Annule ceux de mes ordres dont il n'a plus rempli l'ordre jumeau depuis `cancel_after_min` ; la trace sert à
    /// repérer les annulations à tort.
    async fn expire(&mut self) {
        let now = now_ms(&self.clock);
        let stale: Vec<(u64, maker::Key, u64)> = self
            .rests
            .iter()
            .filter_map(|r| {
                let k = maker::key(&r.token, r.buy, r.price);
                let last = self.his_orders.get(&k).map_or(r.posted_ms, |h| h.last_ms);
                (now.saturating_sub(last) > self.strat.cancel_after_ms).then_some((r.id, k, last))
            })
            .collect();
        for (id, k, last) in stale {
            self.cancelled.insert(k, (now, last));
            self.cancel(id, "inactif").await;
        }
    }

    /// Exécution réelle (live) ou remplissage sur le carnet réel observé (paper) : (parts, $).
    #[allow(clippy::too_many_arguments)]
    async fn fill(
        &mut self,
        token: &str,
        buy: bool,
        size: f64,
        limit: f64,
        tick: f64,
        bk: &book::Book,
        lat: &mut Value,
        line: &mut Value,
    ) -> (f64, f64) {
        let (sh, usd) = match self.live.as_mut() {
            None => {
                if buy {
                    book::walk_buy(&bk.asks, limit, size)
                } else {
                    book::walk_sell(&bk.bids, limit, size)
                }
            }
            Some(l) => match send(l, &self.clock, token, buy, size, limit, tick).await {
                Ok(s) => {
                    (lat["sent"], lat["ack"], lat["fill"]) =
                        (json!(s.sent), json!(s.ack), json!(s.fill));
                    line["order"] = json!({ "phase": format!("{:?}", s.phase), "answer": s.raw.chars().take(300).collect::<String>() });
                    (s.shares, s.usd)
                }
                Err(e) => {
                    line["order"] = json!({ "error": e });
                    (0.0, 0.0)
                }
            },
        };
        line["fill_shares"] = json!(sh);
        line["fill_usd"] = json!(usd);
        if sh <= 0.0 {
            line["reason"] = json!("pas de liquidité dans la borne");
        }
        (sh, usd)
    }

    /// Secours et contrôle : data-api /activity. Un fill absent de la chaîne 10 s après son bloc est traité comme manqué.
    async fn poll_activity(&mut self) {
        let Ok(v) = get_json(&format!(
            "{DATA}/activity?user={}&limit=100&type=TRADE",
            self.cfg.wallet
        ))
        .await
        else {
            return;
        };
        let Some(rows) = v.as_array() else { return };
        let now = now_ms(&self.clock);
        let mut by: BTreeMap<String, (String, bool, f64, f64, u64)> = BTreeMap::new();
        for r in rows {
            let (Some(tx), Some(ts), Some(tok), Some(side), Some(size), Some(usd)) = (
                r["transactionHash"].as_str(),
                r["timestamp"].as_u64(),
                r["asset"].as_str(),
                r["side"].as_str(),
                r["size"].as_f64(),
                r["usdcSize"].as_f64(),
            ) else {
                continue;
            };
            if ts * 1000 < self.start_ms {
                continue;
            }
            let e =
                by.entry(tx.to_lowercase())
                    .or_insert((tok.into(), side == "BUY", 0.0, 0.0, ts));
            e.2 += size;
            e.3 += usd;
        }
        for (tx, (tok, buy, size, usd, ts)) in by {
            match self.by_tx.get(&tx).cloned() {
                Some((t, b, s)) => {
                    if self.checked.insert(tx.clone()) {
                        let ok = t == tok && b == buy && (s - size).abs() < 0.01;
                        self.log(json!({ "t": "check", "tx": tx, "ok": ok, "chain": [t, b, s], "data_api": [tok, buy, size] }));
                    }
                }
                None if ts * 1000 + 10_000 < now => {
                    // un fill manqué par la chaîne : l'abonnement aux logs est sans doute mort, on le refait (≤ 1/min)
                    if now >= self.last_resync_ms + 60_000 {
                        self.last_resync_ms = now;
                        let _ = self.chain_cmd.send(Cmd::Resync);
                        self.log(json!({ "t": "feed", "source": "chain", "health": "resync : fill manqué par la chaîne" }));
                    }
                    self.recovered.insert(tx.clone());
                    self.by_tx.insert(tx.clone(), (tok.clone(), buy, size));
                    self.checked.insert(tx.clone());
                    self.on_fill(
                        chain::Fill {
                            tx,
                            log_index: u64::MAX,
                            token: tok,
                            buy,
                            usd,
                            shares: size,
                        },
                        "data-api",
                        now,
                    )
                    .await;
                }
                None => {}
            }
        }
    }

    /// Chaque minute : sa valeur, le prix des positions tenues, les marchés résolus, puis une ligne `state`.
    async fn minute(&mut self) {
        match his_value(&self.cfg).await {
            Ok(v) => self.his_value = v,
            Err(e) => {
                self.log(json!({ "t": "warn", "what": format!("valeur du wallet suivi : {e}") }))
            }
        }
        let held: HashSet<String> = [&self.obs, &self.dem, &self.real]
            .iter()
            .flat_map(|a| a.pos.keys().cloned())
            .collect();
        let mut done = HashSet::new();
        for tok in held {
            if let Ok(b) = self.book(&tok).await {
                if let (Some(x), Some(y)) = (b.bids.first(), b.asks.first()) {
                    self.marks.insert(tok.clone(), (x.0 + y.0) / 2.0);
                }
            }
            let Some(m) = self.market_of.get(&tok).cloned() else {
                continue;
            };
            if !done.insert(m.clone()) {
                continue;
            }
            let Ok(v) = get_json(&format!("{CLOB}/markets/{m}")).await else {
                continue;
            };
            if v["closed"] != true {
                continue;
            }
            let Some(tokens) = v["tokens"].as_array() else {
                continue;
            };
            if !tokens.iter().any(|t| t["winner"] == true) {
                continue;
            }
            for t in tokens {
                let (Some(id), win) = (t["token_id"].as_str(), t["winner"] == true) else {
                    continue;
                };
                let payout = if win { 1.0 } else { 0.0 };
                for a in [&mut self.obs, &mut self.dem, &mut self.real] {
                    a.resolve(id, payout);
                }
                self.marks.remove(id);
                self.log(json!({ "t": "resolution", "market": m, "token": id, "payout": payout }));
            }
        }
        let st = |a: &Account, marks: &BTreeMap<String, f64>| {
            let (v, u) = a.value(marks);
            json!({ "value": v, "pnl": v - a.start, "unmarked": u, "cash": a.cash, "positions": a.pos.len() })
        };
        self.expire().await;
        let orders: Vec<Value> = self.rests.iter().map(|r| json!({ "id": r.id, "token": r.token, "side": if r.buy { "BUY" } else { "SELL" },
            "price": r.price, "size": r.size, "filled": r.filled, "ahead": r.ahead, "posted_ms": r.posted_ms })).collect();
        let line = json!({ "t": "state", "his_value": self.his_value, "obs": st(&self.obs, &self.marks), "dem": st(&self.dem, &self.marks),
            "real": st(&self.real, &self.marks), "marks": self.marks, "orders": orders, "real_pos": self.real.pos });
        self.log(line);
    }
}

fn run(path: &str) {
    let cfg: Config =
        serde_json::from_str(&std::fs::read_to_string(path).expect("config illisible"))
            .expect("config invalide");
    let ok = cfg.wallet.len() == 42
        && cfg.wallet.starts_with("0x")
        && cfg.wallet[2..].bytes().all(|b| b.is_ascii_hexdigit());
    assert!(ok, "config : wallet attendu 0x + 40 hex");
    assert!(
        cfg.mode == "paper" || cfg.mode == "live",
        "config : mode paper ou live"
    );
    let mut journal = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cfg.journal)
        .expect("journal");
    let clock = Arc::new(Clock::new());
    let lock = format!("{}.live-lock", cfg.journal);
    let live = (cfg.mode == "live").then(|| {
        // après un crash ou un redémarrage, le verrou du run précédent est toujours là : pas de reprise seule en live
        if std::path::Path::new(&lock).exists() {
            let _ = writeln!(journal, "{}", json!({ "t": "halt", "at": now_ms(&clock), "reason": "le run live précédent ne s'est pas arrêté proprement (crash ou redémarrage) : pas de reprise automatique en live ; relancer par deploy/start" }));
            std::process::exit(3);
        }
        std::fs::write(&lock, b"").expect("verrou live");
        let armed = engine_app::arm::arm(&cfg.wallet_file, T);
        let mut clob = Clob::new(HOST, 443, true, NetGuard::new(&ALLOWED, Arc::new(AtomicBool::new(true))));
        // réponses mesurées sur ordres réels : 0,35 à 3,5 s (docs/journal-decisions.md)
        (clob.deadline, clob.grace) = (Duration::from_secs(4), Duration::from_secs(10));
        Live { armed, clob }
    });
    if live.is_some() {
        println!("ARMED");
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let start_ms = now_ms(&clock);
        let real_cash = match &live {
            None => cfg.paper_cash_usd,
            Some(l) => {
                let ts = start_ms / 1000;
                let req = adapter_polymarket::account::balance_request(HOST, &l.armed.signer, &l.armed.creds, ts).expect("balance");
                let mut c = Clob::new(HOST, 443, true, NetGuard::new(&ALLOWED, Arc::new(AtomicBool::new(false))));
                let r = c.get(&req, T).await.expect("live : solde illisible");
                adapter_polymarket::account::read_balance(&r).expect("live : solde illisible") as f64 / 1e6
            }
        };
        let his_pos: HashMap<String, f64> = adapter_polymarket::account::holdings(&cfg.wallet, T, 40).await.unwrap_or_default()
            .into_iter().map(|h| (h.token, h.shares as f64 / 1e6)).collect();
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel();
        let (chain_cmd, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(feed::run_with(Box::new(chain::Logs { url: cfg.rpc_ws.clone(), wallet: cfg.wallet.clone() }), 0, 0, vec![], clock.clone(), cmd_rx, out_tx.clone(), true));
        // source 1 : les trades publics des tokens qu'il détient (heure du match) ; un marché calme peut se taire 30 s
        let subscribed: HashSet<String> = his_pos.iter().filter(|(_, s)| **s > 0.0).map(|(t, _)| t.clone()).collect();
        let (market_cmd, market_rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(feed::run_tuned(Box::new(Market), 1, 0, subscribed.iter().cloned().collect(), clock.clone(), market_rx, out_tx, false, Timing::default(), Duration::from_secs(30), Some(Duration::from_secs(10))));
        let mut bot = Bot {
            his_value: 0.0, his_pos, obs: Account::new(real_cash), dem: Account::new(real_cash), real: Account::new(real_cash),
            marks: BTreeMap::new(), market_of: HashMap::new(), matched: HashMap::new(), subscribed, market_cmd, chain_cmd, last_resync_ms: 0, recovered: HashSet::new(), seen_logs: HashSet::new(), by_tx: HashMap::new(), checked: HashSet::new(), entered: HashSet::new(),
            start_ms, live, journal, clock: clock.clone(), strat: strategy::build(&cfg.strategy),
            rests: vec![], next_id: 1, his_orders: HashMap::new(), cancelled: HashMap::new(), his_first: HashMap::new(), his_cum: HashMap::new(),
            his_kept: HashMap::new(), bought: HashMap::new(), sold: HashMap::new(), applied: HashSet::new(), side_checks: 0, cfg,
        };
        bot.minute().await;
        let line = json!({ "t": "start", "mode": bot.cfg.mode, "wallet": bot.cfg.wallet, "tiers": bot.cfg.tiers, "cash": real_cash, "his_value": bot.his_value, "his_positions": bot.his_pos.len(),
            "strategy": { "exec": bot.cfg.strategy.exec, "buy": bot.cfg.strategy.buy, "sell": bot.cfg.strategy.sell, "cancel_after_min": bot.cfg.strategy.cancel_after_min } });
        bot.log(line);
        let mut poll = tokio::time::interval(Duration::from_secs(5));
        let mut minute = tokio::time::interval(Duration::from_secs(60));
        minute.tick().await;
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM");
        loop {
            tokio::select! {
                m = out_rx.recv() => match m {
                    Some(Out::Frame { at, text, .. }) => if let chain::Frame::Fill(f) = chain::read(&text) {
                        if bot.recovered.contains(&f.tx) || !bot.seen_logs.insert((f.tx.clone(), f.log_index)) { continue; }
                        let e = bot.by_tx.entry(f.tx.clone()).or_insert((f.token.clone(), f.buy, 0.0));
                        e.2 += f.shares;
                        bot.on_fill(f, "chain", at.wall_ns / 1_000_000).await;
                    },
                    Some(Out::Event(e)) => if let Payload::Trade { id, asset, price, size, side } = e.payload {
                        let id = id.to_lowercase();
                        if let Some(ms) = e.event_ms {
                            if bot.matched.len() > 100_000 { bot.matched.clear(); }
                            bot.matched.entry(id.clone()).or_insert(ms);
                        }
                        let taker_buy = side == engine_core::event::Side::Bid;
                        // son achat posé rempli = un preneur vendeur : on vérifie que `side` est bien le sens du preneur
                        if let Some((_, his_buy, _)) = bot.by_tx.get(&id).cloned() {
                            if bot.side_checks < 50 {
                                bot.side_checks += 1;
                                bot.log(json!({ "t": "check", "kind": "trade_side", "tx": id, "ok": taker_buy != his_buy }));
                            }
                        }
                        if bot.strat.maker && bot.live.is_none() {
                            bot.on_public_trade(&asset, price as f64 / 1e6, size as f64 / 1e6, taker_buy, &id);
                        }
                    },
                    Some(Out::Health { source, health, .. }) => bot.log(json!({ "t": "feed", "source": if source == 0 { "chain" } else { "market" }, "health": format!("{health:?}") })),
                    Some(_) => {}
                    None => { bot.log(json!({ "t": "halt", "reason": "flux on-chain fermé" })); break; }
                },
                _ = poll.tick() => { bot.poll_activity().await; bot.poll_rests().await; },
                _ = minute.tick() => bot.minute().await,
                _ = tokio::signal::ctrl_c() => { bot.log(json!({ "t": "stop", "reason": "SIGINT" })); break; }
                _ = term.recv() => { bot.log(json!({ "t": "stop", "reason": "SIGTERM" })); break; }
            }
        }
        if bot.live.is_some() { let _ = std::fs::remove_file(&lock); }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le chemin d'ordre live jusqu'à la signature : la requête signée d'un achat FAK, montants du SDK, en-têtes L2.
    #[test]
    fn live_order_is_signed_up_to_the_request() {
        let s =
            Signer::from_bytes(&[0x11; 32], "0x7a57144fafde87f5ce4a880d570bae58eebc83f2").unwrap();
        let c = exec::Creds {
            key: "k".into(),
            secret: "c2VjcmV0".into(),
            passphrase: "p".into(),
        };
        let token = "18048113644252055228047575277787633022068076468097449060148334743204784347285";
        let req = String::from_utf8(
            order_request(
                &s,
                &c,
                token,
                true,
                2.349,
                0.55,
                0.01,
                42,
                1_793_240_248_866,
                "FAK",
            )
            .unwrap(),
        )
        .unwrap();
        let (head, body) = req.split_once("\r\n\r\n").unwrap();
        assert!(
            head.starts_with("POST /order HTTP/1.1\r\nHost: clob.polymarket.com\r\n")
                && head.contains("POLY_SIGNATURE: ")
                && head.contains("POLY_TIMESTAMP: 1793240248\r\n")
        );
        let v: Value = serde_json::from_str(body).unwrap();
        // 2,34 $ (au cent inférieur) à 0,55 au plus → 4,2546 parts (arrondi du SDK : 4 décimales vers le haut)
        assert_eq!(
            (
                v["orderType"].as_str(),
                v["order"]["side"].as_str(),
                v["order"]["makerAmount"].as_str(),
                v["order"]["takerAmount"].as_str()
            ),
            (Some("FAK"), Some("BUY"), Some("2340000"), Some("4254600"))
        );
        assert_eq!(
            (v["order"]["tokenId"].as_str(), v["owner"].as_str()),
            (Some(token), Some("k"))
        );
        let sig = v["order"]["signature"].as_str().unwrap();
        assert!(
            sig.starts_with("0x") && sig.len() > 2 + 2 * (65 + 64),
            "signature ERC-7739 complète"
        );
        assert!(
            order_request(&s, &c, token, false, 10.0, 0.555, 0.01, 1, 0, "FAK").is_err(),
            "prix hors grille refusé"
        );
    }

    #[test]
    fn wipe_zeroes() {
        let mut b = [7u8; 4];
        wipe(&mut b);
        assert_eq!(b, [0; 4]);
    }
}
