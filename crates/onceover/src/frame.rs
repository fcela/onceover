//! The frame model, step 1: semantic column roles and time-axis
//! assembly, computed once over the extracted table before any
//! detector runs.
//!
//! Roles answer "what kind of thing is this column" beyond dtype and
//! cardinality: a restarting row counter, the parts of one timestamp,
//! and a pollutant measurement are different kinds of column, and the
//! detectors downstream should not treat them alike. Every assignment
//! carries the evidence for it, is disclosed in the column card and
//! the report's `frame` section, and can be overridden per column
//! via `hints.roles`.
//!
//! The axis is assembled in a fixed precedence — a declared
//! `hints.time` column, a native date/datetime column, a string
//! column that parses as timestamps, a set of timestamp-part
//! columns, and finally a single year-like integer column recorded
//! as a low-confidence candidate. When the axis comes from strings
//! or parts it is materialized as a real date/datetime column (in a
//! cloned DataFrame built by [`build_temporal_df`]) so the temporal
//! passes consume it through their existing axis selection.

use onceover_core::dateutil;
use onceover_core::{TypedCol, TypedTable};
use onceover_report::{FrameAxis, FrameColumnRole, FrameInfo, ProfileHints};
use polars::prelude::*;
use std::collections::{BTreeSet, HashMap};

/// The semantic role vocabulary of the frame model.
pub const ROLE_VOCABULARY: [&str; 7] = [
    "identifier",
    "index",
    "timestamp",
    "timestamp_part",
    "dimension",
    "measure",
    "text",
];

/// Card-level facts the role rules read, kept separate from the
/// report card so this module does not depend on card construction.
pub struct CardFacts {
    /// The card's dtype-and-cardinality role (categorical, ...).
    pub card_role: String,
    pub distinct: usize,
    pub null_count: usize,
    pub numeric_min: Option<f64>,
    pub numeric_max: Option<f64>,
}

/// The computed frame model: the serializable `frame` section plus
/// the working state the pipeline needs (per-column roles, the axis,
/// and the ticks of a materialized axis).
pub struct FrameModel {
    pub info: FrameInfo,
    /// Semantic role per column index (None = no assignment).
    pub roles: Vec<Option<String>>,
    /// Name of the axis column in the table the temporal passes
    /// receive (the materialized name for assembled axes).
    pub axis_name: Option<String>,
    /// hint | native | string_parse | timestamp_parts | year_candidate
    pub axis_source: Option<String>,
    /// The part column names, when the axis was assembled from parts.
    pub axis_parts: Vec<String>,
    /// Column index replaced by the materialized axis, when the axis
    /// comes from strings or parts.
    pub axis_replace_index: Option<usize>,
    /// Per-row axis ticks (microseconds when `axis_is_datetime`,
    /// days otherwise); present exactly when `axis_replace_index` is.
    pub axis_ticks: Option<Vec<Option<i64>>>,
    pub axis_is_datetime: bool,
}

const YEAR_RANGE: (i64, i64) = (1900, 2100);
/// Parse / joint-presence bar for assembled axes: nearly all values
/// must participate before a column or a part set is trusted as the
/// frame's time axis.
const ASSEMBLY_BAR: f64 = 0.99;
/// Smallest support the structural rules engage on.
const MIN_SUPPORT: usize = 8;

// ---------------------------------------------------------------------
// timestamp parts
// ---------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PartKind {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

impl PartKind {
    fn of_name(name: &str) -> Option<PartKind> {
        match name.to_ascii_lowercase().as_str() {
            "year" => Some(PartKind::Year),
            "month" => Some(PartKind::Month),
            "day" => Some(PartKind::Day),
            "hour" => Some(PartKind::Hour),
            "minute" => Some(PartKind::Minute),
            "second" => Some(PartKind::Second),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            PartKind::Year => "year",
            PartKind::Month => "month",
            PartKind::Day => "day",
            PartKind::Hour => "hour",
            PartKind::Minute => "minute",
            PartKind::Second => "second",
        }
    }
    fn range(self) -> (i64, i64) {
        match self {
            PartKind::Year => YEAR_RANGE,
            PartKind::Month => (1, 12),
            PartKind::Day => (1, 31),
            PartKind::Hour => (0, 23),
            PartKind::Minute | PartKind::Second => (0, 59),
        }
    }
}

