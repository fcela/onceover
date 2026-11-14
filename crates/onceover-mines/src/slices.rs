//! Searched conjunction-conditional distribution shifts (issue #272).
//!
//! The profiler states extremes inside one categorical level and
//! many fixed patterns, but until this pass it never *searched*
//! for the conjunction of conditions under which a column's
//! distribution moves. This pass searches slices — conjunctions
//! of categorical levels and numeric threshold predicates — and
//! states the ones whose target distribution differs from the
//! rest of the table:
//!
//! > "Within channel = 'partner' and basket > 100 (214 rows),
//! > delivery_time's median is 3.1 against 1.2 overall, and 22%
//! > of rows exceed the column's 95th percentile (7) against
//! > 5% overall."
//!
//! This is the core move of subgroup discovery (Herrera et al.,
//! "An overview on subgroup discovery: foundations", KAIS 2011),
//! exceptional model mining (Leman, Feelders & Knobbe, ECML PKDD
//! 2008), and slice finding in ML validation (Chung et al.,
//! "Slice Finder", 2019). The search is the easy half; the design
//! problem is false-positive control, and the control stack here
//! is the one the issue filed:
//!
//! - **Split-sample confirmation.** The pass's row sample is
//!   split into two deterministic halves. Slices are searched on
//!   one half and stated only when the same effect, in the same
//!   direction, clears a confirmation bar on the held-out half.
//! - **Support floors and depth caps.** A slice needs at least
//!   `max(50, 1% of the sample)` rows on the full sample and at
//!   least 25 rows in each half. Conjunction depth is at most 2
//!   (3 at the deep budget, extending only slices that cleared
//!   the search bars — a beam, stated as such in coverage).
//! - **Benjamini–Hochberg across the confirmed family.** Every
//!   candidate that clears the search bars gets a p-value from
//!   the confirmation half (Mann–Whitney for numeric targets,
//!   two-proportion z for level shares); BH q-values are computed
//!   over that whole family, and a statement needs q <= 0.05 on
//!   top of the confirmation effect bars.
//! - **The searched candidate count is stated in coverage**, so
//!   the size of the search the controls had to discipline is
//!   part of the record.
//!
//! The uncalibrated version — raw top-k by in-sample score — is
//! exactly the failure mode the robust subgroup-discovery
//! literature exists to fix, and it is not what ships.
//!
//! Bars (defaults; the prototype measurements behind them are
//! recorded in docs/benchmarks/2026-11-13-slice-shifts.md):
//! a numeric slice clears the search bars when its share of rows
//! above the target's global 95th percentile is at least 0.15
//! (base rate 0.05), or its median differs from the global median
//! by at least 0.75 IQR. Confirmation requires the same form on
//! the held-out half at 0.10 tail share and at least half the
//! search-half share, or 0.5 IQR in the same
//! direction. A categorical level share clears the search bars
//! at a rate ratio of at least 2 or at most 0.5 against the
//! global rate, an absolute difference of at least 0.02, and at
//! least 5 expected positives under the global rate; confirmation
//! requires ratio at least 1.5 (high side), at most 0.5 (low
//! side), or — for slices whose search-half rate is exactly 0 —
//! a held-out ratio of at most 0.25, with an absolute difference
//! of at least 0.01.
//!
//! Ranking is by absolute effect, not by ratio alone: a naive
//! ratio ranking front-loads degenerate slices (a slice whose
//! rate is 0 because its maximum never reaches the threshold is
//! a threshold artifact, not a finding). Statement caps bind per
//! target column (8) as well as overall (20): without the
//! per-target cap, one target's family of near-restatements —
//! every level of a duplicated column against every level of its
//! duplicate, at rate exactly 0 — fills the whole statement cap.
//! Two further statement hygiene rules: slices with identical
//! row sets are one statement (the shallowest description is
//! kept), and a deeper slice whose row set is at least 90% the
//! same as a shallower stated slice for the same target is
//! demoted as a restatement of it, under the project's
//! triviality-demotion convention — demoted, never removed.
//! Where a slice carries a
//! disproportionate share of the target's top-1% values, that
//! concentration is stated as part of the same finding's
//! evidence, not as a separate finding.
//!
//! The pass is opt-in: a searched statement family with a
//! split-sample design is heavier and more inferential than the
//! default set's direct measurements, and the project's
//! convention for passes of this cost class (views, changeforest)
//! is explicit selection. Findings are `not_checkable` for
//! `report.verify`: their statistics are measured on the pass's
//! deterministic sample and split, which a recount on the full
//! frame does not reproduce; the slice definition is carried in
//! the statement definition so the slice itself can be recounted
//! on any row set directly.

use onceover_core::{sample_rows, TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug)]
pub struct SliceConfig {
    /// Rows of the shared sample the search runs on; the pipeline
    /// sets it from the tier's encoding row cap (20,000 standard,
    /// 50,000 deep).
    pub row_cap: usize,
    /// Maximum conjunction depth: 2, or 3 at the deep budget.
    pub max_depth: usize,
    /// Maximum levels kept per categorical column (most frequent
    /// first), and the distinct-value ceiling for a column to be
    /// treated as categorical.
    pub max_levels: usize,
    /// Maximum categorical condition columns searched.
    pub max_condition_columns: usize,
    /// Maximum numeric columns contributing threshold predicates.
    pub max_threshold_columns: usize,
    /// Maximum numeric target columns.
    pub max_numeric_targets: usize,
    /// Maximum categorical target columns (each level is a target).
    pub max_categorical_targets: usize,
    /// Absolute support floor, on the full sample.
    pub min_support: usize,
    /// Support floor as a fraction of the sample; the effective
    /// floor is the larger of the two.
    pub min_support_frac: f64,
    /// Minimum slice rows in each half for a slice to be evaluated.
    pub half_support: usize,
    /// Search bar: slice share above the global 95th percentile.
    pub tail_share_bar: f64,
    /// Search bar: |slice median - global median| in IQR units.
    pub median_shift_bar: f64,
    /// Confirmation bars for the tail form (held-out half): the
    /// share must clear this floor and retain at least
    /// `confirm_tail_retention` of the search-half share (the
    /// prototype protocol's holdout criterion).
    pub confirm_tail_share_bar: f64,
    pub confirm_tail_retention: f64,
    /// Confirmation bar for the median form, IQR units, same
    /// direction as the search half.
    pub confirm_median_shift_bar: f64,
    /// Search bars for level shares: rate ratio outside
    /// [rate_ratio_low, rate_ratio_high] is required, with an
    /// absolute difference of at least rate_diff_bar and at least
    /// expected_positives expected positives under the global rate.
    pub rate_ratio_high: f64,
    pub rate_ratio_low: f64,
    pub rate_diff_bar: f64,
    pub expected_positives: f64,
    /// Confirmation bars for level shares.
    pub confirm_rate_ratio_high: f64,
    pub confirm_rate_ratio_low: f64,
    /// Confirmation bar when the search-half rate is exactly 0:
    /// the held-out ratio must be at most this.
    pub confirm_rate_zero_ratio: f64,
    pub confirm_rate_diff_bar: f64,
    /// Benjamini–Hochberg q bar over the confirmation family.
    pub bh_alpha: f64,
    /// Findings stated by the pass; further confirmed slices are
    /// counted in coverage, not stated.
    pub max_findings: usize,
    /// Findings stated per target column. Without it, one
    /// target's family of near-restatements (every level of a
    /// duplicated column against every level of its duplicate, at
    /// rate exactly 0) fills the whole statement cap and crowds
    /// out the statements about other targets.
    pub max_per_target: usize,
    /// Work budget in approximate cell visits (slice rows examined
    /// across target evaluations, plus bitset words per slice).
    /// Exhaustion stops the search cooperatively and coverage
    /// states how much of the slice space was examined.
    pub work_budget: u64,
    /// Cooperative deadline (issue #117, stage 2), polled at slice
    /// boundaries.
    pub deadline: crate::deadline::Deadline,
}

