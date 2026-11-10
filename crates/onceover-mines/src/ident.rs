//! Arithmetic identity probe: y ≈ a·x + b on pairs with strong dependence
//! (the cheap ancestor of BACON/BHUNT-style law discovery).

use onceover_report::{Method, Pattern};
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

use onceover_report::{EvidenceBasis, Finding};
use serde_json::json;

use crate::dep::PairDep;

/// Fit and check y = a·x + b; returns (a, b, exact_frac) when the fit is
/// strong enough to report.
pub fn fit_affine(pairs: &[(f64, f64)]) -> Option<(f64, f64, f64)> {
    let n = pairs.len();
    if n < 100 {
        return None;
    }
    let mx = pairs.iter().map(|p| p.0).sum::<f64>() / n as f64;
    let my = pairs.iter().map(|p| p.1).sum::<f64>() / n as f64;
    let mut cov = 0.0;
    let mut vx = 0.0;
    for (x, y) in pairs {
        cov += (x - mx) * (y - my);
        vx += (x - mx) * (x - mx);
    }
    if vx <= 0.0 {
        return None;
    }
    let a = cov / vx;
    let b = my - a * mx;
    let tol = 1e-9;
    let exact = pairs
        .iter()
        .filter(|(x, y)| (y - (a * x + b)).abs() <= tol * (1.0 + y.abs()))
        .count();
    Some((a, b, exact as f64 / n as f64))
}

/// Fit y = c0 + c1*x1 + ... + ck*xk (k <= 3) by ordinary least squares on the given sampled rows.
/// Returns coefficients [c0, c1, ..., ck], or None when fewer than 100 rows have all values present
/// or the normal-equations matrix is singular (|pivot| < 1e-12 during Gaussian elimination with
/// partial pivoting).
pub fn fit_linear_sample(
    predictors: &[&onceover_core::TypedCol],
    target: &onceover_core::TypedCol,
    rows: &[u32],
) -> Option<Vec<f64>> {
    let k = predictors.len();
    if k > 3 {
        return None;
    }
    let n = k + 1;

    // Collect usable rows: every predictor and the target must be numeric.
    let mut xs: Vec<Vec<f64>> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for &row in rows {
        let row = row as usize;
        let mut x = Vec::with_capacity(n);
        x.push(1.0);
        let mut ok = true;
        for p in predictors {
            match p.numeric_at(row) {
                Some(v) => x.push(v),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            if let Some(y) = target.numeric_at(row) {
                xs.push(x);
                ys.push(y);
            }
        }
    }

    if xs.len() < 100 {
        return None;
    }

    // Build normal equations: A = X^T X, b = X^T y.
    let mut a = vec![vec![0.0; n]; n];
    let mut b = vec![0.0; n];
    for (i, x) in xs.iter().enumerate() {
        for r in 0..n {
            for c in 0..n {
                a[r][c] += x[r] * x[c];
            }
            b[r] += x[r] * ys[i];
        }
    }

    // Gaussian elimination with partial pivoting.
    for col in 0..n {
        // Find pivot row.
        let mut pivot = col;
        let mut best = a[col][col].abs();
        for (r, row) in a.iter().enumerate().skip(col + 1) {
            if row[col].abs() > best {
                best = row[col].abs();
                pivot = r;
            }
        }
        if best < 1e-12 {
            return None;
        }
        if pivot != col {
            a.swap(col, pivot);
            b.swap(col, pivot);
        }
        // Eliminate below.
        let (head, tail) = a.split_at_mut(col + 1);
        let prow = &head[col];
        let pval = prow[col];
        let pvec = b[col];
        for (dr, row) in tail.iter_mut().enumerate() {
            let r = dr + col + 1;
            let f = row[col] / pval;
            for (dst, src) in row[col..].iter_mut().zip(prow[col..].iter()) {
                *dst -= f * *src;
            }
            b[r] -= f * pvec;
        }
    }

    // Back substitution.
    let mut coeffs = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = b[r];
        for c in r + 1..n {
            s -= a[r][c] * coeffs[c];
        }
        coeffs[r] = s / a[r][r];
    }

    Some(coeffs)
}

/// Strict tolerance check shared by all identity scans: |y - predicted| <= 1e-9 * (1 + |y|).
fn within_tol(y: f64, predicted: f64) -> bool {
    (y - predicted).abs() <= 1e-9 * (1.0 + y.abs())
}

/// Loose tolerance check for sample screens: |y - predicted| <= 1e-6 * (1 + |y|).
fn within_loose_tol(y: f64, predicted: f64) -> bool {
    (y - predicted).abs() <= 1e-6 * (1.0 + y.abs())
}

/// Two-phase scan for a fixed-coefficient identity target = sum(coeffs[i] * preds[i]).
/// Phase 1 screens on sample_rows: the matching fraction within loose tolerance must be >= 0.99.
/// Phase 2 exact-scans all n_rows with strict tolerance and requires fraction >= 0.999.
/// Rows missing any value are excluded from both counts. Returns (n, matched) from the full scan,
/// or None when the sample screen fails, the full fraction is below 0.999, or either phase has
/// zero usable rows. Accepts between two and MAX_SPARSE_TERMS predictors: the pair and triple
/// enumerations call it with two or three, the components-first chase with more.
pub fn scan_fixed_linear(
    target: &onceover_core::TypedCol,
    predictors: &[&onceover_core::TypedCol],
    coeffs: &[f64],
    sample_rows: &[u32],
    n_rows: usize,
) -> Option<(usize, usize)> {
    if predictors.len() < 2
        || predictors.len() > MAX_SPARSE_TERMS
        || coeffs.len() < predictors.len()
    {
        return None;
    }

    let eval = |cols: &[&onceover_core::TypedCol], row: usize| -> Option<f64> {
        let mut acc = 0.0;
        for (i, c) in cols.iter().enumerate() {
            acc += coeffs[i] * c.numeric_at(row)?;
        }
        Some(acc)
    };

    let mut usable = 0usize;
    let mut matched = 0usize;
    for r in sample_rows {
        let row = *r as usize;
        let predicted = match eval(predictors, row) {
            Some(p) => p,
            None => continue,
        };
        let y = match target.numeric_at(row) {
            Some(v) => v,
            None => continue,
        };
        usable += 1;
        if within_loose_tol(y, predicted) {
            matched += 1;
        }
    }
    if usable == 0 || (matched as f64) / (usable as f64) < 0.99 {
        return None;
    }

    let mut n = 0usize;
    let mut m = 0usize;
    for row in 0..n_rows {
        let predicted = match eval(predictors, row) {
            Some(p) => p,
            None => continue,
        };
        let y = match target.numeric_at(row) {
            Some(v) => v,
            None => continue,
        };
        n += 1;
        if within_tol(y, predicted) {
            m += 1;
        }
    }
    if n == 0 || (m as f64) / (n as f64) < 0.999 {
        return None;
    }

    Some((n, m))
}

/// Two-phase scan for target = p0 * p1 (is_ratio false) or target = p0 / p1 (is_ratio true).
/// Phase 1 screens on sample_rows: the matching fraction within loose tolerance must be >= 0.99.
/// Phase 2 exact-scans all n_rows with strict tolerance and requires fraction >= 0.999.
/// Rows missing any value, and rows with divisor == 0 for ratios, are excluded from both counts,
/// never counted as violations. Returns (n, matched) from the full scan, or None when the sample
/// screen fails, the full fraction is below 0.999, or either phase has zero usable rows.
pub fn scan_product_ratio(
    target: &onceover_core::TypedCol,
    predictors: &[&onceover_core::TypedCol],
    is_ratio: bool,
    sample_rows: &[u32],
    n_rows: usize,
) -> Option<(usize, usize)> {
    if predictors.len() != 2 {
        return None;
    }
    let p0 = predictors[0];
    let p1 = predictors[1];
    let eval = |cols: (
        &onceover_core::TypedCol,
        &onceover_core::TypedCol,
        &onceover_core::TypedCol,
    ),
                row: usize|
     -> Option<f64> {
        let (_, a, b) = cols;
        let a = a.numeric_at(row)?;
        let b = b.numeric_at(row)?;
        if is_ratio {
            if b == 0.0 {
                None
            } else {
                Some(a / b)
            }
        } else {
            Some(a * b)
        }
    };
    let cols = (target, p0, p1);

    // Phase 1: sample screen with loose tolerance.
    let mut sample_n = 0usize;
    let mut sample_matched = 0usize;
    for &r in sample_rows {
        let row = r as usize;
        let predicted = match eval(cols, row) {
            Some(v) => v,
            None => continue,
        };
        let y = match target.numeric_at(row) {
            Some(v) => v,
            None => continue,
        };
        sample_n += 1;
        if within_loose_tol(y, predicted) {
            sample_matched += 1;
        }
    }
    if sample_n == 0 {
        return None;
    }
    let sample_fraction = sample_matched as f64 / sample_n as f64;
    if sample_fraction < 0.99 {
        return None;
    }

    // Phase 2: full scan with strict tolerance.
    let mut n = 0usize;
    let mut matched = 0usize;
    for row in 0..n_rows {
        let predicted = match eval(cols, row) {
            Some(v) => v,
            None => continue,
        };
        let y = match target.numeric_at(row) {
            Some(v) => v,
            None => continue,
        };
        n += 1;
        if within_tol(y, predicted) {
            matched += 1;
        }
    }
    if n == 0 {
        return None;
    }
    let fraction = matched as f64 / n as f64;
    if fraction < 0.999 {
        return None;
    }
    Some((n, matched))
}