struct PartSet {
    /// (kind, column index), sorted by kind.
    cols: Vec<(PartKind, usize)>,
    ticks: Vec<Option<i64>>,
    is_datetime: bool,
}

fn int_values(col: &TypedCol) -> Option<Vec<Option<i64>>> {
    match col {
        TypedCol::I64 { vals } => Some(vals.iter().map(|v| v.copied()).collect()),
        TypedCol::U64 { vals } => Some(
            vals.iter()
                .map(|v| v.copied().and_then(|x| i64::try_from(x).ok()))
                .collect(),
        ),
        _ => None,
    }
}

/// Detect a timestamp-part set: integer columns named year / month /
/// day / hour / minute / second, values in plausible ranges, jointly
/// non-null on at least 99% of rows, whose assembled calendar values
/// are real dates on at least 99% of complete rows. Requires year
/// and month at minimum and at least 8 distinct assembled values.
fn detect_part_set(table: &TypedTable) -> Option<PartSet> {
    if table.n < MIN_SUPPORT {
        return None;
    }
    let mut by_kind: Vec<(PartKind, usize, Vec<Option<i64>>)> = Vec::new();
    for (i, col) in table.cols.iter().enumerate() {
        let Some(kind) = PartKind::of_name(&table.names[i]) else {
            continue;
        };
        let Some(vals) = int_values(col) else {
            continue;
        };
        let (lo, hi) = kind.range();
        if vals.iter().flatten().all(|&v| (lo..=hi).contains(&v)) {
            by_kind.push((kind, i, vals));
        }
    }
    by_kind.sort_by_key(|(k, _, _)| *k);
    by_kind.dedup_by_key(|(k, _, _)| *k);
    let get = |kind: PartKind| by_kind.iter().find(|(k, _, _)| *k == kind);
    get(PartKind::Year)?;
    get(PartKind::Month)?;

    let present = (0..table.n)
        .filter(|&r| by_kind.iter().all(|(_, _, vals)| vals[r].is_some()))
        .count();
    if (present as f64) < ASSEMBLY_BAR * table.n as f64 {
        return None;
    }
    let is_datetime = [PartKind::Hour, PartKind::Minute, PartKind::Second]
        .iter()
        .any(|k| get(*k).is_some());
    let part =
        |kind: PartKind, r: usize| -> Option<i64> { get(kind).and_then(|(_, _, vals)| vals[r]) };
    let mut ticks = Vec::with_capacity(table.n);
    let mut valid = 0usize;
    let mut distinct = BTreeSet::new();
    for r in 0..table.n {
        let complete = by_kind.iter().all(|(_, _, vals)| vals[r].is_some());
        let tick = complete.then(|| {
            let y = part(PartKind::Year, r)?;
            let mo = part(PartKind::Month, r)?;
            let d = part(PartKind::Day, r).unwrap_or(1);
            let days = dateutil::days_from_civil(y, mo, d);
            if dateutil::civil_from_days(days) != (y, mo as u32, d as u32) {
                return None;
            }
            if is_datetime {
                let secs = part(PartKind::Hour, r).unwrap_or(0) * 3600
                    + part(PartKind::Minute, r).unwrap_or(0) * 60
                    + part(PartKind::Second, r).unwrap_or(0);
                Some(days * 86_400_000_000 + secs * 1_000_000)
            } else {
                Some(days)
            }
        });
        let tick = tick.flatten();
        if let Some(t) = tick {
            valid += 1;
            distinct.insert(t);
        }
        ticks.push(tick);
    }
    if (valid as f64) < ASSEMBLY_BAR * (present as f64) || distinct.len() < MIN_SUPPORT {
        return None;
    }
    Some(PartSet {
        cols: by_kind.iter().map(|(k, i, _)| (*k, *i)).collect(),
        ticks,
        is_datetime,
    })
}

// ---------------------------------------------------------------------
// string timestamp parsing
// ---------------------------------------------------------------------

