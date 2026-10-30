//! Cheap broad screening before expensive model and dependence calculations.

use crate::encode::MISSING;

/// Symmetric normalized mutual information of quantile/level codes.
/// Caller supplies training rows when this is used to shortlist predictors.
/// Integer log table for the fast screening score: ln(k) for
/// counts up to 2^20, built once per process. Counts above the
/// table fall back to libm.
fn ln_table() -> &'static [f64] {
    static TABLE: std::sync::OnceLock<Vec<f64>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        (0..=(1 << 20))
            .map(|k| if k == 0 { f64::NEG_INFINITY } else { (k as f64).ln() })
            .collect()
    })
}

fn ln_count(c: u32) -> f64 {
    let table = ln_table();
    if (c as usize) < table.len() {
        table[c as usize]
    } else {
        (c as f64).ln()
    }
}

/// Marginal entropy of one encoded column over `rows`, in the
/// decomposed log-table form `information_fast` uses. Returns
/// None when the column has a missing value in `rows` — the
/// entropy is only cacheable when it cannot depend on the
/// paired column's missingness.
pub fn marginal_entropy(bins: &[u8], rows: &[u32]) -> Option<f64> {
    let mut counts = [0u32; 32];
    for &r in rows {
        let v = bins[r as usize];
        if v == MISSING {
            return None;
        }
        counts[v as usize] += 1;
    }
    let n = rows.len() as f64;
    if n < 30.0 {
        return None;
    }
    let ln_n = ln_count(rows.len() as u32);
    Some(
        counts
            .iter()
            .filter(|&&c| c > 0)
            .map(|&c| {
                let p = c as f64 / n;
                -p * (ln_count(c) - ln_n)
            })
            .sum::<f64>(),
    )
}

/// Stage-1 ranking variant of `information`: the same normalized
/// mutual information, with every logarithm decomposed over the
/// integer log table (ln(c*n/(l*r)) as ln c + ln n - ln l - ln r).
/// Scores agree with `information` to ~1e-12 relative — close
/// enough to rank pairs for a survivor set that stage 2 then
/// rescores exactly; never used for reported statistics.
///
/// `ent` optionally carries the two columns' marginal entropies
/// over `rows`, precomputed by the caller. They are used only
/// when BOTH are present, which the caller may supply only when
/// neither column has a missing value in `rows` (so the
/// pair-complete row count equals the full row count and the
/// marginals are pair-independent).
pub fn information_fast(
    a: &[u8],
    b: &[u8],
    rows: &[u32],
    ent: (Option<f64>, Option<f64>),
) -> f64 {
    information_fast_t(a, b, rows, ent, ln_table())
}

fn information_fast_t(
    a: &[u8],
    b: &[u8],
    rows: &[u32],
    ent: (Option<f64>, Option<f64>),
    table: &[f64],
) -> f64 {
    let ln = |c: u32| -> f64 {
        table
            .get(c as usize)
            .copied()
            .unwrap_or_else(|| (c as f64).ln())
    };
    let mut joint = [0u32; 1024];
    let mut left = [0u32; 32];
    let mut right = [0u32; 32];
    let mut cells = [0u16; 1024];
    let mut ncells = 0usize;
    let mut n = 0u32;
    for &r in rows {
        let (x, y) = (a[r as usize], b[r as usize]);
        if x == MISSING || y == MISSING {
            continue;
        }
        let cell = x as usize * 32 + y as usize;
        if joint[cell] == 0 {
            cells[ncells] = cell as u16;
            ncells += 1;
        }
        joint[cell] += 1;
        left[x as usize] += 1;
        right[y as usize] += 1;
        n += 1;
    }
    if n < 30 {
        return 0.0;
    }
    let nf = n as f64;
    let ln_n = ln(n);
    let (ha, hb) = match ent {
        (Some(ha), Some(hb)) => (ha, hb),
        _ => {
            let entropy = |counts: &[u32]| {
                counts
                    .iter()
                    .filter(|&&c| c > 0)
                    .map(|&c| {
                        let p = c as f64 / nf;
                        -p * (ln(c) - ln_n)
                    })
                    .sum::<f64>()
            };
            (entropy(&left), entropy(&right))
        }
    };
    let norm = (ha * hb).sqrt();
    if norm <= 0.0 {
        return 0.0;
    }
    // Per-bin log marginals, hoisted out of the cell loop.
    let mut ln_left = [0.0f64; 32];
    let mut ln_right = [0.0f64; 32];
    for k in 0..32 {
        if left[k] > 0 {
            ln_left[k] = ln(left[k]);
        }
        if right[k] > 0 {
            ln_right[k] = ln(right[k]);
        }
    }
    let mut mi = 0.0;
    for &cell in &cells[..ncells] {
        let (x, y) = (cell as usize / 32, cell as usize % 32);
        let c = joint[cell as usize];
        mi += c as f64 / nf * (ln(c) + ln_n - ln_left[x] - ln_right[y]);
    }
    (mi / norm).clamp(0.0, 1.0)
}

