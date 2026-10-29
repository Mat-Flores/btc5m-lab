//! `report` : synthèse ≤ 15 lignes du journal (JSONL). Les trois PnL viennent de la dernière ligne `state` ; les latences
//! de chaque segment, des lignes `trade` qui l'ont mesuré. Un segment jamais mesuré s'affiche « non mesuré ».
use serde_json::Value;
use std::collections::BTreeMap;

/// Segments, dans l'ordre : (libellé, horodatage de début, de fin) dans `lat`.
pub const SEGMENTS: [(&str, &str, &str); 5] = [("ordre suivi → observation", "src", "obs"), ("observation → décision", "obs", "dec"),
    ("décision → envoi", "dec", "sent"), ("envoi → accusé CLOB", "sent", "ack"), ("accusé → fill", "ack", "fill")];

/// Quantile au rang le plus proche (valeur observée, jamais interpolée).
fn q(v: &mut [f64], p: f64) -> f64 { v.sort_by(f64::total_cmp); v[((p * v.len() as f64).ceil() as usize).clamp(1, v.len()) - 1] }

pub fn report(journal: &str) -> String {
    let rows: Vec<Value> = journal.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let of = |t: &'static str| rows.iter().filter(move |r| r["t"] == t);
    let mut out = vec![];
    let start = of("start").last();
    let span = |k: &str| rows.iter().filter_map(|r| r["at"].as_u64()).fold(None, |a: Option<u64>, x| Some(if k == "min" { a.map_or(x, |a| a.min(x)) } else { a.map_or(x, |a| a.max(x)) }));
    let mins = match (span("min"), span("max")) { (Some(a), Some(b)) => (b - a) / 60_000, _ => 0 };
    out.push(format!("copy-trading — mode {} — wallet {} — paliers {} — {} min de journal",
        start.map_or("?".into(), |s| s["mode"].as_str().unwrap_or("?").to_string()), start.map_or("?".into(), |s| s["wallet"].as_str().unwrap_or("?").to_string()),
        start.map_or("?".into(), |s| s["tiers"].to_string()), mins));
    let trades: Vec<&Value> = of("trade").collect();
    let mut why: BTreeMap<String, usize> = BTreeMap::new();
    for t in trades.iter().filter(|t| t["decision"] == "skip") { *why.entry(t["reason"].as_str().unwrap_or("?").to_string()).or_default() += 1; }
    let n = |f: &dyn Fn(&&&Value) -> bool| trades.iter().filter(f).count();
    out.push(format!("trades vus {} (chaîne {}, secours data-api {}) ; copiés {} ; ignorés {}", trades.len(), n(&|t| t["src"] == "chain"), n(&|t| t["src"] == "data-api"),
        n(&|t| t["decision"] != "skip"), n(&|t| t["decision"] == "skip")));
    if !why.is_empty() { out.push(format!("  ignorés : {}", why.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", "))); }
    let checks: Vec<&Value> = of("check").collect();
    out.push(format!("contrôle data-api : {} fills concordants, {} écarts", checks.iter().filter(|c| c["ok"] == true).count(), checks.iter().filter(|c| c["ok"] == false).count()));
    match of("state").last() {
        Some(s) => for (k, name) in [("obs", "observé"), ("dem", "demandé"), ("real", "réel")] {
            let a = &s[k];
            out.push(format!("PnL {name:<8} {:+.2} $ (valeur {:.2} $, positions non valorisées : {})", a["pnl"].as_f64().unwrap_or(0.0), a["value"].as_f64().unwrap_or(0.0), a["unmarked"]));
        },
        None => out.push("PnL : aucune ligne state".into()),
    }
    out.push("latences (ms)            médiane      p90      n".into());
    for (label, a, b) in SEGMENTS {
        let mut v: Vec<f64> = trades.iter().filter_map(|t| Some(t["lat"][b].as_f64()? - t["lat"][a].as_f64()?)).collect();
        out.push(if v.is_empty() { format!("  {label:<28} non mesuré") } else { let k = v.len(); format!("  {label:<28} {:>7.0} {:>8.0} {k:>6}", q(&mut v, 0.5), q(&mut v, 0.9)) });
    }
    if let Some(h) = of("halt").last() { out.push(format!("ARRÊT : {}", h["reason"].as_str().unwrap_or("?"))); }
    out.truncate(15);
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_report_with_unmeasured_segments_named() {
        let j = [
            r#"{"t":"start","at":0,"mode":"paper","wallet":"0xf031","tiers":[[0.00001,0.03]]}"#,
            r#"{"t":"trade","at":1000,"src":"chain","decision":"buy","lat":{"src":0,"obs":1800,"dec":1840,"sent":null,"ack":null,"fill":null}}"#,
            r#"{"t":"trade","at":2000,"src":"chain","decision":"skip","reason":"sous le minimum","lat":{"src":0,"obs":2000,"dec":2030}}"#,
            r#"{"t":"trade","at":3000,"src":"data-api","decision":"skip","reason":"sous le minimum","lat":{"src":null,"obs":21000,"dec":21040}}"#,
            r#"{"t":"check","at":4000,"ok":true}"#,
            "ligne illisible",
            r#"{"t":"state","at":120000,"obs":{"pnl":0.5,"value":20.5,"unmarked":0},"dem":{"pnl":0.25,"value":20.25,"unmarked":0},"real":{"pnl":-0.1,"value":19.9,"unmarked":1}}"#,
        ].join("\n");
        let r = report(&j);
        assert!(r.lines().count() <= 15, "{r}");
        assert!(r.contains("2 min de journal") && r.contains("trades vus 3 (chaîne 2, secours data-api 1) ; copiés 1 ; ignorés 2"), "{r}");
        assert!(r.contains("sous le minimum 2") && r.contains("1 fills concordants, 0 écarts"), "{r}");
        assert!(r.contains("PnL réel     -0.10 $") && r.contains("non valorisées : 1"), "{r}");
        assert!(r.contains("ordre suivi → observation       1800     2000      2"), "{r}");
        assert!(r.contains("décision → envoi             non mesuré") && r.contains("accusé → fill                non mesuré"), "{r}");
    }
}
