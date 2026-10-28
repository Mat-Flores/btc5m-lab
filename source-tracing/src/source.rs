//! État d'une source : transforme les sorties du feed (ou les réponses HTTP) en lignes du Contrat 1.
//! Synchrone et sans réseau : l'horloge est injectée (`Now`), les tests rejouent des sorties fabriquées.
use crate::json::js;
use crate::seal;
use crate::util::*;
use crate::writer::{Clk, Meta, Writer};
use engine_core::clock::Stamp;
use engine_core::event::Payload;
use engine_core::feed::Out;
use engine_core::supervisor::{Health, Reason};
use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;

/// La rotation attend un peu après minuit pour que les trames estampillées avant minuit soient déjà écrites.
const ROTATE_GRACE_US: i64 = 20_000;
/// Une trame sans événement derrière elle est écrite (`msg`) après ce délai.
const PENDING_US: i64 = 100_000;

pub struct Info { pub name: String, pub adapter: String, pub http: bool }

pub struct HttpRes {
    pub at: i64,
    pub t_sent: i64,
    pub url: String,
    pub status: Option<u16>,
    pub raw: Option<String>,
    pub error: Option<String>,
}

/// Ce que lit `status.json` (Contrat 3).
#[derive(Clone, Default, Debug)]
pub struct Row { pub enabled: bool, pub connected: bool, pub last_msg_us: Option<i64>, pub msgs_per_s: f64, pub gaps_today: u64, pub gap_seconds_today: f64 }

struct Pending { at: Stamp, conn: u16, text: String, kind: Option<&'static str>, mixed: bool, n_ev: u64, t_src: Option<i64>, seq_src: Option<u64> }

#[derive(Clone)]
struct Gap { from: i64, reason: &'static str, expected: Option<u64>, got: Option<u64>, connected_since: bool }

pub struct Source {
    pub info: Info,
    pub topics: Vec<String>,
    root: PathBuf,
    host_id: String,
    hostname: String,
    run_id: String,
    now: Clk,
    w: Option<Writer>,
    enabled: bool,
    connected: bool,
    paused: bool,
    last_msg_us: Option<i64>,
    attempt: u64,
    pending: Option<Pending>,
    gap: Option<Gap>,
    gaps_today: u64,
    gap_us_today: i64,
    frames: VecDeque<i64>,
    decode_error: Option<String>,
}

fn kind_of(p: &Payload) -> Option<&'static str> {
    Some(match p {
        Payload::Quote { .. } => "quote",
        Payload::BookSnapshot { .. } => "snapshot",
        Payload::BookDelta { .. } => "delta",
        Payload::TickChange { .. } => "tick",
        Payload::Trade { .. } => "trade",
        Payload::Oracle { .. } => "oracle",
        Payload::Twap { .. } => "twap",
        _ => return None,
    })
}

fn us(s: Stamp) -> i64 { (s.wall_ns / 1000) as i64 }

impl Source {
    pub fn new(info: Info, topics: Vec<String>, root: PathBuf, host_id: String, now: Clk) -> Source {
        let run_id = format!("{}-{}", host_id, now.now().wall_ns / 1000);
        Source {
            info, topics, root, host_id, hostname: hostname(), run_id, now, w: None, enabled: false, connected: false, paused: false,
            last_msg_us: None, attempt: 0, pending: None, gap: None, gaps_today: 0, gap_us_today: 0, frames: VecDeque::new(), decode_error: None,
        }
    }

    fn t_now(&self) -> i64 { us(self.now.now()) }

    pub fn is_open(&self) -> bool { self.w.is_some() }

    /// Active la source : ouvre le morceau du jour (header) s'il ne l'est pas encore.
    pub fn activate(&mut self, prev_run_clean: Option<bool>) -> io::Result<()> {
        self.enabled = true;
        if self.w.is_none() {
            let m = Meta {
                root: self.root.clone(), source: self.info.name.clone(), adapter: self.info.adapter.clone(), connections: 1,
                host_id: self.host_id.clone(), hostname: self.hostname.clone(), run_id: self.run_id.clone(), prev_run_clean,
            };
            let t = self.t_now();
            self.w = Some(Writer::open(m, self.now.clone(), t, &self.topics)?);
        }
        if let Some(g) = self.gap.as_mut() { g.connected_since = self.info.http; }
        Ok(())
    }

    // ---- écriture -------------------------------------------------------------------------------------

    fn wput(&mut self, t: i64, kind: &str, t_src: Option<i64>, seq_src: Option<u64>, conn: Option<u16>, raw: Option<&str>, extra: &[(&str, String)]) -> io::Result<()> {
        match self.w.as_mut() { Some(w) => w.put(t, kind, t_src, seq_src, conn, raw, extra), None => Ok(()) }
    }

    fn wgap(&mut self, from: i64, to: i64, reason: &str, expected: Option<u64>, got: Option<u64>) -> io::Result<()> {
        let mut e = vec![("from", js(&from)), ("to", js(&to)), ("reason", js(reason))];
        if let Some(x) = expected { e.push(("expected", js(&x))); }
        if let Some(x) = got { e.push(("got", js(&x))); }
        self.gaps_today += 1;
        self.gap_us_today += to - from;
        self.wput(to, "gap", None, None, None, None, &e)
    }

