//! Arithmetic identity probe: y ≈ a·x + b on pairs with strong dependence
//! (the cheap ancestor of BACON/BHUNT-style law discovery).

use std::collections::BTreeMap;

use onceover_report::{Confidence, Finding};
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

    let checked: Vec<Option<(usize, f64, f64, f64)>> = candidates
        .par_iter()
        .map(|d| {
            let pairs = crate::dep::pairs_of(&table.cols[d.a], &table.cols[d.b]);
            fit_affine(&pairs).map(|(a, b, frac)| (d.a, a, b, frac))
        })
        .collect();

    let mut out = Vec::new();
    for r in checked.into_iter().flatten() {
        let (a_idx, a, b, frac) = r;
        if frac < 0.999 {
            continue;
        }
        let x = &table.names[a_idx];
        let y = &table.names[candidates
            .iter()
            .find(|d| d.a == a_idx)
            .map(|d| d.b)
            .unwrap_or(a_idx)];
        let claim = if b.abs() < 1e-12 * (1.0 + a.abs()) {
            format!("{y} = {a:.6}·{x} exactly on {frac:.2}% of rows")
        } else {
            format!("{y} = {a:.6}·{x} + {b:.6} exactly on {frac:.2}% of rows")
        };
        out.push(Finding::new(
            "arithmetic_identity",
            BTreeMap::from([("x".into(), json!(x)), ("y".into(), json!(y))]),
            claim,
            Confidence::Exact,
            0.75,
            BTreeMap::from([
                ("a".into(), json!(a)),
                ("b".into(), json!(b)),
                ("exact_fraction".into(), json!(frac)),
                ("tolerance".into(), json!("1e-9 relative")),
            ]),
            Some(format!(
                "{y} is derived from {x}; drop it or treat as a check column"
            )),
            "pairwise OLS + exact residual scan (BHUNT/BACON lineage, onceover v0.1)",
        ));
    }
    let coverage = format!(
        "{} candidate pairs checked, {} identities",
        candidates.len(),
        out.len()
    );
    (out, coverage)
}