/// Parse one timestamp string: "YYYY-MM-DD" or "YYYY/MM/DD", with an
/// optional " HH:MM[:SS[.fraction]]" (or 'T' separator) and an
/// optional trailing 'Z'. Returns (ticks, has_time): ticks are days
/// since the epoch, or microseconds when the value carries a time.
/// Calendar validity is checked by round-tripping the civil date.
pub fn parse_timestamp(text: &str) -> Option<(i64, bool)> {
    let s = text.trim();
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    let digit = |i: usize| -> Option<i64> {
        bytes
            .get(i)
            .filter(|b| b.is_ascii_digit())
            .map(|b| (b - b'0') as i64)
    };
    let num = |start: usize, len: usize| -> Option<i64> {
        let mut v = 0i64;
        for k in 0..len {
            v = v * 10 + digit(start + k)?;
        }
        Some(v)
    };
    let sep = bytes[4];
    if sep != b'-' && sep != b'/' {
        return None;
    }
    let year = num(0, 4)?;
    if bytes[7] != sep {
        return None;
    }
    let month = num(5, 2)?;
    let day = num(8, 2)?;
    let days = dateutil::days_from_civil(year, month, day);
    if dateutil::civil_from_days(days) != (year, month as u32, day as u32) {
        return None;
    }
    if bytes.len() == 10 {
        return Some((days, false));
    }
    if bytes[10] != b' ' && bytes[10] != b'T' {
        return None;
    }
    let time = s[11..].strip_suffix('Z').unwrap_or(&s[11..]);
    let (hms, frac) = match time.split_once('.') {
        Some((h, f)) => {
            if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            (h, f)
        }
        None => (time, ""),
    };
    let parts: Vec<&str> = hms.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let field = |p: &str| -> Option<i64> {
        if p.len() != 2 || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        p.parse().ok()
    };
    let hour = field(parts[0])?;
    let minute = field(parts[1])?;
    let second = if parts.len() == 3 {
        field(parts[2])?
    } else {
        0
    };
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let mut micros = (hour * 3600 + minute * 60 + second) * 1_000_000;
    if !frac.is_empty() {
        let mut f: String = frac.chars().take(6).collect();
        while f.len() < 6 {
            f.push('0');
        }
        micros += f.parse::<i64>().ok()?;
    }
    Some((days * 86_400_000_000 + micros, true))
}

/// Try to read a whole string column as timestamps. Returns the
/// per-row ticks and whether the axis is sub-day, when at least 99%
/// of non-null values parse and at least 8 distinct values result.
/// A strided sample is checked first so free-text columns cost a
/// bounded probe, not a full parse.
fn parse_string_column(col: &TypedCol) -> Option<(Vec<Option<i64>>, bool)> {
    let n = col.len();
    let nonnull: Vec<usize> = (0..n).filter(|&r| col.str_at(r).is_some()).collect();
    if nonnull.len() < MIN_SUPPORT {
        return None;
    }
    let stride = (nonnull.len() / 2048).max(1);
    let sample: Vec<usize> = nonnull.iter().copied().step_by(stride).collect();
    let sample_ok = sample
        .iter()
        .filter(|&&r| parse_timestamp(col.str_at(r).expect("non-null")).is_some())
        .count();
    if (sample_ok as f64) < ASSEMBLY_BAR * (sample.len() as f64) {
        return None;
    }
    let mut raw: Vec<Option<(i64, bool)>> = vec![None; n];
    let mut parsed = 0usize;
    let mut any_time = false;
    for &r in &nonnull {
        if let Some(p) = parse_timestamp(col.str_at(r).expect("non-null")) {
            raw[r] = Some(p);
            parsed += 1;
            any_time |= p.1;
        }
    }
    if (parsed as f64) < ASSEMBLY_BAR * (nonnull.len() as f64) {
        return None;
    }
    let ticks: Vec<Option<i64>> = raw
        .iter()
        .map(|p| {
            p.map(|(t, has_time)| {
                if any_time && !has_time {
                    t * 86_400_000_000
                } else {
                    t
                }
            })
        })
        .collect();
    let distinct: BTreeSet<i64> = ticks.iter().flatten().copied().collect();
    if distinct.len() < MIN_SUPPORT {
        return None;
    }
    Some((ticks, any_time))
}

