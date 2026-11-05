//! Missing values: how much of a column is absent, and where.
//!
//! A null count on a column card tells a reader nothing about whether
//! the gaps are harmless. This pass states each column's missing rate
//! when it reaches MIN_RATE, then looks for the pattern behind it: a
//! level of a grouping column where the rate is elevated, a threshold in
//! a numeric column, a point on the time axis where values start or stop
//! being recorded, and columns that go missing on the same rows.
//!
//! The search runs on a row sample with a Bonferroni bar over every
//! comparison tried, so a column missing completely at random gets its
//! rate and nothing else. Each reported pattern is recounted over all
//! rows, and the counts in the statement are exact.

use std::collections::BTreeMap;

use onceover_core::{dateutil, TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use rayon::prelude::*;
use serde_json::{json, Value};

use crate::surprises::{code_column, Coded, SurpriseConfig};

/// A column is stated from this missing rate up.
const MIN_RATE: f64 = 0.01;
const MIN_MISSING_ROWS: usize = 5;
/// Columns examined for patterns, highest missing rate first.
const MAX_TARGETS: usize = 64;
/// Numeric columns offered as explanations.
const MAX_NUMERIC_CONDITIONS: usize = 256;
const SEARCH_ROWS: usize = 20_000;
/// A compared group must hold this many rows, and this share of the rows.
const MIN_GROUP_ROWS: usize = 30;
const MIN_GROUP_SHARE: f64 = 0.005;
const MIN_SPLIT_SHARE: f64 = 0.05;
/// Family-wise error across every comparison in one profile.
const FAMILY_ALPHA: f64 = 0.01;
/// An elevated rate must exceed the rate elsewhere by this factor and this
/// difference to be stated: smaller gaps are real on large tables but do
/// not change what a reader does.
const MIN_RATE_RATIO: f64 = 1.5;
const MIN_RATE_DIFFERENCE: f64 = 0.02;
const DECILE_CUTS: usize = 9;
/// Co-missing blocks: shared rows over rows missing in either, and the
/// factor by which the overlap exceeds what independent gaps would give.
const MIN_JACCARD: f64 = 0.9;
const MIN_LIFT: f64 = 2.0;
const MAX_BLOCK_COLUMNS: usize = 128;
const MAX_LEVELS_IN_EVIDENCE: usize = 5;

/// The time axis, as ticks in microseconds aligned with the table's rows.
pub struct MissingnessAxis<'a> {
    pub name: &'a str,
    pub ticks: &'a [Option<i64>],
}

fn is_missing(col: &TypedCol, row: usize) -> bool {
    col.is_null_at(row)
}

/// One side-by-side comparison of missing rates.
#[derive(Clone, Copy)]
struct Compare {
    in_rows: usize,
    in_missing: usize,
    out_rows: usize,
    out_missing: usize,
}

impl Compare {
    fn rates(&self) -> (f64, f64) {
        (
            self.in_missing as f64 / self.in_rows.max(1) as f64,
            self.out_missing as f64 / self.out_rows.max(1) as f64,
        )
    }

    /// Pearson chi-square of the 2x2 table.
    fn chi2(&self) -> f64 {
        let (a, b, c, d) = (
            self.in_missing as f64,
            (self.in_rows - self.in_missing) as f64,
            self.out_missing as f64,
            (self.out_rows - self.out_missing) as f64,
        );
        let n = a + b + c + d;
        let den = (a + b) * (c + d) * (a + c) * (b + d);
        if den <= 0.0 {
            return 0.0;
        }
        n * (a * d - b * c).powi(2) / den
    }

    /// True when the `in` side has the elevated rate, by enough to state.
    fn elevated(&self) -> bool {
        let (hi, lo) = self.rates();
        hi - lo >= MIN_RATE_DIFFERENCE && hi >= MIN_RATE_RATIO * lo
    }
}

