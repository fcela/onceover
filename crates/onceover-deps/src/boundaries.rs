//! Boundary mass: values piled at the edge of a numeric column's
//! observed support, and unusual interior mass points in
//! zero-inflated columns. A pile at the maximum whose count dwarfs
//! the counts of the values just below it is the shape a recording
//! cap leaves (top-coded income is the canonical case); a pile at
//! the minimum isolated from the bulk by a wide gap is the shape a
//! floor or a failed sensor leaves. The finding states the pile-up
//! as observed — count, share, the local baseline it is compared
//! against, and the shape of the rest of the column — and names
//! censoring only as one possible cause in the interpretation,
//! never as the claim: a genuine bound of the measured quantity
//! (a percentage that saturates at 100) produces the same shape,
//! and the profile cannot tell the two apart.
//!
//! The discrimination rule is local. A candidate value must occur
//! at least MIN_PILE times, hold at most MAX_SHARE of the evaluated
//! rows, and occur at least DOMINANCE times the median count of
//! its nearest distinct neighbours on the support — so a smooth
//! distribution reaching its bound (neighbouring values carry
//! comparable counts, as in graded 0-100 scores) stays quiet, as do
//! count columns (narrow support, below MIN_DISTINCT), constant
//! columns, and ordinary zero inflation: zero itself is only a
//! candidate when the pile is also isolated from the next value
//! by at least the bulk's interquartile range, and a column whose
//! support is exactly a canonical bounded scale (0-1, 0-100, 1-5,
//! ...) never reports its own scale bounds. Interior mass points
//! are evaluated only in zero-inflated columns (at least half the
//! evaluated rows are zero), against the nonzero values' local
//! counts and with a higher count floor than boundary piles —
//! the context in which a repeated exact amount is a recording
//! fingerprint rather than a mode. An interior value must
//! dominate the median count on each side of it separately: a
//! one-sided comparison fires all along a monotone decline, and
//! an ordinary zero-inflated count column (contacts before a
//! campaign: 2772, 2106, 1142, ... on successive values) is a
//! declining distribution, not a row of mass points.

use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};

use onceover_core::{TypedCol, TypedTable};

/// Minimum evaluated (non-null) rows before any comparison is
/// meaningful, and the minimum pile size.
const MIN_EVALUATED: usize = 100;
const MIN_PILE: usize = 10;
/// Interior mass points carry a higher count floor than boundary
/// piles: without a support edge the claim is weaker, and the
/// floor keeps rounding piles at the edge of the nonzero support
/// (measured: 14 rows at 4.7x on a 2dp-rounded zero-inflated
/// control) below the bar while every measured genuine mass
/// point clears it several times over.
const MIN_INTERIOR_PILE: usize = 20;
/// Minimum other rows before the bulk comparison is meaningful.
const MIN_BULK: usize = 40;
/// A pile holding more than this share of the evaluated rows is
/// the column's mode or its floor, not a boundary artifact.
const MAX_SHARE: f64 = 0.25;
/// The pile must occur at least this many times the median count
/// of its nearest distinct neighbours.
const DOMINANCE: f64 = 4.0;
/// Neighbours consulted for a boundary candidate's baseline (the
/// distinct values just inside the support) and, per side, for an
/// interior candidate's baseline.
const NEIGHBOURS: usize = 10;
/// Minimum distinct values on the evaluated support (and, for
/// interior candidates, on the nonzero support).
const MIN_DISTINCT: usize = 20;
/// Interior candidates are evaluated only when at least this
/// share of the evaluated rows is zero.
const ZERO_INFLATION_SHARE: f64 = 0.5;
/// A zero pile must additionally sit at least this many bulk
/// interquartile ranges from the next distinct value.
const ZERO_GAP_IQR: f64 = 1.0;
/// Findings per column are capped (boundary piles first, then
/// interior mass points by count).
const MAX_PER_COLUMN: usize = 4;
/// Canonical bounded scales: a column whose observed support is
/// exactly one of these (min, max) pairs never reports a pile at
/// either bound — the mass there is the scale working as designed.
const CANONICAL_SCALES: [(f64, f64); 7] = [
    (0.0, 1.0),
    (0.0, 5.0),
    (0.0, 10.0),
    (0.0, 100.0),
    (1.0, 5.0),
    (1.0, 7.0),
    (1.0, 10.0),
];

pub fn boundary_mass(table: &TypedTable) -> Vec<Finding> {
    let mut out = Vec::new();
    for (i, col) in table.cols.iter().enumerate() {
        out.extend(boundary_mass_column(table, i, col));
    }
    out
}

