//! The pointed pass's outcome screen (issue #310): the designated
//! outcome's own account, and per-level outcome contrasts with
//! stated nulls.
//!
//! Two passes, both gated on a designated target. The account
//! states the outcome's own distribution — the base every later
//! statement compares against. The contrasts state, for each
//! categorical column, how the outcome runs at each of its levels
//! against the pooled outcome: with an event declared, each
//! level's event rate against the pooled event rate on the event
//! basis (the rows carrying the event or the reference level —
//! for a two-level outcome, every row); for a numeric outcome,
//! each level's median against the pooled median. A grouping
//! enters when its omnibus test clears the screen's
//! Benjamini–Hochberg bar and at least one level's own test
//! clears with a material difference; an entered grouping states
//! every eligible level, the levels that do not clear stated as
//! rates beside the pooled rate, without a difference claimed.
//!
//! A grouping measured and found without a difference is stated
//! as a null, at the same prominence as a difference — but only
//! in the pointed mode, when the caller declared metadata: a
//! null needs a single named quantity whose absence is measured
//! (the declared event's rate, or the numeric outcome's
//! location), and the handover is what names it. Without an
//! event, a categorical outcome's contrasts take the weaker
//! per-level form (each level of the outcome against its pooled
//! share) and absences are not stated; the coverage entry says
//! so.

use crate::dep::{bh_qvalues, chi2_sf, median_sorted, norm_sf, quantile_sorted, rank_sum_p};
use crate::fmt_float;
use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Rows a level needs before its rate or median is stated.
const MIN_LEVEL_ROWS: usize = 30;
/// Rows a grouping's comparison needs in total.
const MIN_GROUPING_ROWS: usize = 60;
/// Distinct levels a grouping column may carry (the outcome
/// screen's own cap: a panel's entity column is a grouping here,
/// and it runs past the level contrasts' 25).
const MAX_GROUP_LEVELS: usize = 64;
/// Levels stated per grouping; past this the largest effects are
/// stated and the coverage entry counts the rest.
const MAX_STATED_LEVELS: usize = 12;
/// Findings the contrasts pass states at most.
const MAX_FINDINGS: usize = 48;
/// The Benjamini–Hochberg bar for the screen's tests.
const Q_BAR: f64 = 0.05;
/// A level's difference must also be material: two percentage
/// points of rate, or half the pooled interquartile range of
/// location (the level contrasts' bar).
const MIN_RATE_DIFF: f64 = 0.02;
const MIN_IQR_SHIFT: f64 = 0.5;
/// A null requires the measurement to have been able to see a
/// difference: every level's rate interval half-width at most
/// five percentage points, or every level's median within a
/// quarter of the pooled interquartile range.
const MAX_NULL_HALF_WIDTH: f64 = 0.05;
const MAX_NULL_IQR_SHIFT: f64 = 0.25;

/// What the pipeline resolved from the caller's declarations for
/// the outcome screen.
#[derive(Clone, Debug, Default)]
pub struct OutcomeDecl {
    /// The declared event, as declared (any JSON scalar).
    pub event: Option<Value>,
    /// The event rendered as a level label of the target column,
    /// when it names one. `None` when no event was declared or
    /// the declared value is not a level of the target; the
    /// caller states the refusal in the declarations coverage.
    pub event_label: Option<String>,
    /// The target's declared unit, stated through the findings'
    /// subject `unit` field.
    pub unit: Option<String>,
    /// Per-level declared meanings of the grouping and target
    /// columns, keyed by column then level label.
    pub level_gloss: BTreeMap<String, BTreeMap<String, String>>,
    /// Whether the pointed handover is engaged (the caller
    /// declared metadata): nulls are stated only then.
    pub nulls: bool,
    /// Column indices never used as groupings (frame scaffolding,
    /// declared recorded-after columns, the axis): the outcome
    /// screen does not organise the outcome by them.
    pub excluded: Vec<usize>,
}

/// Render a declared event value as the level label the profiler
/// uses for it: strings as-is, numbers as the level coder renders
/// them, booleans as "true"/"false". Any other JSON shape cannot
/// name a level.
pub fn event_label_of(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(if *b { "true" } else { "false" }.to_string()),
        Value::Number(n) => n.as_f64().map(fmt_float),
        _ => None,
    }
}

/// Resolve a declared event against the target column: the
/// rendered label, when the target is a codable categorical
/// column and the rendered value names one of its levels. The
/// pipeline checks the declaration once through here and states
/// the result in the declarations coverage entry; the passes
/// consume the resolved label.
pub fn target_event_label(table: &TypedTable, target_name: &str, event: &Value) -> Option<String> {
    let tci = table.names.iter().position(|n| n == target_name)?;
    if table.cols[tci].is_numeric() {
        return None;
    }
    let label = event_label_of(event)?;
    let (_, labels) = crate::levels::code_levels(table, tci)?;
    if labels.contains(&label) {
        Some(label)
    } else {
        None
    }
}

/// Code a column's levels for the outcome screen: string, boolean,
/// and integer columns with 2..=MAX_GROUP_LEVELS distinct non-null
/// values, coded in rendered-label order. The cap is wider than
/// the level contrasts' (25) for string and boolean columns
/// because a panel's entity column is a grouping here; integer
/// columns keep the level contrasts' cap of 25 — an integer
/// column with forty distinct values is a measure that happens
/// to be discrete, not a grouping, and coding it as one lets a
/// shuffled table's noise pose as per-level outcome structure.
fn code_grouping(table: &TypedTable, ci: usize) -> Option<(Vec<Option<u32>>, Vec<String>)> {
    let mut index: BTreeMap<String, u32> = BTreeMap::new();
    let mut labels: Vec<String> = Vec::new();
    let (raw, integer) = match &table.cols[ci] {
        TypedCol::Str { vals } => (
            vals.iter().map(|v| v.map(|s| s.to_string())).collect(),
            false,
        ),
        TypedCol::StrOwned { vals } => (vals.clone(), false),
        TypedCol::Bool { vals } => (
            vals.iter()
                .map(|v| v.map(|b| if b { "true".into() } else { "false".into() }))
                .collect(),
            false,
        ),
        TypedCol::I64 { vals } => (
            vals.iter()
                .map(|v| v.map(|x| fmt_float(*x as f64)))
                .collect(),
            true,
        ),
        TypedCol::U64 { vals } => (
            vals.iter()
                .map(|v| v.map(|x| fmt_float(*x as f64)))
                .collect(),
            true,
        ),
        _ => return None,
    };
    for v in raw.iter().flatten() {
        if !index.contains_key(v) {
            if labels.len() >= MAX_GROUP_LEVELS {
                return None;
            }
            index.insert(v.clone(), labels.len() as u32);
            labels.push(v.clone());
        }
    }
    if !(2..=MAX_GROUP_LEVELS).contains(&labels.len()) {
        return None;
    }
    if integer && labels.len() > 25 {
        return None;
    }
    let codes = raw.iter().map(|v| v.as_ref().map(|s| index[s])).collect();
    Some((codes, labels))
}

fn pct(x: f64) -> String {
    format!("{:.1}%", 100.0 * x)
}

/// The support behind a level's statement: a rate difference
/// on 60 rows and the same difference on 6,000 rows are not
/// the same fact, however significant each is. Full credit at
/// 1,000 rows, square-root below it.
fn support(rows: usize) -> f64 {
    ((rows as f64) / 1000.0).sqrt().min(1.0)
}

/// Rates and shares in evidence metrics, rounded to six decimals
/// so the payload stays readable; counts and locations are exact.
fn r6(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}

