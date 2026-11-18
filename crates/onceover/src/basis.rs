//! Columns that restate other columns, found before the detectors
//! run, so each derivation is stated once and the passes downstream
//! do not rediscover it.
//!
//! Three kinds of column are recognised, by their values:
//!
//! - **Axis-derived columns.** A column the time axis determines
//!   exactly: every axis value occurs with one value of the column
//!   (month, season, weekday, holiday and working-day flags beside a
//!   date). The test is only informative where the axis repeats; on a
//!   table with one row per date every column is trivially a function
//!   of the date. There the column must instead be a function of a
//!   calendar key that repeats (year, quarter, month, week, day of
//!   month, day of week, hour). The closure under the table's exact
//!   functional dependencies adds the columns the derived ones
//!   determine.
//! - **Sub-period columns.** A grain factor that subdivides the
//!   axis's period: an integer column on a consecutive range crossed
//!   with nearly every axis period (an hour-of-day column beside a
//!   date, where hour and date together identify rows).
//! - **Recodings.** One side of a verified one-to-one duplicate
//!   encoding between a label and its code, or between two labels.
//!   A numeric level code is kept (it carries the order of the
//!   levels); otherwise the label is.
//!
//! Axis-derived and sub-period columns get the `timestamp_part`
//! role, with the evidence in the frame section and the column card.
//! All three kinds are excluded as subjects of later passes (targets,
//! series, measures, entities). Axis-derived and sub-period columns
//! remain usable as predictors and groupings, except where a
//! combination lies entirely inside the calendar family. Redundant
//! recodings are excluded as inputs too.

use crate::frame::{axis_ticks_micros, CardFacts, FrameModel, MICROS_PER_DAY};
use onceover_core::dateutil;
use onceover_core::TypedTable;
use onceover_deps::hashutil;
use onceover_report::{Finding, ProfileHints};
use std::collections::{BTreeSet, HashMap};

/// Share of ticked rows that must lie in axis (or calendar key)
/// groups of two or more rows before "determined by the axis" is
/// informative.
const MIN_REPEAT_SHARE: f64 = 0.5;
/// Columns tested against the axis, at most (each test is one pass
/// over the rows).
const MAX_TESTED: usize = 128;
/// A sub-period column takes at most this many values ...
const MAX_SUB_PERIOD_LEVELS: usize = 60;
/// ... and its cross with the axis's periods is at least this full.
const MIN_SUB_PERIOD_CROSS: f64 = 0.95;

/// One column found to restate others, with the evidence in words.
#[derive(Clone, Debug)]
pub struct Derived {
    pub col: usize,
    pub evidence: String,
}

/// The derived-column basis of a table.
#[derive(Clone, Debug, Default)]
pub struct Basis {
    /// The axis's own columns (its source column or parts).
    pub axis_cols: Vec<usize>,
    /// Columns the axis determines (directly, through a calendar
    /// key, or through an exact dependency on derived columns).
    pub axis_derived: Vec<Derived>,
    /// Grain factors that subdivide the axis's period.
    pub sub_period: Vec<Derived>,
    /// Redundant sides of one-to-one recodings: (redundant, kept).
    pub recodings: Vec<(usize, usize)>,
    /// Missingness indicators: (indicator, the column whose missing
    /// rows one of its levels marks exactly).
    pub indicators: Vec<(usize, usize)>,
}

impl Basis {
    /// The calendar family: the axis's columns, the axis-derived
    /// columns and the sub-period columns. A combination entirely
    /// inside it restates the calendar.
    pub fn calendar_family(&self) -> Vec<usize> {
        let mut out: BTreeSet<usize> = self.axis_cols.iter().copied().collect();
        out.extend(self.axis_derived.iter().map(|d| d.col));
        out.extend(self.sub_period.iter().map(|d| d.col));
        out.into_iter().collect()
    }

    /// Every column that is no pass's subject.
    pub fn not_subject(&self) -> Vec<usize> {
        let mut out: BTreeSet<usize> = self.axis_derived.iter().map(|d| d.col).collect();
        out.extend(self.sub_period.iter().map(|d| d.col));
        out.extend(self.recodings.iter().map(|r| r.0));
        out.extend(self.indicators.iter().map(|r| r.0));
        out.into_iter().collect()
    }

    /// Columns that are no pass's input either.
    pub fn not_input(&self) -> Vec<usize> {
        let mut out: Vec<usize> = self.recodings.iter().map(|r| r.0).collect();
        out.extend(self.indicators.iter().map(|r| r.0));
        out
    }
}

fn declared_role(hints: Option<&ProfileHints>, name: &str) -> bool {
    hints.is_some_and(|h| h.roles.contains_key(name))
}

