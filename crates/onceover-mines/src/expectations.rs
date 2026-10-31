//! Declared-expectation checks: the user states a table's intended
//! structure through hints (time axis, row ordering, gap-free columns,
//! cadences, dimensions, measures); this pass verifies the statements
//! exactly. Every finding's evidence is tagged
//! `reference_source: "declared"` — the counterpart to the dates
//! pass's `"inferred"`. Checks that inference cannot do at all:
//! monthly-cadence gaps, gap checks below the inference floor,
//! integer-sequence completeness, ordering *violations*, and panel
//! completeness over declared dimensions × periods.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use onceover_core::dateutil::{civil_from_days, is_weekend, iso_date};
use onceover_core::{TypedCol, TypedTable};
use onceover_report::{Cadence, EvidenceBasis, Finding, Method, Pattern, ProfileHints};
use serde_json::{json, Value};

const MAX_EXPECTED_PERIODS: usize = 200_000;
const MAX_SEQUENCE_EXPECTED: i128 = 1_000_000;
const MAX_GAP_FINDINGS: usize = 10;
const MAX_ABSENT_LISTED: usize = 20;
const MAX_DIM_DISTINCT: usize = 64;
const MAX_PANEL_CELLS: usize = 200_000;

fn method(algorithm: &str) -> Method {
    Method::new(
        "expectations",
        algorithm,
        "declared-expectation checks (onceover v0.1)",
    )
}

pub struct ExpectationsOutcome {
    pub findings: Vec<Finding>,
    pub coverage: String,
}

/// Temporal columns (Date/Datetime) that a declared gap check will
/// cover: named in `no_gaps` or carrying a `cadence` entry. The dates
/// pass skips inference for these so a gap is not reported twice.
pub fn declared_temporal_columns(table: &TypedTable, hints: &ProfileHints) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut wanted: Vec<&str> = hints.no_gaps.iter().map(|s| s.as_str()).collect();
    wanted.extend(hints.cadence.keys().map(|s| s.as_str()));
    for name in wanted {
        if let Some(i) = table.names.iter().position(|n| n == name) {
            if matches!(
                table.cols[i],
                TypedCol::Date { .. } | TypedCol::Datetime { .. }
            ) {
                out.insert(name.to_string());
            }
        }
    }
    out
}

fn col_index(table: &TypedTable, name: &str) -> Option<usize> {
    table.names.iter().position(|n| n == name)
}

/// Distinct sorted non-null days for a Date/Datetime column.
fn distinct_days(col: &TypedCol) -> Option<Vec<i64>> {
    let mut days: Vec<i64> = match col {
        TypedCol::Date { vals } => vals.iter().flatten().map(|&d| d as i64).collect(),
        TypedCol::Datetime { vals, unit } => vals
            .iter()
            .flatten()
            .map(|us| onceover_core::dateutil::timestamp_days(*us, *unit))
            .collect(),
        _ => return None,
    };
    days.sort_unstable();
    days.dedup();
    Some(days)
}

