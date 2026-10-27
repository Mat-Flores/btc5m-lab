//! Append-only ledger: one JSON line per fact, `{"seq","mono_ns","wall_ns","fact"}`, seq consecutive from 0.
//! A fact is acknowledged by the ledger only once written and fsynced (`durable_seq`). Opening verifies every line:
//! a torn tail (last line without newline or unreadable) is cut and reported; any other damage refuses to open.
//! The hot thread never touches the file: it stamps facts and sends them to `spawn_writer`'s thread.
use crate::clock::Stamp;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A fact stamped where it happened, before it reaches the ledger.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stamped<T> { #[serde(flatten)] pub stamp: Stamp, pub fact: T }

/// A fact as read back from the ledger.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry<T> { pub seq: u64, #[serde(flatten)] pub stamp: Stamp, pub fact: T }

#[derive(Serialize)]
struct Line<'a, T> { seq: u64, #[serde(flatten)] stamp: Stamp, fact: &'a T }

#[derive(Deserialize)]
struct SeqOnly { seq: u64 }

#[derive(Debug, PartialEq, Eq)]
pub struct OpenReport { pub entries: u64, pub cut_bytes: u64 }

/// Splits a ledger image into its valid prefix length and entry count, or fails on damage before the tail.
fn scan(bytes: &[u8]) -> io::Result<(usize, u64)> {
    let bad = |m: String| io::Error::new(io::ErrorKind::InvalidData, m);
    let (mut pos, mut n) = (0usize, 0u64);
    while pos < bytes.len() {
        let Some(nl) = bytes[pos..].iter().position(|&b| b == b'\n') else { break }; // torn tail
        let line = &bytes[pos..pos + nl];
        match serde_json::from_slice::<SeqOnly>(line) {
            Ok(s) if s.seq == n => { n += 1; pos += nl + 1; }
            Ok(s) => return Err(bad(format!("seq {} found where {} was expected", s.seq, n))),
            // an unreadable line is a torn tail only if nothing follows it
            Err(_) if pos + nl + 1 == bytes.len() => break,
            Err(e) => return Err(bad(format!("unreadable line at seq {n}: {e}"))),
        }
    }
    Ok((pos, n))
}

pub struct Ledger { file: BufWriter<File>, next_seq: u64, written: u64, durable: Option<u64>, path: PathBuf }

impl Ledger {
    /// Opens (or creates) a ledger for appending, after verifying it and cutting a torn tail.
    pub fn open(path: impl AsRef<Path>) -> io::Result<(Ledger, OpenReport)> {
        let path = path.as_ref().to_path_buf();
        let mut f = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path)?;
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes)?;
        let (valid, n) = scan(&bytes)?;
        let cut = (bytes.len() - valid) as u64;
        if cut > 0 { f.set_len(valid as u64)?; f.sync_data()?; }
        drop(f);
        let file = OpenOptions::new().append(true).open(&path)?;
        let durable = n.checked_sub(1);
        Ok((Ledger { file: BufWriter::with_capacity(1 << 16, file), next_seq: n, written: n, durable, path }, OpenReport { entries: n, cut_bytes: cut }))
    }

    /// Buffers one fact and returns its seq. Not durable until `sync`.
    pub fn append<T: Serialize>(&mut self, s: &Stamped<T>) -> io::Result<u64> {
        let seq = self.next_seq;
        serde_json::to_writer(&mut self.file, &Line { seq, stamp: s.stamp, fact: &s.fact })?;
        self.file.write_all(b"\n")?;
        self.next_seq += 1;
        Ok(seq)
    }

    /// Writes the buffer and fsyncs; returns the last durable seq.
    pub fn sync(&mut self) -> io::Result<Option<u64>> {
        if self.next_seq > self.written {
            self.file.flush()?;
            self.file.get_ref().sync_data()?;
            self.written = self.next_seq;
            self.durable = self.next_seq.checked_sub(1);
        }
        Ok(self.durable)
    }

    pub fn durable_seq(&self) -> Option<u64> { self.durable }
    pub fn pending(&self) -> u64 { self.next_seq - self.written }
    pub fn path(&self) -> &Path { &self.path }
}

/// Reads a whole ledger, strictly: a torn tail is ignored (it was never acknowledged), any other damage is an error.
pub fn read_all<T: DeserializeOwned>(path: impl AsRef<Path>) -> io::Result<Vec<Entry<T>>> {
    let bytes = std::fs::read(path)?;
    let (valid, _) = scan(&bytes)?;
    bytes[..valid].split(|&b| b == b'\n').filter(|l| !l.is_empty())
        .map(|l| serde_json::from_slice(l).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)))
        .collect()
}

/// When the writer thread fsyncs: after `max_pending` facts or `max_delay`, whichever comes first.
#[derive(Clone, Copy)]
pub struct SyncPolicy { pub max_pending: u64, pub max_delay: Duration }
impl Default for SyncPolicy { fn default() -> Self { SyncPolicy { max_pending: 64, max_delay: Duration::from_millis(50) } } }

/// Last durable seq + 1 (0 = nothing durable yet), readable from any thread without locking.
pub type Durable = Arc<AtomicU64>;

