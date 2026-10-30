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
//!
//! The categorical analogue (issue #68) mirrors the shape for
//! categorical targets: within a context — a conjunction of up to
//! three condition levels — one level of the target holds at
//! least `min_dominant_share` of the rows, and the few rows that
//! break the pattern are isolated. Contexts are searched
//! levelwise: single levels for every eligible condition column,
//! then pairs and triples over the columns whose single levels
//! come closest to the bar for that target. Candidates are
//! ranked by a binomial significance (the dominant level's
//! global rate against the context's count), capped per target,
//! and a row already isolated for a target is not reported twice.

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
    /// Categorical: minimum dominant-level share inside a context.
    pub min_dominant_share: f64,
    /// Categorical: maximum pattern-breaking rows per finding.
    pub max_breakers: usize,
    /// Categorical: maximum conditions in a context.
    pub max_context_conditions: usize,
    /// Categorical: condition columns carried into pair/triple
    /// contexts, ranked by their best single-level share.
    pub max_deep_conditions: usize,
    /// Categorical: pair contexts kept for triple extension.
    pub context_beam: usize,
    /// Categorical: minimum share for a pair context to be
    /// extended to a triple.
    pub deep_share_floor: f64,
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
            min_dominant_share: 0.99,
            max_breakers: 3,
            max_context_conditions: 3,
            max_deep_conditions: 12,
            context_beam: 128,
            deep_share_floor: 0.95,
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

// ---------------------------------------------------------------------
// Categorical surprises (issue #68): a value contradicting its
// context's near-universal pattern.
// ---------------------------------------------------------------------

/// Per-row level codes and labels for one eligible column
/// (2..=max_condition_levels distinct non-null levels).
struct Coded {
    codes: Vec<Option<u32>>,
    labels: Vec<String>,
}

fn code_column(table: &TypedTable, col: usize, cfg: &SurpriseConfig) -> Option<Coded> {
    let raw: Vec<Option<String>> = match &table.cols[col] {
        TypedCol::Str { vals } => vals.iter().map(|v| v.map(|s| s.to_string())).collect(),
        TypedCol::StrOwned { vals } => vals.iter().map(|v| v.as_ref().cloned()).collect(),
        TypedCol::Bool { vals } => vals
            .iter()
            .map(|v| v.map(|b| if b { "true".into() } else { "false".into() }))
            .collect(),
        TypedCol::I64 { vals } => (0..table.n)
            .map(|r| match vals.get(r) {
                Some(Some(v)) => Some(v.to_string()),
                _ => None,
            })
            .collect(),
        _ => return None,
    };
    let mut labels: Vec<String> = Vec::new();
    let mut index: BTreeMap<String, u32> = BTreeMap::new();
    for v in raw.iter().flatten() {
        if !index.contains_key(v) {
            index.insert(v.clone(), labels.len() as u32);
            labels.push(v.clone());
        }
    }
    if !(2..=cfg.max_condition_levels).contains(&labels.len()) {
        return None;
    }
    let codes = raw
        .iter()
        .map(|v| v.as_ref().map(|s| index[s]))
        .collect();
    Some(Coded { codes, labels })
}

fn bitset_for(codes: &[Option<u32>], level: u32, words: usize) -> Vec<u64> {
    let mut bs = vec![0u64; words];
    for (r, c) in codes.iter().enumerate() {
        if *c == Some(level) {
            bs[r / 64] |= 1u64 << (r % 64);
        }
    }
    bs
}

/// Lanczos log-gamma (ranking-grade accuracy).
fn lgamma(x: f64) -> f64 {
    const C: [f64; 6] = [
        76.18009172947146,
        -86.50532032941677,
        24.01409824083091,
        -1.231739572450155,
        0.001208650973866179,
        -0.0005395239384953,
    ];
    let mut ser = 1.000000000190015;
    let mut y = x;
    for &c in &C {
        y += 1.0;
        ser += c / y;
    }
    let tmp = x + 5.5;
    tmp.ln() * (x + 0.5) - tmp + (2.5066282746310005 * ser / x).ln()
}