    /// Avant d'écrire à l'instant `t` : la trame en attente d'abord, puis la rotation si minuit est passé.
    fn begin(&mut self, t: i64) -> io::Result<()> {
        self.flush_pending()?;
        self.check_rotate(t)
    }

    fn check_rotate(&mut self, t: i64) -> io::Result<()> {
        while self.w.as_ref().is_some_and(|w| w.next_midnight() <= t) { self.rotate()?; }
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        let Some(mid) = self.w.as_ref().map(|w| w.next_midnight()) else { return Ok(()) };
        if let Some(g) = self.gap.clone() {
            if mid > g.from { self.wgap(g.from, mid, g.reason, g.expected, g.got)?; }
            if let Some(g) = self.gap.as_mut() { g.from = mid; }
        }
        self.gaps_today = 0;
        self.gap_us_today = 0;
        let topics = self.topics.clone();
        self.w.as_mut().expect("writer").rotate(&topics)
    }

    fn flush_pending(&mut self) -> io::Result<()> {
        let Some(p) = self.pending.take() else { return Ok(()) };
        let kind = if p.mixed { "msg" } else { p.kind.unwrap_or("msg") };
        self.wput(us(p.at), kind, p.t_src, p.seq_src, Some(p.conn), Some(&p.text), &[("n_ev", js(&p.n_ev))])
    }

    fn gap_start(&mut self, from: i64, reason: &'static str, expected: Option<u64>, got: Option<u64>) {
        if self.gap.is_none() { self.gap = Some(Gap { from, reason, expected, got, connected_since: self.info.http }); }
    }

    /// Premier message d'une connexion neuve (ou première réponse HTTP) : le trou en cours se ferme.
    fn close_gap(&mut self, t: i64) -> io::Result<()> {
        if let Some(g) = self.gap.clone() {
            if g.connected_since {
                self.gap = None;
                self.wgap(g.from, t, g.reason, g.expected, g.got)?;
            }
        }
        Ok(())
    }

    fn stop_line(&mut self, t: i64, reason: &'static str) -> io::Result<()> {
        if self.gap.is_none() {
            self.wput(t, "disconnect", None, None, None, None, &[("reason", js(reason)), ("detail", "null".into())])?;
            self.gap_start(t, reason, None, None);
        }
        self.connected = false;
        Ok(())
    }

    // ---- entrées --------------------------------------------------------------------------------------

    pub fn on_out(&mut self, o: Out) -> io::Result<()> {
        if self.w.is_none() || self.paused { return Ok(()); }
        match o {
            Out::Event(e) => {
                if let Some(p) = self.pending.as_mut().filter(|p| p.at == e.recv) {
                    match kind_of(&e.payload) {
                        None => p.mixed = true,
                        Some(k) => if p.kind.is_some_and(|x| x != k) { p.mixed = true } else { p.kind = Some(k) },
                    }
                    if p.n_ev == 0 { p.t_src = e.event_ms.map(|m| m as i64 * 1000); p.seq_src = e.seq; }
                    p.n_ev += 1;
                }
            }
            Out::Frame { conn, at, text, .. } => {
                self.begin(us(at))?;
                self.last_msg_us = Some(us(at));
                self.frames.push_back(us(at));
                self.close_gap(us(at))?;
                self.pending = Some(Pending { at, conn, text, kind: None, mixed: false, n_ev: 0, t_src: None, seq_src: None });
            }
            Out::Connected { conn, at, url, .. } => {
                self.begin(us(at))?;
                self.attempt += 1;
                let e = [("url", js(&url)), ("topics", js(&self.topics)), ("attempt", js(&self.attempt))];
                self.wput(us(at), "connect", None, None, Some(conn), None, &e)?;
                self.connected = true;
                if let Some(g) = self.gap.as_mut() { g.connected_since = true; }
            }
            Out::Sent { conn, at, text, .. } => {
                self.begin(us(at))?;
                self.wput(us(at), "sent", None, None, Some(conn), Some(&text), &[])?;
            }
            Out::DecodeError { at, error, head, .. } => {
                // la trame fautive vient d'être annoncée : elle est écrite en entier dans decode_error, pas en `msg`
                let raw = match self.pending.take() { Some(p) if p.at == at => p.text, other => { self.pending = other; self.flush_pending()?; head } };
                self.check_rotate(us(at))?;
                self.wput(us(at), "decode_error", None, None, None, Some(&raw), &[("error", js(&error))])?;
                self.decode_error = Some(error);
            }
            Out::Health { health: Health::Valid, .. } => self.connected = true,
            Out::Health { at, health: Health::Invalid(r), .. } => {
                let t = us(at);
                let (reason, detail, from, exp, got): (&'static str, String, i64, Option<u64>, Option<u64>) = match r {
                    Reason::Connecting => return Ok(()),
                    Reason::Silence { ms } => ("silence", format!("{ms} ms sans message"), self.last_msg_us.unwrap_or(t), None, None),
                    Reason::SeqGap { expected, got } => ("seq_gap", format!("attendu {expected}, reçu {got}"), t, Some(expected), Some(got)),
                    Reason::Decode => ("decode", self.decode_error.take().unwrap_or_default(), t, None, None),
                    // le runner ne dit pas si la connexion s'est fermée, a échoué ou a expiré : `close` [à vérifier]
                    Reason::Disconnected => ("close", "connexion perdue".into(), t, None, None),
                };
                self.begin(t)?;
                self.connected = false;
                if self.gap.is_none() {
                    self.wput(t, "disconnect", None, None, None, None, &[("reason", js(reason)), ("detail", js(&detail))])?;
                    self.gap_start(from, reason, exp, got);
                    if let Some(g) = self.gap.as_mut() { g.connected_since = false; }
                }
            }
        }
        Ok(())
    }

