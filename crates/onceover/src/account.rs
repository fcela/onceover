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
//! hour of day), each dimension column, and each continuous
//! measure binned into quantile bins — are scored by the share of
//! each response's variance that sits between groups
//! (eta-squared). The responses are the measures plus every
//! two-level column: a flag, or a dimension with exactly two
//! levels, is measured as the rate of its on state (the higher
//! level in sorted order), and a grouping that organises a binary
//! response states the peak group's rate, its lift over the base
//! rate, and its support alongside the variance share. Duplicate
//! and rollup encodings among the candidate groupings are
//! collapsed before scoring, so two encodings of one grouping
//! never occupy both account slots. Two vacuity guards keep the
//! account from stating constructions as structure: a grouping
//! whose two groups restate a binary response under another
//! column's encoding is not used as a grouping, and a grouping
//! never claims a response it determines exactly — constant
//! within every group — since that share is 1 by construction.
//! A column named as a weight is
//! not a measure for the account. The strongest groupings are
//! stated with their peak, subject to a materiality bar: a peak
//! whose group-mean spread is below a tenth of the measure's own
//! range is not stated. Nothing is stated below a 0.10 share: a
//! frame with no organising grouping gets no dominant-structure
//! line rather than a dressed-up weak one.
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
    FrameStructurePeak, FrameStructureRate, FrameStructureShare, Method, Pattern, Scope,
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
/// Materiality bar for a rendered peak: the spread between the
/// highest and lowest group means must reach this fraction of the
/// measure's own range (maximum minus minimum), or the peak is
/// not stated as a headline claim.
const PEAK_MATERIALITY_FRACTION: f64 = 0.10;
/// A binary rate contrast is stated only when the peak group's
/// rate exceeds the lowest eligible group's rate by at least this
/// many rate points.
const RATE_MIN_CONTRAST: f64 = 0.10;
/// A group needs at least this many rows carrying the response
/// before its rate can be the stated peak rate.
const RATE_PEAK_MIN_SUPPORT: usize = 20;

/// Whether a column name marks it as a weight (a sampling or case
/// weight, not a measurement of the row): a whole name token of
/// `weight` / `weights` / `wt`, or the census `fnlwgt` spelling.
pub(crate) fn weight_like_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == "fnlwgt" {
        return true;
    }
    lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|t| matches!(t, "weight" | "weights" | "wt"))
}

fn fmt_pct(x: f64) -> String {
    let p = x * 100.0;
    if (p - p.round()).abs() < 0.05 {
        format!("{}", p.round() as i64)
    } else {
        format!("{p:.1}")
    }
}

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
    let dominant = dominant_structure(table, frame, facts, &ticks);
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
    /// The source column of a categorical grouping (a dimension
    /// column, or the measure a binned grouping bins); None for
    /// the time and cycle groupings.
    source: Option<usize>,
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

/// One two-level response: a flag column, or a column with
/// exactly two distinct levels, measured as the rate of its on
/// state. The on state is the higher level in sorted order (the
/// affirmative label for labelled outcomes: "yes" over "no",
/// ">50K" over "<=50K", 1 over 0, true over false).
struct BinaryResponse {
    col: usize,
    name: String,
    on_label: String,
    values: Vec<Option<f64>>,
    base_rate: f64,
}