/// log10 of the binomial survival P(X >= k) for X ~ Bin(n, p):
/// how surprising the dominant count is under the level's
/// global rate.
fn log10_binomial_tail(n: usize, k: usize, p: f64) -> f64 {
    if k == 0 || p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return f64::NEG_INFINITY;
    }
    let lg = |x: f64| lgamma(x);
    let log_term = |x: usize| {
        lg((n + 1) as f64) - lg((x + 1) as f64) - lg((n - x + 1) as f64)
            + x as f64 * p.ln()
            + (n - x) as f64 * (1.0 - p).ln()
    };
    let mut max_lt = f64::NEG_INFINITY;
    let mut terms = Vec::with_capacity(n - k + 1);
    for x in k..=n {
        let lt = log_term(x);
        terms.push(lt);
        if lt > max_lt {
            max_lt = lt;
        }
    }
    let s: f64 = terms.iter().map(|lt| (lt - max_lt).exp()).sum();
    (max_lt + s.ln()) / std::f64::consts::LN_10
}

struct CatCandidate {
    target: usize,
    items: Vec<(usize, u32)>,
    dom: u32,
    share: f64,
    subset: usize,
    breakers: Vec<usize>,
    logtail: f64,
}

/// Evaluate one context (bitset of rows) for one target: counts
/// of the target's levels over the context rows where the target
/// is present. Returns (subset, dom code, dominant count,
/// breaker rows) when the context has at least two levels.
fn eval_context(
    ctx: &[u64],
    tcodes: &[Option<u32>],
    nlevels: usize,
) -> Option<(usize, u32, usize, Vec<usize>)> {
    let mut counts = vec![0usize; nlevels];
    let mut present = 0usize;
    for (w, &word) in ctx.iter().enumerate() {
        let mut bits = word;
        while bits != 0 {
            let r = w * 64 + bits.trailing_zeros() as usize;
            bits &= bits - 1;
            if let Some(c) = tcodes.get(r).copied().flatten() {
                counts[c as usize] += 1;
                present += 1;
            }
        }
    }
    if present == 0 || counts.iter().filter(|&&c| c > 0).count() < 2 {
        return None;
    }
    let (dom, &dom_count) = counts
        .iter()
        .enumerate()
        .max_by_key(|&(_, c)| c)
        .unwrap();
    let mut breakers: Vec<usize> = Vec::new();
    for (w, &word) in ctx.iter().enumerate() {
        let mut bits = word;
        while bits != 0 {
            let r = w * 64 + bits.trailing_zeros() as usize;
            bits &= bits - 1;
            if let Some(c) = tcodes.get(r).copied().flatten() {
                if c as usize != dom {
                    breakers.push(r);
                }
            }
        }
    }
    Some((present, dom as u32, dom_count, breakers))
}

