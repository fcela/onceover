//! The frame account (frame-model step 2): what the dataset is, as
//! a short sequence of measured statements that lead the report —
//! the grain and key, the time axis (its span and frequency are
//! measured in `frame.rs`), completeness against the grain (step 3),
//! and the dominant structure. This module computes the grain and
//! the dominant structure and builds the account's findings; the
//! pipeline calls it once the dependency pass has run, because the
//! grain's factorization and the calendar family both read the
//! exact functional dependencies.
//!
//! The grain: when the frame is a cross of dimension columns and
//! the time axis, the account states the factorization — how many
//! values of each component, whether every combination occurs —
//! instead of leaving the reader to assemble it from a composite
//! key finding. The candidate components are tested directly
//! (hashing the component tuples), so the statement does not depend
//! on the dependency search having found this particular key. When
//! no exact key exists but the axis nearly identifies rows, the
//! account states the near key with its duplicate count: a timestamp
//! that repeats is a property of the frame, not a gap in coverage.
//!
//! The dominant structure: candidate groupings — the time axis
//! itself, the axis's calendar cycles (month of year, day of week,
//! hour of day), and each dimension column — are scored by the
//! share of each measure's variance that sits between groups
//! (eta-squared). The strongest groupings are stated with their
//! peak. Nothing is stated below a 0.10 share: a frame with no
//! organising grouping gets no dominant-structure line rather than
//! a dressed-up weak one.
//!
//! Restatement demotion: findings whose subject is contained in
//! the calendar family (the axis, its parts, and every column the
//! exact dependencies derive from them) or in the scaffolding
//! columns are demoted in ranking via the ledger's triviality
//! mechanism — demoted, never removed, with the reason written
//! into the finding's evidence. Findings about a measure that
//! happen to sit on a calendar boundary are not demoted here; that
//! judgement is the cycle-aware demotion of step 4.

