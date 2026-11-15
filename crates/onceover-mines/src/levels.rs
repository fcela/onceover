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
//! compared with all the other levels pooled: median and
//! interquartile range on both sides, stated when the location
//! shift clears half the pooled IQR or the IQR ratio clears
//! 1.5 (or its reciprocal 2/3). A level whose values sit at a
//! near-constant multiple of the rest's distribution gets its
//! own statement, because the quotient is the actionable fact
//! (a unit error: cm against inch, cents against dollars).
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

use onceover_core::{dateutil, TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
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
pub struct LevelContrastInput<'a> {
    pub axis_name: Option<&'a str>,
    pub axis_ticks: Option<&'a [Option<i64>]>,
}

/// The pass's result: findings plus the coverage detail.
pub struct LevelsOutcome {
    pub findings: Vec<Finding>,
    pub detail: String,
}

/// One grouping of the rows: per-row level codes and labels.
struct Grouping {
    /// The column named in the subject (the axis column for
    /// calendar parts).
    column: String,
    /// How the grouping reads in claims: the column name, or
    /// "day of week of {axis}" for a calendar part.
    describe: String,
    codes: Vec<Option<u32>>,
    labels: Vec<String>,
    calendar_part: Option<&'static str>,
    /// Calendar only: per level, how many distinct cycles its
    /// rows span.
    cycles: Vec<usize>,
}

/// Level codes for one column, when it is a low-cardinality
/// categorical: strings, booleans, and integer columns with
/// 2..=MAX_LEVELS distinct non-null values.
fn code_levels(table: &TypedTable, ci: usize) -> Option<(Vec<Option<u32>>, Vec<String>)> {
    let mut index: BTreeMap<String, u32> = BTreeMap::new();
    let mut labels: Vec<String> = Vec::new();
    let push = |index: &mut BTreeMap<String, u32>, labels: &mut Vec<String>, s: String| {
        if !index.contains_key(&s) {
            index.insert(s.clone(), labels.len() as u32);
            labels.push(s);
        }
    };
    let raw: Vec<Option<String>> = match &table.cols[ci] {
        TypedCol::Str { vals } => vals.iter().map(|v| v.map(|s| s.to_string())).collect(),
        TypedCol::StrOwned { vals } => vals.to_vec(),
        TypedCol::Bool { vals } => vals
            .iter()
            .map(|v| v.map(|b| if b { "true".into() } else { "false".into() }))
            .collect(),
        TypedCol::I64 { vals } => vals
            .iter()
            .map(|v| v.map(|x| crate::fmt_float(*x as f64)))
            .collect(),
        TypedCol::U64 { vals } => vals
            .iter()
            .map(|v| v.map(|x| crate::fmt_float(*x as f64)))
            .collect(),
        _ => return None,
    };
    for v in raw.iter().flatten() {
        push(&mut index, &mut labels, v.clone());
        if labels.len() > MAX_LEVELS {
            return None;
        }
    }
    if !(2..=MAX_LEVELS).contains(&labels.len()) {
        return None;
    }
    let codes = raw.iter().map(|v| v.as_ref().map(|s| index[s])).collect();
    Some((codes, labels))
}

/// Calendar-part groupings derived from the axis ticks.
/// One calendar part: its name, its level labels, and the
/// function mapping a day (days since epoch, or an hour
/// index) to (level code, cycle key).
type CalendarPartSpec = (&'static str, Vec<&'static str>, fn(i64) -> (u32, i64));

fn calendar_groupings(input: &LevelContrastInput) -> Vec<Grouping> {
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

/// Quantile of a sorted slice, linear interpolation (the
/// type-7 convention).
fn quantile(sorted: &[f64], p: f64) -> f64 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let h = (sorted.len() - 1) as f64 * p;
    let lo = h.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    sorted[lo] + (h - lo as f64) * (sorted[hi] - sorted[lo])
}

fn median(sorted: &[f64]) -> f64 {
    quantile(sorted, 0.5)
}

fn iqr(sorted: &[f64]) -> f64 {
    quantile(sorted, 0.75) - quantile(sorted, 0.25)
}

/// Two-sided normal tail (Abramowitz-Stegun 7.1.26, the
/// approximation used across the crate's statistics).
fn normal_sf(z: f64) -> f64 {
    let x = z.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let erf = 1.0 - poly * (-x * x).exp();
    (1.0 - erf) / 2.0
}

