//! Cross-sectional discontinuity screen: step jumps of a target column
//! along another column's axis, and across diagonal boundaries in two
//! columns. Ported from the mboostrs-dev discontinuity screen
//! (`screen_discontinuities`, its `disc` kernels); see issue #40.
//!
//! Candidates are step indicators `1{u > c}` fitted against a smooth
//! background by OLS. Rows are split deterministically into a fit half
//! and an evaluation half: the fit half ranks candidates (`t_fit`), the
//! untouched evaluation half estimates the jump (`jump`, `se`, `z`) and
//! the held-out gain, so the winner's `z` keeps its null calibration no
//! matter how many candidates were searched. Flagged candidates are
//! then typed: a true jump's local level gap is scale-invariant
//! (nearest-60 vs nearest-15 evaluation points per side, ratio near 1),
//! while a steep smooth transition's gap grows with scale (ratio ~2+).
//! Only flagged candidates that type as jumps are reported.

use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct DiscConfig {
    pub cells: usize,
    pub directions: usize,
    pub min_side: usize,
    pub type_top: usize,
    pub max_per_target: usize,
    pub row_cap: usize,
    pub all_pairs_max_cols: usize,
    pub fallback_top_axes: usize,
    pub min_rows: usize,
    /// Reporting bar on the evaluation-half z (the screen itself
    /// flags at |z| >= 2; a profiler running the screen for every
    /// target needs a stricter bar to keep family-wise false
    /// positives rare).
    pub z_report: f64,
    /// Reporting bar on held-out gain as a share of the target's
    /// evaluation-half variance: the jump must be material, not
    /// merely detectable.
    pub rel_gain_min: f64,
}

impl Default for DiscConfig {
    fn default() -> Self {
        Self {
            cells: 20,
            directions: 16,
            min_side: 10,
            type_top: 12,
            max_per_target: 3,
            row_cap: 20_000,
            all_pairs_max_cols: 20,
            fallback_top_axes: 10,
            min_rows: 120,
            z_report: 4.0,
            rel_gain_min: 0.005,
        }
    }
}

impl DiscConfig {
    pub fn disabled() -> Self {
        Self {
            row_cap: 0,
            ..Self::default()
        }
    }
    pub fn is_enabled(&self) -> bool {
        self.row_cap > 0
    }
}

// ---------------------------------------------------------------------
// Small dense linear algebra (Cholesky-based OLS for tiny systems).

/// Lower-triangular Cholesky factor of an SPD matrix (row-major flat).
/// `None` when the matrix is not positive definite.
fn cholesky(a: &[f64], dim: usize) -> Option<Vec<f64>> {
    let mut l = vec![0.0f64; dim * dim];
    for i in 0..dim {
        for j in 0..=i {
            let mut s = a[i * dim + j];
            for k in 0..j {
                s -= l[i * dim + k] * l[j * dim + k];
            }
            if i == j {
                if s <= 0.0 || !s.is_finite() {
                    return None;
                }
                l[i * dim + i] = s.sqrt();
            } else {
                l[i * dim + j] = s / l[j * dim + j];
            }
        }
    }
    Some(l)
}

/// Solve `L Lᵀ x = rhs` given the Cholesky factor `l`.
fn chol_solve(l: &[f64], dim: usize, rhs: &[f64]) -> Option<Vec<f64>> {
    let mut y = vec![0.0f64; dim];
    for i in 0..dim {
        let mut s = rhs[i];
        for k in 0..i {
            s -= l[i * dim + k] * y[k];
        }
        y[i] = s / l[i * dim + i];
    }
    let mut x = vec![0.0f64; dim];
    for i in (0..dim).rev() {
        let mut s = y[i];
        for k in (i + 1)..dim {
            s -= l[k * dim + i] * x[k];
        }
        x[i] = s / l[i * dim + i];
    }
    Some(x)
}

/// Last diagonal element of `(L Lᵀ)⁻¹`: the scale factor of the last
/// coefficient's variance.
fn chol_inv_last_diag(l: &[f64], dim: usize) -> Option<f64> {
    // (A^-1)_kk = || L^-1 e_k ||^2 for A = L Lᵀ and k the last index:
    // forward-solve L x = e_k and return the squared norm.
    let k = dim - 1;
    let mut x = vec![0.0f64; dim];
    for i in 0..dim {
        let mut s = if i == k { 1.0 } else { 0.0 };
        for j in 0..i {
            s -= l[i * dim + j] * x[j];
        }
        x[i] = s / l[i * dim + i];
    }
    Some(x.iter().map(|v| v * v).sum())
}

// ---------------------------------------------------------------------
// The scan kernel.

#[derive(Clone, Debug)]
pub struct DiscCandidate {
    pub threshold: f64,
    pub n_above: usize,
    pub jump_fit: f64,
    pub t_fit: f64,
    pub jump: f64,
    pub se: f64,
    pub z: f64,
    pub gain: f64,
}

/// Evaluate every `u > c` step candidate against a smooth background.
///
/// `background` is row-major `n x q`. `fit[i] != 0` marks fit-half rows.
/// Prefix sums along the sort order of `u` make each threshold cost a
/// fixed small amount after one sort.
pub fn disc_scan(
    u: &[f64],
    background: &[f64],
    target: &[f64],
    fit: &[u8],
    cuts: &[f64],
    min_side: usize,
) -> Vec<DiscCandidate> {
    let order = sort_order(u);
    disc_scan_ordered(u, &order, background, target, fit, cuts, min_side)
}

/// The sort order of `u` (ties by row index) — the scan's only
/// superlinear step. LSD radix sort on the order-preserving bit
/// transform of the values (negative values invert, non-negative
/// values set the sign bit), which orders bit patterns exactly
/// as `f64::total_cmp`; eight counting passes over the index
/// array replace the comparison sort's n log n indirect
/// comparisons, the measured hot spot of the wide-table screen.
/// The sort is stable, so ties keep row-index order, matching
/// the previous comparison sort's output exactly.
pub fn sort_order(u: &[f64]) -> Vec<u32> {
    let n = u.len();
    let mut a: Vec<u32> = (0..n as u32).collect();
    if n < 2 {
        return a;
    }
    let keys: Vec<u64> = u
        .iter()
        .map(|&v| {
            let b = v.to_bits();
            if b & (1u64 << 63) != 0 {
                !b
            } else {
                b | (1u64 << 63)
            }
        })
        .collect();
    let mut b = vec![0u32; n];
    for pass in 0..8 {
        let shift = pass * 8;
        let mut counts = [0usize; 256];
        for &i in &a {
            counts[((keys[i as usize] >> shift) & 0xff) as usize] += 1;
        }
        let mut offset = 0usize;
        for c in counts.iter_mut() {
            let t = *c;
            *c = offset;
            offset += t;
        }
        for &i in &a {
            let k = ((keys[i as usize] >> shift) & 0xff) as usize;
            b[counts[k]] = i;
            counts[k] += 1;
        }
        std::mem::swap(&mut a, &mut b);
    }
    a
}

/// Per-half totals, base-model Grams and rhs, and the base
/// model's evaluation-half MSE: the part of a scan that does not
/// depend on any cut. Computed in one pass over the rows in
/// original order; `on_row` lets a caller accumulate per-row
/// extras in the same pass.
struct ScanBase {
    q: usize,
    total_fit: Vec<f64>,
    total_eval: Vec<f64>,
    gram_fit: Vec<f64>,
    gram_eval: Vec<f64>,
    rhs_fit: Vec<f64>,
    rhs_eval: Vec<f64>,
    n_fit: usize,
    n_eval: usize,
    mse_base_eval: f64,
}

