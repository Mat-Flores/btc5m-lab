//! Orchestration : une tâche par source (WebSocket via `feed::run`, ou sondage HTTP), config relue au changement
//! de mtime, `status.json` toutes les secondes, garde disque (D3), arrêt propre sur SIGTERM.
use crate::config::{is_http, Config, SourceCfg, CLOB_MARKET, HTTP_CLOB_TIME, HTTP_GAMMA};
use crate::json::Obj;
use crate::source::{prev_clean, HttpRes, Info, Row, Source};
use crate::util::*;
use crate::writer::{part_dir, Clk, RECORDER_VERSION};
use crate::{http, seal};
use engine_core::clock::Clock;
use engine_core::event::SourceId;
use engine_core::feed::{self, Cmd, Out};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::watch;
use tokio::task::JoinHandle;

enum Ctl { Enable(bool), Shutdown }

/// Réglages communs à toutes les sources (lus au démarrage ; les changer demande un redémarrage).
struct Env {
    root: PathBuf,
    host_id: String,
    market: engine_core::market::MarketSpec,
    gamma_poll: Duration,
    clob_time_poll: Duration,
    ahead: u64,
    grace_s: u64,
    clock: Arc<Clock>,
    now: Clk,
}

struct Handle { cfg: SourceCfg, ctl: UnboundedSender<Ctl>, join: JoinHandle<()>, row: Arc<Mutex<Row>> }

fn fatal(r: io::Result<()>) {
    if let Err(e) = r {
        // shortcut : une erreur d'écriture arrête le processus (systemd Restart=always le relance, les morceaux sont réparés) ; à raffiner si une source seule pose problème
        eprintln!("erreur d'écriture, arrêt : {e}");
        std::process::exit(1);
    }
}

async fn recv<T>(r: &mut Option<UnboundedReceiver<T>>) -> Option<T> {
    match r { Some(r) => r.recv().await, None => std::future::pending().await }
}

#[derive(Default)]
struct Active {
    feed_out: Option<UnboundedReceiver<Out>>,
    feed_cmd: Option<UnboundedSender<Cmd>>,
    http_rx: Option<UnboundedReceiver<HttpRes>>,
    disc_rx: Option<UnboundedReceiver<Vec<String>>>,
    tasks: Vec<JoinHandle<()>>,
    /// Topics découverts (tokens des fenêtres), en plus des topics statiques de la config.
    dynamic: Vec<String>,
}

/// L'appel à `feed::run` (3 sorties de tracing activées) est isolé ici : un seul endroit à ajuster si l'engine change sa signature.
fn spawn_feed(kind: &str, id: SourceId, topics: Vec<String>, env: &Env) -> (JoinHandle<()>, UnboundedSender<Cmd>, UnboundedReceiver<Out>) {
    let (adapter, _) = engine_app::registry::feed_adapter(kind).expect("adapter validé à la lecture de la config");
    let (cmd_tx, cmd_rx) = unbounded_channel();
    let (out_tx, out_rx) = unbounded_channel();
    (tokio::spawn(feed::run_with(adapter, id, 0, topics, env.clock.clone(), cmd_rx, out_tx, true)), cmd_tx, out_rx)
}

async fn start(src: &mut Source, a: &mut Active, id: SourceId, statics: &[String], env: &Env) -> io::Result<()> {
    let prev = if src.is_open() { None } else {
        let (root, name, t) = (env.root.clone(), src.info.name.clone(), (env.now.now().wall_ns / 1000) as i64);
        tokio::task::spawn_blocking(move || prev_clean(&root, &name, t)).await.ok().flatten()
    };
    let market = src.info.adapter == CLOB_MARKET;
    if market {
        for _ in 0..3 {
            a.dynamic = http::discover(&env.market, env.ahead, &env.now).await;
            if !a.dynamic.is_empty() { break; }
        }
        src.topics = statics.iter().chain(a.dynamic.iter()).cloned().collect();
    }
    src.activate(prev)?;
    match src.info.adapter.as_str() {
        HTTP_GAMMA => {
            let (tx, rx) = unbounded_channel();
            a.tasks.push(tokio::spawn(http::poll_gamma(env.market.clone(), env.gamma_poll, env.ahead, env.grace_s, env.now.clone(), tx)));
            a.http_rx = Some(rx);
        }
        HTTP_CLOB_TIME => {
            let (tx, rx) = unbounded_channel();
            a.tasks.push(tokio::spawn(http::poll_clob_time(env.clob_time_poll, env.now.clone(), tx)));
            a.http_rx = Some(rx);
        }
        kind => {
            let (task, cmd, out) = spawn_feed(kind, id, src.topics.clone(), env);
            a.tasks.push(task);
            a.feed_cmd = Some(cmd);
            a.feed_out = Some(out);
            if market {
                let (tx, rx) = unbounded_channel();
                a.tasks.push(tokio::spawn(http::discover_loop(env.market.clone(), env.gamma_poll, env.ahead, env.now.clone(), tx)));
                a.disc_rx = Some(rx);
            }
        }
    }
    Ok(())
}

