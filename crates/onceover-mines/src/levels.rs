//! Per-level contrasts: one level of a categorical column
//! against the pooled rest on a numeric measure.
//!
//! The mixed-pair screen tests level *means* and states an
//! association. It is blind to the failure mode pooled data
//! actually suffers from: one source, site, or instrument that
//! is off in scale or shape while its mean is unremarkable.
//! This pass is the automatic scan for that shape. For each
//! low-cardinality categorical column (at most 25 levels) and
//! each numeric measure, every level with enough rows is
//! compared with the other levels. Location: the level's median
//! against the median of all other rows pooled, stated when the
//! shift clears half the pooled IQR. Scale: the level's IQR
//! against the typical other level's IQR (the median of the other
//! levels' IQRs), stated when the ratio clears 1.5 or its
//! reciprocal; for a measure with no negative values the ratio is
//! of relative spread (IQR over median), and no scale contrast is
//! stated when either IQR is zero. When several levels of one
//! grouping differ on one measure, the pair is stated once, as the
//! measure's profile over the grouping, with each level in the
//! evidence. A level whose values sit at a near-constant multiple
//! of the rest's distribution gets its own statement, because the
//! quotient is the actionable fact (a unit error: cm against inch,
//! cents against dollars).
//!
//! Location and scale are each backed by a rank-sum test
//! (level against rest; scale ranks the absolute deviations
//! from each side's own median), and every test performed —
//! the family is enumerable: levels x measures x two forms —
//! is Benjamini-Hochberg adjusted together; a contrast is
//! stated only when its effect clears the bar *and* its
//! q-value clears 0.05. The constant-multiple form is an
//! exactness check, not a test, and stands outside the family
//! the way the arithmetic identities do.
//!
//! The grouping can also be a calendar part of the table's
//! date axis — day of week, month of year, hour of day —
//! derived by the pipeline from the axis it identified. A
//! calendar level is eligible only when its rows span at
//! least two distinct cycles (two different weeks for a
//! weekday, two different years for a month, two different
//! days for an hour), so a single unusual week cannot pose
//! as a weekday effect.
//!
//! Bars: a level and its rest each need at least 30 rows with
//! both values known; at most 400 grouping x measure pairs
//! are examined (the count is stated when the cap binds) and
//! at most 20 contrasts are reported, ranked by effect size.

use std::collections::BTreeMap;

use crate::dep::{iqr_sorted, median_sorted, quantile_sorted, rank_sum_p};
use onceover_core::{dateutil, TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use serde_json::{json, Value};

/// Most levels a grouping column may have to enter the scan.
const MAX_LEVELS: usize = 25;
/// Smallest number of complete rows on either side of one
/// level's contrast.
const MIN_SIDE_VALUES: usize = 30;
/// Location bar: the level's median shift, in pooled IQRs.
const LOCATION_BAR: f64 = 0.5;
/// Scale bar: the level's IQR against the rest's.
const SCALE_RATIO_BAR: f64 = 1.5;
/// Benjamini-Hochberg q-value bar over the whole test family.
const BH_ALPHA: f64 = 0.05;
/// Constant-multiple form: the quotient must sit at least
/// this far from 1 (log-symmetric).
const MULTIPLE_MIN: f64 = 1.25;
/// Constant-multiple form: the spread of the nine decile
/// quotients around their median, relative to the quotient.
/// Tight on purpose: a pure location shift on a distribution
/// bounded away from zero also concentrates its quotients
/// somewhat, and a false multiple asserts a mechanism.
const QUOTIENT_CV_MAX: f64 = 0.025;
/// Constant-multiple form: the largest single decile
/// quotient's relative deviation from the median quotient.
const QUOTIENT_DEV_MAX: f64 = 0.06;
/// Grouping x measure pairs examined per profile.
const MAX_PAIRS: usize = 400;
/// Findings emitted by this pass per profile.
const MAX_FINDINGS: usize = 20;
/// Calendar repetition gate: a part level's rows must span
/// at least this many distinct cycles.
const MIN_CYCLES: usize = 2;

const MICROS_PER_DAY: i64 = 86_400_000_000;

const METHOD_TEXT: &str = "per-level quantile contrast: each level of a low-cardinality categorical (at most 25 levels) against the pooled rest on each numeric measure; a level and its rest each need at least 30 complete rows; location is stated when the median shift clears 0.5 pooled IQRs, scale when the level's IQR against the median IQR of the other levels clears 1.5 or 2/3, each backed by a rank-sum test (scale ranks absolute deviations from each side's own median) with Benjamini-Hochberg q <= 0.05 over the whole enumerable family of location and scale tests; a level whose nine decile quotients against the rest concentrate at one quotient at least 1.25x from 1 (quotient CV at most 0.025, largest decile deviation at most 0.06) and whose IQR ratio moves with the quotient (same direction, at least half its log size) is stated as a constant multiple instead; calendar parts of the date axis (day of week, month of year, hour of day) enter as groupings whose levels must span at least two distinct cycles; a contrast that falls under its bar once the pair's most extreme level on that form is set aside — the shift recomputed against the rest without the most-shifted level, the IQR ratio against the typical level without the widest or narrowest — is not stated (the primary level explains it), and a grouping with exactly two levels states its contrast once, keeping the larger effect; at most 400 pairs examined and 20 findings reported, ranked by effect size";

/// The calendar axis the pipeline identified, when it did:
/// the axis column's name and its per-row ticks in
/// microseconds since the epoch.
#[derive(Default)]
pub struct LevelContrastInput<'a> {
    pub axis_name: Option<&'a str>,
    pub axis_ticks: Option<&'a [Option<i64>]>,
    /// Measures the pass must not state contrasts on: the
    /// designated target, whose per-level contrasts the outcome
    /// screen owns when a target is designated (issue #310), so
    /// the same pair is not stated twice in two vocabularies.
    pub exclude_measures: &'a [&'a str],
    /// Columns that are neither a grouping nor a measure: the
    /// scaffolding (row numbers, identifiers, the axis) and the
    /// redundant side of a one-to-one recoding.
    pub exclude_columns: &'a [usize],
    /// Columns that are not measures (flags, dimensions, columns
    /// the axis determines); they may still group.
    pub non_measures: &'a [usize],
    /// Columns the axis determines. Such a column is dropped as a
    /// grouping when it partitions the rows exactly as a calendar
    /// part the pass derives from the axis (month of year, day of
    /// week, hour of day): the two groupings are one, stated once.
    pub axis_derived: &'a [usize],
}

/// The pass's result: findings plus the coverage detail.
pub struct LevelsOutcome {
    pub findings: Vec<Finding>,
    pub detail: String,
}

/// One grouping of the rows: per-row level codes and labels.
pub(crate) struct Grouping {
    /// The column named in the subject (the axis column for
    /// calendar parts).
    pub(crate) column: String,
    /// How the grouping reads in claims: the column name, or
    /// "day of week of {axis}" for a calendar part.
    pub(crate) describe: String,
    pub(crate) codes: Vec<Option<u32>>,
    pub(crate) labels: Vec<String>,
    pub(crate) calendar_part: Option<&'static str>,
    /// Calendar only: per level, how many distinct cycles its
    /// rows span.
    pub(crate) cycles: Vec<usize>,
}