#[allow(clippy::too_many_arguments)]
fn make_categorical_finding(
    table: &TypedTable,
    target: usize,
    tcoded: &Coded,
    ctx_cols: &[(usize, &Coded, u32)],
    cand: &CatCandidate,
) -> Finding {
    let tname = table.names[target].as_str();
    let dom_label = &tcoded.labels[cand.dom as usize];
    let ctx_desc: Vec<String> = ctx_cols
        .iter()
        .map(|(c, coded, lvl)| {
            format!("{} = {}", table.names[*c], coded.labels[*lvl as usize])
        })
        .collect();
    let breaker_desc: Vec<String> = cand
        .breakers
        .iter()
        .map(|&r| {
            format!(
                "row {} ({} = {})",
                r,
                tname,
                tcoded.labels[tcoded.codes[r].unwrap() as usize]
            )
        })
        .collect();
    let claim = format!(
        "within {} ({} rows), {} is {} in {:.1}% of rows — {} {} {} the pattern: {}",
        ctx_desc.join(" and "),
        cand.subset,
        tname,
        dom_label,
        cand.share * 100.0,
        cand.breakers.len(),
        if cand.breakers.len() == 1 { "row" } else { "rows" },
        if cand.breakers.len() == 1 { "breaks" } else { "break" },
        breaker_desc.join(", "),
    );
    let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    let mut cols = vec![tname.to_string()];
    let mut conditions = Vec::new();
    for (c, coded, lvl) in ctx_cols {
        cols.push(table.names[*c].clone());
        conditions.push(json!({
            "column": table.names[*c],
            "value": coded.labels[*lvl as usize],
        }));
    }
    params.insert("columns".into(), json!(cols));
    params.insert("column".into(), json!(tname));
    params.insert("conditions".into(), json!(conditions));
    let mut metrics: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    metrics.insert("n".into(), json!(cand.subset));
    metrics.insert("dominant_level".into(), json!(dom_label));
    metrics.insert("dominant_share".into(), json!(cand.share));
    metrics.insert("outlier_count".into(), json!(cand.breakers.len()));
    metrics.insert("outlier_rows".into(), json!(cand.breakers));
    metrics.insert(
        "outlier_values".into(),
        json!(cand
            .breakers
            .iter()
            .map(|&r| tcoded.labels[tcoded.codes[r].unwrap() as usize].clone())
            .collect::<Vec<_>>()),
    );
    metrics.insert("log10_significance".into(), json!(cand.logtail));
    let score = 0.60 + 0.30 * (-cand.logtail / 60.0).min(1.0);
    Finding::new(
        Pattern::CategoricalOutlier,
        params,
        claim,
        EvidenceBasis::Measured,
        score.clamp(0.0, 0.9),
        metrics,
        Some(
            "verify the flagged rows at the source; a value contradicting a near-universal pattern is often a coding or entry error"
                .into(),
        ),
        Method::new(
            "surprises",
            "near-degenerate conditional distribution",
            "onceover subset-conditional surprise v1 (issues #31, #68)",
        ),
    )
}

