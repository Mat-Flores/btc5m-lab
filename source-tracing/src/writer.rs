//! Un fichier gzip par source et par jour : lignes du Contrat 1, un membre gzip complet par lancement.
//! Le gzip est le binaire `gzip -6` en sous-processus (comme engine/app/src/paper.rs).
use crate::json::{js, Obj};
use crate::{seal, util::*};
use engine_core::clock::Now;
use std::fs::OpenOptions;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::thread::JoinHandle;

pub type Clk = Arc<dyn Now + Send + Sync>;
pub const RECORDER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
pub struct Meta {
    pub root: PathBuf,
    pub source: String,
    pub adapter: String,
    pub connections: u32,
    pub host_id: String,
    pub hostname: String,
    pub run_id: String,
    pub prev_run_clean: Option<bool>,
}

struct Gz { child: Child, w: BufWriter<ChildStdin> }

pub struct Writer {
    m: Meta,
    now: Clk,
    pub run: u32,
    /// Contigu sur tout le lancement, rotations comprises : seul le premier header vaut 0 [à vérifier].
    seq: u64,
    date: String,
    midnight: i64,
    gz: Option<Gz>,
    /// Lignes du fichier courant.
    pub lines: u64,
    pub sealers: Vec<JoinHandle<()>>,
}

/// `<root>/<source>/<AAAA>/<MM>`
pub fn part_dir(root: &Path, source: &str, date: &str) -> PathBuf { root.join(source).join(&date[0..4]).join(&date[5..7]) }

/// Plus grand `.part-N` du jour + 1 (1 si aucun).
fn next_run(dir: &Path, date: &str) -> u32 {
    let prefix = format!("{date}.jsonl.gz.part-");
    let max = std::fs::read_dir(dir).into_iter().flatten().flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_prefix(&prefix)?.parse::<u32>().ok())).max();
    max.map_or(1, |m| m + 1)
}

impl Writer {
    /// Ouvre le morceau du jour de `t` et écrit le header (seq 0).
    pub fn open(m: Meta, now: Clk, t: i64, topics: &[String]) -> io::Result<Writer> {
        let date = date_str(t);
        let run = next_run(&part_dir(&m.root, &m.source, &date), &date);
        let mut w = Writer { m, now, run, seq: 0, date, midnight: 0, gz: None, lines: 0, sealers: vec![] };
        w.begin(t, topics)?;
        Ok(w)
    }

    pub fn next_midnight(&self) -> i64 { self.midnight }

    fn begin(&mut self, t: i64, topics: &[String]) -> io::Result<()> {
        self.date = date_str(t);
        self.midnight = next_midnight(t);
        let dir = part_dir(&self.m.root, &self.m.source, &self.date);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.jsonl.gz.part-{}", self.date, self.run));
        // create_new : on n'écrase jamais un morceau existant
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let mut child = Command::new("gzip").args(["-6", "-c"]).stdin(Stdio::piped()).stdout(file).spawn()?;
        let w = BufWriter::with_capacity(1 << 16, child.stdin.take().expect("stdin gzip"));
        self.gz = Some(Gz { child, w });
        self.lines = 0;
        let m = &self.m;
        let extra = [
            ("schema", js("updown5m-trace")), ("schema_version", "1".into()), ("time_unit", js("us")),
            ("source", js(&m.source)), ("adapter", js(&m.adapter)), ("topics", js(topics)), ("connections", js(&m.connections)),
            ("date", js(&self.date)), ("host_id", js(&m.host_id)), ("hostname", js(&m.hostname)),
            ("recorder", js("source-tracing")), ("recorder_version", js(RECORDER_VERSION)),
            ("run", js(&self.run)), ("run_id", js(&m.run_id)), ("format", js("current")), ("prev_run_clean", js(&m.prev_run_clean)),
        ];
        self.put(t, "header", None, None, None, None, &extra)
    }

    /// Écrit une ligne : enveloppe (clés dans l'ordre du contrat) puis champs propres.
    #[allow(clippy::too_many_arguments)]
    pub fn put(&mut self, t: i64, kind: &str, t_src: Option<i64>, seq_src: Option<u64>, conn: Option<u16>, raw: Option<&str>, extra: &[(&str, String)]) -> io::Result<()> {
        let mut o = Obj::new().v("seq", &self.seq).v("t_recv", &t).v("t_src", &t_src).v("seq_src", &seq_src).v("conn", &conn).v("kind", kind).v("raw", &raw);
        for (k, j) in extra { o = o.raw(k, j); }
        let mut s = o.end();
        s.push('\n');
        self.gz.as_mut().ok_or_else(|| io::Error::other("fichier fermé"))?.w.write_all(s.as_bytes())?;
        self.seq += 1;
        self.lines += 1;
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> { self.gz.as_mut().map_or(Ok(()), |g| g.w.flush()) }

    /// `end` puis fermeture du membre gzip.
    pub fn end(&mut self, t: i64, reason: &str) -> io::Result<()> {
        let n = self.lines + 1;
        self.put(t, "end", None, None, None, None, &[("lines", js(&n)), ("reason", js(reason))])?;
        if let Some(Gz { mut child, mut w }) = self.gz.take() {
            w.flush()?;
            drop(w); // ferme l'entrée de gzip
            if !child.wait()?.success() { return Err(io::Error::other("gzip a échoué")); }
        }
        Ok(())
    }

    /// 00:00:00 UTC : `end{rotate}`, scellement de la veille dans un fil séparé, nouveau morceau avec header.
    pub fn rotate(&mut self, topics: &[String]) -> io::Result<()> {
        let mid = self.midnight;
        self.end(mid, "rotate")?;
        let (dir, source, date, now) = (part_dir(&self.m.root, &self.m.source, &self.date), self.m.source.clone(), self.date.clone(), self.now.clone());
        self.sealers.retain(|h| !h.is_finished());
        self.sealers.push(std::thread::spawn(move || {
            if let Err(e) = seal::seal_day(&dir, &source, &date, &*now) { eprintln!("scellement {source} {date} : {e}"); }
        }));
        self.begin(mid, topics)
    }

    /// Pour les tests : attend les fils de scellement.
    pub fn join_sealers(&mut self) { for h in self.sealers.drain(..) { let _ = h.join(); } }
}