// ---------------------------------------------------------------------
// index detection
// ---------------------------------------------------------------------

fn as_i128(col: &TypedCol, r: usize) -> Option<i128> {
    match col {
        TypedCol::I64 { vals } => vals.at(r).map(i128::from),
        TypedCol::U64 { vals } => vals.at(r).map(i128::from),
        _ => None,
    }
}

/// A whole-column consecutive run in row order: every step is +1
/// (or -1). Returns the start value.
fn monotonic_run(table: &TypedTable, ci: usize) -> Option<i128> {
    if table.n < MIN_SUPPORT {
        return None;
    }
    let start = as_i128(&table.cols[ci], 0)?;
    let mut ascending = true;
    let mut descending = true;
    for r in 1..table.n {
        let v = as_i128(&table.cols[ci], r)?;
        ascending &= v == start + r as i128;
        descending &= v == start - r as i128;
        if !ascending && !descending {
            return None;
        }
    }
    (ascending || descending).then_some(start)
}

/// A counter restarting per group: within each level of a grouping
/// column, the values in row order are exactly start, start+1, ...
/// with the same start (0 or 1) for every group, at least 2 groups
/// and at least 3 rows per group. Returns the grouping column index.
fn restart_run(table: &TypedTable, ci: usize, group_cols: &[usize]) -> Option<usize> {
    if table.n < MIN_SUPPORT {
        return None;
    }
    'groups: for &g in group_cols {
        if g == ci {
            continue;
        }
        let mut next: HashMap<Vec<u8>, i128> = HashMap::new();
        let mut sizes: HashMap<Vec<u8>, usize> = HashMap::new();
        let mut starts: HashMap<Vec<u8>, i128> = HashMap::new();
        for r in 0..table.n {
            let Some(v) = as_i128(&table.cols[ci], r) else {
                continue 'groups;
            };
            let mut key = Vec::new();
            onceover_deps::hashutil::encode_value(&table.cols[g], r, &mut key);
            match next.get(&key) {
                None => {
                    if v != 0 && v != 1 {
                        continue 'groups;
                    }
                    next.insert(key.clone(), v + 1);
                    starts.insert(key.clone(), v);
                    *sizes.entry(key).or_insert(0) += 1;
                }
                Some(&expected) => {
                    if v != expected {
                        continue 'groups;
                    }
                    next.insert(key.clone(), v + 1);
                    *sizes.entry(key).or_insert(0) += 1;
                }
            }
        }
        if next.len() < 2 {
            continue;
        }
        let start0 = *starts.values().next().expect("non-empty");
        if starts.values().any(|&s| s != start0) {
            continue;
        }
        if sizes.values().any(|&s| s < 3) {
            continue;
        }
        return Some(g);
    }
    None
}

// ---------------------------------------------------------------------
// the model
// ---------------------------------------------------------------------

/// Grouping columns eligible for restart detection: categorical /
/// boolean card roles with at least 2 levels, at most 8 of them, in
/// column order.
fn grouping_candidates(facts: &[CardFacts]) -> Vec<usize> {
    facts
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            matches!(f.card_role.as_str(), "categorical" | "boolean") && f.distinct >= 2
        })
        .map(|(i, _)| i)
        .take(8)
        .collect()
}

