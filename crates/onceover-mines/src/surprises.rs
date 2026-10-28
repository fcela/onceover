//! Subset-conditional surprise: individual values that are extreme
//! relative to the bulk of their column, or relative to the bulk of a
//! subset defined by one categorical column (issue #31).
//!
//! The corpus motivation: profilers find structures (dependencies,
//! rules) but do not isolate the surprising cases inside them — a
//! 455-year-old patient in a screening table where 99.97% of values
//! are <= 94, or a 75-year-old among pregnant patients whose other
//! ages are <= 42. GritBot's published method is the reference shape:
//! report the value, the subset, the contrasting bulk, and how
//! isolated the value is.
//!
//! Method (v1, numeric targets): a value on one side of a
//! distribution is surprising when it lies beyond the ordinary
//! Tukey fence (Q3 + 1.5 * IQR) of the remaining values and it is
//! isolated — the gap between it and the bulk's edge is at least
//! `gap_ratio` times the central spread (p90 - p10) of the
//! remaining values.
//! A small cluster of adjacent extremes masks itself under a
//! nearest-neighbour gap, so cluster sizes up to `max_per_side` are
//! tested against the bulk as a unit. The same test runs
//! globally per column and within each level of each eligible
//! categorical column; a subset extreme already reported globally
//! (same row) is not reported twice.

use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct SurpriseConfig {
    /// Minimum non-null values for a column to be scanned globally.
    pub min_rows: usize,
    /// Minimum values of the target inside a subset level.
    pub min_subset_rows: usize,
    /// Maximum distinct levels for a conditioning column.
    pub max_condition_levels: usize,
    /// Maximum conditioning columns tried per target.
    pub max_conditions: usize,
    /// Tukey fence multiplier (sanity gate).
    pub fence_iqr_mult: f64,
    /// Isolation gap, as a multiple of the remainder's p90-p10 spread.
    pub gap_ratio: f64,
    /// Extremes peeled per side per (target, subset).
    pub max_per_side: usize,
    /// Findings cap for the pass.
    pub max_findings: usize,
}

impl Default for SurpriseConfig {
    fn default() -> Self {
        Self {
            min_rows: 50,
            min_subset_rows: 30,
            max_condition_levels: 32,
            max_conditions: 8,
            fence_iqr_mult: 1.5,
            gap_ratio: 1.0,
            max_per_side: 3,
            max_findings: 20,
        }
    }
}

#[derive(Clone, Debug)]
struct Extreme {
    row: usize,
    value: f64,
    /// Nearest remaining value on the same side (the bulk's edge).
    bulk_edge: f64,
    gap: f64,
    spread: f64,
    /// Share of all values in the scope that lie on the bulk side.
    bulk_share: f64,
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    let frac = pos - lo as f64;
    sorted[lo] * (1.0 - frac) + sorted[hi] * frac
}

/// Test the top-`k` values of `sorted` (ascending) as an extreme
/// cluster against the remaining bulk; `high` selects the side.
/// Returns the cluster when every member lies beyond the bulk's
/// Tukey fence and the cluster's inner edge is isolated from the
/// bulk by at least `gap_ratio` times the bulk's central spread.
fn test_cluster(
    sorted: &[(f64, usize)],
    k: usize,
    high: bool,
    cfg: &SurpriseConfig,
) -> Option<Vec<Extreme>> {
    let n = sorted.len();
    if k == 0 || k >= n {
        return None;
    }
    let (cluster, bulk) = if high {
        (&sorted[n - k..], &sorted[..n - k])
    } else {
        (&sorted[..k], &sorted[k..])
    };
    let bulk_vals: Vec<f64> = bulk.iter().map(|v| v.0).collect();
    let q1 = quantile(&bulk_vals, 0.25);
    let q3 = quantile(&bulk_vals, 0.75);
    let iqr = q3 - q1;
    let spread = quantile(&bulk_vals, 0.90) - quantile(&bulk_vals, 0.10);
    if spread <= 0.0 {
        return None;
    }
    let inner = if high {
        cluster[0].0
    } else {
        cluster[cluster.len() - 1].0
    };
    let bulk_edge = if high {
        bulk_vals[bulk_vals.len() - 1]
    } else {
        bulk_vals[0]
    };
    let (beyond_fence, gap) = if high {
        (inner > q3 + cfg.fence_iqr_mult * iqr, inner - bulk_edge)
    } else {
        (inner < q1 - cfg.fence_iqr_mult * iqr, bulk_edge - inner)
    };
    if !beyond_fence || gap < cfg.gap_ratio * spread {
        return None;
    }
    let bulk_share = bulk.len() as f64 / n as f64;
    let mut members: Vec<Extreme> = cluster
        .iter()
        .map(|&(value, row)| Extreme {
            row,
            value,
            bulk_edge,
            gap,
            spread,
            bulk_share,
        })
        .collect();
    if high {
        members.reverse(); // most extreme first
    }
    Some(members)
}