use crate::frame::{axis_ticks_micros, CardFacts, FrameModel, MICROS_PER_DAY};
use onceover_core::dateutil;
use onceover_core::{TypedCol, TypedTable};
use onceover_deps::fd::DepsOutcome;
use onceover_deps::hashutil;
use onceover_report::{
    round4, EvidenceBasis, Finding, FrameDominant, FrameGrain, FrameGrainFactor, FrameNearKey,
    FrameStructurePeak, FrameStructureShare, Method, Pattern, Scope,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// The step-2 account: the grain and dominant structure for the
/// report's `frame` section, the account's own findings, and the
/// subject sets of bare key findings the grain finding replaces.
pub struct Account {
    pub grain: Option<FrameGrain>,
    pub dominant: Vec<FrameDominant>,
    pub findings: Vec<Finding>,
    pub replaced_key_subjects: Vec<BTreeSet<String>>,
    /// Candidates existed (an axis, or dimension columns) but no
    /// exact key was found; the coverage note states this.
    pub grain_unresolved: bool,
}

/// Row cap for direct grain verification: above it the account
/// relies on the dependency pass's key results alone.
const GRAIN_VERIFY_MAX_ROWS: usize = 2_000_000;
/// A dominant structure must organise at least this share of some
/// measure's variance to be stated at all; up to two groupings are
/// stated, strongest first.
const DOMINANT_MIN_SHARE: f64 = 0.10;

fn fmt_int(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn fmt_num(x: f64) -> String {
    if x == x.trunc() && x.abs() < 1e15 {
        return format!("{}", x as i64);
    }
    let s = format!("{x:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn render_value(col: &TypedCol, r: usize) -> String {
    match col {
        TypedCol::Str { vals } => vals[r].unwrap_or("").to_string(),
        TypedCol::StrOwned { vals } => vals[r].clone().unwrap_or_default(),
        TypedCol::Bool { vals } => vals[r].map(|b| b.to_string()).unwrap_or_default(),
        TypedCol::Date { vals } => vals[r]
            .map(|d| dateutil::iso_date(d as i64))
            .unwrap_or_default(),
        TypedCol::Datetime { vals, unit } => vals[r]
            .map(|t| dateutil::iso_datetime(t * 1_000_000 / dateutil::ticks_per_second(*unit)))
            .unwrap_or_default(),
        _ => col.numeric_at(r).map(fmt_num).unwrap_or_default(),
    }
}

/// The columns the axis occupies in the profiled frame: its part
/// columns, or its single source column.
pub(crate) fn axis_subject_columns(frame: &FrameModel) -> Vec<String> {
    let Some(axis) = &frame.info.axis else {
        return Vec::new();
    };
    if !axis.parts.is_empty() {
        return axis.parts.clone();
    }
    match &axis.source_column {
        Some(c) => vec![c.clone()],
        None => vec![axis.name.clone()],
    }
}

/// One grain component: a dimension column, or the time axis.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Comp {
    Col(usize),
    Axis,
}

impl Comp {
    fn distinct(&self, facts: &[CardFacts], axis_distinct: usize) -> usize {
        match self {
            Comp::Col(i) => facts[*i].distinct,
            Comp::Axis => axis_distinct,
        }
    }
    fn label(&self, table: &TypedTable, frame: &FrameModel) -> String {
        match self {
            Comp::Col(i) => table.names[*i].clone(),
            Comp::Axis => frame
                .info
                .axis
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_default(),
        }
    }
}

/// Verify a candidate key by hashing its tuples: returns the number
/// of distinct tuples when every row's components are non-null, or
/// None when a null component disqualifies the candidate.
fn verify_key(table: &TypedTable, comps: &[Comp], ticks: &[Option<i64>]) -> Option<usize> {
    let mut seen: HashSet<Vec<u8>> = HashSet::with_capacity(table.n);
    let mut buf: Vec<u8> = Vec::with_capacity(32);
    for (r, tick) in ticks.iter().enumerate() {
        buf.clear();
        for comp in comps {
            match comp {
                Comp::Col(i) => {
                    if table.cols[*i].is_null_at(r) {
                        return None;
                    }
                    buf.push(b'c');
                    hashutil::encode_value(&table.cols[*i], r, &mut buf);
                }
                Comp::Axis => {
                    let t = (*tick)?;
                    buf.push(b'a');
                    buf.extend_from_slice(&t.to_le_bytes());
                }
            }
        }
        seen.insert(buf.clone());
    }
    Some(seen.len())
}

pub fn compute_account(
    table: &TypedTable,
    frame: &FrameModel,
    facts: &[CardFacts],
    deps: &DepsOutcome,
) -> Account {
    let ticks = axis_ticks_micros(table, frame);
    let (grain, findings, replaced, unresolved) = compute_grain(table, frame, facts, deps, &ticks);
    let dominant = dominant_structure(table, frame, &ticks);
    Account {
        grain,
        dominant,
        findings,
        replaced_key_subjects: replaced,
        grain_unresolved: unresolved,
    }
}

fn compute_grain(
    table: &TypedTable,
    frame: &FrameModel,
    facts: &[CardFacts],
    deps: &DepsOutcome,
    ticks: &Option<Vec<Option<i64>>>,
) -> (
    Option<FrameGrain>,
    Vec<Finding>,
    Vec<BTreeSet<String>>,
    bool,
) {
    let n = table.n;
    // Dimension-role columns only: flag-role columns state a
    // condition on the row and are never grain components.
    let dims: Vec<usize> = frame
        .roles
        .iter()
        .enumerate()
        .filter(|(_, r)| r.as_deref() == Some("dimension"))
        .map(|(i, _)| i)
        .collect();

    // The axis's distinct count serves both the exact-key
    // factorization and the near-key branch; compute it at most
    // once, on first use (the branches below do not both run).
    let mut axis_distinct_cache: Option<usize> = None;

    // ---- axis-crossed candidates, verified directly ----
    if let Some(ticks) = ticks {
        if (8..=GRAIN_VERIFY_MAX_ROWS).contains(&n) {
            let axis_distinct: usize = *axis_distinct_cache.get_or_insert_with(|| {
                ticks
                    .iter()
                    .flatten()
                    .copied()
                    .collect::<BTreeSet<_>>()
                    .len()
            });
            let mut by_distinct = dims.clone();
            by_distinct.sort_by_key(|&i| std::cmp::Reverse(facts[i].distinct));
            // The axis alone comes first: when it is already unique,
            // any dimension crossed with it is a non-minimal key.
            let mut candidates: Vec<Vec<Comp>> = vec![vec![Comp::Axis]];
            for &d in &by_distinct {
                candidates.push(vec![Comp::Col(d), Comp::Axis]);
            }
            if dims.len() >= 2 {
                let mut all: Vec<Comp> = dims.iter().take(3).map(|&d| Comp::Col(d)).collect();
                all.push(Comp::Axis);
                candidates.push(all);
            }
            for comps in &candidates {
                let Some(distinct_tuples) = verify_key(table, comps, ticks) else {
                    continue;
                };
                if distinct_tuples != n {
                    continue;
                }
                // The candidate is an exact key. Factorize it.
                let mut product: usize = 1;
                let mut overflow = false;
                let mut factorization: Vec<FrameGrainFactor> = Vec::new();
                for comp in comps {
                    let d = comp.distinct(facts, axis_distinct);
                    factorization.push(FrameGrainFactor {
                        column: comp.label(table, frame),
                        distinct: d,
                    });
                    match product.checked_mul(d) {
                        Some(p) => product = p,
                        None => overflow = true,
                    }
                }
                let balanced = !overflow && product == n;
                let mut subject_columns: Vec<String> = Vec::new();
                for comp in comps {
                    match comp {
                        Comp::Col(i) => subject_columns.push(table.names[*i].clone()),
                        Comp::Axis => subject_columns.extend(axis_subject_columns(frame)),
                    }
                }
                let cross = factorization
                    .iter()
                    .map(|f| format!("{} {}", fmt_int(f.distinct), f.column))
                    .collect::<Vec<_>>()
                    .join(" × ");
                let subject_text = if subject_columns.len() == 1 {
                    subject_columns[0].clone()
                } else {
                    format!("({})", subject_columns.join(", "))
                };
                let statement = if factorization.len() == 1 {
                    format!(
                        "{subject_text} uniquely identifies rows ({} rows)",
                        fmt_int(n)
                    )
                } else if balanced {
                    format!(
                        "the grain is {cross}: {} rows, every combination exactly once — {subject_text} jointly identify rows",
                        fmt_int(n)
                    )
                } else if overflow || product < n {
                    // Distinct counts from the cards can disagree
                    // with the verified tuple count on exotic value
                    // encodings; state only what was verified.
                    format!(
                        "{subject_text} jointly identify rows ({} rows, verified exactly)",
                        fmt_int(n)
                    )
                } else {
                    format!(
                        "the grain is {cross}: {subject_text} jointly identify rows in {} rows — the cross is not full ({} combinations, {} absent)",
                        fmt_int(n),
                        fmt_int(product),
                        fmt_int(product - n)
                    )
                };
                let levels: BTreeMap<String, usize> = factorization
                    .iter()
                    .map(|f| (f.column.clone(), f.distinct))
                    .collect();
                let mut raw = BTreeMap::new();
                raw.insert("columns".to_string(), json!(subject_columns));
                let mut metrics = BTreeMap::new();
                metrics.insert("n".to_string(), json!(n));
                metrics.insert("balanced".to_string(), json!(balanced));
                metrics.insert("levels".to_string(), json!(levels));
                if !overflow {
                    metrics.insert("combinations".to_string(), json!(product));
                }
                let mut finding = Finding::new(
                    Pattern::Uniqueness,
                    raw,
                    statement.clone(),
                    EvidenceBasis::Measured,
                    0.95,
                    metrics,
                    Some(
                        "the account's grain statement; it replaces the bare key finding for the same columns"
                            .into(),
                    ),
                    Method::new(
                        "keys",
                        "grain_factorization",
                        "frame model: exact key verified by hashing the component tuples; factorization from the components' distinct counts (onceover v0.1)",
                    ),
                );
                let mut facets = finding.facets;
                facets.scope = Scope::Frame;
                finding = finding.with_facets(facets);
                let subject_set: BTreeSet<String> = subject_columns.iter().cloned().collect();
                let grain = FrameGrain {
                    key_columns: subject_columns,
                    factorization,
                    balanced,
                    combinations: if overflow { None } else { Some(product) },
                    near_key: None,
                    statement,
                };
                return (Some(grain), vec![finding], vec![subject_set], false);
            }
        }
    }

    // ---- the dependency pass's exact key, without factorization ----
    // A unique float is a measurement, not what the rows are keyed by.
    // A key that needs a flag column is not the grain either: a flag
    // states a condition on the row, it is not an entity axis. The
    // dependency pass still reports such keys as its own key
    // findings; the frame model does not promote them to the grain.
    let is_flag = |i: usize| frame.roles[i].as_deref() == Some("flag");
    let single_key = deps
        .single_unique
        .iter()
        .find(|&&i| !matches!(table.cols[i], TypedCol::F64 { .. }) && !is_flag(i));
    let deps_key: Option<Vec<String>> = if let Some(&i) = single_key {
        Some(vec![table.names[i].clone()])
    } else {
        deps.composite_uccs
            .iter()
            .find(|ucc| ucc.iter().all(|&i| !is_flag(i)))
            .map(|ucc| ucc.iter().map(|&i| table.names[i].clone()).collect())
    };
    if let Some(key_columns) = deps_key {
        let statement = if key_columns.len() == 1 {
            format!("{} uniquely identifies rows", key_columns[0])
        } else {
            format!("({}) jointly identify rows", key_columns.join(", "))
        };
        return (
            Some(FrameGrain {
                key_columns,
                factorization: Vec::new(),
                balanced: false,
                combinations: None,
                near_key: None,
                statement,
            }),
            Vec::new(),
            Vec::new(),
            false,
        );
    }

    // ---- near key: the axis nearly identifies rows ----
    if let Some(ticks) = ticks {
        let distinct: usize = *axis_distinct_cache.get_or_insert_with(|| {
            ticks
                .iter()
                .flatten()
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
        });
        let duplicate_rows = n - distinct.min(n);
        if distinct >= 8 && duplicate_rows >= 1 && (distinct as f64) >= 0.5 * n as f64 {
            let columns = axis_subject_columns(frame);
            let axis_label = frame
                .info
                .axis
                .as_ref()
                .and_then(|a| a.source_column.clone())
                .unwrap_or_else(|| columns.join("/"));
            let statement = format!(
                "the time axis does not identify rows: {} distinct {axis_label} values in {} rows — {} rows repeat a timestamp already present",
                fmt_int(distinct),
                fmt_int(n),
                fmt_int(duplicate_rows)
            );
            let mut raw = BTreeMap::new();
            raw.insert("columns".to_string(), json!(columns));
            let mut metrics = BTreeMap::new();
            metrics.insert("n".to_string(), json!(n));
            metrics.insert("distinct_values".to_string(), json!(distinct));
            metrics.insert("duplicate_rows".to_string(), json!(duplicate_rows));
            let mut finding = Finding::new(
                Pattern::Duplicates,
                raw,
                statement.clone(),
                EvidenceBasis::Measured,
                0.75,
                metrics,
                Some(
                    "the axis is the frame's natural key candidate; its repeats are a property of the frame (a second record for the same timestamp), distinct from exact duplicate rows"
                        .into(),
                ),
                Method::new(
                    "keys",
                    "near_key_axis",
                    "frame model: distinct count of the assembled time axis against the row count (onceover v0.1)",
                ),
            );
            let mut facets = finding.facets;
            facets.scope = Scope::Frame;
            finding = finding.with_facets(facets);
            let grain = FrameGrain {
                key_columns: Vec::new(),
                factorization: Vec::new(),
                balanced: false,
                combinations: None,
                near_key: Some(FrameNearKey {
                    columns,
                    distinct,
                    duplicate_rows,
                }),
                statement,
            };
            return (Some(grain), vec![finding], Vec::new(), false);
        }
    }

    // No key: unresolved only when candidates existed to test.
    let unresolved = ticks.is_some() || !dims.is_empty();
    (None, Vec::new(), Vec::new(), unresolved)
}

// ---------------------------------------------------------------------
// dominant structure
// ---------------------------------------------------------------------

struct Grouping {
    kind: &'static str,
    label: String,
    grouping: String,
    /// One group key per row (None = the row has no group).
    keys: Vec<Option<i64>>,
    /// Peak-group labels by key, for groupings whose labels are
    /// data (dimension levels, time points).
    peak_labels: Option<BTreeMap<i64, String>>,
    /// Peak-group label from the key, for calendar groupings.
    peak_label_fn: Option<fn(i64) -> String>,
}

fn month_name(m: i64) -> String {
    [
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
    ][(m - 1).clamp(0, 11) as usize]
        .to_string()
}

fn weekday_name(w: i64) -> String {
    [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ][w.clamp(0, 6) as usize]
        .to_string()
}

/// The label of a time point at the axis's own granularity: the
/// month for a monthly axis, the date for a daily one, the datetime
/// otherwise. The two axis-wide alignment facts are computed once
/// by the caller; they were previously re-scanned from the full
/// tick vector on every call (once per distinct tick).
fn time_peak_label(day_aligned: bool, all_month_starts: bool, key: i64) -> String {
    if day_aligned {
        let days = key.div_euclid(MICROS_PER_DAY);
        let (y, m, _) = dateutil::civil_from_days(days);
        if all_month_starts {
            return format!("{y:04}-{m:02}");
        }
        return dateutil::iso_date(days);
    }
    dateutil::iso_datetime(key)
}

pub fn dominant_structure(
    table: &TypedTable,
    frame: &FrameModel,
    ticks: &Option<Vec<Option<i64>>>,
) -> Vec<FrameDominant> {
    let measures: Vec<usize> = frame
        .roles
        .iter()
        .enumerate()
        .filter(|(_, r)| r.as_deref() == Some("measure"))
        .map(|(i, _)| i)
        .filter(|&i| table.cols[i].is_numeric())
        .take(24)
        .collect();
    if measures.is_empty() || table.n < 30 {
        return Vec::new();
    }

    let mut groupings: Vec<Grouping> = Vec::new();
    if let Some(ticks) = ticks {
        let present: Vec<i64> = ticks.iter().flatten().copied().collect();
        if !present.is_empty() {
            let distinct_ticks: BTreeSet<i64> = present.iter().copied().collect();
            let days_of = |t: i64| t.div_euclid(MICROS_PER_DAY);
            let days_min = days_of(*present.iter().min().expect("non-empty"));
            let days_max = days_of(*present.iter().max().expect("non-empty"));
            let span_days = days_max - days_min;
            // The time axis itself, when rows replicate over it
            // (a panel): the shared time pattern.
            if distinct_ticks.len() >= 2 && distinct_ticks.len() * 2 <= table.n {
                let day_aligned = present.iter().all(|&t| t.rem_euclid(MICROS_PER_DAY) == 0);
                let all_month_starts = day_aligned
                    && present
                        .iter()
                        .all(|&t| dateutil::civil_from_days(t.div_euclid(MICROS_PER_DAY)).2 == 1);
                let peak_labels: BTreeMap<i64, String> = distinct_ticks
                    .iter()
                    .map(|&t| (t, time_peak_label(day_aligned, all_month_starts, t)))
                    .collect();
                groupings.push(Grouping {
                    kind: "time",
                    label: "shared time pattern".to_string(),
                    grouping: "axis value".to_string(),
                    keys: ticks.clone(),
                    peak_labels: Some(peak_labels),
                    peak_label_fn: None,
                });
            }
            // A cycle grouping is only meaningful when the axis
            // samples the cycle: the modal step must be no coarser
            // than the cycle's period. (Month-start dates fall on
            // every weekday in turn; a "weekly cycle" measured on a
            // monthly axis is an artifact of the sampling, not a
            // pattern in the data.)
            let modal_step: Option<i64> = {
                let d: Vec<i64> = distinct_ticks.iter().copied().collect();
                crate::frame::modal_diff(&d)
            };
            let step_days = modal_step.map(|s| s as f64 / MICROS_PER_DAY as f64);
            let (y0, _, _) = dateutil::civil_from_days(days_min);
            let (y1, _, _) = dateutil::civil_from_days(days_max);
            if y0 != y1 && step_days.is_some_and(|s| s <= 31.0) {
                groupings.push(Grouping {
                    kind: "cycle",
                    label: "annual cycle".to_string(),
                    grouping: "month of year".to_string(),
                    keys: ticks
                        .iter()
                        .map(|t| t.map(|t| dateutil::civil_from_days(days_of(t)).1 as i64))
                        .collect(),
                    peak_labels: None,
                    peak_label_fn: Some(month_name),
                });
            }
            if span_days >= 14 && step_days.is_some_and(|s| s <= 1.0) {
                groupings.push(Grouping {
                    kind: "cycle",
                    label: "weekly cycle".to_string(),
                    grouping: "day of week".to_string(),
                    keys: ticks
                        .iter()
                        .map(|t| t.map(|t| dateutil::weekday(days_of(t)) as i64))
                        .collect(),
                    peak_labels: None,
                    peak_label_fn: Some(weekday_name),
                });
            }
            let sub_day = present.iter().any(|t| t.rem_euclid(MICROS_PER_DAY) != 0);
            if sub_day && step_days.is_some_and(|s| s < 1.0) {
                groupings.push(Grouping {
                    kind: "cycle",
                    label: "daily cycle".to_string(),
                    grouping: "hour of day".to_string(),
                    keys: ticks
                        .iter()
                        .map(|t| t.map(|t| t.rem_euclid(MICROS_PER_DAY) / 3_600_000_000))
                        .collect(),
                    peak_labels: None,
                    peak_label_fn: Some(|h| format!("{h:02}:00")),
                });
            }
        }
    }
    for (i, role) in frame.roles.iter().enumerate() {
        if role.as_deref() != Some("dimension") {
            continue;
        }
        // Group ids in first-appearance order over encoded values;
        // the peak label of a group is its level, rendered from the
        // first row that carries it.
        let mut ids: HashMap<Vec<u8>, i64> = HashMap::new();
        let mut peak_labels: BTreeMap<i64, String> = BTreeMap::new();
        let mut keys: Vec<Option<i64>> = Vec::with_capacity(table.n);
        let mut buf: Vec<u8> = Vec::new();
        for r in 0..table.n {
            if table.cols[i].is_null_at(r) {
                keys.push(None);
                continue;
            }
            buf.clear();
            hashutil::encode_value(&table.cols[i], r, &mut buf);
            let next = ids.len() as i64;
            let id = *ids.entry(buf.clone()).or_insert(next);
            peak_labels
                .entry(id)
                .or_insert_with(|| render_value(&table.cols[i], r));
            keys.push(Some(id));
        }
        if ids.len() < 2 || ids.len() > 256 {
            continue;
        }
        let name = table.names[i].clone();
        groupings.push(Grouping {
            kind: "dimension",
            label: name.clone(),
            grouping: name,
            keys,
            peak_labels: Some(peak_labels),
            peak_label_fn: None,
        });
    }

    // Score every (grouping, measure) pair by eta-squared: the
    // between-group sum of squares over the total.
    struct Scored {
        order: usize,
        score: f64,
        shares: Vec<FrameStructureShare>,
        peak: Option<FrameStructurePeak>,
    }
    let mut scored: Vec<Scored> = Vec::new();
    for (order, g) in groupings.iter().enumerate() {
        let mut group_ids: HashMap<i64, usize> = HashMap::new();
        let mut row_group: Vec<Option<usize>> = Vec::with_capacity(table.n);
        for k in &g.keys {
            row_group.push(k.map(|key| {
                let next = group_ids.len();
                *group_ids.entry(key).or_insert(next)
            }));
        }
        let n_groups = group_ids.len();
        if !(2..=10_000).contains(&n_groups) {
            continue;
        }
        let mut shares: Vec<FrameStructureShare> = Vec::new();
        let mut best: Option<(f64, usize, i64, f64)> = None;
        for &mi in &measures {
            let col = &table.cols[mi];
            let mut sums: Vec<(usize, f64)> = vec![(0, 0.0); n_groups];
            let (mut cnt, mut sum, mut sumsq) = (0usize, 0.0f64, 0.0f64);
            for (r, &gi) in row_group.iter().enumerate() {
                let (Some(x), Some(gi)) = (col.numeric_at(r), gi) else {
                    continue;
                };
                if !x.is_finite() {
                    continue;
                }
                sums[gi].0 += 1;
                sums[gi].1 += x;
                cnt += 1;
                sum += x;
                sumsq += x * x;
            }
            if cnt < 30 {
                continue;
            }
            let total_ss = sumsq - sum * sum / cnt as f64;
            if total_ss <= 0.0 {
                continue;
            }
            let mut between_ss = 0.0;
            for &(c, s) in &sums {
                if c > 0 {
                    between_ss += s * s / c as f64;
                }
            }
            between_ss -= sum * sum / cnt as f64;
            let share = (between_ss / total_ss).clamp(0.0, 1.0);
            if share >= DOMINANT_MIN_SHARE {
                shares.push(FrameStructureShare {
                    measure: table.names[mi].clone(),
                    share: round4(share),
                });
            }
            if best.is_none_or(|(bs, _, _, _)| share > bs) {
                let mut peak: Option<(i64, f64)> = None;
                for (key, &gi) in &group_ids {
                    let (c, s) = sums[gi];
                    if c == 0 {
                        continue;
                    }
                    let mean = s / c as f64;
                    if peak.is_none_or(|(_, pm)| mean > pm) {
                        peak = Some((*key, mean));
                    }
                }
                if let Some((key, mean)) = peak {
                    best = Some((share, mi, key, mean));
                }
            }
        }
        if shares.is_empty() {
            continue;
        }
        shares.sort_by(|a, b| b.share.total_cmp(&a.share));
        shares.truncate(3);
        let score = shares[0].share;
        let peak = best.and_then(|(_, mi, key, mean)| {
            let label = match (&g.peak_labels, g.peak_label_fn) {
                (Some(labels), _) => labels.get(&key)?.clone(),
                (None, Some(f)) => f(key),
                (None, None) => return None,
            };
            Some(FrameStructurePeak {
                label,
                measure: table.names[mi].clone(),
                mean: round4(mean),
            })
        });
        scored.push(Scored {
            order,
            score,
            shares,
            peak,
        });
    }
    // Selection: at most two groupings, strongest first. Every
    // scored grouping already clears the dominance bar on its
    // strongest measure (a tenth of that measure's variance
    // between the grouping's levels); the shares are printed in
    // the statement, so a 0.11 structure next to a 0.82 one is
    // self-calibrating. Below the bar the account states no
    // dominant structure rather than dressing up noise.
    scored.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.order.cmp(&b.order)));
    let chosen: Vec<&Scored> = scored.iter().take(2).collect();
    chosen
        .iter()
        .map(|s| {
            let g = &groupings[s.order];
            let share_text = s
                .shares
                .iter()
                .map(|sh| {
                    format!(
                        "{}% of the variance in {}",
                        (sh.share * 100.0).round() as i64,
                        sh.measure
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let between = match g.kind {
                "time" => "time points".to_string(),
                "cycle" if g.grouping == "month of year" => "months of the year".to_string(),
                "cycle" if g.grouping == "day of week" => "days of the week".to_string(),
                "cycle" if g.grouping == "hour of day" => "hours of the day".to_string(),
                _ => format!("{} values", g.label),
            };
            let peak_phrase = match &s.peak {
                Some(p) if g.kind == "dimension" => format!(
                    "; {} averages highest for {} ({})",
                    p.measure,
                    p.label,
                    fmt_num(p.mean)
                ),
                Some(p) if g.grouping == "day of week" => format!(
                    "; {} averages highest on {} ({})",
                    p.measure,
                    p.label,
                    fmt_num(p.mean)
                ),
                Some(p) if g.grouping == "hour of day" => format!(
                    "; {} averages highest at {} ({})",
                    p.measure,
                    p.label,
                    fmt_num(p.mean)
                ),
                Some(p) => format!(
                    "; {} averages highest in {} ({})",
                    p.measure,
                    p.label,
                    fmt_num(p.mean)
                ),
                None => String::new(),
            };
            let statement = if g.kind == "dimension" {
                format!(
                    "the {} dimension organises the frame: {} sits between {}{}",
                    g.label, share_text, between, peak_phrase
                )
            } else {
                format!(
                    "the {} organises the frame: {} sits between {}{}",
                    g.label, share_text, between, peak_phrase
                )
            };
            FrameDominant {
                kind: g.kind.to_string(),
                label: g.label.clone(),
                grouping: g.grouping.clone(),
                shares: s.shares.clone(),
                peak: s.peak.clone(),
                statement,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------
// restatement demotion
// ---------------------------------------------------------------------

/// The calendar family: the axis's own columns (its parts, or its
/// source column), plus every column the axis determines exactly
/// (measured directly — see the seed in the body), closed under
/// the exact functional dependencies. Every column the calendar
/// determines exactly is a calendar restatement waiting to happen.
/// Returns (family, base): the base is the axis's own columns;
/// family \ base are the derived columns.
pub fn calendar_family(
    table: &TypedTable,
    frame: &FrameModel,
    deps: &DepsOutcome,
) -> (BTreeSet<usize>, BTreeSet<usize>) {
    let mut base: BTreeSet<usize> = BTreeSet::new();
    if let Some(axis) = &frame.info.axis {
        let names: Vec<&str> = if !axis.parts.is_empty() {
            axis.parts.iter().map(|s| s.as_str()).collect()
        } else if let Some(src) = &axis.source_column {
            vec![src.as_str()]
        } else {
            vec![]
        };
        for name in names {
            if let Some(i) = table.names.iter().position(|n| n == name) {
                base.insert(i);
            }
        }
    }
    let mut family = base.clone();
    // The seed cannot come from the mined dependencies alone:
    // where the timestamp is itself unique, the dependency pass
    // reports no dependency from it, so the closure would have
    // nothing to start from. Measure the seed directly instead:
    // a column the axis determines exactly —
    // every axis value co-occurs with exactly one value of the
    // column — is a calendar derivation by construction (month
    // from date, holiday from date, season from date). Measures
    // are not tested: a measure that happens to be an exact
    // function of the axis is a time pattern, not a derivation.
    if let Some(ticks) = axis_ticks_micros(table, frame) {
        let mut tested = 0usize;
        for (i, role) in frame.roles.iter().enumerate() {
            if family.contains(&i) || role.as_deref() == Some("measure") {
                continue;
            }
            if tested >= 128 {
                break;
            }
            tested += 1;
            if axis_determines(table, &ticks, i) {
                family.insert(i);
            }
        }
    }
    loop {
        let mut grew = false;
        for (lhs, rhs) in &deps.exact_fds {
            if !family.contains(rhs) && lhs.iter().all(|c| family.contains(c)) {
                family.insert(*rhs);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    (family, base)
}

/// Whether the axis value determines column `col` exactly: over
/// the rows that carry a tick, each distinct tick co-occurs with
/// exactly one value of the column (a null counts as a value).
fn axis_determines(table: &TypedTable, ticks: &[Option<i64>], col: usize) -> bool {
    let mut seen: std::collections::HashMap<i64, Vec<u8>> = std::collections::HashMap::new();
    let mut buf: Vec<u8> = Vec::new();
    for (r, tick) in ticks.iter().enumerate() {
        let Some(t) = *tick else { continue };
        buf.clear();
        if table.cols[col].is_null_at(r) {
            buf.push(0xFE);
        } else {
            buf.push(0x01);
            hashutil::encode_value(&table.cols[col], r, &mut buf);
        }
        match seen.get(&t) {
            Some(prev) => {
                if *prev != buf {
                    return false;
                }
            }
            None => {
                seen.insert(t, buf.clone());
            }
        }
    }
    !seen.is_empty()
}

/// Demote restatements in place, ahead of the ledger: a finding is
/// a restatement when every column of its subject is inside the
/// calendar family and at least one is a derived column
/// (`calendar_derivation`), or when its whole subject is
/// scaffolding (`scaffolding_subject`). The demotion is declared
/// in the evidence metrics (`triviality`, `triviality_factor`
/// 0.6), the mechanism the ledger already applies and records;
/// nothing is removed. Key statements and frame-scope findings
/// are the account itself and are never demoted here, and a
/// finding a producer already demoted keeps its own declaration.
/// Returns the demotion counts by reason.
pub fn demote_restatements(
    findings: &mut [Finding],
    table: &TypedTable,
    frame: &FrameModel,
    deps: &DepsOutcome,
) -> BTreeMap<String, usize> {
    let (family, base) = calendar_family(table, frame, deps);
    let derived: BTreeSet<usize> = family.difference(&base).copied().collect();
    let scaffold: BTreeSet<usize> = frame
        .roles
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            matches!(
                r.as_deref(),
                Some("index" | "identifier" | "timestamp" | "timestamp_part")
            )
        })
        .map(|(i, _)| i)
        .collect();
    let index_of: HashMap<&str, usize> = table
        .names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    if derived.is_empty() && scaffold.is_empty() {
        return counts;
    }
    for f in findings.iter_mut() {
        if f.facets.scope == Scope::Frame {
            continue;
        }
        if f.category == onceover_report::Category::Uniqueness {
            continue;
        }
        if f.evidence.iter().any(|e| e.method.detector == "keys") {
            continue;
        }
        if f.evidence
            .iter()
            .any(|e| e.metrics.contains_key("triviality"))
        {
            continue;
        }
        let cols: Option<BTreeSet<usize>> = f
            .subject
            .columns
            .iter()
            .map(|c| index_of.get(c.as_str()).copied())
            .collect();
        let Some(cols) = cols else { continue };
        if cols.is_empty() {
            continue;
        }
        // The calendar rule targets relation findings — a stated
        // dependency, association, or rule between calendar
        // columns restates the derivation. A finding about a
        // derived column's own content (its concentration, its
        // format) is not a restatement and keeps its rank.
        let relation = matches!(
            f.category,
            onceover_report::Category::Dependencies
                | onceover_report::Category::Associations
                | onceover_report::Category::ConditionalRules
        );
        let reason = if relation
            && !derived.is_empty()
            && cols.iter().all(|c| family.contains(c))
            && cols.iter().any(|c| derived.contains(c))
        {
            Some("calendar_derivation")
        } else if cols.iter().all(|c| scaffold.contains(c)) {
            Some("scaffolding_subject")
        } else {
            None
        };
        if let Some(reason) = reason {
            for e in &mut f.evidence {
                e.metrics.insert("triviality".to_string(), json!(reason));
                e.metrics
                    .insert("triviality_factor".to_string(), json!(0.6));
            }
            *counts.entry(reason.to_string()).or_default() += 1;
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::analyze_frame;
    use onceover_core::extract;
    use polars::prelude::*;

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

    fn empty_deps() -> DepsOutcome {
        DepsOutcome {
            findings: Vec::new(),
            exact_fds: Vec::new(),
            composite_uccs: Vec::new(),
            single_unique: Vec::new(),
            n_exact: 0,
            n_afd: 0,
            coverage: String::new(),
            triple_key_search: false,
            caps_bound: Vec::new(),
            sampled: false,
        }
    }

    #[test]
    fn balanced_panel_factorizes() {
        // 3 states x 12 months, one row per combination.
        let mut state = Vec::new();
        let mut date = Vec::new();
        let mut rate = Vec::new();
        for s in ["CA", "MI", "NV"] {
            for m in 1..=12 {
                state.push(s);
                date.push(format!("2024-{m:02}-01"));
                rate.push(4.0 + m as f64);
            }
        }
        let df = DataFrame::new_infer_height(vec![
            Series::new("state".into(), state).into(),
            Series::new("date".into(), date).into(),
            Series::new("rate".into(), rate).into(),
        ])
        .unwrap();
        let (table, facts) = facts_for(&df);
        let frame = analyze_frame(&table, &facts, None);
        let account = compute_account(&table, &frame, &facts, &empty_deps());
        let grain = account.grain.expect("grain computed");
        assert!(grain.balanced, "statement: {}", grain.statement);
        assert_eq!(grain.combinations, Some(36));
        assert!(grain.statement.contains("3 state"), "{}", grain.statement);
        assert!(grain.statement.contains("12 date"), "{}", grain.statement);
        assert_eq!(account.findings.len(), 1);
        assert_eq!(account.findings[0].facets.scope, Scope::Frame);
        assert_eq!(
            account.findings[0].category,
            onceover_report::Category::Uniqueness
        );
        let replaced: BTreeSet<String> = ["date".to_string(), "state".to_string()]
            .into_iter()
            .collect();
        assert_eq!(account.replaced_key_subjects, vec![replaced]);
        // The axis carries its measured span and monthly step.
        let axis = frame.info.axis.as_ref().expect("axis");
        assert_eq!(axis.min.as_deref(), Some("2024-01-01"));
        assert_eq!(axis.max.as_deref(), Some("2024-12-01"));
        assert_eq!(axis.distinct_values, Some(12));
        assert_eq!(axis.step.as_deref(), Some("1 month"));
        // No weekly cycle on a monthly axis: month-start dates fall
        // on every weekday in turn, so the grouping would measure
        // the sampling, not the data.
        assert!(
            account.dominant.iter().all(|d| d.label != "weekly cycle"),
            "dominant: {:?}",
            account
                .dominant
                .iter()
                .map(|d| &d.label)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn repeating_axis_is_a_near_key() {
        // 20 hourly rows over 12 distinct timestamps, no dimensions.
        let mut ts = Vec::new();
        let mut volume = Vec::new();
        for r in 0..20 {
            ts.push(format!("2024-03-01 {:02}:00:00", r % 12));
            volume.push(100.0 + (r % 5) as f64);
        }
        let df = DataFrame::new_infer_height(vec![
            Series::new("date_time".into(), ts).into(),
            Series::new("volume".into(), volume).into(),
        ])
        .unwrap();
        let (table, facts) = facts_for(&df);
        let frame = analyze_frame(&table, &facts, None);
        let account = compute_account(&table, &frame, &facts, &empty_deps());
        let grain = account.grain.expect("grain computed");
        let near = grain.near_key.expect("near key recorded");
        assert_eq!(near.distinct, 12);
        assert_eq!(near.duplicate_rows, 8);
        assert_eq!(account.findings.len(), 1);
        assert_eq!(
            account.findings[0].category,
            onceover_report::Category::Duplicates
        );
        assert!(
            grain.statement.contains("12 distinct"),
            "{}",
            grain.statement
        );
    }

    #[test]
    fn annual_cycle_is_dominant() {
        // Two years of daily values driven by the season.
        let mut date = Vec::new();
        let mut temp = Vec::new();
        let start = dateutil::days_from_civil(2023, 1, 1);
        for k in 0..731 {
            let days = start + k;
            let (y, m, d) = dateutil::civil_from_days(days);
            date.push(format!("{y:04}-{m:02}-{d:02}"));
            let seasonal = -((k as f64) * 2.0 * std::f64::consts::PI / 365.25).cos();
            temp.push(15.0 + 10.0 * seasonal + ((k * 37) % 7) as f64 * 0.1);
        }
        let df = DataFrame::new_infer_height(vec![
            Series::new("date".into(), date).into(),
            Series::new("temp".into(), temp).into(),
        ])
        .unwrap();
        let (table, facts) = facts_for(&df);
        let frame = analyze_frame(&table, &facts, None);
        let account = compute_account(&table, &frame, &facts, &empty_deps());
        assert!(
            !account.dominant.is_empty(),
            "a dominant structure is stated"
        );
        assert_eq!(account.dominant[0].label, "annual cycle");
        assert_eq!(account.dominant[0].shares[0].measure, "temp");
        assert!(account.dominant[0].shares[0].share > 0.8);
        let peak = account.dominant[0].peak.as_ref().expect("peak");
        assert_eq!(peak.label, "July");
    }

    #[test]
    fn calendar_family_closes_under_exact_fds() {
        // dteday -> mnth -> season: both derived columns join the
        // family; a finding over them is a calendar restatement.
        let mut dteday = Vec::new();
        let mut mnth = Vec::new();
        let mut season = Vec::new();
        let mut cnt = Vec::new();
        let start = dateutil::days_from_civil(2024, 1, 1);
        // Two rows per day, with cnt differing between them: on a
        // one-row-per-day frame every column is trivially a
        // function of the date, and the fixture needs cnt to be a
        // genuine measure the axis does not determine.
        for k in 0..120 {
            let days = start + k;
            let (_, m, _) = dateutil::civil_from_days(days);
            for rep in 0..2 {
                dteday.push(dateutil::iso_date(days));
                mnth.push(m as i64);
                season.push(((m - 1) / 3 + 1) as i64);
                cnt.push(((k + 3 * rep) % 9) as f64);
            }
        }
        let df = DataFrame::new_infer_height(vec![
            Series::new("dteday".into(), dteday).into(),
            Series::new("mnth".into(), mnth).into(),
            Series::new("season".into(), season).into(),
            Series::new("cnt".into(), cnt).into(),
        ])
        .unwrap();
        let (table, facts) = facts_for(&df);
        let frame = analyze_frame(&table, &facts, None);
        let idx = |name: &str| table.names.iter().position(|n| n == name).unwrap();
        let mut deps = empty_deps();
        deps.exact_fds = vec![
            (vec![idx("dteday")], idx("mnth")),
            (vec![idx("mnth")], idx("season")),
        ];
        let (family, base) = calendar_family(&table, &frame, &deps);
        assert!(base.contains(&idx("dteday")));
        assert!(family.contains(&idx("season")), "closure reaches season");

        let fd = |lhs: &str, rhs: &str, detector: &str| {
            Finding::new(
                Pattern::FunctionalDependency,
                BTreeMap::from([
                    ("lhs".to_string(), json!([lhs])),
                    ("rhs".to_string(), json!(rhs)),
                ]),
                format!("{lhs} determines {rhs}"),
                EvidenceBasis::Measured,
                0.9,
                BTreeMap::from([("violations".to_string(), json!(0))]),
                None,
                Method::new(detector, "value_mapping", "test"),
            )
        };
        let mut findings = vec![
            fd("mnth", "season", "functional_dependencies"),
            fd("cnt", "season", "functional_dependencies"),
        ];
        let counts = demote_restatements(&mut findings, &table, &frame, &deps);
        assert_eq!(counts.get("calendar_derivation"), Some(&1));
        assert_eq!(
            findings[0].evidence[0]
                .metrics
                .get("triviality")
                .and_then(|v| v.as_str()),
            Some("calendar_derivation")
        );
        // The measure-involving dependency is not a restatement.
        assert!(!findings[1]
            .evidence
            .iter()
            .any(|e| e.metrics.contains_key("triviality")));
    }
}