/// Level codes for one column, when it is a low-cardinality
/// categorical: strings, booleans, and integer columns with
/// 2..=MAX_LEVELS distinct non-null values.
///
/// Bails out immediately on the `(MAX_LEVELS + 1)`-th distinct value
/// without allocating per-row Strings.
pub(crate) fn code_levels(
    table: &TypedTable,
    ci: usize,
) -> Option<(Vec<Option<u32>>, Vec<String>)> {
    let n = table.n;
    match &table.cols[ci] {
        TypedCol::Str { vals } => {
            let mut index: FxHashMap<&str, u32> = FxHashMap::default();
            let mut labels: Vec<String> = Vec::new();
            let mut codes = Vec::with_capacity(n);
            for &v in vals {
                match v {
                    None => codes.push(None),
                    Some(s) => {
                        let code = if let Some(&c) = index.get(s) {
                            c
                        } else {
                            if labels.len() >= MAX_LEVELS {
                                return None;
                            }
                            let c = labels.len() as u32;
                            index.insert(s, c);
                            labels.push(s.to_string());
                            c
                        };
                        codes.push(Some(code));
                    }
                }
            }
            (labels.len() >= 2).then_some((codes, labels))
        }
        TypedCol::StrOwned { vals } => {
            let mut index: FxHashMap<&str, u32> = FxHashMap::default();
            let mut labels: Vec<String> = Vec::new();
            let mut codes = Vec::with_capacity(n);
            for v in vals {
                match v.as_deref() {
                    None => codes.push(None),
                    Some(s) => {
                        let code = if let Some(&c) = index.get(s) {
                            c
                        } else {
                            if labels.len() >= MAX_LEVELS {
                                return None;
                            }
                            let c = labels.len() as u32;
                            index.insert(s, c);
                            labels.push(s.to_string());
                            c
                        };
                        codes.push(Some(code));
                    }
                }
            }
            (labels.len() >= 2).then_some((codes, labels))
        }
        TypedCol::Bool { vals } => {
            let mut labels: Vec<String> = Vec::new();
            let mut code_false: Option<u32> = None;
            let mut code_true: Option<u32> = None;
            let mut codes = Vec::with_capacity(n);
            for &v in vals {
                match v {
                    None => codes.push(None),
                    Some(false) => {
                        let c = *code_false.get_or_insert_with(|| {
                            let c = labels.len() as u32;
                            labels.push("false".into());
                            c
                        });
                        codes.push(Some(c));
                    }
                    Some(true) => {
                        let c = *code_true.get_or_insert_with(|| {
                            let c = labels.len() as u32;
                            labels.push("true".into());
                            c
                        });
                        codes.push(Some(c));
                    }
                }
            }
            (labels.len() >= 2).then_some((codes, labels))
        }
        TypedCol::I64 { vals } => {
            let mut index: FxHashMap<i64, u32> = FxHashMap::default();
            let mut labels: Vec<String> = Vec::new();
            let mut codes = Vec::with_capacity(n);
            for v in vals.iter() {
                match v.copied() {
                    None => codes.push(None),
                    Some(x) => {
                        let code = if let Some(&c) = index.get(&x) {
                            c
                        } else {
                            if labels.len() >= MAX_LEVELS {
                                return None;
                            }
                            let c = labels.len() as u32;
                            index.insert(x, c);
                            labels.push(crate::fmt_float(x as f64));
                            c
                        };
                        codes.push(Some(code));
                    }
                }
            }
            (labels.len() >= 2).then_some((codes, labels))
        }
        TypedCol::U64 { vals } => {
            let mut index: FxHashMap<u64, u32> = FxHashMap::default();
            let mut labels: Vec<String> = Vec::new();
            let mut codes = Vec::with_capacity(n);
            for v in vals.iter() {
                match v.copied() {
                    None => codes.push(None),
                    Some(x) => {
                        let code = if let Some(&c) = index.get(&x) {
                            c
                        } else {
                            if labels.len() >= MAX_LEVELS {
                                return None;
                            }
                            let c = labels.len() as u32;
                            index.insert(x, c);
                            labels.push(crate::fmt_float(x as f64));
                            c
                        };
                        codes.push(Some(code));
                    }
                }
            }
            (labels.len() >= 2).then_some((codes, labels))
        }
        _ => None,
    }
}

/// Calendar-part groupings derived from the axis ticks.
/// One calendar part: its name, its level labels, and the
/// function mapping a day (days since epoch, or an hour
/// index) to (level code, cycle key).
type CalendarPartSpec = (&'static str, Vec<&'static str>, fn(i64) -> (u32, i64));