/// Find surprising extremes on one side of `vals` (value, row).
/// Cluster sizes from `max_per_side` down to 1 are tried in order:
/// adjacent extremes mask each other under a nearest-neighbour gap,
/// so the cluster is tested against the bulk as a unit.
fn peel_side(
    vals: &[(f64, usize)],
    high: bool,
    min_n: usize,
    cfg: &SurpriseConfig,
) -> Vec<Extreme> {
    if vals.len() < min_n.max(10) {
        return Vec::new();
    }
    let mut sorted: Vec<(f64, usize)> = vals.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let raw = peel_sorted(&sorted, high, min_n, cfg, cfg.gap_ratio);
    if raw.is_empty() || !sorted.iter().all(|(v, _)| *v > 0.0) {
        return raw;
    }
    // Dual-scale guard: skewed positive columns (assays, money)
    // produce raw-scale isolation gaps in their ordinary tail. The
    // extreme must also be isolated on the log scale, at half the
    // gap bar, before it is reported.
    let logged: Vec<(f64, usize)> = sorted.iter().map(|(v, r)| (v.ln(), *r)).collect();
    let log_hits = peel_sorted(&logged, high, min_n, cfg, cfg.gap_ratio * 0.5);
    let top_row = raw[0].row;
    if log_hits.iter().any(|e| e.row == top_row) {
        raw
    } else {
        Vec::new()
    }
}

fn peel_sorted(
    sorted: &[(f64, usize)],
    high: bool,
    min_n: usize,
    cfg: &SurpriseConfig,
    gap_ratio: f64,
) -> Vec<Extreme> {
    let max_k = cfg
        .max_per_side
        .min(sorted.len().saturating_sub(min_n.max(10)) + 1)
        .max(1);
    let mut local = cfg.clone();
    local.gap_ratio = gap_ratio;
    for k in (1..=max_k).rev() {
        if let Some(m) = test_cluster(sorted, k, high, &local) {
            return m;
        }
    }
    Vec::new()
}

fn column_values(table: &TypedTable, col: usize) -> Vec<(f64, usize)> {
    let mut out = Vec::new();
    match &table.cols[col] {
        TypedCol::I64 { vals } => {
            for r in 0..table.n {
                if let Some(Some(v)) = vals.get(r) {
                    out.push((v as f64, r));
                }
            }
        }
        TypedCol::F64 { vals } => {
            for r in 0..table.n {
                if let Some(Some(v)) = vals.get(r) {
                    if v.is_finite() {
                        out.push((v, r));
                    }
                }
            }
        }
        _ => {}
    }
    out
}

