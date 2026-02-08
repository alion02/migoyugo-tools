//! Pentanomial GSPRT implementation without approximations.
//!
//! Based on the mathematical framework from:
//! - http://hardy.uhasselt.be/Fishtest/support_MLE_multinomial.pdf
//! - https://github.com/vdbergh/pentanomial

/// Result of the SPRT test
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GsprtResult {
    Accept,   // H1 accepted (dev is better)
    Reject,   // H0 accepted (dev is not better)
    Continue, // Need more games
}

/// State of the SPRT test
pub struct SprtState {
    elo0: f64,
    elo1: f64,
    lower_bound: f64,
    upper_bound: f64,
    llr: f64,
}

impl SprtState {
    pub fn new(alpha: f64, beta: f64, elo0: f64, elo1: f64) -> Self {
        let lower_bound = (beta / (1.0 - alpha)).ln();
        let upper_bound = ((1.0 - beta) / alpha).ln();
        Self { elo0, elo1, lower_bound, upper_bound, llr: 0.0 }
    }

    pub fn lower_bound(&self) -> f64 {
        self.lower_bound
    }

    pub fn upper_bound(&self) -> f64 {
        self.upper_bound
    }

    pub fn llr(&self) -> f64 {
        self.llr
    }

    /// Update the LLR based on current pentanomial counts.
    pub fn update(&mut self, pentanomial: &[u64; 5]) {
        self.llr = compute_llr_logistic(self.elo0, self.elo1, pentanomial);
    }

    /// Test result based on current LLR
    pub fn test_result(&self) -> GsprtResult {
        if self.llr >= self.upper_bound {
            GsprtResult::Accept
        } else if self.llr <= self.lower_bound {
            GsprtResult::Reject
        } else {
            GsprtResult::Continue
        }
    }

    /// Estimate normalized Pentanomial Elo (nElo) from results.
    ///
    /// This uses the formula: nElo = (mu - 0.5) / sigma_pg * (800 / ln(10))
    /// where sigma_pg is the per-game standard deviation.
    pub fn elo_estimate(&self, pentanomial: &[u64; 5]) -> f64 {
        let (_, pdf) = results_to_pdf(pentanomial);
        let (mu, var) = stats(&pdf);
        // var is the variance of the pair score (average of two games).
        // The per-game variance is 2 * var (assuming independence within pair, which is standard for nElo).
        let sigma_pg = (2.0 * var).sqrt();

        // Avoid division by zero in extreme cases (though regularization in results_to_pdf prevents exact zero)
        if sigma_pg < 1e-9 {
            return if mu > 0.5 { f64::INFINITY } else { f64::NEG_INFINITY };
        }

        const NELO_DIVIDER: f64 = 800.0 / std::f64::consts::LN_10;
        (mu - 0.5) / sigma_pg * NELO_DIVIDER
    }
}

/// Convert logistic Elo to expected score
fn elo_to_score(elo: f64) -> f64 {
    1.0 / (1.0 + 10.0_f64.powf(-elo / 400.0))
}

/// Convert expected score to logistic Elo
fn score_to_elo(score: f64) -> f64 {
    let score = score.clamp(0.001, 0.999);
    -400.0 * (1.0 / score - 1.0).log10()
}

/// Compute statistics (mean, variance) of a discrete PDF
fn stats(pdf: &[(f64, f64)]) -> (f64, f64) {
    let mean: f64 = pdf.iter().map(|(val, prob)| prob * val).sum();
    let var: f64 = pdf.iter().map(|(val, prob)| prob * (val - mean).powi(2)).sum();
    (mean, var)
}

/// Convert pentanomial counts to PDF
fn results_to_pdf(results: &[u64; 5]) -> (f64, Vec<(f64, f64)>) {
    const EPSILON: f64 = 1e-3;
    let regularized: Vec<f64> = results.iter().map(|&r| (r as f64).max(EPSILON)).collect();
    let n: f64 = regularized.iter().sum();

    // Pentanomial outcomes: 0, 0.25, 0.5, 0.75, 1.0 (normalized per-game scores)
    let pdf: Vec<(f64, f64)> = regularized.iter().enumerate().map(|(i, &count)| (i as f64 / 4.0, count / n)).collect();
    (n, pdf)
}

/// MLE for a discrete distribution with target expectation.
fn mle(pdf: &[(f64, f64)], s: f64) -> Vec<(f64, f64)> {
    let v = pdf.first().unwrap().0;
    let w = pdf.last().unwrap().0;
    let s = s.clamp(v + 1e-9, w - 1e-9);

    let l = -1.0 / (w - s);
    let u = 1.0 / (s - v);
    let eps = 1e-9;

    let f = |x: f64| -> f64 { pdf.iter().map(|(a, p)| p * (a - s) / (1.0 + x * (a - s))).sum() };
    let x = brent_root(f, l + eps, u - eps, 1e-12);

    pdf.iter().map(|(a, p)| (*a, p / (1.0 + x * (a - s)))).collect()
}

