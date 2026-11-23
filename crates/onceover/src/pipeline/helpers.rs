//! Pure helpers shared across pipeline stages.

use std::collections::{BTreeMap, BTreeSet};

use onceover_core::{CardOut, TypedTable, VKey};
use onceover_deps::{ColInfo, DepsOutcome};
use onceover_report::{Category, ColumnCard, Finding, Overview};
use polars::prelude::DataFrame;

use crate::config::ProfileConfig;
use crate::frame::{CardFacts, FrameModel};

/// Entity columns of the frame's entity × time grain, in priority
/// order: the grain's single non-axis factor when the account found
/// one, then identifier-role columns, then dimension-role columns.
/// Flag columns carry their own role and never enter this list, and
/// neither does a timestamp part (an hour beside a date subdivides
/// the period; it is not an entity followed through time). An
/// entity axis also needs at least 10 distinct values — the panel
/// checks' own floor for the entities they follow — and at least two
/// non-null rows per entity on average: a column with one row per value (an
/// order number) has no history to follow, and its "entity total" is
/// one row's value. At most three columns are returned.
pub(super) fn entity_columns(
    table: &TypedTable,
    frame: &FrameModel,
    card_facts: &[CardFacts],
    axis_name: &str,
    declared: Option<usize>,
) -> Vec<usize> {
    let mut entities: Vec<usize> = Vec::new();
    // A declared entity (issue #310) is the caller's grain and
    // takes precedence over every inferred candidate.
    if let Some(idx) = declared {
        entities.push(idx);
    }
    if let Some(grain) = &frame.info.grain {
        let non_axis: Vec<(&str, usize)> = grain
            .factorization
            .iter()
            .map(|f| (f.column.as_str(), f.distinct))
            .filter(|(c, _)| *c != axis_name)
            .collect();
        if non_axis.len() == 1 && non_axis[0].1 >= 10 {
            if let Some(i) = table.names.iter().position(|n| n == non_axis[0].0) {
                if !entities.contains(&i)
                    && 2 * card_facts[i].distinct <= table.n - card_facts[i].null_count
                    && !matches!(
                        frame.roles[i].as_deref(),
                        Some("timestamp_part" | "row_number")
                    )
                {
                    entities.push(i);
                }
            }
        }
    }
    for role in ["identifier", "dimension"] {
        for (i, r) in frame.roles.iter().enumerate() {
            if r.as_deref() == Some(role)
                && card_facts[i].distinct >= 10
                && 2 * card_facts[i].distinct <= table.n - card_facts[i].null_count
                && !entities.contains(&i)
                && table.names[i] != axis_name
            {
                entities.push(i);
            }
        }
    }
    entities.truncate(3);
    entities
}

pub(super) fn quick_deps(info: &[ColInfo], n: usize) -> DepsOutcome {
    DepsOutcome {
        single_unique: (0..info.len())
            .filter(|&i| info[i].null_count == 0 && info[i].distinct == n && n > 0)
            .collect(),
        coverage: "quick tier: single-column uniqueness only".into(),
        ..Default::default()
    }
}

pub(super) fn keys_pk(findings: &[Finding]) -> Option<String> {
    findings
        .iter()
        .find(|f| {
            f.category == Category::Uniqueness
                && f.subject.columns.len() == 1
                && f.claim.contains("primary key candidate")
                && f.evidence
                    .iter()
                    .any(|e| e.method.algorithm == "uniqueness")
        })
        .and_then(|f| f.subject.columns.first().cloned())
}