pub(crate) fn calendar_groupings(input: &LevelContrastInput) -> Vec<Grouping> {
    let (Some(axis), Some(ticks)) = (input.axis_name, input.axis_ticks) else {
        return Vec::new();
    };
    let days: Vec<Option<i64>> = ticks
        .iter()
        .map(|t| t.map(|us| us.div_euclid(MICROS_PER_DAY)))
        .collect();
    let parts: [CalendarPartSpec; 3] = [
        (
            "day_of_week",
            vec![
                "Sunday",
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
            ],
            |day| (dateutil::weekday(day), day.div_euclid(7)),
        ),
        (
            "month_of_year",
            vec![
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ],
            |day| {
                let (y, m, _) = dateutil::civil_from_days(day);
                (m - 1, y)
            },
        ),
        (
            "hour_of_day",
            vec![
                "00", "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12", "13",
                "14", "15", "16", "17", "18", "19", "20", "21", "22", "23",
            ],
            |day| (0, day),
        ),
    ];
    let mut out = Vec::new();
    for (part, names, key) in parts {
        // Hour needs the ticks themselves, not just the day.
        let codes: Vec<Option<u32>> = if part == "hour_of_day" {
            if ticks
                .iter()
                .flatten()
                .all(|us| us.rem_euclid(MICROS_PER_DAY) == 0)
            {
                continue;
            }
            ticks
                .iter()
                .map(|t| t.map(|us| (us.rem_euclid(MICROS_PER_DAY) / 3_600_000_000) as u32))
                .collect()
        } else {
            days.iter().map(|d| d.map(|day| key(day).0)).collect()
        };
        let present: Vec<u32> = {
            let mut v: Vec<u32> = codes.iter().flatten().copied().collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        if !(2..=MAX_LEVELS).contains(&present.len()) {
            continue;
        }
        // Remap the present codes to dense codes in sorted
        // order, so labels and cycles align.
        let remap: BTreeMap<u32, u32> = present
            .iter()
            .enumerate()
            .map(|(i, c)| (*c, i as u32))
            .collect();
        let codes: Vec<Option<u32>> = codes.iter().map(|c| c.map(|x| remap[&x])).collect();
        let labels: Vec<String> = present
            .iter()
            .map(|c| names[*c as usize].to_string())
            .collect();
        let mut cycle_sets: Vec<std::collections::BTreeSet<i64>> =
            (0..present.len()).map(|_| Default::default()).collect();
        for (row, code) in codes.iter().enumerate() {
            if let (Some(c), Some(day)) = (code, days[row]) {
                let cycle = if part == "hour_of_day" {
                    day
                } else {
                    key(day).1
                };
                cycle_sets[*c as usize].insert(cycle);
            }
        }
        let cycles: Vec<usize> = cycle_sets.iter().map(|s| s.len()).collect();
        let describe = match part {
            "day_of_week" => format!("day of week of {axis}"),
            "month_of_year" => format!("month of year of {axis}"),
            _ => format!("hour of day of {axis}"),
        };
        out.push(Grouping {
            column: axis.to_string(),
            describe,
            codes,
            labels,
            calendar_part: Some(part),
            cycles,
        });
    }
    out
}

/// The spread ratio a scale contrast is judged on, or None when
/// it is undefined. Either IQR at zero (a zero-inflated measure,
/// a level with no spread) makes the ratio 0 or infinite, which
/// says the two sides are discrete, not that one varies more. For a
/// measure with no negative values the ratio is of relative spread
/// (IQR over median): a count's spread grows with its level, so a
/// low-count level is narrower in absolute terms by construction.
fn scale_effect_ratio(c: &Candidate) -> Option<f64> {
    if c.level_iqr <= 0.0 || c.rest_iqr <= 0.0 {
        return None;
    }
    if c.nonneg {
        if c.level_median <= 0.0 || c.rest_median <= 0.0 {
            return None;
        }
        return Some((c.level_iqr / c.level_median) / (c.rest_iqr / c.rest_median));
    }
    Some(c.ratio)
}

/// Whether two groupings put the rows into the same classes:
/// a one-to-one map between their codes on the rows where both
/// are known, and the same rows known.
fn same_partition(a: &[Option<u32>], b: &[Option<u32>]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut ab: BTreeMap<u32, u32> = BTreeMap::new();
    let mut ba: BTreeMap<u32, u32> = BTreeMap::new();
    for (x, y) in a.iter().zip(b) {
        match (x, y) {
            (Some(x), Some(y)) => {
                if *ab.entry(*x).or_insert(*y) != *y || *ba.entry(*y).or_insert(*x) != *x {
                    return false;
                }
            }
            (None, None) => {}
            _ => return false,
        }
    }
    !ab.is_empty()
}

/// Whether a candidate's constant-multiple evidence holds as
/// a rescaling: the decile quotients concentrate at one
/// quotient AND the level's IQR ratio moves with that
/// quotient — same direction, at least half its size on the
/// log scale. A pure location shift on a distribution bounded
/// away from zero also concentrates its decile quotients, but
/// it leaves the spread unchanged; a rescaling cannot.
fn multiple_holds(c: &Candidate) -> bool {
    match c.multiple {
        Some((quotient, _)) => {
            c.ratio.is_finite()
                && c.ratio > 0.0
                && (c.ratio.ln() * quotient.ln() > 0.0)
                && c.ratio.ln().abs() >= 0.5 * quotient.ln().abs()
        }
        None => false,
    }
}

/// The rest of a pair's values with one level removed: a
/// merge-difference of the pooled sorted slice and the
/// level's sorted slice (both are the same multiset union).
fn rest_of(pooled: &[f64], level: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(pooled.len() - level.len());
    let mut j = 0usize;
    for &v in pooled {
        if j < level.len() && level[j] == v {
            j += 1;
        } else {
            out.push(v);
        }
    }
    out
}

/// One tested level of one pair, before the BH decision.
struct Candidate {
    grouping: usize,
    measure: usize,
    level: usize,
    level_label: String,
    n_level: usize,
    n_rest: usize,
    level_median: f64,
    rest_median: f64,
    shift: f64,
    std_shift: f64,
    level_iqr: f64,
    rest_iqr: f64,
    ratio: f64,
    p_location: f64,
    p_scale: f64,
    q_location: f64,
    q_scale: f64,
    /// The constant-multiple quotient when the decile
    /// quotients concentrate; takes precedence over the
    /// location/scale statement for this level.
    multiple: Option<(f64, f64)>, // (quotient, largest decile deviation)
    /// Largest |standardized shift| among the pair's other
    /// tested levels (for the "no other level" sentence).
    max_other_std_shift: f64,
    /// This level's standardized shift recomputed against
    /// the rest without the pair's most-shifted level; None
    /// for that level itself, and when the reduced rest is
    /// too small to recompute against.
    std_shift_without_primary: Option<f64>,
    /// This level's IQR ratio recomputed against the
    /// typical IQR of the other levels without the pair's
    /// most extreme level on the scale form; None for that
    /// level itself, and when no other level remains.
    ratio_without_primary: Option<f64>,
    /// The measure takes no negative value on the pair's rows.
    nonneg: bool,
}

/// A stated level contrast: its effect, its finding, and its
/// candidate index.
type StatedLevel = (f64, Finding, usize);

/// The per-level contrast pass. See the module documentation
/// for the reporting bars.
pub fn profile_level_contrasts(table: &TypedTable, input: &LevelContrastInput) -> LevelsOutcome {
    // ---------- groupings ----------
    let mut groupings: Vec<Grouping> = Vec::new();
    let mut grouping_cols: Vec<usize> = Vec::new();
    for ci in 0..table.cols.len() {
        if input.exclude_columns.contains(&ci) {
            continue;
        }
        if let Some((codes, labels)) = code_levels(table, ci) {
            groupings.push(Grouping {
                column: table.names[ci].clone(),
                describe: table.names[ci].clone(),
                codes,
                labels,
                calendar_part: None,
                cycles: Vec::new(),
            });
            grouping_cols.push(ci);
        }
    }
    let calendar = calendar_groupings(input);
    // A column the axis determines that partitions the rows as a
    // derived calendar part is the same grouping: keep the
    // calendar part, which names its source.
    let mut merged_into_calendar: Vec<String> = Vec::new();
    let mut kept: Vec<Grouping> = Vec::new();
    for (g, ci) in groupings.into_iter().zip(grouping_cols) {
        if input.axis_derived.contains(&ci)
            && calendar.iter().any(|c| same_partition(&g.codes, &c.codes))
        {
            merged_into_calendar.push(g.column);
        } else {
            kept.push(g);
        }
    }
    let mut groupings = kept;
    let n_column_groupings = groupings.len();
    groupings.extend(calendar);

    // ---------- measures ----------
    let measures: Vec<usize> = (0..table.cols.len())
        .filter(|&ci| {
            table.cols[ci].is_numeric()
                && !input.exclude_measures.contains(&table.names[ci].as_str())
                && !input.exclude_columns.contains(&ci)
                && !input.non_measures.contains(&ci)
        })
        .collect();

    let mut pair_tasks: Vec<(usize, usize)> = Vec::new();
    let mut pairs_capped = false;
    'pairs: for (gi, g) in groupings.iter().enumerate() {
        for &mi in &measures {
            if table.names[mi] == g.column && g.calendar_part.is_none() {
                continue;
            }
            if pair_tasks.len() >= MAX_PAIRS {
                pairs_capped = true;
                break 'pairs;
            }
            pair_tasks.push((gi, mi));
        }
    }
    let pairs_examined = pair_tasks.len();

    type PairShift = (usize, usize, usize, f64);
    let pair_results: Vec<(Vec<Candidate>, Vec<PairShift>)> = pair_tasks
        .par_iter()
        .map(|&(gi, mi)| {
            let g = &groupings[gi];
            let col = &table.cols[mi];
            let mut sorted_buckets: Vec<Vec<f64>> = vec![Vec::new(); g.labels.len()];
            for (row, &code) in g.codes.iter().enumerate().take(table.n) {
                if let (Some(code), Some(v)) = (code, col.numeric_at(row)) {
                    if v.is_finite() {
                        sorted_buckets[code as usize].push(v);
                    }
                }
            }
            let total: usize = sorted_buckets.iter().map(|b| b.len()).sum();
            if total < 2 * MIN_SIDE_VALUES {
                return (Vec::new(), Vec::new());
            }
            let mut pooled_sorted: Vec<f64> = Vec::with_capacity(total);
            for b in &mut sorted_buckets {
                pooled_sorted.extend_from_slice(b);
                b.sort_by(|a, b| a.total_cmp(b));
            }
            pooled_sorted.sort_by(|a, b| a.total_cmp(b));
            let distinct_count = pooled_sorted.windows(2).filter(|w| w[0] != w[1]).count() + 1;
            if distinct_count < 3 {
                return (Vec::new(), Vec::new());
            }
            let spread = {
                let s = iqr_sorted(&pooled_sorted);
                if s > 0.0 {
                    s
                } else {
                    let s2 =
                        quantile_sorted(&pooled_sorted, 0.9) - quantile_sorted(&pooled_sorted, 0.1);
                    if s2 > 0.0 {
                        s2
                    } else {
                        pooled_sorted[pooled_sorted.len() - 1] - pooled_sorted[0]
                    }
                }
            };
            if spread <= 0.0 {
                return (Vec::new(), Vec::new());
            }
            // Per-level sorted values, medians, IQRs, and
            // deviations from each level's own median, computed
            // once per pair. The scale contrast compares a
            // level's IQR with the typical other level's IQR
            // (the median of the other levels' IQRs) and ranks
            // deviations from each row's own level median:
            // the pooled rest is a mixture, and a mixture's IQR
            // and median-centred deviations inflate whenever
            // one level is shifted, which would make every
            // other level look narrow by comparison.
            let level_medians: Vec<f64> = sorted_buckets
                .iter()
                .map(|s| if s.is_empty() { 0.0 } else { median_sorted(s) })
                .collect();
            let level_iqrs: Vec<f64> = sorted_buckets
                .iter()
                .map(|s| if s.len() >= 2 { iqr_sorted(s) } else { 0.0 })
                .collect();
            let mut pooled_dev_sorted: Vec<f64> = Vec::with_capacity(total);
            let dev_buckets: Vec<Vec<f64>> = sorted_buckets
                .iter()
                .enumerate()
                .map(|(j, s)| {
                    let m = level_medians[j];
                    let mut d: Vec<f64> = s.iter().map(|v| (v - m).abs()).collect();
                    pooled_dev_sorted.extend_from_slice(&d);
                    d.sort_by(|a, b| a.total_cmp(b));
                    d
                })
                .collect();
            pooled_dev_sorted.sort_by(|a, b| a.total_cmp(b));

            let mut local_cands: Vec<Candidate> = Vec::new();
            let mut local_shifts: Vec<(usize, usize, usize, f64)> = Vec::new();
            for (li, level_sorted) in sorted_buckets.iter().enumerate() {
                let n_level = level_sorted.len();
                let n_rest = total - n_level;
                if n_level < MIN_SIDE_VALUES || n_rest < MIN_SIDE_VALUES {
                    continue;
                }
                if g.calendar_part.is_some() && g.cycles.get(li).copied().unwrap_or(0) < MIN_CYCLES
                {
                    continue;
                }
                let rest_sorted = rest_of(&pooled_sorted, level_sorted);
                let level_median = level_medians[li];
                let rest_median = median_sorted(&rest_sorted);
                let level_iqr = level_iqrs[li];
                let rest_iqr = {
                    let mut others: Vec<f64> = level_iqrs
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != li && sorted_buckets[*j].len() >= 2)
                        .map(|(_, v)| *v)
                        .collect();
                    if others.is_empty() {
                        iqr_sorted(&rest_sorted)
                    } else {
                        others.sort_by(|a, b| a.total_cmp(b));
                        median_sorted(&others)
                    }
                };
                let shift = level_median - rest_median;
                let std_shift = shift / spread;
                local_shifts.push((gi, mi, li, std_shift));
                let p_location = rank_sum_p(level_sorted, &rest_sorted);
                // Scale: rank-sum over absolute deviations
                // from each row's own level median.
                let level_dev = &dev_buckets[li];
                let rest_dev = rest_of(&pooled_dev_sorted, level_dev);
                let p_scale = rank_sum_p(level_dev, &rest_dev);
                let ratio = if rest_iqr > 0.0 {
                    level_iqr / rest_iqr
                } else if level_iqr > 0.0 {
                    f64::INFINITY
                } else {
                    1.0
                };
                // Constant-multiple check: the rest's deciles
                // must sit on one side of zero for quotients
                // to mean anything.
                let multiple = {
                    let deciles: [f64; 9] = std::array::from_fn(|k| {
                        quantile_sorted(&rest_sorted, (k + 1) as f64 / 10.0)
                    });
                    let one_sided =
                        deciles.iter().all(|d| *d > 0.0) || deciles.iter().all(|d| *d < 0.0);
                    if one_sided && rest_median != 0.0 {
                        let quotients: [f64; 9] = std::array::from_fn(|k| {
                            quantile_sorted(level_sorted, (k + 1) as f64 / 10.0) / deciles[k]
                        });
                        let mut qs = quotients;
                        qs.sort_by(|a, b| a.total_cmp(b));
                        let k_hat = median_sorted(&qs);
                        if k_hat > 0.0 && (k_hat.ln().abs() >= MULTIPLE_MIN.ln()) {
                            let max_dev = quotients
                                .iter()
                                .map(|q| (q - k_hat).abs() / k_hat)
                                .fold(0.0f64, f64::max);
                            let cv = iqr_sorted(&qs) / k_hat;
                            if max_dev <= QUOTIENT_DEV_MAX && cv <= QUOTIENT_CV_MAX {
                                Some((k_hat, max_dev))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                local_cands.push(Candidate {
                    grouping: gi,
                    measure: mi,
                    level: li,
                    level_label: g.labels[li].clone(),
                    n_level,
                    n_rest,
                    level_median,
                    rest_median,
                    shift,
                    std_shift,
                    level_iqr,
                    rest_iqr,
                    ratio,
                    p_location,
                    p_scale,
                    q_location: 1.0,
                    q_scale: 1.0,
                    multiple,
                    max_other_std_shift: 0.0,
                    std_shift_without_primary: None,
                    ratio_without_primary: None,
                    nonneg: pooled_sorted[0] >= 0.0,
                });
            }
            // The pair's primary level (largest |standardized
            // shift|): every other level's shift is also
            // recomputed against the rest without the primary,
            // so a contrast the primary explains can be
            // recognized at decision time.
            if !local_cands.is_empty() {
                let primary = (0..local_cands.len())
                    .max_by(|&a, &b| {
                        local_cands[a]
                            .std_shift
                            .abs()
                            .total_cmp(&local_cands[b].std_shift.abs())
                    })
                    .unwrap();
                let primary_level = local_cands[primary].level;
                let without_primary = rest_of(&pooled_sorted, &sorted_buckets[primary_level]);
                for (slot, cand) in local_cands.iter_mut().enumerate() {
                    if slot == primary {
                        continue;
                    }
                    let li = cand.level;
                    let reduced = rest_of(&without_primary, &sorted_buckets[li]);
                    if reduced.len() >= MIN_SIDE_VALUES {
                        let s = (level_medians[li] - median_sorted(&reduced)) / spread;
                        cand.std_shift_without_primary = Some(s);
                    }
                }
                // The scale form's primary: the level whose
                // IQR ratio is most extreme. Every other
                // level's ratio is recomputed against the
                // typical IQR without it, so a narrowness
                // that exists only because one other level
                // is wide can be recognized the same way.
                let scale_key = |c: &Candidate| -> f64 {
                    if c.ratio == 0.0 || c.ratio.is_infinite() {
                        f64::INFINITY
                    } else {
                        c.ratio.ln().abs()
                    }
                };
                let primary_scale = (0..local_cands.len())
                    .max_by(|&a, &b| {
                        scale_key(&local_cands[a]).total_cmp(&scale_key(&local_cands[b]))
                    })
                    .unwrap();
                let primary_scale_level = local_cands[primary_scale].level;
                for (slot, cand) in local_cands.iter_mut().enumerate() {
                    if slot == primary_scale {
                        continue;
                    }
                    let li = cand.level;
                    let mut others: Vec<f64> = level_iqrs
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| {
                            *j != li && *j != primary_scale_level && sorted_buckets[*j].len() >= 2
                        })
                        .map(|(_, v)| *v)
                        .collect();
                    if others.is_empty() {
                        continue;
                    }
                    others.sort_by(|a, b| a.total_cmp(b));
                    let typical = median_sorted(&others);
                    let r = if typical > 0.0 {
                        level_iqrs[li] / typical
                    } else if level_iqrs[li] > 0.0 {
                        f64::INFINITY
                    } else {
                        1.0
                    };
                    cand.ratio_without_primary = Some(r);
                }
            }
            (local_cands, local_shifts)
        })
        .collect();

    let mut candidates: Vec<Candidate> = Vec::new();
    let mut pvals: Vec<f64> = Vec::new();
    let mut p_index: Vec<(usize, bool)> = Vec::new(); // (candidate slot, is_location)
    let mut pair_shifts: Vec<(usize, usize, usize, f64)> = Vec::new();
    for (local_cands, local_shifts) in pair_results {
        pair_shifts.extend(local_shifts);
        for c in local_cands {
            let slot = candidates.len();
            p_index.push((slot, true));
            pvals.push(c.p_location);
            p_index.push((slot, false));
            pvals.push(c.p_scale);
            candidates.push(c);
        }
    }

    // ---------- BH over the whole enumerable family ----------
    let family_size = pvals.len();
    let qvals: Vec<f64> =
        crate::dep::bh_qvalues(&pvals.iter().map(|&p| Some(p)).collect::<Vec<_>>())
            .into_iter()
            .map(|q| q.unwrap_or(1.0))
            .collect();
    for (k, &(slot, is_location)) in p_index.iter().enumerate() {
        if is_location {
            candidates[slot].q_location = qvals[k];
        } else {
            candidates[slot].q_scale = qvals[k];
        }
    }
    for c in candidates.iter_mut() {
        let mut max_other = 0.0f64;
        for &(gi, mi, li, s) in &pair_shifts {
            if gi == c.grouping && mi == c.measure && li != c.level {
                max_other = max_other.max(s.abs());
            }
        }
        c.max_other_std_shift = max_other;
    }

    // ---------- decide and build findings ----------
    // Firing flags per candidate, under two refinements.
    // (A) A location contrast the pair's primary level
    // explains is not stated: recomputed against the rest
    // without the primary, its shift falls under the
    // location bar. With the primary inside its pooled
    // rest, every other level of a small grouping inherits
    // a mirror shift from the primary alone.
    // (B) A grouping with exactly two levels states its
    // contrast once: the two levels are each other's rest,
    // so a second statement would mirror the first. The
    // statement with the larger effect is kept.
    let mut loc_flags: Vec<bool> = candidates
        .iter()
        .map(|c| c.std_shift.abs() >= LOCATION_BAR && c.q_location <= BH_ALPHA)
        .collect();
    let mut scale_flags: Vec<bool> = candidates
        .iter()
        .map(|c| {
            let ratio = scale_effect_ratio(c);
            ratio.is_some_and(|r| r >= SCALE_RATIO_BAR || r <= 1.0 / SCALE_RATIO_BAR)
                && c.q_scale <= BH_ALPHA
        })
        .collect();
    let mut mult_flags: Vec<bool> = candidates.iter().map(multiple_holds).collect();
    for (i, c) in candidates.iter().enumerate() {
        if loc_flags[i] {
            if let Some(s) = c.std_shift_without_primary {
                if s.abs() < LOCATION_BAR {
                    loc_flags[i] = false;
                }
            }
        }
        if scale_flags[i] {
            if let Some(r) = c.ratio_without_primary {
                if r < SCALE_RATIO_BAR && r > 1.0 / SCALE_RATIO_BAR {
                    scale_flags[i] = false;
                }
            }
        }
    }
    {
        let mut pairs: std::collections::BTreeMap<(usize, usize), Vec<usize>> =
            std::collections::BTreeMap::new();
        for (i, c) in candidates.iter().enumerate() {
            if groupings[c.grouping].labels.len() == 2 {
                pairs.entry((c.grouping, c.measure)).or_default().push(i);
            }
        }
        for (_, idxs) in pairs {
            if idxs.len() != 2 {
                continue;
            }
            let (a, b) = (idxs[0], idxs[1]);
            let fires = |i: usize, lf: &[bool], sf: &[bool], mf: &[bool]| lf[i] || sf[i] || mf[i];
            if fires(a, &loc_flags, &scale_flags, &mult_flags)
                && fires(b, &loc_flags, &scale_flags, &mult_flags)
            {
                let eff = |i: usize| -> f64 {
                    let c = &candidates[i];
                    if mult_flags[i] {
                        return 2.0;
                    }
                    let le = if loc_flags[i] { c.std_shift.abs() } else { 0.0 };
                    let se = if scale_flags[i] && c.ratio.is_infinite() {
                        2.0
                    } else if scale_flags[i] && c.ratio > 0.0 {
                        c.ratio.ln().abs() / std::f64::consts::LN_2
                    } else {
                        0.0
                    };
                    le.max(se)
                };
                let keep = if eff(b) > eff(a) { b } else { a };
                let drop = if keep == a { b } else { a };
                loc_flags[drop] = false;
                scale_flags[drop] = false;
                mult_flags[drop] = false;
            }
        }
    }
    let mut scored: Vec<(f64, Finding)> = Vec::new();
    // Location and scale statements by (grouping, measure) pair,
    // with their candidate index: a pair with more than one
    // stated level is stated once, as a profile (below).
    let mut per_pair: BTreeMap<(usize, usize), Vec<StatedLevel>> = BTreeMap::new();
    let mut n_multiple = 0usize;
    for (ci, c) in candidates.iter().enumerate() {
        let g = &groupings[c.grouping];
        let measure = &table.names[c.measure];
        let loc_fires = loc_flags[ci];
        let scale_fires = scale_flags[ci];
        if mult_flags[ci] {
            let (quotient, max_dev) = c.multiple.unwrap();
            n_multiple += 1;
            scored.push((
                2.0,
                make_finding(
                    g,
                    measure,
                    c,
                    "constant_multiple",
                    format!(
                        "{} for {} = '{}' sits at a near-constant multiple of the pooled rest: median {} against {}, a quotient of {}, and the level's deciles track {}x the pooled rest's deciles (largest decile deviation {:.1}%)",
                        measure,
                        g.describe,
                        c.level_label,
                        crate::fmt_float(c.level_median),
                        crate::fmt_float(c.rest_median),
                        crate::fmt_float(quotient),
                        crate::fmt_float(quotient),
                        100.0 * max_dev,
                    ),
                    0.9,
                    Some(format!(
                        "a constant multiple is the signature of a unit difference (centimetres against inches, cents against dollars) or of one source recorded on a different scale; check how the '{}' rows of {} were recorded before pooling them with the rest",
                        c.level_label, g.describe
                    )),
                ),
            ));
            continue;
        }
        if !loc_fires && !scale_fires {
            continue;
        }
        let form = match (loc_fires, scale_fires) {
            (true, true) => "location_scale",
            (true, false) => "location",
            _ => "scale",
        };
        let above = c.shift > 0.0;
        let ratio_text = if c.ratio.is_infinite() {
            "the typical other level has no spread".to_string()
        } else {
            format!("{}x", crate::fmt_float(c.ratio))
        };
        let mut claim = if form == "scale" {
            format!(
                "{} for {} = '{}' is {} variable than a typical other level: IQR {} against {} ({}), medians {} against {} pooled",
                measure,
                g.describe,
                c.level_label,
                if c.ratio >= 1.0 { "more" } else { "less" },
                crate::fmt_float(c.level_iqr),
                crate::fmt_float(c.rest_iqr),
                ratio_text,
                crate::fmt_float(c.level_median),
                crate::fmt_float(c.rest_median),
            )
        } else {
            let pct = if c.rest_median != 0.0 {
                format!(
                    ", {:+.0}% against the pooled median",
                    100.0 * c.shift / c.rest_median.abs()
                )
            } else {
                String::new()
            };
            let mut s = format!(
                "{} for {} = '{}' sits {} the pooled rest: median {} against {} (a shift of {}, {:.2} pooled IQRs{})",
                measure,
                g.describe,
                c.level_label,
                if above { "above" } else { "below" },
                crate::fmt_float(c.level_median),
                crate::fmt_float(c.rest_median),
                crate::fmt_float(c.shift),
                c.std_shift.abs(),
                pct,
            );
            if form == "location_scale" {
                s.push_str(&format!(
                    ", and is {} variable than a typical other level: IQR {} against {} ({})",
                    if c.ratio >= 1.0 { "more" } else { "less" },
                    crate::fmt_float(c.level_iqr),
                    crate::fmt_float(c.rest_iqr),
                    ratio_text,
                ));
            }
            s
        };
        // The "no other level" sentence: stated when this is
        // the pair's only location-firing level and every
        // other tested level sits well inside the bar.
        if loc_fires {
            let others_fire = candidates.iter().any(|o| {
                o.grouping == c.grouping
                    && o.measure == c.measure
                    && o.level != c.level
                    && !multiple_holds(o)
                    && o.std_shift.abs() >= LOCATION_BAR
                    && o.q_location <= BH_ALPHA
            });
            if !others_fire && c.max_other_std_shift < LOCATION_BAR {
                claim.push_str(&format!(
                    "; no other level of {} has a median more than {:.2} pooled IQRs from its pooled rest",
                    g.describe, c.max_other_std_shift
                ));
            }
        }
        let loc_eff = c.std_shift.abs();
        let scale_eff = if c.ratio.is_finite() && c.ratio > 0.0 {
            c.ratio.ln().abs() / std::f64::consts::LN_2
        } else if c.ratio.is_infinite() {
            2.0
        } else {
            0.0
        };
        let effect = loc_eff.max(scale_eff);
        let score = 0.5 + 0.25 * effect.min(2.0);
        per_pair.entry((c.grouping, c.measure)).or_default().push((
            effect,
            make_finding(g, measure, c, form, claim, score, Some(format!(
                "one level of {} reads differently from the rest on {} — in location, spread, or both — while the pooled mean over all levels can look unremarkable; check whether the '{}' rows were recorded, converted, or collected differently before pooling them with the rest",
                g.describe, measure, c.level_label
            ))),
            ci,
        ));
    }
    let mut n_profiles = 0usize;
    for ((gi, mi), mut stated) in per_pair {
        if stated.len() == 1 {
            let (effect, f, _) = stated.pop().unwrap();
            scored.push((effect, f));
            continue;
        }
        n_profiles += 1;
        let tested: Vec<&Candidate> = candidates
            .iter()
            .filter(|c| c.grouping == gi && c.measure == mi)
            .collect();
        let effect = stated.iter().map(|s| s.0).fold(0.0f64, f64::max);
        let levels: Vec<usize> = stated.iter().map(|s| s.2).collect();
        scored.push((
            effect,
            profile_finding(
                &groupings[gi],
                &table.names[mi],
                &tested,
                &levels,
                &candidates,
                effect,
            ),
        ));
    }
    let scored = merge_across_measures(scored);
    let mut scored = scored;
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.claim.cmp(&b.1.claim)));
    let reported = scored.len().min(MAX_FINDINGS);
    scored.truncate(MAX_FINDINGS);
    let findings: Vec<Finding> = scored.into_iter().map(|(_, f)| f).collect();

    let n_calendar = groupings.len() - n_column_groupings;
    let mut detail = format!(
        "{} groupings ({} categorical columns, {} calendar parts of the axis) x {} numeric measures: {} pairs examined, {} levels tested ({} location and scale tests in the Benjamini-Hochberg family), {} contrasts reported{}{} (cap {MAX_FINDINGS})",
        groupings.len(),
        n_column_groupings,
        n_calendar,
        measures.len(),
        pairs_examined,
        candidates.len(),
        family_size,
        reported,
        if n_multiple > 0 {
            format!(", {n_multiple} of them constant multiples")
        } else {
            String::new()
        },
        if n_profiles > 0 {
            format!(
                ", {n_profiles} of them profiles stating every differing level of one grouping on one measure"
            )
        } else {
            String::new()
        },
    );
    if pairs_capped {
        detail.push_str(&format!(
            "; pair cap {MAX_PAIRS} reached — later grouping x measure pairs were not examined"
        ));
    }
    if !input.exclude_measures.is_empty() {
        detail.push_str(&format!(
            "; measure(s) withheld because the outcome screen owns their per-level contrasts: {}",
            input.exclude_measures.join(", ")
        ));
    }
    if !merged_into_calendar.is_empty() {
        detail.push_str(&format!(
            "; column(s) grouping the rows exactly as a calendar part of the axis, examined once as that part: {}",
            merged_into_calendar.join(", ")
        ));
    }
    if groupings.is_empty() || measures.is_empty() {
        detail.push_str("; no eligible grouping x measure pair exists on this table");
    }
    LevelsOutcome { findings, detail }
}