pub fn information(a: &[u8], b: &[u8], rows: &[u32]) -> f64 {
    let mut joint = [0u32; 1024];
    let mut left = [0u32; 32];
    let mut right = [0u32; 32];
    // Nonzero joint cells, collected during the histogram pass
    // and sorted so the MI sum below visits exactly the terms
    // the full 32x32 grid loop would, in the same order — the
    // result is bit-identical, at the cost of the nonzero cells
    // (<= the row count) instead of the full grid.
    let mut cells = [0u16; 1024];
    let mut ncells = 0usize;
    let mut n = 0u32;
    for &r in rows {
        let (x, y) = (a[r as usize], b[r as usize]);
        if x == MISSING || y == MISSING {
            continue;
        }
        let cell = x as usize * 32 + y as usize;
        if joint[cell] == 0 {
            cells[ncells] = cell as u16;
            ncells += 1;
        }
        joint[cell] += 1;
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
    cells[..ncells].sort_unstable();
    let mut mi = 0.0;
    for &cell in &cells[..ncells] {
        let (x, y) = (cell as usize / 32, cell as usize % 32);
        let c = joint[cell as usize] as f64;
        mi += c / n * (c * n / (left[x] as f64 * right[y] as f64)).ln();
    }
    (mi / norm).clamp(0.0, 1.0)
}

#[cfg(test)]
mod fast_score_tests {
    use super::*;

    fn bins_from(state: &mut u64, n: usize, missing_every: usize) -> Vec<u8> {
        (0..n)
            .map(|i| {
                *state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1);
                if missing_every > 0 && i % missing_every == 7 {
                    MISSING
                } else {
                    ((*state >> 33) % 32) as u8
                }
            })
            .collect()
    }

    #[test]
    fn fast_matches_exact_within_tolerance() {
        let rows: Vec<u32> = (0..300).collect();
        for seed in [1u64, 42, 977] {
            let mut s = seed;
            let a = bins_from(&mut s, 300, 0);
            let b = bins_from(&mut s, 300, 0);
            let exact = information(&a, &b, &rows);
            let fast = information_fast(&a, &b, &rows, (None, None));
            assert!(
                (exact - fast).abs() < 1e-9,
                "seed {seed}: exact {exact} fast {fast}"
            );
            // With cached entropies (no missing anywhere) the
            // fast score is unchanged.
            let ea = marginal_entropy(&a, &rows);
            let eb = marginal_entropy(&b, &rows);
            assert!(ea.is_some() && eb.is_some());
            let fast_cached = information_fast(&a, &b, &rows, (ea, eb));
            assert_eq!(fast, fast_cached);
        }
    }

    #[test]
    fn fast_matches_exact_with_missing_values() {
        let rows: Vec<u32> = (0..300).collect();
        let mut s = 7u64;
        let a = bins_from(&mut s, 300, 11);
        let b = bins_from(&mut s, 300, 13);
        let exact = information(&a, &b, &rows);
        let fast = information_fast(&a, &b, &rows, (None, None));
        assert!(
            (exact - fast).abs() < 1e-9,
            "exact {exact} fast {fast}"
        );
        // A column with missing values has no cacheable entropy.
        assert!(marginal_entropy(&a, &rows).is_none());
        assert!(marginal_entropy(&b, &rows).is_none());
    }

    #[test]
    fn fast_detects_dependence() {
        let rows: Vec<u32> = (0..400).collect();
        let mut s = 3u64;
        let a = bins_from(&mut s, 400, 0);
        let b = a.clone();
        assert!(information_fast(&a, &b, &rows, (None, None)) > 0.99);
    }
}