/// Compute the frame model: semantic roles for every column and the
/// assembled time axis.
pub fn analyze_frame(
    table: &TypedTable,
    facts: &[CardFacts],
    hints: Option<&ProfileHints>,
) -> FrameModel {
    let n_cols = table.cols.len();
    let part_set = detect_part_set(table);
    let part_of: Vec<Option<PartKind>> = (0..n_cols)
        .map(|i| {
            part_set
                .as_ref()
                .and_then(|ps| ps.cols.iter().find(|(_, ci)| *ci == i).map(|(k, _)| *k))
        })
        .collect();
    let groups = grouping_candidates(facts);

    // ---- roles ----
    let mut roles: Vec<Option<String>> = vec![None; n_cols];
    let mut evidence: Vec<String> = vec![String::new(); n_cols];
    for i in 0..n_cols {
        let name = &table.names[i];
        let col = &table.cols[i];
        let fact = &facts[i];
        // (a) declared override
        if let Some(declared) = hints.and_then(|h| h.roles.get(name)) {
            if ROLE_VOCABULARY.contains(&declared.as_str()) {
                roles[i] = Some(declared.clone());
                evidence[i] = "declared by hints.roles".into();
                continue;
            }
        }
        // (b) native temporal dtype
        if matches!(col, TypedCol::Date { .. } | TypedCol::Datetime { .. }) {
            roles[i] = Some("timestamp".into());
            evidence[i] = format!("native {} column", table.dtypes[i]);
            continue;
        }
        // (c) timestamp part
        if let Some(kind) = part_of[i] {
            roles[i] = Some("timestamp_part".into());
            evidence[i] = format!(
                "the {} part of the timestamp assembled from {}",
                kind.name(),
                part_set
                    .as_ref()
                    .expect("part set present")
                    .cols
                    .iter()
                    .map(|(k, _)| k.name())
                    .collect::<Vec<_>>()
                    .join("/")
            );
            continue;
        }
        // (d) opaque / empty columns carry no semantic role
        if matches!(fact.card_role.as_str(), "all_null" | "other") {
            continue;
        }
        // (e) index: a consecutive run, whole-column or restarting
        // per group
        if matches!(col, TypedCol::I64 { .. } | TypedCol::U64 { .. }) && fact.null_count == 0 {
            if let Some(start) = monotonic_run(table, i) {
                roles[i] = Some("index".into());
                evidence[i] = format!(
                    "values run consecutively from {start} in row order ({} rows)",
                    table.n
                );
                continue;
            }
            let prefilter = matches!(fact.numeric_min, Some(m) if m == 0.0 || m == 1.0);
            let restart = if prefilter {
                restart_run(table, i, &groups)
            } else {
                None
            };
            if let Some(g) = restart {
                roles[i] = Some("index".into());
                evidence[i] = format!(
                    "values restart and run consecutively within each level of {} ({} levels)",
                    table.names[g], facts[g].distinct
                );
                continue;
            }
        }
        // (f) identifier: every non-null value unique, label-typed
        let n_nonnull = table.n - fact.null_count;
        if fact.distinct == n_nonnull
            && n_nonnull >= MIN_SUPPORT
            && (col.is_string() || matches!(col, TypedCol::I64 { .. } | TypedCol::U64 { .. }))
        {
            roles[i] = Some("identifier".into());
            evidence[i] = format!(
                "{} distinct values in {} non-null rows; every value unique",
                fact.distinct, n_nonnull
            );
            continue;
        }
        // (g) dimension: low-cardinality grouping columns
        if matches!(fact.card_role.as_str(), "categorical" | "boolean")
            || matches!(col, TypedCol::Bool { .. })
        {
            roles[i] = Some("dimension".into());
            evidence[i] = format!("{} distinct values", fact.distinct);
            continue;
        }
        // (h/i) the rest: numeric measures, text
        if col.is_numeric() {
            roles[i] = Some("measure".into());
            evidence[i] = "numeric column not otherwise classified".into();
        } else if col.is_string() {
            roles[i] = Some("text".into());
            evidence[i] = "text column not otherwise classified".into();
        }
    }

    // ---- axis ----
    let mut axis: Option<FrameAxis> = None;
    let mut axis_name: Option<String> = None;
    let mut axis_source: Option<String> = None;
    let mut axis_parts: Vec<String> = Vec::new();
    let mut axis_replace_index: Option<usize> = None;
    let mut axis_ticks: Option<Vec<Option<i64>>> = None;
    let mut axis_is_datetime = false;

    // 1. declared axis
    if let Some(hint) = hints.and_then(|h| h.time.as_ref()) {
        if let Some(i) = table.names.iter().position(|n| n == hint) {
            match &table.cols[i] {
                TypedCol::Date { .. }
                | TypedCol::Datetime { .. }
                | TypedCol::I64 { .. }
                | TypedCol::U64 { .. } => {
                    axis = Some(FrameAxis {
                        name: hint.clone(),
                        source: "hint".into(),
                        description: format!("declared by hints.time: the column '{hint}'"),
                        confidence: "declared".into(),
                        parts: Vec::new(),
                        source_column: Some(hint.clone()),
                    });
                    axis_name = Some(hint.clone());
                    axis_source = Some("hint".into());
                }
                col if col.is_string() => {
                    if let Some((ticks, is_dt)) = parse_string_column(col) {
                        axis = Some(FrameAxis {
                            name: hint.clone(),
                            source: "hint".into(),
                            description: format!(
                                "declared by hints.time: the string column '{hint}', parsed as timestamps"
                            ),
                            confidence: "declared".into(),
                            parts: Vec::new(),
                            source_column: Some(hint.clone()),
                        });
                        axis_name = Some(hint.clone());
                        axis_source = Some("hint".into());
                        axis_replace_index = Some(i);
                        axis_ticks = Some(ticks);
                        axis_is_datetime = is_dt;
                    }
                }
                _ => {}
            }
        }
    }
    // 2. native date / datetime
    if axis.is_none() {
        if let Some(i) = table
            .cols
            .iter()
            .position(|c| matches!(c, TypedCol::Date { .. } | TypedCol::Datetime { .. }))
        {
            let name = table.names[i].clone();
            axis = Some(FrameAxis {
                name: name.clone(),
                source: "native".into(),
                description: format!("the {} column '{name}'", table.dtypes[i]),
                confidence: "inferred".into(),
                parts: Vec::new(),
                source_column: Some(name.clone()),
            });
            axis_name = Some(name);
            axis_source = Some("native".into());
        }
    }
    // 3. parseable string column
    if axis.is_none() {
        for (i, col) in table.cols.iter().enumerate() {
            if !col.is_string() {
                continue;
            }
            if let Some((ticks, is_dt)) = parse_string_column(col) {
                let name = table.names[i].clone();
                axis = Some(FrameAxis {
                    name: name.clone(),
                    source: "string_parse".into(),
                    description: format!("the string column '{name}', parsed as timestamps"),
                    confidence: "inferred".into(),
                    parts: Vec::new(),
                    source_column: Some(name.clone()),
                });
                axis_name = Some(name);
                axis_source = Some("string_parse".into());
                axis_replace_index = Some(i);
                axis_ticks = Some(ticks);
                axis_is_datetime = is_dt;
                break;
            }
        }
    }
    // 4. timestamp parts
    if axis.is_none() {
        if let Some(ps) = &part_set {
            let names: Vec<String> = ps.cols.iter().map(|(k, _)| k.name().to_string()).collect();
            let joined = names.join("/");
            let year_idx = ps
                .cols
                .iter()
                .find(|(k, _)| *k == PartKind::Year)
                .map(|(_, i)| *i)
                .expect("part set requires a year");
            axis = Some(FrameAxis {
                name: joined.clone(),
                source: "timestamp_parts".into(),
                description: format!("a timestamp assembled from {joined}"),
                confidence: "inferred".into(),
                parts: names.clone(),
                source_column: None,
            });
            axis_name = Some(joined);
            axis_source = Some("timestamp_parts".into());
            axis_parts = names;
            axis_replace_index = Some(year_idx);
            axis_ticks = Some(ps.ticks.clone());
            axis_is_datetime = ps.is_datetime;
        }
    }
    // 5. year-like integer candidate, recorded at low confidence
    if axis.is_none() {
        for (i, col) in table.cols.iter().enumerate() {
            if !matches!(col, TypedCol::I64 { .. } | TypedCol::U64 { .. }) {
                continue;
            }
            let fact = &facts[i];
            let in_range = matches!(
                (fact.numeric_min, fact.numeric_max),
                (Some(lo), Some(hi)) if lo >= YEAR_RANGE.0 as f64 && hi <= YEAR_RANGE.1 as f64
            );
            if in_range && fact.distinct >= 2 {
                let name = table.names[i].clone();
                axis = Some(FrameAxis {
                    name: name.clone(),
                    source: "year_candidate".into(),
                    description: format!(
                        "the integer column '{name}' holds plausible years ({}..={}); recorded as a candidate axis at low confidence",
                        fact.numeric_min.map(|v| v as i64).unwrap_or(0),
                        fact.numeric_max.map(|v| v as i64).unwrap_or(0)
                    ),
                    confidence: "low".into(),
                    parts: Vec::new(),
                    source_column: Some(name.clone()),
                });
                axis_name = Some(name);
                axis_source = Some("year_candidate".into());
                break;
            }
        }
    }

    // A string column the axis was parsed from is a timestamp,
    // whatever its cardinality suggested: the axis record and the
    // role must not contradict each other. A declared role wins.
    if let (Some(i), Some(src)) = (axis_replace_index, axis_source.as_deref()) {
        if (src == "string_parse" || src == "hint")
            && evidence[i] != "declared by hints.roles"
            && table.cols[i].is_string()
        {
            roles[i] = Some("timestamp".into());
            evidence[i] = if src == "hint" {
                "declared by hints.time; its values parse as timestamps".into()
            } else {
                "its values parse as timestamps; the frame's time axis".into()
            };
        }
    }

    let info = FrameInfo {
        roles: (0..n_cols)
            .map(|i| FrameColumnRole {
                column: table.names[i].clone(),
                role: roles[i].clone(),
                evidence: evidence[i].clone(),
            })
            .collect(),
        axis,
    };
    FrameModel {
        info,
        roles,
        axis_name,
        axis_source,
        axis_parts,
        axis_replace_index,
        axis_ticks,
        axis_is_datetime,
    }
}