impl Default for SliceConfig {
    fn default() -> Self {
        Self {
            row_cap: 20_000,
            max_depth: 2,
            max_levels: 25,
            max_condition_columns: 12,
            max_threshold_columns: 12,
            max_numeric_targets: 24,
            max_categorical_targets: 8,
            min_support: 50,
            min_support_frac: 0.01,
            half_support: 25,
            tail_share_bar: 0.15,
            median_shift_bar: 0.75,
            confirm_tail_share_bar: 0.10,
            confirm_tail_retention: 0.5,
            confirm_median_shift_bar: 0.5,
            rate_ratio_high: 2.0,
            rate_ratio_low: 0.5,
            rate_diff_bar: 0.02,
            expected_positives: 5.0,
            confirm_rate_ratio_high: 1.5,
            confirm_rate_ratio_low: 0.5,
            confirm_rate_zero_ratio: 0.25,
            confirm_rate_diff_bar: 0.01,
            bh_alpha: 0.05,
            max_findings: 20,
            max_per_target: 8,
            work_budget: 300_000_000,
            deadline: crate::deadline::Deadline::none(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SliceStatus {
    NotApplicable(String),
    Ran(String),
    Capped(String),
}

pub struct SliceOutcome {
    pub findings: Vec<Finding>,
    pub status: SliceStatus,
}

// ---------------------------------------------------------------------
// Small statistics helpers.
// ---------------------------------------------------------------------

pub(crate) fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

pub(crate) fn quantile_sorted(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    let frac = pos - lo as f64;
    sorted[lo] * (1.0 - frac) + sorted[hi] * frac
}

pub(crate) fn median_sorted(sorted: &[f64]) -> f64 {
    quantile_sorted(sorted, 0.5)
}

/// Two-sided Mann–Whitney U p-value (normal approximation with
/// tie correction and continuity correction) for slice values
/// `a` against complement values `b`.
pub(crate) fn mann_whitney_p(a: &[f64], b: &[f64]) -> f64 {
    let n1 = a.len() as f64;
    let n2 = b.len() as f64;
    if n1 < 3.0 || n2 < 3.0 {
        return 1.0;
    }
    let mut combined: Vec<(f64, bool)> = a
        .iter()
        .map(|&v| (v, true))
        .chain(b.iter().map(|&v| (v, false)))
        .collect();
    combined.sort_by(|x, y| x.0.total_cmp(&y.0));
    let n = combined.len() as f64;
    let mut rank_sum_a = 0.0f64;
    let mut tie_term = 0.0f64;
    let mut i = 0usize;
    while i < combined.len() {
        let mut j = i + 1;
        while j < combined.len() && combined[j].0 == combined[i].0 {
            j += 1;
        }
        let avg_rank = (i + 1 + j) as f64 / 2.0;
        for item in &combined[i..j] {
            if item.1 {
                rank_sum_a += avg_rank;
            }
        }
        let t = (j - i) as f64;
        if t > 1.0 {
            tie_term += t * (t * t - 1.0);
        }
        i = j;
    }
    let u = rank_sum_a - n1 * (n1 + 1.0) / 2.0;
    let mean = n1 * n2 / 2.0;
    let var = n1 * n2 / 12.0 * ((n + 1.0) - tie_term / (n * (n - 1.0)));
    if var <= 0.0 {
        return 1.0;
    }
    let z = (u - mean).abs() - 0.5;
    if z <= 0.0 {
        return 1.0;
    }
    let z = z / var.sqrt();
    (2.0 * crate::dep::norm_sf(z)).clamp(0.0, 1.0)
}

/// Two-sided two-proportion z-test p-value.
fn two_proportion_p(x1: usize, n1: usize, x2: usize, n2: usize) -> f64 {
    if n1 == 0 || n2 == 0 {
        return 1.0;
    }
    let p1 = x1 as f64 / n1 as f64;
    let p2 = x2 as f64 / n2 as f64;
    let pooled = (x1 + x2) as f64 / (n1 + n2) as f64;
    let se = (pooled * (1.0 - pooled) * (1.0 / n1 as f64 + 1.0 / n2 as f64)).sqrt();
    if se <= 0.0 {
        return if p1 == p2 { 1.0 } else { 0.0 };
    }
    let z = (p1 - p2).abs() / se;
    (2.0 * crate::dep::norm_sf(z)).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------
// Conditions and targets over the sample.
// ---------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum CondKind {
    Level,
    Above,
}

#[derive(Clone)]
struct Cond {
    col: usize,
    kind: CondKind,
    /// Statement fragment: `channel = 'partner'` or `basket > 100`.
    text: String,
    /// The predicate as JSON, for the statement definition.
    predicate: Value,
    bits: Vec<u64>,
}

struct NumTarget {
    col: usize,
    name: String,
    /// Values aligned with the sample positions.
    vals: Vec<Option<f64>>,
    median: f64,
    iqr: f64,
    p95: f64,
    p99: f64,
    /// Share of all sample rows (nulls included) above p95 / p99.
    tail_share: f64,
    top1_count: usize,
}

struct CatTargetCol {
    col: usize,
    name: String,
    /// Level code per sample position.
    codes: Vec<Option<u32>>,
    labels: Vec<String>,
    /// Global rate of each level over the column's non-null rows.
    rates: Vec<f64>,
}

/// Level labels per row for a categorical-eligible column, over
/// the sample positions. Eligible: string, boolean, and integer
/// columns whose distinct non-null count is within
/// `2..=max_levels`. Returns (labels per row, distinct labels).
pub(crate) fn level_labels(
    table: &TypedTable,
    col: usize,
    sample: &[u32],
    max_levels: usize,
) -> Option<(Vec<Option<String>>, Vec<String>)> {
    let mut seen: Vec<String> = Vec::new();
    let mut labels: Vec<Option<String>> = Vec::with_capacity(sample.len());
    let push = |seen: &mut Vec<String>, v: String| -> Option<()> {
        if !seen.contains(&v) {
            if seen.len() >= max_levels {
                return None;
            }
            seen.push(v.clone());
        }
        Some(())
    };
    match &table.cols[col] {
        TypedCol::Str { vals } => {
            for &r in sample {
                match vals[r as usize] {
                    Some(s) => {
                        push(&mut seen, s.to_string())?;
                        labels.push(Some(s.to_string()));
                    }
                    None => labels.push(None),
                }
            }
        }
        TypedCol::StrOwned { vals } => {
            for &r in sample {
                match &vals[r as usize] {
                    Some(s) => {
                        push(&mut seen, s.clone())?;
                        labels.push(Some(s.clone()));
                    }
                    None => labels.push(None),
                }
            }
        }
        TypedCol::Bool { vals } => {
            for &r in sample {
                match vals[r as usize] {
                    Some(b) => {
                        let s = if b { "true" } else { "false" }.to_string();
                        push(&mut seen, s)?;
                        labels.push(Some(if b { "true" } else { "false" }.to_string()));
                    }
                    None => labels.push(None),
                }
            }
        }
        TypedCol::I64 { vals } => {
            for &r in sample {
                match vals.at(r as usize) {
                    Some(v) => {
                        push(&mut seen, v.to_string())?;
                        labels.push(Some(v.to_string()));
                    }
                    None => labels.push(None),
                }
            }
        }
        TypedCol::U64 { vals } => {
            for &r in sample {
                match vals.at(r as usize) {
                    Some(v) => {
                        push(&mut seen, v.to_string())?;
                        labels.push(Some(v.to_string()));
                    }
                    None => labels.push(None),
                }
            }
        }
        _ => return None,
    }
    if seen.len() < 2 {
        return None;
    }
    Some((labels, seen))
}

/// JSON value for a level of `col`, matching the column's kind so
/// the statement definition carries the value in its own type.
pub(crate) fn level_json(table: &TypedTable, col: usize, label: &str) -> Value {
    match &table.cols[col] {
        TypedCol::Bool { .. } => json!(label == "true"),
        TypedCol::I64 { .. } => label
            .parse::<i64>()
            .map(|v| json!(v))
            .unwrap_or_else(|_| json!(label)),
        TypedCol::U64 { .. } => label
            .parse::<u64>()
            .map(|v| json!(v))
            .unwrap_or_else(|_| json!(label)),
        _ => json!(label),
    }
}

pub(crate) fn bits_for(n: usize, mut set: impl FnMut(usize) -> bool) -> Vec<u64> {
    let mut bits = vec![0u64; n.div_ceil(64)];
    for i in 0..n {
        if set(i) {
            bits[i / 64] |= 1u64 << (i % 64);
        }
    }
    bits
}

pub(crate) fn bits_and(a: &[u64], b: &[u64]) -> Vec<u64> {
    a.iter().zip(b).map(|(x, y)| x & y).collect()
}

pub(crate) fn bits_count(bits: &[u64]) -> usize {
    bits.iter().map(|w| w.count_ones() as usize).sum()
}

pub(crate) fn bits_hash(bits: &[u64]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bits.hash(&mut h);
    h.finish()
}

/// Iterate the sample positions set in `bits`.
pub(crate) fn bits_positions(bits: &[u64]) -> impl Iterator<Item = usize> + '_ {
    bits.iter().enumerate().flat_map(|(w, &word)| {
        (0..64).filter_map(move |b| {
            if word >> b & 1 == 1 {
                Some(w * 64 + b)
            } else {
                None
            }
        })
    })
}

// ---------------------------------------------------------------------
// Candidates.
// ---------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetRef {
    Num(usize),
    Cat(usize, u32),
}

struct Candidate {
    /// Condition indices, sorted.
    conds: Vec<usize>,
    target: TargetRef,
    /// "tail" | "median" | "rate".
    form: &'static str,
    /// Slice rows: full sample, search half, confirmation half.
    n_full: usize,
    n_search: usize,
    n_confirm: usize,
    /// Full-sample slice bitset (for row-set dedupe and overlap).
    bits: Vec<u64>,
    // Numeric-form statistics (full / search / confirmation).
    median_full: f64,
    median_search: f64,
    median_confirm: f64,
    tail_full: f64,
    tail_search: f64,
    tail_confirm: f64,
    // Rate-form statistics.
    rate_full: f64,
    rate_search: f64,
    rate_confirm: f64,
    /// Confirmation-half p-value for the candidate's form.
    p_confirm: f64,
    /// BH q-value over the confirmation family (filled in later).
    q_value: f64,
    /// Whether the confirmation-half effect bars cleared.
    confirmed: bool,
    /// Normalised absolute effect in [0, 1], for ranking.
    effect_norm: f64,
}

struct NumStats {
    median: f64,
    tail_share: f64,
    values: Vec<f64>,
}

fn num_stats(bits: &[u64], t: &NumTarget, total_rows: usize) -> NumStats {
    let mut values: Vec<f64> = Vec::new();
    let mut tail = 0usize;
    for pos in bits_positions(bits) {
        if let Some(v) = t.vals[pos] {
            if v > t.p95 {
                tail += 1;
            }
            values.push(v);
        }
    }
    values.sort_by(|a, b| a.total_cmp(b));
    NumStats {
        median: if values.is_empty() {
            f64::NAN
        } else {
            median_sorted(&values)
        },
        tail_share: tail as f64 / total_rows.max(1) as f64,
        values,
    }
}

struct RateStats {
    rate: f64,
    positives: usize,
    denom: usize,
}

fn rate_stats(bits: &[u64], t: &CatTargetCol, level: u32) -> RateStats {
    let mut positives = 0usize;
    let mut denom = 0usize;
    for pos in bits_positions(bits) {
        if let Some(code) = t.codes[pos] {
            denom += 1;
            if code == level {
                positives += 1;
            }
        }
    }
    RateStats {
        rate: if denom == 0 {
            f64::NAN
        } else {
            positives as f64 / denom as f64
        },
        positives,
        denom,
    }
}

// ---------------------------------------------------------------------
// The pass.
// ---------------------------------------------------------------------

pub fn profile_slice_shifts(
    table: &TypedTable,
    excluded: &[usize],
    cfg: &SliceConfig,
) -> SliceOutcome {
    let na = |msg: String| SliceOutcome {
        findings: Vec::new(),
        status: SliceStatus::NotApplicable(msg),
    };
    if cfg.row_cap == 0 || table.n < 100 {
        return na(format!("fewer than 100 rows to search ({} rows)", table.n));
    }
    let sample: Vec<u32> = sample_rows(table.n, cfg.row_cap);
    let n_sample = sample.len();
    if n_sample < 100 {
        return na(format!("fewer than 100 rows on the sample ({n_sample})"));
    }
    let words = n_sample.div_ceil(64);
    let min_support = cfg
        .min_support
        .max((cfg.min_support_frac * n_sample as f64).ceil() as usize);
    // Deterministic split: the parity of a fixed hash of the
    // sample position. Independent of the values, so a shuffled
    // target faces the same split.
    let half_a = bits_for(n_sample, |i| splitmix64(i as u64).is_multiple_of(2));
    let half_b: Vec<u64> = half_a.iter().map(|w| !w).collect();
    let half_b = {
        let mut b = half_b;
        // Mask off bits beyond the sample.
        let rem = n_sample % 64;
        if rem != 0 {
            let last = b.len() - 1;
            b[last] &= (1u64 << rem) - 1;
        }
        b
    };
    let n_half_a = bits_count(&half_a);
    let n_half_b = n_sample - n_half_a;

    let excluded_set: std::collections::BTreeSet<usize> = excluded.iter().copied().collect();

    // ---- condition columns: categorical levels -------------------
    let mut conds: Vec<Cond> = Vec::new();
    let mut cat_cols: Vec<usize> = Vec::new();
    let mut level_rows: Vec<(usize, String, usize)> = Vec::new(); // (col, label, count)
    for col in 0..table.names.len() {
        if excluded_set.contains(&col) {
            continue;
        }
        let Some((labels, _distinct)) = level_labels(table, col, &sample, cfg.max_levels) else {
            continue;
        };
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for l in labels.iter().flatten() {
            *counts.entry(l.clone()).or_default() += 1;
        }
        let kept: Vec<(String, usize)> = counts
            .into_iter()
            .filter(|(_, c)| *c >= min_support)
            .collect();
        if kept.len() < 2 {
            continue;
        }
        cat_cols.push(col);
        for (label, count) in kept {
            level_rows.push((col, label, count));
        }
    }
    // Cap the condition columns: most levels first, table order
    // breaks ties. Levels of dropped columns leave the search.
    let mut cat_rank: Vec<usize> = cat_cols.clone();
    cat_rank.sort_by_key(|&col| {
        let n_levels = level_rows.iter().filter(|(c, _, _)| *c == col).count();
        (std::cmp::Reverse(n_levels), col)
    });
    let capped_cat_cols: std::collections::BTreeSet<usize> = cat_rank
        .iter()
        .take(cfg.max_condition_columns)
        .copied()
        .collect();
    let cat_cols_capped = cat_rank.len() > capped_cat_cols.len();
    level_rows.retain(|(c, _, _)| capped_cat_cols.contains(c));
    // Most frequent levels first within the cap.
    level_rows.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));
    level_rows.truncate(cfg.max_condition_columns * cfg.max_levels);
    {
        // Per-row labels are computed once per kept column and
        // shared by all of its level conditions.
        let mut labels_by_col: BTreeMap<usize, Vec<Option<String>>> = BTreeMap::new();
        for (col, _, _) in &level_rows {
            if !labels_by_col.contains_key(col) {
                let (labels, _) = level_labels(table, *col, &sample, cfg.max_levels)
                    .expect("column was eligible above");
                labels_by_col.insert(*col, labels);
            }
        }
        for (col, label, _) in &level_rows {
            let labels = &labels_by_col[col];
            let bits = bits_for(n_sample, |i| labels[i].as_deref() == Some(label.as_str()));
            conds.push(Cond {
                col: *col,
                kind: CondKind::Level,
                text: format!("{} = '{}'", table.names[*col], label),
                predicate: json!({
                    "column": table.names[*col],
                    "op": "eq",
                    "value": level_json(table, *col, label),
                }),
                bits,
            });
        }
    }

