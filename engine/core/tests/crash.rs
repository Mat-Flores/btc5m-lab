// Phase 3 exit test: a writer process is killed (SIGKILL) in the middle of writing; reopening must find every fact
// it reported durable, byte-for-byte as generated, with consecutive seq, and must accept new appends.
use engine_core::clock::Stamp;
use engine_core::ledger::{read_all, spawn_writer, Entry, Ledger, Stamped, SyncPolicy};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Fact { i: u64, text: String }

// deterministic content, so the reader knows exactly what each seq must hold
fn fact(i: u64) -> Stamped<Fact> {
    Stamped { stamp: Stamp { mono_ns: i, wall_ns: 1_000_000 + i }, fact: Fact { i, text: format!("fait n°{i} \"guillemets\" \\ é 🚀 {}", "x".repeat((i % 50) as usize)) } }
}

/// Child mode: write forever through the writer thread, printing the durable count after each change.
#[test]
fn writer_child() {
    let Ok(path) = std::env::var("LEDGER_CHILD") else { return };
    let (l, rep) = Ledger::open(&path).unwrap();
    let (tx, durable, _h) = spawn_writer::<Fact>(l, SyncPolicy { max_pending: 64, max_delay: Duration::from_millis(5) });
    let mut i = rep.entries;
    let mut last = u64::MAX;
    loop {
        for _ in 0..50 { tx.send(fact(i)).unwrap(); i += 1; }
        let d = durable.load(Ordering::Acquire);
        if d != last { println!("DURABLE {d}"); last = d; }
        std::thread::sleep(Duration::from_micros(200));
    }
}

#[test]
fn killed_mid_write_then_exact_reread() {
    let dir = std::env::temp_dir().join(format!("ledger-crash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ledger.jsonl");
    let _ = std::fs::remove_file(&path);
    let exe = std::env::current_exe().unwrap();
    let mut total_checked = 0u64;
    for round in 0..20 {
        let mut child = Command::new(&exe).args(["writer_child", "--exact", "--nocapture", "--test-threads=1"])
            .env("LEDGER_CHILD", &path).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let out = BufReader::new(child.stdout.take().unwrap());
        let reader = std::thread::spawn(move || {
            let mut last = 0u64;
            for line in out.lines().map_while(Result::ok) { if let Some(d) = line.strip_prefix("DURABLE ") { last = d.parse().unwrap(); } }
            last
        });
        let jitter = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos() as u64 % 300;
        std::thread::sleep(Duration::from_millis(150 + jitter));
        child.kill().unwrap(); // SIGKILL: no flush, no destructor
        child.wait().unwrap();
        let durable = reader.join().unwrap();

        let (mut l, rep) = Ledger::open(&path).unwrap();
        assert!(rep.entries >= durable, "round {round}: {} entries but {durable} were reported durable", rep.entries);
        let back: Vec<Entry<Fact>> = read_all(&path).unwrap();
        assert_eq!(back.len() as u64, rep.entries);
        for e in &back { let want = fact(e.seq); assert_eq!(e.fact, want.fact); assert_eq!(e.stamp, want.stamp); }
        // appending after the crash continues the sequence
        l.append(&fact(rep.entries)).unwrap();
        l.sync().unwrap();
        drop(l);
        total_checked = rep.entries + 1;
        assert_eq!(read_all::<Fact>(&path).unwrap().len() as u64, total_checked);
    }
    assert!(total_checked > 1000, "too few facts written to mean anything: {total_checked}");
    println!("crash test: 20 kills, {total_checked} facts re-read exactly");
    std::fs::remove_dir_all(&dir).unwrap();
}
