// The maker port against the original simulator, bit for bit: `fixtures/maker/cases.json` holds what `simWindow`
// (engine/maker-bt/sim.mjs of btc5m-lag-sim, bound prudent, delay 0.15 s) produced on a synthetic window
// (`fixtures/maker/gen.mjs`). This test replays the same records through `Maker` with the simulator's loop and its
// prudent fill rule (test harness only: the simulated exchange lives in backtest/), then compares every float.
use engine_strategy::maker::{Act, Fee, Maker, Params};
use engine_strategy::model::jround;
use serde_json::Value;

const ONE_WAY: f64 = 12.2;
const DELAY_MS: f64 = 150.0;
const END: f64 = 300e3;

fn bits(v: &Value) -> f64 { f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap()) }
fn arr(v: &Value) -> Vec<f64> { v.as_array().unwrap().iter().map(bits).collect() }
fn same(a: f64, e: f64) -> bool { (a.is_nan() && e.is_nan()) || a.to_bits() == e.to_bits() }

/// Order live on a side at `t` (`liveAt`): the last change effective at or before it.
fn live_at(h: &[(f64, Option<usize>)], t: f64) -> Option<usize> { let i = h.partition_point(|x| x.0 <= t); if i == 0 { None } else { h[i - 1].1 } }

fn record(acts: &mut Vec<Act>, hist: &mut [Vec<(f64, Option<usize>)>; 2]) {
    for a in acts.drain(..) {
        match a {
            Act::Place { s, id, t, .. } => hist[s].push((t + ONE_WAY, Some(id))),
            Act::Cancel { s, t, .. } => hist[s].push((t + ONE_WAY, None)),
            Act::Exit { .. } => panic!("no exit in these cases"),
        }
    }
}

fn run(px: &[f64], cl: &[f64], book: &[f64], strat: &Value, budget: Option<f64>) -> Maker {
    let p: Params = serde_json::from_value(strat.clone()).unwrap();
    let mut m = Maker::new(p, Fee { rate: 0.07, exponent: 1.0, rebate_rate: 0.2 }, 0.01);
    let mut start = || Some(budget);
    let mut hist: [Vec<(f64, Option<usize>)>; 2] = [vec![], vec![]];
    let mut acts = vec![];
    let (n_ev, n_px, n_cl) = (book.len() / 5, px.len() / 2, cl.len() / 3);
    let (mut jb, mut jp, mut jc) = (0, 0, 0);
    loop {
        let tb = if jb < n_ev { book[5 * jb] } else { f64::INFINITY };
        let tp = if jp < n_px { px[2 * jp] } else { f64::INFINITY };
        let tc = if jc < n_cl { cl[3 * jc] } else { f64::INFINITY };
        let t = tb.min(tp).min(tc);
        if t > END + 2500.0 { break; }
        if t == tc {
            m.cl(t, cl[3 * jc + 1], cl[3 * jc + 2], &mut acts, &mut start);
            jc += 1;
        } else if t == tp {
            m.px(t, px[2 * jp + 1], &mut acts, &mut start);
            jp += 1;
        } else {
            let (ts, typ, price, size) = (book[5 * jb + 1], book[5 * jb + 2] as u8, book[5 * jb + 3], book[5 * jb + 4]);
            let step = m.book(t, typ, price, size, &mut acts, &mut start);
            record(&mut acts, &mut hist);
            // the prudent bound: a seller at our price fills us up to its size, for the order live at ts - delay
            let skip = typ == 0 || t == m.snap_t() || step.silent_before || t < 0.0 || ts > END + 2000.0;
            if !skip && !m.stopped {
                for s in 0..2 {
                    let Some(id) = live_at(&hist[s], ts - DELAY_MS) else { continue };
                    if m.orders[id].r <= 0.0 { continue; }
                    let qi = jround(m.orders[id].q * 1000.0) as i64;
                    let ba = m.book.best_ask(s);
                    if ba <= qi { let take = m.book.ask_size(s, ba); if take > 0.0 { m.filled(id, take, ts, true); } }
                }
            }
            m.after_book(step, &mut acts, &mut start);
            jb += 1;
        }
        record(&mut acts, &mut hist);
    }
    m.tick(END + 2500.0, &mut acts, &mut start);
    record(&mut acts, &mut hist);
    m.finish(Some(0));
    m
}