/// Arrête les tâches ; ce qu'elles avaient déjà envoyé est écrit avant.
fn stop(src: &mut Source, a: &mut Active) -> io::Result<()> {
    for t in a.tasks.drain(..) { t.abort(); }
    if let Some(rx) = a.feed_out.as_mut() { while let Ok(o) = rx.try_recv() { src.on_out(o)?; } }
    if let Some(rx) = a.http_rx.as_mut() { while let Ok(r) = rx.try_recv() { src.on_http(r)?; } }
    a.feed_out = None; a.feed_cmd = None; a.http_rx = None; a.disc_rx = None;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn actor(mut src: Source, id: SourceId, enabled: bool, statics: Vec<String>, env: Arc<Env>, mut ctl: UnboundedReceiver<Ctl>, mut disk: watch::Receiver<bool>, row: Arc<Mutex<Row>>) {
    let mut a = Active::default();
    if *disk.borrow() { fatal(src.disk_pause()); }
    if enabled { fatal(start(&mut src, &mut a, id, &statics, &env).await); }
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            biased;
            o = recv(&mut a.feed_out) => match o { Some(o) => fatal(src.on_out(o)), None => a.feed_out = None },
            r = recv(&mut a.http_rx) => match r { Some(r) => fatal(src.on_http(r)), None => a.http_rx = None },
            d = recv(&mut a.disc_rx) => match d {
                Some(set) => {
                    let (new, old): (Vec<String>, Vec<String>) = (set.iter().filter(|t| !a.dynamic.contains(t)).cloned().collect(), a.dynamic.iter().filter(|t| !set.contains(t)).cloned().collect());
                    if let Some(c) = &a.feed_cmd {
                        if !new.is_empty() { let _ = c.send(Cmd::Subscribe(new)); }
                        if !old.is_empty() { let _ = c.send(Cmd::Unsubscribe(old)); }
                    }
                    src.topics = statics.iter().chain(set.iter()).cloned().collect();
                    a.dynamic = set;
                }
                None => a.disc_rx = None,
            },
            c = ctl.recv() => match c {
                Some(Ctl::Enable(true)) => if !src.snapshot().enabled { fatal(start(&mut src, &mut a, id, &statics, &env).await); },
                Some(Ctl::Enable(false)) => if src.snapshot().enabled { fatal(stop(&mut src, &mut a)); fatal(src.disable()); },
                Some(Ctl::Shutdown) | None => { fatal(stop(&mut src, &mut a)); fatal(src.shutdown()); *row.lock().unwrap() = src.snapshot(); return; }
            },
            _ = disk.changed() => {
                if *disk.borrow_and_update() { fatal(src.disk_pause()); }
                else {
                    src.disk_resume();
                    // le flux a un trou : une connexion neuve (snapshot complet) plutôt que la suite d'un flux incomplet
                    if let Some(c) = &a.feed_cmd { let _ = c.send(Cmd::Resync); }
                }
            }
            _ = tick.tick() => { fatal(src.tick()); *row.lock().unwrap() = src.snapshot(); }
        }
    }
}

// ---- garde disque (D3) ---------------------------------------------------------------------------------

/// Hystérésis : on s'arrête sous `min`, on ne reprend qu'au-dessus de `resume`.
pub fn guard_step(paused: bool, free_gb: f64, min: f64, resume: f64) -> bool { if paused { free_gb <= resume } else { free_gb < min } }

