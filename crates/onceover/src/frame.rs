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
    /// All non-null values of a numeric column are whole numbers.
    pub integral: bool,
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
    /// hint | native | string_parse | timestamp_parts | year_candidate | week_candidate
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
    Week,
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
            "week" | "iso_week" => Some(PartKind::Week),
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
            PartKind::Week => "week",
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
            PartKind::Week => (1, 53),
            PartKind::Day => (1, 31),
            PartKind::Hour => (0, 23),
            PartKind::Minute | PartKind::Second => (0, 59),
        }
    }
}

/// The Monday of ISO week `w` of year `y`, in days since the epoch,
/// or None when that year has no such week. ISO week 1 is the week
/// containing 4 January; a week belongs to the year of its Thursday,
/// so week 53 exists only in years whose 1 January is a Thursday
/// (or a Wednesday in a leap year).
fn iso_week_monday(y: i64, w: i64) -> Option<i64> {
    if !(1..=53).contains(&w) {
        return None;
    }
    let jan4 = dateutil::days_from_civil(y, 1, 4);
    // Weekday of a day count with 0 = Sunday (1970-01-01, day 0,
    // was a Thursday, hence the +4).
    let weekday = (jan4 + 4).rem_euclid(7);
    let monday = jan4 - (weekday + 6).rem_euclid(7) + 7 * (w - 1);
    let (thursday_year, _, _) = dateutil::civil_from_days(monday + 3);
    (thursday_year == y).then_some(monday)
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
/// week / day / hour / minute / second, values in plausible ranges,
/// jointly non-null on at least 99% of rows, whose assembled
/// calendar values are real dates on at least 99% of complete rows.
/// Requires a year and at least 8 distinct assembled values. With a
/// month part the assembly is the calendar date; with a week part
/// and no month part it is the Monday of the ISO week, and the part
/// set is the year and week columns alone (a day or time part
/// cannot be placed on the week calendar). A week part beside a
/// month part is not part of the set.
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
    let weekly = {
        let get = |kind: PartKind| by_kind.iter().find(|(k, _, _)| *k == kind);
        get(PartKind::Year)?;
        let weekly = get(PartKind::Month).is_none() && get(PartKind::Week).is_some();
        if !weekly {
            get(PartKind::Month)?;
        }
        weekly
    };
    if weekly {
        by_kind.retain(|(k, _, _)| matches!(k, PartKind::Year | PartKind::Week));
    } else {
        by_kind.retain(|(k, _, _)| *k != PartKind::Week);
    }
    let get = |kind: PartKind| by_kind.iter().find(|(k, _, _)| *k == kind);

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
            if weekly {
                let w = part(PartKind::Week, r)?;
                return iso_week_monday(y, w);
            }
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
pub(crate) fn parse_timestamp(text: &str) -> Option<(i64, bool)> {
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
// axis span (step 2): the account states the axis's span and
// frequency for every temporal frame, so both are measured here,
// where the axis is assembled.
// ---------------------------------------------------------------------

pub(crate) const MICROS_PER_DAY: i64 = 86_400_000_000;

/// Convert per-row day ticks to per-row microsecond ticks. The
/// conversion fails as a whole when any value sits outside the
/// range i64 microseconds can represent — |days| above
/// 106,751,991, roughly ±292,000 years — because a partial
/// conversion would drop exactly the extreme values and state a
/// span the data does not have. Callers treat `None` the way they
/// already treat a column with no tick representation at all:
/// the axis is recorded without a measured span.
fn day_ticks_to_micros(ticks: &[Option<i64>]) -> Option<Vec<Option<i64>>> {
    let mut out = Vec::with_capacity(ticks.len());
    for t in ticks {
        out.push(match t {
            Some(d) => Some(d.checked_mul(MICROS_PER_DAY)?),
            None => None,
        });
    }
    Some(out)
}

/// Per-row ticks of a native temporal column, normalized to
/// microseconds since the epoch.
fn column_ticks_micros(col: &TypedCol) -> Option<Vec<Option<i64>>> {
    match col {
        TypedCol::Date { vals } => {
            let days: Vec<Option<i64>> = vals.iter().map(|v| v.map(i64::from)).collect();
            day_ticks_to_micros(&days)
        }
        TypedCol::Datetime { vals, .. } => {
            // As for dates: a millisecond value beyond the i64
            // microsecond range fails the whole conversion rather
            // than dropping that row, which would state a span the
            // data does not have.
            let mut out = Vec::with_capacity(vals.len());
            for (r, v) in vals.iter().enumerate() {
                out.push(match v {
                    Some(_) => Some(col.datetime_micros(r)?),
                    None => None,
                });
            }
            Some(out)
        }
        _ => None,
    }
}

/// The modal difference between consecutive distinct values:
/// the difference with the highest count, smallest on ties.
/// `None` with fewer than two values.
pub(crate) fn modal_diff(sorted_distinct: &[i64]) -> Option<i64> {
    if sorted_distinct.len() < 2 {
        return None;
    }
    let mut counts: Vec<(i64, usize)> = Vec::new();
    for w in sorted_distinct.windows(2) {
        // A span wider than the i64 microsecond range has no
        // representable step; report none rather than a wrapped
        // difference.
        let diff = w[1].checked_sub(w[0])?;
        match counts.iter_mut().find(|(d, _)| *d == diff) {
            Some((_, c)) => *c += 1,
            None => counts.push((diff, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    counts.first().map(|c| c.0)
}

/// The modal step between consecutive distinct axis values, as a
/// reader-facing label. Calendar steps are recognized by their
/// landing pattern (month starts, year starts), not by a fixed day
/// count.
fn modal_step_label(sorted_distinct_micros: &[i64]) -> Option<String> {
    let diff = modal_diff(sorted_distinct_micros)?;
    if diff <= 0 {
        return None;
    }
    let civil = |t: i64| dateutil::civil_from_days(t.div_euclid(MICROS_PER_DAY));
    let days = diff.div_euclid(MICROS_PER_DAY);
    if diff % MICROS_PER_DAY == 0
        && (28..=31).contains(&days)
        && sorted_distinct_micros.iter().all(|&t| civil(t).2 == 1)
    {
        return Some("1 month".into());
    }
    if diff % MICROS_PER_DAY == 0
        && (365..=366).contains(&days)
        && sorted_distinct_micros
            .iter()
            .all(|&t| civil(t).1 == 1 && civil(t).2 == 1)
    {
        return Some("1 year".into());
    }
    if diff % MICROS_PER_DAY == 0 {
        return Some(match days {
            1 => "1 day".to_string(),
            7 => "1 week".to_string(),
            d => format!("{d} days"),
        });
    }
    if diff % 3_600_000_000 == 0 {
        let h = diff / 3_600_000_000;
        return Some(if h == 1 {
            "1 hour".to_string()
        } else {
            format!("{h} hours")
        });
    }
    if diff % 60_000_000 == 0 {
        let m = diff / 60_000_000;
        return Some(if m == 1 {
            "1 minute".to_string()
        } else {
            format!("{m} minutes")
        });
    }
    let secs = diff / 1_000_000;
    if secs >= 1 {
        return Some(if secs == 1 {
            "1 second".to_string()
        } else {
            format!("{secs} seconds")
        });
    }
    Some("less than 1 second".into())
}

/// Measure the axis span from per-row microsecond ticks: the ISO
/// endpoints (dates when the axis is day-granular, datetimes when
/// any value carries a time of day), the distinct count, and the
/// modal step label.
fn span_from_ticks(
    ticks: &[Option<i64>],
) -> (
    Option<String>,
    Option<String>,
    Option<usize>,
    Option<String>,
) {
    let mut vals: Vec<i64> = ticks.iter().flatten().copied().collect();
    if vals.is_empty() {
        return (None, None, None, None);
    }
    vals.sort_unstable();
    vals.dedup();
    let sub_day = vals.iter().any(|t| t.rem_euclid(MICROS_PER_DAY) != 0);
    let fmt = |t: i64| {
        if sub_day {
            dateutil::iso_datetime(t)
        } else {
            dateutil::iso_date(t.div_euclid(MICROS_PER_DAY))
        }
    };
    let step = modal_step_label(&vals);
    (
        Some(fmt(vals[0])),
        Some(fmt(vals[vals.len() - 1])),
        Some(vals.len()),
        step,
    )
}

/// The axis's per-row ticks in microseconds, from whichever
/// representation the axis carries: materialized ticks (strings /
/// parts) or a native temporal column. None when the axis has no
/// tick representation (a year candidate, an integer hint column).
pub(crate) fn axis_ticks_micros(
    table: &TypedTable,
    frame: &FrameModel,
) -> Option<Vec<Option<i64>>> {
    if let Some(ticks) = &frame.axis_ticks {
        return if frame.axis_is_datetime {
            Some(ticks.clone())
        } else {
            day_ticks_to_micros(ticks)
        };
    }
    let src = frame.info.axis.as_ref()?.source_column.as_deref()?;
    let i = table.names.iter().position(|n| n == src)?;
    column_ticks_micros(&table.cols[i])
}

/// The span of the assembled axis, from whichever representation
/// the axis carries: materialized ticks (strings / parts), a native
/// temporal column, or the year candidate's card facts.
fn measure_axis_span(
    table: &TypedTable,
    facts: &[CardFacts],
    axis: &FrameAxis,
    axis_ticks: &Option<Vec<Option<i64>>>,
    axis_is_datetime: bool,
) -> (
    Option<String>,
    Option<String>,
    Option<usize>,
    Option<String>,
) {
    if let Some(ticks) = axis_ticks {
        let micros: Vec<Option<i64>> = if axis_is_datetime {
            ticks.clone()
        } else {
            match day_ticks_to_micros(ticks) {
                Some(micros) => micros,
                None => return (None, None, None, None),
            }
        };
        return span_from_ticks(&micros);
    }
    let Some(src) = axis.source_column.as_deref() else {
        return (None, None, None, None);
    };
    let Some(i) = table.names.iter().position(|n| n == src) else {
        return (None, None, None, None);
    };
    if let Some(ticks) = column_ticks_micros(&table.cols[i]) {
        return span_from_ticks(&ticks);
    }
    if axis.source == "year_candidate" || axis.source == "week_candidate" {
        let fact = &facts[i];
        return (
            fact.numeric_min.map(|v| format!("{}", v as i64)),
            fact.numeric_max.map(|v| format!("{}", v as i64)),
            Some(fact.distinct),
            None,
        );
    }
    (None, None, None, None)
}

// ---------------------------------------------------------------------
// the model
// ---------------------------------------------------------------------

/// Grouping columns eligible for restart detection: categorical /
/// boolean card roles with at least 2 levels, at most 8 of them, in
/// column order.
/// Whole-number columns with at most this many values can be grouping
/// columns (hour, month, a coded category); above it they are measured
/// quantities on a grid (age in years, hours worked).
const MAX_NUMERIC_DIMENSION_LEVELS: usize = 32;
/// The same limit for columns holding fractional values.
const MAX_FRACTIONAL_DIMENSION_LEVELS: usize = 12;
/// Unique whole numbers are issued labels when their range spans at most
/// this many times the row count.
const MAX_ISSUED_ID_SPREAD: f64 = 20.0;

/// True when the name's last word marks a key: `customer_id`, `orderId`,
/// `sku`. Splits on `_`, `-`, spaces and lower-to-upper case changes.
fn key_like_name(name: &str) -> bool {
    let mut last = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if !c.is_alphanumeric() || (c.is_uppercase() && prev_lower) {
            last.clear();
        }
        if c.is_alphanumeric() {
            last.extend(c.to_lowercase());
        }
        prev_lower = c.is_lowercase();
    }
    matches!(
        last.as_str(),
        "id" | "uid" | "uuid" | "guid" | "key" | "code" | "sku"
    )
}

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
        // (f) identifier: every non-null value unique, label-typed. A
        // whole-number column also needs to look issued: values packed
        // close together, or a key-like name. Widely spaced unique
        // integers are amounts (307 hospital spending totals in the
        // tens of thousands are all distinct and identify nothing).
        let n_nonnull = table.n - fact.null_count;
        let integer_col = matches!(col, TypedCol::I64 { .. } | TypedCol::U64 { .. });
        let densely_issued = match (fact.numeric_min, fact.numeric_max) {
            (Some(lo), Some(hi)) => (hi - lo + 1.0) <= MAX_ISSUED_ID_SPREAD * n_nonnull as f64,
            _ => false,
        };
        if fact.distinct == n_nonnull
            && n_nonnull >= MIN_SUPPORT
            && (col.is_string() || (integer_col && (densely_issued || key_like_name(name))))
        {
            roles[i] = Some("identifier".into());
            evidence[i] = format!(
                "{} distinct values in {} non-null rows; every value unique",
                fact.distinct, n_nonnull
            );
            continue;
        }
        // (g) dimension: low-cardinality grouping columns. The card's
        // "categorical" scales with row count, so age (74 values) or a
        // normalized reading (50 values) reaches it on a large frame;
        // a numeric column groups only on a small whole-number grid.
        let numeric_grid = col.is_numeric()
            && fact.card_role == "categorical"
            && !(fact.integral && fact.distinct <= MAX_NUMERIC_DIMENSION_LEVELS)
            && fact.distinct > MAX_FRACTIONAL_DIMENSION_LEVELS;
        if !numeric_grid
            && (matches!(fact.card_role.as_str(), "categorical" | "boolean")
                || matches!(col, TypedCol::Bool { .. }))
        {
            roles[i] = Some("dimension".into());
            evidence[i] = format!("{} distinct values", fact.distinct);
            continue;
        }
        // (h) identifier: a whole-number label named like a key.
        // Averaging it is meaningless, even when values repeat.
        if col.is_numeric() && fact.integral && key_like_name(name) {
            roles[i] = Some("identifier".into());
            evidence[i] = format!(
                "whole-number column named like a key ({} distinct values)",
                fact.distinct
            );
            continue;
        }
        // (i/j) the rest: numeric measures, text
        if numeric_grid {
            roles[i] = Some("measure".into());
            evidence[i] = format!(
                "numeric column on a grid of {} values, too fine to group by",
                fact.distinct
            );
        } else if col.is_numeric() {
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
                    axis = Some(FrameAxis::unmeasured(
                        hint.clone(),
                        "hint",
                        format!("declared by hints.time: the column '{hint}'"),
                        "declared",
                        Vec::new(),
                        Some(hint.clone()),
                    ));
                    axis_name = Some(hint.clone());
                    axis_source = Some("hint".into());
                }
                col if col.is_string() => {
                    if let Some((ticks, is_dt)) = parse_string_column(col) {
                        axis = Some(FrameAxis::unmeasured(
                            hint.clone(),
                            "hint",
                            format!(
                                "declared by hints.time: the string column '{hint}', parsed as timestamps"
                            ),
                            "declared",
                            Vec::new(),
                            Some(hint.clone()),
                        ));
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
            axis = Some(FrameAxis::unmeasured(
                name.clone(),
                "native",
                format!("the {} column '{name}'", table.dtypes[i]),
                "inferred",
                Vec::new(),
                Some(name.clone()),
            ));
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
                axis = Some(FrameAxis::unmeasured(
                    name.clone(),
                    "string_parse",
                    format!("the string column '{name}', parsed as timestamps"),
                    "inferred",
                    Vec::new(),
                    Some(name.clone()),
                ));
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
            axis = Some(FrameAxis::unmeasured(
                joined.clone(),
                "timestamp_parts",
                format!("a timestamp assembled from {joined}"),
                "inferred",
                names.clone(),
                None,
            ));
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
                axis = Some(FrameAxis::unmeasured(
                    name.clone(),
                    "year_candidate",
                    format!(
                        "the integer column '{name}' holds plausible years ({}..={}); recorded as a candidate axis at low confidence",
                        fact.numeric_min.map(|v| v as i64).unwrap_or(0),
                        fact.numeric_max.map(|v| v as i64).unwrap_or(0)
                    ),
                    "low",
                    Vec::new(),
                    Some(name.clone()),
                ));
                axis_name = Some(name);
                axis_source = Some("year_candidate".into());
                break;
            }
        }
    }
    // 6. week-number candidate, recorded at low confidence: a lone
    // week column orders rows within a year but repeats across
    // years, so it is an axis candidate, never an assembled axis.
    if axis.is_none() {
        for (i, col) in table.cols.iter().enumerate() {
            if !matches!(col, TypedCol::I64 { .. } | TypedCol::U64 { .. }) {
                continue;
            }
            if PartKind::of_name(&table.names[i]) != Some(PartKind::Week) {
                continue;
            }
            let fact = &facts[i];
            let in_range = matches!(
                (fact.numeric_min, fact.numeric_max),
                (Some(lo), Some(hi)) if lo >= 1.0 && hi <= 53.0
            );
            if in_range && fact.integral && fact.distinct >= MIN_SUPPORT {
                let name = table.names[i].clone();
                axis = Some(FrameAxis::unmeasured(
                    name.clone(),
                    "week_candidate",
                    format!(
                        "the integer column '{name}' holds week numbers ({}..={}); recorded as a candidate axis at low confidence — week numbers repeat every year, so the axis orders periods within a year only",
                        fact.numeric_min.map(|v| v as i64).unwrap_or(0),
                        fact.numeric_max.map(|v| v as i64).unwrap_or(0)
                    ),
                    "low",
                    Vec::new(),
                    Some(name.clone()),
                ));
                axis_name = Some(name);
                axis_source = Some("week_candidate".into());
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

    // Step 2: the account states the axis's span and frequency for
    // every temporal frame; measure them now that the axis is final.
    if let Some(ax) = axis.as_mut() {
        let (min, max, distinct, step) =
            measure_axis_span(table, facts, ax, &axis_ticks, axis_is_datetime);
        ax.min = min;
        ax.max = max;
        ax.distinct_values = distinct;
        ax.step = step;
        // A seven-day modal step makes the axis weekly; the account
        // says so in those terms, whichever representation the axis
        // arrived in.
        if ax.step.as_deref() == Some("1 week") && !ax.description.contains("weekly") {
            ax.description.push_str("; the axis is weekly");
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
        // The grain and dominant structure are computed by the
        // account step in the pipeline, which also has the
        // dependency outcome; they are filled in there.
        grain: None,
        dominant: Vec::new(),
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
                    integral: false,
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
    fn date_ticks_stop_at_the_microsecond_range() {
        // The largest day count whose microsecond form fits in an
        // i64 converts exactly, nulls included; one day beyond it
        // the conversion fails as a whole rather than wrapping or
        // dropping the extreme value.
        let edge = i64::MAX / MICROS_PER_DAY; // 106,751,991 days
        let col = TypedCol::Date {
            vals: vec![Some(-(edge as i32)), None, Some(edge as i32)],
        };
        assert_eq!(
            column_ticks_micros(&col),
            Some(vec![
                Some(-edge * MICROS_PER_DAY),
                None,
                Some(edge * MICROS_PER_DAY)
            ])
        );
        let beyond = TypedCol::Date {
            vals: vec![Some(0), Some((edge + 1) as i32)],
        };
        assert_eq!(column_ticks_micros(&beyond), None);
        // The full Polars date range is far beyond the boundary.
        let full = TypedCol::Date {
            vals: vec![Some(i32::MIN), Some(i32::MAX)],
        };
        assert_eq!(column_ticks_micros(&full), None);
    }

    #[test]
    fn millisecond_ticks_stop_at_the_microsecond_range() {
        let edge = i64::MAX / 1000;
        let fits = TypedCol::Datetime {
            vals: vec![Some(-edge), None, Some(edge)],
            unit: TimeUnit::Milliseconds,
        };
        assert_eq!(
            column_ticks_micros(&fits),
            Some(vec![Some(-edge * 1000), None, Some(edge * 1000)])
        );
        let beyond = TypedCol::Datetime {
            vals: vec![Some(0), Some(edge + 1)],
            unit: TimeUnit::Milliseconds,
        };
        assert_eq!(column_ticks_micros(&beyond), None);
    }

    #[test]
    fn modal_diff_of_an_unrepresentable_span_is_none() {
        assert_eq!(modal_diff(&[i64::MIN, i64::MAX]), None);
        assert_eq!(
            modal_diff(&[0, MICROS_PER_DAY, 2 * MICROS_PER_DAY]),
            Some(MICROS_PER_DAY)
        );
    }

    #[test]
    fn extreme_date_axis_is_recorded_without_a_span() {
        let df =
            DataFrame::new_infer_height(vec![Series::new("d".into(), vec![i32::MIN, i32::MAX])
                .cast(&DataType::Date)
                .unwrap()
                .into()])
            .unwrap();
        let (table, facts) = facts_for(&df);
        let frame = analyze_frame(&table, &facts, None);
        let axis = frame.info.axis.as_ref().expect("native date axis");
        assert_eq!(axis.min, None);
        assert_eq!(axis.max, None);
        assert_eq!(axis.distinct_values, None);
        assert_eq!(axis.step, None);
        assert_eq!(axis_ticks_micros(&table, &frame), None);
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

    #[test]
    fn iso_week_mondays() {
        // 2023 week 1 starts on Monday 2 January 2023.
        assert_eq!(
            iso_week_monday(2023, 1),
            Some(dateutil::days_from_civil(2023, 1, 2))
        );
        // 2020 has a week 53 (a leap year starting on a Wednesday);
        // its Monday is 28 December 2020.
        assert_eq!(
            iso_week_monday(2020, 53),
            Some(dateutil::days_from_civil(2020, 12, 28))
        );
        // 2021 has 52 weeks only.
        assert_eq!(iso_week_monday(2021, 53), None);
        assert_eq!(iso_week_monday(2023, 0), None);
        assert_eq!(iso_week_monday(2023, 54), None);
    }

    #[test]
    fn year_week_parts_assemble_a_weekly_axis() {
        // Year and week columns with no month: the axis is the
        // Monday of each ISO week, and the account calls it weekly.
        let mut year = Vec::new();
        let mut week = Vec::new();
        let mut value = Vec::new();
        for y in [2023i64, 2024] {
            for w in 1..=30i64 {
                year.push(y);
                week.push(w);
                value.push(10.0 + w as f64);
            }
        }
        let df = DataFrame::new_infer_height(vec![
            Series::new("year".into(), year).into(),
            Series::new("week".into(), week).into(),
            Series::new("value".into(), value).into(),
        ])
        .unwrap();
        let (table, facts) = facts_for(&df);
        let model = analyze_frame(&table, &facts, None);
        let axis = model.info.axis.as_ref().expect("axis assembled");
        assert_eq!(axis.source, "timestamp_parts");
        assert_eq!(axis.name, "year/week");
        assert_eq!(axis.step.as_deref(), Some("1 week"));
        assert!(axis.description.contains("weekly"), "{}", axis.description);
        assert_eq!(axis.min.as_deref(), Some("2023-01-02"));
        let role =
            |name: &str| model.roles[table.names.iter().position(|n| n == name).unwrap()].clone();
        assert_eq!(role("week").as_deref(), Some("timestamp_part"));
    }
}
