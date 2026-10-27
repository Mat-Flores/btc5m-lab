//! Exact decimal parsing to integer micro-units (1e-6): prices, sizes and amounts never go through a float.
pub type Micro = i64;
pub const MICRO: Micro = 1_000_000;

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError { Empty, Invalid, TooPrecise, Overflow }

/// "0.581" -> 581_000. Refuses more than 6 decimals (a value we would have to round is an error, not a guess).
pub fn parse_micro(s: &str) -> Result<Micro, ParseError> { parse_scaled(s, 6, false) }

/// Like `parse_micro`, but rounds half away from zero beyond 6 decimals (for reference prices only, never money).
pub fn parse_micro_rounded(s: &str) -> Result<Micro, ParseError> { parse_scaled(s, 6, true) }

/// Integer string carrying `decimals` implied decimals ("80985877815741550000000", 18) -> micro, rounded.
pub fn scaled_int_to_micro(s: &str, decimals: u32) -> Result<Micro, ParseError> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) { return Err(ParseError::Invalid); }
    let v: i128 = s.parse().map_err(|_| ParseError::Overflow)?;
    let out = if decimals >= 6 {
        let div = 10i128.checked_pow(decimals - 6).ok_or(ParseError::Overflow)?;
        (v + div / 2) / div
    } else {
        v.checked_mul(10i128.pow(6 - decimals)).ok_or(ParseError::Overflow)?
    };
    Micro::try_from(out).map_err(|_| ParseError::Overflow)
}

fn parse_scaled(s: &str, scale: u32, round: bool) -> Result<Micro, ParseError> {
    let (neg, digits) = match s.strip_prefix('-') { Some(r) => (true, r), None => (false, s) };
    if digits.is_empty() { return Err(ParseError::Empty); }
    let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
    if int.is_empty() && frac.is_empty() { return Err(ParseError::Invalid); }
    if !int.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit()) { return Err(ParseError::Invalid); }
    let mut v: i128 = 0;
    for b in int.bytes() { v = v.checked_mul(10).and_then(|x| x.checked_add((b - b'0') as i128)).ok_or(ParseError::Overflow)?; }
    let mut f = frac.bytes();
    for _ in 0..scale { let d = f.next().map_or(0, |b| (b - b'0') as i128); v = v.checked_mul(10).and_then(|x| x.checked_add(d)).ok_or(ParseError::Overflow)?; }
    let rest: Vec<u8> = f.collect();
    if rest.iter().any(|&b| b != b'0') {
        if !round { return Err(ParseError::TooPrecise); }
        if rest[0] >= b'5' { v += 1; }
    }
    if neg { v = -v; }
    Micro::try_from(v).map_err(|_| ParseError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn examples() {
        assert_eq!(parse_micro("0.58"), Ok(580_000));
        assert_eq!(parse_micro("0.001"), Ok(1_000));
        assert_eq!(parse_micro("81089.9"), Ok(81_089_900_000));
        assert_eq!(parse_micro("17.54386"), Ok(17_543_860));
        assert_eq!(parse_micro("5"), Ok(5_000_000));
        assert_eq!(parse_micro(".5"), Ok(500_000));
        assert_eq!(parse_micro("0.1234567"), Err(ParseError::TooPrecise));
        assert_eq!(parse_micro("0.1234560"), Ok(123_456));
        assert_eq!(parse_micro_rounded("0.45456597"), Ok(454_566));
        assert_eq!(parse_micro(""), Err(ParseError::Empty));
        assert_eq!(parse_micro("."), Err(ParseError::Invalid));
        assert_eq!(parse_micro("1e3"), Err(ParseError::Invalid));
        assert_eq!(parse_micro("0x10"), Err(ParseError::Invalid));
        assert_eq!(scaled_int_to_micro("80985877815741550000000", 18), Ok(80_985_877_816));
    }

    proptest! {
        // roundtrip: any micro value printed with 6 decimals parses back to itself
        #[test]
        fn roundtrip(v in -9_000_000_000_000_000i64..9_000_000_000_000_000i64) {
            let s = format!("{}{}.{:06}", if v < 0 { "-" } else { "" }, (v / MICRO).unsigned_abs(), (v % MICRO).unsigned_abs());
            prop_assert_eq!(parse_micro(&s), Ok(v));
        }
        // garbage never panics
        #[test]
        fn never_panics(s in ".{0,40}") { let _ = parse_micro(&s); let _ = parse_micro_rounded(&s); let _ = scaled_int_to_micro(&s, 18); }
    }
}