fn glossed(decl: &OutcomeDecl, column: &str, label: &str) -> String {
    match decl.level_gloss.get(column).and_then(|m| m.get(label)) {
        Some(g) => format!("'{label}' ({g})"),
        None => format!("'{label}'"),
    }
}

// ---------------------------------------------------------------------
// The outcome account.
// ---------------------------------------------------------------------

/// State the designated outcome's own distribution. Returns the
/// finding (at most one) and the coverage detail; every early
/// exit is a stated reason in the detail.
pub fn outcome_account(
    table: &TypedTable,
    target_name: &str,
    decl: &OutcomeDecl,
) -> (Vec<Finding>, String) {
    let Some(tci) = table.names.iter().position(|n| n == target_name) else {
        return (
            Vec::new(),
            format!("designated target '{target_name}' is not a column of the frame"),
        );
    };
    let col = &table.cols[tci];
    if col.is_numeric() {
        let mut values: Vec<f64> = (0..col.len())
            .filter_map(|r| col.numeric_at(r))
            .filter(|v| v.is_finite())
            .collect();
        let n = values.len();
        let missing = col.len() - n;
        if n == 0 {
            return (
                Vec::new(),
                format!(
                    "target '{target_name}' designated (numeric); it carries no finite values, so no account can be stated"
                ),
            );
        }
        values.sort_by(|a, b| a.total_cmp(b));
        let mean = values.iter().sum::<f64>() / n as f64;
        let median = median_sorted(&values);
        let q1 = quantile_sorted(&values, 0.25);
        let q3 = quantile_sorted(&values, 0.75);
        let (min, max) = (values[0], values[n - 1]);
        let unit_clause = decl
            .unit
            .as_ref()
            .map(|u| format!(" (in {u})"))
            .unwrap_or_default();
        let claim = format!(
            "The designated outcome '{target_name}' is numeric{unit_clause}: median {}, mean {}, middle half {} to {}, range {} to {}, over {} values{}.",
            fmt_float(median),
            fmt_float(mean),
            fmt_float(q1),
            fmt_float(q3),
            fmt_float(min),
            fmt_float(max),
            n,
            if missing > 0 {
                format!(" ({missing} missing)")
            } else {
                String::new()
            },
        );
        let metrics: BTreeMap<String, Value> = BTreeMap::from([
            ("column".into(), json!(target_name)),
            ("form".into(), json!("center_spread_range")),
            ("n".into(), json!(n)),
            ("n_missing".into(), json!(missing)),
            ("mean".into(), json!(mean)),
            ("median".into(), json!(median)),
            ("q1".into(), json!(q1)),
            ("q3".into(), json!(q3)),
            ("min".into(), json!(min)),
            ("max".into(), json!(max)),
        ]);
        let mut f = Finding::new(
            Pattern::OutcomeDistribution,
            BTreeMap::from([
                ("columns".into(), json!([target_name])),
                ("column".into(), json!(target_name)),
                ("target".into(), json!(target_name)),
            ]),
            claim,
            EvidenceBasis::Measured,
            0.9,
            metrics,
            None,
            Method::new(
                "outcome_account",
                "outcome_distribution",
                "the designated outcome's own distribution (onceover v0.1)",
            ),
        );
        if let Some(u) = &decl.unit {
            f.subject.unit = Some(u.clone());
        }
        return (
            vec![f],
            format!(
                "target '{target_name}' designated (numeric); its distribution stated over {n} values"
            ),
        );
    }
    // Categorical outcome: the share of each level.
    let Some((codes, labels)) = crate::levels::code_levels(table, tci) else {
        let distinct: std::collections::BTreeSet<String> = (0..col.len())
            .filter(|&r| !col.is_null_at(r))
            .map(|r| col.render_cell(r))
            .collect();
        return (
            Vec::new(),
            format!(
                "target '{target_name}' designated (categorical); it carries {} distinct values, past the level cap, so its distribution is stated by its column card rather than as an account",
                distinct.len()
            ),
        );
    };
    let mut counts = vec![0usize; labels.len()];
    let mut n = 0usize;
    for c in codes.iter().flatten() {
        counts[*c as usize] += 1;
        n += 1;
    }
    let missing = table.n - n;
    if n == 0 {
        return (
            Vec::new(),
            format!(
                "target '{target_name}' designated (categorical); it carries no values, so no account can be stated"
            ),
        );
    }
    // Levels stated in share order, largest first; the metrics
    // carry every level.
    let mut order: Vec<usize> = (0..labels.len()).collect();
    order.sort_by(|&a, &b| counts[b].cmp(&counts[a]).then(labels[a].cmp(&labels[b])));
    let stated = &order[..order.len().min(8)];
    let mut parts: Vec<String> = stated
        .iter()
        .map(|&li| {
            format!(
                "{} {} ({} rows)",
                glossed(decl, target_name, &labels[li]),
                pct(counts[li] as f64 / n as f64),
                counts[li]
            )
        })
        .collect();
    if order.len() > stated.len() {
        parts.push(format!("and {} more levels", order.len() - stated.len()));
    }
    let mut claim = format!(
        "The designated outcome '{target_name}' takes {} levels over {} rows{}: {}.",
        labels.len(),
        n,
        if missing > 0 {
            format!(" ({missing} missing)")
        } else {
            String::new()
        },
        parts.join(", ")
    );
    let mut metrics: BTreeMap<String, Value> = BTreeMap::from([
        ("column".into(), json!(target_name)),
        ("form".into(), json!("level_shares")),
        ("n".into(), json!(n)),
        ("n_missing".into(), json!(missing)),
        (
            "levels".into(),
            json!(labels
                .iter()
                .enumerate()
                .map(|(li, label)| json!({
                    "level": label,
                    "rows": counts[li],
                    "share": r6(counts[li] as f64 / n as f64),
                }))
                .collect::<Vec<_>>()),
        ),
    ]);
    // The declared event's rate, over the whole file and on the
    // event basis: the base the contrasts compare against.
    let mut event_detail = String::new();
    if let Some(event_label) = &decl.event_label {
        if let Some(ei) = labels.iter().position(|l| l == event_label) {
            let event_rows = counts[ei];
            // The reference level: the modal other level.
            let reference = (0..labels.len())
                .filter(|&li| li != ei)
                .max_by(|&a, &b| counts[a].cmp(&counts[b]).then(labels[b].cmp(&labels[a])));
            if let Some(ri) = reference {
                let basis_rows = event_rows + counts[ri];
                let basis_rate = event_rows as f64 / basis_rows as f64;
                claim.push_str(&format!(
                    " The declared event is {}: {} of all rows, and {} of the {} rows carrying {} or {}.",
                    glossed(decl, target_name, event_label),
                    pct(event_rows as f64 / n as f64),
                    pct(basis_rate),
                    basis_rows,
                    glossed(decl, target_name, event_label),
                    glossed(decl, target_name, &labels[ri]),
                ));
                metrics.insert("event".into(), json!(event_label));
                metrics.insert("event_rows".into(), json!(event_rows));
                metrics.insert(
                    "event_share".into(),
                    json!(r6(event_rows as f64 / n as f64)),
                );
                metrics.insert("reference_level".into(), json!(labels[ri]));
                metrics.insert("basis_rows".into(), json!(basis_rows));
                metrics.insert("basis_event_rate".into(), json!(r6(basis_rate)));
                event_detail = format!(
                    "; the declared event '{event_label}' stated over the whole file and on its basis"
                );
            }
        }
    }
    let mut f = Finding::new(
        Pattern::OutcomeDistribution,
        BTreeMap::from([
            ("columns".into(), json!([target_name])),
            ("column".into(), json!(target_name)),
            ("target".into(), json!(target_name)),
        ]),
        claim,
        EvidenceBasis::Measured,
        0.9,
        metrics,
        None,
        Method::new(
            "outcome_account",
            "outcome_distribution",
            "the designated outcome's own distribution (onceover v0.1)",
        ),
    );
    if let Some(u) = &decl.unit {
        f.subject.unit = Some(u.clone());
    }
    (
        vec![f],
        format!(
            "target '{target_name}' designated (categorical); its level distribution stated over {n} rows{event_detail}"
        ),
    )
}