/// Materialize the pivoted wide form of a verified long-format
/// frame: one row per index tuple (first-appearance order), the
/// index columns with their original dtypes (a take of their
/// first-occurrence rows), and one column per attribute level holding
/// the measure with its original dtype. None when a level name collides with an
/// index column or the measure is not numeric.
pub(super) fn build_wide_frame(
    df: &DataFrame,
    table: &TypedTable,
    spec: &onceover_deps::PivotSpec,
) -> Option<DataFrame> {
    use polars::prelude::*;
    use std::collections::HashMap;

    if spec.levels.iter().any(|l| spec.index.contains(l)) {
        return None;
    }
    let attr_idx = table.names.iter().position(|n| n == &spec.attribute)?;
    let measure_idx = table.names.iter().position(|n| n == &spec.measure)?;
    let index_idx: Option<Vec<usize>> = spec
        .index
        .iter()
        .map(|n| table.names.iter().position(|x| x == n))
        .collect();
    let index_idx = index_idx?;
    if !table.cols[measure_idx].is_numeric() {
        return None;
    }
    let measure = df.columns()[measure_idx].as_materialized_series();
    let level_pos: HashMap<&str, usize> = spec
        .levels
        .iter()
        .enumerate()
        .map(|(i, l)| (l.as_str(), i))
        .collect();
    let mut row_of: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut first_rows: Vec<u32> = Vec::new();
    let mut cells: Vec<Vec<Option<u32>>> = Vec::new();
    for r in 0..table.n {
        let mut key = Vec::new();
        for &i in &index_idx {
            onceover_deps::hashutil::encode_value(&table.cols[i], r, &mut key);
        }
        let w = *row_of.entry(key).or_insert_with(|| {
            first_rows.push(r as u32);
            cells.push(vec![None; spec.levels.len()]);
            cells.len() - 1
        });
        let level = table.cols[attr_idx].render_cell(r);
        if let Some(&lp) = level_pos.get(level.as_str()) {
            cells[w][lp] = Some(r as u32);
        }
    }
    let take_idx = IdxCa::from_vec("".into(), first_rows);
    let mut columns: Vec<Column> = Vec::new();
    for name in &spec.index {
        let s = df
            .columns()
            .iter()
            .find(|c| c.name() == name.as_str())?
            .as_materialized_series()
            .take(&take_idx)
            .ok()?;
        columns.push(s.into_column());
    }
    for (li, level) in spec.levels.iter().enumerate() {
        let indices = IdxCa::from_iter_options("".into(), cells.iter().map(|row| row[li]));
        let mut col = measure.take(&indices).ok()?;
        col.rename(level.into());
        columns.push(col.into_column());
    }
    DataFrame::new_infer_height(columns).ok()
}

/// Duplicate value examples for dirty-key findings (dtype-aware display).
pub(super) fn dup_examples(table: &TypedTable, i: usize, c: &CardOut) -> Vec<String> {
    c.counts
        .top
        .iter()
        .filter(|(_, count)| *count >= 2)
        .take(3)
        .map(|(k, _)| display_key(k, &table.cols[i]))
        .collect()
}

fn display_key(k: &VKey, col: &onceover_core::TypedCol) -> String {
    match (k, col) {
        (VKey::I(v), onceover_core::TypedCol::Date { .. }) => onceover_core::dateutil::iso_date(*v),
        (VKey::I(v), onceover_core::TypedCol::Datetime { unit, .. }) => {
            onceover_core::dateutil::iso_timestamp(*v, *unit)
        }
        _ => k.display(),
    }
}

pub(super) fn build_overview(
    table: &TypedTable,
    columns: &BTreeMap<String, ColumnCard>,
    findings: &[Finding],
    cfg: &ProfileConfig,
    labels: &BTreeMap<String, String>,
) -> Overview {
    let mut dtype_counts: BTreeMap<String, usize> = BTreeMap::new();
    for d in &table.dtypes {
        *dtype_counts.entry(d.clone()).or_insert(0) += 1;
    }
    let mut role_counts: BTreeMap<String, usize> = BTreeMap::new();
    for c in columns.values() {
        *role_counts.entry(c.card_role.clone()).or_insert(0) += 1;
    }

    let n_functional = findings
        .iter()
        .filter(|f| f.is(Category::Dependencies, Some("functional")))
        .count();
    let n_predictive = findings
        .iter()
        .filter(|f| f.is(Category::Dependencies, Some("predictive")))
        .count();
    let n_rules = findings
        .iter()
        .filter(|f| f.category == Category::ConditionalRules)
        .count();
    let pk = keys_pk(findings)
        .map(|k| format!("; key candidate: {}", labels.get(&k).unwrap_or(&k)))
        .unwrap_or_default();
    let headline = format!(
        "{} rows × {} columns ({} budget){}; {} functional dependencies, {} predictive dependencies, {} conditional rules",
        table.n,
        table.cols.len(),
        cfg.budget.as_str(),
        pk,
        n_functional,
        n_predictive,
        n_rules
    );

    Overview {
        n_rows: table.n,
        n_columns: table.cols.len(),
        headline,
        dtype_counts,
        role_counts,
        top_findings: findings.iter().take(5).map(|f| f.claim.clone()).collect(),
    }
}