/// Two-phase scan for a fitted linear combination target = c0 + c1*p1 + ... + ck*pk.
/// Phase 1 fits coefficients with fit_linear_sample on sample_rows, then screens: the matching
/// fraction within loose tolerance must be >= 0.99. Phase 2 exact-scans all n_rows with strict
/// tolerance and requires fraction >= 0.999. Rows missing any value are excluded from both
/// counts. Returns (coefficients, n, matched) from the full scan, or None when fitting fails,
/// the sample screen fails, the full fraction is below 0.999, or either phase has zero usable rows.
pub fn scan_linear_combo(
    target: &onceover_core::TypedCol,
    predictors: &[&onceover_core::TypedCol],
    sample_rows: &[u32],
    n_rows: usize,
) -> Option<(Vec<f64>, usize, usize)> {
    if predictors.len() != 2 && predictors.len() != 3 {
        return None;
    }

    let coeffs = fit_linear_sample(predictors, target, sample_rows)?;

    // Phase 1: sample screen with loose tolerance.
    let mut usable = 0usize;
    let mut matched = 0usize;
    for &r in sample_rows {
        let row = r as usize;
        let mut vals: Vec<f64> = Vec::with_capacity(predictors.len());
        let mut ok = true;
        for p in predictors {
            match p.numeric_at(row) {
                Some(v) => vals.push(v),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        let y = target.numeric_at(row)?;
        usable += 1;
        let mut predicted = coeffs[0];
        for i in 0..vals.len() {
            predicted += coeffs[i + 1] * vals[i];
        }
        if within_loose_tol(y, predicted) {
            matched += 1;
        }
    }
    if usable == 0 {
        return None;
    }
    let fraction = matched as f64 / usable as f64;
    if fraction < 0.99 {
        return None;
    }

    // Phase 2: full scan with strict tolerance.
    let mut full_usable = 0usize;
    let mut full_matched = 0usize;
    for row in 0..n_rows {
        let mut vals: Vec<f64> = Vec::with_capacity(predictors.len());
        let mut ok = true;
        for p in predictors {
            match p.numeric_at(row) {
                Some(v) => vals.push(v),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        let y = match target.numeric_at(row) {
            Some(v) => v,
            None => continue,
        };
        full_usable += 1;
        let mut predicted = coeffs[0];
        for i in 0..vals.len() {
            predicted += coeffs[i + 1] * vals[i];
        }
        if within_tol(y, predicted) {
            full_matched += 1;
        }
    }
    if full_usable == 0 {
        return None;
    }
    let full_fraction = full_matched as f64 / full_usable as f64;
    if full_fraction < 0.999 {
        return None;
    }

    Some((coeffs, full_usable, full_matched))
}

/// A verified multi-term identity, ready to become a Finding.
/// For sum/difference/product/ratio, coefficients are the fixed multipliers aligned with
/// predictors (unit/sign values; empty for product/ratio). For linear_combination, coefficients
/// are [intercept, c1, ..., ck] and has_intercept is true.
pub struct MultitermHit {
    pub form: &'static str,
    pub target: String,
    pub predictors: Vec<String>,
    pub coefficients: Vec<f64>,
    pub has_intercept: bool,
    pub n: usize,
    pub matched: usize,
    /// Set on a near identity: the largest residual among matching rows.
    pub near_within: Option<f64>,
}

fn build_multiterm_finding(hit: MultitermHit) -> Finding {
    let violations = hit.n - hit.matched;
    let agreement = if violations == 0 {
        format!("all {} non-missing rows", hit.n)
    } else {
        format!(
            "{} of {} non-missing rows ({} violation{})",
            hit.matched,
            hit.n,
            violations,
            if violations == 1 { "" } else { "s" }
        )
    };

    let target = &hit.target;

    let claim = match hit.form {
        "sum" | "difference" => {
            // Positive terms first, so `-a + b` reads `b - a`.
            let sign = |i: usize| hit.coefficients.get(i).copied().unwrap_or(1.0) >= 0.0;
            let order = (0..hit.predictors.len())
                .filter(|&i| sign(i))
                .chain((0..hit.predictors.len()).filter(|&i| !sign(i)));
            let mut terms = String::new();
            for (at, i) in order.enumerate() {
                let p = &hit.predictors[i];
                match (at, sign(i)) {
                    (0, true) => terms.push_str(p),
                    (0, false) => terms.push_str(&format!("-{}", p)),
                    (_, true) => terms.push_str(&format!(" + {}", p)),
                    (_, false) => terms.push_str(&format!(" - {}", p)),
                }
            }
            match hit.near_within {
                Some(band) => format!("{} = {} within {:.6} on {}", target, terms, band, agreement),
                None => format!("{} = {} within tolerance on {}", target, terms, agreement),
            }
        }
        "product" => {
            let p1 = hit.predictors.first().map(|s| s.as_str()).unwrap_or("");
            let p2 = hit.predictors.get(1).map(|s| s.as_str()).unwrap_or("");
            format!(
                "{} = {} · {} within tolerance on {}",
                target, p1, p2, agreement
            )
        }
        "ratio" => {
            let p1 = hit.predictors.first().map(|s| s.as_str()).unwrap_or("");
            let p2 = hit.predictors.get(1).map(|s| s.as_str()).unwrap_or("");
            format!(
                "{} = {} / {} within tolerance on {}",
                target, p1, p2, agreement
            )
        }
        _ => {
            let mut terms = String::new();
            for (i, p) in hit.predictors.iter().enumerate() {
                let c = hit.coefficients.get(i + 1).copied().unwrap_or(0.0);
                if i == 0 {
                    terms.push_str(&format!("{:.6}·{}", c, p));
                } else if c >= 0.0 {
                    terms.push_str(&format!(" + {:.6}·{}", c, p));
                } else {
                    terms.push_str(&format!(" - {:.6}·{}", c.abs(), p));
                }
            }
            let mut claim = format!("{} = {}", target, terms);
            if hit.has_intercept {
                let intercept = hit.coefficients.first().copied().unwrap_or(0.0);
                let scale = 1.0
                    + hit
                        .coefficients
                        .get(1..)
                        .map(|cs| cs.iter().map(|c| c.abs()).sum::<f64>())
                        .unwrap_or(0.0);
                if intercept.abs() >= 1e-12 * scale {
                    if intercept >= 0.0 {
                        claim.push_str(&format!(" + {:.6}", intercept));
                    } else {
                        claim.push_str(&format!(" - {:.6}", intercept.abs()));
                    }
                }
            }
            format!("{} within tolerance on {}", claim, agreement)
        }
    };

    let mut subject = BTreeMap::new();
    subject.insert("predictors".to_string(), json!(hit.predictors));
    subject.insert("target".to_string(), json!(hit.target));

    let mut metrics = BTreeMap::new();
    metrics.insert("form".to_string(), json!(hit.form));
    metrics.insert("terms".to_string(), json!(hit.predictors));
    metrics.insert("target".to_string(), json!(hit.target));
    metrics.insert("coefficients".to_string(), json!(hit.coefficients));
    metrics.insert("n".to_string(), json!(hit.n));
    metrics.insert("matching_rows".to_string(), json!(hit.matched));
    metrics.insert("violations".to_string(), json!(violations));
    metrics.insert(
        "exact_fraction".to_string(),
        json!(hit.matched as f64 / hit.n.max(1) as f64),
    );
    match hit.near_within {
        Some(band) => {
            metrics.insert("tolerance".to_string(), json!(format!("within {band}")));
            metrics.insert("absolute_tolerance".to_string(), json!(band));
            metrics.insert("relative_tolerance".to_string(), json!(0.0));
        }
        None => {
            metrics.insert(
                "tolerance".to_string(),
                json!("1e-9 absolute + 1e-9 relative"),
            );
            metrics.insert("absolute_tolerance".to_string(), json!(1e-9));
            metrics.insert("relative_tolerance".to_string(), json!(1e-9));
        }
    }

    let preds = hit.predictors.join(", ");
    let interpretation = if violations == 0 {
        format!(
            "{} is derived from {}; drop it or treat as a check column",
            target, preds
        )
    } else {
        format!(
            "inspect {} violating rows before treating {} as derived from {}",
            violations, target, preds
        )
    };

    Finding::new(
        Pattern::ArithmeticIdentity,
        subject,
        claim,
        EvidenceBasis::Measured,
        0.75,
        metrics,
        Some(interpretation),
        Method::new(
            "identities",
            "multiterm_scan",
            "multiterm exact scan (BHUNT/BACON lineage, onceover v0.1)",
        ),
    )
}

/// Filter multi-term hits down to minimal predictor sets.
///
/// Two rules, both evaluated against the original, unfiltered hit list:
///
/// Rule A (unused predictor): for a fitted `linear_combination`, a predictor
/// coefficient whose absolute value is <= 1e-6 * scale (scale = 1 + sum of the
/// absolute predictor coefficients) means the identity does not actually use
/// that predictor, so the hit is a non-minimal extension of a smaller identity —
/// drop it.
///
/// Rule B (verified proper subset): if some other hit has the same target and a
/// proper subset of this hit's predictors, a smaller verified identity already
/// exists — drop the superset. A hit dropped by Rule A still counts as a verified
/// subset witness for Rule B.
///
/// The input order of surviving hits is preserved.
fn minimal_hits(hits: Vec<MultitermHit>) -> Vec<MultitermHit> {
    // Predictor sets over the ORIGINAL list, so Rule B sees hits Rule A drops.
    let orig_sets: Vec<(&str, BTreeSet<&str>)> = hits
        .iter()
        .map(|h| {
            (
                h.target.as_str(),
                h.predictors.iter().map(|s| s.as_str()).collect(),
            )
        })
        .collect();

    let keep: Vec<bool> = hits
        .iter()
        .enumerate()
        .map(|(i, h)| {
            // Rule A: unused predictor in a fitted linear combination
            // (index 0 is the intercept; skip when there are no predictor coeffs).
            if h.form == "linear_combination" && h.coefficients.len() >= 2 {
                let scale = 1.0 + h.coefficients[1..].iter().map(|c| c.abs()).sum::<f64>();
                if h.coefficients[1..].iter().any(|c| c.abs() <= 1e-6 * scale) {
                    return false;
                }
            }
            // Rule B: a verified hit for the same target whose predictor set is a
            // proper subset of this hit's predictor set.
            let set = &orig_sets[i].1;
            if orig_sets.iter().any(|(target, other)| {
                *target == h.target.as_str() && other.len() < set.len() && other.is_subset(set)
            }) {
                return false;
            }
            true
        })
        .collect();

    hits.into_iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(hit, _)| hit)
        .collect()
}

/// Largest number of terms the greedy sparse search will assemble.
const MAX_SPARSE_TERMS: usize = 16;
/// Unexplained variance share at which the greedy fit tries pruning.
const SPARSE_PRUNE_GAP: f64 = 1e-4;
/// Unexplained variance share of an accepted sparse fit. Rounding of a few
/// units on a total ranging over tens of thousands sits near 1e-8.
const SPARSE_DONE_GAP: f64 = 1e-7;
/// Above this width the greedy search is skipped: its cost is quadratic
/// in columns for every target. Coverage states the skip.
const SPARSE_MAX_COLUMNS: usize = 1_000;
/// Rows may miss the sample screen's 99% bar by at most this many out of
/// the 512-row sample, so a sixth miss rejects a candidate immediately.
const SAMPLE_MISSES_ALLOWED: usize = 5;
/// Fitted combinations below this sample R² cannot pass the strict scan.
const LINEAR_GATE_R2: f64 = 0.9;
/// A near identity leaves residuals within this share of the target's
/// central range (p90-p10): the size of rounding in reported totals.
const NEAR_RESIDUAL_SHARE: f64 = 1e-3;
/// Largest number of terms the components-first chase will assemble.
/// The enumeration covers two and three terms and the greedy fitted
/// search reaches further when its fit behaves; the chase exists for
/// exact decompositions of four to eight parts, the reported shape.
const MAX_CHASE_TERMS: usize = 8;
/// A column is a plausible part of a target when it lies between zero
/// and the target on at least this share of the sample rows where
/// both are present: a part cannot exceed the whole it belongs to.
const PART_BOUND_SHARE: f64 = 0.99;
/// Smallest number of sample rows where both target and candidate
/// are present before the part bound is judged.
const MIN_PART_ROWS: usize = 50;
/// When more columns pass the part bound, the chase keeps this many,
/// the largest by mean share of the target; a decomposition's parts
/// carry material shares of the whole, and the cap keeps the search
/// linear in the table's width per step.
const CHASE_POOL_MAX: usize = 24;
/// A chase step that leaves the miss count unchanged must cut the
/// mean absolute residual to at most this share of its previous
/// value for the search to continue.
const CHASE_PROGRESS: f64 = 0.98;

/// Sampled values of every usable column, NaN where missing.
struct SampleMatrix {
    cols: Vec<Vec<f64>>,
}

impl SampleMatrix {
    fn build(table: &onceover_core::TypedTable, numeric: &[usize], rows: &[u32]) -> Self {
        let cols = numeric
            .iter()
            .map(|&c| {
                rows.iter()
                    .map(|&r| table.cols[c].numeric_at(r as usize).unwrap_or(f64::NAN))
                    .collect()
            })
            .collect();
        Self { cols }
    }

    /// Pearson correlation over pairwise-complete rows, using at most
    /// `limit` rows. None when either column is constant there.
    fn corr(&self, a: usize, b: usize, limit: usize) -> Option<f64> {
        let (mut n, mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        for (&x, &y) in self.cols[a].iter().zip(&self.cols[b]).take(limit) {
            if x.is_nan() || y.is_nan() {
                continue;
            }
            n += 1.0;
            sx += x;
            sy += y;
            sxx += x * x;
            syy += y * y;
            sxy += x * y;
        }
        if n < 3.0 {
            return None;
        }
        let vx = sxx - sx * sx / n;
        let vy = syy - sy * sy / n;
        (vx > 0.0 && vy > 0.0).then(|| (sxy - sx * sy / n) / (vx * vy).sqrt())
    }
}

fn has_three_values(col: &[f64]) -> bool {
    let mut seen: Vec<f64> = Vec::with_capacity(3);
    for &v in col {
        if !v.is_nan() && !seen.contains(&v) {
            seen.push(v);
            if seen.len() == 3 {
                return true;
            }
        }
    }
    false
}

/// True unless more rows miss than the sample screen can tolerate.
fn screen_rows(n: usize, predicted: impl Fn(usize) -> Option<f64>, y: &[f64]) -> bool {
    let mut misses = 0;
    for (r, &yr) in y.iter().enumerate().take(n) {
        if yr.is_nan() {
            continue;
        }
        let Some(p) = predicted(r) else { continue };
        if p.is_nan() {
            continue;
        }
        if !within_loose_tol(yr, p) {
            misses += 1;
            if misses > SAMPLE_MISSES_ALLOWED {
                return false;
            }
        }
    }
    true
}

/// The largest k with C(k, r) at most `budget`.
fn pool_for_budget(budget: usize, r: usize, available: usize) -> usize {
    let mut k = 0usize;
    while k < available {
        let next = match r {
            2 => (k + 1) * k / 2,
            _ => (k + 1) * k * k.saturating_sub(1) / 6,
        };
        if next > budget {
            break;
        }
        k += 1;
    }
    k
}

/// Sample R² of target on 2 or 3 predictors from pairwise correlations.
/// Collinear predictors return 1.0: the gate defers to the exact fit.
fn r2_from_corr(pred_pred: &[Vec<f64>], pred_target: &[f64]) -> f64 {
    let k = pred_target.len();
    let mut a: Vec<Vec<f64>> = pred_pred.to_vec();
    let mut b: Vec<f64> = pred_target.to_vec();
    for col in 0..k {
        let pivot = (col..k)
            .max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))
            .unwrap_or(col);
        if a[pivot][col].abs() < 1e-10 {
            return 1.0;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let (head, tail) = a.split_at_mut(col + 1);
        let pivot_row = &head[col];
        for (offset, row) in tail.iter_mut().enumerate() {
            let f = row[col] / pivot_row[col];
            for (dst, src) in row[col..].iter_mut().zip(&pivot_row[col..]) {
                *dst -= f * *src;
            }
            b[col + 1 + offset] -= f * b[col];
        }
    }
    let mut x = vec![0.0; k];
    for r in (0..k).rev() {
        let mut v = b[r];
        for c in r + 1..k {
            v -= a[r][c] * x[c];
        }
        x[r] = v / a[r][r];
    }
    x.iter().zip(pred_target).map(|(c, r)| c * r).sum()
}

/// [intercept, c1, ..., ck], share of variance explained, residuals, and the
/// rows the residuals align with.
type SparseFit = (Vec<f64>, f64, Vec<f64>, Vec<usize>);

/// Least-squares fit of the target on `selected` over rows complete in all
/// of them.
fn fit_selected(
    mat: &SampleMatrix,
    t: usize,
    candidates: &[usize],
    selected: &[usize],
) -> Option<SparseFit> {
    let y = &mat.cols[t];
    let rows: Vec<usize> = (0..y.len())
        .filter(|&r| {
            !y[r].is_nan()
                && selected
                    .iter()
                    .all(|&p| !mat.cols[candidates[p]][r].is_nan())
        })
        .collect();
    if rows.len() < 2 * selected.len() + 50 {
        return None;
    }
    let k = selected.len() + 1;
    let mut a = vec![vec![0.0; k]; k];
    let mut b = vec![0.0; k];
    let mut x = Vec::with_capacity(k);
    for &r in &rows {
        x.clear();
        x.push(1.0);
        x.extend(selected.iter().map(|&p| mat.cols[candidates[p]][r]));
        for i in 0..k {
            for j in 0..k {
                a[i][j] += x[i] * x[j];
            }
            b[i] += x[i] * y[r];
        }
    }
    let coeffs = solve(a, b)?;
    let residual: Vec<f64> = rows
        .iter()
        .map(|&r| {
            let mut p = coeffs[0];
            for (i, &q) in selected.iter().enumerate() {
                p += coeffs[i + 1] * mat.cols[candidates[q]][r];
            }
            y[r] - p
        })
        .collect();
    let n = rows.len() as f64;
    let mean = rows.iter().map(|&r| y[r]).sum::<f64>() / n;
    let var_y = rows
        .iter()
        .map(|&r| (y[r] - mean) * (y[r] - mean))
        .sum::<f64>();
    if var_y <= 0.0 {
        return None;
    }
    let rss = residual.iter().map(|e| e * e).sum::<f64>();
    Some((coeffs, 1.0 - rss / var_y, residual, rows))
}

/// Components-first search for an exact unit-coefficient identity
/// over more terms than the enumeration reaches.
///
/// The candidate parts are drawn from the columns the target
/// plausibly decomposes into: a column enters the pool only when it
/// lies between zero and the target on at least 99% of the sample
/// rows where both are present — a part cannot exceed the whole.
/// That one screen is what keeps the search bounded and honest: it
/// excludes scaled copies of the target and unrelated measures
/// larger than it, however strongly they correlate with it, and the
/// pool is capped at the 24 largest by mean share of the target.
///
/// The chase then subtracts parts from the target on the sample,
/// one at a time: at each step it takes the signed part whose
/// removal leaves the fewest sample rows outside the loose identity
/// tolerance, breaking ties by the smaller mean absolute residual.
/// A step that reduces neither the miss count nor (by at least 2%)
/// the mean absolute residual ends the search. No coefficients are
/// fitted anywhere, so collinear or duplicated columns cannot make
/// the search fail the way a singular least-squares fit does. When
/// the sample residual falls to at most SAMPLE_MISSES_ALLOWED
/// misses, the caller verifies the assembled form on every row with
/// the pass's usual exact scan; the chase itself claims nothing.
///
/// Returns the chosen (column position, sign) terms, or None.
fn unit_chase(mat: &SampleMatrix, t: usize, candidates: &[usize]) -> Option<Vec<(usize, f64)>> {
    let y = &mat.cols[t];
    let complete = y.iter().filter(|v| !v.is_nan()).count();
    if complete < 100 {
        return None;
    }
    let loose = |yv: f64| 1e-6 * (1.0 + yv.abs());
    // The part-bound pool, in column order.
    let mut pool: Vec<usize> = candidates
        .iter()
        .copied()
        .filter(|&c| {
            let col = &mat.cols[c];
            let (mut n, mut inside) = (0usize, 0usize);
            for r in 0..y.len() {
                if y[r].is_nan() || col[r].is_nan() {
                    continue;
                }
                n += 1;
                if col[r] >= -loose(y[r]) && col[r] <= y[r] + loose(y[r]) {
                    inside += 1;
                }
            }
            n >= MIN_PART_ROWS && (inside as f64) >= PART_BOUND_SHARE * n as f64
        })
        .collect();
    if pool.len() > CHASE_POOL_MAX {
        // Largest mean share of the target first; column order
        // among ties, then restored to column order for the search.
        let share = |c: usize| -> f64 {
            let col = &mat.cols[c];
            let (mut sc, mut sy, mut n) = (0.0, 0.0, 0.0);
            for r in 0..y.len() {
                if !y[r].is_nan() && !col[r].is_nan() {
                    sc += col[r];
                    sy += y[r];
                    n += 1.0;
                }
            }
            if n > 0.0 && sy > 0.0 {
                sc / sy
            } else {
                0.0
            }
        };
        pool.sort_by(|&a, &b| share(b).total_cmp(&share(a)).then_with(|| a.cmp(&b)));
        pool.truncate(CHASE_POOL_MAX);
        pool.sort_unstable();
    }
    if pool.len() < 2 {
        return None;
    }

    // (misses, mean absolute residual) of a residual vector over its
    // usable rows: rows where the target and every chosen part are
    // present, NaN elsewhere.
    let score = |resid: &[f64]| -> (usize, f64, usize) {
        let (mut misses, mut sum, mut usable) = (0usize, 0.0, 0usize);
        for r in 0..y.len() {
            if resid[r].is_nan() {
                continue;
            }
            usable += 1;
            sum += resid[r].abs();
            if resid[r].abs() > loose(y[r]) {
                misses += 1;
            }
        }
        (misses, sum / usable.max(1) as f64, usable)
    };

    let mut resid: Vec<f64> = y.clone();
    let (mut cur_misses, mut cur_mean, _) = score(&resid);
    let mut chosen: Vec<(usize, f64)> = Vec::new();
    for _step in 0..MAX_CHASE_TERMS {
        if cur_misses <= SAMPLE_MISSES_ALLOWED && chosen.len() >= 2 {
            return Some(chosen);
        }
        let mut best: Option<(usize, f64, Vec<f64>, usize, f64)> = None;
        for &c in &pool {
            if chosen.iter().any(|&(p, _)| p == c) {
                continue;
            }
            let col = &mat.cols[c];
            for sign in [1.0, -1.0] {
                let next: Vec<f64> = (0..y.len())
                    .map(|r| {
                        if resid[r].is_nan() || col[r].is_nan() {
                            f64::NAN
                        } else {
                            resid[r] - sign * col[r]
                        }
                    })
                    .collect();
                let (misses, mean, usable) = score(&next);
                if usable < MIN_PART_ROWS {
                    continue;
                }
                let better = match &best {
                    None => true,
                    Some((_, _, _, bm, bmean)) => misses < *bm || (misses == *bm && mean < *bmean),
                };
                if better {
                    best = Some((c, sign, next, misses, mean));
                }
            }
        }
        let (c, sign, next, misses, mean) = best?;
        let progress = misses < cur_misses || mean <= CHASE_PROGRESS * cur_mean;
        if !progress {
            break;
        }
        chosen.push((c, sign));
        resid = next;
        cur_misses = misses;
        cur_mean = mean;
    }
    if cur_misses <= SAMPLE_MISSES_ALLOWED && chosen.len() >= 2 {
        Some(chosen)
    } else {
        None
    }
}

/// Greedy sparse regression of the target over the sample: add the column
/// most correlated with the current residual, refit, repeat. Returns the
/// selected columns (positions in `candidates`) and [intercept, c1, ..., ck]
/// once the fit leaves no visible residual. An exact sum of many terms
/// surfaces here because each removed term raises the next one's
/// correlation with what remains, including summands far smaller than the
/// total, at a cost linear in columns per step. Correlated columns can enter
/// on the way, so a nearly complete fit is pruned backwards: any term whose
/// removal leaves the fit as good is dropped.
fn greedy_sparse_fit(
    mat: &SampleMatrix,
    t: usize,
    candidates: &[usize],
) -> Option<(Vec<usize>, Vec<f64>)> {
    let y = &mat.cols[t];
    let mut rows: Vec<usize> = (0..y.len()).filter(|&r| !y[r].is_nan()).collect();
    if rows.len() < 100 {
        return None;
    }
    let mean = rows.iter().map(|&r| y[r]).sum::<f64>() / rows.len() as f64;
    let mut residual: Vec<f64> = rows.iter().map(|&r| y[r] - mean).collect();
    let mut selected: Vec<usize> = Vec::new();

    for step in 0..MAX_SPARSE_TERMS.min(candidates.len()) {
        let mut best: Option<(usize, f64)> = None;
        for (pos, &c) in candidates.iter().enumerate() {
            if selected.contains(&pos) {
                continue;
            }
            let (mut n, mut sx, mut sxx, mut sxy, mut sr) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for (i, &r) in rows.iter().enumerate() {
                let x = mat.cols[c][r];
                if x.is_nan() {
                    continue;
                }
                n += 1.0;
                sx += x;
                sxx += x * x;
                sxy += x * residual[i];
                sr += residual[i];
            }
            if n < 50.0 {
                continue;
            }
            let vx = sxx - sx * sx / n;
            if vx <= 0.0 {
                continue;
            }
            let score = (sxy - sx * sr / n).abs() / vx.sqrt();
            if best.is_none_or(|(_, s)| score > s) {
                best = Some((pos, score));
            }
        }
        let (pos, _) = best?;
        selected.push(pos);
        let (_, explained, next_residual, next_rows) = fit_selected(mat, t, candidates, &selected)?;
        residual = next_residual;
        rows = next_rows;
        if explained >= 1.0 - SPARSE_PRUNE_GAP {
            let mut pruned = selected.clone();
            let mut at = 0;
            while at < pruned.len() && pruned.len() > 1 {
                let mut without = pruned.clone();
                without.remove(at);
                match fit_selected(mat, t, candidates, &without) {
                    Some((_, e, _, _)) if e >= 1.0 - SPARSE_DONE_GAP => pruned = without,
                    _ => at += 1,
                }
            }
            let (final_coeffs, final_explained, _, _) = fit_selected(mat, t, candidates, &pruned)?;
            if final_explained >= 1.0 - SPARSE_DONE_GAP {
                return Some((pruned, final_coeffs));
            }
        }
        // An unrelated target never reaches a usable fit; stop paying for it.
        if step >= 3 && explained < 0.5 {
            return None;
        }
    }
    None
}

/// Gaussian elimination with partial pivoting.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let k = b.len();
    for col in 0..k {
        let pivot = (col..k).max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))?;
        if a[pivot][col].abs() < 1e-9 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let (head, tail) = a.split_at_mut(col + 1);
        let pivot_row = &head[col];
        for (offset, row) in tail.iter_mut().enumerate() {
            let f = row[col] / pivot_row[col];
            for (dst, src) in row[col..].iter_mut().zip(&pivot_row[col..]) {
                *dst -= f * *src;
            }
            b[col + 1 + offset] -= f * b[col];
        }
    }
    let mut x = vec![0.0; k];
    for r in (0..k).rev() {
        let mut v = b[r];
        for c in r + 1..k {
            v -= a[r][c] * x[c];
        }
        x[r] = v / a[r][r];
    }
    Some(x)
}