// ---------------------------------------------------------------------
// Per-level outcome contrasts.
// ---------------------------------------------------------------------

/// One grouping column prepared against the target: its name and
/// level labels (its codes travel alongside in the pass inputs).
struct Grouping {
    /// How the grouping reads in claims: the column name, or
    /// "hour of day of {axis}" for a calendar part of the axis.
    name: String,
    /// The column named in the finding's subject: the grouping
    /// column itself, or the axis column for a calendar part.
    subject: String,
    labels: Vec<String>,
}

fn two_proportion_p(x1: usize, n1: usize, x2: usize, n2: usize) -> f64 {
    if n1 == 0 || n2 == 0 {
        return 1.0;
    }
    let p = (x1 + x2) as f64 / (n1 + n2) as f64;
    if p <= 0.0 || p >= 1.0 {
        return 1.0;
    }
    let se = (p * (1.0 - p) * (1.0 / n1 as f64 + 1.0 / n2 as f64)).sqrt();
    if se == 0.0 {
        return 1.0;
    }
    let z = (x1 as f64 / n1 as f64 - x2 as f64 / n2 as f64) / se;
    (2.0 * norm_sf(z.abs())).clamp(0.0, 1.0)
}

/// Kruskal-Wallis omnibus for the numeric path: do the
/// grouping's levels differ at all, on ranks? The grouping-level
/// gate the event path gets from its chi-square omnibus; without
/// it, entry rests on single levels, and on a coarse discrete
/// outcome a few chance levels clear the shift bar on their own.
fn kruskal_wallis_p(groups: &[&[f64]]) -> f64 {
    let k = groups.len();
    if k < 2 {
        return 1.0;
    }
    let mut pooled: Vec<(f64, usize)> = Vec::new();
    for (gi, vs) in groups.iter().enumerate() {
        pooled.extend(vs.iter().map(|&v| (v, gi)));
    }
    let n = pooled.len() as f64;
    if n < 3.0 {
        return 1.0;
    }
    pooled.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut rank_sums = vec![0.0f64; k];
    let mut tie_term = 0.0f64;
    let mut i = 0usize;
    while i < pooled.len() {
        let mut j = i + 1;
        while j < pooled.len() && pooled[j].0 == pooled[i].0 {
            j += 1;
        }
        let avg_rank = (i + 1 + j) as f64 / 2.0;
        for item in &pooled[i..j] {
            rank_sums[item.1] += avg_rank;
        }
        let t = (j - i) as f64;
        if t > 1.0 {
            tie_term += t * t * t - t;
        }
        i = j;
    }
    let mut h = 0.0f64;
    for (gi, vs) in groups.iter().enumerate() {
        if !vs.is_empty() {
            h += rank_sums[gi] * rank_sums[gi] / vs.len() as f64;
        }
    }
    h = 12.0 / (n * (n + 1.0)) * h - 3.0 * (n + 1.0);
    let corr = 1.0 - tie_term / (n * n * n - n);
    if corr <= 0.0 || h <= 0.0 {
        return 1.0;
    }
    crate::dep::chi2_sf(h / corr, (k - 1) as f64)
}

fn contrast_finding(
    decl: &OutcomeDecl,
    grouping: &Grouping,
    target_name: &str,
    form: &str,
    claim: String,
    score: f64,
    mut metrics: BTreeMap<String, Value>,
) -> Finding {
    metrics.insert("form".into(), json!(form));
    metrics.insert("grouping_column".into(), json!(grouping.name));
    metrics.insert("target".into(), json!(target_name));
    let mut f = Finding::new(
        Pattern::OutcomeContrast,
        BTreeMap::from([
            ("columns".into(), json!([grouping.subject, target_name])),
            ("inputs".into(), json!([grouping.subject])),
            ("target".into(), json!(target_name)),
        ]),
        claim,
        EvidenceBasis::Measured,
        score,
        metrics,
        None,
        Method::new(
            "outcome_contrasts",
            "per_level_outcome_contrast",
            "per-level outcome contrasts with stated nulls (onceover v0.1)",
        ),
    );
    if let Some(u) = &decl.unit {
        f.subject.unit = Some(u.clone());
    }
    f
}

/// State the per-level outcome contrasts for a designated target.
/// Returns the findings and the coverage detail; every early exit
/// is a stated reason in the detail.
pub fn outcome_contrasts(
    table: &TypedTable,
    target_name: &str,
    decl: &OutcomeDecl,
    axis: Option<(&str, &[Option<i64>])>,
) -> (Vec<Finding>, String) {
    let Some(tci) = table.names.iter().position(|n| n == target_name) else {
        return (
            Vec::new(),
            format!("designated target '{target_name}' is not a column of the frame"),
        );
    };
    let target_col = &table.cols[tci];
    // Prepare the groupings once: every codable column that is
    // neither the target nor an excluded column.
    let mut groupings: Vec<(Vec<Option<u32>>, Grouping)> = Vec::new();
    for ci in 0..table.cols.len() {
        if ci == tci || decl.excluded.contains(&ci) {
            continue;
        }
        if let Some((codes, labels)) = code_grouping(table, ci) {
            groupings.push((
                codes,
                Grouping {
                    name: table.names[ci].clone(),
                    subject: table.names[ci].clone(),
                    labels,
                },
            ));
        }
    }
    // Calendar parts of the frame's date axis enter as groupings
    // too, as they do for the level contrasts: the outcome by
    // hour of day is the same kind of statement as the outcome
    // by a categorical column. A calendar level whose rows span
    // fewer than two distinct cycles is not a level of the
    // grouping (the level contrasts' gate), so a single unusual
    // week cannot pose as a weekday effect.
    if let Some((axis_name, axis_ticks)) = axis {
        // A calendar part finer than the axis's own resolution
        // carries no information: the weekday of a monthly
        // timestamp is an artifact of the calendar, not a
        // property of the rows. The modal gap between distinct
        // ticks is the axis's resolution; day of week requires
        // a sub-monthly one. (Hour of day already requires
        // intra-day ticks, in the derivation itself.)
        let modal_gap: Option<i64> = {
            let mut distinct: Vec<i64> = axis_ticks.iter().flatten().copied().collect();
            distinct.sort_unstable();
            distinct.dedup();
            let mut gaps: Vec<i64> = distinct
                .windows(2)
                .map(|w| w[1] - w[0])
                .filter(|g| *g > 0)
                .collect();
            gaps.sort_unstable();
            gaps.get(gaps.len() / 2).copied()
        };
        let input = crate::levels::LevelContrastInput {
            axis_name: Some(axis_name),
            axis_ticks: Some(axis_ticks),
            ..Default::default()
        };
        for cg in crate::levels::calendar_groupings(&input) {
            if cg.calendar_part == Some("day_of_week")
                && modal_gap.is_none_or(|g| g >= 28 * 86_400_000_000)
            {
                continue;
            }
            let keep: Vec<usize> = (0..cg.labels.len())
                .filter(|&li| cg.cycles[li] >= 2)
                .collect();
            if keep.len() < 2 {
                continue;
            }
            let remap: std::collections::BTreeMap<u32, u32> = keep
                .iter()
                .enumerate()
                .map(|(i, &li)| (li as u32, i as u32))
                .collect();
            let codes: Vec<Option<u32>> = cg
                .codes
                .iter()
                .map(|c| c.and_then(|x| remap.get(&x).copied()))
                .collect();
            groupings.push((
                codes,
                Grouping {
                    name: cg.describe.clone(),
                    subject: cg.column.clone(),
                    labels: keep.iter().map(|&li| cg.labels[li].clone()).collect(),
                },
            ));
        }
    }
    if target_col.is_numeric() {
        numeric_contrasts(table, target_name, decl, &groupings)
    } else {
        categorical_contrasts(table, target_name, tci, decl, &groupings)
    }
}

