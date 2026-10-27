//! Replays a recorded session of the old application through the reference strategy and prints
//! the decisions of its OKX lane as JSON lines.
//! Reads the decompressed session on stdin: `zcat <session>.jsonl.gz | replay [patch-json] [mode] [source]`.
//! Modes: `decisions` (default); `fair`, one fair value of Up per second of each window (calibration, C6);
//! `maker`, the simulated maker candidate (C4, `maker.rs`) at half spreads 0.02 and 0.05, both fill bounds.
//! `source` keeps that lane only.
//! Like the old backtester's non-exact mode: the header's configuration (plus the patch), recorded `cfg`, `reset`
//! and `lane` events ignored, one lane: OKX (older sessions also recorded venues that are no longer supported).
mod maker;

use engine_strategy::reference::{Config, Event, Exit, Fee, Meta, Rec, Reference};
use maker::{Bound, Maker};
use serde_json::{Map, Value};
use std::io::{BufRead, Write};

fn num(v: &Value, what: &str) -> f64 { v.as_f64().unwrap_or_else(|| panic!("{what}: not a number: {v}")) }
fn num_or_nan(v: &Value) -> f64 { v.as_f64().unwrap_or(f64::NAN) }
fn text(v: &Value, what: &str) -> String { v.as_str().unwrap_or_else(|| panic!("{what}: not a string: {v}")).to_string() }
fn truthy(v: Option<&Value>) -> bool { matches!(v, Some(Value::Bool(true))) || v.and_then(Value::as_f64).is_some_and(|x| x != 0.0) }

/// `src/core/engine.js` DEFAULT_CONFIG for the keys a session header may lack.
const DEFAULTS: &str = r#"{"paused":false,"off":false,"minEdge":0.10,"minFair":0.60,"jump":0.20,"sigmaMult":1.5,"lookback":5,
"minTau":5,"maxTau":300,"cooldownMs":2000,"maxEntriesPerWindow":1,"bankroll":1000,"stakeUsd":100,"maxOpen":0,
"maxOpenFrac":0.5,"dailyStopFrac":0.3,"exit":"hold","takeProfit":0.02,"maxHoldSec":60,"exitSlippage":0.03,"signMs":3,
"matchMs":10,"defaultRttMs":150,"rttShare":1}"#;
const SOURCE_KEYS: [&str; 5] = ["bankroll", "stakeUsd", "maxOpen", "maxOpenFrac", "dailyStopFrac"];

fn config(src: &str, c: &Map<String, Value>) -> Config {
    let g = |k: &str| num(&c[k], k);
    // the realistic account's latency profile (DEFAULT_CONFIG: mode measured, extraMs 0)
    let extra_ms = c.get("profiles").and_then(Value::as_array)
        .and_then(|ps| ps.iter().find(|p| p["id"] == "real"))
        .map_or(0.0, |p| { assert_eq!(p["mode"], "measured", "realistic profile"); p["extraMs"].as_f64().unwrap_or(0.0) });
    Config {
        source: src.to_string(), paused: truthy(c.get("paused")), off: truthy(c.get("off")),
        min_edge: g("minEdge"), min_fair: g("minFair"), jump: g("jump"), sigma_mult: g("sigmaMult"), lookback: g("lookback"),
        min_tau: g("minTau"), max_tau: g("maxTau"), cooldown_ms: g("cooldownMs"), max_entries_per_window: g("maxEntriesPerWindow"),
        bankroll: g("bankroll"), stake_usd: g("stakeUsd"), max_open: g("maxOpen"), max_open_frac: g("maxOpenFrac"),
        daily_stop_frac: g("dailyStopFrac"), exit: if c["exit"] == "converge" { Exit::Converge } else { Exit::Hold },
        take_profit: g("takeProfit"), max_hold_sec: g("maxHoldSec"), exit_slippage: g("exitSlippage"), sign_ms: g("signMs"),
        match_ms: g("matchMs"), default_rtt_ms: g("defaultRttMs"), rtt_share: g("rttShare"), extra_ms,
    }
}

