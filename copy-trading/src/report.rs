//! Rapport JSONL de copy-trading, rendu comme un tableau de bord de terminal.
use serde_json::Value;
use std::collections::BTreeMap;

pub const SEGMENTS: [(&str, &str, &str); 5] = [
    ("ordre suivi → observation", "src", "obs"),
    ("observation → décision", "obs", "dec"),
    ("décision → envoi", "dec", "sent"),
    ("envoi → accusé CLOB", "sent", "ack"),
    ("accusé → fill", "ack", "fill"),
];

const INNER_W: usize = 82;
const BAR_W: usize = 20;
const TRADE_BAR_W: usize = 28;
const LATENCY_W: [usize; 4] = [27, 8, 8, 5];
const ACCOUNT_W: [usize; 6] = [14, 9, 8, 9, 9, 12];
const COPY_W: [usize; 9] = [8, 4, 5, 5, 6, 7, 7, 8, 8];
const SEP: &str = " │ ";
const MAX_REASONS: usize = 3;
const MAX_REASON_CHARS: usize = 24;
const MAX_COPY_ROWS: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tint {
    Plain,
    Cyan,
    Green,
    Red,
    Yellow,
    Orange,
    Magenta,
    Dim,
    Gray,
}

impl Tint {
    fn sgr(self) -> &'static str {
        match self {
            Self::Plain => "",
            Self::Cyan => "36",
            Self::Green => "32",
            Self::Red => "31",
            Self::Yellow => "33",
            Self::Orange => "38;5;208",
            Self::Magenta => "35",
            Self::Dim => "2",
            Self::Gray => "90",
        }
    }
}

#[derive(Clone)]
struct Segment {
    text: String,
    tint: Tint,
    bold: bool,
}

impl Segment {
    fn width(&self) -> usize {
        self.text.chars().count()
    }

    fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
}

fn text(value: impl Into<String>, tint: Tint) -> Segment {
    Segment {
        text: value.into(),
        tint,
        bold: false,
    }
}

fn paint(segment: &Segment, color: bool) -> String {
    if !color || segment.text.is_empty() {
        return segment.text.clone();
    }
    let mut params = Vec::with_capacity(2);
    if segment.bold {
        params.push("1");
    }
    if !segment.tint.sgr().is_empty() {
        params.push(segment.tint.sgr());
    }
    if params.is_empty() {
        segment.text.clone()
    } else {
        format!("\x1b[{}m{}\x1b[0m", params.join(";"), segment.text)
    }
}

fn take_chars(value: &str, count: usize) -> String {
    value.chars().take(count).collect()
}

fn fit_left(value: &str, width: usize) -> String {
    let count = value.chars().count();
    if count <= width {
        format!("{value}{}", " ".repeat(width - count))
    } else if width == 0 {
        String::new()
    } else {
        format!("{}…", take_chars(value, width - 1))
    }
}

fn fit_right(value: &str, width: usize) -> String {
    let count = value.chars().count();
    if count <= width {
        format!("{}{value}", " ".repeat(width - count))
    } else if width == 0 {
        String::new()
    } else {
        format!("{}…", take_chars(value, width - 1))
    }
}

fn fit_number(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        format!("{}{value}", " ".repeat(width - value.chars().count()))
    } else {
        "#".repeat(width)
    }
}

fn left(value: &str, width: usize, tint: Tint) -> Segment {
    text(fit_left(value, width), tint)
}

fn right(value: &str, width: usize, tint: Tint) -> Segment {
    text(fit_right(value, width), tint)
}

fn number(value: &str, width: usize, tint: Tint) -> Segment {
    text(fit_number(value, width), tint)
}

fn fit_segments(segments: Vec<Segment>, width: usize) -> Vec<Segment> {
    let total: usize = segments.iter().map(Segment::width).sum();
    if total <= width {
        let mut result = segments;
        result.push(text(" ".repeat(width - total), Tint::Plain));
        return result;
    }

    if width == 0 {
        return Vec::new();
    }
    let mut remaining = width - 1;
    let mut result = Vec::with_capacity(segments.len() + 1);
    let mut last_style = (Tint::Plain, false);
    for segment in segments {
        if remaining == 0 {
            break;
        }
        last_style = (segment.tint, segment.bold);
        let size = segment.width();
        if size <= remaining {
            remaining -= size;
            result.push(segment);
        } else {
            result.push(Segment {
                text: take_chars(&segment.text, remaining),
                ..segment
            });
            remaining = 0;
        }
    }
    result.push(Segment {
        text: "…".into(),
        tint: last_style.0,
        bold: last_style.1,
    });
    result
}

fn join_cells(cells: Vec<Segment>) -> Vec<Segment> {
    let mut result = Vec::with_capacity(cells.len() * 2);
    for (index, cell) in cells.into_iter().enumerate() {
        if index > 0 {
            result.push(text(SEP, Tint::Dim));
        }
        result.push(cell);
    }
    result
}

struct Canvas {
    color: bool,
    lines: Vec<String>,
}

impl Canvas {
    fn new(color: bool) -> Self {
        Self {
            color,
            lines: Vec::new(),
        }
    }

    fn emit(&mut self, segments: Vec<Segment>) {
        self.lines.push(
            segments
                .iter()
                .map(|segment| paint(segment, self.color))
                .collect(),
        );
    }

    fn row(&mut self, segments: Vec<Segment>) {
        let mut line = vec![text("│ ", Tint::Dim)];
        line.extend(fit_segments(segments, INNER_W));
        line.push(text(" │", Tint::Dim));
        self.emit(line);
    }

    fn top(&mut self, title: &str, badge: Segment) {
        let badge_width = badge.width();
        let dash_width = INNER_W.saturating_sub(title.chars().count() + badge_width + 2);
        self.emit(vec![
            text("╭─", Tint::Dim),
            text(title, Tint::Cyan).bold(),
            text(" ", Tint::Dim),
            text("─".repeat(dash_width), Tint::Dim),
            text(" ", Tint::Dim),
            badge,
            text("─╮", Tint::Dim),
        ]);
    }

    fn section(&mut self, title: &str) {
        let dash_width = INNER_W.saturating_sub(title.chars().count() + 1);
        self.emit(vec![
            text("├─ ", Tint::Dim),
            text(title, Tint::Cyan).bold(),
            text(" ", Tint::Dim),
            text("─".repeat(dash_width), Tint::Dim),
            text("┤", Tint::Dim),
        ]);
    }

    fn bottom(&mut self) {
        self.emit(vec![text(
            format!("╰{}╯", "─".repeat(INNER_W + 2)),
            Tint::Dim,
        )]);
    }

    fn finish(self) -> String {
        self.lines.join("\n")
    }
}