/// The event-basis machinery shared by the account and the
/// contrasts: the event's code, the reference level's code (the
/// modal other level), and the basis predicate, resolved against
/// the target's coded levels.
struct EventBasis {
    event_label: String,
    reference_label: String,
    event_code: u32,
    reference_code: u32,
}

fn event_basis(
    target_codes: &[Option<u32>],
    target_labels: &[String],
    event_label: &str,
) -> Option<EventBasis> {
    let ei = target_labels.iter().position(|l| l == event_label)?;
    let mut counts = vec![0usize; target_labels.len()];
    for c in target_codes.iter().flatten() {
        counts[*c as usize] += 1;
    }
    let ri = (0..target_labels.len())
        .filter(|&li| li != ei)
        .max_by(|&a, &b| {
            counts[a]
                .cmp(&counts[b])
                .then(target_labels[b].cmp(&target_labels[a]))
        })?;
    Some(EventBasis {
        event_label: target_labels[ei].clone(),
        reference_label: target_labels[ri].clone(),
        event_code: ei as u32,
        reference_code: ri as u32,
    })
}

fn categorical_contrasts(
    table: &TypedTable,
    target_name: &str,
    tci: usize,
    decl: &OutcomeDecl,
    groupings: &[(Vec<Option<u32>>, Grouping)],
) -> (Vec<Finding>, String) {
    let Some((tcodes, tlabels)) = crate::levels::code_levels(table, tci) else {
        return (
            Vec::new(),
            format!(
                "target '{target_name}' designated (categorical); it carries more levels than the contrast screen codes, so no contrasts were measured"
            ),
        );
    };
    match &decl.event_label {
        Some(event_label) => {
            let Some(basis) = event_basis(&tcodes, &tlabels, event_label) else {
                return (
                    Vec::new(),
                    format!(
                        "target '{target_name}' designated (categorical); the declared event '{event_label}' is not a level of the target, so no contrasts were measured"
                    ),
                );
            };
            event_contrasts(table, target_name, decl, groupings, &tcodes, &basis)
        }
        None => weaker_contrasts(table, target_name, decl, groupings, &tcodes, &tlabels),
    }
}

