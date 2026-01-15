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
    alpha: f64,
    beta: f64,
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
        Self { alpha, beta, elo0, elo1, lower_bound, upper_bound, llr: 0.0 }
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
    /// pentanomial = [LL, LD, DD/WL, WD, WW]
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

    /// Estimate Elo with confidence interval
    pub fn elo_estimate(&self) -> Option<(f64, f64, f64)> {
        // This is a simplified estimate based on the score
        // For a full implementation, we'd need the Brownian motion analysis
        None // TODO: implement proper Elo estimation
    }
}

/// Convert logistic Elo to expected score
fn elo_to_score(elo: f64) -> f64 {
    1.0 / (1.0 + 10.0_f64.powf(-elo / 400.0))
}

/// Compute statistics (mean, variance) of a discrete PDF
fn stats(pdf: &[(f64, f64)]) -> (f64, f64) {
    let s: f64 = pdf.iter().map(|(value, prob)| prob * value).sum();
    let var: f64 = pdf.iter().map(|(value, prob)| prob * (value - s).powi(2)).sum();
    (s, var)
}

/// Convert pentanomial counts to PDF
/// Returns (total_count, pdf) where pdf is [(value, probability), ...]
fn results_to_pdf(results: &[u64; 5]) -> (f64, Vec<(f64, f64)>) {
    // Regularize to avoid zero probabilities
    let epsilon = 1e-3;
    let regularized: Vec<f64> = results.iter().map(|&r| (r as f64).max(epsilon)).collect();
    let n: f64 = regularized.iter().sum();

    // For pentanomial: outcomes are scored as 0, 0.25, 0.5, 0.75, 1.0
    // (representing pair scores: LL=0, LD=0.5, DD/WL=1.0, WD=1.5, WW=2.0 for 2 games)
    // Normalized to per-game: 0, 0.25, 0.5, 0.75, 1.0
    let pdf: Vec<(f64, f64)> = regularized.iter().enumerate().map(|(i, &count)| (i as f64 / 4.0, count / n)).collect();

    (n, pdf)
}

/// Maximum Likelihood Estimation for a discrete distribution with target expectation s.
///
/// Given an empirical distribution pdf and a target expectation s,
/// find the MLE distribution that has expectation exactly s.
///
/// Based on Proposition 1.1 from:
/// http://hardy.uhasselt.be/Fishtest/support_MLE_multinomial.pdf
fn mle(pdf: &[(f64, f64)], s: f64) -> Vec<(f64, f64)> {
    let v = pdf.first().unwrap().0;
    let w = pdf.last().unwrap().0;

    // If s is outside the range, clamp it
    let s = s.clamp(v + 1e-9, w - 1e-9);

    // Find x using Brent's method
    // f(x) = sum(p_i * (a_i - s) / (1 + x * (a_i - s))) = 0
    let l = -1.0 / (w - s);
    let u = 1.0 / (s - v);
    let epsilon = 1e-9;

    let f = |x: f64| -> f64 { pdf.iter().map(|(a, p)| p * (a - s) / (1.0 + x * (a - s))).sum() };

    // Simple bisection to find the root
    let x = brent_find_root(f, l + epsilon, u - epsilon, 1e-12);

    // Compute MLE distribution
    pdf.iter().map(|(a, p)| (*a, p / (1.0 + x * (a - s)))).collect()
}

