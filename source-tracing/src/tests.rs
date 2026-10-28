//! Tests sans réseau : horloge injectée, sorties du feed fabriquées, fichiers lus par `gzip -dc` et `sha256sum`
//! (recalcul indépendant du sha256 du manifeste).
use crate::config::Config;
use crate::seal;
use crate::source::{prev_clean, Info, Source};
use crate::util::*;
use crate::writer::{part_dir, Clk};
use engine_core::clock::{Now, Stamp};
use engine_core::event::{Event, Level, Payload};
use engine_core::feed::Out;
use engine_core::supervisor::{Health, Reason};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const SEC: i64 = 1_000_000;
const SRC: &str = "okx-btc-usdt-swap";

struct FakeNow(AtomicU64);
impl FakeNow { fn set(&self, us: i64) { self.0.store(us as u64, Ordering::SeqCst); } }
impl Now for FakeNow { fn now(&self) -> Stamp { let us = self.0.load(Ordering::SeqCst); Stamp { mono_ns: us * 1000, wall_ns: us * 1000 } } }

fn stamp(us: i64) -> Stamp { Stamp { mono_ns: us as u64 * 1000, wall_ns: us as u64 * 1000 } }

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("st-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn d0() -> i64 { day_start_us("2026-10-20").unwrap() }

fn new_source(root: &Path, now: &Arc<FakeNow>) -> Source {
    let clk: Clk = now.clone();
    let info = Info { name: SRC.into(), adapter: "okx-bbo-tbt".into(), http: false };
    Source::new(info, vec!["BTC-USDT-SWAP".into()], root.to_path_buf(), "test-host".into(), clk)
}

fn open(root: &Path, now: &Arc<FakeNow>, t: i64) -> Source {
    now.set(t);
    let mut s = new_source(root, now);
    let prev = prev_clean(root, SRC, t);
    s.activate(prev).unwrap();
    s
}

fn quote() -> Payload { Payload::Quote { bid: 1_000_000, ask: 2_000_000 } }

fn frame(s: &mut Source, t: i64, text: &str, evs: Vec<Payload>) {
    s.on_out(Out::Frame { source: 0, conn: 0, at: stamp(t), text: text.into() }).unwrap();
    for (i, p) in evs.into_iter().enumerate() {
        s.on_out(Out::Event(Event { source: 0, recv: stamp(t), event_ms: Some(t as u64 / 1000 - 5), seq: Some(100 + i as u64), payload: p })).unwrap();
    }
}

fn connected(s: &mut Source, t: i64) { s.on_out(Out::Connected { source: 0, conn: 0, at: stamp(t), url: "wss://test".into() }).unwrap(); }

fn shutdown(s: &mut Source, now: &Arc<FakeNow>, t: i64) { now.set(t); s.shutdown().unwrap(); }

fn gz_lines(path: &Path) -> Vec<String> {
    let o = Command::new("gzip").arg("-dc").arg(path).output().unwrap();
    assert!(o.status.success(), "gzip -dc a échoué sur {}", path.display());
    String::from_utf8(o.stdout).unwrap().lines().map(String::from).collect()
}

fn sha256sum(path: &Path) -> String {
    let o = Command::new("sha256sum").arg(path).output().unwrap();
    String::from_utf8(o.stdout).unwrap().split_whitespace().next().unwrap().to_string()
}

struct Day { lines: Vec<Value>, manifest: Value, dir: PathBuf }

/// Scelle les jours passés (`today`) puis relit le jour `date` validé.
fn sealed(root: &Path, now: &Arc<FakeNow>, date: &str, today: &str) -> Day {
    seal::seal_past(root, today, &**now);
    let dir = part_dir(root, SRC, date);
    let data = dir.join(format!("{date}.jsonl.gz"));
    let manifest: Value = serde_json::from_slice(&std::fs::read(dir.join(format!("{date}.manifest.json"))).unwrap()).unwrap();
    // le manifeste est exact : octets, sha256 recalculé hors du code, nombre de lignes
    assert_eq!(manifest["bytes"], std::fs::metadata(&data).unwrap().len());
    assert_eq!(manifest["sha256"], sha256sum(&data));
    let lines: Vec<Value> = gz_lines(&data).iter().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(manifest["lines"], lines.len());
    Day { lines, manifest, dir }
}

fn no_leftovers(dir: &Path, date: &str) {
    for e in std::fs::read_dir(dir).unwrap() {
        let n = e.unwrap().file_name().to_string_lossy().into_owned();
        if n.starts_with(date) { assert!(!n.contains(".part-") && !n.ends_with(".tmp"), "reste : {n}"); }
    }
}

fn kinds(d: &Day) -> Vec<&str> { d.lines.iter().map(|l| l["kind"].as_str().unwrap()).collect() }

fn gap_of<'a>(d: &'a Day, reason: &str, origin: &str) -> Option<&'a Value> {
    d.manifest["gaps"].as_array().unwrap().iter().find(|g| g["reason"] == reason && g["origin"] == origin)
}