#[allow(clippy::too_many_arguments)]
/// Measures one level must shift in the same direction on before
/// its location statements merge into one.
const MIN_MEASURES_TO_MERGE: usize = 3;

/// One level that sits on the same side of its pooled rest on
/// several measures (infants are smaller on every size measure) is
/// one fact: its location statements merge into one statement
/// naming each measure with its median shift, the per-measure
/// statements kept in the evidence's `measures` table.
fn merge_across_measures(scored: Vec<(f64, Finding)>) -> Vec<(f64, Finding)> {
    let key = |f: &Finding| -> Option<(String, String, bool)> {
        let m = &f.evidence.first()?.metrics;
        let form = m.get("form")?.as_str()?;
        if form != "location" && form != "location_scale" {
            return None;
        }
        if m.get("calendar_part").is_some() {
            return None;
        }
        Some((
            m.get("grouping_column")?.as_str()?.to_string(),
            m.get("level")?.as_str()?.to_string(),
            m.get("shift")?.as_f64()? > 0.0,
        ))
    };
    let mut groups: BTreeMap<(String, String, bool), Vec<usize>> = BTreeMap::new();
    for (i, (_, f)) in scored.iter().enumerate() {
        if let Some(k) = key(f) {
            groups.entry(k).or_default().push(i);
        }
    }
    let mut merged_away: std::collections::BTreeSet<usize> = Default::default();
    let mut out: Vec<(f64, Finding)> = Vec::new();
    for ((grouping, level, above), idx) in &groups {
        if idx.len() < MIN_MEASURES_TO_MERGE {
            continue;
        }
        let mut parts: Vec<String> = Vec::new();
        let mut rows: Vec<Value> = Vec::new();
        let mut measures: Vec<String> = Vec::new();
        let mut effect = 0.0f64;
        for &i in idx {
            let (e, f) = &scored[i];
            effect = effect.max(*e);
            let m = &f.evidence[0].metrics;
            let measure = m["measure_column"].as_str().unwrap_or("").to_string();
            let lm = m["level_median"].as_f64().unwrap_or(f64::NAN);
            let rm = m["rest_median"].as_f64().unwrap_or(f64::NAN);
            let pct = if rm != 0.0 {
                format!("{:+.0}%", 100.0 * (lm - rm) / rm.abs())
            } else {
                format!(
                    "median {} against {}",
                    crate::fmt_float(lm),
                    crate::fmt_float(rm)
                )
            };
            parts.push(format!("{measure} ({pct})"));
            rows.push(
                json!({"measure": measure, "level_median": lm, "rest_median": rm,
                             "shift_in_pooled_iqrs": m.get("shift_in_pooled_iqrs"),
                             "location_q_value": m.get("location_q_value"), "claim": f.claim}),
            );
            measures.push(measure);
            merged_away.insert(i);
        }
        let claim = format!(
            "{grouping} = '{level}' sits {} the rest on {} measures, by median: {}",
            if *above { "above" } else { "below" },
            idx.len(),
            parts.join(", ")
        );
        let mut columns = vec![grouping.clone()];
        columns.extend(measures.iter().cloned());
        let metrics: BTreeMap<String, Value> = BTreeMap::from([
            ("grouping_column".into(), json!(grouping)),
            ("level".into(), json!(level)),
            ("form".into(), json!("across_measures")),
            ("measures".into(), Value::Array(rows)),
            (
                "n".into(),
                scored[idx[0]].1.evidence[0]
                    .metrics
                    .get("n")
                    .cloned()
                    .unwrap_or(json!(null)),
            ),
            ("method".into(), json!(METHOD_TEXT)),
        ]);
        out.push((
            effect,
            Finding::new(
                Pattern::LevelContrast,
                BTreeMap::from([("columns".into(), json!(columns))]),
                claim,
                EvidenceBasis::Measured,
                0.5 + 0.25 * effect.min(2.0),
                metrics,
                Some(format!(
                    "the '{level}' rows of {grouping} differ from the rest on every measure named, in the same direction: one property of that group, not several"
                )),
                Method::new(
                    "level_contrasts",
                    "per_level_quantile_contrast",
                    "per-level quantile contrasts (onceover v0.1)",
                ),
            ),
        ));
    }
    out.extend(
        scored
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !merged_away.contains(i))
            .map(|(_, x)| x),
    );
    out
}