fn profile_categorical_surprises(
    table: &TypedTable,
    excluded: &[usize],
    cfg: &SurpriseConfig,
    budget: usize,
) -> (Vec<Finding>, usize, usize) {
    let words = (table.n + 63) / 64;
    // Eligible columns coded once: conditions may be any eligible
    // column; targets are the text/bool ones.
    let coded: Vec<Option<Coded>> = (0..table.cols.len())
        .map(|i| {
            if excluded.contains(&i) {
                None
            } else {
                code_column(table, i, cfg)
            }
        })
        .collect();
    let is_text_target = |i: usize| {
        matches!(
            table.cols[i],
            TypedCol::Str { .. } | TypedCol::StrOwned { .. } | TypedCol::Bool { .. }
        )
    };
    let targets: Vec<usize> = (0..table.cols.len())
        .filter(|&i| coded[i].is_some() && is_text_target(i))
        .collect();
    let conditions: Vec<usize> = (0..table.cols.len())
        .filter(|&i| coded[i].is_some())
        .collect();
    let mut candidates: Vec<CatCandidate> = Vec::new();
    let mut contexts_evaluated = 0usize;

    for &t in &targets {
        let tcoded = coded[t].as_ref().unwrap();
        let tlevels = tcoded.labels.len();
        // Global rate of each target level (for significance).
        let mut gcounts = vec![0usize; tlevels];
        let mut gtotal = 0usize;
        for c in tcoded.codes.iter().flatten() {
            gcounts[*c as usize] += 1;
            gtotal += 1;
        }
        if gtotal < cfg.min_subset_rows {
            continue;
        }
        let conds: Vec<usize> = conditions.iter().copied().filter(|&c| c != t).collect();
        // Item bitsets per condition column.
        let item_bits: Vec<Vec<Vec<u64>>> = conds
            .iter()
            .map(|&c| {
                let cc = coded[c].as_ref().unwrap();
                (0..cc.labels.len() as u32)
                    .map(|lvl| bitset_for(&cc.codes, lvl, words))
                    .collect()
            })
            .collect();
        let consider = |items: Vec<(usize, u32)>,
                        ctx: &[u64],
                        cands: &mut Vec<CatCandidate>,
                        counted: &mut usize| {
            *counted += 1;
            if let Some((subset, dom, dom_count, breakers)) =
                eval_context(ctx, &tcoded.codes, tlevels)
            {
                if subset < cfg.min_subset_rows {
                    return;
                }
                let share = dom_count as f64 / subset as f64;
                if share >= cfg.min_dominant_share
                    && !breakers.is_empty()
                    && breakers.len() <= cfg.max_breakers
                {
                    let p0 = gcounts[dom as usize] as f64 / gtotal as f64;
                    let logtail = log10_binomial_tail(subset, dom_count, p0);
                    cands.push(CatCandidate {
                        target: t,
                        items,
                        dom,
                        share,
                        subset,
                        breakers,
                        logtail,
                    });
                }
            }
        };
        // Level 1: every condition level; record each column's
        // best share for the deep-column ranking.
        let mut best_share = vec![0.0f64; conds.len()];
        for (ci, _) in conds.iter().enumerate() {
            let cc = coded[conds[ci]].as_ref().unwrap();
            for lvl in 0..cc.labels.len() as u32 {
                let ctx = &item_bits[ci][lvl as usize];
                contexts_evaluated += 1;
                if let Some((subset, dom, dom_count, breakers)) =
                    eval_context(ctx, &tcoded.codes, tlevels)
                {
                    if subset < cfg.min_subset_rows {
                        continue;
                    }
                    let share = dom_count as f64 / subset as f64;
                    if share > best_share[ci] {
                        best_share[ci] = share;
                    }
                    if share >= cfg.min_dominant_share
                        && !breakers.is_empty()
                        && breakers.len() <= cfg.max_breakers
                    {
                        let p0 = gcounts[dom as usize] as f64 / gtotal as f64;
                        candidates.push(CatCandidate {
                            target: t,
                            items: vec![(conds[ci], lvl)],
                            dom,
                            share,
                            subset,
                            breakers,
                            logtail: log10_binomial_tail(subset, dom_count, p0),
                        });
                    }
                }
            }
        }
        if cfg.max_context_conditions >= 2 {
            // Deep columns: top max_deep_conditions by best share.
            let mut order: Vec<usize> = (0..conds.len()).collect();
            order.sort_by(|&a, &b| {
                best_share[b]
                    .partial_cmp(&best_share[a])
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.cmp(&b))
            });
            order.truncate(cfg.max_deep_conditions);
            order.sort();
            let deep_items: Vec<(usize, u32)> = order
                .iter()
                .flat_map(|&ci| {
                    (0..coded[conds[ci]].as_ref().unwrap().labels.len() as u32)
                        .map(move |lvl| (ci, lvl))
                })
                .collect();
            // Level 2: pairs of items from distinct deep columns.
            let mut beam: Vec<(Vec<(usize, u32)>, Vec<u64>, f64, usize)> = Vec::new();
            let mut scratch = vec![0u64; words];
            for i in 0..deep_items.len() {
                for j in i + 1..deep_items.len() {
                    let (ci, li) = deep_items[i];
                    let (cj, lj) = deep_items[j];
                    if ci == cj {
                        continue;
                    }
                    for w in 0..words {
                        scratch[w] = item_bits[ci][li as usize][w] & item_bits[cj][lj as usize][w];
                    }
                    let items = vec![(conds[ci], li), (conds[cj], lj)];
                    if let Some((subset, _, dom_count, brk)) =
                        eval_context(&scratch, &tcoded.codes, tlevels)
                    {
                        if subset >= cfg.min_subset_rows {
                            let share = dom_count as f64 / subset as f64;
                            if share >= cfg.deep_share_floor
                                && !brk.is_empty()
                                && brk.len() <= cfg.max_breakers * 4
                            {
                                beam.push((items.clone(), scratch.clone(), share, subset));
                            }
                        }
                    }
                    consider(items, &scratch, &mut candidates, &mut contexts_evaluated);
                }
            }
            // Level 3: extend the beam with one more deep item.
            if cfg.max_context_conditions >= 3 {
                beam.sort_by(|a, b| {
                    b.2.partial_cmp(&a.2)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(b.3.cmp(&a.3))
                });
                beam.truncate(cfg.context_beam);
                for (items, bits, _, _) in &beam {
                    let used: Vec<usize> = items.iter().map(|(c, _)| *c).collect();
                    for &(ci, lvl) in &deep_items {
                        if used.contains(&conds[ci]) {
                            continue;
                        }
                        for w in 0..words {
                            scratch[w] = bits[w] & item_bits[ci][lvl as usize][w];
                        }
                        let mut items3 = items.clone();
                        items3.push((conds[ci], lvl));
                        consider(items3, &scratch, &mut candidates, &mut contexts_evaluated);
                    }
                }
            }
        }
    }

    // Selection: significance order, brevity as tie-break, at
    // most 3 findings per target, a breaker row reported once
    // per target.
    candidates.sort_by(|a, b| {
        a.logtail
            .partial_cmp(&b.logtail)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.items.len().cmp(&b.items.len()))
            .then(b.subset.cmp(&a.subset))
    });
    let mut findings: Vec<Finding> = Vec::new();
    let mut reported: BTreeMap<usize, std::collections::BTreeSet<usize>> = BTreeMap::new();
    let mut per_target: BTreeMap<usize, usize> = BTreeMap::new();
    for cand in &candidates {
        if findings.len() >= budget {
            break;
        }
        if per_target.get(&cand.target).copied().unwrap_or(0) >= 3 {
            continue;
        }
        let seen = reported.entry(cand.target).or_default();
        let fresh: Vec<usize> = cand
            .breakers
            .iter()
            .copied()
            .filter(|r| !seen.contains(r))
            .collect();
        if fresh.is_empty() {
            continue;
        }
        for &r in &fresh {
            seen.insert(r);
        }
        *per_target.entry(cand.target).or_insert(0) += 1;
        let tcoded = coded[cand.target].as_ref().unwrap();
        let ctx_cols: Vec<(usize, &Coded, u32)> = cand
            .items
            .iter()
            .map(|&(c, lvl)| (c, coded[c].as_ref().unwrap(), lvl))
            .collect();
        let mut cand = CatCandidate {
            target: cand.target,
            items: cand.items.clone(),
            dom: cand.dom,
            share: cand.share,
            subset: cand.subset,
            breakers: fresh,
            logtail: cand.logtail,
        };
        cand.breakers.sort_unstable();
        findings.push(make_categorical_finding(table, cand.target, tcoded, &ctx_cols, &cand));
    }
    (findings, targets.len(), contexts_evaluated)
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
    let numeric_findings = findings.len();
    let (cat_findings, cat_targets, cat_contexts) = if findings.len() < cfg.max_findings {
        profile_categorical_surprises(
            table,
            excluded,
            cfg,
            cfg.max_findings - findings.len(),
        )
    } else {
        (Vec::new(), 0, 0)
    };
    let cat_count = cat_findings.len();
    findings.extend(cat_findings);
    let detail = format!(
        "{} numeric columns scanned against {} conditioning columns; {} surprise findings; {} categorical columns scanned in {} contexts up to {} conditions; {} categorical surprise findings",
        scanned,
        conditions.len(),
        numeric_findings,
        cat_targets,
        cat_contexts,
        cfg.max_context_conditions,
        cat_count
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

    fn str_col(vals: Vec<Option<&str>>) -> TypedCol<'static> {
        TypedCol::StrOwned {
            vals: vals
                .into_iter()
                .map(|v| v.map(|s| s.to_string()))
                .collect(),
        }
    }

    fn cat_table(names: Vec<&str>, cols: Vec<TypedCol<'static>>, n: usize) -> TypedTable<'static> {
        TypedTable {
            n,
            names: names.iter().map(|s| s.to_string()).collect(),
            dtypes: vec!["string".into(); names.len()],
            cols,
        }
    }

    #[test]
    fn categorical_flags_single_breaker() {
        // grp = A (150 rows): outcome is "ok" except row 7.
        // grp = B (150 rows): outcome varies ordinarily.
        let mut grp: Vec<Option<&str>> = Vec::new();
        let mut out: Vec<Option<&str>> = Vec::new();
        for i in 0..150 {
            grp.push(Some("A"));
            out.push(Some(if i == 7 { "bad" } else { "ok" }));
        }
        for i in 0..150 {
            grp.push(Some("B"));
            out.push(Some(if i % 5 == 0 { "bad" } else { "ok" }));
        }
        let table = cat_table(
            vec!["grp", "outcome"],
            vec![str_col(grp), str_col(out)],
            300,
        );
        let cfg = SurpriseConfig::default();
        let (findings, _, _) = profile_categorical_surprises(&table, &[], &cfg, 20);
        assert_eq!(findings.len(), 1, "claims: {:?}", findings.iter().map(|f| &f.claim).collect::<Vec<_>>());
        assert!(findings[0].claim.contains("row 7"), "claim: {}", findings[0].claim);
        assert!(findings[0].claim.contains("grp = A"), "claim: {}", findings[0].claim);
    }

    #[test]
    fn categorical_quiet_on_ordinary_variation() {
        // Every subset splits outcome roughly 80/20: no context
        // is near-degenerate.
        let mut grp: Vec<Option<&str>> = Vec::new();
        let mut out: Vec<Option<&str>> = Vec::new();
        for i in 0..300 {
            grp.push(Some(if i % 2 == 0 { "A" } else { "B" }));
            out.push(Some(if i % 5 == 0 { "bad" } else { "ok" }));
        }
        let table = cat_table(
            vec!["grp", "outcome"],
            vec![str_col(grp), str_col(out)],
            300,
        );
        let cfg = SurpriseConfig::default();
        let (findings, _, _) = profile_categorical_surprises(&table, &[], &cfg, 20);
        assert!(findings.is_empty(), "claims: {:?}", findings.iter().map(|f| &f.claim).collect::<Vec<_>>());
    }

    #[test]
    fn categorical_flags_pair_context_breaker() {
        // outcome is "y" only inside (c1 = a, c2 = b), with one
        // breaker; each single condition is uninformative.
        let mut c1: Vec<Option<&str>> = Vec::new();
        let mut c2: Vec<Option<&str>> = Vec::new();
        let mut out: Vec<Option<&str>> = Vec::new();
        for i in 0..400 {
            let (a, b, o): (&str, &str, &str) = match i {
                0..=99 => ("a", "b", if i == 42 { "x" } else { "y" }),
                100..=199 => ("a", "z", "x"),
                200..=299 => ("w", "b", "x"),
                _ => ("w", "z", if i % 2 == 0 { "x" } else { "y" }),
            };
            c1.push(Some(a));
            c2.push(Some(b));
            out.push(Some(o));
        }
        let table = cat_table(
            vec!["c1", "c2", "outcome"],
            vec![str_col(c1), str_col(c2), str_col(out)],
            400,
        );
        let cfg = SurpriseConfig::default();
        let (findings, _, _) = profile_categorical_surprises(&table, &[], &cfg, 20);
        // The planted context ranks first; the same row is also
        // the breaker from the other two columns' perspectives
        // (one finding per target, by design).
        assert!(!findings.is_empty());
        assert!(findings[0].claim.contains("row 42"), "claim: {}", findings[0].claim);
        assert!(
            findings[0].claim.contains("c1 = a") && findings[0].claim.contains("c2 = b"),
            "claim: {}",
            findings[0].claim
        );
        assert!(
            findings.iter().all(|f| f.claim.contains("row 42")),
            "claims: {:?}",
            findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
    }

    #[test]
    fn binomial_tail_orders_significance() {
        let strong = log10_binomial_tail(200, 199, 0.5);
        let weak = log10_binomial_tail(200, 110, 0.5);
        assert!(strong < -50.0, "strong tail {strong}");
        assert!(weak > -5.0, "weak tail {weak}");
    }
}
