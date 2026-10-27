//! Fair value of the UP share, ported from `src/core/model.js` operation for operation (same f64 order of
//! evaluation), so that the old engine's decisions are reproduced bit for bit.

/// `Math.max` / `Math.min`: NaN wins, unlike `f64::max`.
pub fn jmax(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else if a >= b { a } else { b }
}
pub fn jmin(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else if a <= b { a } else { b }
}
/// `Math.round`: ties toward +infinity.
pub fn jround(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// Abramowitz & Stegun 7.1.26, |err| < 1.5e-7.
pub fn norm_cdf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.3275911 * x.abs() / std::f64::consts::SQRT_2);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592)
            * t
            * (-x * x / 2.0).exp();
    if x >= 0.0 { (1.0 + y) / 2.0 } else { (1.0 - y) / 2.0 }
}

/// UP wins if the Chainlink TWAP over the last `twap` s is >= strike; arithmetic Brownian motion, sigma in $/sqrt(s).
pub fn fair_up(spot: f64, strike: f64, tau: f64, sigma: f64, twap: f64, known_sum: f64, known_n: f64) -> f64 {
    let tau = jmax(0.0, tau);
    let (mean, sd);
    if twap <= 0.0 {
        mean = spot;
        sd = sigma * tau.sqrt();
    } else if tau >= twap {
        mean = spot;
        sd = sigma * (tau - twap + twap / 3.0).sqrt();
    } else {
        let n = jmax(1.0, known_n + tau);
        mean = (known_sum + tau * spot) / n;
        sd = (tau / n) * sigma * (tau / 3.0).sqrt();
    }
    if !(sd > 1e-9) {
        return if mean >= strike { 1.0 } else { 0.0 };
    }
    norm_cdf((mean - strike) / sd)
}

/// Realised volatility of 1-second changes over a rolling window, with a floor and a prior.
#[derive(Clone)]
pub struct Vol {
    d: Vec<f64>,
    n: usize,
    i: usize,
    s2: f64,
    last: f64,
}
impl Vol {
    const WINDOW: usize = 300;
    const FLOOR: f64 = 1.0;
    const PRIOR: f64 = 8.0;
    pub fn new() -> Self {
        Vol { d: vec![0.0; Self::WINDOW], n: 0, i: 0, s2: 0.0, last: f64::NAN }
    }
    pub fn push(&mut self, price: f64) {
        if self.last.is_finite() && price.is_finite() {
            let x = price - self.last;
            if self.n == Self::WINDOW { self.s2 -= self.d[self.i] * self.d[self.i]; } else { self.n += 1; }
            self.d[self.i] = x;
            self.s2 += x * x;
            self.i = (self.i + 1) % Self::WINDOW;
        }
        if price.is_finite() { self.last = price; }
    }
    pub fn sigma(&self) -> f64 {
        if self.n < 30 { return jmax(Self::FLOOR, Self::PRIOR); }
        jmax(Self::FLOOR, (jmax(0.0, self.s2) / self.n as f64).sqrt())
    }
}

impl Default for Vol {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_cdf_is_symmetric_and_bounded() {
        for i in -60..=60 {
            let x = i as f64 / 10.0;
            let (a, b) = (norm_cdf(x), norm_cdf(-x));
            assert!((a + b - 1.0).abs() < 1.5e-7 && (0.0..=1.0).contains(&a));
        }
        assert!((norm_cdf(1.0) - 0.841_344_746).abs() < 2e-7);
    }

    #[test]
    fn js_helpers_follow_math_semantics() {
        assert!(jmax(1.0, f64::NAN).is_nan() && jmin(f64::NAN, 1.0).is_nan());
        assert_eq!(jround(-2.5), -2.0);
        assert_eq!(jround(2.5), 3.0);
        assert_eq!(jround(0.49999999999999994), 0.0);
    }
}