#[test]
fn decisions_and_fair_values_match_the_simulator_bit_for_bit() {
    let path = format!("{}/../fixtures/maker/cases.json", env!("CARGO_MANIFEST_DIR"));
    let c: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let (px, cl, book) = (arr(&c["px"]), arr(&c["cl"]), arr(&c["book"]));
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 7);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let e = &case["expect"];
        let m = run(&px, &cl, &book, &case["strat"], case["budget"].as_f64());
        let r = &m.rec;
        assert_eq!(r.why, e["why"].as_str(), "{name}: why");
        assert_eq!((r.posted, r.silent_skips, r.pulled), (e["posted"].as_u64().unwrap(), e["silentSkips"].as_u64().unwrap(), e["pulled"].as_u64().unwrap()), "{name}: counts");
        assert_eq!(r.capped.to_vec(), e["capped"].as_array().unwrap().iter().map(|v| v.as_bool().unwrap()).collect::<Vec<_>>(), "{name}: capped");
        assert_eq!(r.loser_filled_before, e["loserFilledBefore"].as_bool().unwrap(), "{name}");
        assert_eq!(r.cancel_loser_at.map(f64::to_bits), (!e["cancelLoserAt"].is_null()).then(|| bits(&e["cancelLoserAt"]).to_bits()), "{name}: cancelLoserAt");
        for (k, a) in [("sigma0", r.sigma0), ("spent", r.spent), ("rebates", r.rebates), ("pnl", r.pnl)] { assert!(same(a, bits(&e[k])), "{name}: {k} {a} vs {}", bits(&e[k])); }
        for (k, a) in [("drift", r.drift), ("posN", r.pos_n)] { for i in 0..2 { assert!(same(a[i], arr(&e[k])[i]), "{name}: {k}[{i}]"); } }
        let fair = arr(&e["fairSec"]);
        assert_eq!(fair.len(), m.fair_sec.len());
        for (i, (a, b)) in m.fair_sec.iter().zip(&fair).enumerate() { assert!(same(*a, *b), "{name}: fairSec[{i}] {a} vs {b}"); }
        let joins: Vec<Vec<f64>> = e["joins"].as_array().unwrap().iter().map(arr).collect();
        assert_eq!(r.joins.len(), joins.len(), "{name}: joins");
        for (a, b) in r.joins.iter().zip(&joins) { for i in 0..3 { assert!(same(a[i], b[i]), "{name}: join {a:?} vs {b:?}"); } }
        for s in 0..2 {
            let fills: Vec<Vec<f64>> = e["fills"][s].as_array().unwrap().iter().map(arr).collect();
            assert_eq!(r.fills[s].len(), fills.len(), "{name}: fills of side {s}");
            for (a, b) in r.fills[s].iter().zip(&fills) { for i in 0..4 { assert!(same(a[i], b[i]), "{name}: fill {a:?} vs {b:?}"); } }
        }
    }
}

/// `js_exp` gives V8's bits (`Math.exp` of Node 24, fixtures/maker/js-exp.txt); the platform's `exp` does not always.
#[test]
fn js_exp_is_v8_math_exp() {
    let path = format!("{}/../fixtures/maker/js-exp.txt", env!("CARGO_MANIFEST_DIR"));
    let (mut n, mut platform_differs) = (0, 0);
    for l in std::fs::read_to_string(path).unwrap().lines().filter(|l| !l.starts_with('#')) {
        let (x, e) = (f64::from_bits(u64::from_str_radix(&l[..16], 16).unwrap()), f64::from_bits(u64::from_str_radix(&l[16..], 16).unwrap()));
        assert!(same(engine_strategy::model::js_exp(x), e), "exp({x}): {} vs {e}", engine_strategy::model::js_exp(x));
        if !same(x.exp(), e) { platform_differs += 1; }
        n += 1;
    }
    assert_eq!(n, 410);
    eprintln!("f64::exp differs from Math.exp on {platform_differs} of {n} values");
}