#[test]
fn midnight_rotation_seals_the_day() {
    let (root, now) = (tmp("midnight"), Arc::new(FakeNow(AtomicU64::new(0))));
    let mut s = open(&root, &now, d0() + 86_400 * SEC - 2 * SEC);
    connected(&mut s, d0() + 86_400 * SEC - 1_900_000);
    frame(&mut s, d0() + 86_400 * SEC - 500_000, r#"{"a":1}"#, vec![quote()]);
    // minuit passé de 30 ms : la minuterie tourne même sans nouvelle trame
    now.set(d0() + 86_400 * SEC + 30_000);
    s.tick().unwrap();
    s.join_sealers();
    frame(&mut s, d0() + 86_400 * SEC + SEC, r#"{"a":2}"#, vec![quote()]);
    shutdown(&mut s, &now, d0() + 86_400 * SEC + 2 * SEC);

    let d = sealed(&root, &now, "2026-10-20", "2026-10-21");
    no_leftovers(&d.dir, "2026-10-20");
    assert_eq!(kinds(&d), ["header", "connect", "quote", "end"]);
    let first = gz_lines(&d.dir.join("2026-10-20.jsonl.gz"))[0].clone();
    assert!(first.starts_with(r#"{"seq":0,"t_recv":"#) && first.contains(r#""kind":"header","raw":null,"schema":"updown5m-trace","schema_version":1,"time_unit":"us","source":"okx-btc-usdt-swap""#), "{first}");
    let end = d.lines.last().unwrap();
    assert_eq!((end["reason"].as_str(), end["t_recv"].as_i64()), (Some("rotate"), Some(d0() + 86_400 * SEC)));
    assert_eq!(end["lines"], 4);
    for (i, l) in d.lines.iter().enumerate() { assert_eq!(l["seq"], i); }
    // les deux dernières secondes de la veille seulement sont couvertes
    let h23 = &d.manifest["hours"][23];
    assert!((h23["covered_s"].as_f64().unwrap() - 2.0).abs() < 1e-6, "{h23}");
    assert_eq!(h23["lines"], 4);
    assert!(gap_of(&d, "no_recording", "inferred").is_some());
    assert_eq!(d.manifest["runs"][0]["clean_end"], true);

    // le lendemain est un nouveau morceau, seq contigu, header à minuit
    let part = part_dir(&root, SRC, "2026-10-21").join("2026-10-21.jsonl.gz.part-1");
    let next = gz_lines(&part);
    let h: Value = serde_json::from_str(&next[0]).unwrap();
    assert_eq!((h["kind"].as_str(), h["seq"].as_u64(), h["t_recv"].as_i64(), h["date"].as_str()), (Some("header"), Some(4), Some(d0() + 86_400 * SEC), Some("2026-10-21")));
    let last: Value = serde_json::from_str(next.last().unwrap()).unwrap();
    assert_eq!((last["kind"].as_str(), last["reason"].as_str()), (Some("end"), Some("shutdown")));
    assert!(next.iter().any(|l| l.contains(r#""kind":"disconnect""#) && l.contains(r#""reason":"shutdown""#)));
}

#[test]
fn restart_in_the_middle_of_a_day() {
    let (root, now) = (tmp("restart"), Arc::new(FakeNow(AtomicU64::new(0))));
    let mut s = open(&root, &now, d0() + 10 * 3600 * SEC);
    connected(&mut s, d0() + 10 * 3600 * SEC);
    frame(&mut s, d0() + 10 * 3600 * SEC + 300 * SEC, "{}", vec![quote()]);
    shutdown(&mut s, &now, d0() + 10 * 3600 * SEC + 600 * SEC);
    // 2e lancement : le précédent s'est fini proprement, morceau numéro 2
    let mut s = open(&root, &now, d0() + 11 * 3600 * SEC);
    connected(&mut s, d0() + 11 * 3600 * SEC);
    frame(&mut s, d0() + 11 * 3600 * SEC + 1800 * SEC, "{}", vec![quote()]);
    shutdown(&mut s, &now, d0() + 12 * 3600 * SEC);

    let d = sealed(&root, &now, "2026-10-20", "2026-10-21");
    no_leftovers(&d.dir, "2026-10-20");
    let headers: Vec<&Value> = d.lines.iter().filter(|l| l["kind"] == "header").collect();
    assert_eq!(headers.len(), 2);
    assert_eq!((headers[0]["run"].as_u64(), headers[1]["run"].as_u64()), (Some(1), Some(2)));
    assert_eq!((headers[0]["prev_run_clean"].is_null(), headers[1]["prev_run_clean"].as_bool()), (true, Some(true)));
    let runs = d.manifest["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    assert!(runs.iter().all(|r| r["clean_end"] == true));
    let g = gap_of(&d, "restart", "inferred").expect("trou entre les lancements");
    assert_eq!((g["from"].as_i64(), g["to"].as_i64()), (Some(d0() + 10 * 3600 * SEC + 600 * SEC), Some(d0() + 11 * 3600 * SEC)));
    // 10 min + 1 h enregistrées : couverture bien inférieure à 1
    assert!(d.manifest["coverage"].as_f64().unwrap() < 0.1);
}

#[test]
fn truncated_part_is_repaired() {
    let (root, now) = (tmp("trunc"), Arc::new(FakeNow(AtomicU64::new(0))));
    let t0 = d0() + 10 * 3600 * SEC;
    let mut s = open(&root, &now, t0);
    connected(&mut s, t0);
    for i in 0..30_000u64 {
        let text = format!(r#"{{"n":{i},"x":"{:x}"}}"#, i.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        frame(&mut s, t0 + 1_000 + i as i64 * 1_000, &text, vec![quote()]);
    }
    shutdown(&mut s, &now, t0 + 60 * SEC);
    // coupure franche en fin de fichier : membre gzip tronqué
    let part = part_dir(&root, SRC, "2026-10-20").join("2026-10-20.jsonl.gz.part-1");
    let len = std::fs::metadata(&part).unwrap().len();
    std::fs::OpenOptions::new().write(true).open(&part).unwrap().set_len(len - 200).unwrap();

    let d = sealed(&root, &now, "2026-10-20", "2026-10-21");
    no_leftovers(&d.dir, "2026-10-20");
    let n = d.lines.len();
    assert!(n > 1_000 && n < 30_002, "{n} lignes gardées");
    assert_eq!(d.manifest["runs"][0]["clean_end"], false);
    assert_eq!(d.lines.last().unwrap()["kind"], "quote");
    // la dernière ligne gardée est complète : le fichier se lit jusqu'au bout sans erreur (vérifié dans `sealed`)
}

#[test]
fn recorded_gap_and_inferred_silence() {
    let (root, now) = (tmp("gaps"), Arc::new(FakeNow(AtomicU64::new(0))));
    let t0 = d0() + 10 * 3600 * SEC;
    let mut s = open(&root, &now, t0);
    connected(&mut s, t0);
    frame(&mut s, t0 + SEC, "{}", vec![quote()]);
    // le superviseur déclare le silence, le runner se reconnecte
    s.on_out(Out::Health { source: 0, at: stamp(t0 + 4 * SEC), health: Health::Invalid(Reason::Silence { ms: 3000 }) }).unwrap();
    connected(&mut s, t0 + 5 * SEC);
    frame(&mut s, t0 + 6 * SEC, "{}", vec![quote()]);
    frame(&mut s, t0 + 7 * SEC, "{}", vec![quote()]);
    // 33 s sans trame ni événement de connexion : trou déduit
    frame(&mut s, t0 + 40 * SEC, "{}", vec![quote()]);
    shutdown(&mut s, &now, t0 + 41 * SEC);

    let d = sealed(&root, &now, "2026-10-20", "2026-10-21");
    let k = kinds(&d);
    assert_eq!(&k[..8], ["header", "connect", "quote", "disconnect", "connect", "gap", "quote", "quote"]);
    let dis = &d.lines[3];
    assert_eq!((dis["reason"].as_str(), dis["t_recv"].as_i64()), (Some("silence"), Some(t0 + 4 * SEC)));
    let gap = &d.lines[5];
    assert_eq!((gap["from"].as_i64(), gap["to"].as_i64(), gap["reason"].as_str()), (Some(t0 + SEC), Some(t0 + 6 * SEC), Some("silence")));
    assert_eq!(d.manifest["disconnects"], 2); // silence + shutdown
    let rec = gap_of(&d, "silence", "recorded").unwrap();
    assert_eq!(rec["to"].as_i64(), Some(t0 + 6 * SEC));
    let inf = gap_of(&d, "silence", "inferred").unwrap();
    assert_eq!((inf["from"].as_i64(), inf["to"].as_i64()), (Some(t0 + 7 * SEC), Some(t0 + 40 * SEC)));
    assert_eq!(d.manifest["silence_threshold_ms"], 3000);
    // les heures sans rien : 0 seconde couverte hors de 10 h
    assert_eq!(d.manifest["hours"][3]["covered_s"], 0.0);
}

#[test]
fn disk_low_stops_writing_then_resumes_with_a_gap() {
    let (root, now) = (tmp("disk"), Arc::new(FakeNow(AtomicU64::new(0))));
    let t0 = d0() + 10 * 3600 * SEC;
    let mut s = open(&root, &now, t0);
    connected(&mut s, t0);
    frame(&mut s, t0 + SEC, "avant", vec![quote()]);
    now.set(t0 + 2 * SEC);
    s.disk_pause().unwrap();
    frame(&mut s, t0 + 3 * SEC, "perdu", vec![quote()]);
    now.set(t0 + 10 * SEC);
    s.disk_resume();
    connected(&mut s, t0 + 11 * SEC);
    frame(&mut s, t0 + 12 * SEC, "apres", vec![quote()]);
    shutdown(&mut s, &now, t0 + 13 * SEC);

    let d = sealed(&root, &now, "2026-10-20", "2026-10-21");
    assert_eq!(kinds(&d), ["header", "connect", "quote", "disconnect", "connect", "gap", "quote", "disconnect", "end"]);
    assert_eq!(d.lines[3]["reason"], "disk_low");
    assert_eq!((d.lines[5]["reason"].as_str(), d.lines[5]["from"].as_i64(), d.lines[5]["to"].as_i64()), (Some("disk_low"), Some(t0 + 2 * SEC), Some(t0 + 12 * SEC)));
    assert!(!d.lines.iter().any(|l| l["raw"] == "perdu"));
}

#[test]
fn frames_are_classified() {
    let (root, now) = (tmp("kinds"), Arc::new(FakeNow(AtomicU64::new(0))));
    let t0 = d0() + 10 * 3600 * SEC;
    let mut s = open(&root, &now, t0);
    connected(&mut s, t0);
    s.on_out(Out::Sent { source: 0, conn: 0, at: stamp(t0 + 1), text: r#"{"op":"subscribe"}"#.into() }).unwrap();
    frame(&mut s, t0 + 10, "ack", vec![]);
    let snap = Payload::BookSnapshot { asset: "a".into(), bids: vec![Level { price: 1, size: 1 }], asks: vec![], tick: None };
    frame(&mut s, t0 + 20, "snap", vec![snap]);
    let tr = Payload::Trade { asset: "a".into(), price: 1, size: 1, side: engine_core::event::Side::Bid, id: "x".into() };
    frame(&mut s, t0 + 30, "mixte", vec![quote(), tr]);
    frame(&mut s, t0 + 40, "deux", vec![quote(), quote()]);
    // trame illisible : écrite en entier dans decode_error, pas en `msg`
    s.on_out(Out::Frame { source: 0, conn: 0, at: stamp(t0 + 50), text: "{pas du json".into() }).unwrap();
    s.on_out(Out::DecodeError { source: 0, at: stamp(t0 + 50), error: "json: x".into(), head: "{pas".into() }).unwrap();
    shutdown(&mut s, &now, t0 + SEC);

    let d = sealed(&root, &now, "2026-10-20", "2026-10-21");
    assert_eq!(kinds(&d), ["header", "connect", "sent", "msg", "snapshot", "msg", "quote", "decode_error", "disconnect", "end"]);
    let l = &d.lines;
    assert_eq!((l[2]["raw"].as_str(), l[2]["conn"].as_u64()), (Some(r#"{"op":"subscribe"}"#), Some(0)));
    assert_eq!((l[3]["raw"].as_str(), l[3]["n_ev"].as_u64(), l[3]["t_src"].is_null()), (Some("ack"), Some(0), true));
    assert_eq!((l[4]["n_ev"].as_u64(), l[4]["seq_src"].as_u64(), l[4]["t_src"].as_i64()), (Some(1), Some(100), Some((t0 + 20) / 1000 * 1000 - 5000)));
    assert_eq!(l[5]["n_ev"], 2);
    assert_eq!((l[6]["kind"].as_str(), l[6]["n_ev"].as_u64()), (Some("quote"), Some(2)));
    assert_eq!((l[7]["raw"].as_str(), l[7]["error"].as_str()), (Some("{pas du json"), Some("json: x")));
    assert_eq!(d.manifest["decode_errors"], 1);
    assert_eq!(d.manifest["lines_by_kind"]["msg"], 2);
}

#[test]
fn example_config_is_valid_and_complete() {
    let text = include_str!("../config.example.json");
    let c = Config::parse(text).unwrap();
    let names: Vec<&str> = c.sources.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["polymarket-clob", "polymarket-gamma", "polymarket-clob-time", "chainlink-rtds", "okx-btc-usdt-swap", "binance-btcusdt", "binance-perp-btcusdt", "coinbase-btc-usd"]);
    assert_eq!((c.market.asset.as_str(), c.market.horizon_s), ("btc", 300));
    assert!(Config::parse(r#"{"host_id":"h","root":"r","sources":[{"name":"x","adapter":"nope","enabled":true}]}"#).is_err());
    assert!(Config::parse(r#"{"host_id":"h","root":"r","surplus":1,"sources":[]}"#).is_err());
}