/// Level labels per row for a conditioning column, if eligible.
fn condition_levels(
    table: &TypedTable,
    col: usize,
    cfg: &SurpriseConfig,
) -> Option<Vec<Option<String>>> {
    let distinct_ok = |n: usize| (2..=cfg.max_condition_levels).contains(&n);
    match &table.cols[col] {
        TypedCol::Str { vals } => {
            let mut set = std::collections::BTreeSet::new();
            for s in vals.iter().flatten() {
                set.insert(*s);
            }
            if !distinct_ok(set.len()) {
                return None;
            }
            Some(vals.iter().map(|v| v.map(|s| s.to_string())).collect())
        }
        TypedCol::StrOwned { vals } => {
            let mut set = std::collections::BTreeSet::new();
            for s in vals.iter().flatten() {
                set.insert(s.as_str());
            }
            if !distinct_ok(set.len()) {
                return None;
            }
            Some(vals.iter().map(|v| v.as_ref().cloned()).collect())
        }
        TypedCol::Bool { vals } => Some(
            vals.iter()
                .map(|v| v.map(|b| if b { "true".into() } else { "false".into() }))
                .collect(),
        ),
        TypedCol::I64 { vals } => {
            let mut set = std::collections::BTreeSet::new();
            for r in 0..table.n {
                if let Some(Some(v)) = vals.get(r) {
                    set.insert(v);
                }
            }
            if !distinct_ok(set.len()) {
                return None;
            }
            Some(
                (0..table.n)
                    .map(|r| match vals.get(r) {
                        Some(Some(v)) => Some(v.to_string()),
                        _ => None,
                    })
                    .collect(),
            )
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn make_finding(
    target: &str,
    condition: Option<(&str, &str)>,
    scope_rows: usize,
    extremes: &[Extreme],
    high: bool,
    cfg: &SurpriseConfig,
) -> Finding {
    let ex = &extremes[0];
    let n_extreme = extremes.len();
    let rows_list: Vec<usize> = extremes.iter().map(|e| e.row).collect();
    let values_list: Vec<f64> = extremes.iter().map(|e| e.value).collect();
    let pct = ex.bulk_share * 100.0;
    let claim = match condition {
        None => format!(
            "{} reaches {} — isolated from the rest of the column: {:.2}% of values are {} {}, and the gap of {} is {:.1}x the central spread (p90-p10 = {}) of the remainder",
            target,
            crate::fmt_float(ex.value),
            pct,
            if high { "<=" } else { ">=" },
            crate::fmt_float(ex.bulk_edge),
            crate::fmt_float(ex.gap),
            ex.gap / ex.spread,
            crate::fmt_float(ex.spread),
        ),
        Some((cname, cval)) => format!(
            "within {} = {} ({} rows), {} reaches {} — isolated from the rest of the subset: all other values are {} {}, a gap of {} ({:.1}x the subset's central spread, p90-p10 = {})",
            cname,
            cval,
            scope_rows,
            target,
            crate::fmt_float(ex.value),
            if high { "<=" } else { ">=" },
            crate::fmt_float(ex.bulk_edge),
            crate::fmt_float(ex.gap),
            ex.gap / ex.spread,
            crate::fmt_float(ex.spread),
        ),
    };
    let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    let mut cols = vec![target.to_string()];
    if let Some((cname, _)) = condition {
        cols.push(cname.to_string());
    }
    params.insert("columns".into(), json!(cols));
    params.insert("column".into(), json!(target));
    if let Some((cname, cval)) = condition {
        params.insert("condition_column".into(), json!(cname));
        params.insert("condition_value".into(), json!(cval));
    }
    let mut metrics: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    metrics.insert("n".into(), json!(scope_rows));
    metrics.insert("extreme_count".into(), json!(n_extreme));
    metrics.insert("extreme_rows".into(), json!(rows_list));
    metrics.insert("extreme_values".into(), json!(values_list));
    metrics.insert("bulk_edge".into(), json!(ex.bulk_edge));
    metrics.insert("gap".into(), json!(ex.gap));
    metrics.insert("central_spread_p90_p10".into(), json!(ex.spread));
    metrics.insert("gap_ratio".into(), json!(ex.gap / ex.spread));
    metrics.insert("side".into(), json!(if high { "high" } else { "low" }));
    let score = (0.55 + 0.1 * (ex.gap / ex.spread).min(3.0)).min(0.9);
    let _ = cfg;
    Finding::new(
        Pattern::ExtremeValue,
        params,
        claim,
        EvidenceBasis::Measured,
        score,
        metrics,
        Some(
            "verify the flagged rows at the source; an isolated value is often a unit, entry, or coding error"
                .into(),
        ),
        Method::new(
            "surprises",
            "robust isolation gap vs the subset bulk",
            "onceover subset-conditional surprise v1 (issue #31)",
        ),
    )
}

pub fn profile_surprises(
    table: &TypedTable,
    excluded: &[usize],
    cfg: &SurpriseConfig,
) -> (Vec<Finding>, String) {
    let numeric: Vec<usize> = (0..table.cols.len())
        .filter(|&i| table.cols[i].is_numeric() && !excluded.contains(&i))
        .collect();
    let conditions: Vec<usize> = (0..table.cols.len())
        .filter(|&i| !excluded.contains(&i))
        .filter(|&i| condition_levels(table, i, cfg).is_some())
        .collect();
    let mut findings: Vec<Finding> = Vec::new();
    let mut scanned = 0usize;
    'targets: for &t in &numeric {
        let vals = column_values(table, t);
        if vals.len() < cfg.min_rows {
            continue;
        }
        // Near-constant columns are the concentration pass's story.
        let mut sorted: Vec<f64> = vals.iter().map(|v| v.0).collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if quantile(&sorted, 0.75) - quantile(&sorted, 0.25) <= 0.0 {
            continue;
        }
        scanned += 1;
        let tname = table.names[t].as_str();
        let mut reported_rows: std::collections::BTreeSet<usize> = Default::default();
        for high in [true, false] {
            let ex = peel_side(&vals, high, cfg.min_rows, cfg);
            if !ex.is_empty() {
                for e in &ex {
                    reported_rows.insert(e.row);
                }
                findings.push(make_finding(tname, None, vals.len(), &ex, high, cfg));
                if findings.len() >= cfg.max_findings {
                    break 'targets;
                }
            }
        }
        // Subset pass: one categorical condition at a time.
        let mut tried = 0usize;
        for &c in &conditions {
            if c == t || tried >= cfg.max_conditions {
                continue;
            }
            tried += 1;
            let levels = condition_levels(table, c, cfg).unwrap();
            let mut by_level: BTreeMap<String, Vec<(f64, usize)>> = BTreeMap::new();
            let val_by_row: BTreeMap<usize, f64> = vals.iter().map(|(v, r)| (*r, *v)).collect();
            for (r, lvl) in levels.iter().enumerate() {
                if let (Some(l), Some(v)) = (lvl, val_by_row.get(&r)) {
                    by_level.entry(l.clone()).or_default().push((*v, r));
                }
            }
            for (lvl, lvals) in &by_level {
                if lvals.len() < cfg.min_subset_rows {
                    continue;
                }
                for high in [true, false] {
                    let ex = peel_side(lvals, high, cfg.min_subset_rows, cfg);
                    let fresh: Vec<Extreme> = ex
                        .into_iter()
                        .filter(|e| !reported_rows.contains(&e.row))
                        .collect();
                    if !fresh.is_empty() {
                        for e in &fresh {
                            reported_rows.insert(e.row);
                        }
                        findings.push(make_finding(
                            tname,
                            Some((table.names[c].as_str(), lvl.as_str())),
                            lvals.len(),
                            &fresh,
                            high,
                            cfg,
                        ));
                        if findings.len() >= cfg.max_findings {
                            break 'targets;
                        }
                    }
                }
            }
        }
    }
    let detail = format!(
        "{} numeric columns scanned against {} conditioning columns; {} surprise findings",
        scanned,
        conditions.len(),
        findings.len()
    );
    (findings, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peel_flags_isolated_max() {
        let cfg = SurpriseConfig::default();
        let mut vals: Vec<(f64, usize)> = (0..200).map(|i| (20.0 + (i % 50) as f64, i)).collect();
        vals.push((455.0, 999));
        let ex = peel_side(&vals, true, 50, &cfg);
        assert_eq!(ex.len(), 1);
        assert_eq!(ex[0].value, 455.0);
        assert_eq!(ex[0].row, 999);
    }

    #[test]
    fn peel_ignores_smooth_tail() {
        let cfg = SurpriseConfig::default();
        // Exponential-ish tail: each next value a small step above the last.
        let vals: Vec<(f64, usize)> = (0..300).map(|i| ((i as f64).ln_1p() * 10.0, i)).collect();
        let ex = peel_side(&vals, true, 50, &cfg);
        assert!(ex.is_empty(), "smooth heavy tail must not flag: {:?}", ex);
    }

    #[test]
    fn peel_flags_subset_style_cluster() {
        let cfg = SurpriseConfig::default();
        // Bulk 17..42, then two isolated values at 73 and 75.
        let mut vals: Vec<(f64, usize)> = (0..51).map(|i| (17.0 + (i % 26) as f64, i)).collect();
        vals.push((73.0, 100));
        vals.push((75.0, 101));
        let ex = peel_side(&vals, true, 30, &cfg);
        assert_eq!(ex.len(), 2);
        assert_eq!(ex[0].value, 75.0);
    }
}