/// Brent's method for root finding
fn brent_root<F: Fn(f64) -> f64>(f: F, mut a: f64, mut b: f64, tol: f64) -> f64 {
    let (mut fa, mut fb) = (f(a), f(b));
    if fa.abs() < fb.abs() {
        std::mem::swap(&mut a, &mut b);
        std::mem::swap(&mut fa, &mut fb);
    }

    let (mut c, mut fc) = (a, fa);
    let mut d = b - a;
    let mut mflag = true;

    for _ in 0..100 {
        if fb.abs() < tol {
            return b;
        }
        if fa.abs() < tol {
            return a;
        }

        let s = if (fa - fc).abs() > tol && (fb - fc).abs() > tol {
            // Inverse quadratic interpolation
            a * fb * fc / ((fa - fb) * (fa - fc))
                + b * fa * fc / ((fb - fa) * (fb - fc))
                + c * fa * fb / ((fc - fa) * (fc - fb))
        } else {
            // Secant method
            b - fb * (b - a) / (fb - fa)
        };

        let mid = (3.0 * a + b) / 4.0;
        let use_bisection = !(s > mid.min(b) && s < mid.max(b))
            || (mflag && (s - b).abs() >= (b - c).abs() / 2.0)
            || (!mflag && (s - b).abs() >= (c - d).abs() / 2.0)
            || (mflag && (b - c).abs() < tol)
            || (!mflag && (c - d).abs() < tol);

        let s = if use_bisection {
            mflag = true;
            (a + b) / 2.0
        } else {
            mflag = false;
            s
        };

        let fs = f(s);
        d = c;
        c = b;
        fc = fb;

        if fa * fs < 0.0 {
            b = s;
            fb = fs;
        } else {
            a = s;
            fa = fs;
        }

        if fa.abs() < fb.abs() {
            std::mem::swap(&mut a, &mut b);
            std::mem::swap(&mut fa, &mut fb);
        }
    }
    b
}

/// Compute LLR using the logistic Elo model
pub fn compute_llr_logistic(elo0: f64, elo1: f64, pentanomial: &[u64; 5]) -> f64 {
    let s0 = elo_to_score(elo0);
    let s1 = elo_to_score(elo1);
    let (n, pdf) = results_to_pdf(pentanomial);

    let pdf0 = mle(&pdf, s0);
    let pdf1 = mle(&pdf, s1);

    let llr: f64 = pdf
        .iter()
        .zip(pdf0.iter())
        .zip(pdf1.iter())
        .map(|((_, (_, p0)), (_, p1))| if *p0 > 1e-15 && *p1 > 1e-15 { p1.ln() - p0.ln() } else { 0.0 })
        .zip(pdf.iter().map(|(_, p)| p))
        .map(|(jump, p)| jump * p)
        .sum();

    n * llr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elo_score_roundtrip() {
        for elo in [-100.0, -50.0, 0.0, 50.0, 100.0] {
            let score = elo_to_score(elo);
            let back = score_to_elo(score);
            assert!((elo - back).abs() < 0.01, "elo={elo}, back={back}");
        }
    }

    #[test]
    fn test_sprt_bounds() {
        let sprt = SprtState::new(0.05, 0.05, 0.0, 5.0);
        assert!((sprt.lower_bound() - (-2.944)).abs() < 0.01);
        assert!((sprt.upper_bound() - 2.944).abs() < 0.01);
    }

    #[test]
    fn test_llr_symmetry() {
        let neutral = [10, 20, 40, 20, 10];
        let llr = compute_llr_logistic(0.0, 5.0, &neutral);
        assert!(llr.abs() < 1.0, "LLR should be near 0 for symmetric results: {llr}");
    }

    #[test]
    fn test_nelo_calculation() {
        let sprt = SprtState::new(0.05, 0.05, 0.0, 5.0);

        // Scenario: W/L ratio 3:1, no draws.
        // Pairs outcomes: WW (56.25%), WL/LW (37.5%), LL (6.25%)
        // Counts: [625, 0, 3750, 0, 5625] (total 10000)
        let counts = [625, 0, 3750, 0, 5625];

        let nelo = sprt.elo_estimate(&counts);

        // Expected nElo: approx 200.5
        // Logistic Elo would be ~190.8
        assert!((nelo - 200.5).abs() < 1.0, "nElo was {nelo}, expected ~200.5");
    }
}