fn boundary_mass_column(table: &TypedTable, i: usize, col: &TypedCol) -> Vec<Finding> {
    // (row, exact key, value) over the non-null entries, exactly as
    // the sentinel scan reads a column.
    let entries: Vec<(usize, u64, f64)> = match col {
        TypedCol::F64 { vals } => vals
            .iter()
            .enumerate()
            .filter_map(|(r, v)| v.filter(|x| x.is_finite()).map(|x| (r, x.to_bits(), *x)))
            .collect(),
        TypedCol::I64 { vals } => vals
            .iter()
            .enumerate()
            .filter_map(|(r, v)| v.map(|x| (r, *x as u64, *x as f64)))
            .collect(),
        TypedCol::U64 { vals } => vals
            .iter()
            .enumerate()
            .filter_map(|(r, v)| v.map(|x| (r, *x, *x as f64)))
            .collect(),
        _ => return Vec::new(),
    };
    let n = entries.len();
    if n < MIN_EVALUATED {
        return Vec::new();
    }
    let mut counts: HashMap<u64, usize> = HashMap::new();
    for &(_, key, _) in &entries {
        *counts.entry(key).or_default() += 1;
    }
    if counts.len() < MIN_DISTINCT {
        return Vec::new();
    }
    // Distinct values ascending, with counts.
    let mut support: Vec<(f64, usize)> = counts
        .iter()
        .map(|(&key, &c)| {
            let v = match col {
                TypedCol::I64 { .. } => (key as i64) as f64,
                TypedCol::U64 { .. } => key as f64,
                _ => f64::from_bits(key),
            };
            (v, c)
        })
        .collect();
    support.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let distinct = support.len();
    let (col_min, _) = support[0];
    let (col_max, _) = support[distinct - 1];
    let zero_key = 0.0f64.to_bits();
    let zero_count = counts.get(&zero_key).copied().unwrap_or(0);
    let zero_share = zero_count as f64 / n as f64;
    let canonical = CANONICAL_SCALES
        .iter()
        .any(|&(lo, hi)| col_min == lo && col_max == hi);
    // All evaluated values, sorted once: bulk quartiles for any
    // candidate are read off this array with the pile's own run
    // skipped, instead of re-sorting the bulk per candidate.
    let mut sorted_vals: Vec<f64> = entries.iter().map(|&(_, _, x)| x).collect();
    sorted_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // Example rows for candidate values only (count >= MIN_PILE):
    // first few and last row index, so the implicated scope is
    // retrievable from the report.
    let candidate_keys: std::collections::HashSet<u64> = counts
        .iter()
        .filter(|(_, &c)| c >= MIN_PILE)
        .map(|(&k, _)| k)
        .collect();
    let mut rows_of: HashMap<u64, (Vec<usize>, usize)> = HashMap::new();
    for &(r, key, _) in &entries {
        if candidate_keys.contains(&key) {
            let e = rows_of.entry(key).or_default();
            if e.0.len() < 10 {
                e.0.push(r);
            }
            e.1 = r;
        }
    }

    let mut emitted: Vec<Finding> = Vec::new();
    // The caller computes the local baseline (median count of the
    // nearest distinct values, over the same window the selection
    // test used) so the evidence reports exactly the comparison
    // that fired.
    let emit = |kind: &str,
                idx: usize,
                baseline: f64,
                window_len: usize,
                below_med: f64,
                above_med: f64|
     -> Option<Finding> {
        let (v, c) = support[idx];
        let key = match col {
            TypedCol::I64 { .. } => (v as i64) as u64,
            TypedCol::U64 { .. } => v as u64,
            _ => v.to_bits(),
        };
        let (examples, last_row) = rows_of.get(&key).cloned().unwrap_or_default();
        // Bulk: every value except the pile's own.
        let (bmed, biqr) = bulk_quartiles(&sorted_vals, v, c);
        // Nearest distinct value inside the support.
        let (neighbour_value, neighbour_count) = match kind {
            "minimum" => (support[idx + 1].0, support[idx + 1].1),
            _ => (support[idx - 1].0, support[idx - 1].1),
        };
        let ratio = c as f64 / baseline.max(1.0);
        let share = c as f64 / n as f64;
        let gap = match kind {
            "maximum" => v - support[idx - 1].0,
            "minimum" => support[idx + 1].0 - v,
            _ => 0.0,
        };
        let first_row = examples.first().copied().unwrap_or(0);
        let (claim, interpretation) = match kind {
            "maximum" => (
                format!(
                    "column {} piles {} values ({:.1}% of {} evaluated rows) at its maximum, {}: the nearest lower value, {}, occurs {} times, and the median count among the {} nearest lower values is {} — the pile is {:.1}x that local baseline; the rest of the column spans {} to {} over {} distinct values and is 0 on {:.1}% of evaluated rows",
                    table.names[i], c, 100.0 * share, n, fmt_num(v),
                    fmt_num(neighbour_value), neighbour_count,
                    window_len, fmt_num(baseline), ratio,
                    fmt_num(col_min), fmt_num(support[idx - 1].0), distinct - 1,
                    100.0 * zero_share,
                ),
                format!(
                    "the pile-up is the observation; a recording cap at {} (top-coding) produces this shape, and so does a genuine upper bound of the measured quantity — the profile cannot distinguish them. Check the source before treating {} as an exact value; if it is a cap, the recorded value is a lower bound on the true values",
                    fmt_num(v), fmt_num(v),
                ),
            ),
            "minimum" => (
                format!(
                    "column {} piles {} values ({:.1}% of {} evaluated rows) at its minimum, {}: the nearest higher value, {}, occurs {} times, and the median count among the {} nearest higher values is {} — the pile is {:.1}x that local baseline; the rest of the column spans {} to {} over {} distinct values and is 0 on {:.1}% of evaluated rows",
                    table.names[i], c, 100.0 * share, n, fmt_num(v),
                    fmt_num(neighbour_value), neighbour_count,
                    window_len, fmt_num(baseline), ratio,
                    fmt_num(support[idx + 1].0), fmt_num(col_max), distinct - 1,
                    100.0 * zero_share,
                ),
                format!(
                    "the pile-up is the observation; a recording floor at {} (a sensor floor, a detection limit, or left-censoring) produces this shape, and so does a genuine lower bound of the measured quantity — the profile cannot distinguish them. Check the source before treating {} as an exact value; whether this value can occur for the measured quantity depends on its units and domain, which the profile does not know",
                    fmt_num(v), fmt_num(v),
                ),
            ),
            _ => (
                format!(
                    "column {} piles {} values ({:.1}% of {} evaluated rows) at the interior value {}: the column is 0 on {:.1}% of evaluated rows, and among its {} nonzero values the median counts of the nearest distinct values are {} on the low side and {} on the high side — the pile is {:.1}x the larger local baseline",
                    table.names[i], c, 100.0 * share, n, fmt_num(v),
                    100.0 * zero_share, n - zero_count,
                    fmt_num(below_med), fmt_num(above_med), ratio,
                ),
                "a repeated exact amount far above the local mass of a zero-inflated column: consistent with a coded, default, or capped amount written by the recording process. The pile-up is the observation; it is not, by itself, evidence of any particular cause".to_string(),
            ),
        };
        let score = (0.5 + 0.08 * ratio.ln()).clamp(0.5, 0.9);
        let mut metrics = BTreeMap::from([
                ("boundary".into(), json!(kind)),
                ("value".into(), json!(v)),
                ("count".into(), json!(c)),
                ("share".into(), json!(share)),
                ("n".into(), json!(n)),
                ("population_rows".into(), json!(table.n)),
                ("null_rows".into(), json!(table.n - n)),
                ("distinct_values".into(), json!(distinct)),
                ("bulk_distinct_values".into(), json!(distinct - 1)),
                (
                    "bulk_min".into(),
                    json!(if kind == "minimum" { support[1].0 } else { col_min }),
                ),
                (
                    "bulk_max".into(),
                    json!(if kind == "maximum" {
                        support[distinct - 2].0
                    } else {
                        col_max
                    }),
                ),
                ("bulk_median".into(), json!(bmed)),
                ("bulk_iqr".into(), json!(biqr)),
                ("neighbour_value".into(), json!(neighbour_value)),
                ("neighbour_count".into(), json!(neighbour_count)),
                ("neighbour_baseline".into(), json!(baseline)),
                ("neighbour_window".into(), json!(window_len)),
                ("dominance_ratio".into(), json!(ratio)),
                ("gap_to_neighbour".into(), json!(gap)),
                ("zero_count".into(), json!(zero_count)),
                ("zero_share".into(), json!(zero_share)),
                ("example_rows".into(), json!(examples)),
                ("first_row".into(), json!(first_row)),
                ("last_row".into(), json!(last_row)),
                ("row_sampled".into(), json!(false)),
                (
                    "method".into(),
                    json!("exact value-count scan: a boundary value occurring >= 10 times (an interior mass point in a zero-inflated column: >= 20 times), <= 25% of evaluated rows, and >= 4x the median count of its nearest distinct neighbours (an interior value must dominate the median count on each side of it separately); a zero pile must also be isolated from the next value by >= 1 bulk IQR; piles at the bounds of a canonical bounded scale (0-1, 0-100, 1-5, ...) are not reported"),
                ),
        ]);
        if kind == "interior" {
            metrics.insert("below_baseline".into(), json!(below_med));
            metrics.insert("above_baseline".into(), json!(above_med));
        }
        Some(Finding::new(
            Pattern::BoundaryMass,
            BTreeMap::from([("column".into(), json!(table.names[i]))]),
            claim,
            EvidenceBasis::Measured,
            score,
            metrics,
            Some(interpretation),
            Method::new(
                "boundaries",
                "boundary_mass_scan",
                "exact value-count scan with a local-median dominance rule (onceover v0.1)",
            ),
        ))
    };

    // Boundary candidates: maximum (last index), then minimum.
    for (kind, idx) in [("maximum", distinct - 1), ("minimum", 0)] {
        if emitted.len() >= MAX_PER_COLUMN {
            break;
        }
        let (v, c) = support[idx];
        if c < MIN_PILE || c as f64 / n as f64 > MAX_SHARE || n - c < MIN_BULK {
            continue;
        }
        if canonical {
            continue; // the pile sits at the scale's own bound
        }
        // Local baseline for the dominance test.
        let window: Vec<usize> = if kind == "maximum" {
            let lo = idx.saturating_sub(NEIGHBOURS);
            support[lo..idx].iter().map(|&(_, c)| c).collect()
        } else {
            let hi = (idx + 1 + NEIGHBOURS).min(distinct);
            support[idx + 1..hi].iter().map(|&(_, c)| c).collect()
        };
        let baseline = median_usize(&window);
        if (c as f64) < DOMINANCE * baseline.max(1.0) {
            continue;
        }
        if v == 0.0 {
            // A zero pile is ordinary zero inflation unless it is
            // also isolated from the rest of the support.
            let (_, biqr) = bulk_quartiles(&sorted_vals, 0.0, c);
            let gap = if kind == "maximum" {
                v - support[idx - 1].0
            } else {
                support[idx + 1].0 - v
            };
            if gap < ZERO_GAP_IQR * biqr && !(biqr == 0.0 && gap > 0.0) {
                continue;
            }
        }
        if let Some(f) = emit(kind, idx, baseline, window.len(), 0.0, 0.0) {
            emitted.push(f);
        }
    }

    // Interior mass points, only under zero inflation: a repeated
    // exact nonzero amount far above the local mass of the nonzero
    // sub-distribution.
    if zero_share >= ZERO_INFLATION_SHARE && emitted.len() < MAX_PER_COLUMN {
        let nonzero: Vec<(usize, f64, usize)> = support
            .iter()
            .enumerate()
            .filter(|&(_, &(v, _))| v != 0.0)
            .map(|(idx, &(v, c))| (idx, v, c))
            .collect();
        if nonzero.len() >= MIN_DISTINCT {
            let mut spikes: Vec<(usize, f64, usize, f64, f64)> = Vec::new();
            for w in 0..nonzero.len() {
                let (idx, _v, c) = nonzero[w];
                if idx == 0 || idx == distinct - 1 {
                    continue; // the support boundary: handled above
                }
                if w == 0 || w == nonzero.len() - 1 {
                    continue; // the nonzero support's own edges are
                              // boundary questions, not interior ones
                }
                if c < MIN_INTERIOR_PILE || c as f64 / n as f64 > MAX_SHARE {
                    continue;
                }
                let lo = w.saturating_sub(NEIGHBOURS);
                let hi = (w + 1 + NEIGHBOURS).min(nonzero.len());
                let below: Vec<usize> = nonzero[lo..w].iter().map(|&(_, _, c)| c).collect();
                let above: Vec<usize> = nonzero[w + 1..hi].iter().map(|&(_, _, c)| c).collect();
                let below_med = median_usize(&below);
                let above_med = median_usize(&above);
                // Two-sided gate: the pile must dominate the local
                // mass on EACH side separately. A one-sided window
                // median also fires all along a monotone decline —
                // an ordinary count distribution (a zero-inflated
                // contacts column declines 2772, 2106, 1142, ...:
                // every low value beats the median of its thinner
                // upper neighbours) — while a genuine mass point
                // stands out from both directions.
                let baseline = below_med.max(above_med);
                if (c as f64) >= DOMINANCE * baseline.max(1.0) {
                    spikes.push((
                        idx,
                        baseline,
                        below.len() + above.len(),
                        below_med,
                        above_med,
                    ));
                }
            }
            // Largest first, deterministic on ties by value.
            spikes.sort_by(|&a, &b| {
                support[b.0]
                    .1
                    .cmp(&support[a.0].1)
                    .then(support[a.0].0.partial_cmp(&support[b.0].0).unwrap())
            });
            for (idx, baseline, window_len, below_med, above_med) in spikes {
                if emitted.len() >= MAX_PER_COLUMN {
                    break;
                }
                if let Some(f) = emit("interior", idx, baseline, window_len, below_med, above_med) {
                    emitted.push(f);
                }
            }
        }
    }
    emitted
}

