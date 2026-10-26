//! Arithmetic identity probe: y ≈ a·x + b on pairs with strong dependence
//! (the cheap ancestor of BACON/BHUNT-style law discovery).

use onceover_report::{Method, Pattern};
use std::collections::BTreeMap;

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