fn distinct_ints(col: &TypedCol) -> Option<Vec<i128>> {
    match col {
        TypedCol::I64 { vals } => {
            let mut v: Vec<i128> = vals.iter().flatten().map(|v| *v as i128).collect();
            v.sort_unstable();
            v.dedup();
            Some(v)
        }
        TypedCol::U64 { vals } => {
            let mut v: Vec<i128> = vals.iter().flatten().map(|v| *v as i128).collect();
            v.sort_unstable();
            v.dedup();
            Some(v)
        }
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CadenceKind {
    Daily,
    BusinessDay,
    Weekly,
    Monthly,
    Fixed(i64),
}

impl CadenceKind {
    fn label(self) -> &'static str {
        match self {
            CadenceKind::Daily => "calendar_days",
            CadenceKind::BusinessDay => "weekdays",
            CadenceKind::Weekly => "weeks",
            CadenceKind::Monthly => "months",
            CadenceKind::Fixed(_) => "fixed_interval",
        }
    }
    fn cadence_days(self) -> Option<i64> {
        match self {
            CadenceKind::Daily => Some(1),
            CadenceKind::Weekly => Some(7),
            CadenceKind::Fixed(k) => Some(k),
            _ => None,
        }
    }
}

fn resolve_cadence(hints: &ProfileHints, name: &str) -> Option<CadenceKind> {
    match hints.cadence.get(name)? {
        Cadence::Days(k) if *k >= 1 => Some(CadenceKind::Fixed(*k)),
        Cadence::Days(_) => None,
        Cadence::Named(s) => match s.as_str() {
            "daily" => Some(CadenceKind::Daily),
            "business_day" => Some(CadenceKind::BusinessDay),
            "weekly" => Some(CadenceKind::Weekly),
            "monthly" => Some(CadenceKind::Monthly),
            _ => None,
        },
    }
}

/// Period key for a day under a cadence: the day itself, except for
/// Monthly, where it is the month index (year * 12 + month - 1).
fn period_key(kind: CadenceKind, day: i64) -> i64 {
    match kind {
        CadenceKind::Monthly => {
            let (y, m, _) = civil_from_days(day);
            y * 12 + (m as i64 - 1)
        }
        _ => day,
    }
}

fn expected_periods(kind: CadenceKind, days: &[i64]) -> Vec<i64> {
    let (min, max) = (days[0], days[days.len() - 1]);
    match kind {
        CadenceKind::Daily | CadenceKind::Fixed(_) => {
            let step = match kind {
                CadenceKind::Daily => 1,
                CadenceKind::Fixed(k) => k,
                _ => unreachable!(),
            };
            std::iter::successors(Some(min), |d| d.checked_add(step))
                .take_while(|&d| d <= max)
                .take(MAX_EXPECTED_PERIODS + 1)
                .collect()
        }
        CadenceKind::BusinessDay => (min..=max)
            .filter(|&d| !is_weekend(d))
            .take(MAX_EXPECTED_PERIODS + 1)
            .collect(),
        CadenceKind::Weekly => std::iter::successors(Some(min), |d| d.checked_add(7))
            .take_while(|&d| d <= max)
            .take(MAX_EXPECTED_PERIODS + 1)
            .collect(),
        CadenceKind::Monthly => {
            let lo = period_key(kind, min);
            let hi = period_key(kind, max);
            (lo..=hi).take(MAX_EXPECTED_PERIODS + 1).collect()
        }
    }
}

fn period_label(kind: CadenceKind, key: i64) -> String {
    match kind {
        CadenceKind::Monthly => format!("{:04}-{:02}", key.div_euclid(12), key.rem_euclid(12) + 1),
        _ => iso_date(key),
    }
}

/// Gap findings for one temporal column against a cadence. Returns the
/// findings plus a rejection note when the check cannot run.
fn declared_date_gaps(
    table: &TypedTable,
    name: &str,
    kind: CadenceKind,
    reference_label: &str,
) -> (Vec<Finding>, Option<String>) {
    let col = &table.cols[col_index(table, name).unwrap()];
    let days = distinct_days(col).unwrap_or_default();
    if days.len() < 2 {
        return (
            Vec::new(),
            Some(format!(
                "gap check on '{name}' skipped: fewer than 2 distinct days"
            )),
        );
    }
    let expected = expected_periods(kind, &days);
    if expected.len() > MAX_EXPECTED_PERIODS {
        return (
            Vec::new(),
            Some(format!(
                "gap check on '{name}' skipped: more than {MAX_EXPECTED_PERIODS} expected periods exceeds the {MAX_EXPECTED_PERIODS} cap"
            )),
        );
    }
    let observed: HashSet<i64> = days.iter().map(|&d| period_key(kind, d)).collect();
    // Missing runs: consecutive expected periods with no observation.
    let mut runs: Vec<(i64, i64, i64)> = Vec::new(); // (first key, last key, count)
    let mut i = 0;
    while i < expected.len() {
        if observed.contains(&expected[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < expected.len() && !observed.contains(&expected[i]) {
            i += 1;
        }
        runs.push((expected[start], expected[i - 1], (i - start) as i64));
    }
    let unit = match kind {
        CadenceKind::Daily => "day",
        CadenceKind::BusinessDay => "business day",
        CadenceKind::Weekly => "week",
        CadenceKind::Monthly => "month",
        CadenceKind::Fixed(_) => "interval",
    };
    runs.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    runs.truncate(MAX_GAP_FINDINGS);
    let mut out = Vec::new();
    for (first_key, _last_key, missing) in runs {
        // Bounding observed days around the run.
        let from_day = days
            .iter()
            .rev()
            .find(|&&d| period_key(kind, d) < first_key)
            .copied();
        let to_day = days
            .iter()
            .find(|&&d| period_key(kind, d) > first_key)
            .copied();
        let (Some(from), Some(to)) = (from_day, to_day) else {
            continue;
        };
        let mut metrics = BTreeMap::from([
            ("missing".into(), json!(missing)),
            ("reference_source".into(), json!("declared")),
            ("reference".into(), json!(reference_label)),
            ("n".into(), json!(table.n)),
            ("from".into(), json!(iso_date(from))),
            ("to".into(), json!(iso_date(to))),
            ("elapsed_days".into(), json!(to - from)),
            ("approximate".into(), json!(false)),
        ]);
        if let Some(cd) = kind.cadence_days() {
            metrics.insert("cadence_days".into(), json!(cd));
        }
        out.push(Finding::new(
            Pattern::Gap,
            BTreeMap::from([
                ("column".into(), json!(name)),
                ("from".into(), json!(iso_date(from))),
                ("to".into(), json!(iso_date(to))),
            ]),
            format!(
                "column {} is missing {} {}{} at {}..{} ({} days elapsed)",
                name,
                missing,
                unit,
                if missing > 1 { "s" } else { "" },
                iso_date(from),
                iso_date(to),
                to - from
            ),
            EvidenceBasis::Measured,
            0.58 + 0.2 * ((missing as f64) / 40.0).min(1.0),
            metrics,
            Some(format!(
                "declared {} cadence: this column was declared gap-free; check for missing periods (ETL gaps, holidays not modeled, outages)",
                kind.label()
            )),
            method("declared_cadence"),
        ));
    }
    (out, None)
}

/// Integer-sequence completeness for one `no_gaps` column.
fn sequence_check(
    table: &TypedTable,
    name: &str,
    hints: &ProfileHints,
) -> (Vec<Finding>, Option<String>) {
    let col = &table.cols[col_index(table, name).unwrap()];
    let Some(values) = distinct_ints(col) else {
        return (Vec::new(), None);
    };
    if values.len() < 10 {
        return (
            Vec::new(),
            Some(format!(
                "sequence check on '{name}' skipped: {} distinct values (minimum 10)",
                values.len()
            )),
        );
    }
    let step = match hints.cadence.get(name) {
        Some(Cadence::Days(k)) if *k >= 1 => *k as i128,
        Some(Cadence::Days(_)) => {
            return (
                Vec::new(),
                Some(format!(
                    "sequence check on '{name}' skipped: declared step must be >= 1"
                )),
            );
        }
        Some(Cadence::Named(_)) => {
            return (
                Vec::new(),
                Some(format!(
                    "sequence check on '{name}' skipped: a named calendar cadence does not apply to an integer column; declare a numeric step"
                )),
            );
        }
        None => {
            // Modal positive difference between consecutive distinct values.
            let mut counts: BTreeMap<i128, u32> = BTreeMap::new();
            for w in values.windows(2) {
                *counts.entry(w[1] - w[0]).or_insert(0) += 1;
            }
            counts
                .into_iter()
                .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
                .map(|(d, _)| d)
                .unwrap_or(1)
        }
    };
    let (min, max) = (values[0], values[values.len() - 1]);
    let expected = ((max - min) / step) + 1;
    if expected > MAX_SEQUENCE_EXPECTED {
        return (
            Vec::new(),
            Some(format!(
                "sequence check on '{name}' skipped: {expected} expected values at step {step} exceeds the {MAX_SEQUENCE_EXPECTED} cap"
            )),
        );
    }
    let observed: HashSet<i128> = values.iter().copied().collect();
    let mut absent: Vec<i128> = Vec::new();
    let mut missing: i64 = 0;
    let mut v = min;
    while v <= max {
        if !observed.contains(&v) {
            missing += 1;
            if absent.len() < MAX_ABSENT_LISTED {
                absent.push(v);
            }
        }
        v += step;
    }
    if missing == 0 {
        return (Vec::new(), None);
    }
    let finding = Finding::new(
        Pattern::SequenceGap,
        BTreeMap::from([("column".into(), json!(name))]),
        format!(
            "column {} is missing {} values from its expected sequence (step {} over {}..{})",
            name, missing, step, min, max
        ),
        EvidenceBasis::Measured,
        0.58 + 0.2 * ((missing as f64) / 40.0).min(1.0),
        BTreeMap::from([
            ("missing".into(), json!(missing)),
            ("step".into(), json!(step)),
            ("expected_count".into(), json!(expected as i64)),
            ("observed_distinct".into(), json!(values.len())),
            (
                "absent_values".into(),
                json!(absent.iter().map(|v| json!(v)).collect::<Vec<_>>()),
            ),
            ("reference_source".into(), json!("declared")),
            ("n".into(), json!(table.n)),
            ("from".into(), json!(min)),
            ("to".into(), json!(max)),
        ]),
        Some(
            "the column was declared gap-free; missing values suggest deleted rows or a broken sequence"
                .into(),
        ),
        method("declared_sequence"),
    );
    (vec![finding], None)
}

/// The canonical JSON value of one cell, for group keys and ordering
/// comparisons. Dates and datetimes stay numeric (days / microseconds)
/// so ordering is chronological; display formatting happens later.
fn cell_value(col: &TypedCol, row: usize) -> Option<Value> {
    match col {
        TypedCol::I64 { vals } => vals.get(row).flatten().map(|v| json!(v)),
        TypedCol::U64 { vals } => vals.get(row).flatten().map(|v| json!(v)),
        TypedCol::F64 { vals } => vals.get(row).flatten().and_then(|v| {
            if v.is_finite() {
                serde_json::Number::from_f64(v).map(Value::Number)
            } else {
                None
            }
        }),
        TypedCol::Str { vals } => vals.get(row).copied().flatten().map(|s| json!(s)),
        TypedCol::StrOwned { vals } => vals.get(row).and_then(|o| o.as_deref()).map(|s| json!(s)),
        TypedCol::Bool { vals } => vals.get(row).copied().flatten().map(|b| json!(b)),
        TypedCol::Date { vals } => vals.get(row).copied().flatten().map(|d| json!(d)),
        TypedCol::Datetime { vals, .. } => {
            vals.get(row).copied().flatten().map(|ticks| json!(ticks))
        }
        _ => None,
    }
}

fn cmp_value(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_i64(), y.as_i64()) {
            (Some(p), Some(q)) => p.cmp(&q),
            _ if x.as_u64().is_some() && y.as_u64().is_some() => x.as_u64().cmp(&y.as_u64()),
            _ => x
                .as_f64()
                .unwrap_or(f64::NAN)
                .partial_cmp(&y.as_f64().unwrap_or(f64::NAN))
                .unwrap_or(Ordering::Equal),
        },
        (Value::String(x), Value::String(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => Ordering::Equal,
    }
}

/// Declared-ordering check over the `order_by` columns.
fn ordering_check(
    table: &TypedTable,
    hints: &ProfileHints,
    notes: &mut Vec<String>,
) -> Vec<Finding> {
    let mut cols: Vec<usize> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for name in &hints.order_by {
        match col_index(table, name) {
            None => notes.push(format!("order_by column '{name}' ignored: not in table")),
            Some(i) => {
                cols.push(i);
                names.push(name.clone());
            }
        }
    }
    if cols.is_empty() {
        return Vec::new();
    }
    let mut inversions: i64 = 0;
    let mut pairs: i64 = 0;
    let mut skipped: i64 = 0;
    let mut first_rows: Vec<i64> = Vec::new();
    for r in 0..table.n.saturating_sub(1) {
        let mut ka = Vec::with_capacity(cols.len());
        let mut kb = Vec::with_capacity(cols.len());
        let mut complete = true;
        for &ci in &cols {
            match (
                cell_value(&table.cols[ci], r),
                cell_value(&table.cols[ci], r + 1),
            ) {
                (Some(a), Some(b)) => {
                    ka.push(a);
                    kb.push(b);
                }
                _ => {
                    complete = false;
                    break;
                }
            }
        }
        if !complete {
            skipped += 1;
            continue;
        }
        pairs += 1;
        for (a, b) in ka.iter().zip(kb.iter()) {
            match cmp_value(a, b) {
                std::cmp::Ordering::Greater => {
                    inversions += 1;
                    if first_rows.len() < 5 {
                        first_rows.push((r + 1) as i64);
                    }
                    break;
                }
                std::cmp::Ordering::Less => break,
                std::cmp::Ordering::Equal => continue,
            }
        }
    }
    if pairs == 0 {
        notes.push("ordering check skipped: no complete adjacent row pairs".into());
        return Vec::new();
    }
    let joined = names.join(", ");
    let raw_subject = if names.len() == 1 {
        BTreeMap::from([("column".into(), json!(names[0]))])
    } else {
        BTreeMap::from([("columns".into(), json!(names))])
    };
    let metrics = BTreeMap::from([
        ("inversions".into(), json!(inversions)),
        ("adjacent_pairs".into(), json!(pairs)),
        ("skipped_null_pairs".into(), json!(skipped)),
        (
            "first_inversion_rows".into(),
            json!(first_rows.iter().map(|v| json!(v)).collect::<Vec<_>>()),
        ),
        ("reference_source".into(), json!("declared")),
    ]);
    let finding = if inversions > 0 {
        Finding::new(
            Pattern::ColumnOrdering,
            raw_subject,
            format!(
                "rows are not ordered by ({}) as declared: {} inversions across {} adjacent pairs (first at row {})",
                joined, inversions, pairs, first_rows[0]
            ),
            EvidenceBasis::Measured,
            0.55,
            metrics,
            Some(
                "the declaration may be wrong, or rows were shuffled or filtered after sorting"
                    .into(),
            ),
            method("declared_ordering"),
        )
    } else {
        Finding::new(
            Pattern::ColumnOrdering,
            raw_subject,
            format!(
                "rows are ordered by ({}) as declared ({} adjacent pairs, no inversions)",
                joined, pairs
            ),
            EvidenceBasis::Measured,
            0.28,
            metrics,
            Some("row order matches the declared ordering".into()),
            method("declared_ordering"),
        )
    };
    vec![finding]
}

/// Panel completeness over declared dimensions × periods.
fn panel_check(table: &TypedTable, hints: &ProfileHints, notes: &mut Vec<String>) -> Vec<Finding> {
    if hints.dimensions.is_empty() {
        return Vec::new();
    }
    // Axis: the time hint, else a no_gaps temporal/integer column.
    let axis_name = hints
        .time
        .clone()
        .filter(|t| col_index(table, t).is_some())
        .or_else(|| {
            hints
                .no_gaps
                .iter()
                .find(|c| {
                    col_index(table, c).is_some_and(|i| {
                        matches!(
                            table.cols[i],
                            TypedCol::Date { .. }
                                | TypedCol::Datetime { .. }
                                | TypedCol::I64 { .. }
                                | TypedCol::U64 { .. }
                        )
                    })
                })
                .cloned()
        });
    let Some(axis_name) = axis_name else {
        notes.push(
            "panel check skipped: no usable axis (declare `time`, or a temporal/integer `no_gaps` column)"
                .into(),
        );
        return Vec::new();
    };
    let axis_i = col_index(table, &axis_name).unwrap();
    let axis_is_date = matches!(
        table.cols[axis_i],
        TypedCol::Date { .. } | TypedCol::Datetime { .. }
    );
    if !axis_is_date && distinct_ints(&table.cols[axis_i]).is_none() {
        notes.push(format!(
            "panel check skipped: axis '{axis_name}' is neither temporal nor integer"
        ));
        return Vec::new();
    }
    // Dimensions: present, usable cell values, at most 64 distinct each.
    let mut dim_idx: Vec<usize> = Vec::new();
    let mut dim_names: Vec<String> = Vec::new();
    for name in &hints.dimensions {
        match col_index(table, name) {
            None => notes.push(format!("dimension '{name}' ignored: not in table")),
            Some(i) => {
                let distinct: HashSet<String> = (0..table.n)
                    .filter_map(|r| cell_value(&table.cols[i], r))
                    .map(|v| v.to_string())
                    .collect();
                if distinct.is_empty() {
                    notes.push(format!("dimension '{name}' ignored: no non-null values"));
                } else if distinct.len() > MAX_DIM_DISTINCT {
                    notes.push(format!(
                        "dimension '{name}' ignored: {} distinct values exceeds the {MAX_DIM_DISTINCT} cap",
                        distinct.len()
                    ));
                } else {
                    dim_idx.push(i);
                    dim_names.push(name.clone());
                }
            }
        }
    }
    if dim_idx.is_empty() {
        return Vec::new();
    }
    // Period keys per row.
    let kind = resolve_cadence(hints, &axis_name);
    let period_of = |row: usize| -> Option<i64> {
        match &table.cols[axis_i] {
            TypedCol::Date { vals } => vals
                .get(row)
                .copied()
                .flatten()
                .map(|d| period_key(kind.unwrap_or(CadenceKind::Daily), d as i64)),
            TypedCol::Datetime { vals, unit } => vals.get(row).copied().flatten().map(|us| {
                period_key(
                    kind.unwrap_or(CadenceKind::Daily),
                    onceover_core::dateutil::timestamp_days(us, *unit),
                )
            }),
            TypedCol::I64 { vals } => vals.get(row).flatten(),
            TypedCol::U64 { vals } => vals.at(row).and_then(|v| i64::try_from(v).ok()),
            _ => None,
        }
    };
    let period_label_of = |key: i64| -> String {
        match kind {
            Some(CadenceKind::Monthly) => period_label(CadenceKind::Monthly, key),
            _ if axis_is_date => iso_date(key),
            _ => key.to_string(),
        }
    };
    // Group rows.
    let mut groups: BTreeMap<String, (Vec<Value>, HashSet<i64>)> = BTreeMap::new();
    let mut all_periods: BTreeSet<i64> = BTreeSet::new();
    let mut null_dim_rows: i64 = 0;
    let mut cell_has_measure: HashSet<(String, i64)> = HashSet::new();
    let measure_idx: Vec<usize> = hints
        .measures
        .iter()
        .filter_map(|m| col_index(table, m))
        .collect();
    for r in 0..table.n {
        let mut key_parts: Vec<Value> = Vec::with_capacity(dim_idx.len());
        let mut complete = true;
        for &ci in &dim_idx {
            match cell_value(&table.cols[ci], r) {
                Some(v) => key_parts.push(v),
                None => {
                    complete = false;
                    break;
                }
            }
        }
        if !complete {
            null_dim_rows += 1;
            continue;
        }
        let Some(period) = period_of(r) else { continue };
        let key = Value::Array(key_parts.clone()).to_string();
        all_periods.insert(period);
        let entry = groups
            .entry(key.clone())
            .or_insert_with(|| (key_parts, HashSet::new()));
        entry.1.insert(period);
        if !measure_idx.is_empty()
            && measure_idx
                .iter()
                .any(|&mi| cell_value(&table.cols[mi], r).is_some())
        {
            cell_has_measure.insert((key, period));
        }
    }
    if groups.is_empty() || all_periods.is_empty() {
        notes.push("panel check skipped: no complete dimension × period rows".into());
        return Vec::new();
    }
    // Expected periods: declared cadence enumeration, else the union
    // of observed periods.
    let expected_periods: Vec<i64> = match (kind, axis_is_date) {
        (Some(k), true) => {
            let days: Vec<i64> = all_periods.iter().copied().collect();
            // For monthly, all_periods already holds month indices.
            match k {
                CadenceKind::Monthly => (days[0]..=days[days.len() - 1])
                    .take(MAX_PANEL_CELLS / groups.len() + 1)
                    .collect(),
                _ => expected_periods(k, &days),
            }
        }
        _ => all_periods.iter().copied().collect(),
    };
    let expected_cells = groups.len() * expected_periods.len();
    if expected_cells > MAX_PANEL_CELLS {
        notes.push(format!(
            "panel check skipped: {} groups × at least {} periods exceeds the {MAX_PANEL_CELLS} cell cap",
            groups.len(),
            expected_periods.len()
        ));
        return Vec::new();
    }
    let expected_set: HashSet<i64> = expected_periods.iter().copied().collect();
    let mut total_missing: i64 = 0;
    let mut per_group_missing: Vec<(String, Vec<Value>, i64, Vec<i64>)> = Vec::new();
    let mut absent_values: Vec<Value> = Vec::new();
    let mut observed_cells: i64 = 0;
    for (key, (display, periods)) in &groups {
        let missing_here: Vec<i64> = expected_periods
            .iter()
            .filter(|p| !periods.contains(p))
            .copied()
            .collect();
        observed_cells += (expected_periods.len() - missing_here.len()) as i64;
        if !missing_here.is_empty() {
            total_missing += missing_here.len() as i64;
            for p in &missing_here {
                if absent_values.len() < 10 {
                    let mut group_obj = serde_json::Map::new();
                    for (dn, dv) in dim_names.iter().zip(display.iter()) {
                        group_obj.insert(dn.clone(), display_value(table, dn, dv));
                    }
                    absent_values.push(json!({
                        "group": Value::Object(group_obj),
                        "period": period_label_of(*p),
                    }));
                }
            }
            per_group_missing.push((
                key.clone(),
                display.clone(),
                missing_here.len() as i64,
                missing_here,
            ));
        }
    }
    let mut measure_null_cells: i64 = 0;
    if !measure_idx.is_empty() {
        for (key, (_, periods)) in &groups {
            for p in periods {
                if expected_set.contains(p) && !cell_has_measure.contains(&(key.clone(), *p)) {
                    measure_null_cells += 1;
                }
            }
        }
    }
    if total_missing == 0 && measure_null_cells == 0 {
        return Vec::new();
    }
    per_group_missing.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    let worst_groups: Vec<Value> = per_group_missing
        .iter()
        .take(5)
        .map(|(_, display, missing, _)| {
            let mut group_obj = serde_json::Map::new();
            for (dn, dv) in dim_names.iter().zip(display.iter()) {
                group_obj.insert(dn.clone(), display_value(table, dn, dv));
            }
            json!({"group": Value::Object(group_obj), "missing": missing})
        })
        .collect();
    let groups_with_missing = per_group_missing.len() as i64;
    let missing_share = if expected_cells > 0 {
        total_missing as f64 / expected_cells as f64
    } else {
        0.0
    };
    let worst_label = per_group_missing
        .first()
        .map(|(_, display, missing, _)| {
            let parts: Vec<String> = dim_names
                .iter()
                .zip(display.iter())
                .map(|(dn, dv)| format!("{dn}={}", display_value(table, dn, dv)))
                .collect();
            format!("{} is missing {}", parts.join(", "), missing)
        })
        .unwrap_or_default();
    let mut claim = format!(
        "declared panel is incomplete: {} of {} dimension × period cells are missing ({:.1}%); {}",
        total_missing,
        expected_cells,
        100.0 * missing_share,
        worst_label
    );
    if measure_null_cells > 0 {
        claim.push_str(&format!(
            "; {} present cells are null on every declared measure",
            measure_null_cells
        ));
    }
    let mut subject_cols: Vec<String> = dim_names.clone();
    subject_cols.push(axis_name.clone());
    let mut metrics = BTreeMap::from([
        ("axis".into(), json!(axis_name)),
        (
            "cadence".into(),
            json!(match kind {
                Some(k) => k.label(),
                None => "observed_periods",
            }),
        ),
        ("expected_cells".into(), json!(expected_cells)),
        ("observed_cells".into(), json!(observed_cells)),
        ("missing_cells".into(), json!(total_missing)),
        ("missing_share".into(), json!(missing_share)),
        ("groups".into(), json!(groups.len())),
        ("groups_with_missing".into(), json!(groups_with_missing)),
        ("worst_groups".into(), json!(worst_groups)),
        ("absent_values".into(), json!(absent_values)),
        ("reference_source".into(), json!("declared")),
        ("n_rows".into(), json!(table.n)),
    ]);
    if !measure_idx.is_empty() {
        metrics.insert("measure_null_cells".into(), json!(measure_null_cells));
    }
    if null_dim_rows > 0 {
        metrics.insert("rows_with_null_dimensions".into(), json!(null_dim_rows));
    }
    let score = if total_missing > 0 {
        0.5 + 0.3 * missing_share.min(1.0)
    } else {
        0.45
    };
    let finding = Finding::new(
        Pattern::MissingCombinations,
        BTreeMap::from([("columns".into(), json!(subject_cols))]),
        claim,
        EvidenceBasis::Measured,
        score,
        metrics,
        Some(
            "the grid is the strict cartesian product of the declared dimensions and the axis periods in the observed range; staggered starts or retired dimensions read as missing cells"
                .into(),
        ),
        method("declared_panel"),
    );
    vec![finding]
}

/// Display form of a dimension value: dates stored as day counts are
/// rendered as ISO dates when the column is temporal.
fn display_value(table: &TypedTable, name: &str, v: &Value) -> Value {
    if let Some(i) = col_index(table, name) {
        if matches!(table.cols[i], TypedCol::Date { .. }) {
            if let Some(d) = v.as_i64() {
                return json!(iso_date(d));
            }
        }
    }
    v.clone()
}

pub fn analyze_expectations(table: &TypedTable, hints: &ProfileHints) -> ExpectationsOutcome {
    let mut findings: Vec<Finding> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut checks: Vec<String> = Vec::new();

    if let Some(t) = &hints.time {
        if col_index(table, t).is_none() {
            notes.push(format!("time column '{t}' ignored: not in table"));
        }
    }
    for name in hints.cadence.keys() {
        if col_index(table, name).is_none() {
            notes.push(format!("cadence for '{name}' ignored: not in table"));
        }
    }
    for (name, cad) in &hints.cadence {
        if let Cadence::Named(s) = cad {
            if !Cadence::is_valid_named(s) {
                notes.push(format!(
                    "cadence for '{name}' ignored: unknown cadence '{s}' (expected daily | business_day | weekly | monthly | a day count)"
                ));
            }
        }
        if let Cadence::Days(k) = cad {
            if *k < 1 {
                notes.push(format!("cadence for '{name}' ignored: step must be >= 1"));
            }
        }
    }

    // Gap checks: every no_gaps column, plus temporal columns that
    // carry a cadence but are not in no_gaps.
    let mut gap_columns: Vec<String> = hints.no_gaps.clone();
    for name in hints.cadence.keys() {
        if !gap_columns.contains(name) {
            gap_columns.push(name.clone());
        }
    }
    for name in &gap_columns {
        let Some(i) = col_index(table, name) else {
            notes.push(format!("gap check on '{name}' ignored: not in table"));
            continue;
        };
        let in_no_gaps = hints.no_gaps.contains(name);
        match &table.cols[i] {
            TypedCol::I64 { .. } | TypedCol::U64 { .. } => {
                if !in_no_gaps {
                    notes.push(format!(
                        "gap check on '{name}' skipped: a cadence alone does not declare an integer column gap-free; add it to no_gaps"
                    ));
                    continue;
                }
                let (f, note) = sequence_check(table, name, hints);
                if let Some(n) = note {
                    notes.push(n);
                } else {
                    checks.push(format!("sequence on '{name}'"));
                }
                findings.extend(f);
            }
            TypedCol::Date { .. } | TypedCol::Datetime { .. } => {
                let kind = resolve_cadence(hints, name).or_else(|| {
                    // No declared cadence: enumerate at the modal
                    // interval of the observed days.
                    let days = distinct_days(&table.cols[i])?;
                    if days.len() < 2 {
                        return None;
                    }
                    let mut counts: BTreeMap<i64, u32> = BTreeMap::new();
                    for w in days.windows(2) {
                        *counts.entry(w[1] - w[0]).or_insert(0) += 1;
                    }
                    counts
                        .into_iter()
                        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
                        .map(|(d, _)| CadenceKind::Fixed(d))
                });
                let Some(kind) = kind else {
                    notes.push(format!(
                        "gap check on '{name}' skipped: no cadence could be resolved"
                    ));
                    continue;
                };
                let declared = hints.cadence.contains_key(name);
                let (f, note) = declared_date_gaps(
                    table,
                    name,
                    kind,
                    if declared {
                        kind.label()
                    } else {
                        "modal_interval"
                    },
                );
                if let Some(n) = note {
                    notes.push(n);
                } else {
                    checks.push(format!(
                        "declared-cadence gaps on '{name}' ({})",
                        kind.label()
                    ));
                }
                findings.extend(f);
            }
            _ => {
                if in_no_gaps {
                    notes.push(format!(
                        "gap check on '{name}' skipped: only integer and temporal columns can be checked for gaps"
                    ));
                }
            }
        }
    }

    if !hints.order_by.is_empty() {
        let before = notes.len();
        let f = ordering_check(table, hints, &mut notes);
        if !f.is_empty() {
            checks.push(format!("ordering on ({})", hints.order_by.join(", ")));
        }
        let _ = before;
        findings.extend(f);
    }

    if !hints.dimensions.is_empty() {
        let f = panel_check(table, hints, &mut notes);
        if !f.is_empty() {
            checks.push(format!("panel on ({}) × axis", hints.dimensions.join(", ")));
        } else if !notes.iter().any(|n| n.starts_with("panel check skipped")) {
            checks.push(format!(
                "panel on ({}) × axis (complete)",
                hints.dimensions.join(", ")
            ));
        }
        findings.extend(f);
    }

    let mut coverage = format!(
        "{} checks run ({}), {} findings",
        checks.len(),
        checks.join("; "),
        findings.len()
    );
    if !notes.is_empty() {
        coverage.push_str(&format!("; ignored/skipped: {}", notes.join("; ")));
    }
    ExpectationsOutcome { findings, coverage }
}

#[cfg(test)]
mod tests {
    #[test]
    fn period_enumeration_is_bounded_even_at_date_extremes() {
        use super::*;
        let days = [i32::MIN as i64, i32::MAX as i64];
        for kind in [
            CadenceKind::Daily,
            CadenceKind::BusinessDay,
            CadenceKind::Weekly,
            CadenceKind::Monthly,
        ] {
            assert_eq!(
                expected_periods(kind, &days).len(),
                MAX_EXPECTED_PERIODS + 1
            );
        }
        assert_eq!(
            expected_periods(CadenceKind::Fixed(i64::MAX), &[0, 2]),
            vec![0]
        );
    }
    use super::*;

    fn hints(v: serde_json::Value) -> ProfileHints {
        serde_json::from_value(v).unwrap()
    }

    fn date_col(days: &[i64]) -> TypedCol<'static> {
        TypedCol::Date {
            vals: days.iter().map(|&d| Some(d as i32)).collect(),
        }
    }

    fn table(names: Vec<&str>, cols: Vec<TypedCol<'static>>, n: usize) -> TypedTable<'static> {
        TypedTable {
            n,
            names: names.iter().map(|s| s.to_string()).collect(),
            dtypes: vec!["int64".into(); names.len()],
            cols,
        }
    }

    #[test]
    fn sequence_gap_reports_absent_values() {
        let ids: Vec<i64> = (1..=20).filter(|v| ![4, 7, 18].contains(v)).collect();
        let t = table(
            vec!["id"],
            vec![TypedCol::I64 {
                vals: ids.iter().copied().map(Some).collect::<Vec<_>>().into(),
            }],
            ids.len(),
        );
        let out = analyze_expectations(&t, &hints(json!({"no_gaps": ["id"]})));
        assert_eq!(out.findings.len(), 1);
        let f = &out.findings[0];
        assert_eq!(f.category, onceover_report::Category::Gaps);
        assert_eq!(f.metrics()["missing"], json!(3));
        assert_eq!(f.metrics()["absent_values"], json!([4, 7, 18]));
        assert_eq!(f.metrics()["reference_source"], json!("declared"));
        assert!(f.validate().is_ok(), "{:?}", f.validate());
    }

    #[test]
    fn monthly_declared_gaps_below_inference_floor() {
        // Six monthly observations (2024-01..06), March missing: five
        // distinct months — far below the inferred pass's 30-day floor.
        let days = [19723, 19754, 19814, 19844, 19875]; // Jan, Feb, Apr, May, Jun 2024
        let t = table(vec!["month"], vec![date_col(&days)], days.len());
        let out = analyze_expectations(
            &t,
            &hints(json!({"no_gaps": ["month"], "cadence": {"month": "monthly"}})),
        );
        assert_eq!(out.findings.len(), 1, "{}", out.coverage);
        let f = &out.findings[0];
        assert_eq!(f.metrics()["missing"], json!(1));
        assert_eq!(f.metrics()["reference"], json!("months"));
        assert_eq!(f.metrics()["reference_source"], json!("declared"));
        assert!(f.validate().is_ok(), "{:?}", f.validate());
    }

    #[test]
    fn ordering_violations_are_counted_and_located() {
        let vals: Vec<i64> = vec![1, 2, 3, 5, 4, 6, 7, 8, 9, 10];
        let t = table(
            vec!["x"],
            vec![TypedCol::I64 {
                vals: vals.iter().copied().map(Some).collect::<Vec<_>>().into(),
            }],
            vals.len(),
        );
        let out = analyze_expectations(&t, &hints(json!({"order_by": ["x"]})));
        assert_eq!(out.findings.len(), 1);
        let f = &out.findings[0];
        assert_eq!(f.category, onceover_report::Category::Ordering);
        assert_eq!(f.metrics()["inversions"], json!(1));
        assert_eq!(f.metrics()["first_inversion_rows"], json!([4]));
        assert!(f.claim.contains("not ordered"));
        assert!(f.validate().is_ok(), "{:?}", f.validate());
    }

    #[test]
    fn panel_missing_cell_is_reported_with_group() {
        // Two entities × three years; E1 lacks 2022, E2 complete.
        let entities = ["E0", "E0", "E0", "E1", "E1"];
        let years = [2020, 2021, 2022, 2020, 2022];
        let n = entities.len();
        let t = table(
            vec!["entity", "year", "gdp"],
            vec![
                TypedCol::Str {
                    vals: entities.iter().map(|s| Some(*s)).collect(),
                },
                TypedCol::I64 {
                    vals: years.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                TypedCol::F64 {
                    vals: vec![Some(1.0); n].into(),
                },
            ],
            n,
        );
        let out = analyze_expectations(
            &t,
            &hints(json!({"dimensions": ["entity"], "time": "year", "measures": ["gdp"]})),
        );
        assert_eq!(out.findings.len(), 1, "{}", out.coverage);
        let f = &out.findings[0];
        assert_eq!(f.metrics()["missing_cells"], json!(1));
        assert_eq!(f.metrics()["expected_cells"], json!(6));
        let absent = f.metrics()["absent_values"].to_string();
        assert!(absent.contains("E1") && absent.contains("2021"), "{absent}");
        assert!(f.validate().is_ok(), "{:?}", f.validate());
    }

    #[test]
    fn unknown_columns_are_noted_not_fatal() {
        let t = table(
            vec!["id"],
            vec![TypedCol::I64 {
                vals: (1..=15).map(Some).collect::<Vec<_>>().into(),
            }],
            15,
        );
        let out = analyze_expectations(
            &t,
            &hints(json!({"no_gaps": ["id", "dtae"], "time": "when"})),
        );
        assert!(out.coverage.contains("dtae"), "{}", out.coverage);
        assert!(out.coverage.contains("when"), "{}", out.coverage);
        assert!(out.findings.is_empty());
    }
}