/// The frame's binary responses, in column order, with duplicate
/// encodings collapsed: a second column whose on/off pattern is
/// the same as (or the complement of) an earlier response on
/// every row is the same outcome under another encoding and is
/// not measured twice.
fn binary_responses(
    table: &TypedTable,
    frame: &FrameModel,
    facts: &[CardFacts],
) -> Vec<BinaryResponse> {
    let axis_cols: HashSet<String> = axis_subject_columns(frame).into_iter().collect();
    let mut out: Vec<BinaryResponse> = Vec::new();
    for (i, role) in frame.roles.iter().enumerate() {
        let role = role.as_deref();
        if !matches!(role, Some("flag" | "dimension" | "measure")) {
            continue;
        }
        if axis_cols.contains(&table.names[i]) || weight_like_name(&table.names[i]) {
            continue;
        }
        let (values, on_label): (Vec<Option<f64>>, String) = if role == Some("flag") {
            let Some(states) = onceover_core::two_valued_flag_states(table, i) else {
                continue;
            };
            let on_label = if matches!(table.cols[i], TypedCol::Bool { .. }) {
                "true".to_string()
            } else {
                "1".to_string()
            };
            (
                states
                    .iter()
                    .map(|s| s.map(|b| if b { 1.0 } else { 0.0 }))
                    .collect(),
                on_label,
            )
        } else if facts[i].distinct == 2 {
            if table.cols[i].is_numeric() {
                let mut lo = f64::INFINITY;
                let mut hi = f64::NEG_INFINITY;
                for r in 0..table.n {
                    if let Some(x) = table.cols[i].numeric_at(r) {
                        if x.is_finite() {
                            lo = lo.min(x);
                            hi = hi.max(x);
                        }
                    }
                }
                if !lo.is_finite() || lo == hi {
                    continue;
                }
                (
                    (0..table.n)
                        .map(|r| {
                            table.cols[i]
                                .numeric_at(r)
                                .map(|x| if x == hi { 1.0 } else { 0.0 })
                        })
                        .collect(),
                    fmt_num(hi),
                )
            } else {
                let mut levels: BTreeSet<String> = BTreeSet::new();
                for r in 0..table.n {
                    if !table.cols[i].is_null_at(r) {
                        levels.insert(render_value(&table.cols[i], r));
                    }
                }
                if levels.len() != 2 {
                    continue;
                }
                let on = levels.iter().next_back().expect("two levels").clone();
                (
                    (0..table.n)
                        .map(|r| {
                            if table.cols[i].is_null_at(r) {
                                None
                            } else {
                                Some(if render_value(&table.cols[i], r) == on {
                                    1.0
                                } else {
                                    0.0
                                })
                            }
                        })
                        .collect(),
                    on,
                )
            }
        } else {
            continue;
        };
        let support = values.iter().flatten().count();
        if support < 30 {
            continue;
        }
        let base_rate = values.iter().flatten().sum::<f64>() / support as f64;
        if base_rate <= 0.0 || base_rate >= 1.0 {
            continue;
        }
        let duplicate = out.iter().any(|prev: &BinaryResponse| {
            prev.values.len() == values.len()
                && prev
                    .values
                    .iter()
                    .zip(values.iter())
                    .all(|(a, b)| match (a, b) {
                        (None, None) => true,
                        (Some(a), Some(b)) => a == b || a + b == 1.0,
                        _ => false,
                    })
                && {
                    // Equal everywhere or complement everywhere;
                    // a mix is a different pattern.
                    let mut same = true;
                    let mut comp = true;
                    for (a, b) in prev.values.iter().zip(values.iter()) {
                        if let (Some(a), Some(b)) = (a, b) {
                            if a != b {
                                same = false;
                            }
                            if a + b != 1.0 {
                                comp = false;
                            }
                        }
                    }
                    same || comp
                }
        });
        if duplicate {
            continue;
        }
        out.push(BinaryResponse {
            col: i,
            name: table.names[i].clone(),
            on_label,
            values,
            base_rate,
        });
    }
    out
}

/// Whether partition `a` refines partition `b`: every group of
/// `a` sits inside a single group of `b`, over the rows where
/// both groupings are present (at least 30 such rows).
fn partition_refines(a: &[Option<i64>], b: &[Option<i64>]) -> bool {
    let mut map: HashMap<i64, i64> = HashMap::new();
    let mut both = 0usize;
    for (x, y) in a.iter().zip(b.iter()) {
        if let (Some(x), Some(y)) = (x, y) {
            both += 1;
            match map.get(x) {
                Some(&yy) if yy != *y => return false,
                _ => {
                    map.insert(*x, *y);
                }
            }
        }
    }
    both >= 30
}

/// Collapse duplicate and rollup encodings among the categorical
/// groupings (dimension columns and binned measures): a grouping
/// whose partition duplicates, refines, or is refined by an
/// earlier kept grouping is another encoding of structure already
/// in the account and is dropped. Time and cycle groupings are
/// not categorical encodings and are always kept.
fn collapse_groupings(groupings: Vec<Grouping>) -> Vec<Grouping> {
    let mut kept: Vec<Grouping> = Vec::new();
    for g in groupings {
        if g.source.is_some() {
            let redundant = kept.iter().filter(|k| k.source.is_some()).any(|k| {
                partition_refines(&g.keys, &k.keys) || partition_refines(&k.keys, &g.keys)
            });
            if redundant {
                continue;
            }
        }
        kept.push(g);
    }
    kept
}

