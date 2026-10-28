//! Scellement d'un jour : réparation des morceaux mal fermés, concaténation + sha256, manifeste, renommages.
//! Ordre du Contrat 1 : 1) réparer 2) concaténer + sha256 + fsync (.tmp) 3) manifeste .tmp + fsync
//! 4) renommer les données puis le manifeste (= validation) 5) supprimer les morceaux.
use crate::json::{js, Obj};
use crate::util::*;
use crate::writer::{part_dir, RECORDER_VERSION};
use engine_core::clock::Now;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::Mutex;

/// Un seul scellement à la fois (rotation et démarrage ne se marchent pas dessus).
static LOCK: Mutex<()> = Mutex::new(());

/// Seuil de silence d'une source : `Caps.silence` de son adapter ; HTTP : constantes (sondage 10 s / 2 s) [à vérifier].
pub fn silence_ms(adapter: &str) -> u64 {
    match adapter {
        "polymarket-gamma" => 60_000,
        "polymarket-clob-time" => 10_000,
        a => engine_app::registry::feed_adapter(a).map_or(10_000, |(a, _)| a.caps().silence.as_millis() as u64),
    }
}

#[derive(Default, Debug)]
pub struct PartScan {
    pub run: u32,
    pub host_id: String,
    pub recorder_version: String,
    pub adapter: String,
    pub lines: u64,
    pub kinds: BTreeMap<String, u64>,
    pub first_t: i64,
    pub last_t: i64,
    pub clean_end: bool,
    pub hour_lines: [u64; 24],
    pub disconnects: u64,
    pub decode_errors: u64,
    /// Trous enregistrés (lignes `gap`) : (de, à, raison).
    pub gaps: Vec<(i64, i64, String)>,
    /// Silences supérieurs au seuil entre deux lignes consécutives.
    pub silences: Vec<(i64, i64)>,
    /// Le morceau finit mal (gzip tronqué, dernière ligne coupée ou illisible) : à réparer.
    pub dirty: bool,
}

#[derive(Deserialize)]
struct L<'a> {
    kind: &'a str,
    t_recv: i64,
    run: Option<u32>,
    host_id: Option<String>,
    recorder_version: Option<String>,
    adapter: Option<String>,
    from: Option<i64>,
    to: Option<i64>,
    reason: Option<String>,
}

fn zcat(path: &Path) -> io::Result<(Child, BufReader<ChildStdout>)> {
    let mut c = Command::new("gzip").arg("-dc").arg(path).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let out = BufReader::with_capacity(1 << 20, c.stdout.take().expect("stdout gzip"));
    Ok((c, out))
}

/// Lecture tolérante : jusqu'à la dernière ligne complète et valide. `day_start` sert aux heures.
pub fn scan_part(path: &Path, day_start: i64) -> io::Result<PartScan> {
    let (mut child, mut rd) = zcat(path)?;
    let mut s = PartScan::default();
    let (mut buf, mut prev, mut thr_us) = (Vec::new(), None::<i64>, i64::MAX);
    loop {
        buf.clear();
        if rd.read_until(b'\n', &mut buf).unwrap_or(0) == 0 { break; }
        if buf.last() != Some(&b'\n') { s.dirty = true; break; } // ligne coupée
        let Ok(l) = serde_json::from_slice::<L>(&buf) else { s.dirty = true; break };
        if s.lines == 0 { s.first_t = l.t_recv; }
        s.last_t = l.t_recv;
        s.lines += 1;
        *s.kinds.entry(l.kind.to_string()).or_default() += 1;
        s.hour_lines[((l.t_recv - day_start).div_euclid(3_600_000_000)).clamp(0, 23) as usize] += 1;
        s.clean_end = l.kind == "end";
        match l.kind {
            "header" => {
                s.run = l.run.unwrap_or(0);
                s.host_id = l.host_id.unwrap_or_default();
                s.recorder_version = l.recorder_version.unwrap_or_default();
                s.adapter = l.adapter.unwrap_or_default();
                thr_us = silence_ms(&s.adapter) as i64 * 1000;
            }
            "disconnect" => s.disconnects += 1,
            "decode_error" => s.decode_errors += 1,
            "gap" => if let (Some(f), Some(t)) = (l.from, l.to) { s.gaps.push((f, t, l.reason.unwrap_or_default())); },
            _ => {}
        }
        if !matches!(l.kind, "gap" | "clock" | "sent") {
            if let Some(p) = prev { if l.t_recv - p > thr_us { s.silences.push((p, l.t_recv)); } }
            prev = Some(l.t_recv);
        }
    }
    drop(rd);
    if s.dirty { let _ = child.kill(); }
    // gzip -dc sort en erreur sur un fichier tronqué
    if !child.wait()?.success() { s.dirty = true; }
    Ok(s)
}