fn is_safe(character: char) -> bool {
    character == ' '
        || character.is_ascii_graphic()
        || (('\u{00A1}'..='\u{024F}').contains(&character) && character != '\u{00AD}')
}

fn clean(value: &str, max_chars: usize) -> String {
    let safe: String = value
        .chars()
        .map(|c| if is_safe(c) { c } else { '?' })
        .collect();
    if safe.chars().count() > max_chars {
        format!("{}…", take_chars(&safe, max_chars.saturating_sub(1)))
    } else {
        safe
    }
}

fn short_wallet(wallet: &str) -> String {
    let length = wallet.chars().count();
    if length <= 12 {
        wallet.to_string()
    } else {
        let tail: String = wallet.chars().skip(length - 4).collect();
        format!("{}…{tail}", take_chars(wallet, 6))
    }
}

fn finite(value: &Value) -> Option<f64> {
    value.as_f64().filter(|number| number.is_finite())
}

fn rounded(value: f64, decimals: i32) -> Option<f64> {
    if !value.is_finite() {
        return None;
    }
    let scale = 10_f64.powi(decimals);
    let rounded = (value * scale).round() / scale + 0.0;
    rounded.is_finite().then_some(rounded)
}

fn sign_tint(value: Option<f64>) -> Tint {
    match value {
        Some(number) if number > 0.0 => Tint::Green,
        Some(number) if number < 0.0 => Tint::Red,
        Some(_) => Tint::Plain,
        None => Tint::Dim,
    }
}

fn latency_tint(ms: f64) -> Tint {
    if ms < 0.0 {
        Tint::Magenta
    } else if ms < 100.0 {
        Tint::Green
    } else if ms < 1_000.0 {
        Tint::Yellow
    } else if ms < 3_000.0 {
        Tint::Orange
    } else {
        Tint::Red
    }
}

fn duration_text(ms: u64) -> String {
    let minutes = ms / 60_000;
    if minutes >= 60 {
        format!("{} h {:02} min", minutes / 60, minutes % 60)
    } else {
        format!("{minutes} min")
    }
}

fn clock_text(ms: u64) -> String {
    let seconds = ms / 1_000;
    format!(
        "{:02}:{:02} UTC",
        (seconds / 3_600) % 24,
        (seconds / 60) % 60
    )
}

fn clock_hms(ms: u64) -> String {
    let seconds = ms / 1_000;
    format!(
        "{:02}:{:02}:{:02}",
        (seconds / 3_600) % 24,
        (seconds / 60) % 60,
        seconds % 60
    )
}

fn age_text(ms: u64) -> String {
    let seconds = ms / 1_000;
    if seconds < 60 {
        format!("{seconds} s")
    } else if seconds < 3_600 {
        format!("{} min", seconds / 60)
    } else if seconds < 86_400 {
        format!("{} h", seconds / 3_600)
    } else {
        format!("{} j", seconds / 86_400)
    }
}

fn split_cells(parts: [usize; 3], width: usize) -> [usize; 3] {
    let total = parts
        .iter()
        .fold(0usize, |sum, value| sum.saturating_add(*value));
    if total == 0 {
        return [0, 0, width];
    }
    let mut cells = [0usize; 3];
    for (index, part) in parts.iter().enumerate() {
        cells[index] = part.saturating_mul(width) / total;
    }
    let used: usize = cells.iter().sum();
    let biggest = (0..3).max_by_key(|index| parts[*index]).unwrap_or(0);
    cells[biggest] += width.saturating_sub(used);
    for (index, part) in parts.iter().enumerate() {
        if *part > 0 && cells[index] == 0 && cells[biggest] > 1 {
            cells[index] = 1;
            cells[biggest] -= 1;
        }
    }
    cells
}

fn spark(values: &[f64], width: usize) -> Option<String> {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if values.is_empty() || width == 0 {
        return None;
    }
    let picked: Vec<f64> = if values.len() <= width {
        values.to_vec()
    } else {
        let denominator = (width - 1).max(1);
        (0..width)
            .map(|index| values[index * (values.len() - 1) / denominator])
            .collect()
    };
    let (low, high) = picked
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), value| {
            (lo.min(*value), hi.max(*value))
        });
    let span = high - low;
    Some(
        picked
            .iter()
            .map(|value| {
                if !(span > f64::EPSILON) {
                    BLOCKS[3]
                } else {
                    BLOCKS[(((*value - low) / span * 7.0).round() as usize).min(7)]
                }
            })
            .collect(),
    )
}