/// Contrasts against the declared event: per-level event rates
/// against the pooled event rate, on the event basis.
#[allow(clippy::too_many_arguments)]
fn event_contrasts(
    table: &TypedTable,
    target_name: &str,
    decl: &OutcomeDecl,
    groupings: &[(Vec<Option<u32>>, Grouping)],
    tcodes: &[Option<u32>],
    basis: &EventBasis,
) -> (Vec<Finding>, String) {
    let in_basis: Vec<bool> = tcodes
        .iter()
        .map(|c| *c == Some(basis.event_code) || *c == Some(basis.reference_code))
        .collect();
    let is_event: Vec<bool> = tcodes
        .iter()
        .map(|c| *c == Some(basis.event_code))
        .collect();
    let basis_rows: usize = in_basis.iter().filter(|b| **b).count();
    let basis_events: usize = is_event.iter().filter(|b| **b).count();
    if basis_rows == 0 {
        return (
            Vec::new(),
            format!(
                "target '{target_name}' designated (categorical, event '{}'); no rows carry the event or the reference level '{}', so no contrasts were measured",
                basis.event_label, basis.reference_label
            ),
        );
    }
    let pooled = basis_events as f64 / basis_rows as f64;

    // Per grouping: per eligible level, basis rows and events.
    struct G {
        gi: usize,
        eligible: Vec<usize>,
        n: Vec<usize>,
        e: Vec<usize>,
        omnibus_p: f64,
    }
    let mut gs: Vec<G> = Vec::new();
    for (gi, (codes, g)) in groupings.iter().enumerate() {
        let mut n = vec![0usize; g.labels.len()];
        let mut e = vec![0usize; g.labels.len()];
        for r in 0..table.n {
            if !in_basis[r] {
                continue;
            }
            if let Some(c) = codes[r] {
                n[c as usize] += 1;
                if is_event[r] {
                    e[c as usize] += 1;
                }
            }
        }
        let eligible: Vec<usize> = (0..g.labels.len())
            .filter(|&li| n[li] >= MIN_LEVEL_ROWS)
            .collect();
        let total: usize = eligible.iter().map(|&li| n[li]).sum();
        if eligible.len() < 2 || total < MIN_GROUPING_ROWS {
            continue;
        }
        // Omnibus chi-square over the eligible levels.
        let tot_e: usize = eligible.iter().map(|&li| e[li]).sum();
        let rate = tot_e as f64 / total as f64;
        let mut chi2 = 0.0f64;
        for &li in &eligible {
            for (obs, p) in [(e[li], rate), (n[li] - e[li], 1.0 - rate)] {
                let exp = n[li] as f64 * p;
                if exp > 0.0 {
                    chi2 += (obs as f64 - exp).powi(2) / exp;
                }
            }
        }
        let omnibus_p = chi2_sf(chi2, (eligible.len() - 1) as f64);
        gs.push(G {
            gi,
            eligible,
            n,
            e,
            omnibus_p,
        });
    }
    let q_omni = bh_qvalues(&gs.iter().map(|g| Some(g.omnibus_p)).collect::<Vec<_>>());
    // Level tests, one BH family over every eligible level.
    let mut level_ps: Vec<(usize, usize, f64)> = Vec::new(); // (gs idx, level, p)
    for (si, g) in gs.iter().enumerate() {
        let tot_n: usize = g.eligible.iter().map(|&li| g.n[li]).sum();
        let tot_e: usize = g.eligible.iter().map(|&li| g.e[li]).sum();
        for &li in &g.eligible {
            let p = two_proportion_p(g.e[li], g.n[li], tot_e - g.e[li], tot_n - g.n[li]);
            level_ps.push((si, li, p));
        }
    }
    let q_level = bh_qvalues(&level_ps.iter().map(|t| Some(t.2)).collect::<Vec<_>>());
    let q_of: BTreeMap<(usize, usize), f64> = level_ps
        .iter()
        .zip(q_level.iter())
        .map(|(t, q)| ((t.0, t.1), q.unwrap_or(1.0)))
        .collect();

    let mut findings: Vec<Finding> = Vec::new();
    let (mut entered, mut nulls, mut measured_only) = (0usize, 0usize, 0usize);
    for (si, g) in gs.iter().enumerate() {
        let (_, grp) = &groupings[g.gi];
        let clears: Vec<usize> = g
            .eligible
            .iter()
            .copied()
            .filter(|&li| {
                let rate = g.e[li] as f64 / g.n[li] as f64;
                q_of[&(si, li)] <= Q_BAR && (rate - pooled).abs() >= MIN_RATE_DIFF
            })
            .collect();
        let entered_g = q_omni[si].unwrap_or(1.0) <= Q_BAR && !clears.is_empty();
        if entered_g {
            entered += 1;
            // State every eligible level, largest rate difference
            // first, up to the per-grouping cap.
            let mut levels = g.eligible.clone();
            levels.sort_by(|&a, &b| {
                let da = (g.e[a] as f64 / g.n[a] as f64 - pooled).abs();
                let db = (g.e[b] as f64 / g.n[b] as f64 - pooled).abs();
                db.total_cmp(&da).then(grp.labels[a].cmp(&grp.labels[b]))
            });
            for &li in levels.iter().take(MAX_STATED_LEVELS) {
                let rate = g.e[li] as f64 / g.n[li] as f64;
                let diff = rate - pooled;
                let q = q_of[&(si, li)];
                let difference = clears.contains(&li);
                let claim = if difference {
                    format!(
                        "{} = {}: the {} rate is {} of {} rows carrying {} or {}, against the pooled {}.",
                        grp.name,
                        glossed(decl, &grp.name, &grp.labels[li]),
                        glossed(decl, target_name, &basis.event_label),
                        pct(rate),
                        g.n[li],
                        glossed(decl, target_name, &basis.event_label),
                        glossed(decl, target_name, &basis.reference_label),
                        pct(pooled),
                    )
                } else {
                    format!(
                        "{} = {}: the {} rate is {} of {} rows carrying {} or {}, against the pooled {} — no measured difference from the pooled rate.",
                        grp.name,
                        glossed(decl, &grp.name, &grp.labels[li]),
                        glossed(decl, target_name, &basis.event_label),
                        pct(rate),
                        g.n[li],
                        glossed(decl, target_name, &basis.event_label),
                        glossed(decl, target_name, &basis.reference_label),
                        pct(pooled),
                    )
                };
                let score = if difference {
                    (0.45 + 0.5 * (diff.abs() / 0.25).min(1.0)) * support(g.n[li])
                } else {
                    0.4
                };
                findings.push(contrast_finding(
                    decl,
                    grp,
                    target_name,
                    "event_rate",
                    claim,
                    score,
                    BTreeMap::from([
                        ("level".into(), json!(grp.labels[li])),
                        ("level_rows".into(), json!(g.n[li])),
                        ("level_events".into(), json!(g.e[li])),
                        ("level_rate".into(), json!(r6(rate))),
                        ("pooled_rate".into(), json!(r6(pooled))),
                        ("rate_difference".into(), json!(r6(diff))),
                        ("q_value".into(), json!(q)),
                        ("difference".into(), json!(difference)),
                        ("event".into(), json!(basis.event_label)),
                        ("reference_level".into(), json!(basis.reference_label)),
                        ("basis_rows".into(), json!(basis_rows)),
                    ]),
                ));
            }
            continue;
        }
        // The null: measured, and precise enough to have seen a
        // difference. Pointed mode only.
        let precise = g.omnibus_p >= 0.10
            && g.eligible.iter().all(|&li| {
                let rate = g.e[li] as f64 / g.n[li] as f64;
                let half = 1.96 * (rate * (1.0 - rate) / g.n[li] as f64).sqrt();
                (rate - half) <= pooled && pooled <= (rate + half) && half <= MAX_NULL_HALF_WIDTH
            });
        if decl.nulls && precise {
            nulls += 1;
            let parts: Vec<String> = g
                .eligible
                .iter()
                .map(|&li| {
                    format!(
                        "{} {} of {}",
                        glossed(decl, &grp.name, &grp.labels[li]),
                        pct(g.e[li] as f64 / g.n[li] as f64),
                        g.n[li]
                    )
                })
                .collect();
            let claim = format!(
                "No difference in the {} rate across '{}' was measured: {}, against the pooled {} (rows carrying {} or {}).",
                glossed(decl, target_name, &basis.event_label),
                grp.name,
                parts.join(", "),
                pct(pooled),
                glossed(decl, target_name, &basis.event_label),
                glossed(decl, target_name, &basis.reference_label),
            );
            findings.push(contrast_finding(
                decl,
                grp,
                target_name,
                "event_rate_null",
                claim,
                0.6,
                BTreeMap::from([
                    ("pooled_rate".into(), json!(r6(pooled))),
                    ("omnibus_p_value".into(), json!(g.omnibus_p)),
                    ("event".into(), json!(basis.event_label)),
                    ("reference_level".into(), json!(basis.reference_label)),
                    ("basis_rows".into(), json!(basis_rows)),
                    (
                        "levels".into(),
                        json!(g
                            .eligible
                            .iter()
                            .map(|&li| json!({
                                "level": grp.labels[li],
                                "rows": g.n[li],
                                "events": g.e[li],
                                "rate": r6(g.e[li] as f64 / g.n[li] as f64),
                            }))
                            .collect::<Vec<_>>()),
                    ),
                ]),
            ));
        } else {
            measured_only += 1;
        }
    }
    finish_contrasts(
        findings,
        format!(
            "target '{target_name}' designated (categorical, event '{}'); basis: the {basis_rows} rows carrying '{}' or '{}', pooled event rate {}; {} groupings measured, {entered} entered, {nulls} nulls stated, {measured_only} measured without a statement{}",
            basis.event_label,
            basis.event_label,
            basis.reference_label,
            pct(pooled),
            gs.len(),
            if decl.nulls {
                ""
            } else {
                "; nulls are not stated: no metadata was declared with the target"
            },
        ),
        decl,
    )
}