/// Rank-sum (Mann-Whitney) two-sided p-value for sample `a`
/// against sample `b`, both sorted, with the standard tie
/// correction. Ranks are computed by merging the two sorted
/// slices, so no combined copy is built.
fn rank_sum_p(a: &[f64], b: &[f64]) -> f64 {
    let (n1, n2) = (a.len(), b.len());
    if n1 == 0 || n2 == 0 {
        return 1.0;
    }
    let n = (n1 + n2) as f64;
    let (mut i, mut j) = (0usize, 0usize);
    let mut rank_sum_a = 0.0f64;
    let mut tie_correction = 0.0f64;
    let mut pos = 0usize; // elements consumed, 0-based
    while i < n1 || j < n2 {
        let v = match (a.get(i), b.get(j)) {
            (Some(&x), Some(&y)) => x.min(y),
            (Some(&x), None) => x,
            (None, Some(&y)) => y,
            (None, None) => break,
        };
        let start = pos;
        let mut in_a = 0usize;
        while i < n1 && a[i] == v {
            i += 1;
            pos += 1;
            in_a += 1;
        }
        while j < n2 && b[j] == v {
            j += 1;
            pos += 1;
        }
        let t = (pos - start) as f64;
        if t > 1.0 {
            tie_correction += t * t * t - t;
        }
        // Average rank (1-based) over the tie group start..pos.
        rank_sum_a += in_a as f64 * (start + pos + 1) as f64 / 2.0;
    }
    let u = rank_sum_a - n1 as f64 * (n1 as f64 + 1.0) / 2.0;
    let mean_u = n1 as f64 * n2 as f64 / 2.0;
    let var = n1 as f64 * n2 as f64 / 12.0 * ((n + 1.0) - tie_correction / (n * (n - 1.0)));
    if var <= 0.0 {
        return 1.0;
    }
    let z = (u - mean_u) / var.sqrt();
    (2.0 * normal_sf(z)).clamp(1e-300, 1.0)
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
}