/// Verify a greedy fit over every row. Coefficients within 1e-6 of ±1 with
/// a negligible intercept snap to the unit-coefficient form. Exact rows use
/// the strict tolerance; when the exact bar fails but residuals stay within
/// the rounding-sized band, the hit records that band as `near_within`.
fn verify_sparse(
    table: &onceover_core::TypedTable,
    t: usize,
    numeric: &[usize],
    picks: &[usize],
    coeffs: &[f64],
) -> Option<MultitermHit> {
    let mut sorted: Vec<f64> = (0..table.n)
        .filter_map(|r| table.cols[t].numeric_at(r))
        .collect();
    sorted.sort_by(f64::total_cmp);
    if sorted.is_empty() {
        return None;
    }
    let q = |f: f64| sorted[((f * (sorted.len() - 1) as f64) as usize).min(sorted.len() - 1)];
    let band = NEAR_RESIDUAL_SHARE * (q(0.9) - q(0.1));
    // Least squares absorbs rounding into the coefficients and intercept;
    // the unit form is the statement when they sit this close to it.
    let unit = coeffs[1..].iter().all(|c| (c.abs() - 1.0).abs() <= 1e-3)
        && coeffs[0].abs() <= band.max(1e-9);
    let used: Vec<f64> = if unit {
        coeffs[1..].iter().map(|c| c.signum()).collect()
    } else {
        coeffs[1..].to_vec()
    };
    let intercept = if unit { 0.0 } else { coeffs[0] };
    let predictors: Vec<usize> = picks.iter().map(|&p| numeric[p]).collect();
    let (mut n, mut exact) = (0usize, 0usize);
    let mut residuals: Vec<f64> = Vec::new();
    'rows: for r in 0..table.n {
        let Some(y) = table.cols[t].numeric_at(r) else {
            continue;
        };
        let mut p = intercept;
        for (&c, &k) in predictors.iter().zip(&used) {
            let Some(x) = table.cols[c].numeric_at(r) else {
                continue 'rows;
            };
            p += k * x;
        }
        n += 1;
        if within_tol(y, p) {
            exact += 1;
        }
        residuals.push((y - p).abs());
    }
    if n < 100 {
        return None;
    }
    let names: Vec<String> = predictors.iter().map(|&c| table.names[c].clone()).collect();
    let target = table.names[t].clone();
    let (form, coefficients, has_intercept) = if unit {
        let form = if used.iter().all(|&c| c > 0.0) {
            "sum"
        } else {
            "difference"
        };
        (form, used.clone(), false)
    } else {
        let mut all = vec![intercept];
        all.extend_from_slice(&used);
        ("linear_combination", all, true)
    };
    if exact as f64 / n as f64 >= 0.999 {
        return Some(MultitermHit {
            form,
            target,
            predictors: names,
            coefficients,
            has_intercept,
            n,
            matched: exact,
            near_within: None,
        });
    }
    if !unit {
        return None;
    }
    residuals.sort_by(f64::total_cmp);
    let at_bar = residuals[((0.999 * (n - 1) as f64).ceil() as usize).min(n - 1)];
    if band <= 0.0 || at_bar > band {
        return None;
    }
    let within = residuals.iter().filter(|&&d| d <= at_bar).count();
    Some(MultitermHit {
        form,
        target,
        predictors: names,
        coefficients,
        has_intercept,
        n,
        matched: within,
        near_within: Some(at_bar),
    })
}

