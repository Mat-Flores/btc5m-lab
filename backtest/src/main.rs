//! backtest — the engine's strategies on recorded data, with the simulated exchange (README.md).
//!   backtest run      [--config config/default.json] [--override f.json]... [--batch lot.json] [--windows a,b] [--export out.jsonl]
//!   backtest validate [--config ...] [--override ...] [--windows a,b] [--export out.jsonl]
//!   backtest compare  <oracle.jsonl> <ours.jsonl>
use backtest::config::Config;
use backtest::packs::{parse_utc, Source};
use std::io::Write;
use std::path::PathBuf;

struct Args { config: PathBuf, overrides: Vec<PathBuf>, batch: Option<PathBuf>, windows: Option<Vec<u64>>, export: Option<PathBuf>, rest: Vec<String> }

fn args() -> Result<(String, Args), String> {
    let mut it = std::env::args().skip(1);
    let cmd = it.next().ok_or("usage: backtest run|validate|compare …")?;
    let mut a = Args { config: PathBuf::from("config/default.json"), overrides: vec![], batch: None, windows: None, export: None, rest: vec![] };
    while let Some(x) = it.next() {
        let mut val = || it.next().ok_or(format!("{x}: value missing"));
        match x.as_str() {
            "--config" => a.config = val()?.into(),
            "--override" => a.overrides.push(val()?.into()),
            "--batch" => a.batch = Some(val()?.into()),
            "--windows" => a.windows = Some(val()?.split(',').map(|s| s.parse().map_err(|_| format!("--windows {s}"))).collect::<Result<_, _>>()?),
            "--export" => a.export = Some(val()?.into()),
            _ if x.starts_with("--") => return Err(format!("unknown option {x}")),
            _ => a.rest.push(x),
        }
    }
    Ok((cmd, a))
}

fn source(c: &Config) -> Source {
    let root = c.path(&c.pack.source_root);
    Source::new(root.join(&c.pack.data_dir), root.join(&c.pack.packs_dir))
}

fn export(a: &Args) -> Result<Option<Box<dyn Write>>, String> {
    a.export.as_ref().map(|p| std::fs::File::create(p).map(|f| Box::new(std::io::BufWriter::new(f)) as Box<dyn Write>).map_err(|e| format!("{}: {e}", p.display()))).transpose()
}

fn main() {
    if let Err(e) = real_main() { eprintln!("backtest: {e}"); std::process::exit(1); }
}