fn levels(v: &Value) -> Vec<(f64, f64)> {
    v.as_array().expect("book side").iter().map(|l| {
        let p = num(&l[0], "level price");
        assert!(p >= 0.0, "negative price {p}");
        (p, num(&l[1], "level size"))
    }).collect()
}

fn meta(w: &Value) -> Meta {
    let fee = match &w["fee"] {
        Value::Null => None,
        f => Some(Fee { rate: num_or_nan(&f["rate"]), exponent: f["exponent"].as_f64().unwrap_or(1.0) }),
    };
    let twap = num(&w["twap"], "twap");
    assert!(twap > 0.0, "twap {twap}");
    let min_size = w["minSize"].as_f64().filter(|x| *x != 0.0).unwrap_or(5.0);
    let tokens = w["tokens"].as_array().expect("tokens");
    Meta {
        slug: text(&w["slug"], "slug"), start: num(&w["start"], "start"), len: num(&w["len"], "len"),
        tokens: [text(&tokens[0], "token"), text(&tokens[1], "token")], tick: num(&w["tick"], "tick"), min_size, fee, twap,
        price_to_beat: num_or_nan(&w["priceToBeat"]),
    }
}

fn event(e: &Value) -> Option<Event> {
    let k = e["k"].as_str().expect("event kind");
    if matches!(k, "session" | "cfg" | "reset" | "lane") { return None; }
    let t = num(&e["t"], "t");
    Some(match k {
        "px" => Event::Px { t, src: text(&e["src"], "src"), p: num(&e["p"], "px") },
        "cl" => Event::Cl { t, p: num(&e["p"], "cl"), ts: num(&e["ts"], "cl ts") },
        "book" => Event::Book { t, a: text(&e["a"], "asset"), bids: levels(&e["bids"]), asks: levels(&e["asks"]) },
        "delta" => Event::Delta {
            t,
            ch: e["ch"].as_array().expect("ch").iter().map(|c| {
                let p = num(&c[2], "delta price");
                assert!(p >= 0.0, "negative price {p}");
                (text(&c[0], "asset"), c[1] == "A", p, num(&c[3], "delta size"))
            }).collect(),
        },
        "meta" => Event::Meta { t, w: meta(&e["w"]) },
        "res" => Event::Res { t, slug: text(&e["slug"], "slug"), winner: text(&e["winner"], "winner") },
        "ptb" => Event::Ptb { t, slug: text(&e["slug"], "slug"), price_to_beat: num_or_nan(&e["priceToBeat"]) },
        "rtt" => Event::Rtt { t, ms: num_or_nan(&e["ms"]) },
        "clock" => Event::Clock { t, offset: num(&e["offset"], "offset") },
        _ => Event::Other { t },
    })
}

fn f(x: f64) -> Value { serde_json::Number::from_f64(x).map_or(Value::Null, Value::Number) }