/// Screen and verify multi-term identities over the table's numeric columns.
/// Returns (findings, coverage string). `multiterm_cap` is the total
/// candidate budget, split evenly across targets so that no column loses
/// its turn by sitting late in the frame; when a target's combinations
/// exceed its share, the predictors most correlated with it are tried first.
/// Results are independent of the thread count.
pub fn multiterm_identities(
    table: &onceover_core::TypedTable,
    multiterm_cap: usize,
    excluded_targets: &[usize],
) -> (Vec<Finding>, String) {
    if multiterm_cap == 0 {
        return (
            vec![],
            "0 multi-term candidates screened, 0 multi-term identities".into(),
        );
    }
    let sample_rows = crate::stride_rows(table.n, 512);
    let n_rows = table.n;

    // Collect usable numeric column indices.
    let numeric_cols: Vec<usize> = table
        .cols
        .iter()
        .enumerate()
        .filter(|(_, col)| {
            matches!(
                col,
                onceover_core::TypedCol::I64 { .. }
                    | onceover_core::TypedCol::U64 { .. }
                    | onceover_core::TypedCol::F64 { .. }
            )
        })
        .map(|(idx, _)| idx)
        .filter(|&c| {
            let vals: Vec<f64> = sample_rows
                .iter()
                .filter_map(|&r| table.cols[c].numeric_at(r as usize))
                .collect();
            if vals.len() < 2 {
                return false;
            }
            let mean = vals.iter().sum::<f64>() / vals.len() as f64;
            let var = vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / vals.len() as f64;
            var != 0.0
        })
        .collect();

    let mat = SampleMatrix::build(table, &numeric_cols, &sample_rows);
    // A two-valued column is a flag: arithmetic on it is logic, and its
    // zeros make products and ratios hold by construction.
    let graded: Vec<bool> = mat.cols.iter().map(|c| has_three_values(c)).collect();
    let targets: Vec<usize> = (0..numeric_cols.len())
        .filter(|&t| graded[t] && !excluded_targets.contains(&numeric_cols[t]))
        .collect();
    let per_target = multiterm_cap / targets.len().max(1);
    let width = numeric_cols.len();
    let priority_rows = (40_000_000 / (width * width).max(1)).clamp(16, 512);
    let run_sparse = width <= SPARSE_MAX_COLUMNS && per_target > 0;

    let spare = multiterm_cap % targets.len().max(1);
    let outcomes: Vec<(Vec<MultitermHit>, usize, usize)> = targets
        .par_iter()
        .enumerate()
        .map(|(at, &t)| {
            multiterm_for_target(
                table,
                &mat,
                &numeric_cols,
                &graded,
                t,
                per_target + usize::from(at < spare),
                priority_rows,
                run_sparse,
                &sample_rows,
                n_rows,
            )
        })
        .collect();

    let mut hits: Vec<MultitermHit> = Vec::new();
    let (mut screened, mut possible) = (0usize, 0usize);
    for (h, s, p) in outcomes {
        hits.extend(h);
        screened += s;
        possible = possible.saturating_add(p);
    }

    let hits = one_statement_per_identity(minimal_hits(hits), table, &numeric_cols);
    let out: Vec<Finding> = hits.into_iter().map(build_multiterm_finding).collect();

    let mut coverage = format!(
        "{} multi-term candidates screened, {} multi-term identities",
        screened,
        out.len()
    );
    if screened < possible {
        coverage.push_str(&format!(
            "; {possible} candidates possible, so each target tried its most correlated predictors first"
        ));
    }
    if !run_sparse && per_target > 0 {
        coverage.push_str(&format!(
            "; sums of more than three terms not searched above {SPARSE_MAX_COLUMNS} numeric columns"
        ));
    }
    (out, coverage)
}