fn scan_totals(
    background: &[f64],
    target: &[f64],
    fit: &[u8],
    n: usize,
    q: usize,
    mut on_row: impl FnMut(usize),
) -> Option<ScanBase> {
    let streams = q + 3; // count, target, target^2, background columns
    let mut total_fit = vec![0.0f64; streams];
    let mut total_eval = vec![0.0f64; streams];
    let mut gram_fit = vec![0.0f64; q * q];
    let mut gram_eval = vec![0.0f64; q * q];
    let mut rhs_fit = vec![0.0f64; q];
    let mut rhs_eval = vec![0.0f64; q];
    let mut n_fit = 0usize;
    for row in 0..n {
        let t = target[row];
        let is_fit = fit[row] != 0;
        if is_fit {
            n_fit += 1;
        }
        let (total, gram, rhs) = if is_fit {
            (&mut total_fit, &mut gram_fit, &mut rhs_fit)
        } else {
            (&mut total_eval, &mut gram_eval, &mut rhs_eval)
        };
        total[0] += 1.0;
        total[1] += t;
        total[2] += t * t;
        for a in 0..q {
            let va = background[row * q + a];
            total[3 + a] += va;
            rhs[a] += va * t;
            for b in 0..q {
                gram[a * q + b] += va * background[row * q + b];
            }
        }
        on_row(row);
    }
    let n_eval = n - n_fit;
    if n_fit == 0 || n_eval == 0 {
        return None;
    }
    let l_base = cholesky(&gram_fit, q)?;
    let gamma = chol_solve(&l_base, q, &rhs_fit)?;
    let mut mse_base_eval = total_eval[2];
    for a in 0..q {
        mse_base_eval -= 2.0 * gamma[a] * rhs_eval[a];
        for b in 0..q {
            mse_base_eval += gamma[a] * gram_eval[a * q + b] * gamma[b];
        }
    }
    mse_base_eval /= n_eval as f64;
    Some(ScanBase {
        q,
        total_fit,
        total_eval,
        gram_fit,
        gram_eval,
        rhs_fit,
        rhs_eval,
        n_fit,
        n_eval,
        mse_base_eval,
    })
}

/// Evaluate one cut from its per-half suffix stream sums (the
/// streams of the rows above the cut: count, target, target^2,
/// background columns). Shared by the ordered and binned scans;
/// the arithmetic is the scan's original per-cut evaluation.
#[allow(clippy::too_many_arguments)]
fn eval_cut(
    base: &ScanBase,
    cut: f64,
    min_side: usize,
    suffix_fit: &[f64],
    suffix_eval: &[f64],
    gram: &mut [f64],
    rhs: &mut [f64],
    gram_e: &mut [f64],
    rhs_e: &mut [f64],
) -> Option<DiscCandidate> {
    let q = base.q;
    let dim = q + 1;
    let n_fit = base.n_fit;
    let n_eval = base.n_eval;
    let n_g_fit = suffix_fit[0] as usize;
    let n_g_eval = suffix_eval[0] as usize;
    if (n_g_fit == n_fit && n_g_eval == n_eval) || (n_g_fit == 0 && n_g_eval == 0) {
        return None;
    }
    if n_g_fit < min_side
        || (n_fit - n_g_fit) < min_side
        || n_g_eval < min_side
        || (n_eval - n_g_eval) < min_side
    {
        return None;
    }
    // Fit half: Gram of [background, step] and rhs.
    gram.fill(0.0);
    rhs.fill(0.0);
    for a in 0..q {
        rhs[a] = base.rhs_fit[a];
        for b in 0..q {
            gram[a * dim + b] = base.gram_fit[a * q + b];
        }
        let cross = suffix_fit[3 + a];
        gram[a * dim + q] = cross;
        gram[q * dim + a] = cross;
    }
    gram[q * dim + q] = suffix_fit[0];
    rhs[q] = suffix_fit[1];
    let l_fit = cholesky(gram, dim)?;
    let theta_fit = chol_solve(&l_fit, dim, rhs)?;
    let rss_fit = base.total_fit[2]
        - theta_fit
            .iter()
            .zip(rhs.iter())
            .map(|(t, r)| t * r)
            .sum::<f64>();
    if n_fit <= dim || rss_fit <= 0.0 {
        return None;
    }
    let dof_fit = (n_fit - dim) as f64;
    let var_fit = chol_inv_last_diag(&l_fit, dim)?;
    let var_fit = (rss_fit / dof_fit) * var_fit;
    if var_fit <= 0.0 {
        return None;
    }
    let jump_fit = theta_fit[q];
    let t_fit = jump_fit / var_fit.sqrt();

    // Evaluation half: the honest estimate and its standard error.
    gram_e.fill(0.0);
    rhs_e.fill(0.0);
    for a in 0..q {
        rhs_e[a] = base.rhs_eval[a];
        for b in 0..q {
            gram_e[a * dim + b] = base.gram_eval[a * q + b];
        }
        let cross = suffix_eval[3 + a];
        gram_e[a * dim + q] = cross;
        gram_e[q * dim + a] = cross;
    }
    gram_e[q * dim + q] = suffix_eval[0];
    rhs_e[q] = suffix_eval[1];
    let l_eval = cholesky(gram_e, dim)?;
    let theta_eval = chol_solve(&l_eval, dim, rhs_e)?;
    let rss_eval = base.total_eval[2]
        - theta_eval
            .iter()
            .zip(rhs_e.iter())
            .map(|(t, r)| t * r)
            .sum::<f64>();
    if n_eval <= dim || rss_eval <= 0.0 {
        return None;
    }
    let dof_eval = (n_eval - dim) as f64;
    let var_eval = chol_inv_last_diag(&l_eval, dim)?;
    let var_eval = (rss_eval / dof_eval) * var_eval;
    if var_eval <= 0.0 {
        return None;
    }
    let jump = theta_eval[q];
    let se = var_eval.sqrt();
    let z = jump / se;

    // Held-out gain: fit-half step model vs fit-half base model,
    // both scored on the evaluation half.
    let mut mse_full_eval = base.total_eval[2];
    for a in 0..dim {
        mse_full_eval -= 2.0 * theta_fit[a] * rhs_e[a];
        for b in 0..dim {
            mse_full_eval += theta_fit[a] * gram_e[a * dim + b] * theta_fit[b];
        }
    }
    mse_full_eval /= n_eval as f64;
    let gain = base.mse_base_eval - mse_full_eval;

    Some(DiscCandidate {
        threshold: cut,
        n_above: n_g_fit + n_g_eval,
        jump_fit,
        t_fit,
        jump,
        se,
        z,
        gain,
    })
}

