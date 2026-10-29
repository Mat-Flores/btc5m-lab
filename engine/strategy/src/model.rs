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
pub fn norm_cdf(x: f64) -> f64 { norm_cdf_with(x, f64::exp) }

/// `norm_cdf` with the exponential of JavaScript (`js_exp`): bit for bit `normCdf` of src/core/model.js.
pub fn norm_cdf_js(x: f64) -> f64 { norm_cdf_with(x, js_exp) }

fn norm_cdf_with(x: f64, exp: fn(f64) -> f64) -> f64 {
    let t = 1.0 / (1.0 + 0.3275911 * x.abs() / std::f64::consts::SQRT_2);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592)
            * t
            * exp(-x * x / 2.0);
    if x >= 0.0 { (1.0 + y) / 2.0 } else { (1.0 - y) / 2.0 }
}

/// `Math.exp` of V8 (fdlibm `__ieee754_exp`, V8 src/base/ieee754.cc): within 1 ulp like the platform's `exp`, but not
/// always the same bits. The maker port needs JavaScript's bits; the reference strategy keeps `f64::exp` (its replay
/// parity was established with it).
pub fn js_exp(x: f64) -> f64 {
    const HALF: [f64; 2] = [0.5, -0.5];
    const HUGE: f64 = 1.0e+300;
    const TWOM1000: f64 = 9.33263618503218878990e-302;
    const O_THRESHOLD: f64 = 7.09782712893383973096e+02;
    const U_THRESHOLD: f64 = -7.45133219101941108420e+02;
    const LN2HI: [f64; 2] = [6.93147180369123816490e-01, -6.93147180369123816490e-01];
    const LN2LO: [f64; 2] = [1.90821492927058770002e-10, -1.90821492927058770002e-10];
    const INVLN2: f64 = 1.44269504088896338700e+00;
    const P1: f64 = 1.66666666666666019037e-01;
    const P2: f64 = -2.77777777770155933842e-03;
    const P3: f64 = 6.61375632143793436117e-05;
    const P4: f64 = -1.65339022054652515390e-06;
    const P5: f64 = 4.13813679705723846039e-08;
    let mut x = x;
    let bits = x.to_bits();
    let mut hx = (bits >> 32) as u32;
    let xsb = ((hx >> 31) & 1) as usize;
    hx &= 0x7fff_ffff;
    if hx >= 0x4086_2E42 {
        if hx >= 0x7ff0_0000 {
            if ((hx & 0xfffff) | bits as u32) != 0 { return x + x; }
            return if xsb == 0 { x } else { 0.0 };
        }
        if x > O_THRESHOLD { return HUGE * HUGE; }
        if x < U_THRESHOLD { return TWOM1000 * TWOM1000; }
    }
    let (mut hi, mut lo, mut k) = (0.0, 0.0, 0i32);
    if hx > 0x3fd6_2e42 {
        if hx < 0x3FF0_A2B2 {
            // V8 returns E itself for exp(1), where this computation gets the last bit wrong
            if x == 1.0 { return std::f64::consts::E; }
            hi = x - LN2HI[xsb]; lo = LN2LO[xsb]; k = 1 - xsb as i32 - xsb as i32;
        } else {
            k = (INVLN2 * x + HALF[xsb]) as i32;
            let t = k as f64;
            hi = x - t * LN2HI[0];
            lo = t * LN2LO[0];
        }
        x = hi - lo;
    } else if hx < 0x3e30_0000 {
        if HUGE + x > 1.0 { return 1.0 + x; }
    }
    let t = x * x;
    let c = x - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    if k == 0 { return 1.0 - ((x * c) / (c - 2.0) - x); }
    let y = 1.0 - ((lo - (x * c) / (2.0 - c)) - hi);
    let add = |y: f64, k: i32| f64::from_bits(y.to_bits().wrapping_add(((k as i64) << 52) as u64));
    if k >= -1021 { add(y, k) } else { add(y, k + 1000) * TWOM1000 }
}

/// UP wins if the Chainlink TWAP over the last `twap` s is >= strike; arithmetic Brownian motion, sigma in $/sqrt(s).
pub fn fair_up(spot: f64, strike: f64, tau: f64, sigma: f64, twap: f64, known_sum: f64, known_n: f64) -> f64 {
    fair_up_with(spot, strike, tau, sigma, twap, known_sum, known_n, norm_cdf)
}

/// `fair_up` with JavaScript's exponential: bit for bit `fairUp` of src/core/model.js (maker port).
pub fn fair_up_js(spot: f64, strike: f64, tau: f64, sigma: f64, twap: f64, known_sum: f64, known_n: f64) -> f64 {
    fair_up_with(spot, strike, tau, sigma, twap, known_sum, known_n, norm_cdf_js)
}

#[allow(clippy::too_many_arguments)]
fn fair_up_with(spot: f64, strike: f64, tau: f64, sigma: f64, twap: f64, known_sum: f64, known_n: f64, cdf: fn(f64) -> f64) -> f64 {
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
    cdf((mean - strike) / sd)
}

/// Realised volatility of 1-second changes over a rolling window, with a floor and a prior
/// (`new Vol(window, floor, prior)` of src/core/model.js; its defaults 300, 1, 8 are the strategies' configuration).
#[derive(Clone)]
pub struct Vol {
    d: Vec<f64>,
    n: usize,
    i: usize,
    s2: f64,
    last: f64,
    window: usize,
    floor: f64,
    prior: f64,
}
impl Vol {
    /// `window`: number of 1-second changes kept; `floor`, `prior` ($/√s): lowest sigma, sigma before 30 changes.
    pub fn with(window: usize, floor: f64, prior: f64) -> Self {
        Vol { d: vec![0.0; window], n: 0, i: 0, s2: 0.0, last: f64::NAN, window, floor, prior }
    }
    pub fn push(&mut self, price: f64) {
        if self.last.is_finite() && price.is_finite() {
            let x = price - self.last;
            if self.n == self.window { self.s2 -= self.d[self.i] * self.d[self.i]; } else { self.n += 1; }
            self.d[self.i] = x;
            self.s2 += x * x;
            self.i = (self.i + 1) % self.window;
        }
        if price.is_finite() { self.last = price; }
    }
    pub fn sigma(&self) -> f64 {
        if self.n < 30 { return jmax(self.floor, self.prior); }
        jmax(self.floor, (jmax(0.0, self.s2) / self.n as f64).sqrt())
    }
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