/// Whether a candidate grouping restates a binary response under
/// another column's encoding: the grouping has exactly two groups,
/// and on the rows where both are present the group indicator is
/// the response's on/off pattern (or its complement) throughout.
/// Such a grouping is the outcome wearing a grouping's clothes —
/// grouping by it would state the response's own split as if it
/// were structure in the frame — so it is not used as a grouping.
/// A grouping on the response's own column is not covered here;
/// the scoring loop drops that pair because the grouping
/// determines the response exactly.
fn grouping_restates_response(g: &Grouping, binaries: &[BinaryResponse]) -> bool {
    let Some(source) = g.source else {
        return false;
    };
    if g.kind != "dimension" && g.kind != "binned" {
        return false;
    }
    binaries.iter().any(|b| {
        if b.col == source {
            return false;
        }
        let mut key_state: HashMap<i64, f64> = HashMap::new();
        let mut shared = 0usize;
        for (r, k) in g.keys.iter().enumerate() {
            let (Some(key), Some(v)) = (k, b.values[r]) else {
                continue;
            };
            shared += 1;
            match key_state.get(key) {
                Some(&prev) if prev != v => return false,
                _ => {
                    key_state.insert(*key, v);
                }
            }
        }
        shared >= 30
            && key_state.len() == 2
            && key_state.values().any(|&v| v == 1.0)
            && key_state.values().any(|&v| v == 0.0)
    })
}