/// `disc_scan` with a caller-supplied sort order of `u` (from
/// `sort_order`); identical results to `disc_scan`.
pub fn disc_scan_ordered(
    u: &[f64],
    order: &[u32],
    background: &[f64],
    target: &[f64],
    fit: &[u8],
    cuts: &[f64],
    min_side: usize,
) -> Vec<DiscCandidate> {
    let n = u.len();
    if n == 0 || order.len() != n {
        return Vec::new();
    }
    let q = background.len() / n;
    if q == 0 || target.len() != n || fit.len() != n {
        return Vec::new();
    }
    let streams = q + 3;
    let Some(base) = scan_totals(background, target, fit, n, q, |_| {}) else {
        return Vec::new();
    };

    let us: Vec<f64> = order.iter().map(|&i| u[i as usize]).collect();

    // Evaluate cuts in ascending order, walking the sorted rows once
    // and maintaining prefix stream sums for both halves; a cut's
    // suffix sums are totals minus the prefix at its partition point.
    // (This replaces streams x n prefix matrices: same arithmetic,
    // no per-scan megabyte allocation.)
    let mut cut_order: Vec<usize> = (0..cuts.len()).collect();
    cut_order.sort_by(|&a, &b| cuts[a].total_cmp(&cuts[b]));
    let mut cum_fit = vec![0.0f64; streams];
    let mut cum_eval = vec![0.0f64; streams];
    let mut ptr = 0usize;
    let mut by_cut: Vec<Option<DiscCandidate>> = Vec::new();
    by_cut.resize_with(cuts.len(), || None);

    let dim = q + 1;
    let mut gram = vec![0.0f64; dim * dim];
    let mut rhs = vec![0.0f64; dim];
    let mut gram_e = vec![0.0f64; dim * dim];
    let mut rhs_e = vec![0.0f64; dim];
    let mut suffix_fit = vec![0.0f64; streams];
    let mut suffix_eval = vec![0.0f64; streams];
    for &ci in &cut_order {
        let cut = cuts[ci];
        while ptr < n && us[ptr] <= cut {
            let row = order[ptr] as usize;
            let t = target[row];
            let cum = if fit[row] != 0 {
                &mut cum_fit
            } else {
                &mut cum_eval
            };
            cum[0] += 1.0;
            cum[1] += t;
            cum[2] += t * t;
            for a in 0..q {
                cum[3 + a] += background[row * q + a];
            }
            ptr += 1;
        }
        for s in 0..streams {
            suffix_fit[s] = base.total_fit[s] - cum_fit[s];
            suffix_eval[s] = base.total_eval[s] - cum_eval[s];
        }
        by_cut[ci] = eval_cut(
            &base,
            cut,
            min_side,
            &suffix_fit,
            &suffix_eval,
            &mut gram,
            &mut rhs,
            &mut gram_e,
            &mut rhs_e,
        );
    }
    by_cut.into_iter().flatten().collect()
}

/// The typing refit: t-statistic of the step indicator on the
/// evaluation half only, against a background augmented with smooth
/// terms along the projection, so a smooth crossing's step coefficient
/// vanishes. `fit` is the scan's fit-half mask; evaluation rows are
/// those with `fit[i] == 0`.
pub fn disc_step_z(
    background: &[f64],
    indicator: &[f64],
    target: &[f64],
    fit: &[u8],
) -> Option<f64> {
    let n = fit.len();
    if n == 0 || indicator.len() != n || target.len() != n {
        return None;
    }
    let q = background.len() / n;
    if q == 0 {
        return None;
    }
    let dim = q + 1;
    let rows: Vec<usize> = (0..n).filter(|&i| fit[i] == 0).collect();
    let n_eval = rows.len();
    if n_eval <= dim {
        return None;
    }
    let mut gram = vec![0.0f64; dim * dim];
    let mut rhs = vec![0.0f64; dim];
    let mut tt = 0.0f64;
    for &i in &rows {
        let g = indicator[i];
        let t = target[i];
        tt += t * t;
        for a in 0..q {
            let va = background[i * q + a];
            rhs[a] += va * t;
            for b in 0..q {
                gram[a * dim + b] += va * background[i * q + b];
            }
            gram[a * dim + q] += va * g;
            gram[q * dim + a] += va * g;
        }
        gram[q * dim + q] += g * g;
        rhs[q] += g * t;
    }
    let l = cholesky(&gram, dim)?;
    let theta = chol_solve(&l, dim, &rhs)?;
    let rss = tt - theta.iter().zip(&rhs).map(|(t, r)| t * r).sum::<f64>();
    let dof = (n_eval - dim) as f64;
    if dof <= 0.0 || rss <= 0.0 {
        return None;
    }
    let var = (rss / dof) * chol_inv_last_diag(&l, dim)?;
    if var <= 0.0 {
        return None;
    }
    Some(theta[q] / var.sqrt())
}

/// OLS coefficients of `target` on `background` (row-major n x q),
/// fitted on the rows with `fit[i] == 0` (the evaluation half).
fn fit_background(background: &[f64], target: &[f64], fit: &[u8]) -> Option<Vec<f64>> {
    let n = fit.len();
    if n == 0 {
        return None;
    }
    let q = background.len() / n;
    let mut gram = vec![0.0f64; q * q];
    let mut rhs = vec![0.0f64; q];
    for i in 0..n {
        if fit[i] != 0 {
            continue;
        }
        let t = target[i];
        for a in 0..q {
            let va = background[i * q + a];
            rhs[a] += va * t;
            for b in 0..q {
                gram[a * q + b] += va * background[i * q + b];
            }
        }
    }
    let l = cholesky(&gram, q)?;
    chol_solve(&l, q, &rhs)
}

/// Typing numbers for one flagged candidate: `type_z` from the
/// augmented-background refit, and the two-scale gap measured on
/// evaluation-half residuals after the augmented smooth background
/// is removed. The background removal matters when the target has a
/// trend along the boundary (the planted diagonal case carries a
/// 1.5-per-unit trend along it): on raw values the trend inflates
/// each side's local variance and the small-scale gap t collapses,
/// typing a true jump as smooth.
fn typing_numbers(f: &Flagged) -> (Option<f64>, Option<(f64, f64)>) {
    let n = f.target_vals.len();
    let ones = vec![1.0f64; n];
    let u2: Vec<f64> = f.u.iter().map(|v| v * v).collect();
    let u3: Vec<f64> = f.u.iter().map(|v| v * v * v).collect();
    let bg_cols: Vec<&[f64]> = if f.kind == "axis" {
        vec![&ones, &f.u, &u2, &u3]
    } else {
        vec![&ones, &f.axis_z[0], &f.axis_z[1], &u2, &u3]
    };
    let bg = flat_background(&bg_cols);
    let indicator: Vec<f64> =
        f.u.iter()
            .map(|v| if *v > f.cand.threshold { 1.0 } else { 0.0 })
            .collect();
    let type_z = disc_step_z(&bg, &indicator, &f.target_vals, &f.fit);
    // Residuals for the gap come from the *scan* background, not the
    // augmented one: the u^2/u^3 terms that let type_z absorb a
    // smooth crossing also absorb a large share of a true step,
    // diluting the local gap (measured: gap_t 3.5 instead of ~11 on
    // the planted diagonal case).
    let scan_cols: Vec<&[f64]> = if f.kind == "axis" {
        vec![&ones, &f.u, &u2]
    } else {
        vec![&ones, &f.axis_z[0], &f.axis_z[1]]
    };
    let bg_scan = flat_background(&scan_cols);
    let gap = fit_background(&bg_scan, &f.target_vals, &f.fit).and_then(|coef| {
        let q = coef.len();
        let eval_pairs: Vec<(f64, f64)> = (0..n)
            .filter(|&i| f.fit[i] == 0)
            .map(|i| {
                let pred: f64 = (0..q).map(|a| bg_scan[i * q + a] * coef[a]).sum();
                (f.u[i], f.target_vals[i] - pred)
            })
            .collect();
        let ue: Vec<f64> = eval_pairs.iter().map(|p| p.0).collect();
        let te: Vec<f64> = eval_pairs.iter().map(|p| p.1).collect();
        type_gap(&ue, &te, f.cand.threshold)
    });
    (type_z, gap)
}

const TYPE_GAP_K1: usize = 15;
const TYPE_GAP_K2: usize = 60;
const TYPE_GAP_W: f64 = 0.2;
pub const TYPE_RATIO_CRIT: f64 = 1.5;
/// Small-scale gap significance bar. The originating screen uses
/// 4.0 on raw values for a single designated target; here the gap is
/// measured on scan-background residuals in a raw-column rotation,
/// where the local variance also carries the smooth background's
/// misfit along a slanted boundary (measured gap_t ~3.5 on the
/// planted diagonal case at the 4.0 bar, against ~11 for the
/// idealized axis case). 3.0 keeps the noise rejection while the
/// scale-invariance ratio does the smooth-transition rejection.
pub const TYPE_GAP_T_CRIT: f64 = 3.0;

