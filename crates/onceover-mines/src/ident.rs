//! Arithmetic identity probe: y ≈ a·x + b on pairs with strong dependence
//! (the cheap ancestor of BACON/BHUNT-style law discovery).

use onceover_report::{Method, Pattern};
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
            match crate::encode::numeric_at(p, row) {
                Some(v) => x.push(v),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            if let Some(y) = crate::encode::numeric_at(target, row) {
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
/// zero usable rows.
pub fn scan_fixed_linear(
    target: &onceover_core::TypedCol,
    predictors: &[&onceover_core::TypedCol],
    coeffs: &[f64],
    sample_rows: &[u32],
    n_rows: usize,
) -> Option<(usize, usize)> {
    if predictors.len() < 2 || predictors.len() > 3 || coeffs.len() < predictors.len() {
        return None;
    }

    let eval = |cols: &[&onceover_core::TypedCol], row: usize| -> Option<f64> {
        let mut acc = 0.0;
        for (i, c) in cols.iter().enumerate() {
            acc += coeffs[i] * crate::encode::numeric_at(c, row)?;
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
        let y = match crate::encode::numeric_at(target, row) {
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
        let y = match crate::encode::numeric_at(target, row) {
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
        let a = crate::encode::numeric_at(a, row)?;
        let b = crate::encode::numeric_at(b, row)?;
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
        let y = match crate::encode::numeric_at(target, row) {
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
        let y = match crate::encode::numeric_at(target, row) {
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
            match crate::encode::numeric_at(p, row) {
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
        let y = crate::encode::numeric_at(target, row)?;
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
            match crate::encode::numeric_at(p, row) {
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
        let y = match crate::encode::numeric_at(target, row) {
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
            let mut terms = String::new();
            for (i, p) in hit.predictors.iter().enumerate() {
                if i == 0 {
                    terms.push_str(p);
                } else if hit.coefficients.get(i).copied().unwrap_or(1.0) >= 0.0 {
                    terms.push_str(&format!(" + {}", p));
                } else {
                    terms.push_str(&format!(" - {}", p));
                }
            }
            format!("{} = {} within tolerance on {}", target, terms, agreement)
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
    metrics.insert(
        "tolerance".to_string(),
        json!("1e-9 absolute + 1e-9 relative"),
    );
    metrics.insert("absolute_tolerance".to_string(), json!(1e-9));
    metrics.insert("relative_tolerance".to_string(), json!(1e-9));

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
/// coefficient whose absolute value is <= 1e-9 * scale (scale = 1 + sum of the
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
                if h.coefficients[1..].iter().any(|c| c.abs() <= 1e-9 * scale) {
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

/// Screen and verify multi-term identities over the table's numeric columns.
/// Returns (findings, coverage string). Runs sequentially in deterministic order so reruns are
/// byte-identical.
pub fn multiterm_identities(
    table: &onceover_core::TypedTable,
    multiterm_cap: usize,
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
                onceover_core::TypedCol::I64 { .. } | onceover_core::TypedCol::F64 { .. }
            )
        })
        .map(|(idx, _)| idx)
        .filter(|&c| {
            let vals: Vec<f64> = sample_rows
                .iter()
                .filter_map(|&r| crate::encode::numeric_at(&table.cols[c], r as usize))
                .collect();
            if vals.len() < 2 {
                return false;
            }
            let mean = vals.iter().sum::<f64>() / vals.len() as f64;
            let var = vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / vals.len() as f64;
            var != 0.0
        })
        .collect();

    let mut hits: Vec<MultitermHit> = Vec::new();
    let mut screened: usize = 0;

    for &t in &numeric_cols {
        let others: Vec<usize> = numeric_cols.iter().copied().filter(|&c| c != t).collect();

        // Pairs.
        for w in 0..others.len() {
            for x in (w + 1)..others.len() {
                if screened == multiterm_cap {
                    break;
                }
                let (i, j) = (others[w], others[x]);
                screened += 1;
                if let Some((n, matched)) = scan_fixed_linear(
                    &table.cols[t],
                    &[&table.cols[i], &table.cols[j]],
                    &[1.0, 1.0],
                    &sample_rows,
                    n_rows,
                ) {
                    hits.push(MultitermHit {
                        form: "sum",
                        target: table.names[t].clone(),
                        predictors: vec![table.names[i].clone(), table.names[j].clone()],
                        coefficients: vec![1.0, 1.0],
                        has_intercept: false,
                        n,
                        matched,
                    });
                    continue;
                }
                if let Some((n, matched)) = scan_fixed_linear(
                    &table.cols[t],
                    &[&table.cols[i], &table.cols[j]],
                    &[1.0, -1.0],
                    &sample_rows,
                    n_rows,
                ) {
                    hits.push(MultitermHit {
                        form: "difference",
                        target: table.names[t].clone(),
                        predictors: vec![table.names[i].clone(), table.names[j].clone()],
                        coefficients: vec![1.0, -1.0],
                        has_intercept: false,
                        n,
                        matched,
                    });
                    continue;
                }
                if let Some((n, matched)) = scan_product_ratio(
                    &table.cols[t],
                    &[&table.cols[i], &table.cols[j]],
                    false,
                    &sample_rows,
                    n_rows,
                ) {
                    hits.push(MultitermHit {
                        form: "product",
                        target: table.names[t].clone(),
                        predictors: vec![table.names[i].clone(), table.names[j].clone()],
                        coefficients: vec![],
                        has_intercept: false,
                        n,
                        matched,
                    });
                    continue;
                }
                if let Some((n, matched)) = scan_product_ratio(
                    &table.cols[t],
                    &[&table.cols[i], &table.cols[j]],
                    true,
                    &sample_rows,
                    n_rows,
                ) {
                    hits.push(MultitermHit {
                        form: "ratio",
                        target: table.names[t].clone(),
                        predictors: vec![table.names[i].clone(), table.names[j].clone()],
                        coefficients: vec![],
                        has_intercept: false,
                        n,
                        matched,
                    });
                    continue;
                }
                if let Some((coeffs, n, matched)) = scan_linear_combo(
                    &table.cols[t],
                    &[&table.cols[i], &table.cols[j]],
                    &sample_rows,
                    n_rows,
                ) {
                    hits.push(MultitermHit {
                        form: "linear_combination",
                        target: table.names[t].clone(),
                        predictors: vec![table.names[i].clone(), table.names[j].clone()],
                        coefficients: coeffs,
                        has_intercept: true,
                        n,
                        matched,
                    });
                    continue;
                }
            }
            if screened == multiterm_cap {
                break;
            }
        }
        if screened == multiterm_cap {
            break;
        }

        // Triples.
        for w in 0..others.len() {
            for x in (w + 1)..others.len() {
                for y in (x + 1)..others.len() {
                    if screened == multiterm_cap {
                        break;
                    }
                    let (i, j, k) = (others[w], others[x], others[y]);
                    screened += 1;
                    let preds = [
                        table.names[i].clone(),
                        table.names[j].clone(),
                        table.names[k].clone(),
                    ];
                    if let Some((n, matched)) = scan_fixed_linear(
                        &table.cols[t],
                        &[&table.cols[i], &table.cols[j], &table.cols[k]],
                        &[1.0, 1.0, 1.0],
                        &sample_rows,
                        n_rows,
                    ) {
                        hits.push(MultitermHit {
                            form: "sum",
                            target: table.names[t].clone(),
                            predictors: preds.to_vec(),
                            coefficients: vec![1.0, 1.0, 1.0],
                            has_intercept: false,
                            n,
                            matched,
                        });
                        continue;
                    }
                    let mut hit = false;
                    for coeffs in [
                        &[1.0f64, 1.0, -1.0],
                        &[1.0f64, -1.0, 1.0],
                        &[-1.0f64, 1.0, 1.0],
                    ] {
                        if let Some((n, matched)) = scan_fixed_linear(
                            &table.cols[t],
                            &[&table.cols[i], &table.cols[j], &table.cols[k]],
                            coeffs,
                            &sample_rows,
                            n_rows,
                        ) {
                            hits.push(MultitermHit {
                                form: "difference",
                                target: table.names[t].clone(),
                                predictors: preds.to_vec(),
                                coefficients: coeffs.to_vec(),
                                has_intercept: false,
                                n,
                                matched,
                            });
                            hit = true;
                            break;
                        }
                    }
                    if hit {
                        continue;
                    }
                    if let Some((coeffs, n, matched)) = scan_linear_combo(
                        &table.cols[t],
                        &[&table.cols[i], &table.cols[j], &table.cols[k]],
                        &sample_rows,
                        n_rows,
                    ) {
                        hits.push(MultitermHit {
                            form: "linear_combination",
                            target: table.names[t].clone(),
                            predictors: preds.to_vec(),
                            coefficients: coeffs,
                            has_intercept: true,
                            n,
                            matched,
                        });
                        continue;
                    }
                }
                if screened == multiterm_cap {
                    break;
                }
            }
            if screened == multiterm_cap {
                break;
            }
        }
        if screened == multiterm_cap {
            break;
        }
    }

    let hits = minimal_hits(hits);
    let out: Vec<Finding> = hits.into_iter().map(build_multiterm_finding).collect();

    let coverage = format!(
        "{} multi-term candidates screened, {} multi-term identities",
        screened,
        out.len()
    );
    (out, coverage)
}

/// Build identity findings from strong pairs.
pub fn identities(
    table: &onceover_core::TypedTable,
    deps: &[PairDep],
    pair_cap: usize,
) -> (Vec<Finding>, String) {
    use rayon::prelude::*;

    let candidates: Vec<&PairDep> = deps
        .iter()
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
                if let (Some(x), Some(y)) = (
                    crate::encode::numeric_at(x, row),
                    crate::encode::numeric_at(y, row),
                ) {
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
        let (findings, _) = multiterm_identities(&table, 2000);
        assert!(findings
            .iter()
            .any(|f| f.specification.form == Some("sum".into())
                && f.subject.target == Some("c".into())));
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
        let (findings, coverage) = multiterm_identities(&table, 0);
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
        let (f1, cov1) = multiterm_identities(&table, 1);
        let (f2, _) = multiterm_identities(&table, 1);
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
        let (findings, _) = multiterm_identities(&table, 2000);
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
        let (findings, _) = multiterm_identities(&table, 2000);
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
        let (findings, _) = multiterm_identities(&table, 2000);
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
        let (findings, _) = multiterm_identities(&table, 2000);
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
        let (findings, _) = multiterm_identities(&table, 2000);
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