/// What a column's missing values follow.
#[derive(Clone)]
enum GapPattern {
    Level {
        column: usize,
        label: String,
        compare: Compare,
    },
    Threshold {
        column: usize,
        cut: f64,
        missing_above: bool,
        compare: Compare,
    },
    Onset {
        cut: i64,
        missing_after: bool,
        compare: Compare,
    },
}

/// Conditions the search compares against, prepared once per profile.
struct Conditions {
    coded: Vec<(usize, Coded)>,
    /// (column, sample values aligned with the sample rows, NaN if missing)
    numeric: Vec<(usize, Vec<f64>, Vec<f64>)>,
    /// Sample ticks and cut points on the time axis.
    axis: Option<(Vec<Option<i64>>, Vec<i64>)>,
    comparisons_per_target: usize,
}

fn decile_cuts(values: &[f64]) -> Vec<f64> {
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| !v.is_nan()).collect();
    if sorted.len() < MIN_GROUP_ROWS {
        return Vec::new();
    }
    sorted.sort_by(f64::total_cmp);
    let mut cuts: Vec<f64> = (1..=DECILE_CUTS)
        .map(|k| sorted[k * sorted.len() / (DECILE_CUTS + 1)])
        .collect();
    cuts.dedup();
    cuts
}

fn split_compare(
    miss: &[bool],
    above: impl Fn(usize) -> Option<bool>,
) -> Option<(Compare, Compare)> {
    let (mut hi_rows, mut hi_miss, mut lo_rows, mut lo_miss) = (0, 0, 0, 0);
    for (r, &m) in miss.iter().enumerate() {
        match above(r) {
            Some(true) => {
                hi_rows += 1;
                hi_miss += usize::from(m);
            }
            Some(false) => {
                lo_rows += 1;
                lo_miss += usize::from(m);
            }
            None => {}
        }
    }
    let total = hi_rows + lo_rows;
    if total == 0
        || (hi_rows as f64) < MIN_SPLIT_SHARE * total as f64
        || (lo_rows as f64) < MIN_SPLIT_SHARE * total as f64
    {
        return None;
    }
    let above_side = Compare {
        in_rows: hi_rows,
        in_missing: hi_miss,
        out_rows: lo_rows,
        out_missing: lo_miss,
    };
    let below_side = Compare {
        in_rows: lo_rows,
        in_missing: lo_miss,
        out_rows: hi_rows,
        out_missing: hi_miss,
    };
    Some((above_side, below_side))
}

/// The strongest pattern for one target on the sample, if any clears `bar`.
fn search_pattern(
    target: usize,
    miss: &[bool],
    sample: &[u32],
    conditions: &Conditions,
    bar: f64,
) -> Option<GapPattern> {
    let mut best: Option<(f64, GapPattern)> = None;
    let mut consider = |compare: Compare, pattern: GapPattern| {
        if !compare.elevated() {
            return;
        }
        let chi2 = compare.chi2();
        if chi2 < bar {
            return;
        }
        if best.as_ref().is_none_or(|(c, _)| chi2 > *c) {
            best = Some((chi2, pattern));
        }
    };

    for (c, coded) in &conditions.coded {
        if *c == target {
            continue;
        }
        let k = coded.labels.len();
        let (mut rows, mut missing) = (vec![0usize; k], vec![0usize; k]);
        for (i, &r) in sample.iter().enumerate() {
            if let Some(level) = coded.codes[r as usize] {
                rows[level as usize] += 1;
                missing[level as usize] += usize::from(miss[i]);
            }
        }
        let (total_rows, total_missing): (usize, usize) = (rows.iter().sum(), missing.iter().sum());
        let floor = MIN_GROUP_ROWS.max((MIN_GROUP_SHARE * total_rows as f64) as usize);
        for level in 0..k {
            if rows[level] < floor || total_rows == rows[level] {
                continue;
            }
            let compare = Compare {
                in_rows: rows[level],
                in_missing: missing[level],
                out_rows: total_rows - rows[level],
                out_missing: total_missing - missing[level],
            };
            consider(
                compare,
                GapPattern::Level {
                    column: *c,
                    label: coded.labels[level].clone(),
                    compare,
                },
            );
        }
    }

    for (c, values, cuts) in &conditions.numeric {
        if *c == target {
            continue;
        }
        for &cut in cuts {
            let split = split_compare(miss, |r| {
                let v = values[r];
                (!v.is_nan()).then_some(v > cut)
            });
            let Some((above, below)) = split else {
                continue;
            };
            consider(
                above,
                GapPattern::Threshold {
                    column: *c,
                    cut,
                    missing_above: true,
                    compare: above,
                },
            );
            consider(
                below,
                GapPattern::Threshold {
                    column: *c,
                    cut,
                    missing_above: false,
                    compare: below,
                },
            );
        }
    }

    if let Some((ticks, cuts)) = &conditions.axis {
        for &cut in cuts {
            let split = split_compare(miss, |r| ticks[r].map(|t| t > cut));
            let Some((after, before)) = split else {
                continue;
            };
            consider(
                after,
                GapPattern::Onset {
                    cut,
                    missing_after: true,
                    compare: after,
                },
            );
            consider(
                before,
                GapPattern::Onset {
                    cut,
                    missing_after: false,
                    compare: before,
                },
            );
        }
    }
    best.map(|(_, p)| p)
}