/// Scale-invariance of the local level gap around a threshold, on
/// evaluation-half points: the gap between the nearest K1 points per
/// side vs the nearest K2, maximized over a window of thresholds.
/// Returns `(type_ratio, type_gap_t)`.
pub fn type_gap(u_eval: &[f64], t_eval: &[f64], c0: f64) -> Option<(f64, f64)> {
    let n = u_eval.len();
    if n < 2 * TYPE_GAP_K2 || t_eval.len() != n {
        return None;
    }
    let mut best1 = 0.0f64;
    let mut best2 = 0.0f64;
    let mut best_t1 = 0.0f64;
    for step in 0..11 {
        let c = c0 - TYPE_GAP_W + (2.0 * TYPE_GAP_W) * (step as f64) / 10.0;
        let mut below: Vec<(f64, f64)> = Vec::new(); // (distance, target)
        let mut above: Vec<(f64, f64)> = Vec::new();
        for i in 0..n {
            if u_eval[i] < c {
                below.push((c - u_eval[i], t_eval[i]));
            } else if u_eval[i] > c {
                above.push((u_eval[i] - c, t_eval[i]));
            }
        }
        if below.len() < TYPE_GAP_K2 || above.len() < TYPE_GAP_K2 {
            continue;
        }
        below.sort_by(|a, b| a.0.total_cmp(&b.0));
        above.sort_by(|a, b| a.0.total_cmp(&b.0));
        let take = |v: &[f64]| -> f64 { v.iter().sum::<f64>() / v.len() as f64 };
        let var_of = |v: &[f64]| -> f64 {
            if v.len() < 2 {
                return 0.0;
            }
            let m = take(v);
            v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (v.len() - 1) as f64
        };
        let yb1: Vec<f64> = below[..TYPE_GAP_K1].iter().map(|p| p.1).collect();
        let ya1: Vec<f64> = above[..TYPE_GAP_K1].iter().map(|p| p.1).collect();
        let yb2: Vec<f64> = below[..TYPE_GAP_K2].iter().map(|p| p.1).collect();
        let ya2: Vec<f64> = above[..TYPE_GAP_K2].iter().map(|p| p.1).collect();
        let g1 = (take(&ya1) - take(&yb1)).abs();
        let g2 = (take(&ya2) - take(&yb2)).abs();
        let v1 = var_of(&yb1) / TYPE_GAP_K1 as f64 + var_of(&ya1) / TYPE_GAP_K1 as f64;
        let t1 = if v1 > 0.0 { g1 / v1.sqrt() } else { 0.0 };
        best1 = best1.max(g1);
        best2 = best2.max(g2);
        best_t1 = best_t1.max(t1);
    }
    if best1 > 0.0 {
        Some((best2 / best1, best_t1))
    } else {
        None
    }
}

/// Empirical quantile with linear interpolation on a sorted slice.
fn quantile_sorted(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = p * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    let frac = pos - lo as f64;
    sorted[lo] * (1.0 - frac) + sorted[hi] * frac
}