    // ---- condition columns: numeric thresholds -------------------
    let mut numeric_cols: Vec<usize> = Vec::new();
    for col in 0..table.names.len() {
        if excluded_set.contains(&col) {
            continue;
        }
        if matches!(
            table.cols[col],
            TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. }
        ) {
            numeric_cols.push(col);
        }
    }
    let sample_values = |col: usize| -> Vec<f64> {
        sample
            .iter()
            .filter_map(|&r| table.cols[col].numeric_at(r as usize))
            .collect()
    };
    let mut thr_rank: Vec<(usize, usize)> = numeric_cols
        .iter()
        .map(|&col| {
            let vals = sample_values(col);
            let mut sorted = vals.clone();
            sorted.sort_by(|a, b| a.total_cmp(b));
            sorted.dedup();
            (col, sorted.len())
        })
        .filter(|(_, distinct)| *distinct >= 20)
        .collect();
    thr_rank.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let thr_cols_capped = thr_rank.len() > cfg.max_threshold_columns;
    thr_rank.truncate(cfg.max_threshold_columns);
    let mut thr_cols: Vec<usize> = Vec::new();
    for (col, _) in &thr_rank {
        let col = *col;
        thr_cols.push(col);
        let mut sorted = sample_values(col);
        sorted.sort_by(|a, b| a.total_cmp(b));
        let max_v = *sorted.last().unwrap();
        for q in [0.75, 0.90] {
            let t = quantile_sorted(&sorted, q);
            if t.partial_cmp(&max_v) != Some(std::cmp::Ordering::Less) {
                continue;
            }
            let bits = bits_for(n_sample, |i| {
                table.cols[col]
                    .numeric_at(sample[i] as usize)
                    .is_some_and(|v| v > t)
            });
            if bits_count(&bits) < min_support {
                continue;
            }
            conds.push(Cond {
                col,
                kind: CondKind::Above,
                text: format!("{} > {}", table.names[col], crate::fmt_float(t)),
                predicate: json!({
                    "column": table.names[col],
                    "op": "gt",
                    "value": t,
                }),
                bits,
            });
        }
    }

    if conds.is_empty() {
        return na(
            "no eligible condition columns (categorical levels or numeric thresholds)".into(),
        );
    }

    // ---- targets --------------------------------------------------
    let mut num_targets: Vec<NumTarget> = Vec::new();
    let mut num_rank: Vec<(usize, usize)> = numeric_cols
        .iter()
        .map(|&col| {
            let nonnull = sample
                .iter()
                .filter(|&&r| table.cols[col].numeric_at(r as usize).is_some())
                .count();
            (col, nonnull)
        })
        .filter(|(_, n)| *n >= 50)
        .collect();
    num_rank.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let num_targets_capped = num_rank.len() > cfg.max_numeric_targets;
    num_rank.truncate(cfg.max_numeric_targets);
    for (col, _) in &num_rank {
        let col = *col;
        let vals: Vec<Option<f64>> = sample
            .iter()
            .map(|&r| table.cols[col].numeric_at(r as usize))
            .collect();
        let mut sorted: Vec<f64> = vals.iter().flatten().copied().collect();
        sorted.sort_by(|a, b| a.total_cmp(b));
        if sorted.len() < 50 {
            continue;
        }
        let q25 = quantile_sorted(&sorted, 0.25);
        let q75 = quantile_sorted(&sorted, 0.75);
        let iqr = q75 - q25;
        if iqr <= 0.0 {
            continue;
        }
        let p95 = quantile_sorted(&sorted, 0.95);
        let p99 = quantile_sorted(&sorted, 0.99);
        let tail = sorted.iter().filter(|&&v| v > p95).count();
        let top1 = sorted.iter().filter(|&&v| v > p99).count();
        num_targets.push(NumTarget {
            col,
            name: table.names[col].clone(),
            vals,
            median: median_sorted(&sorted),
            iqr,
            p95,
            p99,
            tail_share: tail as f64 / n_sample as f64,
            top1_count: top1,
        });
    }

    let mut cat_targets: Vec<CatTargetCol> = Vec::new();
    let mut cat_target_rank: Vec<usize> = cat_cols
        .iter()
        .copied()
        .filter(|c| capped_cat_cols.contains(c))
        .collect();
    cat_target_rank.sort_by_key(|&col| {
        let n_levels = level_rows.iter().filter(|(c, _, _)| *c == col).count();
        (std::cmp::Reverse(n_levels), col)
    });
    let cat_targets_capped = cat_target_rank.len() > cfg.max_categorical_targets;
    cat_target_rank.truncate(cfg.max_categorical_targets);
    for col in &cat_target_rank {
        let col = *col;
        let (labels, _) =
            level_labels(table, col, &sample, cfg.max_levels).expect("column was eligible above");
        let mut distinct: Vec<String> = Vec::new();
        for l in labels.iter().flatten() {
            if !distinct.contains(l) {
                distinct.push(l.clone());
            }
        }
        distinct.sort();
        let index: BTreeMap<String, u32> = distinct
            .iter()
            .enumerate()
            .map(|(i, s)| (s.clone(), i as u32))
            .collect();
        let codes: Vec<Option<u32>> = labels
            .iter()
            .map(|l| l.as_ref().map(|s| index[s]))
            .collect();
        let denom = codes.iter().filter(|c| c.is_some()).count();
        if denom == 0 {
            continue;
        }
        let mut rates = vec![0.0f64; distinct.len()];
        for c in codes.iter().flatten() {
            rates[*c as usize] += 1.0;
        }
        for r in rates.iter_mut() {
            *r /= denom as f64;
        }
        cat_targets.push(CatTargetCol {
            col,
            name: table.names[col].clone(),
            codes,
            labels: distinct,
            rates,
        });
    }

    if num_targets.is_empty() && cat_targets.is_empty() {
        return na(
            "no eligible target columns (numeric with positive IQR, or categorical levels)".into(),
        );
    }

    // ---- slice enumeration (depth 1 and 2) ------------------------
    let level_conds: Vec<usize> = conds
        .iter()
        .enumerate()
        .filter(|(_, c)| c.kind == CondKind::Level)
        .map(|(i, _)| i)
        .collect();
    let thr_conds: Vec<usize> = conds
        .iter()
        .enumerate()
        .filter(|(_, c)| c.kind == CondKind::Above)
        .map(|(i, _)| i)
        .collect();
    let mut slice_defs: Vec<Vec<usize>> = Vec::new();
    for &i in &level_conds {
        slice_defs.push(vec![i]);
    }
    for &i in &thr_conds {
        slice_defs.push(vec![i]);
    }
    if cfg.max_depth >= 2 {
        for (a, &i) in level_conds.iter().enumerate() {
            for &j in &level_conds[a + 1..] {
                if conds[i].col != conds[j].col {
                    slice_defs.push(vec![i, j]);
                }
            }
            for &j in &thr_conds {
                if conds[i].col != conds[j].col {
                    slice_defs.push(vec![i, j]);
                }
            }
        }
    }
    let total_slices = slice_defs.len();

    // ---- search + confirmation ------------------------------------
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut slices_examined = 0usize;
    let mut slices_with_support = 0usize;
    let mut pairs_evaluated = 0u64;
    let mut work: u64 = 0;
    let mut stopped = false;

    let process_slice = |slice: &[usize],
                         conds: &Vec<Cond>,
                         candidates: &mut Vec<Candidate>,
                         work: &mut u64,
                         pairs_evaluated: &mut u64,
                         slices_with_support: &mut usize|
     -> bool {
        // Returns false when the work budget is exhausted.
        let mut bits = conds[slice[0]].bits.clone();
        for &ci in &slice[1..] {
            bits = bits_and(&bits, &conds[ci].bits);
        }
        *work += words as u64;
        let n_full = bits_count(&bits);
        if n_full < min_support {
            return true;
        }
        let bits_search = bits_and(&bits, &half_a);
        let bits_confirm = bits_and(&bits, &half_b);
        let n_search = bits_count(&bits_search);
        let n_confirm = bits_count(&bits_confirm);
        if n_search < cfg.half_support || n_confirm < cfg.half_support {
            return true;
        }
        *slices_with_support += 1;
        let slice_cols: Vec<usize> = slice.iter().map(|&ci| conds[ci].col).collect();

        for (ti, t) in num_targets.iter().enumerate() {
            if slice_cols.contains(&t.col) {
                continue;
            }
            *pairs_evaluated += 1;
            *work += n_full as u64;
            if *work > cfg.work_budget {
                return false;
            }
            let s = num_stats(&bits_search, t, n_search);
            if s.values.len() < 20 {
                continue;
            }
            let tail_hit = s.tail_share >= cfg.tail_share_bar;
            let med_shift_s = (s.median - t.median) / t.iqr;
            let med_hit = med_shift_s.abs() >= cfg.median_shift_bar;
            if !tail_hit && !med_hit {
                continue;
            }
            let form = if tail_hit { "tail" } else { "median" };
            let c = num_stats(&bits_confirm, t, n_confirm);
            if c.values.len() < 20 {
                continue;
            }
            let med_shift_c = (c.median - t.median) / t.iqr;
            let confirmed = if form == "tail" {
                c.tail_share >= cfg.confirm_tail_share_bar
                    && c.tail_share >= cfg.confirm_tail_retention * s.tail_share
            } else {
                med_shift_c.signum() == med_shift_s.signum()
                    && med_shift_c.abs() >= cfg.confirm_median_shift_bar
            };
            // Confirmation-half p-value: slice vs complement.
            let comp_bits: Vec<u64> = bits_confirm
                .iter()
                .zip(half_b.iter())
                .map(|(s, h)| h & !s)
                .collect();
            let comp = num_stats(&comp_bits, t, n_confirm.max(1));
            let p = mann_whitney_p(&c.values, &comp.values);
            let f = num_stats(&bits, t, n_full);
            let effect_norm = if form == "tail" {
                ((f.tail_share - t.tail_share) / 0.45).clamp(0.0, 1.0)
            } else {
                (((f.median - t.median) / t.iqr).abs() / 2.0).clamp(0.0, 1.0)
            };
            candidates.push(Candidate {
                conds: slice.to_vec(),
                target: TargetRef::Num(ti),
                form,
                n_full,
                n_search,
                n_confirm,
                bits: bits.clone(),
                median_full: f.median,
                median_search: s.median,
                median_confirm: c.median,
                tail_full: f.tail_share,
                tail_search: s.tail_share,
                tail_confirm: c.tail_share,
                rate_full: f64::NAN,
                rate_search: f64::NAN,
                rate_confirm: f64::NAN,
                p_confirm: p,
                q_value: 1.0,
                confirmed,
                effect_norm,
            });
        }

        for (ti, t) in cat_targets.iter().enumerate() {
            for (level, &global_rate) in t.rates.iter().enumerate() {
                let level = level as u32;
                if slice_cols.contains(&t.col) {
                    continue;
                }
                if !(0.005..=0.995).contains(&global_rate) {
                    continue;
                }
                let level_count = (global_rate
                    * t.codes.iter().filter(|c| c.is_some()).count() as f64)
                    .round() as usize;
                if level_count < min_support {
                    continue;
                }
                *pairs_evaluated += 1;
                *work += n_full as u64;
                if *work > cfg.work_budget {
                    return false;
                }
                let s = rate_stats(&bits_search, t, level);
                if s.denom < cfg.half_support {
                    continue;
                }
                if s.rate.is_nan() || global_rate <= 0.0 {
                    continue;
                }
                let ratio_s = s.rate / global_rate;
                let diff_s = s.rate - global_rate;
                let expected = s.denom as f64 * global_rate;
                if !((ratio_s >= cfg.rate_ratio_high || ratio_s <= cfg.rate_ratio_low)
                    && diff_s.abs() >= cfg.rate_diff_bar
                    && expected >= cfg.expected_positives)
                {
                    continue;
                }
                let c = rate_stats(&bits_confirm, t, level);
                if c.denom < cfg.half_support || c.rate.is_nan() {
                    continue;
                }
                let ratio_c = c.rate / global_rate;
                let diff_c = c.rate - global_rate;
                let confirmed = if ratio_s >= cfg.rate_ratio_high {
                    ratio_c >= cfg.confirm_rate_ratio_high
                        && diff_c.abs() >= cfg.confirm_rate_diff_bar
                } else if s.rate == 0.0 {
                    ratio_c <= cfg.confirm_rate_zero_ratio
                        && diff_c.abs() >= cfg.confirm_rate_diff_bar
                } else {
                    ratio_c <= cfg.confirm_rate_ratio_low
                        && diff_c.abs() >= cfg.confirm_rate_diff_bar
                };
                let comp_bits: Vec<u64> = bits_confirm
                    .iter()
                    .zip(half_b.iter())
                    .map(|(sb, h)| h & !sb)
                    .collect();
                let comp = rate_stats(&comp_bits, t, level);
                let p = two_proportion_p(c.positives, c.denom, comp.positives, comp.denom);
                let f = rate_stats(&bits, t, level);
                // Absolute effect: the log-ratio and the absolute
                // difference count equally, so a rate of exactly 0
                // against a small global rate does not outrank a
                // large shift on a common level.
                let log_term = if f.rate > 0.0 && global_rate > 0.0 {
                    ((f.rate / global_rate).log2().abs() / 3.0).clamp(0.0, 1.0)
                } else if f.rate == 0.0 {
                    (global_rate / 0.3).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let diff_term = ((f.rate - global_rate).abs() / 0.4).clamp(0.0, 1.0);
                let effect_norm = 0.5 * log_term + 0.5 * diff_term;
                candidates.push(Candidate {
                    conds: slice.to_vec(),
                    target: TargetRef::Cat(ti, level),
                    form: "rate",
                    n_full,
                    n_search,
                    n_confirm,
                    bits: bits.clone(),
                    median_full: f64::NAN,
                    median_search: f64::NAN,
                    median_confirm: f64::NAN,
                    tail_full: f64::NAN,
                    tail_search: f64::NAN,
                    tail_confirm: f64::NAN,
                    rate_full: f.rate,
                    rate_search: s.rate,
                    rate_confirm: c.rate,
                    p_confirm: p,
                    q_value: 1.0,
                    confirmed,
                    effect_norm,
                });
            }
        }
        true
    };

    for slice in &slice_defs {
        if cfg.deadline.exceeded() {
            stopped = true;
            break;
        }
        slices_examined += 1;
        if !process_slice(
            slice,
            &conds,
            &mut candidates,
            &mut work,
            &mut pairs_evaluated,
            &mut slices_with_support,
        ) {
            stopped = true;
            break;
        }
    }

    // ---- depth 3 (deep budget): extend slices that produced a
    // search candidate, as a beam. ----------------------------------
    let mut depth3_defs: Vec<Vec<usize>> = Vec::new();
    if cfg.max_depth >= 3 && !stopped {
        let mut parents: Vec<Vec<usize>> = candidates
            .iter()
            .map(|c| c.conds.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter(|s| s.len() == 2)
            .collect();
        parents.sort();
        for parent in &parents {
            let parent_cols: Vec<usize> = parent.iter().map(|&ci| conds[ci].col).collect();
            for &ci in &level_conds {
                if parent_cols.contains(&conds[ci].col) || parent.contains(&ci) {
                    continue;
                }
                let mut def = parent.clone();
                def.push(ci);
                def.sort_unstable();
                depth3_defs.push(def);
            }
        }
        depth3_defs.sort();
        depth3_defs.dedup();
    }
    let total_slices_all = total_slices + depth3_defs.len();
    for slice in &depth3_defs {
        if cfg.deadline.exceeded() {
            stopped = true;
            break;
        }
        slices_examined += 1;
        if !process_slice(
            slice,
            &conds,
            &mut candidates,
            &mut work,
            &mut pairs_evaluated,
            &mut slices_with_support,
        ) {
            stopped = true;
            break;
        }
    }

    // ---- BH over the confirmation family --------------------------
    let family = candidates.len();
    if family > 0 {
        let pvals: Vec<Option<f64>> = candidates.iter().map(|c| Some(c.p_confirm)).collect();
        let qs = crate::dep::bh_qvalues(&pvals);
        for (c, q) in candidates.iter_mut().zip(qs) {
            c.q_value = q.unwrap_or(1.0);
        }
    }
    let mut kept: Vec<Candidate> = candidates
        .into_iter()
        .filter(|c| c.confirmed && c.q_value <= cfg.bh_alpha)
        .collect();
    let confirmed_count = kept.len();

    // ---- row-set dedupe -------------------------------------------
    // Group by (target, row-set hash): identical row sets are one
    // statement. Keep the shallowest description, then the largest
    // effect, then the lexicographically smallest conditions text.
    let cond_text = |cands: &[usize]| -> String {
        cands
            .iter()
            .map(|&ci| conds[ci].text.clone())
            .collect::<Vec<_>>()
            .join(" and ")
    };
    kept.sort_by(|a, b| {
        a.conds
            .len()
            .cmp(&b.conds.len())
            .then(b.effect_norm.total_cmp(&a.effect_norm))
            .then(cond_text(&a.conds).cmp(&cond_text(&b.conds)))
    });
    let mut seen_sets: BTreeMap<(usize, u64), ()> = BTreeMap::new();
    let mut deduped: Vec<Candidate> = Vec::new();
    for c in kept {
        let tkey = match c.target {
            TargetRef::Num(ti) => (ti + 1) * 1_000_000,
            TargetRef::Cat(ti, level) => (ti + 1) * 1_000_000 + level as usize + 500_000,
        };
        let key = (tkey, bits_hash(&c.bits));
        if seen_sets.insert(key, ()).is_none() {
            deduped.push(c);
        }
    }

    // ---- restatement demotion --------------------------------------
    // A deeper kept slice whose row set is >= 90% the same as a
    // shallower kept slice for the same target restates it.
    let mut demoted: Vec<bool> = vec![false; deduped.len()];
    for i in 0..deduped.len() {
        for j in 0..deduped.len() {
            if deduped[j].conds.len() >= deduped[i].conds.len() {
                continue;
            }
            if deduped[j].target != deduped[i].target {
                continue;
            }
            let inter: usize = deduped[i]
                .bits
                .iter()
                .zip(deduped[j].bits.iter())
                .map(|(a, b)| (a & b).count_ones() as usize)
                .sum();
            let union: usize = deduped[i]
                .bits
                .iter()
                .zip(deduped[j].bits.iter())
                .map(|(a, b)| (a | b).count_ones() as usize)
                .sum();
            if union > 0 && inter as f64 / union as f64 >= 0.9 {
                demoted[i] = true;
                break;
            }
        }
    }

    // ---- ranking by absolute effect, then support ------------------
    let mut ranked: Vec<(Candidate, bool)> = deduped.into_iter().zip(demoted).collect();
    ranked.sort_by(|a, b| {
        b.0.effect_norm
            .total_cmp(&a.0.effect_norm)
            .then(b.0.n_full.cmp(&a.0.n_full))
            .then(cond_text(&a.0.conds).cmp(&cond_text(&b.0.conds)))
    });
    // Statement caps: at most max_per_target statements about one
    // target column, at most max_findings in all.
    let mut per_target: BTreeMap<usize, usize> = BTreeMap::new();
    let mut capped: Vec<(Candidate, bool)> = Vec::new();
    for item in ranked {
        let tkey = match item.0.target {
            TargetRef::Num(ti) => ti + 1,
            TargetRef::Cat(ti, _) => (ti + 1) * 1_000_000,
        };
        let count = per_target.entry(tkey).or_default();
        if *count >= cfg.max_per_target {
            continue;
        }
        *count += 1;
        capped.push(item);
        if capped.len() >= cfg.max_findings {
            break;
        }
    }
    let stated_count = capped.len();
    let ranked = capped;

    // ---- findings ---------------------------------------------------
    let pct = |x: f64| format!("{:.1}", 100.0 * x);
    let mut findings: Vec<Finding> = Vec::new();
    for (cand, is_demoted) in &ranked {
        let cond_texts: Vec<String> = cand
            .conds
            .iter()
            .map(|&ci| conds[ci].text.clone())
            .collect();
        let cond_json: Vec<Value> = cand
            .conds
            .iter()
            .map(|&ci| conds[ci].predicate.clone())
            .collect();
        let slice_desc = cond_texts.join(" and ");
        let mut cond_cols: Vec<String> = cand
            .conds
            .iter()
            .map(|&ci| table.names[conds[ci].col].clone())
            .collect();
        cond_cols.sort();
        cond_cols.dedup();

        let (target_name, form_full, claim, mut metrics) = match cand.target {
            TargetRef::Num(ti) => {
                let t = &num_targets[ti];
                let mut top1_in_slice = 0usize;
                for pos in bits_positions(&cand.bits) {
                    if let Some(v) = t.vals[pos] {
                        if v > t.p99 {
                            top1_in_slice += 1;
                        }
                    }
                }
                let top1_share = if t.top1_count > 0 {
                    top1_in_slice as f64 / t.top1_count as f64
                } else {
                    0.0
                };
                let row_share = cand.n_full as f64 / n_sample as f64;
                let concentration = if row_share > 0.0 {
                    top1_share / row_share
                } else {
                    0.0
                };
                let mut claim = if cand.form == "tail" {
                    format!(
                        "within {} ({} rows), {} has {}% of its rows above the column's 95th percentile ({}) against {}% overall; its median there is {} against {} overall",
                        slice_desc,
                        cand.n_full,
                        t.name,
                        pct(cand.tail_full),
                        crate::fmt_float(t.p95),
                        pct(t.tail_share),
                        crate::fmt_float(cand.median_full),
                        crate::fmt_float(t.median),
                    )
                } else {
                    format!(
                        "within {} ({} rows), {}'s median is {} against {} overall — a shift of {} IQR; {}% of its rows lie above the column's 95th percentile ({}) against {}% overall",
                        slice_desc,
                        cand.n_full,
                        t.name,
                        crate::fmt_float(cand.median_full),
                        crate::fmt_float(t.median),
                        crate::fmt_float((cand.median_full - t.median).abs() / t.iqr),
                        pct(cand.tail_full),
                        crate::fmt_float(t.p95),
                        pct(t.tail_share),
                    )
                };
                if concentration >= 3.0 && top1_in_slice >= 8 {
                    claim.push_str(&format!(
                        "; the slice holds {}% of the column's top-1% values on {}% of the rows",
                        pct(top1_share),
                        pct(row_share),
                    ));
                }
                let metrics = BTreeMap::from([
                    (
                        "form".into(),
                        json!(if cand.form == "tail" {
                            "tail_shift"
                        } else {
                            "median_shift"
                        }),
                    ),
                    ("conditions".into(), json!(cond_json)),
                    ("slice_rows".into(), json!(cand.n_full)),
                    ("slice_rows_search".into(), json!(cand.n_search)),
                    ("slice_rows_confirm".into(), json!(cand.n_confirm)),
                    ("n_train".into(), json!(n_half_a)),
                    ("n_holdout".into(), json!(n_half_b)),
                    ("depth".into(), json!(cand.conds.len())),
                    ("slice_median".into(), json!(cand.median_full)),
                    ("global_median".into(), json!(t.median)),
                    (
                        "median_shift_iqr".into(),
                        json!((cand.median_full - t.median) / t.iqr),
                    ),
                    (
                        "search_median_shift_iqr".into(),
                        json!((cand.median_search - t.median) / t.iqr),
                    ),
                    (
                        "confirm_median_shift_iqr".into(),
                        json!((cand.median_confirm - t.median) / t.iqr),
                    ),
                    ("tail_share".into(), json!(cand.tail_full)),
                    ("tail_share_search".into(), json!(cand.tail_search)),
                    ("tail_share_confirm".into(), json!(cand.tail_confirm)),
                    ("global_tail_share".into(), json!(t.tail_share)),
                    ("tail_threshold".into(), json!(t.p95)),
                    ("top1_share_in_slice".into(), json!(top1_share)),
                    ("row_share".into(), json!(row_share)),
                    ("top1_concentration".into(), json!(concentration)),
                    ("p_value".into(), json!(cand.p_confirm)),
                    ("q_value".into(), json!(cand.q_value)),
                    ("search_candidates".into(), json!(family)),
                    ("candidates_evaluated".into(), json!(pairs_evaluated)),
                ]);
                (
                    t.name.clone(),
                    if cand.form == "tail" {
                        "tail_shift"
                    } else {
                        "median_shift"
                    },
                    claim,
                    metrics,
                )
            }
            TargetRef::Cat(ti, level) => {
                let t = &cat_targets[ti];
                let label = t.labels[level as usize].clone();
                let global_rate = t.rates[level as usize];
                let ratio = if global_rate > 0.0 {
                    cand.rate_full / global_rate
                } else {
                    f64::NAN
                };
                let claim = format!(
                    "within {} ({} rows), {}% of rows have {} = '{}', against {}% overall ({}x)",
                    slice_desc,
                    cand.n_full,
                    pct(cand.rate_full),
                    t.name,
                    label,
                    pct(global_rate),
                    crate::fmt_float(ratio),
                );
                let metrics = BTreeMap::from([
                    ("form".into(), json!("level_share")),
                    ("conditions".into(), json!(cond_json)),
                    ("target_level".into(), json!(label)),
                    ("slice_rows".into(), json!(cand.n_full)),
                    ("slice_rows_search".into(), json!(cand.n_search)),
                    ("slice_rows_confirm".into(), json!(cand.n_confirm)),
                    ("n_train".into(), json!(n_half_a)),
                    ("n_holdout".into(), json!(n_half_b)),
                    ("depth".into(), json!(cand.conds.len())),
                    ("slice_rate".into(), json!(cand.rate_full)),
                    ("search_rate".into(), json!(cand.rate_search)),
                    ("confirm_rate".into(), json!(cand.rate_confirm)),
                    ("global_rate".into(), json!(global_rate)),
                    ("rate_ratio".into(), json!(ratio)),
                    ("p_value".into(), json!(cand.p_confirm)),
                    ("q_value".into(), json!(cand.q_value)),
                    ("search_candidates".into(), json!(family)),
                    ("candidates_evaluated".into(), json!(pairs_evaluated)),
                ]);
                (t.name.clone(), "level_share", claim, metrics)
            }
        };
        if *is_demoted {
            metrics.insert("triviality".into(), json!("restates_parent_slice"));
            metrics.insert("triviality_factor".into(), json!(0.6));
        }
        let mut columns = vec![target_name.clone()];
        columns.extend(cond_cols.iter().cloned());
        let mut params: BTreeMap<String, Value> = BTreeMap::new();
        params.insert("columns".into(), json!(columns));
        params.insert("column".into(), json!(target_name));
        params.insert("conditions".into(), json!(cond_json));
        params.insert("form".into(), json!(form_full));
        let support_factor = 0.8 + 0.2 * ((cand.n_full as f64).log10() / 3.0).min(1.0);
        let score = ((0.5 + 0.45 * cand.effect_norm) * support_factor).clamp(0.0, 0.95);
        findings.push(Finding::new(
            Pattern::SliceShift,
            params,
            claim,
            EvidenceBasis::Estimated,
            score,
            metrics,
            Some(
                "a searched association, not a cause: the slice was selected because the shift is large there, and confirmed on held-out rows; check whether the slice's definition itself explains the shift before acting on it"
                    .into(),
            ),
            Method::new(
                "slice_shifts",
                "slice_search_split_sample",
                "searched conjunction slices with split-sample confirmation and Benjamini-Hochberg control over the confirmation family (issue #272)",
            ),
        ));
    }

    // ---- coverage ----------------------------------------------------
    let mut detail = format!(
        "searched {} slice x target candidates over {} supported slices ({} of {} slice definitions examined, depth <= {}) on a sample of {} rows split {} / {} for search and confirmation; {} cleared the search bars, {} confirmed on the held-out half at Benjamini-Hochberg q <= {}, {} stated (cap {})",
        pairs_evaluated,
        slices_with_support,
        slices_examined,
        total_slices_all,
        cfg.max_depth,
        n_sample,
        n_half_a,
        n_half_b,
        family,
        confirmed_count,
        crate::fmt_float(cfg.bh_alpha),
        stated_count,
        cfg.max_findings,
    );
    if table.n > n_sample {
        detail.push_str(&format!(
            "; the sample is {} of {} rows (row cap {})",
            n_sample, table.n, cfg.row_cap
        ));
    }
    if cat_cols_capped {
        detail.push_str(&format!(
            "; condition columns capped at {}",
            cfg.max_condition_columns
        ));
    }
    if thr_cols_capped {
        detail.push_str(&format!(
            "; threshold columns capped at {}",
            cfg.max_threshold_columns
        ));
    }
    if num_targets_capped {
        detail.push_str(&format!(
            "; numeric targets capped at {}",
            cfg.max_numeric_targets
        ));
    }
    if cat_targets_capped {
        detail.push_str(&format!(
            "; categorical targets capped at {}",
            cfg.max_categorical_targets
        ));
    }
    if stopped {
        detail.push_str(
            "; stopped early (work budget or deadline): the examined slices are a prefix of the deterministic enumeration, so the negative is partial",
        );
    }
    let status = if stopped {
        SliceStatus::Capped(detail)
    } else {
        SliceStatus::Ran(detail)
    };
    SliceOutcome { findings, status }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mann_whitney_separates_shifted_samples() {
        let a: Vec<f64> = (0..50).map(|i| 10.0 + (i % 7) as f64 * 0.1).collect();
        let b: Vec<f64> = (0..50).map(|i| (i % 7) as f64 * 0.1).collect();
        assert!(mann_whitney_p(&a, &b) < 1e-10);
        assert!(mann_whitney_p(&a, &a) > 0.5);
    }

    #[test]
    fn two_proportion_detects_rate_gap() {
        assert!(two_proportion_p(80, 100, 20, 100) < 1e-10);
        assert!(two_proportion_p(50, 100, 50, 100) > 0.9);
    }

    #[test]
    fn quantile_and_median_match_definitions() {
        let v: Vec<f64> = (0..101).map(|i| i as f64).collect();
        assert_eq!(quantile_sorted(&v, 0.5), 50.0);
        assert_eq!(quantile_sorted(&v, 0.25), 25.0);
        assert_eq!(median_sorted(&v), 50.0);
    }
}