/// The weaker form, without a declared event: each level of the
/// outcome against its pooled share, per grouping level, stating
/// only measured differences. Absences are not stated: without a
/// declared event there is no single rate whose absence is the
/// statement.
fn weaker_contrasts(
    table: &TypedTable,
    target_name: &str,
    decl: &OutcomeDecl,
    groupings: &[(Vec<Option<u32>>, Grouping)],
    tcodes: &[Option<u32>],
    tlabels: &[String],
) -> (Vec<Finding>, String) {
    // Tests: (grouping idx, level, target level, p, share, pooled).
    struct T {
        gi: usize,
        li: usize,
        ti: usize,
        p: f64,
        share: f64,
        pooled: f64,
        n: usize,
    }
    // Pass 1: per grouping, the counts and the omnibus
    // chi-square over its eligible levels x the outcome's
    // levels — the grouping-level gate, as in the event path.
    // Without it, entry rests on single cells, and across the
    // hundreds of cells a wide table offers, chance cells clear
    // the share bar on their own.
    struct Prep {
        gi: usize,
        counts: Vec<Vec<usize>>,
        level_n: Vec<usize>,
        col_total: Vec<usize>,
        eligible: Vec<usize>,
        elig_total: usize,
        omnibus_p: f64,
    }
    let mut prepared: Vec<Prep> = Vec::new();
    for (gi, (codes, g)) in groupings.iter().enumerate() {
        // counts[level][target level]
        let mut counts = vec![vec![0usize; tlabels.len()]; g.labels.len()];
        let mut level_n = vec![0usize; g.labels.len()];
        for r in 0..table.n {
            if let (Some(gc), Some(tc)) = (codes[r], tcodes[r]) {
                counts[gc as usize][tc as usize] += 1;
                level_n[gc as usize] += 1;
            }
        }
        let eligible: Vec<usize> = (0..g.labels.len())
            .filter(|&li| level_n[li] >= MIN_LEVEL_ROWS)
            .collect();
        let elig_total: usize = eligible.iter().map(|&li| level_n[li]).sum();
        if eligible.len() < 2 || elig_total < MIN_GROUPING_ROWS {
            continue;
        }
        let mut col_total = vec![0usize; tlabels.len()];
        for &li in &eligible {
            for ti in 0..tlabels.len() {
                col_total[ti] += counts[li][ti];
            }
        }
        let mut chi2 = 0.0f64;
        for &li in &eligible {
            for ti in 0..tlabels.len() {
                let expected = level_n[li] as f64 * col_total[ti] as f64 / elig_total as f64;
                if expected > 0.0 {
                    let d = counts[li][ti] as f64 - expected;
                    chi2 += d * d / expected;
                }
            }
        }
        let df = ((eligible.len() - 1) * (tlabels.len() - 1)) as f64;
        let omnibus_p = if df > 0.0 { chi2_sf(chi2, df) } else { 1.0 };
        prepared.push(Prep {
            gi,
            counts,
            level_n,
            col_total,
            eligible,
            elig_total,
            omnibus_p,
        });
    }
    let omnibus_qs = bh_qvalues(
        &prepared
            .iter()
            .map(|p| Some(p.omnibus_p))
            .collect::<Vec<_>>(),
    );
    let mut tests: Vec<T> = Vec::new();
    let mut measured = 0usize;
    for (prep, oq) in prepared.iter().zip(omnibus_qs.iter()) {
        let (gi, counts, level_n, eligible, elig_total) = (
            prep.gi,
            &prep.counts,
            &prep.level_n,
            &prep.eligible,
            prep.elig_total,
        );
        measured += 1;
        if oq.unwrap_or(1.0) > Q_BAR {
            continue;
        }
        for &li in eligible {
            for (ti, col) in prep.col_total.iter().enumerate() {
                let rest_n = elig_total - level_n[li];
                let rest_x: usize = eligible
                    .iter()
                    .filter(|&&o| o != li)
                    .map(|&o| counts[o][ti])
                    .sum();
                let p = two_proportion_p(counts[li][ti], level_n[li], rest_x, rest_n);
                let pooled_share = *col as f64 / elig_total as f64;
                tests.push(T {
                    gi,
                    li,
                    ti,
                    p,
                    share: counts[li][ti] as f64 / level_n[li] as f64,
                    pooled: pooled_share,
                    n: level_n[li],
                });
            }
        }
    }
    let qs = bh_qvalues(&tests.iter().map(|t| Some(t.p)).collect::<Vec<_>>());
    let mut findings: Vec<Finding> = Vec::new();
    for (t, q) in tests.iter().zip(qs.iter()) {
        let q = q.unwrap_or(1.0);
        let diff = t.share - t.pooled;
        if q > Q_BAR || diff.abs() < MIN_RATE_DIFF {
            continue;
        }
        let (_, g) = &groupings[t.gi];
        let claim = format!(
            "{} = {}: {} is {} of {} rows, against {} of all rows.",
            g.name,
            glossed(decl, &g.name, &g.labels[t.li]),
            glossed(decl, target_name, &tlabels[t.ti]),
            pct(t.share),
            t.n,
            pct(t.pooled),
        );
        findings.push(contrast_finding(
            decl,
            g,
            target_name,
            "level_rate",
            claim,
            (0.45 + 0.5 * (diff.abs() / 0.25).min(1.0)) * support(t.n),
            BTreeMap::from([
                ("level".into(), json!(g.labels[t.li])),
                ("target_level".into(), json!(tlabels[t.ti])),
                ("level_rows".into(), json!(t.n)),
                ("level_share".into(), json!(r6(t.share))),
                ("pooled_share".into(), json!(r6(t.pooled))),
                ("share_difference".into(), json!(r6(diff))),
                ("q_value".into(), json!(q)),
            ]),
        ));
    }
    let null_note =
        "; absences are not stated: without a declared event there is no single rate whose absence is the statement";
    finish_contrasts(
        findings,
        format!(
            "target '{target_name}' designated (categorical, no event declared); {measured} groupings measured, contrasts stated per level of the outcome against its pooled share{null_note}"
        ),
        decl,
    )
}