#[allow(clippy::too_many_arguments)]
fn multiterm_for_target(
    table: &onceover_core::TypedTable,
    mat: &SampleMatrix,
    numeric_cols: &[usize],
    graded: &[bool],
    t: usize,
    budget: usize,
    priority_rows: usize,
    run_sparse: bool,
    sample_rows: &[u32],
    n_rows: usize,
) -> (Vec<MultitermHit>, usize, usize) {
    let tc = numeric_cols[t];
    let others: Vec<usize> = (0..numeric_cols.len()).filter(|&c| c != t).collect();
    let m = others.len();
    let n_pairs = m.saturating_mul(m.saturating_sub(1)) / 2;
    let n_triples = n_pairs.saturating_mul(m.saturating_sub(2)) / 3;
    let possible = n_pairs.saturating_add(n_triples);

    // Most correlated first, column order among ties, when the budget binds.
    let ranked: Vec<usize> = if possible > budget {
        let mut scored: Vec<(usize, f64)> = others
            .iter()
            .map(|&c| {
                let r = mat.corr(t, c, priority_rows).unwrap_or(0.0).abs();
                (c, if r.is_nan() { 0.0 } else { r })
            })
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        scored.into_iter().map(|(c, _)| c).collect()
    } else {
        others.clone()
    };
    let pair_k = pool_for_budget(budget, 2, m);
    let pairs_used = pair_k * pair_k.saturating_sub(1) / 2;
    let triple_k = pool_for_budget(budget - pairs_used, 3, m);

    let y = &mat.cols[t];
    let rows = y.len();
    let mut hits: Vec<MultitermHit> = Vec::new();
    let mut screened = 0usize;

    let mut pair_pool: Vec<usize> = ranked[..pair_k].to_vec();
    pair_pool.sort_unstable();
    for (w, &a) in pair_pool.iter().enumerate() {
        for &b in &pair_pool[w + 1..] {
            screened += 1;
            let (xa, xb) = (&mat.cols[a], &mat.cols[b]);
            let (ca, cb) = (&table.cols[numeric_cols[a]], &table.cols[numeric_cols[b]]);
            let names = || {
                vec![
                    table.names[numeric_cols[a]].clone(),
                    table.names[numeric_cols[b]].clone(),
                ]
            };
            let hit_fixed = |coeffs: [f64; 2], form: &'static str| {
                if !screen_rows(rows, |r| Some(coeffs[0] * xa[r] + coeffs[1] * xb[r]), y) {
                    return None;
                }
                scan_fixed_linear(&table.cols[tc], &[ca, cb], &coeffs, sample_rows, n_rows).map(
                    |(n, matched)| MultitermHit {
                        form,
                        target: table.names[tc].clone(),
                        predictors: names(),
                        coefficients: coeffs.to_vec(),
                        has_intercept: false,
                        n,
                        matched,
                        near_within: None,
                    },
                )
            };
            if let Some(h) = hit_fixed([1.0, 1.0], "sum")
                .or_else(|| hit_fixed([1.0, -1.0], "difference"))
                .or_else(|| hit_fixed([-1.0, 1.0], "difference"))
            {
                hits.push(h);
                continue;
            }
            let both_graded = graded[a] && graded[b];
            if both_graded && screen_rows(rows, |r| Some(xa[r] * xb[r]), y) {
                if let Some((n, matched)) =
                    scan_product_ratio(&table.cols[tc], &[ca, cb], false, sample_rows, n_rows)
                {
                    hits.push(MultitermHit {
                        form: "product",
                        target: table.names[tc].clone(),
                        predictors: names(),
                        coefficients: vec![],
                        has_intercept: false,
                        n,
                        matched,
                        near_within: None,
                    });
                    continue;
                }
            }
            let ratio_screen = |num: &[f64], den: &[f64]| {
                screen_rows(rows, |r| (den[r] != 0.0).then(|| num[r] / den[r]), y)
            };
            if both_graded && ratio_screen(xa, xb) {
                if let Some((n, matched)) =
                    scan_product_ratio(&table.cols[tc], &[ca, cb], true, sample_rows, n_rows)
                {
                    hits.push(MultitermHit {
                        form: "ratio",
                        target: table.names[tc].clone(),
                        predictors: names(),
                        coefficients: vec![],
                        has_intercept: false,
                        n,
                        matched,
                        near_within: None,
                    });
                    continue;
                }
            }
            if both_graded && ratio_screen(xb, xa) {
                if let Some((n, matched)) =
                    scan_product_ratio(&table.cols[tc], &[cb, ca], true, sample_rows, n_rows)
                {
                    hits.push(MultitermHit {
                        form: "ratio",
                        target: table.names[tc].clone(),
                        predictors: vec![
                            table.names[numeric_cols[b]].clone(),
                            table.names[numeric_cols[a]].clone(),
                        ],
                        coefficients: vec![],
                        has_intercept: false,
                        n,
                        matched,
                        near_within: None,
                    });
                    continue;
                }
            }
            let (Some(rat), Some(rbt), Some(rab)) = (
                mat.corr(a, t, usize::MAX),
                mat.corr(b, t, usize::MAX),
                mat.corr(a, b, usize::MAX),
            ) else {
                continue;
            };
            if r2_from_corr(&[vec![1.0, rab], vec![rab, 1.0]], &[rat, rbt]) < LINEAR_GATE_R2 {
                continue;
            }
            if let Some((coeffs, n, matched)) =
                scan_linear_combo(&table.cols[tc], &[ca, cb], sample_rows, n_rows)
            {
                hits.push(MultitermHit {
                    form: "linear_combination",
                    target: table.names[tc].clone(),
                    predictors: names(),
                    coefficients: coeffs,
                    has_intercept: true,
                    n,
                    matched,
                    near_within: None,
                });
            }
        }
    }

    let mut triple_pool: Vec<usize> = ranked[..triple_k].to_vec();
    triple_pool.sort_unstable();
    // Pairwise correlations inside the pool, computed once for the gate.
    let pool_corr: Vec<Vec<f64>> = triple_pool
        .iter()
        .map(|&p| {
            triple_pool
                .iter()
                .map(|&q| {
                    if p == q {
                        1.0
                    } else {
                        mat.corr(p, q, usize::MAX).unwrap_or(0.0)
                    }
                })
                .collect()
        })
        .collect();
    let pool_target: Vec<f64> = triple_pool
        .iter()
        .map(|&p| mat.corr(p, t, usize::MAX).unwrap_or(0.0))
        .collect();
    for w in 0..triple_pool.len() {
        for x in w + 1..triple_pool.len() {
            for z in x + 1..triple_pool.len() {
                screened += 1;
                let (i, j, k) = (triple_pool[w], triple_pool[x], triple_pool[z]);
                let (xi, xj, xk) = (&mat.cols[i], &mat.cols[j], &mat.cols[k]);
                let cols = [
                    &table.cols[numeric_cols[i]],
                    &table.cols[numeric_cols[j]],
                    &table.cols[numeric_cols[k]],
                ];
                let preds = || {
                    vec![
                        table.names[numeric_cols[i]].clone(),
                        table.names[numeric_cols[j]].clone(),
                        table.names[numeric_cols[k]].clone(),
                    ]
                };
                let mut found = false;
                for (coeffs, form) in [
                    ([1.0f64, 1.0, 1.0], "sum"),
                    ([1.0, 1.0, -1.0], "difference"),
                    ([1.0, -1.0, 1.0], "difference"),
                    ([-1.0, 1.0, 1.0], "difference"),
                ] {
                    let ok = screen_rows(
                        rows,
                        |r| Some(coeffs[0] * xi[r] + coeffs[1] * xj[r] + coeffs[2] * xk[r]),
                        y,
                    );
                    if !ok {
                        continue;
                    }
                    if let Some((n, matched)) =
                        scan_fixed_linear(&table.cols[tc], &cols, &coeffs, sample_rows, n_rows)
                    {
                        hits.push(MultitermHit {
                            form,
                            target: table.names[tc].clone(),
                            predictors: preds(),
                            coefficients: coeffs.to_vec(),
                            has_intercept: false,
                            n,
                            matched,
                            near_within: None,
                        });
                        found = true;
                        break;
                    }
                }
                if found {
                    continue;
                }
                let sub = [w, x, z];
                let pp: Vec<Vec<f64>> = sub
                    .iter()
                    .map(|&p| sub.iter().map(|&q| pool_corr[p][q]).collect())
                    .collect();
                let pt: Vec<f64> = sub.iter().map(|&p| pool_target[p]).collect();
                if r2_from_corr(&pp, &pt) < LINEAR_GATE_R2 {
                    continue;
                }
                if let Some((coeffs, n, matched)) =
                    scan_linear_combo(&table.cols[tc], &cols, sample_rows, n_rows)
                {
                    hits.push(MultitermHit {
                        form: "linear_combination",
                        target: table.names[tc].clone(),
                        predictors: preds(),
                        coefficients: coeffs,
                        has_intercept: true,
                        n,
                        matched,
                        near_within: None,
                    });
                }
            }
        }
    }

    // Exact decompositions into more parts than the enumeration
    // reaches, found components-first: the chase assembles signed
    // parts bounded by the target and the assembled form is verified
    // on every row by the same exact scan the enumeration uses.
    if run_sparse && !others.is_empty() {
        if let Some(picks) = unit_chase(mat, t, &others) {
            let chosen: Vec<usize> = picks.iter().map(|&(c, _)| c).collect();
            let duplicate = hits.iter().any(|h| {
                h.predictors.len() == chosen.len()
                    && chosen
                        .iter()
                        .all(|&c| h.predictors.contains(&table.names[numeric_cols[c]]))
            });
            if !duplicate && chosen.len() >= 2 {
                let mut order: Vec<usize> = (0..picks.len()).collect();
                order.sort_by_key(|&k| picks[k].0);
                let pred_cols: Vec<&onceover_core::TypedCol> = order
                    .iter()
                    .map(|&k| &table.cols[numeric_cols[picks[k].0]])
                    .collect();
                let coeffs: Vec<f64> = order.iter().map(|&k| picks[k].1).collect();
                if let Some((n, matched)) =
                    scan_fixed_linear(&table.cols[tc], &pred_cols, &coeffs, sample_rows, n_rows)
                {
                    hits.push(MultitermHit {
                        form: if coeffs.iter().all(|&c| c > 0.0) {
                            "sum"
                        } else {
                            "difference"
                        },
                        target: table.names[tc].clone(),
                        predictors: order
                            .iter()
                            .map(|&k| table.names[numeric_cols[picks[k].0]].clone())
                            .collect(),
                        coefficients: coeffs,
                        has_intercept: false,
                        n,
                        matched,
                        near_within: None,
                    });
                }
            }
        }
    }

    // Sums too long to enumerate, and near identities of any length.
    if run_sparse && !others.is_empty() {
        if let Some((picks, coeffs)) = greedy_sparse_fit(mat, t, &others) {
            let chosen: Vec<usize> = picks.iter().map(|&p| others[p]).collect();
            let duplicate = hits.iter().any(|h| {
                h.predictors.len() == chosen.len()
                    && chosen
                        .iter()
                        .all(|&c| h.predictors.contains(&table.names[numeric_cols[c]]))
            });
            // One term is the affine pass's statement, not a multi-term one.
            if !duplicate && chosen.len() >= 2 {
                if let Some(h) = verify_sparse(table, tc, numeric_cols, &chosen, &coeffs) {
                    hits.push(h);
                }
            }
        }
    }
    (hits, screened, possible)
}

