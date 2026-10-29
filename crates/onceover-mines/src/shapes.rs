//! Target-aware shape decomposition: how each numeric predictor
//! relates to a designated response column.
//!
//! The profiler is response-free by default; this pass runs only
//! when the caller designates a target (`profile(df, target="y")`).
//! The statistics are a port of mboostrs-dev's
//! `column_diagnostics` (its specification search uses them to
//! pick a shape per term): linear R², a degrees-of-freedom-adjusted
//! binned-curve R² (up to 20 equal-count bins, never splitting a
//! tie), the best single-step R² over all splits of the sorted
//! predictor, and Spearman monotonicity — all on the same
//! complete-case rows per predictor, with the missing-value policy
//! stated in the coverage line.
//!
//! A finding is the interpretation, not the numbers (the column
//! cards and the dependence screen already carry summaries), so a
//! predictor is reported only when a shape is decisive: a single
//! step carries the curve's gain over the line, or the curve adds
//! materially over the line. The step finding records the located
//! cut (`step_at`) — the same quantity the discontinuity screen
//! reports for its jumps, so the two findings corroborate by value
//! when both fire.

use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct ShapesConfig {
    /// Minimum complete-case rows per predictor.
    pub min_rows: usize,
    /// Findings cap per designated target.
    pub max_findings: usize,
    /// Decisiveness bars (see module docs).
    pub step_r2_min: f64,
    pub step_over_line_min: f64,
    pub step_share_min: f64,
    pub curve_r2_min: f64,
    pub curvature_min: f64,
}

impl Default for ShapesConfig {
    fn default() -> Self {
        Self {
            min_rows: 30,
            max_findings: 10,
            step_r2_min: 0.05,
            step_over_line_min: 0.05,
            step_share_min: 0.8,
            curve_r2_min: 0.10,
            curvature_min: 0.05,
        }
    }
}

/// The ported per-column diagnostics, plus the located step cut
/// (mboostrs-dev returns only the R² shares; the cut is needed to
/// state and corroborate the step).
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeDiagnostics {
    pub distinct: usize,
    pub pearson: f64,
    pub spearman: f64,
    pub linear_r2: f64,
    pub curve_r2: f64,
    pub step_r2: f64,
    /// Midpoint between the two predictor values bracketing the
    /// best split.
    pub step_at: f64,
}

impl ShapeDiagnostics {
    pub fn curvature(&self) -> f64 {
        (self.curve_r2 - self.linear_r2).max(0.0)
    }
    /// What a single step explains beyond the line, relative to
    /// what the curve adds: near or above one when a jump carries
    /// the curve.
    pub fn step_share(&self) -> f64 {
        let curve = self.curvature();
        if curve <= 0.0 {
            0.0
        } else {
            (self.step_r2 - self.linear_r2).max(0.0) / curve
        }
    }
}

fn ranks(values: &[f64]) -> Vec<f64> {
    let n = values.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| values[a].total_cmp(&values[b]));
    let mut out = vec![0.0; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && values[order[j + 1]] == values[order[i]] {
            j += 1;
        }
        let average = (i + j) as f64 / 2.0;
        for &k in &order[i..=j] {
            out[k] = average;
        }
        i = j + 1;
    }
    out
}

fn correlation(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let ma = a.iter().sum::<f64>() / n;
    let mb = b.iter().sum::<f64>() / n;
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        sab += (x - ma) * (y - mb);
        saa += (x - ma).powi(2);
        sbb += (y - mb).powi(2);
    }
    if saa > 0.0 && sbb > 0.0 {
        sab / (saa * sbb).sqrt()
    } else {
        0.0
    }
}

