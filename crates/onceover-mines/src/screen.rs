//! Cheap broad screening before expensive model and dependence calculations.

use crate::encode::MISSING;

/// Symmetric normalized mutual information of quantile/level codes.
/// Caller supplies training rows when this is used to shortlist predictors.
pub fn information(a: &[u8], b: &[u8], rows: &[u32]) -> f64 {
    let mut joint = [0u32; 1024];
    let mut left = [0u32; 32];
    let mut right = [0u32; 32];
    let mut n = 0u32;
    for &r in rows {
        let (x, y) = (a[r as usize], b[r as usize]);
        if x == MISSING || y == MISSING {
            continue;
        }
        joint[x as usize * 32 + y as usize] += 1;
        left[x as usize] += 1;
        right[y as usize] += 1;
        n += 1;
    }
    if n < 30 {
        return 0.0;
    }
    let n = n as f64;
    let entropy = |counts: &[u32]| {
        counts
            .iter()
            .filter(|&&c| c > 0)
            .map(|&c| {
                let p = c as f64 / n;
                -p * p.ln()
            })
            .sum::<f64>()
    };
    let norm = (entropy(&left) * entropy(&right)).sqrt();
    if norm <= 0.0 {
        return 0.0;
    }
    let mut mi = 0.0;
    for x in 0..32 {
        for y in 0..32 {
            let c = joint[x * 32 + y] as f64;
            if c > 0.0 {
                mi += c / n * (c * n / (left[x] as f64 * right[y] as f64)).ln();
            }
        }
    }
    (mi / norm).clamp(0.0, 1.0)
}