/// Octets libres du volume de `path`. shortcut : mesure par `df` sous Linux seulement ; ailleurs (poste de dev) pas de garde.
pub fn free_bytes(path: &Path) -> Option<u64> {
    if !cfg!(target_os = "linux") { return None; }
    let out = std::process::Command::new("df").args(["--output=avail", "-B1"]).arg(path).output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().last()?.trim().parse().ok()
}

async fn disk_loop(root: PathBuf, min: f64, resume: f64, tx: watch::Sender<bool>) {
    loop {
        let r = root.clone();
        if let Some(free) = tokio::task::spawn_blocking(move || free_bytes(&r)).await.ok().flatten() {
            let paused = guard_step(*tx.borrow(), free as f64 / 1e9, min, resume);
            if paused != *tx.borrow() { eprintln!("garde disque : {:.2} Go libres, écriture {}", free as f64 / 1e9, if paused { "arrêtée" } else { "reprise" }); let _ = tx.send(paused); }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

// ---- status.json (Contrat 3) ---------------------------------------------------------------------------

fn file_today_bytes(root: &Path, source: &str, today: &str) -> u64 {
    let prefix = format!("{today}.jsonl.gz.part-");
    std::fs::read_dir(part_dir(root, source, today)).into_iter().flatten().flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix)).filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()
}

fn status_json(root: &Path, host_id: &str, started: i64, now: i64, rows: &[(String, Row)]) -> String {
    let today = date_str(now);
    let src: Vec<String> = rows.iter().map(|(name, r)| Obj::new()
        .v("name", name).v("enabled", &r.enabled).v("connected", &r.connected)
        .v("last_msg_age_ms", &r.last_msg_us.map(|t| ((now - t) / 1000).max(0)))
        .v("msgs_per_s", &((r.msgs_per_s * 10.0).round() / 10.0)).v("gaps_today", &r.gaps_today)
        .v("gap_seconds_today", &((r.gap_seconds_today * 10.0).round() / 10.0)).v("file_today_bytes", &file_today_bytes(root, name, &today)).end()).collect();
    Obj::new().v("schema_version", &1).v("host_id", host_id).v("recorder_version", RECORDER_VERSION).v("now_us", &now).v("started_at_us", &started)
        .raw("sources", &format!("[{}]", src.join(","))).end()
}

fn write_status(root: &Path, text: &str) -> io::Result<()> {
    let tmp = root.join("status.json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, root.join("status.json"))
}

// ---- orchestrateur -------------------------------------------------------------------------------------

fn spawn_source(sc: &SourceCfg, id: SourceId, env: &Arc<Env>, disk: &watch::Receiver<bool>) -> Handle {
    let info = Info { name: sc.name.clone(), adapter: sc.adapter.clone(), http: is_http(&sc.adapter) };
    let src = Source::new(info, sc.topics.clone(), env.root.clone(), env.host_id.clone(), env.now.clone());
    let (ctl, ctl_rx) = unbounded_channel();
    let row = Arc::new(Mutex::new(Row::default()));
    let join = tokio::spawn(actor(src, id, sc.enabled, sc.topics.clone(), env.clone(), ctl_rx, disk.clone(), row.clone()));
    Handle { cfg: sc.clone(), ctl, join, row }
}

fn mtime(p: &str) -> Option<std::time::SystemTime> { std::fs::metadata(p).ok()?.modified().ok() }

pub async fn run(config_path: String) -> Result<(), String> {
    let cfg = Config::load(&config_path)?;
    let root = PathBuf::from(&cfg.root);
    std::fs::create_dir_all(&root).map_err(|e| format!("{} : {e}", root.display()))?;
    let clock = Arc::new(Clock::new());
    let now: Clk = clock.clone();
    let started = (now.now().wall_ns / 1000) as i64;
    let env = Arc::new(Env {
        root: root.clone(), host_id: cfg.host_id.clone(), market: cfg.market.clone(), gamma_poll: Duration::from_millis(cfg.gamma_poll_ms),
        clob_time_poll: Duration::from_millis(cfg.clob_time_poll_ms), ahead: cfg.windows_ahead, grace_s: cfg.gamma_prev_grace_s, clock, now: now.clone(),
    });
    // scellement des jours passés laissés en `.part-*` (fil séparé : le tracing démarre sans attendre)
    {
        let (r, n, today) = (root.clone(), now.clone(), date_str(started));
        std::thread::spawn(move || seal::seal_past(&r, &today, &*n));
    }
    let (disk_tx, disk_rx) = watch::channel(false);
    tokio::spawn(disk_loop(root.clone(), cfg.disk_min_free_gb, cfg.disk_resume_free_gb, disk_tx));

    let mut handles: Vec<Handle> = vec![];
    let mut next_id: SourceId = 0;
    for sc in &cfg.sources { handles.push(spawn_source(sc, next_id, &env, &disk_rx)); next_id += 1; }
    eprintln!("source-tracing {RECORDER_VERSION} : {} sources, racine {}", handles.len(), root.display());

    let mut last_mtime = mtime(&config_path);
    let mut ticker = tokio::time::interval(Duration::from_millis(500));
    let mut status_tick = 0u32;
    #[cfg(unix)]
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).map_err(|e| e.to_string())?;
    loop {
        #[cfg(unix)]
        let term_fut = term.recv();
        #[cfg(not(unix))]
        let term_fut = std::future::pending::<Option<()>>();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = term_fut => break,
            _ = ticker.tick() => {}
        }
        // config : relue dès que le mtime change
        let m = mtime(&config_path);
        if m != last_mtime {
            last_mtime = m;
            match Config::load(&config_path) {
                Err(e) => eprintln!("config ignorée : {e}"),
                Ok(new) => {
                    for sc in &new.sources {
                        match handles.iter_mut().find(|h| h.cfg.name == sc.name) {
                            Some(h) => {
                                if h.cfg.enabled != sc.enabled { let _ = h.ctl.send(Ctl::Enable(sc.enabled)); }
                                if (h.cfg.adapter.as_str(), &h.cfg.topics, h.cfg.connections) != (sc.adapter.as_str(), &sc.topics, sc.connections) { eprintln!("{} : adapter/topics inchangés tant que le tracer n'est pas redémarré", sc.name); }
                                else { h.cfg = sc.clone(); }
                                h.cfg.enabled = sc.enabled;
                            }
                            None => { handles.push(spawn_source(sc, next_id, &env, &disk_rx)); next_id += 1; }
                        }
                    }
                    // source retirée de la config : arrêt propre
                    handles.retain(|h| { let keep = new.sources.iter().any(|s| s.name == h.cfg.name); if !keep { let _ = h.ctl.send(Ctl::Shutdown); } keep });
                }
            }
        }
        // status.json : toutes les secondes (le ticker bat toutes les 500 ms)
        status_tick += 1;
        if status_tick % 2 == 0 {
            let rows: Vec<(String, Row)> = handles.iter().map(|h| (h.cfg.name.clone(), h.row.lock().unwrap().clone())).collect();
            let n = (now.now().wall_ns / 1000) as i64;
            if let Err(e) = write_status(&root, &status_json(&root, &cfg.host_id, started, n, &rows)) { eprintln!("status.json : {e}"); }
        }
    }
    eprintln!("arrêt demandé : fermeture des sources");
    for h in &handles { let _ = h.ctl.send(Ctl::Shutdown); }
    for h in handles { let _ = tokio::time::timeout(Duration::from_secs(20), h.join).await; }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_guard_hysteresis() {
        assert!(!guard_step(false, 2.5, 2.0, 3.0));
        assert!(guard_step(false, 1.9, 2.0, 3.0));
        assert!(guard_step(true, 2.5, 2.0, 3.0)); // entre les deux seuils : on reste arrêté
        assert!(guard_step(true, 3.0, 2.0, 3.0));
        assert!(!guard_step(true, 3.1, 2.0, 3.0));
    }

    #[test]
    fn status_shape() {
        let r = Row { enabled: true, connected: true, last_msg_us: Some(990_000), msgs_per_s: 34.14, gaps_today: 2, gap_seconds_today: 5.26 };
        let j = status_json(Path::new("/nonexistent"), "dry-run", 5, 1_000_000, &[("okx".into(), r), ("vide".into(), Row::default())]);
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["schema_version"], 1);
        assert_eq!(v["sources"][0]["last_msg_age_ms"], 10);
        assert_eq!(v["sources"][0]["msgs_per_s"], 34.1);
        assert_eq!(v["sources"][0]["gap_seconds_today"], 5.3);
        assert!(v["sources"][1]["last_msg_age_ms"].is_null());
        assert_eq!(v["sources"][1]["file_today_bytes"], 0);
    }
}