/// Materialize the assembled axis: a clone of `df` whose axis
/// source column is replaced, at the same position, by a real
/// date/datetime column carrying the parsed/assembled ticks. Only
/// needed when the axis came from strings or parts; `None` when the
/// profiled frame already carries the axis.
pub fn build_temporal_df(df: &DataFrame, model: &FrameModel) -> Option<DataFrame> {
    let replace = model.axis_replace_index?;
    let ticks = model.axis_ticks.as_ref()?;
    let name = model.axis_name.as_ref()?;
    let axis_series: Series = if model.axis_is_datetime {
        Series::new(name.as_str().into(), ticks.clone())
            .cast(&DataType::Datetime(TimeUnit::Microseconds, None))
            .ok()?
    } else {
        let vals: Vec<Option<i32>> = ticks.iter().map(|t| t.map(|v| v as i32)).collect();
        Series::new(name.as_str().into(), vals)
            .cast(&DataType::Date)
            .ok()?
    };
    let mut columns: Vec<Column> = Vec::with_capacity(df.width());
    for (j, col) in df.columns().iter().enumerate() {
        if j == replace {
            columns.push(axis_series.clone().into());
        } else {
            columns.push(col.clone());
        }
    }
    DataFrame::new_infer_height(columns).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::extract;

    fn facts_for(df: &DataFrame) -> (TypedTable<'_>, Vec<CardFacts>) {
        let table = extract(df);
        let facts = table
            .cols
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let card = onceover_core::column_card(&table.names[i], &table.dtypes[i], col);
                CardFacts {
                    card_role: card.card.role.clone(),
                    distinct: card.card.distinct,
                    null_count: card.card.null_count,
                    numeric_min: card.card.numeric_stats.as_ref().and_then(|s| s.min),
                    numeric_max: card.card.numeric_stats.as_ref().and_then(|s| s.max),
                }
            })
            .collect();
        (table, facts)
    }

    #[test]
    fn parses_timestamp_strings() {
        assert_eq!(parse_timestamp("1970-01-01"), Some((0, false)));
        assert_eq!(
            parse_timestamp("1970-01-02 01:00:00"),
            Some((86_400_000_000 + 3_600_000_000, true))
        );
        assert_eq!(parse_timestamp("2024-02-29"), Some((19_782, false)));
        assert_eq!(parse_timestamp("2023-02-29"), None); // not a leap year
        assert_eq!(parse_timestamp("2023-02-30"), None);
        assert_eq!(parse_timestamp("2023-13-01"), None);
        assert_eq!(parse_timestamp("not a date"), None);
        assert_eq!(parse_timestamp("2023-1-1"), None); // strict widths
        assert!(parse_timestamp("2012-10-02 09:00:00Z").is_some());
        assert!(parse_timestamp("2012-10-02T09:00:00.5").is_some());
    }

    #[test]
    fn beijing_shape_roles_and_axis() {
        // Two stations, a restarting counter, split timestamp parts.
        let n_per = 40usize;
        let mut no = Vec::new();
        let mut year = Vec::new();
        let mut month = Vec::new();
        let mut day = Vec::new();
        let mut hour = Vec::new();
        let mut station = Vec::new();
        let mut pm = Vec::new();
        for s in ["A", "B"] {
            for k in 0..n_per {
                no.push((k + 1) as i64);
                year.push(2016i64);
                month.push(1 + (k / 24) as i64 % 3);
                day.push(1 + (k / 24) as i64);
                hour.push((k % 24) as i64);
                station.push(s);
                pm.push(50.0 + ((k * 37) % 41) as f64);
            }
        }
        let df = DataFrame::new_infer_height(vec![
            Series::new("No".into(), no).into(),
            Series::new("year".into(), year).into(),
            Series::new("month".into(), month).into(),
            Series::new("day".into(), day).into(),
            Series::new("hour".into(), hour).into(),
            Series::new("station".into(), station).into(),
            Series::new("PM2.5".into(), pm).into(),
        ])
        .unwrap();
        let (table, facts) = facts_for(&df);
        let model = analyze_frame(&table, &facts, None);
        let role =
            |name: &str| model.roles[table.names.iter().position(|n| n == name).unwrap()].clone();
        assert_eq!(role("No").as_deref(), Some("index"));
        assert_eq!(role("year").as_deref(), Some("timestamp_part"));
        assert_eq!(role("month").as_deref(), Some("timestamp_part"));
        assert_eq!(role("hour").as_deref(), Some("timestamp_part"));
        assert_eq!(role("station").as_deref(), Some("dimension"));
        assert_eq!(role("PM2.5").as_deref(), Some("measure"));
        let axis = model.info.axis.as_ref().expect("axis assembled");
        assert_eq!(axis.source, "timestamp_parts");
        assert_eq!(axis.name, "year/month/day/hour");
        let tdf = build_temporal_df(&df, &model).expect("temporal frame");
        assert_eq!(
            tdf.column("year/month/day/hour").unwrap().dtype(),
            &DataType::Datetime(TimeUnit::Microseconds, None)
        );
    }

    #[test]
    fn string_axis_and_monotonic_index() {
        let n = 60usize;
        let df = DataFrame::new_infer_height(vec![
            Series::new("row_id".into(), (0..n as i64).collect::<Vec<_>>()).into(),
            Series::new(
                "date".into(),
                (0..n)
                    .map(|k| format!("2024-01-{:02}", 1 + k % 28))
                    .collect::<Vec<_>>(),
            )
            .into(),
            Series::new(
                "value".into(),
                (0..n).map(|k| (k % 9) as f64).collect::<Vec<_>>(),
            )
            .into(),
        ])
        .unwrap();
        let (table, facts) = facts_for(&df);
        let model = analyze_frame(&table, &facts, None);
        assert_eq!(model.roles[0].as_deref(), Some("index"));
        // The parsed string column is a timestamp, not a dimension.
        assert_eq!(model.roles[1].as_deref(), Some("timestamp"));
        let axis = model.info.axis.as_ref().expect("axis parsed");
        assert_eq!(axis.source, "string_parse");
        let tdf = build_temporal_df(&df, &model).expect("temporal frame");
        assert_eq!(tdf.column("date").unwrap().dtype(), &DataType::Date);
    }

    #[test]
    fn hint_role_override_wins() {
        let n = 40usize;
        let df = DataFrame::new_infer_height(vec![
            Series::new("x".into(), (0..n as i64).collect::<Vec<_>>()).into(),
            Series::new(
                "y".into(),
                (0..n).map(|k| (k % 5) as f64).collect::<Vec<_>>(),
            )
            .into(),
        ])
        .unwrap();
        let (table, facts) = facts_for(&df);
        let mut hints = ProfileHints::default();
        hints.roles.insert("x".into(), "measure".into());
        let model = analyze_frame(&table, &facts, Some(&hints));
        assert_eq!(model.roles[0].as_deref(), Some("measure"));
        assert!(model.info.roles[0].evidence.contains("hints.roles"));
    }
}