/// Brent's method for root finding
fn brent_find_root<F: Fn(f64) -> f64>(f: F, mut a: f64, mut b: f64, tol: f64) -> f64 {
    let mut fa = f(a);
    let mut fb = f(b);

    if fa.abs() < fb.abs() {
        std::mem::swap(&mut a, &mut b);
        std::mem::swap(&mut fa, &mut fb);
    }

    let mut c = a;
    let mut fc = fa;
    let mut s;
    let mut d = b - a;
    let mut mflag = true;

    for _ in 0..100 {
        if fb.abs() < tol {
            return b;
        }
        if fa.abs() < tol {
            return a;
        }

        if (fa - fc).abs() > tol && (fb - fc).abs() > tol {
            // Inverse quadratic interpolation
            s = a * fb * fc / ((fa - fb) * (fa - fc))
                + b * fa * fc / ((fb - fa) * (fb - fc))
                + c * fa * fb / ((fc - fa) * (fc - fb));
        } else {
            // Secant method
            s = b - fb * (b - a) / (fb - fa);
        }

        let cond1 = !(s > ((3.0 * a + b) / 4.0).min(b) && s < ((3.0 * a + b) / 4.0).max(b));
        let cond2 = mflag && (s - b).abs() >= (b - c).abs() / 2.0;
        let cond3 = !mflag && (s - b).abs() >= (c - d).abs() / 2.0;
        let cond4 = mflag && (b - c).abs() < tol;
        let cond5 = !mflag && (c - d).abs() < tol;

        if cond1 || cond2 || cond3 || cond4 || cond5 {
            s = (a + b) / 2.0;
            mflag = true;
        } else {
            mflag = false;
        }

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

/// Compute LLR jumps for the GSPRT
fn llr_jumps(pdf: &[(f64, f64)], s0: f64, s1: f64) -> Vec<(f64, f64)> {
    let pdf0 = mle(pdf, s0);
    let pdf1 = mle(pdf, s1);

    pdf.iter()
        .zip(pdf0.iter())
        .zip(pdf1.iter())
        .map(|(((_, p_obs), (_, p0)), (_, p1))| {
            let jump = if *p0 > 1e-15 && *p1 > 1e-15 { p1.ln() - p0.ln() } else { 0.0 };
            (jump, *p_obs)
        })
        .collect()
}

/// Compute the generalized log likelihood ratio (exact, no approximation)
fn compute_llr(pdf: &[(f64, f64)], s0: f64, s1: f64) -> f64 {
    let jumps = llr_jumps(pdf, s0, s1);
    stats(&jumps).0
}

/// Compute LLR using the logistic Elo model
/// pentanomial = [LL, LD, DD/WL, WD, WW]
pub fn compute_llr_logistic(elo0: f64, elo1: f64, pentanomial: &[u64; 5]) -> f64 {
    let s0 = elo_to_score(elo0);
    let s1 = elo_to_score(elo1);
    let (n, pdf) = results_to_pdf(pentanomial);
    n * compute_llr(&pdf, s0, s1)
}

/// Approximate LLR formula (for reference/comparison)
/// This is faster but not exact
#[allow(dead_code)]
pub fn compute_llr_approximate(elo0: f64, elo1: f64, pentanomial: &[u64; 5]) -> f64 {
    let s0 = elo_to_score(elo0);
    let s1 = elo_to_score(elo1);
    let (n, pdf) = results_to_pdf(pentanomial);
    let (s, var) = stats(&pdf);

    if var < 1e-15 {
        return 0.0;
    }

    // LLR ≈ (s1 - s0) * (2*s - s0 - s1) / (2 * var) * N
    n * (s1 - s0) * (2.0 * s - s0 - s1) / (2.0 * var)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elo_to_score() {
        assert!((elo_to_score(0.0) - 0.5).abs() < 1e-10);
        assert!((elo_to_score(400.0) - 0.909090909).abs() < 1e-6);
        assert!((elo_to_score(-400.0) - 0.090909091).abs() < 1e-6);
    }

    #[test]
    fn test_sprt_bounds() {
        let sprt = SprtState::new(0.05, 0.05, 0.0, 5.0);
        assert!((sprt.lower_bound() - (-2.944)).abs() < 0.01);
        assert!((sprt.upper_bound() - 2.944).abs() < 0.01);
    }

    #[test]
    fn test_llr_neutral() {
        // Equal results should give LLR near 0
        let pentanomial = [10, 20, 40, 20, 10];
        let llr = compute_llr_logistic(0.0, 5.0, &pentanomial);
        assert!(llr.abs() < 1.0);
    }

    #[test]
    fn test_llr_positive() {
        // More wins should give positive LLR
        let pentanomial = [5, 10, 30, 30, 25];
        let llr = compute_llr_logistic(0.0, 5.0, &pentanomial);
        assert!(llr > 0.0);
    }

    #[test]
    fn test_llr_negative() {
        // More losses should give negative LLR
        let pentanomial = [25, 30, 30, 10, 5];
        let llr = compute_llr_logistic(0.0, 5.0, &pentanomial);
        assert!(llr < 0.0);
    }
}