fn record(src: &str, r: &Rec) -> Value {
    let op = |buy: bool| if buy { "BUY" } else { "SELL" };
    match r {
        Rec::Order { t, slug, side, buy, limit, maker, taker, latency_ms } => serde_json::json!({"src": src, "type": "order", "t": f(*t),
            "slug": slug, "side": side.name(), "op": op(*buy), "limit": f(*limit), "makerAmount": maker.to_string(),
            "takerAmount": taker.to_string(), "latencyMs": f(*latency_ms)}),
        Rec::Fill { t, slug, side, buy, shares, usd, fees } => serde_json::json!({"src": src, "type": "fill", "t": f(*t), "slug": slug,
            "side": side.name(), "op": op(*buy), "shares": f(*shares), "usd": f(*usd), "fees": f(*fees)}),
        Rec::Miss { t, slug, side, buy } => serde_json::json!({"src": src, "type": "miss", "t": f(*t), "slug": slug, "side": side.name(), "op": op(*buy)}),
        Rec::Skip { t, slug, side, reason } => serde_json::json!({"src": src, "type": "skip", "t": f(*t), "slug": slug, "side": side.name(), "reason": reason}),
        Rec::Close { t, slug, side, pnl, how } => serde_json::json!({"src": src, "type": "close", "t": f(*t), "slug": slug, "side": side.name(), "pnl": f(*pnl), "how": how}),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let patch: Map<String, Value> = serde_json::from_str(args.get(1).map_or("{}", |s| s)).expect("patch");
    let mode = args.get(2).map_or("decisions", |s| s);
    let only = args.get(3);
    let mut lanes: Vec<(String, Reference)> = Vec::new();
    let mut makers: Vec<Vec<(f64, Bound, Maker)>> = Vec::new();
    let mut second: Vec<f64> = Vec::new();
    let (mut n, mut last_t) = (0u64, f64::NAN);
    let stdin = std::io::stdin().lock();
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for line in stdin.lines() {
        let Ok(line) = line else { break };            // truncated tail of a session still being written
        if line.is_empty() { continue; }
        let Ok(e) = serde_json::from_str::<Value>(&line) else { break };
        if e["k"] == "session" {
            if !lanes.is_empty() { continue; }
            let mut c: Map<String, Value> = serde_json::from_str(DEFAULTS).unwrap();
            c.extend(e["config"].as_object().expect("config").clone());
            c.extend(patch.clone());
            let srcs: Vec<String> = match e["sources"].as_array() {
                Some(a) => a.iter().map(|s| text(s, "source")).filter(|s| s == "okx").collect(),
                None => vec![text(&c["source"], "source")],
            };
            for s in srcs {
                if only.is_some_and(|o| *o != s) { continue; }
                let mut own = c.clone();
                if let Some(sc) = e["sourceCfg"].get(&s).and_then(Value::as_object) {
                    for k in SOURCE_KEYS { if let Some(v) = sc.get(k) { own.insert(k.into(), v.clone()); } }
                }
                lanes.push((s.clone(), Reference::new(config(&s, &own))));
                second.push(f64::NAN);
                makers.push([0.02, 0.05].iter().flat_map(|h| [Bound::Prudent, Bound::Optimistic].map(|b| (*h, b, Maker::new(b, *h)))).collect());
            }
            continue;
        }
        if lanes.is_empty() { continue; }
        let Some(ev) = event(&e) else { continue };
        for (_, r) in &mut lanes { r.process(&ev); }
        n += 1;
        last_t = ev.t();
        for (i, (s, r)) in lanes.iter_mut().enumerate() {
            match mode {
                "decisions" => for rec in r.records.drain(..) { writeln!(out, "{}", record(s, &rec)).unwrap(); },
                "fair" => {
                    r.records.clear();
                    let sec = (r.time() / 1000.0).floor();
                    if sec != second[i] {
                        second[i] = sec;
                        if let Some((slug, _, _, tau, fair)) = r.now().filter(|x| x.4.is_finite()) {
                            writeln!(out, "{}", serde_json::json!({"src": s, "slug": slug, "tau": f(tau), "fair": f(fair)})).unwrap();
                        }
                    }
                }
                "maker" => { r.records.clear(); for (_, _, m) in &mut makers[i] { m.step(r); } }
                _ => panic!("mode {mode}"),
            }
        }
    }
    for (i, (s, r)) in lanes.iter_mut().enumerate() {
        r.advance(last_t);
        if mode == "decisions" { for rec in r.records.drain(..) { writeln!(out, "{}", record(s, &rec)).unwrap(); } }
        for (h, b, m) in &mut makers[i] {
            m.finish();
            let bound = if *b == Bound::Prudent { "prudent" } else { "optimiste" };
            for x in &m.done {
                writeln!(out, "{}", serde_json::json!({"src": s, "half": h, "bound": bound, "slug": x.slug, "side": x.side, "t": f(x.t),
                    "shares": f(x.shares), "price": f(x.price), "fair": f(x.fair), "fairLater": f(x.fair_later)})).unwrap();
            }
        }
    }
    out.flush().unwrap();
    eprintln!("{n} events, {} lanes", lanes.len());
}