fn quantile(sorted: &[f64], percentile: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = ((percentile * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    sorted.get(rank - 1).copied()
}

fn latencies(trades: &[&Value], from: &str, to: &str) -> Vec<f64> {
    let mut values: Vec<f64> = trades
        .iter()
        .filter_map(|trade| {
            let delta = finite(&trade["lat"][to])? - finite(&trade["lat"][from])?;
            delta.is_finite().then_some(delta)
        })
        .collect();
    values.sort_by(f64::total_cmp);
    values
}

fn count_segment(count: usize, tint: Tint) -> Segment {
    if count == 0 {
        text("0", Tint::Dim)
    } else {
        text(count.to_string(), tint).bold()
    }
}

fn percent_suffix(part: usize, total: usize) -> Segment {
    if total == 0 {
        text(" (NULL)", Tint::Dim)
    } else {
        text(
            format!(" ({:.0} %)", part as f64 * 100.0 / total as f64),
            Tint::Dim,
        )
    }
}

fn latency_row(label: &str, sorted: &[f64], bold: bool) -> Vec<Segment> {
    let count = sorted.len();
    let median = quantile(sorted, 0.5);
    let p90 = quantile(sorted, 0.9);
    let format_ms = |value: Option<f64>| match value.and_then(|number| rounded(number, 0)) {
        Some(number) => (format!("{number:.0}"), latency_tint(number)),
        None => ("NULL".into(), Tint::Dim),
    };
    let (median_text, median_tint) = format_ms(median);
    let (p90_text, p90_tint) = format_ms(p90);
    let style = |segment: Segment| if bold { segment.bold() } else { segment };
    let mut row = join_cells(vec![
        style(left(
            &format!("  {label}"),
            LATENCY_W[0],
            if count == 0 { Tint::Dim } else { Tint::Plain },
        )),
        style(number(&median_text, LATENCY_W[1], median_tint)),
        style(number(&p90_text, LATENCY_W[2], p90_tint)),
        style(number(
            &count.to_string(),
            LATENCY_W[3],
            if count == 0 { Tint::Dim } else { Tint::Gray },
        )),
    ]);
    row.push(text(SEP, Tint::Dim));
    if let (Some(median), Some(p90)) = (median, p90) {
        let cells = |value: f64| {
            if value <= 0.0 {
                1
            } else {
                ((value / 400.0).round() as usize).clamp(1, BAR_W)
            }
        };
        let median_cells = cells(median);
        let p90_cells = cells(p90).max(median_cells);
        row.push(text("█".repeat(median_cells), Tint::Cyan));
        row.push(text("▒".repeat(p90_cells - median_cells), Tint::Orange));
    }
    row
}

fn copy_margin(price: Option<f64>) -> Option<f64> {
    price
        .filter(|price| *price > 0.0 && *price <= 1.0)
        .and_then(|price| rounded((1.0 / price - 1.0) * 100.0, 1))
}

fn format_copy_margin(margin: f64) -> String {
    let magnitude = margin.abs();
    if magnitude >= 1_000_000.0 {
        format!("{margin:+.1e}%")
    } else if magnitude >= 1_000.0 {
        format!("{:+.1}k%", margin / 1_000.0)
    } else if magnitude >= 100.0 {
        format!("{margin:+.1}%")
    } else {
        format!("{margin:+.1} %")
    }
}

fn copy_fill(trade: &Value) -> Option<(f64, f64, f64)> {
    let shares = finite(&trade["fill_shares"])?;
    let usd = finite(&trade["fill_usd"])?;
    if shares <= 0.0 || usd < 0.0 {
        return None;
    }
    let price = rounded(usd / shares, 3)?;
    let usd = rounded(usd, 2)?;
    Some((shares, usd, price))
}

fn copy_row(trade: &Value) -> Vec<Segment> {
    let is_buy = trade["decision"] == "buy";
    let side = if is_buy { "BUY" } else { "SELL" };
    let side_tint = if is_buy { Tint::Cyan } else { Tint::Magenta };
    let at = trade["at"].as_u64().map(clock_hms);
    let his_price = finite(&trade["his_price"]).and_then(|price| rounded(price, 3));
    let fill = copy_fill(trade);
    let my_price = fill.map(|(_, _, price)| price);
    let difference = match (his_price, my_price) {
        (Some(his), Some(mine)) => rounded(mine - his, 3),
        _ => None,
    };
    let difference_tint = match (difference, is_buy) {
        (Some(value), true) if value > 0.0 => Tint::Red,
        (Some(value), true) if value < 0.0 => Tint::Green,
        (Some(value), false) if value > 0.0 => Tint::Green,
        (Some(value), false) if value < 0.0 => Tint::Red,
        (Some(_), _) => Tint::Plain,
        (None, _) => Tint::Dim,
    };
    let his_usd = finite(&trade["his_usd"]).and_then(|usd| rounded(usd, 2));
    let my_usd = fill.map(|(_, usd, _)| usd);
    let his_margin = if is_buy { copy_margin(his_price) } else { None };
    let my_margin = if is_buy { copy_margin(my_price) } else { None };
    let price_cell = |price: Option<f64>, tint| {
        number(
            &price.map_or_else(|| "NULL".into(), |price| format!("{price:.3}")),
            5,
            tint,
        )
    };
    let amount_cell = |amount: Option<f64>| {
        number(
            &amount.map_or_else(|| "NULL".into(), |amount| format!("{amount:.2}")),
            7,
            amount.map_or(Tint::Dim, |_| Tint::Plain),
        )
    };
    let margin_cell = |margin: Option<f64>| {
        number(
            &margin.map_or_else(|| "NULL".into(), format_copy_margin),
            COPY_W[7],
            margin.map_or(Tint::Dim, |margin| sign_tint(Some(margin))),
        )
    };
    join_cells(vec![
        left(at.as_deref().unwrap_or("NULL"), COPY_W[0], Tint::Gray),
        left(side, COPY_W[1], side_tint).bold(),
        price_cell(his_price, Tint::Plain),
        price_cell(
            my_price,
            if my_price.is_some() {
                Tint::Plain
            } else {
                Tint::Dim
            },
        ),
        number(
            &difference.map_or_else(|| "NULL".into(), |value| format!("{value:+.3}")),
            COPY_W[4],
            difference_tint,
        ),
        amount_cell(his_usd),
        amount_cell(my_usd),
        margin_cell(his_margin),
        margin_cell(my_margin),
    ])
}

/// Copié = un ordre est réellement parti (live) ou a été joué sur le carnet (paper). Une décision de vente sous le
/// minimum d'ordre n'exécute rien : elle compte avec les ignorés, motif compris.
fn executed(trade: &Value) -> bool {
    (trade["decision"] == "buy" || trade["decision"] == "sell")
        && (trade.get("fill_shares").is_some()
            || trade.get("order").is_some()
            || finite(&trade["lat"]["sent"]).is_some())
}

fn copy_summary(trades: &[&Value]) -> [Vec<Segment>; 2] {
    let copies: Vec<&Value> = trades.iter().copied().filter(|t| executed(t)).collect();
    let filled: Vec<(&Value, (f64, f64, f64))> = copies
        .iter()
        .filter_map(|trade| copy_fill(trade).map(|fill| (*trade, fill)))
        .collect();
    let price_differences: Vec<f64> = filled
        .iter()
        .filter_map(|(trade, (_, _, my_price))| {
            finite(&trade["his_price"]).map(|his_price| my_price - his_price)
        })
        .filter(|difference| difference.is_finite())
        .collect();
    let buy_fills: Vec<_> = filled
        .iter()
        .filter(|(trade, _)| trade["decision"] == "buy")
        .collect();
    let stakes: Vec<(f64, f64)> = buy_fills
        .iter()
        .filter_map(|(trade, (_, my_usd, _))| {
            finite(&trade["his_usd"])
                .filter(|his_usd| *his_usd > 0.0)
                .map(|his_usd| (his_usd, *my_usd))
        })
        .collect();
    let his_stake: f64 = stakes.iter().map(|(his_usd, _)| his_usd).sum();
    let my_stake: f64 = stakes.iter().map(|(_, my_usd)| my_usd).sum();
    // pondéré par les mises : parts totales / $ totaux − 1, ce que rapporterait l'ensemble si tout gagnait
    let weighted = |pairs: Vec<(f64, f64)>| {
        let (shares, usd) = pairs
            .iter()
            .fold((0.0, 0.0), |(s, u), (ps, pu)| (s + ps, u + pu));
        (usd > 0.0)
            .then(|| rounded((shares / usd - 1.0) * 100.0, 1))
            .flatten()
    };
    let his_margin = weighted(
        buy_fills
            .iter()
            // mêmes paris, mêmes mises (les miennes), à son prix : sinon ses grosses mises écrasent la comparaison
            .filter_map(|(trade, (_, usd, _))| {
                let price = finite(&trade["his_price"]).filter(|price| *price > 0.0)?;
                Some((usd / price, *usd))
            })
            .collect(),
    );
    let my_margin = weighted(
        buy_fills
            .iter()
            .map(|(_, (shares, usd, _))| (*shares, *usd))
            .collect(),
    );
    let average = |values: &[f64], decimals: usize| {
        if values.is_empty() {
            None
        } else {
            rounded(
                values.iter().sum::<f64>() / values.len() as f64,
                decimals as i32,
            )
        }
    };
    let display =
        |value: Option<f64>, format: fn(f64) -> String| value.map_or_else(|| "NULL".into(), format);
    let average_difference = average(&price_differences, 3);
    let stake_ratio = if his_stake > 0.0 {
        rounded(my_stake / his_stake * 100.0, 1)
    } else {
        None
    };
    let summary = vec![
        text("moyenne écart ", Tint::Gray),
        text(
            display(average_difference, |value| format!("{value:+.3}")),
            sign_tint(average_difference),
        )
        .bold(),
        text(" · mise ", Tint::Dim),
        text(
            display(stake_ratio, |value| format!("{value:.1} %")),
            Tint::Plain,
        )
        .bold(),
        text(" de la sienne · gain si gagné ", Tint::Dim),
        text(
            display(his_margin, |value| format!("{value:.1} %")),
            Tint::Gray,
        )
        .bold(),
        text(" → ", Tint::Dim),
        text(
            display(my_margin, |value| format!("{value:.1} %")),
            sign_tint(match (his_margin, my_margin) {
                (Some(his), Some(mine)) => Some(mine - his),
                _ => None,
            }),
        )
        .bold(),
    ];
    let unfilled = copies.len().saturating_sub(filled.len());
    let older = copies.len().saturating_sub(MAX_COPY_ROWS);
    let mut counts = vec![
        count_segment(copies.len(), Tint::Plain),
        text(" copiés · ", Tint::Gray),
        count_segment(filled.len(), Tint::Green),
        text(" remplis · ", Tint::Gray),
        count_segment(unfilled, Tint::Magenta),
        text(" NULL · écart = mon prix − son prix", Tint::Dim),
    ];
    if older > 0 {
        counts.extend([
            text(" · +", Tint::Dim),
            count_segment(older, Tint::Yellow),
            text(" anciens", Tint::Dim),
        ]);
    }
    [summary, counts]
}

fn account_row(
    name: &str,
    pnl: Option<f64>,
    value: Option<f64>,
    unmarked: Option<u64>,
    history: &[f64],
) -> Vec<Segment> {
    let pnl = pnl.and_then(|value| rounded(value, 2));
    let pnl_text = pnl.map_or_else(|| "NULL".into(), |value| format!("{value:+.2}"));
    let percentage = match (pnl, value) {
        (Some(pnl), Some(value)) if (value - pnl) > f64::EPSILON => {
            rounded(pnl / (value - pnl) * 100.0, 1)
        }
        _ => None,
    };
    let percentage_text =
        percentage.map_or_else(|| "NULL".into(), |value| format!("{value:+.1} %"));
    let value = value.and_then(|value| rounded(value, 2));
    let value_text = value.map_or_else(|| "NULL".into(), |value| format!("{value:.2}"));
    let (unmarked_text, unmarked_tint) = match unmarked {
        None => ("NULL".into(), Tint::Dim),
        Some(0) => ("0".into(), Tint::Dim),
        Some(count) => (count.to_string(), Tint::Yellow),
    };
    let trend = spark(history, ACCOUNT_W[5]);
    let mut pnl_cell = number(&pnl_text, ACCOUNT_W[1], sign_tint(pnl));
    if pnl.is_some() {
        pnl_cell = pnl_cell.bold();
    }
    let mut unmarked_cell = number(&unmarked_text, ACCOUNT_W[4], unmarked_tint);
    if matches!(unmarked, Some(count) if count > 0) {
        unmarked_cell = unmarked_cell.bold();
    }
    join_cells(vec![
        left(&format!("  {name}"), ACCOUNT_W[0], Tint::Plain),
        pnl_cell,
        number(&percentage_text, ACCOUNT_W[2], sign_tint(percentage)),
        number(
            &value_text,
            ACCOUNT_W[3],
            if value.is_some() {
                Tint::Plain
            } else {
                Tint::Dim
            },
        ),
        unmarked_cell,
        left(
            trend.as_deref().unwrap_or("NULL"),
            ACCOUNT_W[5],
            if trend.is_some() {
                sign_tint(pnl)
            } else {
                Tint::Dim
            },
        ),
    ])
}

/// Écart de PnL entre deux comptes, en $ et en % du capital de départ (`base`) : un % du PnL n'a pas de sens quand il
/// est proche de zéro.
fn slippage(label: &str, from: Option<f64>, to: Option<f64>, base: Option<f64>) -> Vec<Segment> {
    let mut result = vec![text(label, Tint::Gray)];
    match (from, to) {
        (Some(from), Some(to)) => {
            let delta = rounded(to - from, 2);
            let tint = sign_tint(delta);
            match delta {
                Some(delta) => result.push(text(format!("{delta:+.2}"), tint).bold()),
                None => result.push(text("NULL", Tint::Dim)),
            }
            let percentage = base
                .filter(|base| *base > f64::EPSILON)
                .and_then(|base| rounded((to - from) / base * 100.0, 1));
            match percentage {
                Some(percentage) => result.push(text(format!(" ({percentage:+.1} %)"), tint)),
                None => result.push(text(" (NULL)", Tint::Dim)),
            }
        }
        _ => result.push(text("NULL", Tint::Dim)),
    }
    result
}

pub fn report(journal: &str) -> String {
    let color = std::env::var_os("NO_COLOR").is_none()
        && std::env::var("COPY_REPORT_COLOR").map_or_else(
            |_| std::io::IsTerminal::is_terminal(&std::io::stdout()),
            |value| value == "1",
        );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok());
    render(journal, color, now)
}

fn env_bool(key: &str) -> Option<bool> {
    match std::env::var(key).ok()?.as_str() {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

fn render(journal: &str, color: bool, now_ms: Option<u64>) -> String {
    let mut rows: Vec<Value> = journal
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    // le run courant seulement : ses comptes repartent de zéro, les trades et la durée doivent faire de même
    let runs = rows.iter().filter(|row| row["t"] == "start").count();
    if let Some(last) = rows.iter().rposition(|row| row["t"] == "start") {
        rows.drain(..last);
    }
    let of = |kind: &'static str| rows.iter().filter(move |row| row["t"] == kind);
    let starts: Vec<&Value> = of("start").collect();
    let start = starts.last().copied();
    let halt = of("halt").last();
    let trades: Vec<&Value> = of("trade").collect();
    let checks: Vec<&Value> = of("check").collect();
    let states: Vec<&Value> = of("state").collect();
    let count =
        |predicate: &dyn Fn(&Value) -> bool| trades.iter().filter(|trade| predicate(trade)).count();
    let mut canvas = Canvas::new(color);

    let mode = start
        .and_then(|row| row["mode"].as_str())
        .map(|value| clean(value, 8))
        .filter(|value| !value.is_empty());
    let badge = match mode.as_deref() {
        Some("live") => text("● LIVE", Tint::Red).bold(),
        Some("paper") => text("● PAPER", Tint::Cyan).bold(),
        Some(other) => text(format!("● {}", other.to_uppercase()), Tint::Yellow).bold(),
        None => text("● NULL", Tint::Dim),
    };
    canvas.top("COPY-TRADING · RAPPORT", badge);

    let times: Vec<u64> = rows.iter().filter_map(|row| row["at"].as_u64()).collect();
    let duration = times
        .iter()
        .min()
        .zip(times.iter().max())
        .map(|(start, end)| duration_text(end.saturating_sub(*start)));
    let wallet = start
        .and_then(|row| row["wallet"].as_str())
        .map(|wallet| short_wallet(&clean(wallet, 64)))
        .filter(|wallet| !wallet.is_empty());
    let run = start.and_then(|row| match &row["run"] {
        Value::Number(number) => number.as_u64().map(|number| format!("#{number}")),
        Value::String(value) => Some(format!("#{}", clean(value, 8))).filter(|value| value != "#"),
        _ => Some(format!("#{runs}")).filter(|_| runs > 0),
    });
    let since = start.and_then(|row| row["at"].as_u64()).map(clock_text);
    let mode_tint = match mode.as_deref() {
        Some("live") => Tint::Orange,
        Some("paper") => Tint::Cyan,
        _ => Tint::Plain,
    };
    let optional = |value: Option<&String>, tint| match value {
        Some(value) => text(value.clone(), tint),
        None => text("NULL", Tint::Dim),
    };
    canvas.row(vec![
        text("wallet ", Tint::Dim),
        optional(wallet.as_ref(), Tint::Plain),
        text(" · ", Tint::Dim),
        text("mode ", Tint::Dim),
        mode.as_ref().map_or_else(
            || text("NULL", Tint::Dim),
            |mode| text(mode.clone(), mode_tint).bold(),
        ),
        text(" · ", Tint::Dim),
        text("durée ", Tint::Dim),
        optional(duration.as_ref(), Tint::Plain),
        text(" · ", Tint::Dim),
        text("run ", Tint::Dim),
        optional(run.as_ref(), Tint::Plain),
        text(" · ", Tint::Dim),
        text("depuis ", Tint::Dim),
        optional(since.as_ref(), Tint::Plain),
    ]);

    canvas.section("TRADES");
    let total = trades.len();
    let copied = count(&|trade| executed(trade));
    let skipped = count(&|trade| !executed(trade) && trade["decision"].is_string());
    let unknown = total.saturating_sub(skipped + copied);
    let [copied_cells, skipped_cells, unknown_cells] =
        split_cells([copied, skipped, unknown], TRADE_BAR_W);
    let mut trade_line = vec![
        text("vus ", Tint::Gray),
        text(total.to_string(), Tint::Plain).bold(),
        text("  ", Tint::Plain),
        text("█".repeat(copied_cells), Tint::Green),
        text("█".repeat(skipped_cells), Tint::Yellow),
        text("░".repeat(unknown_cells), Tint::Dim),
        text("  copiés ", Tint::Gray),
        count_segment(copied, Tint::Green),
        percent_suffix(copied, total),
        text("  ignorés ", Tint::Gray),
        count_segment(skipped, Tint::Yellow),
        percent_suffix(skipped, total),
    ];
    if unknown > 0 {
        trade_line.extend([
            text("  indét. ", Tint::Gray),
            count_segment(unknown, Tint::Magenta),
        ]);
    }
    canvas.row(trade_line);

    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for trade in trades
        .iter()
        .filter(|trade| !executed(trade) && trade["decision"].is_string())
    {
        let reason = trade["reason"]
            .as_str()
            .map(|reason| clean(reason, MAX_REASON_CHARS))
            .filter(|reason| !reason.is_empty())
            .unwrap_or_else(|| "NULL".into());
        *reasons.entry(reason).or_default() += 1;
    }
    let mut ranked: Vec<(&String, &usize)> = reasons.iter().collect();
    ranked.sort_by(|left, right| right.1.cmp(left.1).then(left.0.cmp(right.0)));
    let mut reason_line = vec![text("motifs  ", Tint::Dim)];
    if ranked.is_empty() {
        reason_line.push(text("NULL", Tint::Dim));
    }
    for (index, (reason, count)) in ranked.iter().take(MAX_REASONS).enumerate() {
        if index > 0 {
            reason_line.push(text(" · ", Tint::Dim));
        }
        reason_line.push(text(format!("{reason} "), Tint::Gray));
        reason_line.push(text(format!("×{count}"), Tint::Yellow).bold());
    }
    let hidden = ranked.len().saturating_sub(MAX_REASONS);
    if hidden > 0 {
        reason_line.push(text(" · ", Tint::Dim));
        reason_line.push(text(
            format!("+{hidden} {}", if hidden == 1 { "autre" } else { "autres" }),
            Tint::Dim,
        ));
    }
    canvas.row(reason_line);

    let chain = count(&|trade| trade["src"] == "chain");
    let api = count(&|trade| trade["src"] == "data-api");
    let other_source = total.saturating_sub(chain + api);
    let good = checks.iter().filter(|check| check["ok"] == true).count();
    let bad = checks.iter().filter(|check| check["ok"] == false).count();
    let unknown_checks = checks.len().saturating_sub(good + bad);
    let mut source_line = vec![
        text("source  ", Tint::Dim),
        text("chaîne ", Tint::Gray),
        count_segment(chain, Tint::Cyan),
        percent_suffix(chain, total),
        text(" · secours API ", Tint::Gray),
        count_segment(api, Tint::Yellow),
        percent_suffix(api, total),
    ];
    if other_source > 0 {
        source_line.extend([
            text(" · autre ", Tint::Gray),
            count_segment(other_source, Tint::Magenta),
        ]);
    }
    source_line.push(text(" · contrôle ", Tint::Gray));
    if checks.is_empty() {
        source_line.push(text("NULL", Tint::Dim));
    } else {
        source_line.extend([
            count_segment(good, Tint::Green),
            text(" ok · ", Tint::Gray),
            count_segment(bad, Tint::Red),
            text(if bad == 1 { " écart" } else { " écarts" }, Tint::Gray),
        ]);
        if unknown_checks > 0 {
            source_line.extend([
                text(" · ", Tint::Dim),
                count_segment(unknown_checks, Tint::Magenta),
                text(" indét.", Tint::Gray),
            ]);
        }
    }
    canvas.row(source_line);

    let has_latency = |key: &str| count(&|trade| finite(&trade["lat"][key]).is_some());
    let sent = has_latency("sent");
    let acknowledged = has_latency("ack");
    let filled = has_latency("fill");
    let outcomes: Vec<&Value> = trades
        .iter()
        .filter(|trade| trade["outcome"].is_string())
        .copied()
        .collect();
    let cancelled = outcomes
        .iter()
        .filter(|trade| trade["outcome"] == "cancelled")
        .count();
    let rejected = outcomes
        .iter()
        .filter(|trade| trade["outcome"] == "rejected")
        .count();
    let outcome_count = |value: usize| {
        if outcomes.is_empty() {
            text("NULL", Tint::Dim)
        } else {
            count_segment(value, Tint::Plain)
        }
    };
    let mut order_line = vec![
        text("ordres  ", Tint::Dim),
        text("envoyés ", Tint::Gray),
        count_segment(sent, Tint::Plain),
        text(" · acquittés ", Tint::Gray),
        count_segment(acknowledged, Tint::Plain),
        text(" · remplis ", Tint::Gray),
        count_segment(filled, Tint::Green),
    ];
    if sent > 0 {
        order_line.push(text(
            format!(" {:.0} %", filled as f64 * 100.0 / sent as f64),
            Tint::Green,
        ));
    } else {
        order_line.push(text(" (NULL)", Tint::Dim));
    }
    order_line.extend([
        text(" · annulés ", Tint::Gray),
        outcome_count(cancelled),
        text(" · rejetés ", Tint::Gray),
        outcome_count(rejected),
    ]);
    canvas.row(order_line);

    canvas.section("COMPTES (USD)");
    canvas.row(join_cells(vec![
        left("Compte", ACCOUNT_W[0], Tint::Cyan).bold(),
        right("PnL", ACCOUNT_W[1], Tint::Cyan).bold(),
        right("PnL %", ACCOUNT_W[2], Tint::Cyan).bold(),
        right("Valeur", ACCOUNT_W[3], Tint::Cyan).bold(),
        right("Non valo.", ACCOUNT_W[4], Tint::Cyan).bold(),
        left("Tendance", ACCOUNT_W[5], Tint::Cyan).bold(),
    ]));
    let pnl_of = |key: &str| states.last().and_then(|state| finite(&state[key]["pnl"]));
    for (key, name) in [("obs", "Observé"), ("dem", "Demandé"), ("real", "Réel")] {
        let history: Vec<f64> = states
            .iter()
            .filter_map(|state| finite(&state[key]["pnl"]))
            .collect();
        canvas.row(account_row(
            name,
            pnl_of(key),
            states.last().and_then(|state| finite(&state[key]["value"])),
            states
                .last()
                .and_then(|state| state[key]["unmarked"].as_u64()),
            &history,
        ));
    }
    let mut slippage_line = vec![text("glissement  ", Tint::Dim)];
    let capital = states
        .last()
        .and_then(|state| Some(finite(&state["real"]["value"])? - finite(&state["real"]["pnl"])?));
    slippage_line.extend(slippage(
        "demandé → réel ",
        pnl_of("dem"),
        pnl_of("real"),
        capital,
    ));
    slippage_line.push(text("  ·  ", Tint::Dim));
    slippage_line.extend(slippage(
        "observé → réel ",
        pnl_of("obs"),
        pnl_of("real"),
        capital,
    ));
    canvas.row(slippage_line);

    canvas.section("LATENCE (ms)");
    let mut latency_header = join_cells(vec![
        left("Segment", LATENCY_W[0], Tint::Cyan).bold(),
        right("Médiane", LATENCY_W[1], Tint::Cyan).bold(),
        right("P90", LATENCY_W[2], Tint::Cyan).bold(),
        right("N", LATENCY_W[3], Tint::Cyan).bold(),
    ]);
    latency_header.push(text(SEP, Tint::Dim));
    latency_header.push(left("Distribution", BAR_W, Tint::Cyan).bold());
    canvas.row(latency_header);
    for (label, from, to) in SEGMENTS {
        canvas.row(latency_row(label, &latencies(&trades, from, to), false));
    }
    canvas.row(latency_row(
        "total src → fill",
        &latencies(&trades, "src", "fill"),
        true,
    ));

    canvas.section("COPIES · LUI vs MOI");
    canvas.row(join_cells(vec![
        left("", COPY_W[0], Tint::Plain),
        left("", COPY_W[1], Tint::Plain),
        left(
            "PRIX (0-1)",
            COPY_W[2] + COPY_W[3] + COPY_W[4] + 6,
            Tint::Gray,
        )
        .bold(),
        left("MISE (USD)", COPY_W[5] + COPY_W[6] + 3, Tint::Gray).bold(),
        left("GAIN SI GAGNÉ", COPY_W[7] + COPY_W[8] + 3, Tint::Gray).bold(),
    ]));
    canvas.row(join_cells(vec![
        left("Heure", COPY_W[0], Tint::Cyan).bold(),
        left("Sens", COPY_W[1], Tint::Cyan).bold(),
        right("Lui", COPY_W[2], Tint::Cyan).bold(),
        right("Moi", COPY_W[3], Tint::Cyan).bold(),
        right("Écart", COPY_W[4], Tint::Cyan).bold(),
        right("Lui", COPY_W[5], Tint::Cyan).bold(),
        right("Moi", COPY_W[6], Tint::Cyan).bold(),
        right("Lui", COPY_W[7], Tint::Cyan).bold(),
        right("Moi", COPY_W[8], Tint::Cyan).bold(),
    ]));
    let copies: Vec<&Value> = trades.iter().copied().filter(|t| executed(t)).collect();
    let mut recent: Vec<&Value> = copies.iter().rev().take(MAX_COPY_ROWS).copied().collect();
    recent.reverse();
    for trade in recent {
        canvas.row(copy_row(trade));
    }
    let [copy_metrics, copy_counts] = copy_summary(&trades);
    canvas.row(copy_metrics);
    canvas.row(copy_counts);

    canvas.section("ÉTAT");
    let service = match std::env::var("COPY_REPORT_SERVICE").ok().as_deref() {
        Some("active") => text("● active", Tint::Green),
        Some("inactive") => text("● arrêtée", Tint::Yellow),
        Some("failed") => text("● échec", Tint::Red).bold(),
        _ if halt.is_some() => text("● arrêté", Tint::Red).bold(),
        _ => text("● NULL", Tint::Dim),
    };
    let live_pid = std::env::var("COPY_REPORT_LIVE_PID")
        .ok()
        .filter(|value| !value.is_empty());
    let run_status = match (mode.as_deref(), live_pid.as_deref()) {
        (_, Some(pid)) => text(format!("● live (PID {})", clean(pid, 10)), Tint::Orange),
        (Some(mode), None) => text(format!("● {mode}"), mode_tint),
        _ => text("● NULL", Tint::Dim),
    };
    let lock = env_bool("COPY_REPORT_LIVE_LOCK")
        .or_else(|| start.and_then(|row| row["live_lock"].as_bool()));
    let lock = match lock {
        Some(true) => text("● oui", Tint::Green),
        Some(false) => text("● non", Tint::Yellow),
        None => text("● NULL", Tint::Dim),
    };
    let last_trade_age = trades
        .iter()
        .filter_map(|trade| trade["at"].as_u64())
        .max()
        .zip(now_ms)
        .and_then(|(at, now)| now.checked_sub(at));
    let last_trade = last_trade_age.map_or_else(
        || text("NULL", Tint::Dim),
        |age| text(format!("il y a {}", age_text(age)), Tint::Gray),
    );
    canvas.row(vec![
        text("service ", Tint::Dim),
        service,
        text("   run ", Tint::Dim),
        run_status,
        text("   verrou live ", Tint::Dim),
        lock,
        text("   dernier trade ", Tint::Dim),
        last_trade,
    ]);
    if let Some(halt) = halt {
        let reason = halt["reason"].as_str().map(|reason| clean(reason, 60));
        canvas.row(vec![
            text("ARRÊT  ", Tint::Red).bold(),
            reason.map_or_else(|| text("NULL", Tint::Dim), |reason| text(reason, Tint::Red)),
        ]);
    }
    canvas.row(vec![
        text("█", Tint::Cyan),
        text(" médiane  ", Tint::Dim),
        text("▒", Tint::Orange),
        text(" jusqu'au P90   ", Tint::Dim),
        text("● ", Tint::Green),
        text("<100 ms  ", Tint::Dim),
        text("● ", Tint::Yellow),
        text("<1 s  ", Tint::Dim),
        text("● ", Tint::Orange),
        text("<3 s  ", Tint::Dim),
        text("● ", Tint::Red),
        text("≥3 s", Tint::Dim),
    ]);
    canvas.bottom();
    canvas.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_journal() -> String {
        [
            r#"{"t":"start","at":0,"mode":"live","wallet":"0xf0318c32136c2db7fec88b84869aee6a1106c80c","run":14,"live_lock":false}"#,
            r#"{"t":"trade","at":1000,"src":"chain","decision":"buy","lat":{"src":0,"obs":600,"dec":640,"sent":680,"ack":900,"fill":2210}}"#,
            r#"{"t":"trade","at":2000,"src":"chain","decision":"skip","reason":"sous le premier palier","lat":{"src":0,"obs":700,"dec":730}}"#,
            r#"{"t":"trade","at":3000,"src":"data-api","decision":"skip","reason":"sous le premier palier","lat":{"src":null,"obs":21000,"dec":21040}}"#,
            r#"{"t":"trade","at":4000,"src":"chain","decision":"skip","reason":"trop ancien","lat":{"src":0,"obs":800,"dec":830}}"#,
            r#"{"t":"check","at":4100,"ok":true}"#,
            r#"{"t":"check","at":4200,"ok":false}"#,
            r#"{"t":"state","at":60000,"obs":{"pnl":1.0,"value":101.0,"unmarked":0},"dem":{"pnl":0.5,"value":100.5,"unmarked":0},"real":{"pnl":0.3,"value":100.3,"unmarked":0}}"#,
            r#"{"t":"state","at":120000,"obs":{"pnl":2.0,"value":102.0,"unmarked":0},"dem":{"pnl":1.0,"value":101.0,"unmarked":0},"real":{"pnl":-0.1,"value":99.9,"unmarked":1}}"#,
        ].join("\n")
    }

    fn hostile_journal() -> String {
        [
            r#"{"t":"start","at":18446744073709551615,"mode":"\u001b[31mliv\u00e9\n","wallet":"\u6f22\u5b57\u001b[0m0123456789abcdef0123456789abcdef0123456789","run":"x\ty","live_lock":"peut-être"}"#,
            r#"{"t":"trade","at":1e300,"src":null,"decision":42,"lat":{"src":1e300,"obs":-1e300}}"#,
            r#"{"t":"trade","at":5,"src":"inconnu","decision":"skip","reason":"\u6f22\u5b57 une raison vraiment beaucoup trop longue\r\n","lat":{"src":1e308,"obs":-1e308,"fill":"x"}}"#,
            r#"{"t":"trade","decision":"buy","outcome":"rejected","lat":"oups"}"#,
            r#"{"t":"check","ok":"oui"}"#,
            r#"{"t":"state","at":2,"obs":{"pnl":-1e308,"value":1e308}}"#,
            r#"{"t":"state","at":1,"obs":{"pnl":1e300,"value":1e300,"unmarked":18446744073709551615},"dem":"oups","real":null}"#,
            r#"{"t":"halt","reason":"arr\u00eat\n\u001b[2J"}"#,
            "42",
            "[1,2,3]",
        ].join("\n")
    }

    fn strip_ansi(value: &str) -> String {
        let mut output = String::new();
        let mut escape = false;
        for character in value.chars() {
            if escape {
                escape = character != 'm';
            } else if character == '\x1b' {
                escape = true;
            } else {
                output.push(character);
            }
        }
        output
    }

    fn assert_aligned(output: &str) {
        for (index, line) in output.lines().enumerate() {
            assert_eq!(
                strip_ansi(line).chars().count(),
                INNER_W + 4,
                "ligne {index}: {line:?}"
            );
        }
    }

    #[test]
    fn full_report_has_fixed_width_and_expected_metrics() {
        let output = render(&sample_journal(), false, None);
        assert_eq!(output.lines().count(), 31, "{output}");
        assert_aligned(&output);
        for expected in [
            "COPY-TRADING · RAPPORT",
            "● LIVE",
            "wallet 0xf031…c80c · mode live · durée 2 min · run #14 · depuis 00:00 UTC",
            "vus 4",
            "copiés 1",
            "ignorés 3",
            "sous le premier palier",
            "chaîne 3",
            "secours API 1",
            "contrôle",
            "envoyés 1",
            "acquittés 1",
            "remplis 1",
            "  Observé      │     +2.00 │   +2.0 % │    102.00 │         0 │ ▁█",
            "demandé → réel -1.10 (-1.1 %)",
            "  ordre suivi → observation │      700 │      800 │     3 │ ██",
            "  total src → fill",
            "verrou live ● non",
        ] {
            assert!(
                output.contains(expected),
                "manque {expected:?} dans\n{output}"
            );
        }
    }

    #[test]
    fn empty_report_is_null_safe_and_aligned() {
        let output = render("", false, None);
        assert_eq!(output.lines().count(), 30, "{output}");
        assert_aligned(&output);
        for expected in [
            "● NULL",
            "wallet NULL · mode NULL · durée NULL · run NULL · depuis NULL",
            "vus 0",
            "motifs  NULL",
            "contrôle NULL",
            "  Observé      │      NULL │     NULL │      NULL │      NULL │ NULL",
            "  décision → envoi",
            "service ● NULL",
            "dernier trade NULL",
        ] {
            assert!(
                output.contains(expected),
                "manque {expected:?} dans\n{output}"
            );
        }
    }

    #[test]
    fn hostile_journal_cannot_break_frame_or_emit_controls() {
        let output = render(&hostile_journal(), true, Some(u64::MAX));
        assert_aligned(&output);
        let plain = strip_ansi(&output);
        assert!(
            !plain
                .chars()
                .any(|character| character.is_control() && character != '\n'),
            "{plain}"
        );
        assert!(plain.contains("#########"), "{plain}");
    }

    #[test]
    fn ansi_colors_do_not_change_visible_alignment() {
        let colored = render(&sample_journal(), true, None);
        let plain = render(&sample_journal(), false, None);
        assert_aligned(&colored);
        assert_eq!(strip_ansi(&colored), plain);
        assert!(colored.contains("\x1b[1;36m"));
    }

    #[test]
    fn last_trade_age_uses_the_supplied_clock() {
        let output = render(&sample_journal(), false, Some(16_000));
        assert!(output.contains("dernier trade il y a 12 s"), "{output}");
        let output = render(&sample_journal(), false, Some(10));
        assert!(output.contains("dernier trade NULL"), "{output}");
    }

    #[test]
    fn halt_is_reported_in_a_framed_row() {
        let journal = format!(
            "{}\n{}",
            sample_journal(),
            r#"{"t":"halt","at":130000,"reason":"limite atteinte"}"#
        );
        let output = render(&journal, false, None);
        assert_eq!(output.lines().count(), 32, "{output}");
        assert_aligned(&output);
        assert!(output.contains("ARRÊT"), "{output}");
        assert!(output.contains("● arrêté"), "{output}");
    }

    #[test]
    fn copies_section_compares_actual_fills_and_keeps_unfilled_values_null() {
        let journal = [
            r#"{"t":"trade","at":50502000,"decision":"buy","side":"BUY","his_price":0.52,"his_usd":1250.0,"fill_shares":47.16981132,"fill_usd":25.0}"#,
            r#"{"t":"trade","at":52000000,"decision":"buy","side":"BUY","his_price":0.48,"his_usd":900.0,"fill_shares":0.0,"fill_usd":0.0}"#,
            r#"{"t":"trade","at":53000000,"decision":"skip","his_price":0.6,"his_usd":50.0}"#,
        ]
        .join("\n");
        let output = render(&journal, false, None);
        assert_aligned(&output);
        for expected in [
            "COPIES · LUI vs MOI",
            "Heure    │ Sens │   Lui │   Moi │  Écart │     Lui │     Moi │      Lui │      Moi",
            "14:01:42 │ BUY  │ 0.520 │ 0.530 │ +0.010 │ 1250.00 │   25.00 │  +92.3 % │  +88.7 %",
            "14:26:40 │ BUY  │ 0.480 │  NULL │   NULL │  900.00 │    NULL │  +108.3% │     NULL",
            "moyenne écart +0.010 · mise 2.0 % de la sienne · gain si gagné 92.3 % → 88.7 %",
            "2 copiés · 1 remplis · 1 NULL",
        ] {
            assert!(
                output.contains(expected),
                "manque {expected:?} dans\n{output}"
            );
        }
    }

    #[test]
    fn only_the_last_run_counts_and_unexecuted_sells_are_ignored() {
        let journal = [
            r#"{"t":"start","at":0,"mode":"paper"}"#,
            r#"{"t":"trade","at":10,"decision":"buy","his_price":0.5,"his_usd":10.0,"fill_shares":2.0,"fill_usd":1.0}"#,
            r#"{"t":"start","at":60000,"mode":"paper"}"#,
            r#"{"t":"trade","at":61000,"decision":"sell","reason":"réel sous le minimum","his_price":0.2,"his_usd":5.0}"#,
            r#"{"t":"trade","at":62000,"decision":"buy","his_price":0.5,"his_usd":10.0,"fill_shares":2.0,"fill_usd":1.0}"#,
        ]
        .join("\n");
        let output = render(&journal, false, None);
        assert_aligned(&output);
        for expected in [
            "vus 2",
            "copiés 1",
            "ignorés 1",
            "réel sous le minimum ×1",
            "run #2",
            "1 copiés · 1 remplis · 0 NULL",
        ] {
            assert!(
                output.contains(expected),
                "manque {expected:?} dans\n{output}"
            );
        }
        assert!(!output.contains("│ SELL"), "{output}");
    }

    #[test]
    fn copies_section_limits_rows_to_the_six_most_recent_copies() {
        let journal = (0..8)
            .map(|index| {
                format!(
                    r#"{{"t":"trade","at":{},"decision":"buy","his_price":0.5,"his_usd":10.0,"fill_shares":2.0,"fill_usd":1.0}}"#,
                    index * 1_000
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let output = render(&journal, false, None);
        assert!(output.contains("8 copiés"), "{output}");
        assert!(output.contains("+2 anciens"), "{output}");
        assert!(!output.contains("00:00:00 │ BUY"), "{output}");
        assert!(output.contains("00:00:02 │ BUY"), "{output}");
        assert!(output.contains("00:00:07 │ BUY"), "{output}");
        assert_aligned(&output);
    }

    #[test]
    fn helper_functions_handle_boundaries() {
        assert_eq!(spark(&[1.0, 2.0], 12).as_deref(), Some("▁█"));
        assert_eq!(spark(&[3.0, 3.0, 3.0], 12).as_deref(), Some("▄▄▄"));
        assert_eq!(
            short_wallet("0xf0318c32136c2db7fec88b84869aee6a1106c80c"),
            "0xf031…c80c"
        );
        assert_eq!(clean("a\u{1b}[31m漢\n", 20), "a?[31m??");
        assert_eq!(
            split_cells([0, 0, 0], TRADE_BAR_W).iter().sum::<usize>(),
            TRADE_BAR_W
        );
        assert_eq!(fit_left("long", 3).chars().count(), 3);
        assert_eq!(fit_right("long", 3).chars().count(), 3);
        assert_eq!(fit_number("long", 3), "###");
    }
}