fn real_main() -> Result<(), String> {
    let (cmd, mut a) = args()?;
    match cmd.as_str() {
        "compare" => {
            let [o, u] = &a.rest[..] else { return Err("usage: backtest compare <oracle.jsonl> <ours.jsonl>".into()) };
            let (n, diffs) = backtest::compare::files(o.as_ref(), u.as_ref())?;
            for d in diffs.iter().take(20) { println!("{d}"); }
            println!("compare: {n} lines, {} differences", diffs.len());
            if !diffs.is_empty() { std::process::exit(1); }
            Ok(())
        }
        "validate" => {
            let c = Config::load(&a.config, &a.overrides, None)?;
            let src = source(&c);
            let wins = src.windows()?;
            let runs = c.runs()?;
            let (_, _, p, v) = runs.into_iter().next().ok_or("no run")?;
            let mut base: engine_strategy::maker::Params = maker_params(p)?;
            base.spot = "okx".into(); // validate.mjs: normalize({}), the packs' OKX
            let t0 = std::time::Instant::now();
            let mut ex = export(&a)?;
            let r = backtest::validate::run(&src, &wins, &base, &v, v.fill_lookback_ms / 1000.0, a.windows.as_deref(), ex.as_mut().map(|e| e.as_mut() as &mut dyn Write))?;
            for l in &r.lines { println!("{l}"); }
            println!("({} s)", t0.elapsed().as_secs());
            Ok(())
        }
        "run" => {
            let extra = a.batch.take().map(|b| batch_override(&b)).transpose()?;
            let c = Config::load(&a.config, &a.overrides, extra)?;
            let mut ex = export(&a)?;
            if c.input == "trace" { return backtest::run::run_traces(&c, ex.as_mut().map(|e| e.as_mut() as &mut dyn Write)); }
            let mut src = source(&c);
            let wins = src.windows()?;
            let from = c.period.from.as_deref().map(parse_utc).transpose()?.unwrap_or(0);
            let to = c.period.to.as_deref().map(parse_utc).transpose()?.unwrap_or(u64::MAX);
            let wins: Vec<_> = wins.into_iter().filter(|w| w.start >= from && w.start < to && a.windows.as_ref().is_none_or(|o| o.contains(&w.start))).collect();
            let specs = c.runs()?;
            let mut runs = vec![];
            for (_, _, p, v) in &specs { runs.push(backtest::batch::Run::new(maker_params(p.clone())?, v.clone(), c.capital.start_usd, c.capital.claim_s)); }
            let t0 = std::time::Instant::now();
            let n = wins.len();
            backtest::batch::run_many(&mut src, &wins, &mut runs, (c.pack.mix_latency_ms.perp, c.pack.mix_latency_ms.coinbase), c.market.horizon_s as f64,
                &mut |k, _| if (k + 1) % 500 == 0 { eprintln!("{} / {n} windows, {} s", k + 1, t0.elapsed().as_secs()); })?;
            let h = |v: f64| format!("{:016x}", v.to_bits());
            for ((id, var, _, _), r) in specs.iter().zip(&runs) {
                if let Some(e) = ex.as_mut() {
                    for w in &r.out {
                        let s = w.sum;
                        writeln!(e, "{}", serde_json::json!({ "id": id, "variant": var, "start": w.start, "why": w.why, "played": w.played, "eqBefore": h(w.eq_before), "eqAfter": h(w.eq_after), "pnl": h(w.pnl),
                            "spent": s.map(|s| h(s.0)), "payout": s.map(|s| h(s.1)), "proceeds": s.map(|s| h(s.2)), "fees": s.map(|s| h(s.3)), "sh": s.map(|s| [h(s.4[0]), h(s.4[1])]),
                            "nf": s.map(|s| s.5), "driftS": s.map(|s| h(s.6)), "driftN": s.map(|s| h(s.7)) })).map_err(|e| e.to_string())?;
                    }
                }
                let last = r.out.last().map_or(c.capital.start_usd, |w| w.eq_after);
                println!("{id}{}: {} windows, played {}, final {} ({})", if var.is_empty() { String::new() } else { format!(" [{var}]") }, r.out.len(), r.out.iter().filter(|w| w.played).count(), backtest::validate::to_fixed(last, 4), h(last));
            }
            println!("{} windows, {} runs: {} s", n, runs.len(), t0.elapsed().as_secs());
            Ok(())
        }
        _ => Err(format!("unknown command {cmd}")),
    }
}

/// The maker parameters of a run: what the strategy file, `strategy.base` and the candidate gave, without the keys
/// of the engine's registry (sources, capital, keep, horizon), which the pack runner does not use.
fn maker_params(mut p: serde_json::Map<String, serde_json::Value>) -> Result<engine_strategy::maker::Params, String> {
    for k in ["sources", "capital0", "claim", "keepMs", "horizonS"] { p.remove(k); }
    serde_json::from_value(serde_json::Value::Object(p)).map_err(|e| format!("maker: {e}"))
}

/// A batch of the old format (`engine/maker-bt/batches/*.json`: name, note, base, candidates) as an override: its
/// base REPLACES `strategy.base` (an old base was over sim.mjs DEFAULTS, i.e. the strategy file), its candidates the list.
fn batch_override(b: &PathBuf) -> Result<serde_json::Value, String> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(b).map_err(|e| format!("{}: {e}", b.display()))?).map_err(|e| format!("{}: {e}", b.display()))?;
    let base = v["base"].as_object().filter(|m| !m.is_empty()).map_or(serde_json::Value::Null, |m| serde_json::Value::Object(m.clone()));
    Ok(serde_json::json!({ "strategy": { "base": base, "candidates": v["candidates"] } }))
}