/// Recompresse seulement les `keep` premières lignes (le morceau fautif, lui seul).
fn repair(path: &Path, keep: u64) -> io::Result<()> {
    let tmp = PathBuf::from(format!("{}.fix", path.display()));
    let mut gz = Command::new("gzip").args(["-6", "-c"]).stdin(Stdio::piped()).stdout(File::create(&tmp)?).spawn()?;
    let (mut child, mut rd) = zcat(path)?;
    {
        let mut w = gz.stdin.take().expect("stdin gzip");
        let mut buf = Vec::new();
        for _ in 0..keep {
            buf.clear();
            if rd.read_until(b'\n', &mut buf)? == 0 { break; }
            w.write_all(&buf)?;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    if !gz.wait()?.success() { return Err(io::Error::other("gzip (réparation) a échoué")); }
    File::open(&tmp)?.sync_all()?;
    std::fs::rename(&tmp, path)
}

/// (date, run, chemin) des `.part-N` d'un dossier.
fn list_parts(dir: &Path) -> Vec<(String, u32, PathBuf)> {
    let mut v: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().filter_map(|e| {
        let n = e.file_name().into_string().ok()?;
        let (date, run) = n.split_once(".jsonl.gz.part-")?;
        Some((date.to_string(), run.parse().ok()?, e.path()))
    }).collect();
    v.sort();
    v
}

/// Scelle `date` d'une source dont les morceaux sont dans `dir`. Ok(true) : jour validé par ce passage.
pub fn seal_day(dir: &Path, source: &str, date: &str, now: &dyn Now) -> io::Result<bool> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let day_start = day_start_us(date).ok_or_else(|| io::Error::other("date illisible"))?;
    let parts: Vec<_> = list_parts(dir).into_iter().filter(|p| p.0 == date).collect();
    let (data, man) = (dir.join(format!("{date}.jsonl.gz")), dir.join(format!("{date}.manifest.json")));
    let rm_parts = |parts: &[(String, u32, PathBuf)]| { for p in parts { let _ = std::fs::remove_file(&p.2); } };
    if man.exists() { rm_parts(&parts); return Ok(false); } // déjà validé (reprise après un arrêt avant l'étape 5)
    // 1) réparer
    let mut scans = Vec::new();
    for (_, n, p) in &parts {
        let mut s = scan_part(p, day_start)?;
        if s.dirty { repair(p, s.lines)?; }
        if s.lines > 0 { if s.run == 0 { s.run = *n; } scans.push((p.clone(), s)); }
    }
    if scans.is_empty() { rm_parts(&parts); return Ok(false); }
    scans.sort_by_key(|(_, s)| s.first_t);
    // 2) concaténer octet par octet + sha256 à la volée
    let (data_tmp, man_tmp) = (dir.join(format!("{date}.jsonl.gz.tmp")), dir.join(format!("{date}.manifest.json.tmp")));
    let (mut out, mut h, mut bytes, mut buf) = (File::create(&data_tmp)?, Sha256::new(), 0u64, vec![0u8; 1 << 20]);
    for (p, _) in &scans {
        let mut f = File::open(p)?;
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 { break; }
            h.update(&buf[..n]);
            out.write_all(&buf[..n])?;
            bytes += n as u64;
        }
    }
    out.sync_all()?;
    let sha = hex(&h.finalize());
    // 3) manifeste
    let ss: Vec<&PartScan> = scans.iter().map(|x| &x.1).collect();
    let mut mf = File::create(&man_tmp)?;
    mf.write_all(manifest(source, date, bytes, &sha, &ss, now.now().wall_ns as i64 / 1000).as_bytes())?;
    mf.sync_all()?;
    // 4) renommer les données puis le manifeste = point de validation
    std::fs::rename(&data_tmp, &data)?;
    std::fs::rename(&man_tmp, &man)?;
    if let Ok(d) = File::open(dir) { let _ = d.sync_all(); }
    // 5) supprimer les morceaux
    rm_parts(&parts);
    Ok(true)
}

/// Au démarrage : scelle tout `.part-*` d'un jour strictement passé, pour toutes les sources de `root`.
pub fn seal_past(root: &Path, today: &str, now: &dyn Now) {
    let dirs = |p: &Path| -> Vec<PathBuf> { std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect() };
    for src in dirs(root) {
        let source = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        for month in dirs(&src).iter().flat_map(|y| dirs(y)) {
            let mut dates: Vec<String> = list_parts(&month).into_iter().map(|p| p.0).filter(|d| d.as_str() < today).collect();
            dates.dedup();
            for d in dates {
                if let Err(e) = seal_day(&month, &source, &d, now) { eprintln!("scellement {source} {d} : {e}"); }
            }
        }
    }
}