/// Contrasts for a numeric outcome: per-level medians against the
/// pooled median, with the level contrasts' shift bar.
fn numeric_contrasts(
    table: &TypedTable,
    target_name: &str,
    decl: &OutcomeDecl,
    groupings: &[(Vec<Option<u32>>, Grouping)],
) -> (Vec<Finding>, String) {
    let tcol = &table.cols[table.names.iter().position(|n| n == target_name).unwrap()];
    struct G {
        gi: usize,
        eligible: Vec<usize>,
        values: Vec<Vec<f64>>,
        medians: Vec<f64>,
    }
    let mut gs: Vec<G> = Vec::new();
    // The pooled statistics are the target's own, over every
    // measured row — not a grouping's eligible subset.
    let mut all_values: Vec<f64> = (0..table.n)
        .filter_map(|r| tcol.numeric_at(r))
        .filter(|v| v.is_finite())
        .collect();
    for (gi, (codes, g)) in groupings.iter().enumerate() {
        let mut values: Vec<Vec<f64>> = vec![Vec::new(); g.labels.len()];
        for (r, code) in codes.iter().enumerate() {
            if let (Some(c), Some(v)) = (*code, tcol.numeric_at(r)) {
                if v.is_finite() {
                    values[c as usize].push(v);
                }
            }
        }
        let eligible: Vec<usize> = (0..g.labels.len())
            .filter(|&li| values[li].len() >= MIN_LEVEL_ROWS)
            .collect();
        let total: usize = eligible.iter().map(|&li| values[li].len()).sum();
        if eligible.len() < 2 || total < MIN_GROUPING_ROWS {
            continue;
        }
        let medians: Vec<f64> = values
            .iter()
            .map(|vs| {
                if vs.is_empty() {
                    f64::NAN
                } else {
                    let mut s = vs.clone();
                    s.sort_by(|a, b| a.total_cmp(b));
                    median_sorted(&s)
                }
            })
            .collect();
        gs.push(G {
            gi,
            eligible,
            values,
            medians,
        });
    }
    if gs.is_empty() {
        return (
            Vec::new(),
            format!(
                "target '{target_name}' designated (numeric); no grouping column carried two levels with {MIN_LEVEL_ROWS} measured rows each, so no contrasts were measured"
            ),
        );
    }
    all_values.sort_by(|a, b| a.total_cmp(b));
    let pooled_median = median_sorted(&all_values);
    let pooled_iqr = {
        let q1 = quantile_sorted(&all_values, 0.25);
        let q3 = quantile_sorted(&all_values, 0.75);
        q3 - q1
    };
    // Level tests: rank-sum of the level against the rest, one BH
    // family over every eligible level.
    let mut ps: Vec<(usize, usize, f64)> = Vec::new();
    for (si, g) in gs.iter().enumerate() {
        for &li in &g.eligible {
            let rest: Vec<f64> = g
                .eligible
                .iter()
                .filter(|&&o| o != li)
                .flat_map(|&o| g.values[o].iter().copied())
                .collect();
            ps.push((si, li, rank_sum_p(&g.values[li], &rest)));
        }
    }
    let qs = bh_qvalues(&ps.iter().map(|t| Some(t.2)).collect::<Vec<_>>());
    let q_of: BTreeMap<(usize, usize), f64> = ps
        .iter()
        .zip(qs.iter())
        .map(|(t, q)| ((t.0, t.1), q.unwrap_or(1.0)))
        .collect();
    // The grouping-level gate: a Kruskal-Wallis omnibus per
    // grouping, corrected across groupings as its own family.
    // Entry requires the omnibus and a level to clear.
    let omnibus_ps: Vec<Option<f64>> = gs
        .iter()
        .map(|g| {
            let levels: Vec<&[f64]> = g
                .eligible
                .iter()
                .map(|&li| g.values[li].as_slice())
                .collect();
            Some(kruskal_wallis_p(&levels))
        })
        .collect();
    let omnibus_qs = bh_qvalues(&omnibus_ps);
    let omnibus_q: Vec<f64> = omnibus_qs.iter().map(|q| q.unwrap_or(1.0)).collect();

    let mut findings: Vec<Finding> = Vec::new();
    let (mut entered, mut nulls, mut measured_only) = (0usize, 0usize, 0usize);
    for (si, g) in gs.iter().enumerate() {
        let (_, grp) = &groupings[g.gi];
        let shift = |li: usize| -> f64 {
            // The level's median against the rest's median, in
            // pooled IQRs.
            let rest: Vec<f64> = g
                .eligible
                .iter()
                .filter(|&&o| o != li)
                .flat_map(|&o| g.values[o].iter().copied())
                .collect::<Vec<f64>>();
            let mut rest_sorted = rest;
            rest_sorted.sort_by(|a, b| a.total_cmp(b));
            let rest_median = median_sorted(&rest_sorted);
            if pooled_iqr > 0.0 {
                (g.medians[li] - rest_median) / pooled_iqr
            } else if g.medians[li] != rest_median {
                f64::INFINITY
            } else {
                0.0
            }
        };
        let clears: Vec<usize> = g
            .eligible
            .iter()
            .copied()
            .filter(|&li| q_of[&(si, li)] <= Q_BAR && shift(li).abs() >= MIN_IQR_SHIFT)
            .collect();
        if !clears.is_empty() && omnibus_q[si] <= Q_BAR {
            entered += 1;
            let mut levels = g.eligible.clone();
            levels.sort_by(|&a, &b| {
                let da = (g.medians[a] - pooled_median).abs();
                let db = (g.medians[b] - pooled_median).abs();
                db.total_cmp(&da).then(grp.labels[a].cmp(&grp.labels[b]))
            });
            for &li in levels.iter().take(MAX_STATED_LEVELS) {
                let difference = clears.contains(&li);
                let claim = if difference {
                    format!(
                        "{} = {}: the median of '{}' is {}, against the pooled median {} ({} values).",
                        grp.name,
                        glossed(decl, &grp.name, &grp.labels[li]),
                        target_name,
                        fmt_float(g.medians[li]),
                        fmt_float(pooled_median),
                        g.values[li].len(),
                    )
                } else {
                    format!(
                        "{} = {}: the median of '{}' is {}, against the pooled median {} ({} values) — no measured difference from the pooled median.",
                        grp.name,
                        glossed(decl, &grp.name, &grp.labels[li]),
                        target_name,
                        fmt_float(g.medians[li]),
                        fmt_float(pooled_median),
                        g.values[li].len(),
                    )
                };
                let score = if difference {
                    (0.45 + 0.5 * (shift(li).abs() / 2.0).min(1.0)) * support(g.values[li].len())
                } else {
                    0.4
                };
                findings.push(contrast_finding(
                    decl,
                    grp,
                    target_name,
                    "location",
                    claim,
                    score,
                    BTreeMap::from([
                        ("level".into(), json!(grp.labels[li])),
                        ("level_rows".into(), json!(g.values[li].len())),
                        ("level_median".into(), json!(g.medians[li])),
                        ("pooled_median".into(), json!(pooled_median)),
                        (
                            "shift_in_pooled_iqrs".into(),
                            json!(if shift(li).is_infinite() {
                                Value::Null
                            } else {
                                json!(r6(shift(li)))
                            }),
                        ),
                        ("q_value".into(), json!(q_of[&(si, li)])),
                        ("difference".into(), json!(difference)),
                    ]),
                ));
            }
            continue;
        }
        // The null: every level's median within a quarter of the
        // pooled IQR of the pooled median. Pointed mode only.
        let absent = pooled_iqr > 0.0
            && g.eligible.iter().all(|&li| {
                (g.medians[li] - pooled_median).abs() <= MAX_NULL_IQR_SHIFT * pooled_iqr
            });
        if decl.nulls && absent {
            nulls += 1;
            let parts: Vec<String> = g
                .eligible
                .iter()
                .take(8)
                .map(|&li| {
                    format!(
                        "{} {} ({} values)",
                        glossed(decl, &grp.name, &grp.labels[li]),
                        fmt_float(g.medians[li]),
                        g.values[li].len()
                    )
                })
                .collect();
            let more = if g.eligible.len() > 8 {
                format!(
                    "; and {} more levels within the same band",
                    g.eligible.len() - 8
                )
            } else {
                String::new()
            };
            let claim = format!(
                "No difference in the median of '{}' across '{}' was measured: {}, against the pooled median {}{}.",
                target_name,
                grp.name,
                parts.join(", "),
                fmt_float(pooled_median),
                more,
            );
            findings.push(contrast_finding(
                decl,
                grp,
                target_name,
                "location_null",
                claim,
                0.6,
                BTreeMap::from([
                    ("pooled_median".into(), json!(pooled_median)),
                    ("pooled_iqr".into(), json!(pooled_iqr)),
                    (
                        "levels".into(),
                        json!(g
                            .eligible
                            .iter()
                            .map(|&li| json!({
                                "level": grp.labels[li],
                                "rows": g.values[li].len(),
                                "median": g.medians[li],
                            }))
                            .collect::<Vec<_>>()),
                    ),
                ]),
            ));
        } else {
            measured_only += 1;
        }
    }
    finish_contrasts(
        findings,
        format!(
            "target '{target_name}' designated (numeric); pooled median {} over {} values; {} groupings measured, {entered} entered, {nulls} nulls stated, {measured_only} measured without a statement{}",
            fmt_float(pooled_median),
            all_values.len(),
            gs.len(),
            if decl.nulls {
                ""
            } else {
                "; nulls are not stated: no metadata was declared with the target"
            },
        ),
        decl,
    )
}