/// One statement for a (grouping, measure) pair on which several
/// levels differ from their pooled rest: the measure's profile over
/// the grouping, with every tested level's median and every stated
/// level's contrast in the evidence (`levels`).
fn profile_finding(
    g: &Grouping,
    measure: &str,
    tested: &[&Candidate],
    stated: &[usize],
    candidates: &[Candidate],
    effect: f64,
) -> Finding {
    let lo = tested
        .iter()
        .min_by(|a, b| a.level_median.total_cmp(&b.level_median))
        .unwrap();
    let hi = tested
        .iter()
        .max_by(|a, b| a.level_median.total_cmp(&b.level_median))
        .unwrap();
    let pooled_median = {
        let mut m: Vec<f64> = tested.iter().map(|c| c.level_median).collect();
        m.sort_by(|a, b| a.total_cmp(b));
        median_sorted(&m)
    };
    // A calendar part's labels read alone (January, Saturday); a
    // column's labels are named with the column (hr = '3').
    let label = |c: &Candidate| -> String {
        if g.calendar_part.is_some() {
            c.level_label.clone()
        } else {
            format!("{} = '{}'", g.column, c.level_label)
        }
    };
    let claim = format!(
        "{} varies across {}: medians from {} ({}) to {} ({}); {} of {} levels differ from their pooled rest",
        measure,
        g.describe,
        crate::fmt_float(lo.level_median),
        label(lo),
        crate::fmt_float(hi.level_median),
        label(hi),
        stated.len(),
        tested.len(),
    );
    let level_rows: Vec<Value> = tested
        .iter()
        .map(|c| {
            let idx = candidates
                .iter()
                .position(|o| std::ptr::eq(o, *c))
                .unwrap_or(usize::MAX);
            json!({
                "level": c.level_label,
                "rows": c.n_level,
                "median": c.level_median,
                "iqr": c.level_iqr,
                "shift_in_pooled_iqrs": c.std_shift,
                "location_q_value": c.q_location,
                "scale_q_value": c.q_scale,
                "stated": stated.contains(&idx),
            })
        })
        .collect();
    let mut metrics: BTreeMap<String, Value> = BTreeMap::from([
        ("column".into(), json!(measure)),
        ("measure_column".into(), json!(measure)),
        ("grouping_column".into(), json!(g.column)),
        ("form".into(), json!("profile")),
        ("levels_tested".into(), json!(tested.len())),
        ("levels_stated".into(), json!(stated.len())),
        ("median_of_level_medians".into(), json!(pooled_median)),
        ("n".into(), json!(tested[0].n_level + tested[0].n_rest)),
        ("levels".into(), Value::Array(level_rows)),
        ("method".into(), json!(METHOD_TEXT)),
    ]);
    if let Some(part) = g.calendar_part {
        metrics.insert("calendar_part".into(), json!(part));
        metrics.insert("axis".into(), json!(g.column));
    }
    Finding::new(
        Pattern::LevelContrast,
        BTreeMap::from([(
            "columns".into(),
            json!([g.column.clone(), measure.to_string()]),
        )]),
        claim,
        EvidenceBasis::Measured,
        0.5 + 0.25 * effect.min(2.0),
        metrics,
        Some(format!(
            "{measure} depends on {}: the levels named differ from the rest in location or spread; the per-level medians and tests are in the evidence's levels table",
            g.describe
        )),
        Method::new(
            "level_contrasts",
            "per_level_quantile_contrast",
            "per-level quantile contrasts (onceover v0.1)",
        ),
    )
}