pub fn dominant_structure(
    table: &TypedTable,
    frame: &FrameModel,
    facts: &[CardFacts],
    ticks: &Option<Vec<Option<i64>>>,
) -> Vec<FrameDominant> {
    let measures: Vec<usize> = frame
        .roles
        .iter()
        .enumerate()
        .filter(|(_, r)| r.as_deref() == Some("measure"))
        .map(|(i, _)| i)
        .filter(|&i| table.cols[i].is_numeric() && !weight_like_name(&table.names[i]))
        .take(24)
        .collect();
    let binaries = binary_responses(table, frame, facts);
    if (measures.is_empty() && binaries.is_empty()) || table.n < 30 {
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
                    source: None,
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
                    source: None,
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
                    source: None,
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
                    source: None,
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
            source: Some(i),
        });
    }
    // Continuous measures as candidate groupings, binned on their
    // decile edges (duplicated edges from tied values collapse, so
    // a partly discrete measure yields fewer bins). A binned
    // grouping never scores its own source column as a response.
    for &mi in &measures {
        if facts[mi].distinct < 20 {
            continue;
        }
        let mut vals: Vec<f64> = (0..table.n)
            .filter_map(|r| table.cols[mi].numeric_at(r))
            .filter(|x| x.is_finite())
            .collect();
        if vals.len() < 30 {
            continue;
        }
        vals.sort_by(|a, b| a.total_cmp(b));
        let mut edges: Vec<f64> = Vec::new();
        for k in 1..10 {
            let e = vals[(k * vals.len()) / 10];
            if edges.last().is_none_or(|&last| e > last) {
                edges.push(e);
            }
        }
        if edges.len() < 2 {
            continue;
        }
        let n_bins = edges.len() + 1;
        let mut peak_labels: BTreeMap<i64, String> = BTreeMap::new();
        for b in 0..n_bins {
            let label = if b == 0 {
                format!("up to {}", fmt_num(edges[0]))
            } else if b == n_bins - 1 {
                format!("{} and above", fmt_num(edges[edges.len() - 1]))
            } else {
                format!("{} to {}", fmt_num(edges[b - 1]), fmt_num(edges[b]))
            };
            peak_labels.insert(b as i64, label);
        }
        let keys: Vec<Option<i64>> = (0..table.n)
            .map(|r| {
                table.cols[mi]
                    .numeric_at(r)
                    .filter(|x| x.is_finite())
                    .map(|x| edges.partition_point(|&e| e < x) as i64)
            })
            .collect();
        let present_bins: BTreeSet<i64> = keys.iter().flatten().copied().collect();
        if present_bins.len() < 3 {
            continue;
        }
        let name = table.names[mi].clone();
        groupings.push(Grouping {
            kind: "binned",
            label: name.clone(),
            grouping: format!("{name} (quantile bins)"),
            keys,
            peak_labels: Some(peak_labels),
            peak_label_fn: None,
            source: Some(mi),
        });
    }
    let groupings: Vec<Grouping> = collapse_groupings(groupings)
        .into_iter()
        .filter(|g| !grouping_restates_response(g, &binaries))
        .collect();

    // The responses: the measures (a group's mean is the reading)
    // and the binary responses (a group's rate is the reading).
    // Both are scored by the same eta-squared, so a binary outcome
    // competes with measures on one scale.
    enum Response<'a> {
        Measure(usize),
        Binary(&'a BinaryResponse),
    }
    let mut responses: Vec<Response> = Vec::new();
    for &mi in &measures {
        responses.push(Response::Measure(mi));
    }
    for b in &binaries {
        responses.push(Response::Binary(b));
    }

    // Score every (grouping, response) pair by eta-squared: the
    // between-group sum of squares over the total.
    struct Scored {
        order: usize,
        score: f64,
        shares: Vec<FrameStructureShare>,
        peak: Option<FrameStructurePeak>,
        rates: Vec<FrameStructureRate>,
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
        let group_label = |key: i64| -> Option<String> {
            match (&g.peak_labels, g.peak_label_fn) {
                (Some(labels), _) => labels.get(&key).cloned(),
                (None, Some(f)) => Some(f(key)),
                (None, None) => None,
            }
        };
        let mut shares: Vec<FrameStructureShare> = Vec::new();
        let mut best: Option<(f64, usize, i64, f64, f64, f64)> = None;
        let mut rate_candidates: Vec<(f64, FrameStructureRate)> = Vec::new();
        for resp in &responses {
            let (rcol, rname, binary): (usize, String, Option<&BinaryResponse>) = match resp {
                Response::Measure(mi) => (*mi, table.names[*mi].clone(), None),
                Response::Binary(b) => (b.col, b.name.clone(), Some(b)),
            };
            if g.source == Some(rcol) {
                continue;
            }
            let value_at = |r: usize| -> Option<f64> {
                match binary {
                    Some(b) => b.values[r],
                    None => table.cols[rcol].numeric_at(r),
                }
            };
            let mut sums: Vec<(usize, f64)> = vec![(0, 0.0); n_groups];
            let mut group_lo: Vec<f64> = vec![f64::INFINITY; n_groups];
            let mut group_hi: Vec<f64> = vec![f64::NEG_INFINITY; n_groups];
            let (mut cnt, mut sum, mut sumsq) = (0usize, 0.0f64, 0.0f64);
            let (mut col_min, mut col_max) = (f64::INFINITY, f64::NEG_INFINITY);
            for (r, &gi) in row_group.iter().enumerate() {
                let (Some(x), Some(gi)) = (value_at(r), gi) else {
                    continue;
                };
                if !x.is_finite() {
                    continue;
                }
                sums[gi].0 += 1;
                sums[gi].1 += x;
                group_lo[gi] = group_lo[gi].min(x);
                group_hi[gi] = group_hi[gi].max(x);
                cnt += 1;
                sum += x;
                sumsq += x * x;
                col_min = col_min.min(x);
                col_max = col_max.max(x);
            }
            if cnt < 30 {
                continue;
            }
            let total_ss = sumsq - sum * sum / cnt as f64;
            if total_ss <= 0.0 {
                continue;
            }
            // A response the grouping determines exactly —
            // constant within every group — has a share of 1 by
            // construction: the grouping restates the response's
            // own definition (a derived flag grouped by the column
            // it is derived from, a group attribute grouped by the
            // group). That is not structure the account states, so
            // the pair is dropped.
            let determined = sums
                .iter()
                .enumerate()
                .all(|(gi, &(c, _))| c == 0 || group_lo[gi] == group_hi[gi]);
            if determined {
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
                    measure: rname.clone(),
                    share: round4(share),
                });
            }
            if let Some(b) = binary {
                if share < DOMINANT_MIN_SHARE {
                    continue;
                }
                // The rate reading: the highest group rate among
                // groups with enough support, against the lowest
                // such rate and the response's base rate.
                let mut peak: Option<(i64, f64, usize)> = None;
                let mut low: Option<f64> = None;
                for (key, &gi) in &group_ids {
                    let (c, s) = sums[gi];
                    if c < RATE_PEAK_MIN_SUPPORT {
                        continue;
                    }
                    let rate = s / c as f64;
                    low = Some(low.map_or(rate, |l: f64| l.min(rate)));
                    if peak.is_none_or(|(_, pr, _)| rate > pr) {
                        peak = Some((*key, rate, c));
                    }
                }
                if let (Some((key, rate, support)), Some(low)) = (peak, low) {
                    if rate - low >= RATE_MIN_CONTRAST {
                        if let Some(label) = group_label(key) {
                            rate_candidates.push((
                                share,
                                FrameStructureRate {
                                    response: b.name.clone(),
                                    on_label: b.on_label.clone(),
                                    base_rate: round4(b.base_rate),
                                    peak_group: label,
                                    peak_rate: round4(rate),
                                    peak_support: support,
                                    lift: round4(if b.base_rate > 0.0 {
                                        rate / b.base_rate
                                    } else {
                                        0.0
                                    }),
                                },
                            ));
                        }
                    }
                }
                continue;
            }
            if best.is_none_or(|(bs, _, _, _, _, _)| share > bs) {
                let mut peak: Option<(i64, f64)> = None;
                let (mut mean_min, mut mean_max) = (f64::INFINITY, f64::NEG_INFINITY);
                for (key, &gi) in &group_ids {
                    let (c, s) = sums[gi];
                    if c == 0 {
                        continue;
                    }
                    let mean = s / c as f64;
                    mean_min = mean_min.min(mean);
                    mean_max = mean_max.max(mean);
                    if peak.is_none_or(|(_, pm)| mean > pm) {
                        peak = Some((*key, mean));
                    }
                }
                if let Some((key, mean)) = peak {
                    best = Some((
                        share,
                        rcol,
                        key,
                        mean,
                        mean_max - mean_min,
                        col_max - col_min,
                    ));
                }
            }
        }
        if shares.is_empty() {
            continue;
        }
        shares.sort_by(|a, b| b.share.total_cmp(&a.share));
        shares.truncate(3);
        rate_candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        rate_candidates.truncate(2);
        let rates: Vec<FrameStructureRate> = rate_candidates.into_iter().map(|(_, r)| r).collect();
        let score = shares[0].share;
        // Materiality: a peak is stated only when the spread of
        // the group means reaches the fraction of the measure's
        // own range; a smaller spread is a level difference on a
        // wide scale, not a headline peak.
        let peak = best.and_then(|(_, mi, key, mean, spread, range)| {
            if range <= 0.0 || spread < PEAK_MATERIALITY_FRACTION * range {
                return None;
            }
            let label = group_label(key)?;
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
            rates,
        });
    }
    // Selection: at most two groupings, strongest first. Every
    // scored grouping already clears the dominance bar on its
    // strongest response (a tenth of that response's variance
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
                "binned" => format!("{} quantile bins", g.label),
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
            let rate_phrase = s
                .rates
                .iter()
                .map(|r| {
                    format!(
                        "; the {} rate ({}) is highest for {} at {}% against a base rate of {}% (lift {}x; {} rows in the group)",
                        r.response,
                        r.on_label,
                        r.peak_group,
                        fmt_pct(r.peak_rate),
                        fmt_pct(r.base_rate),
                        fmt_num(r.lift),
                        fmt_int(r.peak_support)
                    )
                })
                .collect::<Vec<_>>()
                .join("");
            let statement = if g.kind == "dimension" {
                format!(
                    "the {} dimension organises the frame: {} sits between {}{}{}",
                    g.label, share_text, between, peak_phrase, rate_phrase
                )
            } else if g.kind == "binned" {
                format!(
                    "the {} quantile bins organise the frame: {} sits between {}{}{}",
                    g.label, share_text, between, peak_phrase, rate_phrase
                )
            } else {
                format!(
                    "the {} organises the frame: {} sits between {}{}{}",
                    g.label, share_text, between, peak_phrase, rate_phrase
                )
            };
            FrameDominant {
                kind: g.kind.to_string(),
                label: g.label.clone(),
                grouping: g.grouping.clone(),
                shares: s.shares.clone(),
                peak: s.peak.clone(),
                rates: s.rates.clone(),
                statement,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------
// restatement demotion
// ---------------------------------------------------------------------

/// Demote restatements in place, ahead of the ledger: a finding is
/// a restatement when its whole subject is scaffolding
/// (`scaffolding_subject`). Columns the time axis determines are
/// found before the detectors run (`basis`) and are no pass's
/// subject, so their restatements are never produced. The demotion
/// is declared in the evidence metrics (`triviality`, `triviality_factor`
/// 0.6), the mechanism the ledger already applies and records;
/// nothing is removed. Key statements and frame-scope findings
/// are the account itself and are never demoted here, and a
/// finding a producer already demoted keeps its own declaration.
/// Returns the demotion counts by reason.
pub fn demote_restatements(
    findings: &mut [Finding],
    table: &TypedTable,
    frame: &FrameModel,
) -> BTreeMap<String, usize> {
    let scaffold: BTreeSet<usize> = frame
        .roles
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            matches!(
                r.as_deref(),
                Some("row_number" | "identifier" | "timestamp" | "timestamp_part")
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
    if scaffold.is_empty() {
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
        let reason = cols
            .iter()
            .all(|c| scaffold.contains(c))
            .then_some("scaffolding_subject");
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
                    card_role: card.card.card_role.clone(),
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
            n_afd_chance: 0,
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
    fn binary_response_is_stated_as_a_rate() {
        // A two-level outcome organised by a dimension: the
        // account states its rate, not only a variance share.
        let mut grp = Vec::new();
        let mut outcome = Vec::new();
        let mut measure = Vec::new();
        for g in 0..3 {
            for i in 0..100 {
                grp.push(["A", "B", "C"][g]);
                outcome.push(if (i % 10) < [8, 5, 2][g] { "yes" } else { "no" });
                measure.push(10.0 + g as f64 + (i % 7) as f64);
            }
        }
        let df = DataFrame::new_infer_height(vec![
            Series::new("grp".into(), grp).into(),
            Series::new("outcome".into(), outcome).into(),
            Series::new("measure".into(), measure).into(),
        ])
        .unwrap();
        let (table, facts) = facts_for(&df);
        let frame = analyze_frame(&table, &facts, None);
        let account = compute_account(&table, &frame, &facts, &empty_deps());
        let dom = account
            .dominant
            .iter()
            .find(|d| d.label == "grp")
            .expect("grp grouping stated");
        let rate = dom
            .rates
            .iter()
            .find(|r| r.response == "outcome")
            .expect("outcome rate recorded");
        assert_eq!(rate.on_label, "yes");
        assert_eq!(rate.peak_group, "A");
        assert!(rate.peak_rate > 0.75 && rate.peak_rate < 0.85, "{rate:?}");
        assert!(rate.lift > 1.4, "{rate:?}");
        assert_eq!(rate.peak_support, 100);
        assert!(
            dom.statement.contains("outcome rate (yes)") && dom.statement.contains("base rate"),
            "{}",
            dom.statement
        );
    }

    #[test]
    fn outcome_encoding_is_not_a_grouping() {
        // `alive` restates the `outcome` flag under a second
        // encoding: grouping by it would state the outcome's own
        // split as structure. The account groups by `grp` instead
        // and states the outcome's rate there.
        let mut grp = Vec::new();
        let mut outcome = Vec::new();
        let mut alive = Vec::new();
        let mut measure = Vec::new();
        for g in 0..3 {
            for i in 0..100 {
                let on = (i % 10) < [8, 5, 2][g];
                grp.push(["A", "B", "C"][g]);
                outcome.push(on);
                alive.push(if on { "yes" } else { "no" });
                measure.push(10.0 + g as f64 + (i % 7) as f64);
            }
        }
        let df = DataFrame::new_infer_height(vec![
            Series::new("grp".into(), grp).into(),
            Series::new("outcome".into(), outcome).into(),
            Series::new("alive".into(), alive).into(),
            Series::new("measure".into(), measure).into(),
        ])
        .unwrap();
        let (table, facts) = facts_for(&df);
        let frame = analyze_frame(&table, &facts, None);
        let account = compute_account(&table, &frame, &facts, &empty_deps());
        assert!(
            account.dominant.iter().all(|d| d.label != "alive"),
            "the outcome's own encoding must not headline: {:?}",
            account
                .dominant
                .iter()
                .map(|d| &d.statement)
                .collect::<Vec<_>>()
        );
        let dom = account
            .dominant
            .iter()
            .find(|d| d.label == "grp")
            .expect("grp grouping stated");
        assert!(
            dom.rates.iter().any(|r| r.response == "outcome"),
            "outcome rate stated under grp: {}",
            dom.statement
        );
    }

    #[test]
    fn determined_response_is_not_claimed() {
        // `adult` is derived from `who` exactly (man -> true), so
        // the share of `adult` between `who` values is 1 by
        // construction. With no other response in the frame, the
        // account states no dominant structure.
        let mut who = Vec::new();
        let mut adult = Vec::new();
        for g in 0..3 {
            for _ in 0..100 {
                who.push(["man", "woman", "child"][g]);
                adult.push(g == 0);
            }
        }
        let df = DataFrame::new_infer_height(vec![
            Series::new("who".into(), who).into(),
            Series::new("adult".into(), adult).into(),
        ])
        .unwrap();
        let (table, facts) = facts_for(&df);
        let frame = analyze_frame(&table, &facts, None);
        let account = compute_account(&table, &frame, &facts, &empty_deps());
        assert!(
            account.dominant.is_empty(),
            "a determined response is not structure: {:?}",
            account
                .dominant
                .iter()
                .map(|d| &d.statement)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn duplicate_encodings_occupy_one_account_slot() {
        // code and label are two encodings of one grouping; the
        // account states at most one of them.
        let mut code = Vec::new();
        let mut label = Vec::new();
        let mut fare = Vec::new();
        for i in 0..300 {
            let p = i % 3;
            code.push((p + 1) as i64);
            label.push(["First", "Second", "Third"][p]);
            fare.push(90.0 - 30.0 * p as f64 + (i % 5) as f64);
        }
        let df = DataFrame::new_infer_height(vec![
            Series::new("code".into(), code).into(),
            Series::new("label".into(), label).into(),
            Series::new("fare".into(), fare).into(),
        ])
        .unwrap();
        let (table, facts) = facts_for(&df);
        let frame = analyze_frame(&table, &facts, None);
        let account = compute_account(&table, &frame, &facts, &empty_deps());
        let labels: Vec<&str> = account.dominant.iter().map(|d| d.label.as_str()).collect();
        assert!(
            !(labels.contains(&"code") && labels.contains(&"label")),
            "dominant: {labels:?}"
        );
        assert!(!labels.is_empty(), "the grouping is still stated");
    }

    #[test]
    fn binned_measure_can_organise_a_response() {
        // A continuous driver binned into quantile bins organises
        // the response it drives.
        let mut driver = Vec::new();
        let mut price = Vec::new();
        let mut grp = Vec::new();
        for i in 0..600 {
            let d = 0.2 + ((i * 37) % 200) as f64 / 40.0;
            driver.push(d);
            price.push(100.0 + 900.0 * d * d + (i % 13) as f64);
            grp.push(["u", "v", "w"][i % 3]);
        }
        let df = DataFrame::new_infer_height(vec![
            Series::new("driver".into(), driver).into(),
            Series::new("price".into(), price).into(),
            Series::new("grp".into(), grp).into(),
        ])
        .unwrap();
        let (table, facts) = facts_for(&df);
        let frame = analyze_frame(&table, &facts, None);
        let account = compute_account(&table, &frame, &facts, &empty_deps());
        let dom = account
            .dominant
            .iter()
            .find(|d| d.kind == "binned" && d.label == "driver")
            .expect("binned driver grouping stated");
        assert!(
            dom.shares.iter().any(|sh| sh.measure == "price"),
            "shares: {:?}",
            dom.shares
        );
    }

    #[test]
    fn immaterial_peak_is_not_stated() {
        // The grouping organises the measure (group means differ
        // against a small within-group spread), but the mean
        // spread is a small fraction of the column's own range,
        // which rare extreme rows set: no headline peak.
        let mut grp = Vec::new();
        let mut val = Vec::new();
        for g in 0..3 {
            for i in 0..400 {
                grp.push(["A", "B", "C"][g]);
                let mut v = 55.0 + 2.0 * g as f64 + ((i * 7) % 20) as f64 / 10.0 - 1.0;
                if i % 199 == 0 {
                    v += if g % 2 == 0 { 26.0 } else { -26.0 };
                }
                val.push(v);
            }
        }
        let df = DataFrame::new_infer_height(vec![
            Series::new("grp".into(), grp).into(),
            Series::new("val".into(), val).into(),
        ])
        .unwrap();
        let (table, facts) = facts_for(&df);
        let frame = analyze_frame(&table, &facts, None);
        let account = compute_account(&table, &frame, &facts, &empty_deps());
        let dom = account
            .dominant
            .iter()
            .find(|d| d.label == "grp")
            .expect("grp grouping stated");
        assert!(dom.peak.is_none(), "peak: {:?}", dom.peak);
        assert!(
            !dom.statement.contains("averages highest"),
            "{}",
            dom.statement
        );
    }
}