/// Apply the screen's finding cap deterministically (score, then
/// claim) and finish the coverage detail with the stated count.
fn finish_contrasts(
    mut findings: Vec<Finding>,
    detail: String,
    _decl: &OutcomeDecl,
) -> (Vec<Finding>, String) {
    findings.sort_by(|a, b| {
        b.interestingness
            .total_cmp(&a.interestingness)
            .then(a.claim.cmp(&b.claim))
    });
    let stated = findings.len().min(MAX_FINDINGS);
    let capped = findings.len() - stated;
    findings.truncate(MAX_FINDINGS);
    (
        findings,
        format!(
            "{detail}; {stated} statements stated{}",
            if capped > 0 {
                format!(" ({capped} further statements past the screen's cap of {MAX_FINDINGS})")
            } else {
                String::new()
            }
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::extract;
    use polars::prelude::*;

    #[test]
    fn calendar_parts_of_the_axis_enter_as_groupings() {
        // 40 days of hourly rows; the outcome sits high at hour
        // 03 and low elsewhere. No categorical column exists, so
        // only the axis's calendar parts can state the contrast.
        let mut y: Vec<f64> = Vec::new();
        let mut ticks: Vec<Option<i64>> = Vec::new();
        for day in 0..40i64 {
            for hour in 0..24i64 {
                y.push(if hour == 3 {
                    50.0
                } else {
                    10.0 + (hour % 3) as f64
                });
                ticks.push(Some((day * 24 + hour) * 3_600_000_000));
            }
        }
        let frame = df! { "y" => y }.unwrap();
        let table = extract(&frame);
        let decl = OutcomeDecl::default();
        let (findings, cov) = outcome_contrasts(&table, "y", &decl, Some(("ts", &ticks)));
        assert!(
            findings
                .iter()
                .any(|f| f.claim.contains("hour of day of ts") && f.claim.contains("'03'")),
            "{cov}: {:?}",
            findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        // The finding's subject names the axis column, not the
        // calendar description.
        let f = findings
            .iter()
            .find(|f| f.claim.contains("hour of day of ts"))
            .unwrap();
        assert_eq!(f.subject.columns, vec!["ts".to_string(), "y".to_string()]);
    }

    fn decl_event(event: &str) -> OutcomeDecl {
        OutcomeDecl {
            event: Some(json!(event)),
            event_label: Some(event.to_string()),
            nulls: true,
            ..Default::default()
        }
    }

    /// 2,000 rows: a binary outcome whose event rate is 60% in
    /// group "a" and 20% in group "b"; a second grouping whose
    /// levels share the pooled rate.
    fn chicago_like() -> DataFrame {
        let mut grp: Vec<&str> = Vec::new();
        let mut flat: Vec<&str> = Vec::new();
        let mut out: Vec<&str> = Vec::new();
        for i in 0..1000 {
            grp.push("a");
            flat.push(if i % 2 == 0 { "x" } else { "y" });
            out.push(if i % 10 < 6 { "Fail" } else { "Pass" });
        }
        for i in 0..1000 {
            grp.push("b");
            flat.push(if i % 2 == 0 { "x" } else { "y" });
            out.push(if i % 10 < 2 { "Fail" } else { "Pass" });
        }
        df! { "grp" => grp, "flat" => flat, "outcome" => out }.unwrap()
    }

    #[test]
    fn numeric_account_states_center_spread_range_and_unit() {
        let df = df! { "y" => &[1.0f64, 2.0, 3.0, 4.0, 100.0], "g" => &["a", "a", "b", "b", "b"] }
            .unwrap();
        let table = extract(&df);
        let decl = OutcomeDecl {
            unit: Some("percent".into()),
            ..Default::default()
        };
        let (findings, cov) = outcome_account(&table, "y", &decl);
        assert_eq!(findings.len(), 1, "{cov}");
        let f = &findings[0];
        f.validate().unwrap();
        assert_eq!(f.finding_type.as_deref(), Some("outcome_distribution"));
        assert_eq!(f.subject.unit.as_deref(), Some("percent"));
        assert!(f.claim.contains("median 3"), "{}", f.claim);
        assert!(f.claim.contains("range 1 to 100"), "{}", f.claim);
        let m = f.metrics();
        assert_eq!(m["median"], json!(3.0));
        assert_eq!(m["mean"], json!(22.0));
    }

    #[test]
    fn categorical_account_states_shares_and_the_event_rate() {
        let df = chicago_like();
        let table = extract(&df);
        let (findings, cov) = outcome_account(&table, "outcome", &decl_event("Fail"));
        assert_eq!(findings.len(), 1, "{cov}");
        let f = &findings[0];
        f.validate().unwrap();
        // Pooled: 800 events of 2,000 rows; the basis is the
        // whole file (two levels), so both rates are 40%.
        assert!(f.claim.contains("'Fail' 40.0% (800 rows)"), "{}", f.claim);
        assert!(
            f.claim
                .contains("The declared event is 'Fail': 40.0% of all rows"),
            "{}",
            f.claim
        );
        let m = f.metrics();
        assert_eq!(m["basis_event_rate"], json!(0.4));
        assert_eq!(m["reference_level"], json!("Pass"));
    }

    #[test]
    fn event_contrasts_state_differences_and_nulls() {
        let df = chicago_like();
        let table = extract(&df);
        let (findings, cov) = outcome_contrasts(&table, "outcome", &decl_event("Fail"), None);
        for f in &findings {
            f.validate().unwrap();
        }
        let diffs: Vec<_> = findings
            .iter()
            .filter(|f| f.specification.form.as_deref() == Some("event_rate"))
            .collect();
        assert_eq!(diffs.len(), 2, "{cov}");
        assert!(
            diffs
                .iter()
                .any(|f| f.claim.contains("grp = 'a'") && f.claim.contains("60.0%")),
            "{cov}"
        );
        let nulls: Vec<_> = findings
            .iter()
            .filter(|f| f.specification.form.as_deref() == Some("event_rate_null"))
            .collect();
        assert_eq!(nulls.len(), 1, "{cov}");
        assert!(
            nulls[0].claim.contains("across 'flat'"),
            "{}",
            nulls[0].claim
        );
        assert!(nulls[0].claim.contains("40.0%"), "{}", nulls[0].claim);
    }

    #[test]
    fn nulls_wait_for_the_handover() {
        let df = chicago_like();
        let table = extract(&df);
        let mut decl = decl_event("Fail");
        decl.nulls = false;
        let (findings, _) = outcome_contrasts(&table, "outcome", &decl, None);
        assert!(
            findings
                .iter()
                .all(|f| f.specification.form.as_deref() != Some("event_rate_null")),
            "no null may be stated without the handover"
        );
        // The differences are still stated.
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn weaker_form_states_level_shares_without_nulls() {
        let df = chicago_like();
        let table = extract(&df);
        let decl = OutcomeDecl {
            nulls: true,
            ..Default::default()
        };
        let (findings, cov) = outcome_contrasts(&table, "outcome", &decl, None);
        for f in &findings {
            f.validate().unwrap();
        }
        assert!(
            findings
                .iter()
                .all(|f| f.specification.form.as_deref() == Some("level_rate")),
            "{cov}"
        );
        assert!(
            findings
                .iter()
                .any(|f| f.claim.contains("grp = 'a'") && f.claim.contains("60.0%")),
            "{cov}"
        );
        assert!(cov.contains("absences are not stated"), "{cov}");
    }

    #[test]
    fn numeric_contrasts_state_locations_and_nulls() {
        // Group "hi" sits 10 above group "lo"; the flat grouping
        // splits each group evenly, so its medians agree.
        let mut grp: Vec<&str> = Vec::new();
        let mut flat: Vec<&str> = Vec::new();
        let mut y: Vec<f64> = Vec::new();
        for i in 0..400 {
            let hi = i >= 200;
            grp.push(if hi { "hi" } else { "lo" });
            flat.push(if i % 2 == 0 { "x" } else { "y" });
            y.push((if hi { 20.0 } else { 10.0 }) + (i % 10) as f64 * 0.1);
        }
        let df = df! { "grp" => grp, "flat" => flat, "y" => y }.unwrap();
        let table = extract(&df);
        let decl = OutcomeDecl {
            nulls: true,
            ..Default::default()
        };
        let (findings, cov) = outcome_contrasts(&table, "y", &decl, None);
        for f in &findings {
            f.validate().unwrap();
        }
        let locs: Vec<_> = findings
            .iter()
            .filter(|f| f.specification.form.as_deref() == Some("location"))
            .collect();
        assert_eq!(locs.len(), 2, "{cov}");
        let nulls: Vec<_> = findings
            .iter()
            .filter(|f| f.specification.form.as_deref() == Some("location_null"))
            .collect();
        assert_eq!(nulls.len(), 1, "{cov}");
        assert!(
            nulls[0].claim.contains("across 'flat'"),
            "{}",
            nulls[0].claim
        );
    }

    #[test]
    fn event_resolution_names_only_real_levels() {
        let df = chicago_like();
        let table = extract(&df);
        assert_eq!(
            target_event_label(&table, "outcome", &json!("Fail")),
            Some("Fail".to_string())
        );
        assert_eq!(target_event_label(&table, "outcome", &json!("Nope")), None);
        assert_eq!(
            target_event_label(&table, "outcome", &json!(["Fail"])),
            None
        );
        assert_eq!(event_label_of(&json!(1)), Some("1".to_string()));
        assert_eq!(event_label_of(&json!(true)), Some("true".to_string()));
    }

    #[test]
    fn excluded_groupings_are_not_measured() {
        let df = chicago_like();
        let table = extract(&df);
        let mut decl = decl_event("Fail");
        // 'grp' is column 0: excluded, so only the flat null can
        // be stated.
        decl.excluded = vec![0];
        let (findings, _) = outcome_contrasts(&table, "outcome", &decl, None);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].claim.contains("across 'flat'"));
    }
}