fn median_usize(counts: &[usize]) -> f64 {
    if counts.is_empty() {
        return 0.0;
    }
    let mut sorted = counts.to_vec();
    sorted.sort_unstable();
    let m = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[m] as f64
    } else {
        (sorted[m - 1] + sorted[m]) as f64 / 2.0
    }
}

/// (median, IQR) of the bulk — every value in `sorted` except
/// the pile's own `pile_count` copies of `pile` — with quartiles
/// by linear interpolation (type-7), as the sentinel fence
/// computes. The pile's copies form one contiguous run in the
/// sorted array, so the bulk's order statistics are the array's
/// own, shifted past the run.
fn bulk_quartiles(sorted: &[f64], pile: f64, pile_count: usize) -> (f64, f64) {
    let start = sorted.partition_point(|&x| x < pile);
    let m = sorted.len() - pile_count;
    if m == 0 {
        return (f64::NAN, f64::NAN);
    }
    let at = |k: usize| -> f64 {
        if k < start {
            sorted[k]
        } else {
            sorted[k + pile_count]
        }
    };
    let q = |p: f64| -> f64 {
        let idx = p * (m - 1) as f64;
        let lo = idx.floor() as usize;
        let hi = (lo + 1).min(m - 1);
        at(lo) + (at(hi) - at(lo)) * (idx - lo as f64)
    };
    let (q1, med, q3) = (q(0.25), q(0.5), q(0.75));
    (med, q3 - q1)
}

fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.4}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::extract;
    use polars::prelude::*;

    fn scan(col: Vec<Option<f64>>) -> Vec<Finding> {
        let df = df! { "x" => col }.unwrap();
        let table = extract(&df);
        boundary_mass(&table)
    }

    #[test]
    fn top_coded_maximum_fires_with_exact_count() {
        // 2,000 bulk values spread thin, 100 piled at the cap.
        let mut vals: Vec<Option<f64>> = (0..2000)
            .map(|k| Some(100.0 + (k % 500) as f64 * 0.37))
            .collect();
        vals.extend((0..100).map(|_| Some(99999.0)));
        let findings = scan(vals);
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert!(f.claim.contains("piles 100 values"), "{}", f.claim);
        assert!(f.claim.contains("at its maximum, 99999"), "{}", f.claim);
        let m = &f.evidence[0].metrics;
        assert_eq!(m["count"], json!(100));
        assert_eq!(m["value"], json!(99999.0));
    }

    #[test]
    fn smooth_scores_reaching_100_stay_quiet() {
        // Integer scores 0..=100 with a smooth mound: neighbouring
        // values carry comparable counts.
        let mut vals: Vec<Option<f64>> = Vec::new();
        for score in 0..=100i32 {
            let count = (200 - (score - 70).abs().max(0)) as usize;
            vals.extend((0..count).map(|_| Some(score as f64)));
        }
        let findings = scan(vals);
        assert!(
            findings.is_empty(),
            "{:?}",
            findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ordinary_zero_inflation_stays_quiet() {
        // 60% zeros, the rest continuous positives approaching 0.
        let mut vals: Vec<Option<f64>> = (0..1200).map(|_| Some(0.0)).collect();
        vals.extend((0..800).map(|k| Some(0.01 + (k % 400) as f64 * 0.613)));
        let findings = scan(vals);
        assert!(findings.is_empty());
    }

    #[test]
    fn constant_and_narrow_columns_stay_quiet() {
        let findings = scan((0..500).map(|_| Some(7.0)).collect());
        assert!(findings.is_empty());
        let findings = scan((0..500).map(|k| Some((k % 6) as f64)).collect());
        assert!(findings.is_empty());
    }
}