/// Move a decile cut to the sample split with the largest rate difference.
fn refine_cut(pairs: &mut [(f64, bool)]) -> Option<f64> {
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let n = pairs.len();
    let total_missing = pairs.iter().filter(|p| p.1).count();
    let min_side = (MIN_SPLIT_SHARE * n as f64).ceil() as usize;
    let mut below_missing = 0usize;
    let mut best: Option<(f64, f64)> = None;
    for i in 0..n.saturating_sub(1) {
        below_missing += usize::from(pairs[i].1);
        let below = i + 1;
        if below < min_side || n - below < min_side || pairs[i].0 == pairs[i + 1].0 {
            continue;
        }
        let rate_below = below_missing as f64 / below as f64;
        let rate_above = (total_missing - below_missing) as f64 / (n - below) as f64;
        let difference = (rate_above - rate_below).abs();
        if best.is_none_or(|(d, _)| difference > d) {
            best = Some((difference, pairs[i].0));
        }
    }
    best.map(|(_, cut)| cut)
}

/// Exact counts over every row for the pattern the sample suggested.
fn verify(
    table: &TypedTable,
    target: usize,
    axis: Option<&MissingnessAxis>,
    sample: &[u32],
    miss: &[bool],
    pattern: GapPattern,
) -> Option<GapPattern> {
    let col = &table.cols[target];
    let compare_over = |side: &dyn Fn(usize) -> Option<bool>| -> Compare {
        let mut c = Compare {
            in_rows: 0,
            in_missing: 0,
            out_rows: 0,
            out_missing: 0,
        };
        for r in 0..table.n {
            match side(r) {
                Some(true) => {
                    c.in_rows += 1;
                    c.in_missing += usize::from(is_missing(col, r));
                }
                Some(false) => {
                    c.out_rows += 1;
                    c.out_missing += usize::from(is_missing(col, r));
                }
                None => {}
            }
        }
        c
    };
    let verified = match pattern {
        GapPattern::Level { column, label, .. } => {
            let coded = code_column(table, column, &SurpriseConfig::default())?;
            let level = coded.labels.iter().position(|l| *l == label)? as u32;
            let compare = compare_over(&|r| coded.codes[r].map(|c| c == level));
            GapPattern::Level {
                column,
                label,
                compare,
            }
        }
        GapPattern::Threshold {
            column,
            cut,
            missing_above,
            ..
        } => {
            let source = &table.cols[column];
            let mut pairs: Vec<(f64, bool)> = sample
                .iter()
                .zip(miss)
                .filter_map(|(&r, &m)| source.numeric_at(r as usize).map(|v| (v, m)))
                .collect();
            let cut = refine_cut(&mut pairs).unwrap_or(cut);
            let compare = compare_over(&|r| {
                source
                    .numeric_at(r)
                    .map(|v| if missing_above { v > cut } else { v <= cut })
            });
            GapPattern::Threshold {
                column,
                cut,
                missing_above,
                compare,
            }
        }
        GapPattern::Onset {
            cut, missing_after, ..
        } => {
            let ticks = axis?.ticks;
            let mut pairs: Vec<(f64, bool)> = sample
                .iter()
                .zip(miss)
                .filter_map(|(&r, &m)| ticks[r as usize].map(|t| (t as f64, m)))
                .collect();
            let cut = refine_cut(&mut pairs).map_or(cut, |c| c as i64);
            let compare =
                compare_over(&|r| ticks[r].map(|t| if missing_after { t > cut } else { t <= cut }));
            GapPattern::Onset {
                cut,
                missing_after,
                compare,
            }
        }
    };
    let compare = match &verified {
        GapPattern::Level { compare, .. }
        | GapPattern::Threshold { compare, .. }
        | GapPattern::Onset { compare, .. } => *compare,
    };
    compare.elevated().then_some(verified)
}