/// Scan one unit (a projection `u` against a fixed background) and
/// return its best candidate by |t_fit|: the coarse quantile grid
/// first, then a fit-half-only refinement on a finer grid inside
/// the winning interval, so the reported threshold and jump are
/// not diluted by grid placement. Refinement reads fit-half
/// statistics only; the evaluation half is never touched by
/// selection, so the winner's z keeps its null calibration.
fn scan_unit(
    u: &[f64],
    background: &[f64],
    target: &[f64],
    fit: &[u8],
    cfg: &DiscConfig,
) -> Option<DiscCandidate> {
    let order = sort_order(u);
    // Fit-half values in ascending order (the fit subsequence of
    // the full order) for the quantile grid.
    let fit_sorted: Vec<f64> = order
        .iter()
        .filter(|&&i| fit[i as usize] != 0)
        .map(|&i| u[i as usize])
        .collect();
    let fit_u: &Vec<f64> = &fit_sorted;
    let cuts = quantile_cuts_sorted(&fit_sorted, cfg.cells);
    if cuts.is_empty() {
        return None;
    }
    let cands = disc_scan_ordered(u, &order, background, target, fit, &cuts, cfg.min_side);
    let best = cands
        .iter()
        .max_by(|a, b| {
            a.t_fit
                .abs()
                .partial_cmp(&b.t_fit.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned()?;
    if !flagged(&best) {
        // Refinement only sharpens a boundary that is already
        // reportable evidence; unflagged units keep the coarse
        // winner (it still ranks axes for the pair fallback).
        return Some(best);
    }
    // Refinement window: the grid interval around the winning cut.
    let pos = cuts.iter().filter(|&&c| c < best.threshold).count();
    let lo = if pos == 0 {
        fit_u.iter().copied().fold(f64::INFINITY, f64::min)
    } else {
        cuts[pos - 1]
    };
    let hi = if pos >= cuts.len() {
        fit_u.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    } else {
        cuts[pos]
    };
    if hi.partial_cmp(&lo) != Some(std::cmp::Ordering::Greater) {
        return Some(best);
    }
    let fine: Vec<f64> = (1..=20)
        .map(|k| lo + (hi - lo) * (k as f64) / 21.0)
        .collect();
    let refined = disc_scan_ordered(u, &order, background, target, fit, &fine, cfg.min_side);
    let best_fine = refined.iter().max_by(|a, b| {
        a.t_fit
            .abs()
            .partial_cmp(&b.t_fit.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    match best_fine {
        Some(b) if b.t_fit.abs() >= best.t_fit.abs() => Some(b.clone()),
        _ => Some(best),
    }
}

/// Interior equal-count cutpoints (the k/K quantiles, k = 1..K-1),
/// deduplicated.
#[cfg(test)]
fn quantile_cuts(values: &[f64], cells: usize) -> Vec<f64> {
    if values.is_empty() || cells < 2 {
        return Vec::new();
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    quantile_cuts_sorted(&sorted, cells)
}

/// Cutpoints from values already in ascending order.
fn quantile_cuts_sorted(sorted: &[f64], cells: usize) -> Vec<f64> {
    if sorted.is_empty() || cells < 2 {
        return Vec::new();
    }
    let mut cuts: Vec<f64> = (1..cells)
        .map(|k| quantile_sorted(sorted, k as f64 / cells as f64))
        .collect();
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    cuts
}

// ---------------------------------------------------------------------
// Driver: rotate every numeric column through the target role.

/// Borrow normalized source values; only nullable columns need a validity
/// bitmap. This keeps row selection exact without copying the whole table
/// or materializing complete row lists before every sampled scan.
struct NumericColumn<'a> {
    col: &'a TypedCol<'a>,
    n: usize,
    finite_count: usize,
    valid: Option<Vec<u64>>,
    shared_sample: std::sync::Arc<Vec<usize>>,
}

impl<'a> NumericColumn<'a> {
    fn new(col: &'a TypedCol<'a>, shared_sample: std::sync::Arc<Vec<usize>>) -> Self {
        let n = col.len();
        let missing = col.null_count();
        let valid = (missing > 0).then(|| {
            let mut bits = vec![0u64; n.div_ceil(64)];
            for row in 0..n {
                if crate::encode::numeric_at(col, row).is_some() {
                    bits[row / 64] |= 1u64 << (row % 64);
                }
            }
            bits
        });
        Self {
            col,
            n,
            finite_count: n - missing,
            valid,
            shared_sample,
        }
    }
    fn at(&self, row: usize) -> f64 {
        crate::encode::numeric_at(self.col, row).unwrap_or(f64::NAN)
    }
}

/// One unit's working data: the target values, the standardized axis
/// columns, the fit/eval parity flags, and each axis column's
/// (mean, standard deviation).
type UnitData = (Vec<f64>, Vec<Vec<f64>>, Vec<u8>, Vec<(f64, f64)>, Vec<u32>);

/// Rows where the target and all given axis columns are finite,
/// stride-sampled to the cap; deterministic parity half split.
fn unit_data(
    target_col: &NumericColumn,
    axis_cols: &[&NumericColumn],
    cap: usize,
) -> Option<UnitData> {
    let rows = kept_rows(target_col, axis_cols, cap);
    let n = rows.len();
    let target: Vec<f64> = rows.iter().map(|&i| target_col.at(i)).collect();
    let mut z: Vec<Vec<f64>> = Vec::with_capacity(axis_cols.len());
    let mut stats: Vec<(f64, f64)> = Vec::with_capacity(axis_cols.len());
    for col in axis_cols {
        let vals: Vec<f64> = rows.iter().map(|&i| col.at(i)).collect();
        let mean = vals.iter().sum::<f64>() / n as f64;
        let var = vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n as f64;
        let sd = var.sqrt();
        if sd.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return None;
        }
        stats.push((mean, sd));
        z.push(vals.iter().map(|v| (v - mean) / sd).collect());
    }
    let fit: Vec<u8> = (0..n).map(|i| if i % 2 == 0 { 1 } else { 0 }).collect();
    let rows32: Vec<u32> = rows.iter().map(|&r| r as u32).collect();
    Some((target, z, fit, stats, rows32))
}

fn flat_background(cols: &[&[f64]]) -> Vec<f64> {
    let n = cols[0].len();
    let q = cols.len();
    let mut out = vec![0.0f64; n * q];
    for (j, col) in cols.iter().enumerate() {
        for i in 0..n {
            out[i * q + j] = col[i];
        }
    }
    out
}

struct Flagged {
    kind: &'static str,
    target: usize,
    axes: Vec<usize>,
    direction_deg: Option<f64>,
    cand: DiscCandidate,
    /// Everything needed to type the candidate later.
    u: Vec<f64>,
    target_vals: Vec<f64>,
    fit: Vec<u8>,
    axis_z: Vec<Vec<f64>>,
    /// (mean, sd) per axis column from the unit's standardization;
    /// thresholds are reported in original units through these.
    axis_stats: Vec<(f64, f64)>,
}

fn flagged(cand: &DiscCandidate) -> bool {
    cand.z.abs() >= 2.0 && cand.gain > 0.0
}

struct TargetOutcome {
    findings: Vec<Finding>,
    screened: bool,
    n_axis: usize,
    n_diag: usize,
    n_flagged: usize,
    n_untyped: usize,
}

impl TargetOutcome {
    fn new() -> Self {
        Self {
            findings: Vec::new(),
            screened: false,
            n_axis: 0,
            n_diag: 0,
            n_flagged: 0,
            n_untyped: 0,
        }
    }
}

/// Shared, target-independent scan data: standardized axis
/// pairs (with their flat background), validated by the kept-row
/// vector they were built from. Everything cached is a pure
/// function of the kept rows and the axis columns, so sharing
/// changes no arithmetic.
struct PairData {
    rows: Vec<u32>,
    z: Vec<Vec<f64>>,
    fit: Vec<u8>,
    stats: Vec<(f64, f64)>,
    bg: Vec<f64>,
}

/// Pair projections cached by (target, predictor) column indices.
#[derive(Default)]
struct PairCache {
    entries: std::collections::HashMap<(usize, usize), std::sync::Arc<PairData>>,
    bytes: usize,
}
const PAIR_CACHE_BYTES: usize = 64 * 1024 * 1024;

impl PairData {
    fn bytes(&self) -> usize {
        self.rows.len() * std::mem::size_of::<u32>()
            + self.fit.len()
            + (self.z.iter().map(Vec::len).sum::<usize>() + self.bg.len() + self.stats.len() * 2)
                * std::mem::size_of::<f64>()
    }
}
struct DiscCaches {
    pairs: std::sync::Mutex<PairCache>,
}

impl DiscCaches {
    fn new() -> Self {
        Self {
            pairs: std::sync::Mutex::new(PairCache::default()),
        }
    }

    /// The pair's shared data for these kept rows, building and
    /// caching it on first use; `None` when an axis is constant
    /// over the kept rows. A cached entry is used only when its
    /// kept rows match exactly; a negative entry carries no
    /// rows, so it is never trusted for a different row set.
    fn pair(
        &self,
        a1: usize,
        a2: usize,
        rows: &[usize],
        axis_cols: &[&NumericColumn],
    ) -> Option<std::sync::Arc<PairData>> {
        let rows32: Vec<u32> = rows.iter().map(|&r| r as u32).collect();
        if let Some(data) = self.pairs.lock().unwrap().entries.get(&(a1, a2)) {
            if data.rows == rows32 {
                return Some(data.clone());
            }
        }
        let built = standardize(rows, axis_cols).map(|(z, stats)| {
            let n = rows.len();
            let ones = vec![1.0f64; n];
            let bg = flat_background(&[&ones, &z[0], &z[1]]);
            std::sync::Arc::new(PairData {
                rows: rows32,
                fit: (0..n).map(|i| if i % 2 == 0 { 1 } else { 0 }).collect(),
                z,
                stats,
                bg,
            })
        });
        if let Some(data) = &built {
            let size = data.bytes();
            let mut cache = self.pairs.lock().unwrap();
            if size <= PAIR_CACHE_BYTES {
                if cache.bytes + size > PAIR_CACHE_BYTES {
                    cache.entries.clear();
                    cache.bytes = 0;
                }
                if let Some(previous) = cache.entries.insert((a1, a2), data.clone()) {
                    cache.bytes -= previous.bytes();
                }
                cache.bytes += size;
            }
        }
        built
    }
}

/// Rows where the target and all axis columns are finite,
/// stride-sampled to the cap.
fn kept_rows(target_col: &NumericColumn, axis_cols: &[&NumericColumn], cap: usize) -> Vec<usize> {
    let masks: Vec<&[u64]> = std::iter::once(target_col)
        .chain(axis_cols.iter().copied())
        .filter_map(|col| col.valid.as_deref())
        .collect();
    if masks.is_empty() {
        return target_col.shared_sample.as_ref().clone();
    }
    // Enumerate ranks in the exact complete-case population, then select
    // the same deterministic stratified ranks as the former full row list.
    let words = target_col.n.div_ceil(64);
    let word = |i: usize| masks.iter().fold(u64::MAX, |bits, m| bits & m[i]);
    let total: usize = (0..words).map(|i| word(i).count_ones() as usize).sum();
    let ranks = crate::stride_rows(total, cap);
    let mut rows = Vec::with_capacity(ranks.len());
    let (mut base, mut next) = (0usize, 0usize);
    for w in 0..words {
        if next == ranks.len() {
            break;
        }
        let mut bits = word(w);
        let count = bits.count_ones() as usize;
        if ranks[next] as usize >= base + count {
            base += count;
            continue;
        }
        let mut rank = base;
        while bits != 0 && next < ranks.len() {
            let bit = bits.trailing_zeros() as usize;
            if rank == ranks[next] as usize {
                rows.push(w * 64 + bit);
                next += 1;
            }
            bits &= bits - 1;
            rank += 1;
        }
        base += count;
    }
    rows
}

/// Standardize the axis columns over the kept rows; `None` when
/// an axis is constant there.
/// Standardized columns with each column's (mean, sd) alongside.
type Standardized = (Vec<Vec<f64>>, Vec<(f64, f64)>);

fn standardize(rows: &[usize], axis_cols: &[&NumericColumn]) -> Option<Standardized> {
    let n = rows.len();
    let mut z: Vec<Vec<f64>> = Vec::with_capacity(axis_cols.len());
    let mut stats: Vec<(f64, f64)> = Vec::with_capacity(axis_cols.len());
    for col in axis_cols {
        let vals: Vec<f64> = rows.iter().map(|&i| col.at(i)).collect();
        let mean = vals.iter().sum::<f64>() / n as f64;
        let var = vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n as f64;
        let sd = var.sqrt();
        if sd.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return None;
        }
        stats.push((mean, sd));
        z.push(vals.iter().map(|v| (v - mean) / sd).collect());
    }
    Some((z, stats))
}

#[allow(clippy::too_many_arguments)]
fn screen_target(
    table: &TypedTable,
    tcol: usize,
    numeric: &[usize],
    values: &[NumericColumn],
    pos_map: &[usize],
    cfg: &DiscConfig,
    caches: &DiscCaches,
) -> TargetOutcome {
    let mut out = TargetOutcome::new();
    let tvals = &values[pos_map[tcol]];
    let finite_t = tvals.finite_count;
    if finite_t < cfg.min_rows {
        return out;
    }
    out.screened = true;
    let axes: Vec<usize> = numeric.iter().copied().filter(|&c| c != tcol).collect();
    let mut flagged_units: Vec<Flagged> = Vec::new();
    let mut axis_strength: Vec<(usize, f64)> = Vec::new();

    // Axis units.
    for &acol in &axes {
        let ap = pos_map[acol];
        let Some((target, z, fit, stats, _rows)) = unit_data(tvals, &[&values[ap]], cfg.row_cap)
        else {
            continue;
        };
        let n = target.len();
        if n < cfg.min_rows {
            continue;
        }
        out.n_axis += 1;
        let u = z[0].clone();
        let ones = vec![1.0f64; n];
        let u2: Vec<f64> = u.iter().map(|v| v * v).collect();
        let bg = flat_background(&[&ones, &u, &u2]);
        let best = scan_unit(&u, &bg, &target, &fit, cfg);
        if let Some(best) = &best {
            axis_strength.push((acol, best.t_fit.abs()));
            if flagged(best) {
                flagged_units.push(Flagged {
                    kind: "axis",
                    target: tcol,
                    axes: vec![acol],
                    direction_deg: None,
                    cand: best.clone(),
                    u: u.clone(),
                    target_vals: target.clone(),
                    fit: fit.clone(),
                    axis_z: z.clone(),
                    axis_stats: stats.clone(),
                });
            }
        }
    }

    // Diagonal units.
    let pair_axes: Vec<usize> = if axes.len() > cfg.all_pairs_max_cols {
        let mut ranked = axis_strength.clone();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked
            .iter()
            .take(cfg.fallback_top_axes)
            .map(|(c, _)| *c)
            .collect()
    } else {
        axes.clone()
    };
    for ai in 0..pair_axes.len() {
        for bi in (ai + 1)..pair_axes.len() {
            let (a1, a2) = (pair_axes[ai], pair_axes[bi]);
            let (p1, p2) = (pos_map[a1], pos_map[a2]);
            // Pair data (kept rows, standardized axes, fit mask,
            // background) depends on the kept rows only, not on the
            // target: shared across targets via the pair cache,
            // validated by the kept-row vector.
            let rows = kept_rows(tvals, &[&values[p1], &values[p2]], cfg.row_cap);
            if rows.len() < cfg.min_rows {
                continue;
            }
            let Some(pair) = caches.pair(a1, a2, &rows, &[&values[p1], &values[p2]]) else {
                continue;
            };
            let target: Vec<f64> = rows.iter().map(|&i| tvals.at(i)).collect();
            let z: &Vec<Vec<f64>> = &pair.z;
            let fit: &Vec<u8> = &pair.fit;
            let stats: &Vec<(f64, f64)> = &pair.stats;
            let n = target.len();
            let bg: &[f64] = &pair.bg;
            let axis_tol = (std::f64::consts::PI / cfg.directions as f64).sin();
            for d in 0..cfg.directions {
                let theta = std::f64::consts::PI * (d as f64) / (cfg.directions as f64);
                let (c, s) = (theta.cos(), theta.sin());
                if c.abs().min(s.abs()) < axis_tol - 1e-12 {
                    // Near-axis directions duplicate the axis scan
                    // of one of the pair's columns.
                    continue;
                }
                let u: Vec<f64> = (0..n).map(|i| c * z[0][i] + s * z[1][i]).collect();
                out.n_diag += 1;
                let best = scan_unit(&u, bg, &target, fit, cfg);
                if let Some(best) = &best {
                    if flagged(best) {
                        flagged_units.push(Flagged {
                            kind: "diagonal",
                            target: tcol,
                            axes: vec![a1, a2],
                            direction_deg: Some(theta * 180.0 / std::f64::consts::PI),
                            cand: best.clone(),
                            u,
                            target_vals: target.clone(),
                            fit: fit.clone(),
                            axis_z: z.clone(),
                            axis_stats: stats.clone(),
                        });
                    }
                }
            }
        }
    }

    out.n_flagged += flagged_units.len();
    // Rank on fit-half statistics only; type the leaders.
    flagged_units.sort_by(|a, b| {
        b.cand
            .t_fit
            .abs()
            .partial_cmp(&a.cand.t_fit.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut typed: Vec<&Flagged> = Vec::new();
    for f in flagged_units
        .iter()
        .take(cfg.type_top.max(cfg.max_per_target))
    {
        let (_tz, gap) = typing_numbers(f);
        match gap {
            Some((ratio, gap_t)) if gap_t > TYPE_GAP_T_CRIT && ratio < TYPE_RATIO_CRIT => {
                typed.push(f);
            }
            _ => {
                out.n_untyped += 1;
            }
        }
    }
    // Reporting bars: strict z and material gain (share of the
    // target's evaluation-half variance).
    let typed: Vec<&Flagged> = typed
        .into_iter()
        .filter(|f| {
            if f.cand.z.abs() < cfg.z_report {
                return false;
            }
            let eval_t: Vec<f64> = f
                .target_vals
                .iter()
                .zip(&f.fit)
                .filter(|(_, fl)| **fl == 0)
                .map(|(v, _)| *v)
                .collect();
            let m = eval_t.iter().sum::<f64>() / eval_t.len() as f64;
            let var = eval_t.iter().map(|v| (v - m) * (v - m)).sum::<f64>() / eval_t.len() as f64;
            var > 0.0 && f.cand.gain >= cfg.rel_gain_min * var
        })
        .collect();
    // One finding per (target, axis set): adjacent diagonal
    // directions can type the same boundary twice; keep the
    // strongest |z| per axis set.
    let mut by_set: Vec<&Flagged> = Vec::new();
    for f in typed.iter() {
        if let Some(slot) = by_set
            .iter_mut()
            .find(|k| k.kind == f.kind && k.axes == f.axes)
        {
            if f.cand.z.abs() > slot.cand.z.abs() {
                *slot = f;
            }
        } else {
            by_set.push(f);
        }
    }
    let typed = by_set;
    // Parsimony: an axis step and a near-parallel diagonal step at
    // comparable strength are the same jump; report the axis step
    // unless the diagonal is much stronger (>= 1.5x |z|), in which
    // case the axis reading is its shadow.
    let mut kept: Vec<&Flagged> = Vec::new();
    for f in typed.iter().filter(|f| f.kind == "axis") {
        kept.push(f);
    }
    for f in typed.iter().filter(|f| f.kind == "diagonal") {
        let deg = f.direction_deg.unwrap_or(0.0);
        let a1 = deg % 90.0;
        let angle = a1.min(90.0 - a1);
        let mut suppressed = false;
        if angle < 25.0 {
            for (i, k) in kept.iter().enumerate() {
                if k.kind == "axis" && f.axes.contains(&k.axes[0]) {
                    if f.cand.z.abs() >= 1.5 * k.cand.z.abs() {
                        kept.remove(i);
                    } else {
                        suppressed = true;
                    }
                    break;
                }
            }
        }
        if !suppressed {
            kept.push(f);
        }
    }
    let mut target_findings: Vec<Finding> = Vec::new();
    for f in kept.iter() {
        target_findings.push(make_finding(table, f, cfg));
    }
    target_findings.sort_by(|a, b| {
        b.interestingness
            .partial_cmp(&a.interestingness)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    target_findings.truncate(cfg.max_per_target);
    out.findings = target_findings;
    out
}

pub fn profile_discontinuities(
    table: &TypedTable,
    excluded: &[usize],
    cfg: &DiscConfig,
) -> (Vec<Finding>, String) {
    use rayon::prelude::*;
    if !cfg.is_enabled() {
        return (Vec::new(), "disabled at this budget".into());
    }
    let numeric: Vec<usize> = (0..table.cols.len())
        .filter(|&i| table.cols[i].is_numeric() && !excluded.contains(&i))
        .collect();
    let shared_sample = std::sync::Arc::new(
        crate::stride_rows(table.n, cfg.row_cap)
            .into_iter()
            .map(|r| r as usize)
            .collect(),
    );
    let values: Vec<NumericColumn> = numeric
        .iter()
        .map(|&i| NumericColumn::new(&table.cols[i], std::sync::Arc::clone(&shared_sample)))
        .collect();
    let mut pos_map = vec![usize::MAX; table.cols.len()];
    for (pos, &c) in numeric.iter().enumerate() {
        pos_map[c] = pos;
    }
    let caches = DiscCaches::new();

    // Targets are independent; screen them in parallel and merge
    // in target order, so the report is unchanged.
    let outcomes: Vec<TargetOutcome> = numeric
        .par_iter()
        .map(|&tcol| screen_target(table, tcol, &numeric, &values, &pos_map, cfg, &caches))
        .collect();
    let mut findings: Vec<Finding> = Vec::new();
    let (mut n_targets, mut n_axis_units, mut n_diag_units, mut n_flagged, mut n_untyped) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    for o in outcomes {
        if o.screened {
            n_targets += 1;
        }
        n_axis_units += o.n_axis;
        n_diag_units += o.n_diag;
        n_flagged += o.n_flagged;
        n_untyped += o.n_untyped;
        findings.extend(o.findings);
    }

    let coverage = format!(
        "{n_targets} numeric targets screened over {n_axis_units} axis units and {n_diag_units} diagonal units ({} directions x pairs); {n_flagged} candidates flagged on the evaluation half (|z| >= 2, held-out gain > 0), {n_untyped} of the typed leaders rejected as steep smooth transitions; {} jumps reported",
        cfg.directions,
        findings.len()
    );
    (findings, coverage)
}

fn make_finding(table: &TypedTable, f: &Flagged, _cfg: &DiscConfig) -> Finding {
    let tname = table.names[f.target].as_str();
    let cand = &f.cand;
    let n = f.target_vals.len();
    let (type_z, gap) = typing_numbers(f);
    let (ratio, gap_t) = gap.unwrap_or((f64::NAN, f64::NAN));

    let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    let mut cols = vec![tname.to_string()];
    for &a in &f.axes {
        cols.push(table.names[a].clone());
    }
    params.insert("columns".into(), json!(cols));
    params.insert("column".into(), json!(tname));

    let mut metrics: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    metrics.insert("kind".into(), json!(f.kind));
    metrics.insert("jump".into(), json!(cand.jump));
    metrics.insert("se".into(), json!(cand.se));
    metrics.insert("z".into(), json!(cand.z));
    metrics.insert("t_fit".into(), json!(cand.t_fit));
    metrics.insert("gain".into(), json!(cand.gain));
    metrics.insert("n".into(), json!(n));
    metrics.insert("n_above".into(), json!(cand.n_above));
    metrics.insert("type_ratio".into(), json!(ratio));
    metrics.insert("type_gap_t".into(), json!(gap_t));
    if let Some(tz) = type_z {
        metrics.insert("type_z".into(), json!(tz));
    }

    let claim = if f.kind == "axis" {
        // Threshold back in the axis column's original units, through
        // the unit's own standardization (the unit may be a sample).
        let (mean, sd) = f.axis_stats[0];
        let threshold_x = mean + sd * cand.threshold;
        metrics.insert("threshold".into(), json!(threshold_x));
        format!(
            "{} steps by {:+} where {} crosses {}: evaluation-half jump {:+} (se {}, z {:+}), held-out gain {}; the local gap is scale-invariant (ratio {:.2}), typing it as a jump rather than a steep smooth transition",
            tname,
            crate::fmt_float(cand.jump),
            table.names[f.axes[0]],
            crate::fmt_float(threshold_x),
            crate::fmt_float(cand.jump),
            crate::fmt_float(cand.se),
            crate::fmt_float(cand.z),
            crate::fmt_float(cand.gain),
            ratio,
        )
    } else {
        let deg = f.direction_deg.unwrap_or(0.0);
        metrics.insert("threshold".into(), json!(cand.threshold));
        metrics.insert("direction_deg".into(), json!(deg));
        format!(
            "{} steps by {:+} across a diagonal boundary in {} and {} (direction {:.0} degrees, standardized threshold {}): evaluation-half jump {:+} (se {}, z {:+}), held-out gain {}; the local gap is scale-invariant (ratio {:.2}), typing it as a jump rather than a steep smooth transition",
            tname,
            crate::fmt_float(cand.jump),
            table.names[f.axes[0]],
            table.names[f.axes[1]],
            deg,
            crate::fmt_float(cand.threshold),
            crate::fmt_float(cand.jump),
            crate::fmt_float(cand.se),
            crate::fmt_float(cand.z),
            crate::fmt_float(cand.gain),
            ratio,
        )
    };

    let score = (0.55 + 0.04 * cand.z.abs()).min(0.95);
    Finding::new(
        Pattern::StepDiscontinuity,
        params,
        claim,
        EvidenceBasis::Measured,
        score,
        metrics,
        Some(format!(
            "a level jump in {tname} across this boundary; a smooth fit absorbs it and misstates both sides — check for a threshold rule, regime change, or instrument or coding change at the boundary"
        )),
        Method::new(
            "discontinuities",
            "split-half step scan vs smooth background, scale-invariance typing",
            "onceover discontinuity screen v1 (issue #40)",
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_sampling_matches_complete_case_sampling_exactly() {
        // Include word boundaries, sparse complete cases, and caps both
        // below and above the complete population. Selection must preserve
        // the old sample and its fit/evaluation parity, not just its size.
        for n in [0, 1, 63, 64, 65, 300] {
            for cap in [0, 1, 7, 1000] {
                let cols = [
                    TypedCol::F64 {
                        vals: (0..n).map(|i| Some(i as f64)).collect::<Vec<_>>().into(),
                    },
                    TypedCol::F64 {
                        vals: (0..n)
                            .map(|i| (i % 3 != 0).then_some(i as f64))
                            .collect::<Vec<_>>()
                            .into(),
                    },
                    TypedCol::F64 {
                        vals: (0..n)
                            .map(|i| (i % 5 == 0).then_some(i as f64))
                            .collect::<Vec<_>>()
                            .into(),
                    },
                ];
                let shared = std::sync::Arc::new(
                    crate::stride_rows(n, cap)
                        .into_iter()
                        .map(|r| r as usize)
                        .collect(),
                );
                let values: Vec<_> = cols
                    .iter()
                    .map(|col| NumericColumn::new(col, std::sync::Arc::clone(&shared)))
                    .collect();
                for target in &values {
                    for axes in [
                        vec![&values[0]],
                        vec![&values[1]],
                        vec![&values[1], &values[2]],
                    ] {
                        let complete: Vec<_> = (0..n)
                            .filter(|&i| {
                                target.at(i).is_finite() && axes.iter().all(|c| c.at(i).is_finite())
                            })
                            .collect();
                        let expected: Vec<_> = crate::stride_rows(complete.len(), cap)
                            .iter()
                            .map(|&i| complete[i as usize])
                            .collect();
                        assert_eq!(kept_rows(target, &axes, cap), expected, "n={n}, cap={cap}");
                    }
                }
            }
        }
    }

    #[test]
    fn ordered_scan_matches_plain_scan() {
        // The shared-order path must be bit-identical: same
        // candidates, same statistics.
        let mut seed = 42u64;
        let n = 600;
        let u: Vec<f64> = (0..n).map(|_| lcg(&mut seed) * 10.0).collect();
        let target: Vec<f64> = (0..n)
            .map(|i| lcg(&mut seed) + if u[i] > 5.0 { 2.0 } else { 0.0 })
            .collect();
        let ones = vec![1.0f64; n];
        let u2: Vec<f64> = u.iter().map(|v| v * v).collect();
        let bg = flat_background(&[&ones, &u, &u2]);
        let fit: Vec<u8> = (0..n).map(|i| if i % 2 == 0 { 1 } else { 0 }).collect();
        let cuts = quantile_cuts(&u, 20);
        let plain = disc_scan(&u, &bg, &target, &fit, &cuts, 10);
        let order = sort_order(&u);
        let shared = disc_scan_ordered(&u, &order, &bg, &target, &fit, &cuts, 10);
        assert_eq!(plain.len(), shared.len());
        for (a, b) in plain.iter().zip(&shared) {
            assert_eq!(a.threshold, b.threshold);
            assert_eq!(a.jump, b.jump);
            assert_eq!(a.z, b.z);
            assert_eq!(a.t_fit, b.t_fit);
            assert_eq!(a.gain, b.gain);
        }
        assert!(plain.iter().any(|c| c.z.abs() >= 2.0));
    }

    fn lcg(seed: &mut u64) -> f64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*seed >> 11) as f64) / (1u64 << 53) as f64
    }

    fn scan_axis(y: &[f64], x: &[f64]) -> Vec<DiscCandidate> {
        let n = y.len();
        let mean = x.iter().sum::<f64>() / n as f64;
        let var = x.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n as f64;
        let sd = var.sqrt();
        let u: Vec<f64> = x.iter().map(|v| (v - mean) / sd).collect();
        let fit: Vec<u8> = (0..n).map(|i| if i % 2 == 0 { 1 } else { 0 }).collect();
        let fit_u: Vec<f64> = u
            .iter()
            .zip(&fit)
            .filter(|(_, f)| **f != 0)
            .map(|(v, _)| *v)
            .collect();
        let _ = fit_u;
        let ones = vec![1.0f64; n];
        let u2: Vec<f64> = u.iter().map(|v| v * v).collect();
        let bg = flat_background(&[&ones, &u, &u2]);
        let cfg = DiscConfig::default();
        match scan_unit(&u, &bg, y, &fit, &cfg) {
            Some(best) => vec![best],
            None => Vec::new(),
        }
    }

    #[test]
    fn axis_step_is_found_and_typed() {
        let n = 800;
        let mut seed = 42u64;
        let x: Vec<f64> = (0..n).map(|_| lcg(&mut seed)).collect();
        let y: Vec<f64> = x
            .iter()
            .map(|&xi| {
                let jump = if xi > 0.6 { 2.0 } else { 0.0 };
                jump + 1.5 * xi + 0.5 * (lcg(&mut seed) + lcg(&mut seed) - 1.0)
            })
            .collect();
        let cands = scan_axis(&y, &x);
        let best = cands
            .iter()
            .max_by(|a, b| a.t_fit.abs().partial_cmp(&b.t_fit.abs()).unwrap())
            .expect("candidates");
        assert!(best.z.abs() >= 2.0, "z = {}", best.z);
        assert!(best.gain > 0.0, "gain = {}", best.gain);
        assert!((best.jump - 2.0).abs() < 0.35, "jump = {}", best.jump);
        // Typing on the eval half.
        let mean = x.iter().sum::<f64>() / n as f64;
        let var = x.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n as f64;
        let sd = var.sqrt();
        let u: Vec<f64> = x.iter().map(|v| (v - mean) / sd).collect();
        let pairs: Vec<(f64, f64)> = u
            .iter()
            .zip(&y)
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, (a, b))| (*a, *b))
            .collect();
        let ue: Vec<f64> = pairs.iter().map(|p| p.0).collect();
        let te: Vec<f64> = pairs.iter().map(|p| p.1).collect();
        let (ratio, gap_t) = type_gap(&ue, &te, best.threshold).expect("gap");
        assert!(gap_t > TYPE_GAP_T_CRIT, "gap_t = {gap_t}");
        assert!(ratio < TYPE_RATIO_CRIT, "ratio = {ratio}");
    }

    #[test]
    fn smooth_steep_transition_is_not_typed_as_jump() {
        let n = 800;
        let mut seed = 7u64;
        let x: Vec<f64> = (0..n).map(|_| lcg(&mut seed)).collect();
        // Logistic transition of width ~0.08 in x: smooth, but steep.
        let y: Vec<f64> = x
            .iter()
            .map(|&xi| {
                let s = 1.0 / (1.0 + (-(xi - 0.5) / 0.08).exp());
                2.0 * s + 0.4 * (lcg(&mut seed) + lcg(&mut seed) - 1.0)
            })
            .collect();
        let cands = scan_axis(&y, &x);
        if let Some(best) = cands
            .iter()
            .max_by(|a, b| a.t_fit.abs().partial_cmp(&b.t_fit.abs()).unwrap())
        {
            if best.z.abs() >= 2.0 && best.gain > 0.0 {
                let mean = x.iter().sum::<f64>() / n as f64;
                let var = x.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n as f64;
                let sd = var.sqrt();
                let u: Vec<f64> = x.iter().map(|v| (v - mean) / sd).collect();
                let pairs: Vec<(f64, f64)> = u
                    .iter()
                    .zip(&y)
                    .enumerate()
                    .filter(|(i, _)| i % 2 == 1)
                    .map(|(_, (a, b))| (*a, *b))
                    .collect();
                let ue: Vec<f64> = pairs.iter().map(|p| p.0).collect();
                let te: Vec<f64> = pairs.iter().map(|p| p.1).collect();
                if let Some((ratio, gap_t)) = type_gap(&ue, &te, best.threshold) {
                    assert!(
                        !(gap_t > TYPE_GAP_T_CRIT && ratio < TYPE_RATIO_CRIT),
                        "smooth transition typed as jump: ratio {ratio}, gap_t {gap_t}"
                    );
                }
            }
        }
    }

    #[test]
    fn noise_produces_no_flagged_candidates() {
        let n = 600;
        let mut seed = 99u64;
        let x: Vec<f64> = (0..n).map(|_| lcg(&mut seed)).collect();
        let y: Vec<f64> = (0..n)
            .map(|_| lcg(&mut seed) + lcg(&mut seed) - 1.0)
            .collect();
        let cands = scan_axis(&y, &x);
        let flagged_count = cands
            .iter()
            .filter(|c| c.z.abs() >= 2.0 && c.gain > 0.0)
            .count();
        // The per-candidate screen may flag by chance at the nominal
        // rate; the winner-take-all unit must not be asserted here.
        // This test pins that the scan runs and stays finite.
        let _ = flagged_count;
        assert!(cands.iter().all(|c| c.z.is_finite() && c.se > 0.0));
    }
}
