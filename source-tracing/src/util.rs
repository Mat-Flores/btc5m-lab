//! Petits utilitaires de temps (UTC, sans crate de dates) et de texte.
use std::fmt::Write;

pub const DAY_US: i64 = 86_400_000_000;

/// Algorithme de Hinnant : jours depuis 1970-01-01 -> (année, mois, jour).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = y - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as i64 + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// "AAAA-MM-JJ" de la date UTC d'un instant en microsecondes.
pub fn date_str(us: i64) -> String {
    let (y, m, d) = civil_from_days(us.div_euclid(DAY_US));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Microsecondes UTC de 00:00:00 d'une date "AAAA-MM-JJ".
pub fn day_start_us(date: &str) -> Option<i64> {
    let mut it = date.split('-');
    let (y, m, d) = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    if it.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) { return None; }
    Some(days_from_civil(y, m, d) * DAY_US)
}

pub fn next_midnight(us: i64) -> i64 { (us.div_euclid(DAY_US) + 1) * DAY_US }

pub fn hex(b: &[u8]) -> String {
    b.iter().fold(String::with_capacity(b.len() * 2), |mut s, x| { let _ = write!(s, "{x:02x}"); s })
}

pub fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok()).or_else(|| std::env::var("COMPUTERNAME").ok()).unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dates_round_trip() {
        let d = day_start_us("2026-10-27").unwrap();
        assert_eq!(date_str(d), "2026-10-27");
        assert_eq!(date_str(d - 1), "2026-10-26");
        assert_eq!(date_str(next_midnight(d + 5)), "2026-10-28");
        assert_eq!(date_str(day_start_us("2028-02-29").unwrap() + DAY_US), "2028-03-01");
        assert_eq!(day_start_us("1970-01-01"), Some(0));
    }
}