/// Diagnostics of how `y` depends on the column `x` (complete,
/// finite pairs; the caller guarantees at least a handful of rows
/// and a non-constant `y` for a meaningful result).
pub fn shape_diagnostics(x: &[f64], y: &[f64]) -> ShapeDiagnostics {
    let n = x.len();
    let nf = n as f64;
    let mean_y = y.iter().sum::<f64>() / nf;
    let tss: f64 = y.iter().map(|v| (v - mean_y).powi(2)).sum();
    let pearson = correlation(x, y);
    let spearman = correlation(&ranks(x), &ranks(y));
    let linear_r2 = pearson * pearson;

    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| x[a].total_cmp(&x[b]));
    let mut distinct = 0usize;
    for k in 0..n {
        if k == 0 || x[order[k]] != x[order[k - 1]] {
            distinct += 1;
        }
    }

    // Curve: means in up to 20 bins of equal counts, never
    // splitting a tie; adjusted for the bins' degrees of freedom.
    let bins = distinct.clamp(1, 20);
    let mut rss_curve = 0.0;
    let mut start = 0usize;
    for b in 0..bins {
        let mut end = ((b + 1) * n) / bins;
        if b + 1 == bins {
            end = n;
        }
        while end < n && end > start && x[order[end]] == x[order[end - 1]] {
            end += 1;
        }
        if end <= start {
            continue;
        }
        let m = order[start..end].iter().map(|&k| y[k]).sum::<f64>() / (end - start) as f64;
        rss_curve += order[start..end]
            .iter()
            .map(|&k| (y[k] - m).powi(2))
            .sum::<f64>();
        start = end;
        if start >= n {
            break;
        }
    }
    let curve_r2 = if tss > 0.0 && n > bins {
        (1.0 - (rss_curve / tss) * (nf - 1.0) / (nf - bins as f64)).max(0.0)
    } else {
        0.0
    };

    // Best single split, scanning the sorted column; splits only
    // between distinct values.
    let total: f64 = y.iter().sum();
    let (mut left_sum, mut best_between, mut best_k) = (0.0, 0.0f64, 0usize);
    for k in 0..n - 1 {
        left_sum += y[order[k]];
        if x[order[k]] == x[order[k + 1]] {
            continue;
        }
        let nl = (k + 1) as f64;
        let nr = nf - nl;
        let between =
            left_sum * left_sum / nl + (total - left_sum).powi(2) / nr - total * total / nf;
        if between > best_between {
            best_between = between;
            best_k = k;
        }
    }
    let step_r2 = if tss > 0.0 { best_between / tss } else { 0.0 };
    let step_at = if n >= 2 && best_between > 0.0 {
        (x[order[best_k]] + x[order[best_k + 1]]) / 2.0
    } else {
        0.0
    };

    ShapeDiagnostics {
        distinct,
        pearson,
        spearman,
        linear_r2,
        curve_r2,
        step_r2,
        step_at,
    }
}

fn column_f64(table: &TypedTable, ci: usize) -> Option<Vec<Option<f64>>> {
    match &table.cols[ci] {
        TypedCol::I64 { vals } => Some(vals.iter().map(|v| v.map(|x| *x as f64)).collect()),
        TypedCol::F64 { vals } => Some(vals.iter().map(|v| v.copied()).collect()),
        _ => None,
    }
}

pub fn profile_target_shapes(
    table: &TypedTable,
    target_name: &str,
    cfg: &ShapesConfig,
) -> (Vec<Finding>, String) {
    let Some(tci) = table.names.iter().position(|n| n == target_name) else {
        return (
            Vec::new(),
            format!("designated target '{target_name}' is not a column of the frame"),
        );
    };
    let Some(target_vals) = column_f64(table, tci) else {
        return (
            Vec::new(),
            format!("designated target '{target_name}' is not numeric; shape decomposition needs a numeric target"),
        );
    };

    let mut scored: Vec<(f64, Finding)> = Vec::new();
    let mut n_predictors = 0usize;
    let mut n_evaluated = 0usize;
    for ci in 0..table.cols.len() {
        if ci == tci {
            continue;
        }
        let Some(pred_vals) = column_f64(table, ci) else {
            continue;
        };
        n_predictors += 1;
        let mut xs: Vec<f64> = Vec::new();
        let mut ys: Vec<f64> = Vec::new();
        for r in 0..table.n {
            if let (Some(x), Some(y)) = (pred_vals[r], target_vals[r]) {
                if x.is_finite() && y.is_finite() {
                    xs.push(x);
                    ys.push(y);
                }
            }
        }
        if xs.len() < cfg.min_rows {
            continue;
        }
        let d = shape_diagnostics(&xs, &ys);
        if d.distinct < 2 {
            continue;
        }
        n_evaluated += 1;
        let is_step = d.step_r2 >= cfg.step_r2_min
            && d.step_r2 - d.linear_r2 >= cfg.step_over_line_min
            && d.step_share() >= cfg.step_share_min;
        let is_curve =
            !is_step && d.curve_r2 >= cfg.curve_r2_min && d.curvature() >= cfg.curvature_min;
        if is_step || is_curve {
            let rank_key = if is_step { d.step_r2 } else { d.curve_r2 };
            scored.push((
                rank_key,
                make_finding(table, ci, target_name, &d, xs.len(), is_step),
            ));
        }
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.claim.cmp(&b.1.claim)));
    scored.truncate(cfg.max_findings);
    let findings: Vec<Finding> = scored.into_iter().map(|(_, f)| f).collect();
    let coverage = format!(
        "target '{target_name}' designated; {} numeric predictors, {} evaluated on complete cases (rows missing the predictor or the target excluded); {} decisive shapes reported",
        n_predictors,
        n_evaluated,
        findings.len()
    );
    (findings, coverage)
}