    pub fn on_http(&mut self, r: HttpRes) -> io::Result<()> {
        if self.w.is_none() || self.paused { return Ok(()); }
        self.begin(r.at)?;
        let ok = r.status.is_some_and(|s| (200..300).contains(&s));
        self.connected = ok;
        if r.status.is_some() {
            self.last_msg_us = Some(r.at);
            self.frames.push_back(r.at);
            self.close_gap(r.at)?;
        }
        let mut e = vec![("method", js("GET")), ("url", js(&r.url)), ("status", js(&r.status)), ("t_sent", js(&r.t_sent))];
        if let Some(x) = &r.error { e.push(("error", js(x))); }
        self.wput(r.at, "http", None, None, None, r.raw.as_deref(), &e)
    }

    /// Appelée souvent (100 ms) : trame orpheline, rotation de minuit, vidage vers gzip.
    pub fn tick(&mut self) -> io::Result<()> {
        if self.w.is_none() { return Ok(()); }
        let t = self.t_now();
        if self.pending.as_ref().is_some_and(|p| t - us(p.at) > PENDING_US) { self.flush_pending()?; }
        if self.pending.is_none() { self.check_rotate(t - ROTATE_GRACE_US)?; }
        match self.w.as_mut() { Some(w) => w.flush(), None => Ok(()) }
    }

    // ---- cycle de vie ---------------------------------------------------------------------------------

    pub fn disable(&mut self) -> io::Result<()> {
        self.enabled = false;
        if self.w.is_none() { return Ok(()); }
        let t = self.t_now();
        self.begin(t)?;
        self.stop_line(t, "disabled")
    }

    pub fn disk_pause(&mut self) -> io::Result<()> {
        if self.w.is_none() || self.paused { self.paused = true; return Ok(()); }
        let t = self.t_now();
        self.begin(t)?;
        self.stop_line(t, "disk_low")?;
        self.paused = true;
        Ok(())
    }

    pub fn disk_resume(&mut self) {
        self.paused = false;
        if let Some(g) = self.gap.as_mut() { g.connected_since = self.info.http; }
    }

    /// SIGTERM : `disconnect{shutdown}` (si on était connecté), `end{shutdown}`, fermeture du membre gzip.
    pub fn shutdown(&mut self) -> io::Result<()> {
        if self.w.is_none() { return Ok(()); }
        let t = self.t_now();
        self.paused = false;
        self.begin(t)?;
        if self.gap.is_none() {
            self.wput(t, "disconnect", None, None, None, None, &[("reason", js("shutdown")), ("detail", "null".into())])?;
        } else if let Some(g) = self.gap.take() {
            self.wgap(g.from, t, g.reason, g.expected, g.got)?;
        }
        self.connected = false;
        let r = self.w.as_mut().expect("writer").end(t, "shutdown");
        self.w.as_mut().expect("writer").join_sealers();
        self.w = None;
        r
    }

    pub fn snapshot(&mut self) -> Row {
        let t = self.t_now();
        while self.frames.front().is_some_and(|f| *f < t - 10_000_000) { self.frames.pop_front(); }
        let (n, us_) = self.gap.as_ref().map_or((0, 0), |g| (1, (t - g.from).max(0)));
        Row {
            enabled: self.enabled, connected: self.connected && !self.paused, last_msg_us: self.last_msg_us,
            msgs_per_s: self.frames.len() as f64 / 10.0, gaps_today: self.gaps_today + n, gap_seconds_today: (self.gap_us_today + us_) as f64 / 1e6,
        }
    }

    /// Tests : attend les scellements lancés par les rotations.
    #[cfg(test)]
    pub fn join_sealers(&mut self) { if let Some(w) = self.w.as_mut() { w.join_sealers(); } }
}

/// Au démarrage, avant d'ouvrir : le lancement précédent s'est-il terminé proprement ?
pub fn prev_clean(root: &std::path::Path, source: &str, now_us: i64) -> Option<bool> { seal::prev_run_clean(root, source, &date_str(now_us)) }