/// Moves the ledger to its own thread. Sending never blocks the caller. The thread ends, after a final fsync,
/// when every sender is dropped; an I/O error ends it and is returned by the join handle.
pub fn spawn_writer<T: Serialize + Send + 'static>(mut ledger: Ledger, policy: SyncPolicy) -> (Sender<Stamped<T>>, Durable, std::thread::JoinHandle<io::Result<()>>) {
    let (tx, rx): (Sender<Stamped<T>>, Receiver<Stamped<T>>) = std::sync::mpsc::channel();
    let durable: Durable = Arc::new(AtomicU64::new(ledger.durable_seq().map_or(0, |s| s + 1)));
    let d = durable.clone();
    let h = std::thread::Builder::new().name("ledger".into()).spawn(move || -> io::Result<()> {
        let mut since = Instant::now();
        loop {
            let left = policy.max_delay.saturating_sub(since.elapsed());
            let done = match rx.recv_timeout(left) {
                Ok(s) => { ledger.append(&s)?; false }
                Err(RecvTimeoutError::Timeout) => false,
                Err(RecvTimeoutError::Disconnected) => true,
            };
            if done || ledger.pending() >= policy.max_pending || since.elapsed() >= policy.max_delay {
                if let Some(seq) = ledger.sync()? { d.store(seq + 1, Ordering::Release); }
                since = Instant::now();
            }
            if done { return Ok(()); }
        }
    })
    .expect("spawn ledger thread");
    (tx, durable, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct Fact { text: String, n: u64 }

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("ledger-{}-{}-{name}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::remove_file(&p);
        p
    }
    fn st(i: u64, f: Fact) -> Stamped<Fact> { Stamped { stamp: Stamp { mono_ns: i * 7, wall_ns: u64::MAX - i }, fact: f } }

    fn write(path: &Path, facts: &[Fact]) {
        let (mut l, _) = Ledger::open(path).unwrap();
        for (i, f) in facts.iter().enumerate() { l.append(&st(i as u64, f.clone())).unwrap(); }
        l.sync().unwrap();
    }

    proptest! {
        // roundtrip: what is read back is exactly what was written, in order, with consecutive seq
        #[test]
        fn roundtrip(facts in proptest::collection::vec((any::<String>(), any::<u64>()).prop_map(|(text, n)| Fact { text, n }), 0..40)) {
            let p = tmp("rt");
            write(&p, &facts);
            let back: Vec<Entry<Fact>> = read_all(&p).unwrap();
            prop_assert_eq!(back.len(), facts.len());
            for (i, (e, f)) in back.iter().zip(&facts).enumerate() {
                prop_assert_eq!(e.seq, i as u64);
                prop_assert_eq!(&e.fact, f);
                prop_assert_eq!(e.stamp, st(i as u64, f.clone()).stamp);
            }
            std::fs::remove_file(&p).unwrap();
        }

        // cut at any byte: reopening keeps exactly the complete lines before the cut, and appending continues the seq
        #[test]
        fn torn_tail_at_any_byte(facts in proptest::collection::vec((".{0,30}", any::<u64>()).prop_map(|(text, n)| Fact { text, n }), 1..15), cut in any::<prop::sample::Index>()) {
            let p = tmp("cut");
            write(&p, &facts);
            let full = std::fs::read(&p).unwrap();
            let at = cut.index(full.len() + 1);
            std::fs::write(&p, &full[..at]).unwrap();
            let complete = full[..at].iter().filter(|&&b| b == b'\n').count();
            let (mut l, rep) = Ledger::open(&p).unwrap();
            prop_assert_eq!(rep.entries as usize, complete);
            l.append(&st(99, Fact { text: "after".into(), n: 1 })).unwrap();
            l.sync().unwrap();
            let back: Vec<Entry<Fact>> = read_all(&p).unwrap();
            prop_assert_eq!(back.len(), complete + 1);
            for (i, e) in back.iter().enumerate() { prop_assert_eq!(e.seq, i as u64); }
            for (e, f) in back.iter().zip(&facts[..complete]) { prop_assert_eq!(&e.fact, f); }
            prop_assert_eq!(&back[complete].fact.text, "after");
            std::fs::remove_file(&p).unwrap();
        }
    }

    #[test]
    fn damage_before_the_tail_refuses_to_open() {
        let p = tmp("dmg");
        write(&p, &[Fact { text: "a".into(), n: 1 }, Fact { text: "b".into(), n: 2 }, Fact { text: "c".into(), n: 3 }]);
        let mut b = std::fs::read(&p).unwrap();
        let first_nl = b.iter().position(|&x| x == b'\n').unwrap();
        b[first_nl + 3] = b'#';
        std::fs::write(&p, &b).unwrap();
        assert_eq!(Ledger::open(&p).err().unwrap().kind(), io::ErrorKind::InvalidData);
        std::fs::remove_file(&p).unwrap();
    }

    #[test]
    fn seq_gap_refuses_to_open() {
        let p = tmp("gap");
        std::fs::write(&p, b"{\"seq\":0,\"mono_ns\":1,\"wall_ns\":1,\"fact\":null}\n{\"seq\":2,\"mono_ns\":1,\"wall_ns\":1,\"fact\":null}\n").unwrap();
        assert!(Ledger::open(&p).is_err());
        std::fs::remove_file(&p).unwrap();
    }

    #[test]
    fn writer_thread_syncs_and_reports_durable() {
        let p = tmp("thr");
        let (l, _) = Ledger::open(&p).unwrap();
        let (tx, durable, h) = spawn_writer::<Fact>(l, SyncPolicy { max_pending: 10, max_delay: Duration::from_millis(5) });
        for i in 0..25 { tx.send(st(i, Fact { text: i.to_string(), n: i })).unwrap(); }
        drop(tx);
        h.join().unwrap().unwrap();
        assert_eq!(durable.load(Ordering::Acquire), 25);
        assert_eq!(read_all::<Fact>(&p).unwrap().len(), 25);
        std::fs::remove_file(&p).unwrap();
    }
}