fn make_finding(
    table: &TypedTable,
    ci: usize,
    target_name: &str,
    d: &ShapeDiagnostics,
    n_used: usize,
    is_step: bool,
) -> Finding {
    let pname = &table.names[ci];
    let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    params.insert("columns".into(), json!([pname.clone(), target_name]));
    params.insert("inputs".into(), json!([pname.clone()]));
    params.insert("target".into(), json!(target_name));
    params.insert("column".into(), json!(target_name));

    let mut metrics: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    metrics.insert("linear_r2".into(), json!(d.linear_r2));
    metrics.insert("curve_r2".into(), json!(d.curve_r2));
    metrics.insert("step_r2".into(), json!(d.step_r2));
    metrics.insert("step_share".into(), json!(d.step_share()));
    metrics.insert("curvature".into(), json!(d.curvature()));
    metrics.insert("pearson".into(), json!(d.pearson));
    metrics.insert("spearman".into(), json!(d.spearman));
    metrics.insert("distinct".into(), json!(d.distinct));
    metrics.insert("n_used".into(), json!(n_used));

    let (claim, score) = if is_step {
        metrics.insert("form".into(), json!("step_shape"));
        metrics.insert("step_at".into(), json!(d.step_at));
        (
            format!(
                "{} relates to {} mainly as a step at {}: a single split explains {:.1}% of {}'s variance, against {:.1}% for a straight line; the step carries {:.0}% of what a binned curve adds over the line",
                pname,
                target_name,
                crate::fmt_float(d.step_at),
                100.0 * d.step_r2,
                target_name,
                100.0 * d.linear_r2,
                100.0 * d.step_share()
            ),
            0.5 + 0.35 * d.step_r2,
        )
    } else {
        metrics.insert("form".into(), json!("curved_shape"));
        let monotone = if d.spearman >= 0.5 {
            "monotone increasing"
        } else if d.spearman <= -0.5 {
            "monotone decreasing"
        } else {
            "non-monotone"
        };
        (
            format!(
                "{} relates to {} as a {} curve, not a line: a binned curve explains {:.1}% of {}'s variance, against {:.1}% for a straight line (Spearman {:.2})",
                pname,
                target_name,
                monotone,
                100.0 * d.curve_r2,
                target_name,
                100.0 * d.linear_r2,
                d.spearman
            ),
            0.5 + 0.35 * d.curve_r2,
        )
    };
    Finding::new(
        Pattern::PredictiveDependency,
        params,
        claim,
        EvidenceBasis::Measured,
        score,
        metrics,
        Some(if is_step {
            format!(
                "model {target_name} with a threshold term in {pname} at the recorded cut, or check the discontinuities screen for a located jump on the same pair"
            )
        } else {
            format!(
                "model {target_name} with a smooth or piecewise term in {pname}; a straight line leaves the recorded curvature unmodelled"
            )
        }),
        Method::new(
            "target_shapes",
            "shape decomposition against the designated target (linear / binned-curve / single-step R² shares)",
            "onceover target shapes v1 (issue #43)",
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(f: impl Fn(f64) -> f64, n: usize) -> (Vec<f64>, Vec<f64>) {
        // Deterministic grid predictors, no RNG.
        let xs: Vec<f64> = (0..n).map(|i| i as f64 / n as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|&x| f(x)).collect();
        (xs, ys)
    }

    #[test]
    fn step_function_scores_step_near_one() {
        let (xs, ys) = pairs(|x| if x < 0.5 { 0.0 } else { 2.0 }, 400);
        let d = shape_diagnostics(&xs, &ys);
        assert!(d.step_r2 > 0.99, "step_r2 = {}", d.step_r2);
        assert!(d.step_share() > 0.9, "share = {}", d.step_share());
        assert!((d.step_at - 0.5).abs() < 0.01, "step_at = {}", d.step_at);
    }

    #[test]
    fn line_scores_linear_near_one_and_no_step() {
        let (xs, ys) = pairs(|x| 3.0 * x + 1.0, 400);
        let d = shape_diagnostics(&xs, &ys);
        assert!(d.linear_r2 > 0.99, "linear_r2 = {}", d.linear_r2);
        assert!(d.curvature() < 0.02, "curvature = {}", d.curvature());
    }

    #[test]
    fn cube_is_curved_and_monotone() {
        let (xs, ys) = pairs(|x| (x - 0.5).powi(3) * 8.0, 400);
        let d = shape_diagnostics(&xs, &ys);
        assert!(d.curvature() > 0.05, "curvature = {}", d.curvature());
        assert!(d.spearman > 0.99, "spearman = {}", d.spearman);
    }

    #[test]
    fn noise_curve_adjusts_to_near_zero() {
        // Deterministic pseudo-noise: y alternates by a hash of i.
        let n = 400;
        let xs: Vec<f64> = (0..n).map(|i| i as f64 / n as f64).collect();
        let ys: Vec<f64> = (0..n)
            .map(|i| ((i * 2654435761usize) % 97) as f64 / 97.0)
            .collect();
        let d = shape_diagnostics(&xs, &ys);
        assert!(d.curve_r2 < 0.05, "curve_r2 = {}", d.curve_r2);
        assert!(d.linear_r2 < 0.05, "linear_r2 = {}", d.linear_r2);
    }
}