/// Give the derived columns the `timestamp_part` role, with their
/// evidence, in the frame model's role vectors (the fields, so the
/// rest of the frame model can stay borrowed).
pub(super) fn assign_timestamp_parts(
    roles: &mut [Option<String>],
    info_roles: &mut [onceover_report::FrameColumnRole],
    derived: &[crate::basis::Derived],
) {
    for d in derived {
        roles[d.col] = Some("timestamp_part".to_string());
        if let Some(r) = info_roles.get_mut(d.col) {
            r.role = Some("timestamp_part".to_string());
            r.evidence = d.evidence.clone();
        }
    }
}

/// Copy the frame model's roles onto the column cards.
pub(super) fn sync_card_roles(
    columns: &mut BTreeMap<String, ColumnCard>,
    table: &TypedTable,
    frame: &FrameModel,
) {
    for (i, name) in table.names.iter().enumerate() {
        if let Some(card) = columns.get_mut(name) {
            card.semantic_role = frame.roles[i].clone();
            card.semantic_role_evidence = frame.roles[i]
                .is_some()
                .then(|| frame.info.roles[i].evidence.clone());
        }
    }
}

/// The coverage suffix naming the columns excluded as targets, with
/// the reason for each: its frame role, or the column it recodes.
pub(super) fn scaffold_note(
    table: &TypedTable,
    frame: &FrameModel,
    info: &[ColInfo],
    excluded: &[usize],
    axis_excluded: &[usize],
    basis: &crate::basis::Basis,
) -> String {
    if excluded.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = excluded
        .iter()
        .map(|&i| {
            let why = match basis
                .recodings
                .iter()
                .find(|(d, _)| *d == i)
                .map(|(_, keep)| format!("recoding of {}", table.names[*keep]))
                .or_else(|| {
                    basis
                        .indicators
                        .iter()
                        .find(|(d, _)| *d == i)
                        .map(|(_, of)| format!("missingness indicator of {}", table.names[*of]))
                }) {
                Some(why) => why,
                None => frame.roles[i]
                    .as_deref()
                    .unwrap_or(info[i].card_role.as_str())
                    .to_string(),
            };
            format!("{} ({why})", table.names[i])
        })
        .collect();
    let mut note = format!("; frame-role target exclusions: {}", parts.join(", "));
    if !axis_excluded.is_empty() {
        let names: Vec<String> = axis_excluded
            .iter()
            .map(|&i| table.names[i].clone())
            .collect();
        note.push_str(&format!(
            "; of these, {} excluded as the report's time axis (structural, \
             regardless of assigned role; a hints.roles entry for the column \
             overrides it)",
            names.join(", ")
        ));
    }
    note
}

/// Merge column groups that share a column: two identities that
/// share a column tie all their columns together (a part's affine
/// copy is a part of the same total).
pub(super) fn merge_groups(groups: &mut Vec<Vec<usize>>) {
    let mut merged: Vec<BTreeSet<usize>> = Vec::new();
    for g in groups.drain(..) {
        let mut set: BTreeSet<usize> = g.into_iter().collect();
        let mut i = 0;
        while i < merged.len() {
            if merged[i].intersection(&set).next().is_some() {
                set.extend(merged.swap_remove(i));
                i = 0;
            } else {
                i += 1;
            }
        }
        merged.push(set);
    }
    groups.extend(merged.into_iter().map(|s| s.into_iter().collect()));
}

/// The table columns of each exact identity finding (an arithmetic
/// identity or an affine map holding on every row): the columns one
/// equation ties together.
pub(super) fn identity_columns(table: &TypedTable, findings: &[Finding]) -> Vec<Vec<usize>> {
    findings
        .iter()
        .filter(|f| {
            matches!(
                f.finding_type.as_deref(),
                Some("arithmetic_identity" | "affine")
            ) && f.evidence.iter().all(|e| {
                e.metrics
                    .get("violations")
                    .and_then(|v| v.as_f64())
                    .is_none_or(|v| v == 0.0)
            })
        })
        .map(|f| {
            let mut cols: Vec<usize> = f
                .subject
                .columns
                .iter()
                .filter_map(|c| table.names.iter().position(|n| n == c))
                .collect();
            cols.sort_unstable();
            cols.dedup();
            cols
        })
        .filter(|cols| cols.len() >= 2)
        .collect()
}
