// The core knows no source: no name, URL, host or native format of any venue may appear in core/src.
// Adding a source means adding an adapter and a configuration line, never touching the core.
const FORBIDDEN: &[&str] = &[
    "polymarket", "polybolt", "chainlink", "rtds", "okx",
    // venue addresses: the URL schemes themselves are generic transport, hostnames are not
    "gamma", "clob", ".com", ".exchange", "itode", "price_change", "bbo-tbt", "asset_id", "event_type",
    "taker_delay", "takerdelay",
];

/// A domain suffix counts only as part of a hostname ("x.com", not "[..complete]"); other words count anywhere.
fn found(line: &str, w: &str) -> bool {
    if !w.starts_with('.') { return line.contains(w); }
    line.match_indices(w).any(|(i, _)| {
        let before = line[..i].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric() || c == '-');
        let after = line[i + w.len()..].chars().next().is_none_or(|c| !c.is_ascii_alphanumeric());
        before && after
    })
}

#[test]
fn the_domain_rule_is_not_fooled() {
    assert!(found("connect to api.example.com now", ".com"));
    assert!(!found("&facts[..complete]", ".com"));
    assert!(!found("x.compute()", ".com"));
}

#[test]
fn core_sources_name_no_venue() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let mut hits = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let text = std::fs::read_to_string(&p).unwrap().to_lowercase();
        for (n, line) in text.lines().enumerate() {
            for w in FORBIDDEN { if found(line, w) { hits.push(format!("{}:{}: '{}'", p.display(), n + 1, w)); } }
        }
    }
    assert!(hits.is_empty(), "the core names a source:\n{}", hits.join("\n"));
}