/// The per-level contrast pass. See the module documentation
/// for the reporting bars.
pub fn profile_level_contrasts(table: &TypedTable, input: &LevelContrastInput) -> LevelsOutcome {
    // ---------- groupings ----------
    let mut groupings: Vec<Grouping> = Vec::new();
    for ci in 0..table.cols.len() {
        if let Some((codes, labels)) = code_levels(table, ci) {
            groupings.push(Grouping {
                column: table.names[ci].clone(),
                describe: table.names[ci].clone(),
                codes,
                labels,
                calendar_part: None,
                cycles: Vec::new(),
            });
        }
    }
    let n_column_groupings = groupings.len();
    groupings.extend(calendar_groupings(input));

    // ---------- measures ----------
    let measures: Vec<usize> = (0..table.cols.len())
        .filter(|&ci| {
            matches!(
                table.cols[ci],
                TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. }
            )
        })
        .collect();

    let mut candidates: Vec<Candidate> = Vec::new();
    let mut pvals: Vec<f64> = Vec::new();
    let mut p_index: Vec<(usize, bool)> = Vec::new(); // (candidate slot, is_location)
    let mut pairs_examined = 0usize;
    let mut pairs_capped = false;
    // Per-pair standardized shifts, for the "no other level"
    // sentence: (pair key, level, std shift).
    let mut pair_shifts: Vec<(usize, usize, usize, f64)> = Vec::new();

    'pairs: for (gi, g) in groupings.iter().enumerate() {
        for &mi in &measures {
            if table.names[mi] == g.column && g.calendar_part.is_none() {
                continue;
            }
            if pairs_examined >= MAX_PAIRS {
                pairs_capped = true;
                break 'pairs;
            }
            pairs_examined += 1;
            let col = &table.cols[mi];
            let mut buckets: Vec<Vec<f64>> = vec![Vec::new(); g.labels.len()];
            for row in 0..table.n {
                if let (Some(code), Some(v)) = (g.codes[row], col.numeric_at(row)) {
                    if v.is_finite() {
                        buckets[code as usize].push(v);
                    }
                }
            }
            let total: usize = buckets.iter().map(|b| b.len()).sum();
            if total < 2 * MIN_SIDE_VALUES {
                continue;
            }
            let mut pooled_sorted: Vec<f64> = Vec::with_capacity(total);
            for b in &buckets {
                pooled_sorted.extend_from_slice(b);
            }
            pooled_sorted.sort_by(|a, b| a.total_cmp(b));
            let distinct_count = pooled_sorted.windows(2).filter(|w| w[0] != w[1]).count() + 1;
            if distinct_count < 3 {
                continue;
            }
            let spread = {
                let s = iqr(&pooled_sorted);
                if s > 0.0 {
                    s
                } else {
                    let s2 = quantile(&pooled_sorted, 0.9) - quantile(&pooled_sorted, 0.1);
                    if s2 > 0.0 {
                        s2
                    } else {
                        pooled_sorted[pooled_sorted.len() - 1] - pooled_sorted[0]
                    }
                }
            };
            if spread <= 0.0 {
                continue;
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
            let sorted_buckets: Vec<Vec<f64>> = buckets
                .iter()
                .map(|b| {
                    let mut s = b.clone();
                    s.sort_by(|a, b| a.total_cmp(b));
                    s
                })
                .collect();
            let level_medians: Vec<f64> = sorted_buckets
                .iter()
                .map(|s| if s.is_empty() { 0.0 } else { median(s) })
                .collect();
            let level_iqrs: Vec<f64> = sorted_buckets
                .iter()
                .map(|s| if s.len() >= 2 { iqr(s) } else { 0.0 })
                .collect();
            let dev_buckets: Vec<Vec<f64>> = sorted_buckets
                .iter()
                .enumerate()
                .map(|(j, s)| {
                    let m = level_medians[j];
                    let mut d: Vec<f64> = s.iter().map(|v| (v - m).abs()).collect();
                    d.sort_by(|a, b| a.total_cmp(b));
                    d
                })
                .collect();
            let pair_start = candidates.len();
            for (li, bucket) in buckets.iter().enumerate() {
                let n_level = bucket.len();
                let n_rest = total - n_level;
                if n_level < MIN_SIDE_VALUES || n_rest < MIN_SIDE_VALUES {
                    continue;
                }
                if g.calendar_part.is_some() && g.cycles.get(li).copied().unwrap_or(0) < MIN_CYCLES
                {
                    continue;
                }
                let level_sorted = sorted_buckets[li].clone();
                let rest_sorted = rest_of(&pooled_sorted, &level_sorted);
                let level_median = level_medians[li];
                let rest_median = median(&rest_sorted);
                let level_iqr = level_iqrs[li];
                let rest_iqr = {
                    let others: Vec<f64> = level_iqrs
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != li && buckets[*j].len() >= 2)
                        .map(|(_, v)| *v)
                        .collect();
                    if others.is_empty() {
                        iqr(&rest_sorted)
                    } else {
                        let mut o = others;
                        o.sort_by(|a, b| a.total_cmp(b));
                        median(&o)
                    }
                };
                let shift = level_median - rest_median;
                let std_shift = shift / spread;
                pair_shifts.push((gi, mi, li, std_shift));
                let p_location = rank_sum_p(&level_sorted, &rest_sorted);
                // Scale: rank-sum over absolute deviations
                // from each row's own level median.
                let level_dev = dev_buckets[li].clone();
                let mut rest_dev: Vec<f64> = Vec::with_capacity(n_rest);
                for (j, d) in dev_buckets.iter().enumerate() {
                    if j != li {
                        rest_dev.extend_from_slice(d);
                    }
                }
                rest_dev.sort_by(|a, b| a.total_cmp(b));
                let p_scale = rank_sum_p(&level_dev, &rest_dev);
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
                    let deciles: Vec<f64> = (1..=9)
                        .map(|k| quantile(&rest_sorted, k as f64 / 10.0))
                        .collect();
                    let one_sided =
                        deciles.iter().all(|d| *d > 0.0) || deciles.iter().all(|d| *d < 0.0);
                    if one_sided && rest_median != 0.0 {
                        let quotients: Vec<f64> = (1..=9)
                            .map(|k| {
                                quantile(&level_sorted, k as f64 / 10.0)
                                    / quantile(&rest_sorted, k as f64 / 10.0)
                            })
                            .collect();
                        let mut qs = quotients.clone();
                        qs.sort_by(|a, b| a.total_cmp(b));
                        let k_hat = median(&qs);
                        if k_hat > 0.0 && (k_hat.ln().abs() >= MULTIPLE_MIN.ln()) {
                            let max_dev = quotients
                                .iter()
                                .map(|q| (q - k_hat).abs() / k_hat)
                                .fold(0.0f64, f64::max);
                            let cv = iqr(&qs) / k_hat;
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
                let slot = candidates.len();
                p_index.push((slot, true));
                pvals.push(p_location);
                p_index.push((slot, false));
                pvals.push(p_scale);
                candidates.push(Candidate {
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
                });
            }
            // The pair's primary level (largest |standardized
            // shift|): every other level's shift is also
            // recomputed against the rest without the primary,
            // so a contrast the primary explains can be
            // recognized at decision time.
            if candidates.len() > pair_start {
                let primary = (pair_start..candidates.len())
                    .max_by(|&a, &b| {
                        candidates[a]
                            .std_shift
                            .abs()
                            .total_cmp(&candidates[b].std_shift.abs())
                    })
                    .unwrap();
                let primary_level = candidates[primary].level;
                for (slot, cand) in candidates.iter_mut().enumerate().skip(pair_start) {
                    if slot == primary {
                        continue;
                    }
                    let li = cand.level;
                    let reduced = rest_of(
                        &rest_of(&pooled_sorted, &sorted_buckets[primary_level]),
                        &sorted_buckets[li],
                    );
                    if reduced.len() >= MIN_SIDE_VALUES {
                        let s = (level_medians[li] - median(&reduced)) / spread;
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
                let primary_scale = (pair_start..candidates.len())
                    .max_by(|&a, &b| {
                        scale_key(&candidates[a]).total_cmp(&scale_key(&candidates[b]))
                    })
                    .unwrap();
                let primary_scale_level = candidates[primary_scale].level;
                for (slot, cand) in candidates.iter_mut().enumerate().skip(pair_start) {
                    if slot == primary_scale {
                        continue;
                    }
                    let li = cand.level;
                    let mut others: Vec<f64> = level_iqrs
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| {
                            *j != li && *j != primary_scale_level && buckets[*j].len() >= 2
                        })
                        .map(|(_, v)| *v)
                        .collect();
                    if others.is_empty() {
                        continue;
                    }
                    others.sort_by(|a, b| a.total_cmp(b));
                    let typical = median(&others);
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
        }
    }

    // ---------- BH over the whole enumerable family ----------
    let family_size = pvals.len();
    let qvals = crate::axisbreaks::bh_qvalues(&pvals);
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
            (c.ratio >= SCALE_RATIO_BAR || c.ratio <= 1.0 / SCALE_RATIO_BAR)
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
        scored.push((
            effect,
            make_finding(g, measure, c, form, claim, score, Some(format!(
                "one level of {} reads differently from the rest on {} — in location, spread, or both — while the pooled mean over all levels can look unremarkable; check whether the '{}' rows were recorded, converted, or collected differently before pooling them with the rest",
                g.describe, measure, c.level_label
            ))),
        ));
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.claim.cmp(&b.1.claim)));
    let reported = scored.len().min(MAX_FINDINGS);
    scored.truncate(MAX_FINDINGS);
    let findings: Vec<Finding> = scored.into_iter().map(|(_, f)| f).collect();

    let n_calendar = groupings.len() - n_column_groupings;
    let mut detail = format!(
        "{} groupings ({} categorical columns, {} calendar parts of the axis) x {} numeric measures: {} pairs examined, {} levels tested ({} location and scale tests in the Benjamini-Hochberg family), {} contrasts reported{} (cap {MAX_FINDINGS})",
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
    );
    if pairs_capped {
        detail.push_str(&format!(
            "; pair cap {MAX_PAIRS} reached — later grouping x measure pairs were not examined"
        ));
    }
    if groupings.is_empty() || measures.is_empty() {
        detail.push_str("; no eligible grouping x measure pair exists on this table");
    }
    LevelsOutcome { findings, detail }
}

#[allow(clippy::too_many_arguments)]
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
            },
        );
        assert!(
            out2.findings.is_empty(),
            "shuffled control fired: {:?}",
            out2.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
    }
}