/// `cnt = casual + registered`, `casual = cnt - registered` and
/// `registered = cnt - casual` are one fact, as are the rotations of a fitted
/// linear combination. Keep one statement per column set: the all-positive
/// sum when there is one, and among equals the target with the largest
/// typical magnitude, which is the total.
fn one_statement_per_identity(
    hits: Vec<MultitermHit>,
    table: &onceover_core::TypedTable,
    numeric_cols: &[usize],
) -> Vec<MultitermHit> {
    let magnitude = |name: &str| -> f64 {
        numeric_cols
            .iter()
            .find(|&&c| table.names[c] == name)
            .map(|&c| {
                let (mut sum, mut n) = (0.0, 0.0);
                for r in crate::stride_rows(table.n, 512) {
                    if let Some(v) = table.cols[c].numeric_at(r as usize) {
                        sum += v.abs();
                        n += 1.0;
                    }
                }
                if n > 0.0 {
                    sum / n
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0)
    };
    let family = |h: &MultitermHit| -> Option<BTreeSet<String>> {
        matches!(h.form, "sum" | "difference" | "linear_combination").then(|| {
            let mut all: BTreeSet<String> = h.predictors.iter().cloned().collect();
            all.insert(h.target.clone());
            all
        })
    };
    let mut best: BTreeMap<BTreeSet<String>, usize> = BTreeMap::new();
    for (i, h) in hits.iter().enumerate() {
        let Some(key) = family(h) else { continue };
        let better = match best.get(&key) {
            None => true,
            Some(&j) => {
                let other = &hits[j];
                let rank = |x: &MultitermHit| (x.form == "sum", magnitude(&x.target));
                let (a, b) = (rank(h), rank(other));
                (a.0 && !b.0) || (a.0 == b.0 && a.1 > b.1)
            }
        };
        if better {
            best.insert(key, i);
        }
    }
    let keep: BTreeSet<usize> = best.values().copied().collect();
    hits.into_iter()
        .enumerate()
        .filter(|(i, h)| family(h).is_none() || keep.contains(i))
        .map(|(_, h)| h)
        .collect()
}

/// Build identity findings from strong pairs. A pair whose fitted
/// target (the pair's second column) is frame-role scaffolding is
/// skipped: an identity *on* a counter or timestamp part restates
/// the frame's construction.
pub fn identities(
    table: &onceover_core::TypedTable,
    deps: &[PairDep],
    pair_cap: usize,
    excluded_targets: &[usize],
) -> (Vec<Finding>, String) {
    use rayon::prelude::*;

    let candidates: Vec<&PairDep> = deps
        .iter()
        .filter(|d| !excluded_targets.contains(&d.b))
        .filter(|d| {
            d.pearson.map(|v| v.abs() >= 0.8).unwrap_or(false)
                || d.xi_ab.map(|v| v >= 0.9).unwrap_or(false)
        })
        .take(pair_cap)
        .collect();

    let fit_rows = crate::stride_rows(table.n, 20_000);
    let checked: Vec<_> = candidates
        .par_iter()
        .filter_map(|d| {
            let x = &table.cols[d.a];
            let y = &table.cols[d.b];
            let pairs = crate::dep::sampled_pairs(x, y, &fit_rows);
            let (a, b, _) = fit_affine(&pairs)?;
            let mut n = 0usize;
            let mut matched = 0usize;
            for row in 0..table.n {
                if let (Some(x), Some(y)) = (x.numeric_at(row), y.numeric_at(row)) {
                    n += 1;
                    matched += usize::from((y - (a * x + b)).abs() <= 1e-9 * (1.0 + y.abs()));
                }
            }
            let frac = matched as f64 / n.max(1) as f64;
            (frac >= 0.999).then_some((d.a, d.b, a, b, frac, n, matched))
        })
        .collect();

    let mut out = Vec::new();
    for (a_idx, b_idx, a, b, frac, n, matched) in checked {
        let x = &table.names[a_idx];
        let y = &table.names[b_idx];
        let violations = n - matched;
        let agreement = if violations == 0 {
            format!("all {n} non-missing row pairs")
        } else {
            format!(
                "{matched} of {n} non-missing row pairs ({violations} violation{})",
                if violations == 1 { "" } else { "s" }
            )
        };
        let claim = if b.abs() < 1e-12 * (1.0 + a.abs()) {
            format!("{y} = {a:.6}·{x} within tolerance on {agreement}")
        } else {
            format!("{y} = {a:.6}·{x} + {b:.6} within tolerance on {agreement}")
        };
        out.push(Finding::new(
Pattern::AffineDependency,
BTreeMap::from([("x".into(), json!(x)), ("y".into(), json!(y))]),
claim,
EvidenceBasis::Measured,
0.75,
BTreeMap::from([
                ("n".into(), json!(n)),
                ("matching_rows".into(), json!(matched)),
                ("violations".into(), json!(violations)),
                ("a".into(), json!(a)),
                ("b".into(), json!(b)),
                ("exact_fraction".into(), json!(frac)),
                ("tolerance".into(), json!("1e-9 absolute + 1e-9 relative")),
                ("absolute_tolerance".into(),json!(1e-9)),
                ("relative_tolerance".into(),json!(1e-9)),
            ]),
Some(if violations == 0 {
                format!("{y} is derived from {x}; drop it or treat as a check column")
            } else {
                format!("inspect {violations} violating row pairs before treating {y} as derived from {x}")
            }),
Method::new("identities", "ols_residual_scan", "pairwise OLS + exact residual scan (BHUNT/BACON lineage, onceover v0.1)")
));
    }
    let coverage = format!(
        "{} candidate pairs checked, {} identities",
        candidates.len(),
        out.len()
    );
    (out, coverage)
}
#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::{TypedCol, TypedTable};

    fn make_col(data: &[f64]) -> TypedCol<'_> {
        TypedCol::F64 {
            vals: data.iter().copied().map(Some).collect::<Vec<_>>().into(),
        }
    }

    #[test]
    fn exact_two_term_sum_found() {
        let n: usize = 200;
        let a: Vec<f64> = (0..n).map(|i| i as f64 * 0.5).collect();
        let b: Vec<f64> = (0..n).map(|i| (i % 7) as f64).collect();
        let c: Vec<f64> = (0..n).map(|i| a[i] + b[i]).collect();
        let sample_rows: Vec<u32> = (0..n as u32).collect();
        let target = make_col(&c);
        let col_a = make_col(&a);
        let col_b = make_col(&b);
        let predictors: Vec<&TypedCol> = vec![&col_a, &col_b];
        let res = scan_fixed_linear(&target, &predictors, &[1.0, 1.0], &sample_rows, n);
        assert_eq!(res, Some((200, 200)));
    }

    #[test]
    fn exact_three_term_sum_found() {
        let n: usize = 200;
        let a: Vec<f64> = (0..n).map(|i| i as f64 * 0.5).collect();
        let b: Vec<f64> = (0..n).map(|i| (i % 7) as f64).collect();
        let d: Vec<f64> = (0..n).map(|i| (i % 3) as f64).collect();
        let c: Vec<f64> = (0..n).map(|i| a[i] + b[i] + d[i]).collect();
        let sample_rows: Vec<u32> = (0..n as u32).collect();
        let target = make_col(&c);
        let col_a = make_col(&a);
        let col_b = make_col(&b);
        let col_d = make_col(&d);
        let predictors: Vec<&TypedCol> = vec![&col_a, &col_b, &col_d];
        let res = scan_fixed_linear(&target, &predictors, &[1.0, 1.0, 1.0], &sample_rows, n);
        assert_eq!(res, Some((200, 200)));
    }

    #[test]
    fn exact_difference_found() {
        let n: usize = 200;
        let a: Vec<f64> = (0..n).map(|i| i as f64 * 0.5).collect();
        let b: Vec<f64> = (0..n).map(|i| (i % 7) as f64).collect();
        let c: Vec<f64> = (0..n).map(|i| a[i] - b[i]).collect();
        let sample_rows: Vec<u32> = (0..n as u32).collect();
        let target = make_col(&c);
        let col_a = make_col(&a);
        let col_b = make_col(&b);
        let predictors: Vec<&TypedCol> = vec![&col_a, &col_b];
        let res = scan_fixed_linear(&target, &predictors, &[1.0, -1.0], &sample_rows, n);
        assert_eq!(res, Some((200, 200)));
    }

    #[test]
    fn exact_product_found() {
        let n: usize = 200;
        let x1: Vec<f64> = (0..n).map(|i| (i + 1) as f64).collect();
        let x2: Vec<f64> = (0..n).map(|i| ((i % 5) + 1) as f64).collect();
        let y: Vec<f64> = (0..n).map(|i| x1[i] * x2[i]).collect();
        let sample_rows: Vec<u32> = (0..n as u32).collect();
        let target = make_col(&y);
        let col_x1 = make_col(&x1);
        let col_x2 = make_col(&x2);
        let predictors: Vec<&TypedCol> = vec![&col_x1, &col_x2];
        let res = scan_product_ratio(&target, &predictors, false, &sample_rows, n);
        assert_eq!(res, Some((200, 200)));
    }

    #[test]
    fn exact_ratio_found_and_zero_divisors_excluded() {
        let n: usize = 200;
        let x1: Vec<f64> = (0..n).map(|i| (i + 1) as f64).collect();
        let x2: Vec<f64> = (0..n)
            .map(|i| {
                if i % 20 == 0 {
                    0.0
                } else {
                    ((i % 9) + 1) as f64
                }
            })
            .collect();
        let y: Vec<f64> = (0..n)
            .map(|i| if x2[i] == 0.0 { 0.0 } else { x1[i] / x2[i] })
            .collect();
        let sample_rows: Vec<u32> = (0..n as u32).collect();
        let target = make_col(&y);
        let col_x1 = make_col(&x1);
        let col_x2 = make_col(&x2);
        let predictors: Vec<&TypedCol> = vec![&col_x1, &col_x2];
        let res = scan_product_ratio(&target, &predictors, true, &sample_rows, n);
        assert_eq!(res, Some((190, 190)));
    }

    #[test]
    fn near_miss_rejected() {
        let n: usize = 200;
        let a: Vec<f64> = (0..n).map(|i| i as f64 * 0.5).collect();
        let b: Vec<f64> = (0..n).map(|i| (i % 7) as f64).collect();
        let mut c: Vec<f64> = (0..n).map(|i| a[i] + b[i]).collect();
        c[13] *= 1.0 + 1e-6;
        let sample_rows: Vec<u32> = (0..n as u32).collect();
        let target = make_col(&c);
        let col_a = make_col(&a);
        let col_b = make_col(&b);
        let predictors: Vec<&TypedCol> = vec![&col_a, &col_b];
        let res = scan_fixed_linear(&target, &predictors, &[1.0, 1.0], &sample_rows, n);
        assert_eq!(res, None);
    }

    #[test]
    fn linear_combo_found() {
        let n: usize = 200;
        let x1: Vec<f64> = (0..n).map(|i| i as f64 * 0.25).collect();
        let x2: Vec<f64> = (0..n).map(|i| (i % 11) as f64).collect();
        let y: Vec<f64> = (0..n).map(|i| 2.5 * x1[i] - 1.25 * x2[i] + 7.0).collect();
        let sample_rows: Vec<u32> = (0..n as u32).collect();
        let target = make_col(&y);
        let col_x1 = make_col(&x1);
        let col_x2 = make_col(&x2);
        let predictors: Vec<&TypedCol> = vec![&col_x1, &col_x2];
        let res = scan_linear_combo(&target, &predictors, &sample_rows, n);
        match res {
            Some((coeffs, n_ok, n_tot)) => {
                assert_eq!((n_ok, n_tot), (200, 200));
                let expected = [7.0, 2.5, -1.25];
                for (c, e) in coeffs.iter().zip(expected.iter()) {
                    assert!(
                        (c - e).abs() < 1e-6,
                        "coefficient mismatch: got {}, expected {}",
                        c,
                        e
                    );
                }
            }
            None => panic!("expected Some((coeffs, 200, 200)), got None"),
        }
    }

    #[test]
    fn linear_combo_near_miss_rejected() {
        let n: usize = 200;
        let x1: Vec<f64> = (0..n).map(|i| i as f64 * 0.25).collect();
        let x2: Vec<f64> = (0..n).map(|i| (i % 11) as f64).collect();
        let mut y: Vec<f64> = (0..n).map(|i| 2.5 * x1[i] - 1.25 * x2[i] + 7.0).collect();
        y[42] *= 1.0 + 1e-6;
        let sample_rows: Vec<u32> = (0..n as u32).collect();
        let target = make_col(&y);
        let col_x1 = make_col(&x1);
        let col_x2 = make_col(&x2);
        let predictors: Vec<&TypedCol> = vec![&col_x1, &col_x2];
        let res = scan_linear_combo(&target, &predictors, &sample_rows, n);
        assert!(res.is_none());
    }

    #[test]
    fn long_sum_found_past_enumeration() {
        // Eight parts: far beyond the triples the enumeration reaches.
        let n = 300usize;
        let parts: Vec<Vec<f64>> = (0..8u64)
            .map(|k| {
                (0..n as u64)
                    .map(|i| {
                        let h = (i + 17 * k + 1).wrapping_mul(0x9e3779b97f4a7c15) >> 40;
                        (h % 10_000) as f64 / (k as f64 + 1.0)
                    })
                    .collect()
            })
            .collect();
        let total: Vec<f64> = (0..n)
            .map(|i| parts.iter().map(|p| p[i]).sum::<f64>())
            .collect();
        let mut cols: Vec<TypedCol<'_>> = parts.iter().map(|p| make_col(p)).collect();
        cols.push(make_col(&total));
        let mut names: Vec<String> = (0..8).map(|k| format!("p{k}")).collect();
        names.push("total".into());
        let dtypes = vec!["float64".to_string(); 9];
        let table = onceover_core::TypedTable {
            n,
            names,
            dtypes,
            cols,
        };
        let (findings, _) = multiterm_identities(&table, 200_000, &[]);
        assert!(
            findings
                .iter()
                .any(|f| f.claim.starts_with("total = p0 + ") && f.claim.contains("p7")),
            "{:?}",
            findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exact_four_term_sum_scanned() {
        // The fixed scan accepts more than three predictors: the
        // wider searches verify their assembled forms through it.
        let n: usize = 200;
        let a: Vec<f64> = (0..n).map(|i| i as f64 * 0.5).collect();
        let b: Vec<f64> = (0..n).map(|i| (i % 7) as f64).collect();
        let d: Vec<f64> = (0..n).map(|i| (i % 3) as f64).collect();
        let e: Vec<f64> = (0..n).map(|i| (i % 5) as f64 * 2.0).collect();
        let c: Vec<f64> = (0..n).map(|i| a[i] + b[i] + d[i] + e[i]).collect();
        let sample_rows: Vec<u32> = (0..n as u32).collect();
        let target = make_col(&c);
        let cols: Vec<TypedCol> = [&a, &b, &d, &e].iter().map(|v| make_col(v)).collect();
        let predictors: Vec<&TypedCol> = cols.iter().collect();
        let res = scan_fixed_linear(&target, &predictors, &[1.0, 1.0, 1.0, 1.0], &sample_rows, n);
        assert_eq!(res, Some((200, 200)));
    }

    /// A total split among four parts whose shares substitute for
    /// one another (the total's budget is nearly fixed, so no part
    /// correlates with the total), plus a scaled copy of the total
    /// and two small decoy counts. The fitted greedy search takes
    /// the scaled copy first and stops; the enumeration reaches
    /// only three terms. `perturb` adds 50 to the total on every
    /// twentieth row, breaking the identity.
    fn substitution_panel(perturb: bool) -> TypedTable<'static> {
        fn owned_col(data: &[f64]) -> TypedCol<'static> {
            TypedCol::F64 {
                vals: data.iter().copied().map(Some).collect::<Vec<_>>().into(),
            }
        }
        let n = 2000usize;
        let hash = |i: u64, k: u64| -> f64 {
            let h = (i + 17 * k + 1).wrapping_mul(0x9e3779b97f4a7c15) >> 40;
            (h % 10_000) as f64
        };
        let mut cols: Vec<Vec<f64>> = Vec::new();
        let mut total = vec![0.0; n];
        let mut parts: Vec<Vec<f64>> = vec![vec![0.0; n]; 4];
        for i in 0..n {
            let budget = 20_000.0 + hash(i as u64, 91) / 10_000.0 * 6_000.0;
            let raw: Vec<f64> = (0..4u64).map(|k| hash(i as u64, k) + 500.0).collect();
            let sum: f64 = raw.iter().sum();
            let mut acc = 0.0;
            for (k, part) in parts.iter_mut().enumerate() {
                let v = if k < 3 {
                    (budget * raw[k] / sum).floor()
                } else {
                    budget - acc
                };
                part[i] = v;
                acc += v;
            }
            total[i] = budget;
            if perturb && i % 20 == 0 {
                total[i] += 50.0;
            }
        }
        let billed: Vec<f64> = total.iter().map(|t| (t * 1.07).round()).collect();
        let shown: Vec<f64> = (0..n).map(|i| hash(i as u64, 55) % 400.0).collect();
        let accepted: Vec<f64> = shown
            .iter()
            .enumerate()
            .map(|(i, s)| (s * (0.3 + 0.4 * hash(i as u64, 77) / 10_000.0)).floor())
            .collect();
        for part in &parts {
            cols.push(part.clone());
        }
        cols.push(total);
        cols.push(billed);
        cols.push(shown);
        cols.push(accepted);
        let names: Vec<String> = [
            "chat",
            "completion",
            "embedding",
            "image",
            "total",
            "billed",
            "shown",
            "accepted",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        TypedTable {
            n,
            names,
            dtypes: vec!["float64".to_string(); 8],
            cols: cols.iter().map(|c| owned_col(c)).collect(),
        }
    }

    #[test]
    fn four_part_sum_found_components_first() {
        let table = substitution_panel(false);
        let (findings, _) = multiterm_identities(&table, 200_000, &[]);
        let hit = findings
            .iter()
            .find(|f| f.subject.target == Some("total".into()));
        let hit = hit.expect("the four-part sum is reported");
        assert_eq!(hit.specification.form.as_deref(), Some("sum"));
        let m = &hit.evidence[0].metrics;
        assert_eq!(m["matching_rows"], json!(2000));
        assert_eq!(m["violations"], json!(0));
        let terms: Vec<&str> = m["terms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        assert_eq!(terms, ["chat", "completion", "embedding", "image"]);
        hit.validate().unwrap();
    }

    #[test]
    fn near_sum_is_not_an_identity() {
        // The same panel with the total off by 50 on 5% of rows:
        // past the exactness standard and past the rounding band,
        // so no identity — exact or near — is stated on the total.
        let table = substitution_panel(true);
        let (findings, _) = multiterm_identities(&table, 200_000, &[]);
        assert!(
            findings
                .iter()
                .all(|f| f.subject.target != Some("total".into())),
            "{:?}",
            findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
    }

    #[test]
    fn leading_negative_term_is_rendered_with_its_sign() {
        let finding = build_multiterm_finding(MultitermHit {
            form: "difference",
            target: "t".into(),
            predictors: vec!["a".into(), "b".into()],
            coefficients: vec![-1.0, 1.0],
            has_intercept: false,
            n: 10,
            matched: 10,
            near_within: None,
        });
        assert!(finding.claim.starts_with("t = b - a"), "{}", finding.claim);
    }

    #[test]
    fn budget_does_not_depend_on_column_position() {
        // Noise columns before the sum exhausted the old column-order cap.
        let n = 300usize;
        let noise: Vec<Vec<f64>> = (0..30u64)
            .map(|k| {
                (0..n as u64)
                    .map(|i| ((i + 31 * k + 3).wrapping_mul(0x9e3779b97f4a7c15) >> 45) as f64)
                    .collect()
            })
            .collect();
        let a: Vec<f64> = (0..n).map(|i| ((i * 37) % 101) as f64 + 0.5).collect();
        let b: Vec<f64> = (0..n).map(|i| ((i * 53) % 97) as f64 * 3.0).collect();
        let t: Vec<f64> = a.iter().zip(&b).map(|(x, y)| x + y).collect();
        let mut cols: Vec<TypedCol<'_>> = noise.iter().map(|p| make_col(p)).collect();
        cols.push(make_col(&a));
        cols.push(make_col(&b));
        cols.push(make_col(&t));
        let mut names: Vec<String> = (0..30).map(|k| format!("n{k}")).collect();
        names.extend(["a".to_string(), "b".to_string(), "t".to_string()]);
        let dtypes = vec!["float64".to_string(); 33];
        let table = onceover_core::TypedTable {
            n,
            names,
            dtypes,
            cols,
        };
        let (findings, _) = multiterm_identities(&table, 200_000, &[]);
        assert!(
            findings.iter().any(|f| f.claim.starts_with("t = a + b")),
            "{:?}",
            findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
    }

    #[test]
    fn weak_summand_found_via_enumeration() {
        let n = 200;
        let a: Vec<f64> = (0..n).map(|i| (i % 10) as f64 * 0.1).collect();
        let b: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let c: Vec<f64> = a.iter().zip(b.iter()).map(|(x, y)| x + y).collect();
        let table = TypedTable {
            n,
            names: vec!["a".into(), "b".into(), "c".into()],
            dtypes: vec!["float64".into(); 3],
            cols: vec![
                onceover_core::TypedCol::F64 {
                    vals: a.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: b.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: c.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
            ],
        };
        let (findings, _) = multiterm_identities(&table, 2000, &[]);
        assert!(findings
            .iter()
            .any(|f| f.specification.form == Some("sum".into())
                && f.subject.target == Some("c".into())));
    }

    #[test]
    fn excluded_target_gets_no_identity() {
        // Same fixture as weak_summand_found_via_enumeration, with the
        // sum's target (column index 2) excluded: no identity may be
        // reported on it, while it remains usable as a term.
        let n = 200;
        let a: Vec<f64> = (0..n).map(|i| (i % 10) as f64 * 0.1).collect();
        let b: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let c: Vec<f64> = a.iter().zip(b.iter()).map(|(x, y)| x + y).collect();
        let table = TypedTable {
            n,
            names: vec!["a".into(), "b".into(), "c".into()],
            dtypes: vec!["float64".into(); 3],
            cols: vec![
                onceover_core::TypedCol::F64 {
                    vals: a.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: b.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: c.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
            ],
        };
        let (findings, _) = multiterm_identities(&table, 2000, &[2]);
        assert!(findings
            .iter()
            .all(|f| f.subject.target.as_deref() != Some("c")));
    }

    #[test]
    fn cap_zero_reports_nothing() {
        let n = 200;
        let a: Vec<f64> = (0..n).map(|i| (i % 10) as f64 * 0.1).collect();
        let b: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let c: Vec<f64> = a.iter().zip(b.iter()).map(|(x, y)| x + y).collect();
        let table = TypedTable {
            n,
            names: vec!["a".into(), "b".into(), "c".into()],
            dtypes: vec!["float64".into(); 3],
            cols: vec![
                onceover_core::TypedCol::F64 {
                    vals: a.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: b.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: c.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
            ],
        };
        let (findings, coverage) = multiterm_identities(&table, 0, &[]);
        assert!(findings.is_empty());
        assert_eq!(
            coverage,
            "0 multi-term candidates screened, 0 multi-term identities"
        );
    }

    #[test]
    fn tiny_cap_is_deterministic() {
        let n = 200;
        let a: Vec<f64> = (0..n).map(|i| (i % 10) as f64 * 0.1).collect();
        let b: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let c: Vec<f64> = a.iter().zip(b.iter()).map(|(x, y)| x + y).collect();
        let table = TypedTable {
            n,
            names: vec!["c".into(), "a".into(), "b".into()],
            dtypes: vec!["float64".into(); 3],
            cols: vec![
                onceover_core::TypedCol::F64 {
                    vals: c.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: a.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: b.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
            ],
        };
        let (f1, cov1) = multiterm_identities(&table, 1, &[]);
        let (f2, _) = multiterm_identities(&table, 1, &[]);
        let claims1: Vec<String> = f1.iter().map(|f| f.claim.clone()).collect();
        let claims2: Vec<String> = f2.iter().map(|f| f.claim.clone()).collect();
        assert_eq!(claims1, claims2);
        assert!(cov1.contains("1 multi-term candidates screened"));
    }

    #[test]
    fn sum_wins_over_linear_combo() {
        let n = 200;
        let a: Vec<f64> = (0..n).map(|i| (i % 10) as f64 * 0.1).collect();
        let b: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let c: Vec<f64> = a.iter().zip(b.iter()).map(|(x, y)| x + y).collect();
        let table = TypedTable {
            n,
            names: vec!["c".into(), "a".into(), "b".into()],
            dtypes: vec!["float64".into(); 3],
            cols: vec![
                onceover_core::TypedCol::F64 {
                    vals: c.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: a.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: b.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
            ],
        };
        let (findings, _) = multiterm_identities(&table, 2000, &[]);
        let c_findings: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.subject.target == Some("c".into()))
            .collect();
        assert_eq!(c_findings.len(), 1);
        assert_eq!(c_findings[0].specification.form, Some("sum".into()));
    }

    #[test]
    fn violations_reported() {
        let n = 2000;
        let a: Vec<f64> = (0..n).map(|i| (i % 10) as f64 * 0.1).collect();
        let b: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let mut c: Vec<f64> = a.iter().zip(b.iter()).map(|(x, y)| x + y).collect();
        c[100] *= 1.0 + 1e-6;
        let table = TypedTable {
            n,
            names: vec!["c".into(), "a".into(), "b".into()],
            dtypes: vec!["float64".into(); 3],
            cols: vec![
                onceover_core::TypedCol::F64 {
                    vals: c.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: a.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: b.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
            ],
        };
        let (findings, _) = multiterm_identities(&table, 2000, &[]);
        assert!(findings
            .iter()
            .any(|f| f.subject.target == Some("c".into()) && f.claim.contains("1 violation")));
    }

    #[test]
    fn pairwise_identity_supersets_not_reported() {
        // y = 2x is a pairwise identity (found by the pairwise probe, not
        // by this function). Every multi-term candidate for every target
        // here is a zero-coefficient extension of a one-term identity or
        // nothing, so the minimality filter must leave no findings.
        let n = 200;
        let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let junk1: Vec<f64> = (0..n).map(|i| ((i * i) % 13) as f64 + 1.0).collect();
        let junk2: Vec<f64> = (0..n).map(|i| (i % 7) as f64 * 3.0 + 2.0).collect();
        let y: Vec<f64> = x.iter().map(|v| 2.0 * v).collect();
        let table = TypedTable {
            n,
            names: vec!["x".into(), "junk1".into(), "junk2".into(), "y".into()],
            dtypes: vec!["float64".into(); 4],
            cols: vec![
                onceover_core::TypedCol::F64 {
                    vals: x.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: junk1.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: junk2.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: y.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
            ],
        };
        let (findings, _) = multiterm_identities(&table, 2000, &[]);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn two_term_sum_supersets_not_reported() {
        // y = a + b. Before the minimality filter, the triple
        // {a, b, junk} also verified as a linear combination with a zero
        // junk coefficient and produced a second finding for y.
        let n = 200;
        let a: Vec<f64> = (0..n).map(|i| i as f64 * 0.5).collect();
        let b: Vec<f64> = (0..n).map(|i| (i % 7) as f64).collect();
        let junk: Vec<f64> = (0..n).map(|i| ((i * i) % 11) as f64 + 1.0).collect();
        let y: Vec<f64> = a.iter().zip(b.iter()).map(|(u, v)| u + v).collect();
        let table = TypedTable {
            n,
            names: vec!["a".into(), "b".into(), "junk".into(), "y".into()],
            dtypes: vec!["float64".into(); 4],
            cols: vec![
                onceover_core::TypedCol::F64 {
                    vals: a.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: b.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: junk.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: y.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
            ],
        };
        let (findings, _) = multiterm_identities(&table, 2000, &[]);
        let y_findings: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.subject.target == Some("y".into()))
            .collect();
        assert_eq!(y_findings.len(), 1, "{findings:?}");
        assert_eq!(y_findings[0].specification.form, Some("sum".into()));
        assert_eq!(y_findings[0].metrics()["terms"], json!(["a", "b"]));
    }

    #[test]
    fn redundant_total_triple_not_reported() {
        // tot is a stored copy of a + b, and y is the same sum. The
        // three-term readings ({a, b, tot} and friends) are supersets or
        // zero-coefficient extensions; only the minimal sum survives.
        let n = 200;
        let a: Vec<f64> = (0..n).map(|i| i as f64 * 0.5).collect();
        let b: Vec<f64> = (0..n).map(|i| (i % 7) as f64).collect();
        let tot: Vec<f64> = a.iter().zip(b.iter()).map(|(u, v)| u + v).collect();
        let y = tot.clone();
        let table = TypedTable {
            n,
            names: vec!["a".into(), "b".into(), "tot".into(), "y".into()],
            dtypes: vec!["float64".into(); 4],
            cols: vec![
                onceover_core::TypedCol::F64 {
                    vals: a.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: b.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: tot.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                onceover_core::TypedCol::F64 {
                    vals: y.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
            ],
        };
        let (findings, _) = multiterm_identities(&table, 2000, &[]);
        let y_findings: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.subject.target == Some("y".into()))
            .collect();
        assert_eq!(y_findings.len(), 1, "{findings:?}");
        assert_eq!(y_findings[0].specification.form, Some("sum".into()));
        assert_eq!(y_findings[0].metrics()["terms"], json!(["a", "b"]));
    }

    #[test]
    fn minimal_hits_drops_nonminimal_hits() {
        let hit = |form: &'static str,
                   target: &str,
                   predictors: &[&str],
                   coefficients: &[f64],
                   has_intercept: bool|
         -> MultitermHit {
            MultitermHit {
                form,
                target: target.into(),
                predictors: predictors.iter().map(|s| s.to_string()).collect(),
                coefficients: coefficients.to_vec(),
                has_intercept,
                n: 200,
                matched: 200,
                near_within: None,
            }
        };
        // H1: minimal; the zero is the intercept, not a predictor.
        let h1 = hit(
            "linear_combination",
            "t",
            &["a", "b"],
            &[0.0, 1.0, 2.0],
            true,
        );
        // H2: all coefficients nonzero (Rule A silent), but {a, b} is a
        // verified proper subset for the same target (Rule B drops it).
        let h2 = hit(
            "linear_combination",
            "t",
            &["a", "b", "c"],
            &[0.0, 0.5, 0.5, 0.5],
            true,
        );
        // H3: zero predictor coefficient (Rule A drops it); no subset hit.
        let h3 = hit(
            "linear_combination",
            "u",
            &["x", "junk"],
            &[0.0, 2.0, 0.0],
            true,
        );
        // H4: a sum (Rule A does not apply), and {x, junk} is not a
        // subset of {x, z} (Rule B silent). Kept.
        let h4 = hit("sum", "u", &["x", "z"], &[1.0, 1.0], false);
        let kept = minimal_hits(vec![h1, h2, h3, h4]);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].predictors, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(kept[1].predictors, vec!["x".to_string(), "z".to_string()]);
    }
}