fn pct(rate: f64) -> String {
    let p = 100.0 * rate;
    if p >= 10.0 || p == 0.0 {
        format!("{p:.0}%")
    } else {
        format!("{p:.1}%")
    }
}

fn group_int(v: usize) -> String {
    let digits = v.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn tick_label(micros: i64) -> String {
    let day = 86_400_000_000i64;
    if micros.rem_euclid(day) == 0 {
        dateutil::iso_date(micros.div_euclid(day))
    } else {
        dateutil::iso_datetime(micros)
    }
}

fn pattern_clause(
    table: &TypedTable,
    axis: Option<&MissingnessAxis>,
    pattern: &GapPattern,
) -> (String, BTreeMap<String, Value>) {
    let mut metrics = BTreeMap::new();
    let clause = match pattern {
        GapPattern::Level {
            column,
            label,
            compare,
        } => {
            let (inside, outside) = compare.rates();
            let name = &table.names[*column];
            metrics.insert("pattern".to_string(), json!("level"));
            metrics.insert("pattern_column".to_string(), json!(name));
            metrics.insert("pattern_level".to_string(), json!(label));
            format!(
                "highest where {name} = {label}: {} of {} rows, against {} elsewhere",
                pct(inside),
                group_int(compare.in_rows),
                pct(outside)
            )
        }
        GapPattern::Threshold {
            column,
            cut,
            missing_above,
            compare,
        } => {
            let (inside, outside) = compare.rates();
            let name = &table.names[*column];
            let (near, far) = if *missing_above {
                (format!("{name} > {cut}"), format!("{name} <= {cut}"))
            } else {
                (format!("{name} <= {cut}"), format!("{name} > {cut}"))
            };
            metrics.insert("pattern".to_string(), json!("threshold"));
            metrics.insert("pattern_column".to_string(), json!(name));
            metrics.insert("pattern_cut".to_string(), json!(cut));
            metrics.insert("pattern_missing_above".to_string(), json!(missing_above));
            format!(
                "{} of rows where {near}, against {} where {far}",
                pct(inside),
                pct(outside)
            )
        }
        GapPattern::Onset {
            cut,
            missing_after,
            compare,
        } => {
            let (inside, outside) = compare.rates();
            let label = tick_label(*cut);
            let axis_name = axis.map_or("", |a| a.name);
            metrics.insert("pattern".to_string(), json!("onset"));
            metrics.insert("pattern_column".to_string(), json!(axis_name));
            metrics.insert("pattern_cut".to_string(), json!(label));
            metrics.insert("pattern_missing_after".to_string(), json!(missing_after));
            if *missing_after {
                format!(
                    "{} of rows after {label} on {axis_name}, against {} up to it",
                    pct(inside),
                    pct(outside)
                )
            } else {
                format!(
                    "{} of rows up to {label} on {axis_name}, against {} after it",
                    pct(inside),
                    pct(outside)
                )
            }
        }
    };
    let compare = match pattern {
        GapPattern::Level { compare, .. }
        | GapPattern::Threshold { compare, .. }
        | GapPattern::Onset { compare, .. } => compare,
    };
    metrics.insert("pattern_rows".to_string(), json!(compare.in_rows));
    metrics.insert(
        "pattern_missing_rows".to_string(),
        json!(compare.in_missing),
    );
    metrics.insert("other_rows".to_string(), json!(compare.out_rows));
    metrics.insert("other_missing_rows".to_string(), json!(compare.out_missing));
    (clause, metrics)
}

/// Sets of columns whose gaps coincide, by union-find over qualifying pairs.
fn co_missing_blocks(table: &TypedTable, columns: &[usize], counts: &[usize]) -> Vec<Vec<usize>> {
    let words = table.n.div_ceil(64);
    let masks: Vec<Vec<u64>> = columns
        .par_iter()
        .map(|&c| {
            let mut mask = vec![0u64; words];
            for r in 0..table.n {
                if is_missing(&table.cols[c], r) {
                    mask[r / 64] |= 1 << (r % 64);
                }
            }
            mask
        })
        .collect();
    let mut parent: Vec<usize> = (0..columns.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let n = table.n as f64;
    for a in 0..columns.len() {
        for b in a + 1..columns.len() {
            let shared: usize = masks[a]
                .iter()
                .zip(&masks[b])
                .map(|(x, y)| (x & y).count_ones() as usize)
                .sum();
            let either = counts[a] + counts[b] - shared;
            if either == 0 || shared < MIN_MISSING_ROWS {
                continue;
            }
            let lift = shared as f64 * n / (counts[a] as f64 * counts[b] as f64);
            if shared as f64 / either as f64 >= MIN_JACCARD && lift >= MIN_LIFT {
                let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                parent[ra.max(rb)] = ra.min(rb);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..columns.len() {
        let r = root(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    groups.into_values().filter(|g| g.len() >= 2).collect()
}

/// State each column's missing rate and the pattern behind it, if any.
pub fn profile_missingness(
    table: &TypedTable,
    excluded: &[usize],
    axis: Option<MissingnessAxis>,
) -> (Vec<Finding>, String) {
    if table.n == 0 {
        return (Vec::new(), "empty table".into());
    }
    let n = table.n;
    let null_counts: Vec<usize> = table.cols.iter().map(|c| c.null_count()).collect();
    let mut eligible: Vec<usize> = (0..table.cols.len())
        .filter(|&c| {
            let count = null_counts[c];
            count >= MIN_MISSING_ROWS && count < n && count as f64 / n as f64 >= MIN_RATE
        })
        .collect();
    eligible.sort_by_key(|&c| (std::cmp::Reverse(null_counts[c]), c));
    let with_gaps = eligible.len();
    if eligible.is_empty() {
        return (
            Vec::new(),
            format!(
                "{} columns checked; none missing on at least 1% of rows",
                table.cols.len()
            ),
        );
    }

    // ---- conditions ----
    let sample = crate::stride_rows(n, SEARCH_ROWS);
    let surprise_cfg = SurpriseConfig::default();
    let coded: Vec<(usize, Coded)> = (0..table.cols.len())
        .filter(|c| !excluded.contains(c))
        .filter_map(|c| code_column(table, c, &surprise_cfg).map(|coded| (c, coded)))
        .collect();
    let coded_cols: Vec<usize> = coded.iter().map(|(c, _)| *c).collect();
    let numeric: Vec<(usize, Vec<f64>, Vec<f64>)> = (0..table.cols.len())
        .filter(|&c| {
            table.cols[c].is_numeric() && !excluded.contains(&c) && !coded_cols.contains(&c)
        })
        .take(MAX_NUMERIC_CONDITIONS)
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|c| {
            let values: Vec<f64> = sample
                .iter()
                .map(|&r| table.cols[c].numeric_at(r as usize).unwrap_or(f64::NAN))
                .collect();
            let cuts = decile_cuts(&values);
            (c, values, cuts)
        })
        .filter(|(_, _, cuts)| !cuts.is_empty())
        .collect();
    let axis_sample = axis.as_ref().map(|a| {
        let ticks: Vec<Option<i64>> = sample.iter().map(|&r| a.ticks[r as usize]).collect();
        let as_float: Vec<f64> = ticks
            .iter()
            .map(|t| t.map_or(f64::NAN, |v| v as f64))
            .collect();
        let cuts: Vec<i64> = decile_cuts(&as_float).iter().map(|&c| c as i64).collect();
        (ticks, cuts)
    });
    let level_comparisons: usize = coded.iter().map(|(_, c)| c.labels.len()).sum();
    let comparisons_per_target = level_comparisons
        + 2 * numeric.iter().map(|(_, _, cuts)| cuts.len()).sum::<usize>()
        + axis_sample.as_ref().map_or(0, |(_, cuts)| 2 * cuts.len());
    let conditions = Conditions {
        coded,
        numeric,
        axis: axis_sample,
        comparisons_per_target,
    };

    let targets: Vec<usize> = eligible.iter().copied().take(MAX_TARGETS).collect();
    let family = (targets.len() * conditions.comparisons_per_target).max(1) as f64;
    // Chi-square with one degree of freedom, from the normal tail bound
    // P(|Z| > z) <= 2 exp(-z^2 / 2): a conservative Bonferroni bar.
    let bar = 2.0 * (2.0 * family / FAMILY_ALPHA).ln();

    let found: Vec<Option<GapPattern>> = targets
        .par_iter()
        .map(|&t| {
            let miss: Vec<bool> = sample
                .iter()
                .map(|&r| is_missing(&table.cols[t], r as usize))
                .collect();
            let candidate = search_pattern(t, &miss, &sample, &conditions, bar)?;
            verify(table, t, axis.as_ref(), &sample, &miss, candidate)
        })
        .collect();
    let patterns: BTreeMap<usize, GapPattern> = targets
        .iter()
        .zip(found)
        .filter_map(|(&t, p)| p.map(|p| (t, p)))
        .collect();

    // ---- co-missing blocks ----
    let block_columns: Vec<usize> = eligible.iter().copied().take(MAX_BLOCK_COLUMNS).collect();
    let block_counts: Vec<usize> = block_columns.iter().map(|&c| null_counts[c]).collect();
    let blocks = co_missing_blocks(table, &block_columns, &block_counts);
    let mut in_block = vec![false; table.cols.len()];

    let mut findings: Vec<Finding> = Vec::new();
    for block in &blocks {
        let members: Vec<usize> = {
            let mut m: Vec<usize> = block.iter().map(|&i| block_columns[i]).collect();
            m.sort_unstable();
            m
        };
        let (mut all, mut any) = (0usize, 0usize);
        for r in 0..n {
            let missing = members
                .iter()
                .filter(|&&c| is_missing(&table.cols[c], r))
                .count();
            all += usize::from(missing == members.len());
            any += usize::from(missing > 0);
        }
        // The block stands for its members. Their gaps coincide, so a
        // pattern found for one holds for all, and one statement carries it.
        for &c in &members {
            in_block[c] = true;
        }
        let names: Vec<&str> = members.iter().map(|&c| table.names[c].as_str()).collect();
        let rate = all as f64 / n as f64;
        let mut metrics: BTreeMap<String, Value> = BTreeMap::new();
        let mut claim = format!(
            "{} are missing together on {} of {} rows ({}); at least one of them on {} rows",
            names.join(", "),
            group_int(all),
            group_int(n),
            pct(rate),
            group_int(any)
        );
        let shared_pattern = members.iter().find_map(|c| patterns.get(c));
        if let Some(pattern) = shared_pattern {
            let (clause, more) = pattern_clause(table, axis.as_ref(), pattern);
            claim.push_str("; ");
            claim.push_str(&clause);
            metrics.extend(more);
        }
        metrics.extend([
            ("n".to_string(), json!(n)),
            ("missing_together".to_string(), json!(all)),
            ("missing_any".to_string(), json!(any)),
            ("columns".to_string(), json!(names)),
            ("block".to_string(), json!(true)),
        ]);
        metrics
            .entry("pattern".to_string())
            .or_insert_with(|| json!("co_missing"));
        findings.push(Finding::new(
            Pattern::MissingValues,
            BTreeMap::from([("columns".to_string(), json!(names))]),
            claim,
            EvidenceBasis::Measured,
            0.55 + 0.35 * (rate / 0.5).min(1.0).sqrt(),
            metrics,
            Some("these columns probably share a source or a collection step".into()),
            Method::new(
                "missingness",
                "co_missing_blocks",
                "pairwise overlap of null masks over all rows (onceover v0.1)",
            ),
        ));
    }

    for &t in &eligible {
        if in_block[t] {
            continue;
        }
        let name = &table.names[t];
        let count = null_counts[t];
        let rate = count as f64 / n as f64;
        let mut metrics: BTreeMap<String, Value> = BTreeMap::from([
            ("n".to_string(), json!(n)),
            ("missing".to_string(), json!(count)),
            ("missing_fraction".to_string(), json!(rate)),
        ]);
        let mut claim = format!(
            "{name} is missing on {} of {} rows ({})",
            group_int(count),
            group_int(n),
            pct(rate)
        );
        let mut score = 0.4 + 0.35 * (rate / 0.5).min(1.0).sqrt();
        if let Some(pattern) = patterns.get(&t) {
            let (clause, more) = pattern_clause(table, axis.as_ref(), pattern);
            claim.push_str("; ");
            claim.push_str(&clause);
            metrics.extend(more);
            score += 0.15;
            if let GapPattern::Level { column, .. } = pattern {
                if let Some(levels) = level_rates(table, t, *column) {
                    metrics.insert("level_rates".to_string(), levels);
                }
            }
        }
        findings.push(Finding::new(
            Pattern::MissingValues,
            BTreeMap::from([("column".to_string(), json!(name))]),
            claim,
            EvidenceBasis::Measured,
            score,
            metrics,
            Some("decide how to treat these rows before relying on statistics of this column".into()),
            Method::new(
                "missingness",
                "missing_rate_scan",
                "exact null counts; pattern search on a row sample with a Bonferroni bar, recounted over all rows (onceover v0.1)",
            ),
        ));
    }

    let mut detail = format!(
        "{} columns missing on at least 1% of rows; {} with a pattern at a Bonferroni family alpha of {FAMILY_ALPHA} over {} comparisons; {} co-missing blocks",
        with_gaps,
        patterns.len(),
        family as usize,
        blocks.len()
    );
    if with_gaps > MAX_TARGETS {
        detail.push_str(&format!(
            "; patterns searched for the {MAX_TARGETS} columns with the highest missing rates"
        ));
    }
    (findings, detail)
}

/// Exact missing rates per level of the pattern's column, highest first.
fn level_rates(table: &TypedTable, target: usize, column: usize) -> Option<Value> {
    let coded = code_column(table, column, &SurpriseConfig::default())?;
    let k = coded.labels.len();
    let (mut rows, mut missing) = (vec![0usize; k], vec![0usize; k]);
    for r in 0..table.n {
        if let Some(level) = coded.codes[r] {
            rows[level as usize] += 1;
            missing[level as usize] += usize::from(is_missing(&table.cols[target], r));
        }
    }
    let mut order: Vec<usize> = (0..k).filter(|&i| rows[i] > 0).collect();
    order.sort_by(|&a, &b| {
        let (ra, rb) = (
            missing[a] as f64 / rows[a] as f64,
            missing[b] as f64 / rows[b] as f64,
        );
        rb.total_cmp(&ra).then(a.cmp(&b))
    });
    Some(json!(order
        .iter()
        .take(MAX_LEVELS_IN_EVIDENCE)
        .map(|&i| json!({
            "level": coded.labels[i],
            "rows": rows[i],
            "missing": missing[i],
        }))
        .collect::<Vec<_>>()))
}