/// Le lancement précédent s'est-il fini proprement ? (dernier morceau d'aujourd'hui, sinon d'hier ou son manifeste)
pub fn prev_run_clean(root: &Path, source: &str, today: &str) -> Option<bool> {
    let last_part = |date: &str| -> Option<bool> {
        let p = list_parts(&part_dir(root, source, date)).into_iter().filter(|p| p.0 == date).max_by_key(|p| p.1)?;
        Some(scan_part(&p.2, 0).ok()?.clean_end)
    };
    if let Some(c) = last_part(today) { return Some(c); }
    let y = date_str(day_start_us(today)? - 1);
    if let Some(c) = last_part(&y) { return Some(c); }
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(part_dir(root, source, &y).join(format!("{y}.manifest.json"))).ok()?).ok()?;
    v["runs"].as_array()?.last()?["clean_end"].as_bool()
}

fn manifest(source: &str, date: &str, bytes: u64, sha: &str, scans: &[&PartScan], now_us: i64) -> String {
    let ds = day_start_us(date).unwrap_or(0);
    let de = ds + DAY_US;
    let clip = |f: i64, t: i64| (f.max(ds), t.min(de));
    // (de, à, raison, origine)
    let mut gaps: Vec<(i64, i64, String, &str)> = vec![];
    for s in scans { for (f, t, r) in &s.gaps { let (f, t) = clip(*f, *t); if t > f { gaps.push((f, t, r.clone(), "recorded")); } } }
    let recorded: Vec<(i64, i64)> = gaps.iter().map(|g| (g.0, g.1)).collect();
    for s in scans {
        for (f, t) in &s.silences {
            let (f, t) = clip(*f, *t);
            if t > f && !recorded.iter().any(|r| r.0 <= f && t <= r.1) { gaps.push((f, t, "silence".into(), "inferred")); }
        }
    }
    for w in scans.windows(2) { if w[1].first_t > w[0].last_t { let (f, t) = clip(w[0].last_t, w[1].first_t); gaps.push((f, t, "restart".into(), "inferred")); } }
    let (first, last) = (scans[0].first_t, scans.iter().map(|s| s.last_t).max().unwrap_or(0));
    if first > ds { gaps.push((ds, first.min(de), "no_recording".into(), "inferred")); }
    if last < de { gaps.push((last.max(ds), de, "no_recording".into(), "inferred")); }
    gaps.sort();
    // union des trous -> secondes couvertes par heure
    let mut union: Vec<(i64, i64)> = vec![];
    for g in &gaps { match union.last_mut() { Some(u) if g.0 <= u.1 => u.1 = u.1.max(g.1), _ => union.push((g.0, g.1)) } }
    let mut hour_lines = [0u64; 24];
    let (mut kinds, mut disc, mut dec, mut lines) = (BTreeMap::<String, u64>::new(), 0u64, 0u64, 0u64);
    for s in scans {
        for h in 0..24 { hour_lines[h] += s.hour_lines[h]; }
        for (k, n) in &s.kinds { *kinds.entry(k.clone()).or_default() += n; }
        disc += s.disconnects; dec += s.decode_errors; lines += s.lines;
    }
    let mut covered = 0.0;
    let hours: Vec<String> = (0..24usize).map(|h| {
        let (a, b) = (ds + h as i64 * 3_600_000_000, ds + (h as i64 + 1) * 3_600_000_000);
        let gap_us: i64 = union.iter().map(|u| (u.1.min(b) - u.0.max(a)).max(0)).sum();
        let c = (3_600_000_000 - gap_us) as f64 / 1e6;
        covered += c;
        Obj::new().v("h", &h).v("lines", &hour_lines[h]).v("covered_s", &c).end()
    }).collect();
    let runs: Vec<String> = scans.iter().map(|s| Obj::new().v("run", &s.run).v("host_id", &s.host_id).v("recorder_version", &s.recorder_version)
        .v("first_t_recv", &s.first_t).v("last_t_recv", &s.last_t).v("lines", &s.lines).v("clean_end", &s.clean_end).end()).collect();
    let gaps_j: Vec<String> = gaps.iter().map(|g| Obj::new().v("from", &g.0).v("to", &g.1).v("reason", &g.2).v("origin", g.3).end()).collect();
    let validator = format!("source-tracing/{RECORDER_VERSION}");
    Obj::new().v("schema_version", &1).v("source", source).v("date", date).v("file", &format!("{date}.jsonl.gz")).v("format", "current")
        .v("host_id", &scans[0].host_id).v("bytes", &bytes).v("sha256", sha).v("lines", &lines).v("lines_by_kind", &kinds)
        .v("first_t_recv", &first).v("last_t_recv", &last)
        .raw("runs", &format!("[{}]", runs.join(","))).raw("hours", &format!("[{}]", hours.join(",")))
        .v("covered_s", &covered).v("coverage", &(covered / 86_400.0)).raw("gaps", &format!("[{}]", gaps_j.join(",")))
        .v("gaps_total_s", &(86_400.0 - covered)).v("disconnects", &disc).v("decode_errors", &dec)
        .v("silence_threshold_ms", &silence_ms(&scans[0].adapter)).raw("legacy", "null")
        .v("validated_at", &now_us).raw("validator_version", &js(&validator)).end()
}