/// The axis's own columns.
pub fn axis_columns(table: &TypedTable, frame: &FrameModel) -> Vec<usize> {
    let Some(axis) = &frame.info.axis else {
        return Vec::new();
    };
    let mut names: Vec<&str> = axis.parts.iter().map(|s| s.as_str()).collect();
    if let Some(src) = &axis.source_column {
        names.push(src.as_str());
    }
    names
        .into_iter()
        .filter_map(|n| table.names.iter().position(|m| m == n))
        .collect()
}

/// Share of the keyed rows lying in key groups of two or more rows.
fn repeat_share(keys: &[Option<i64>]) -> f64 {
    let mut counts: HashMap<i64, usize> = HashMap::new();
    let mut total = 0usize;
    for k in keys.iter().flatten() {
        *counts.entry(*k).or_default() += 1;
        total += 1;
    }
    if total == 0 {
        return 0.0;
    }
    let repeated: usize = counts.values().filter(|&&c| c >= 2).sum();
    repeated as f64 / total as f64
}

/// Whether `keys` determines column `col` exactly: over the rows that
/// carry a key, each distinct key co-occurs with one value of the
/// column (a null counts as a value).
pub(crate) fn key_determines(table: &TypedTable, keys: &[Option<i64>], col: usize) -> bool {
    let mut seen: HashMap<i64, Vec<u8>> = HashMap::new();
    let mut buf: Vec<u8> = Vec::new();
    for (r, key) in keys.iter().enumerate() {
        let Some(k) = *key else { continue };
        buf.clear();
        if table.cols[col].is_null_at(r) {
            buf.push(0xFE);
        } else {
            buf.push(0x01);
            hashutil::encode_value(&table.cols[col], r, &mut buf);
        }
        match seen.get(&k) {
            Some(prev) => {
                if *prev != buf {
                    return false;
                }
            }
            None => {
                seen.insert(k, buf.clone());
            }
        }
    }
    !seen.is_empty()
}

/// Calendar keys of the axis, each with its name, as per-row codes.
fn calendar_keys(ticks: &[Option<i64>]) -> Vec<(&'static str, Vec<Option<i64>>)> {
    let days: Vec<Option<i64>> = ticks
        .iter()
        .map(|t| t.map(|us| us.div_euclid(MICROS_PER_DAY)))
        .collect();
    let civil = |f: &dyn Fn(i64, i64, i64, i64) -> i64| -> Vec<Option<i64>> {
        days.iter()
            .map(|d| {
                d.map(|day| {
                    let (y, m, dd) = dateutil::civil_from_days(day);
                    f(day, y, m as i64, dd as i64)
                })
            })
            .collect()
    };
    let mut keys = vec![
        ("year", civil(&|_, y, _, _| y)),
        ("quarter", civil(&|_, _, m, _| (m - 1) / 3)),
        ("month of year", civil(&|_, _, m, _| m)),
        ("year and month", civil(&|_, y, m, _| y * 12 + m)),
        ("day of month", civil(&|_, _, _, d| d)),
        (
            "day of week",
            civil(&|day, _, _, _| dateutil::weekday(day) as i64),
        ),
        ("week", civil(&|day, _, _, _| day.div_euclid(7))),
    ];
    let sub_day = ticks
        .iter()
        .flatten()
        .any(|us| us.rem_euclid(MICROS_PER_DAY) != 0);
    if sub_day {
        keys.push((
            "hour of day",
            ticks
                .iter()
                .map(|t| t.map(|us| us.rem_euclid(MICROS_PER_DAY) / 3_600_000_000))
                .collect(),
        ));
        keys.push(("date", days));
    }
    keys
}

