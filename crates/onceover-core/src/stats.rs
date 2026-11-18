//! Shared statistics: one implementation of each quantity the passes
//! compute, so a median or a rank correlation means the same thing in
//! every claim.
//!
//! Conventions:
//! - Quantiles interpolate linearly between order statistics at
//!   position `q * (n - 1)` of the sorted values (Hyndman and Fan's
//!   type 7, the default of R and NumPy). The median is the 0.5
//!   quantile and the IQR is the 0.75 quantile minus the 0.25
//!   quantile.
//! - `quantile_nearest_rank` returns an observed value (the order
//!   statistic at the floor of the same position); it serves claims
//!   that must name a value present in the data.
//! - Ranks are 1-based midranks: tied values share the average of the
//!   ranks they occupy.

/// The `q` quantile of ascending-sorted values (type 7). NaN when the
/// slice is empty.
pub fn quantile_sorted(sorted: &[f64], q: f64) -> f64 {
    quantile_at(sorted.len(), q, |i| sorted[i])
}

/// The `q` quantile (type 7) of a sorted sequence accessed by index,
/// for sorted data held in another shape (pairs, records).
pub fn quantile_at(len: usize, q: f64, at: impl Fn(usize) -> f64) -> f64 {
    if len == 0 {
        return f64::NAN;
    }
    let pos = q * (len - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(len - 1);
    let frac = pos - lo as f64;
    at(lo) * (1.0 - frac) + at(hi) * frac
}

/// The observed value at the `q` position of ascending-sorted values:
/// the order statistic at floor(q * (n - 1)).
pub fn quantile_nearest_rank(sorted: &[f64], q: f64) -> f64 {
    sorted[((q * (sorted.len() - 1) as f64) as usize).min(sorted.len() - 1)]
}

/// The median of ascending-sorted values.
pub fn median_sorted(sorted: &[f64]) -> f64 {
    quantile_sorted(sorted, 0.5)
}

/// The median of unsorted values (sorted with `total_cmp`). NaN when
/// empty.
pub fn median(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    median_sorted(&v)
}

/// The interquartile range of ascending-sorted values.
pub fn iqr_sorted(sorted: &[f64]) -> f64 {
    quantile_sorted(sorted, 0.75) - quantile_sorted(sorted, 0.25)
}

/// 1-based midranks: tied values share the average of their ranks.
pub fn midranks(values: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..values.len()).collect();
    idx.sort_by(|&a, &b| values[a].total_cmp(&values[b]));
    let mut ranks = vec![0.0f64; values.len()];
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && values[idx[j + 1]] == values[idx[i]] {
            j += 1;
        }
        let avg = (i + j) as f64 / 2.0 + 1.0;
        for &k in &idx[i..=j] {
            ranks[k] = avg;
        }
        i = j + 1;
    }
    ranks
}

/// Pearson's correlation of two equal-length series. None when fewer
/// than two pairs or either series has no variance.
pub fn pearson(x: &[f64], y: &[f64]) -> Option<f64> {
    let n = x.len().min(y.len());
    if n < 2 {
        return None;
    }
    let mx = x[..n].iter().sum::<f64>() / n as f64;
    let my = y[..n].iter().sum::<f64>() / n as f64;
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (a, b) in x[..n].iter().zip(&y[..n]) {
        let (dx, dy) = (a - mx, b - my);
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    if sxx <= 0.0 || syy <= 0.0 {
        return None;
    }
    Some(sxy / (sxx * syy).sqrt())
}

/// Spearman's rank correlation: Pearson's correlation of the midranks.
pub fn spearman(x: &[f64], y: &[f64]) -> Option<f64> {
    pearson(&midranks(x), &midranks(y))
}

/// The natural log of the gamma function, by the g = 7, n = 9
/// Lanczos approximation (relative error below 1e-15 for positive
/// arguments), with the reflection formula below 0.5.
pub fn ln_gamma(x: f64) -> f64 {
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let z = x - 1.0;
    let mut a = C[0];
    for (i, c) in C.iter().enumerate().skip(1) {
        a += c / (z + i as f64);
    }
    let t = z + 7.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (z + 0.5) * t.ln() - t + a.ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantiles_follow_type_7() {
        let v = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(quantile_sorted(&v, 0.0), 1.0);
        assert_eq!(quantile_sorted(&v, 1.0), 4.0);
        assert!((quantile_sorted(&v, 0.5) - 2.5).abs() < 1e-12);
        assert!((quantile_sorted(&v, 0.25) - 1.75).abs() < 1e-12);
        assert!((iqr_sorted(&v) - 1.5).abs() < 1e-12);
        assert_eq!(quantile_nearest_rank(&v, 0.5), 2.0);
        assert!(quantile_sorted(&[], 0.5).is_nan());
        assert!((median(&[3.0, 1.0, 2.0]) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn midranks_average_ties() {
        assert_eq!(midranks(&[10.0, 20.0, 20.0, 5.0]), vec![2.0, 3.5, 3.5, 1.0]);
    }

    #[test]
    fn correlations_match_known_values() {
        let x = [1.0, 2.0, 3.0, 4.0, 5.0];
        let y = [2.0, 4.0, 6.0, 8.0, 10.0];
        assert!((pearson(&x, &y).unwrap() - 1.0).abs() < 1e-12);
        let z = [1.0, 8.0, 27.0, 64.0, 125.0];
        assert!((spearman(&x, &z).unwrap() - 1.0).abs() < 1e-12);
        assert!(pearson(&x, &[1.0; 5]).is_none());
    }

    #[test]
    fn ln_gamma_matches_factorials() {
        // Gamma(n) = (n - 1)!
        assert!((ln_gamma(1.0)).abs() < 1e-12);
        assert!((ln_gamma(5.0) - 24f64.ln()).abs() < 1e-12);
        assert!((ln_gamma(11.0) - 3_628_800f64.ln()).abs() < 1e-10);
        // Gamma(0.5) = sqrt(pi)
        assert!((ln_gamma(0.5) - std::f64::consts::PI.sqrt().ln()).abs() < 1e-12);
    }
}