fn make_finding(
    g: &Grouping,
    measure: &str,
    c: &Candidate,
    form: &str,
    claim: String,
    score: f64,
    interpretation: Option<String>,
) -> Finding {
    let mut metrics: BTreeMap<String, Value> = BTreeMap::from([
        ("column".into(), json!(measure)),
        ("measure_column".into(), json!(measure)),
        ("grouping_column".into(), json!(g.column)),
        ("level".into(), json!(c.level_label)),
        ("form".into(), json!(form)),
        ("level_rows".into(), json!(c.n_level)),
        ("rest_rows".into(), json!(c.n_rest)),
        ("n".into(), json!(c.n_level + c.n_rest)),
        ("level_median".into(), json!(c.level_median)),
        ("rest_median".into(), json!(c.rest_median)),
        ("shift".into(), json!(c.shift)),
        ("shift_in_pooled_iqrs".into(), json!(c.std_shift)),
        ("level_iqr".into(), json!(c.level_iqr)),
        ("rest_iqr".into(), json!(c.rest_iqr)),
        (
            "iqr_ratio".into(),
            if c.ratio.is_infinite() {
                Value::Null
            } else {
                json!(c.ratio)
            },
        ),
        ("location_p_value".into(), json!(c.p_location)),
        ("location_q_value".into(), json!(c.q_location)),
        ("scale_p_value".into(), json!(c.p_scale)),
        ("scale_q_value".into(), json!(c.q_scale)),
        (
            "max_other_shift_in_pooled_iqrs".into(),
            json!(c.max_other_std_shift),
        ),
        ("method".into(), json!(METHOD_TEXT)),
    ]);
    if let Some(part) = g.calendar_part {
        metrics.insert("calendar_part".into(), json!(part));
        metrics.insert("axis".into(), json!(g.column));
    }
    if form == "constant_multiple" {
        if let Some((quotient, max_dev)) = c.multiple {
            metrics.insert("quotient".into(), json!(quotient));
            metrics.insert("quotient_max_decile_deviation".into(), json!(max_dev));
        }
    }
    Finding::new(
        Pattern::LevelContrast,
        BTreeMap::from([(
            "columns".into(),
            json!([g.column.clone(), measure.to_string()]),
        )]),
        claim,
        EvidenceBasis::Measured,
        score,
        metrics,
        interpretation,
        Method::new(
            "level_contrasts",
            "per_level_quantile_contrast",
            "per-level quantile contrasts (onceover v0.1)",
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::extract;
    use polars::prelude::*;

    /// Deterministic pseudo-noise in [0, 1): a hashed walk, no RNG.
    fn noise(i: usize) -> f64 {
        let mut x = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        x ^= x >> 29;
        x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x ^= x >> 32;
        (x % 10_000) as f64 / 10_000.0
    }

    fn frame(
        sites: &[&str],
        per_site: impl Fn(&str, usize) -> f64,
        n_per_site: usize,
    ) -> (Vec<String>, Vec<f64>) {
        let mut g = Vec::new();
        let mut m = Vec::new();
        for s in sites {
            for i in 0..n_per_site {
                g.push(s.to_string());
                m.push(per_site(s, i));
            }
        }
        (g, m)
    }

    #[test]
    fn location_shifted_level_fires() {
        // Four sites at median ~4.0; K7 shifted up by 1.2 pooled
        // IQRs.
        let (g, m) = frame(
            &["A", "B", "C", "K7"],
            |s, i| {
                let base = 3.5 + noise(i) + noise(i + 5000);
                if s == "K7" {
                    base + 1.2
                } else {
                    base
                }
            },
            400,
        );
        let df = df! { "site" => g, "reading" => m }.unwrap();
        let table = extract(&df);
        let out = profile_level_contrasts(
            &table,
            &LevelContrastInput {
                axis_name: None,
                axis_ticks: None,
                exclude_measures: &[],
                ..Default::default()
            },
        );
        let hits: Vec<_> = out
            .findings
            .iter()
            .filter(|f| f.metrics()["level"] == json!("K7"))
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "findings: {:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        let f = hits[0];
        assert_eq!(f.finding_type.as_deref(), Some("level_contrast"));
        assert!(
            f.claim.contains("sits above the pooled rest"),
            "{}",
            f.claim
        );
        assert!(f.claim.contains("no other level"), "{}", f.claim);
        f.validate().unwrap();
    }

    #[test]
    fn scale_shifted_level_with_equal_means_fires() {
        // WIDE has the same centre as the rest but three times
        // the spread; the mean-based screen sees nothing.
        let (g, m) = frame(
            &["A", "B", "WIDE"],
            |s, i| {
                let u = noise(i) - 0.5 + (noise(i + 9000) - 0.5);
                if s == "WIDE" {
                    10.0 + 3.0 * u
                } else {
                    10.0 + u
                }
            },
            500,
        );
        let df = df! { "site" => g, "reading" => m }.unwrap();
        let table = extract(&df);
        let out = profile_level_contrasts(
            &table,
            &LevelContrastInput {
                axis_name: None,
                axis_ticks: None,
                exclude_measures: &[],
                ..Default::default()
            },
        );
        let hits: Vec<_> = out
            .findings
            .iter()
            .filter(|f| f.metrics()["level"] == json!("WIDE"))
            .collect();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].claim.contains("more variable"), "{}", hits[0].claim);
        hits[0].validate().unwrap();
    }

    #[test]
    fn constant_multiple_level_gets_the_multiple_form() {
        // CM's values are exactly 2.54x the same underlying
        // readings (an inch column among centimetre columns,
        // inverted): the quotient is the statement.
        let (g, m) = frame(
            &["A", "B", "CM"],
            |s, i| {
                let base = 20.0 + 10.0 * noise(i) + 5.0 * noise(i + 3000);
                if s == "CM" {
                    base * 2.54
                } else {
                    base
                }
            },
            400,
        );
        let df = df! { "site" => g, "reading" => m }.unwrap();
        let table = extract(&df);
        let out = profile_level_contrasts(
            &table,
            &LevelContrastInput {
                axis_name: None,
                axis_ticks: None,
                exclude_measures: &[],
                ..Default::default()
            },
        );
        // Only the multiple is stated: A and B look narrow
        // against a typical level only because CM is wide,
        // and that contrast is CM's own statement mirrored.
        assert_eq!(
            out.findings.len(),
            1,
            "findings: {:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        let hits: Vec<_> = out
            .findings
            .iter()
            .filter(|f| f.metrics()["level"] == json!("CM"))
            .collect();
        assert_eq!(hits.len(), 1);
        let f = hits[0];
        assert_eq!(f.metrics()["form"], json!("constant_multiple"));
        let q = f.metrics()["quotient"].as_f64().unwrap();
        assert!((q - 2.54).abs() < 0.05, "quotient {q}");
        assert!(f.claim.contains("near-constant multiple"), "{}", f.claim);
        f.validate().unwrap();
    }

    #[test]
    fn shifted_level_far_from_zero_is_location_not_multiple() {
        // A pure +3.5 shift on a distribution bounded away
        // from zero (readings in [14.5, 16.5] against [11, 13]) also
        // concentrates its decile quotients (at about 1.27x),
        // but it leaves the spread unchanged, so it must be
        // stated as a location contrast, not a multiple.
        let mut groups: Vec<&str> = Vec::new();
        let mut values: Vec<f64> = Vec::new();
        for site in ["A", "B", "SHIFT"] {
            for i in 0..400usize {
                groups.push(site);
                let mut v = 11.0 + noise(i) + noise(i + 5000);
                if site == "SHIFT" {
                    v += 3.5;
                }
                values.push(v);
            }
        }
        let df = df! { "site" => groups, "reading" => values }.unwrap();
        let table = extract(&df);
        let out = profile_level_contrasts(
            &table,
            &LevelContrastInput {
                axis_name: None,
                axis_ticks: None,
                exclude_measures: &[],
                ..Default::default()
            },
        );
        assert_eq!(
            out.findings.len(),
            1,
            "findings: {:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        let f = &out.findings[0];
        assert_eq!(f.metrics()["form"], json!("location"));
        assert!(
            f.claim.contains("sits above the pooled rest"),
            "{}",
            f.claim
        );
        f.validate().unwrap();
    }

    #[test]
    fn calendar_day_of_week_contrast_fires() {
        // Two years of daily readings; Saturdays run high.
        // 2023-01-01 was a Sunday, so day d has weekday
        // (d + 4) % 7 with 0 = Sunday; Saturday is 6.
        let n = 731usize;
        let start = 19_358i64; // 2023-01-01 in days since epoch
        let mut days: Vec<i32> = Vec::new();
        let mut m: Vec<f64> = Vec::new();
        let mut ticks: Vec<Option<i64>> = Vec::new();
        for d in 0..n as i64 {
            let day = start + d;
            days.push(day as i32);
            let saturday = dateutil::weekday(day) == 6;
            let mut v = 10.0 + noise(d as usize) + noise(d as usize + 4000);
            if saturday {
                v += 3.0;
            }
            m.push(v);
            ticks.push(Some(day * MICROS_PER_DAY));
        }
        let day_series = Series::new("day".into(), &days)
            .cast(&DataType::Date)
            .unwrap();
        let df = DataFrame::new(
            days.len(),
            vec![day_series.into(), Series::new("reading".into(), &m).into()],
        )
        .unwrap();
        let table = extract(&df);
        let out = profile_level_contrasts(
            &table,
            &LevelContrastInput {
                axis_name: Some("day"),
                axis_ticks: Some(&ticks),
                exclude_measures: &[],
                ..Default::default()
            },
        );
        let hits: Vec<_> = out
            .findings
            .iter()
            .filter(|f| f.metrics()["calendar_part"] == json!("day_of_week"))
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "findings: {:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        assert_eq!(hits[0].metrics()["level"], json!("Saturday"));
        assert!(
            hits[0].claim.contains("day of week of day"),
            "{}",
            hits[0].claim
        );
        hits[0].validate().unwrap();
    }

    #[test]
    fn clean_and_shuffled_controls_are_silent() {
        // No planted structure: every site draws the same
        // distribution.
        let (g, m) = frame(
            &["A", "B", "C", "D"],
            |_, i| 3.5 + noise(i) + noise(i + 5000),
            400,
        );
        let df = df! { "site" => g.clone(), "reading" => m.clone() }.unwrap();
        let table = extract(&df);
        let out = profile_level_contrasts(
            &table,
            &LevelContrastInput {
                axis_name: None,
                axis_ticks: None,
                exclude_measures: &[],
                ..Default::default()
            },
        );
        assert!(
            out.findings.is_empty(),
            "clean control fired: {:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        // Shuffled levels against a structured measure: the
        // level labels carry no information.
        let mut shuffled = g.clone();
        let n = shuffled.len();
        for i in 0..n {
            shuffled.swap(i, (i * 7919 + 13) % n);
        }
        let m2: Vec<f64> = m
            .iter()
            .enumerate()
            .map(|(i, v)| if i % 4 == 0 { v + 5.0 } else { *v })
            .collect();
        let df2 = df! { "site" => shuffled, "reading" => m2 }.unwrap();
        let table2 = extract(&df2);
        let out2 = profile_level_contrasts(
            &table2,
            &LevelContrastInput {
                axis_name: None,
                axis_ticks: None,
                exclude_measures: &[],
                ..Default::default()
            },
        );
        assert!(
            out2.findings.is_empty(),
            "shuffled control fired: {:?}",
            out2.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
    }
}