/// Phase A, before the detectors: columns the axis determines.
pub fn axis_derived(
    table: &TypedTable,
    frame: &FrameModel,
    facts: &[CardFacts],
    hints: Option<&ProfileHints>,
) -> (Vec<usize>, Vec<Derived>) {
    let axis_cols = axis_columns(table, frame);
    let Some(ticks) = axis_ticks_micros(table, frame) else {
        return (axis_cols, Vec::new());
    };
    let axis_name = frame
        .info
        .axis
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_default();
    let axis_repeats = repeat_share(&ticks) >= MIN_REPEAT_SHARE;
    let keys: Vec<(&'static str, Vec<Option<i64>>)> = calendar_keys(&ticks)
        .into_iter()
        .filter(|(_, k)| repeat_share(k) >= MIN_REPEAT_SHARE)
        .collect();
    let mut out = Vec::new();
    let mut tested = 0usize;
    for (i, role) in frame.roles.iter().enumerate() {
        if axis_cols.contains(&i)
            || declared_role(hints, &table.names[i])
            || matches!(
                role.as_deref(),
                Some("timestamp" | "row_number" | "identifier" | "text" | "timestamp_part")
            )
            || facts[i].distinct < 2
        {
            continue;
        }
        if tested >= MAX_TESTED {
            break;
        }
        tested += 1;
        // A measure the axis or a calendar key determines is a time
        // pattern of the data, not a derivation: only non-measures
        // are tested.
        if axis_repeats && role.as_deref() != Some("measure") && key_determines(table, &ticks, i) {
            out.push(Derived {
                col: i,
                evidence: format!("determined by {axis_name}: one value per {axis_name} value"),
            });
            continue;
        }
        if role.as_deref() == Some("measure") {
            continue;
        }
        if let Some((name, _)) = keys.iter().find(|(_, k)| key_determines(table, k, i)) {
            out.push(Derived {
                col: i,
                evidence: format!("determined by the {name} of {axis_name}"),
            });
        }
    }
    (axis_cols, out)
}

/// Phase B, after the dependency pass and the grain: the closure of
/// the axis-derived columns under exact dependencies, and the
/// sub-period grain factors.
pub fn close_and_sub_period(
    basis: &mut Basis,
    table: &TypedTable,
    frame: &FrameModel,
    facts: &[CardFacts],
    exact_fds: &[(Vec<usize>, usize)],
    hints: Option<&ProfileHints>,
) {
    let skip = |i: usize, role: Option<&str>| {
        declared_role(hints, &table.names[i])
            || matches!(
                role,
                Some("timestamp" | "row_number" | "identifier" | "text" | "measure")
            )
    };
    if !basis.axis_derived.is_empty() {
        loop {
            let family: BTreeSet<usize> = basis.calendar_family().into_iter().collect();
            let mut grew = false;
            for (lhs, rhs) in exact_fds {
                if family.contains(rhs)
                    || skip(*rhs, frame.roles[*rhs].as_deref())
                    || !lhs.iter().all(|c| family.contains(c))
                {
                    continue;
                }
                let names: Vec<&str> = lhs.iter().map(|&c| table.names[c].as_str()).collect();
                basis.axis_derived.push(Derived {
                    col: *rhs,
                    evidence: format!(
                        "determined by {}, which the axis determines",
                        names.join(", ")
                    ),
                });
                grew = true;
            }
            if !grew {
                break;
            }
        }
    }
    let (Some(axis), Some(grain)) = (&frame.info.axis, &frame.info.grain) else {
        return;
    };
    let others: Vec<_> = grain
        .factorization
        .iter()
        .filter(|f| f.column != axis.name)
        .collect();
    let Some(axis_factor) = grain.factorization.iter().find(|f| f.column == axis.name) else {
        return;
    };
    if others.len() != 1 {
        return;
    }
    let f = others[0];
    let Some(i) = table.names.iter().position(|n| *n == f.column) else {
        return;
    };
    if skip(i, None) || basis.calendar_family().contains(&i) || !facts[i].integral {
        return;
    }
    let (Some(lo), Some(hi)) = (facts[i].numeric_min, facts[i].numeric_max) else {
        return;
    };
    let span = (hi - lo) as usize + 1;
    let cross = table.n as f64 / (axis_factor.distinct * f.distinct).max(1) as f64;
    if f.distinct <= MAX_SUB_PERIOD_LEVELS && span == f.distinct && cross >= MIN_SUB_PERIOD_CROSS {
        basis.sub_period.push(Derived {
            col: i,
            evidence: format!(
                "subdivides each {} period: {} consecutive values crossed with {:.0}% of the {} periods",
                axis.name,
                f.distinct,
                100.0 * cross,
                axis_factor.distinct
            ),
        });
    }
}

/// Phase C, after the duplicate-encodings pass: the redundant side
/// of every exact one-to-one recoding. A numeric level code (integers
/// on a consecutive range of at most `MAX_LEVEL_CODE` values) is
/// kept over its label: the code carries the order of the levels,
/// which models use and the label does not. Otherwise the label
/// (string) side is kept, and between two columns of the same kind
/// the first in table order.
pub fn recodings(table: &TypedTable, findings: &[Finding]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for f in findings {
        if f.finding_type.as_deref() != Some("duplicate_encoding") {
            continue;
        }
        let one_to_one = f.evidence.iter().any(|e| {
            e.metrics.get("form").and_then(|v| v.as_str()) == Some("duplicate_encoding")
                && e.metrics.get("exact").and_then(|v| v.as_bool()) == Some(true)
                && e.metrics.get("exceptions").and_then(|v| v.as_u64()) == Some(0)
        });
        if !one_to_one || f.subject.columns.len() != 2 {
            continue;
        }
        let idx: Vec<usize> = f
            .subject
            .columns
            .iter()
            .filter_map(|c| table.names.iter().position(|n| n == c))
            .collect();
        let [a, b] = idx[..] else { continue };
        let label = |i: usize| table.cols[i].is_string();
        let code = |i: usize| level_code(table, i);
        // Two numeric columns paired one-to-one are a numeric
        // relationship (an affine map, a lookup) that the identity
        // and dependency passes state; only a label beside its code,
        // or two labels, is a recoding.
        let (keep, drop) = match (label(a), label(b)) {
            (false, false) => continue,
            (false, true) if code(a) => (a, b),
            (true, false) if code(b) => (b, a),
            (false, true) => (b, a),
            (true, false) => (a, b),
            (true, true) => (a.min(b), a.max(b)),
        };
        let taken = |c: usize| out.iter().any(|(d, k)| *d == c || *k == c);
        if taken(drop) {
            continue;
        }
        out.push((drop, keep));
    }
    out
}

/// Most values a numeric column may take to count as a level code.
const MAX_LEVEL_CODE: usize = 32;

/// Whether a column is a numeric level code: integer values on a
/// consecutive range of at most `MAX_LEVEL_CODE` values, every value
/// of the range present.
fn level_code(table: &TypedTable, col: usize) -> bool {
    if !table.cols[col].is_numeric() {
        return false;
    }
    let mut values: BTreeSet<i64> = BTreeSet::new();
    for r in 0..table.n {
        let Some(v) = table.cols[col].numeric_at(r) else {
            continue;
        };
        if v.fract() != 0.0 || !v.is_finite() {
            return false;
        }
        values.insert(v as i64);
        if values.len() > MAX_LEVEL_CODE {
            return false;
        }
    }
    match (values.first(), values.last()) {
        (Some(lo), Some(hi)) => (hi - lo + 1) as usize == values.len(),
        _ => false,
    }
}

/// Most two-valued columns tested as missingness indicators.
const MAX_INDICATOR_CANDIDATES: usize = 64;

/// Phase A: missingness indicators. A flag column with no
/// missing values of its own, one of whose levels occurs on exactly
/// the rows where another column is missing (a `t3_measured` flag
/// beside `t3`), restates that column's missingness. Returns
/// (indicator, column, evidence).
pub fn missingness_indicators(
    table: &TypedTable,
    frame: &FrameModel,
    facts: &[CardFacts],
    hints: Option<&ProfileHints>,
) -> Vec<(usize, usize, String)> {
    let with_nulls: Vec<usize> = (0..table.cols.len())
        .filter(|&c| facts[c].null_count > 0 && facts[c].null_count < table.n)
        .collect();
    if with_nulls.is_empty() {
        return Vec::new();
    }
    let candidates: Vec<usize> = (0..table.cols.len())
        .filter(|&c| {
            // Flags only (a boolean or a 0/1 column): a two-level
            // category (legacy / current) that coincides with another
            // column's missing rows is a dimension with relations of
            // its own, not a recording of the missingness.
            facts[c].distinct == 2
                && frame.roles[c].as_deref() == Some("flag")
                && facts[c].null_count == 0
                && !declared_role(hints, &table.names[c])
        })
        .take(MAX_INDICATOR_CANDIDATES)
        .collect();
    let mut out = Vec::new();
    for f in candidates {
        // The flag's per-row level, as the encoding of its value.
        let mut first: Option<Vec<u8>> = None;
        let level: Vec<bool> = (0..table.n)
            .map(|r| {
                let mut buf = Vec::new();
                hashutil::encode_value(&table.cols[f], r, &mut buf);
                match &first {
                    Some(v) => *v == buf,
                    None => {
                        first = Some(buf);
                        true
                    }
                }
            })
            .collect();
        for &x in &with_nulls {
            if x == f || facts[x].null_count < 2 {
                continue;
            }
            // One level of the flag must coincide with x missing.
            let null_at = |r: usize| table.cols[x].is_null_at(r);
            let same = (0..table.n).all(|r| level[r] == null_at(r));
            let opposite = (0..table.n).all(|r| level[r] != null_at(r));
            if same || opposite {
                out.push((
                    f,
                    x,
                    format!(
                        "missingness indicator of {}: one of its two values occurs on exactly the {} rows where {} is missing",
                        table.names[x], facts[x].null_count, table.names[x]
                    ),
                ));
                break;
            }
        }
    }
    out
}
