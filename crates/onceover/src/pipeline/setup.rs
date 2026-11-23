//! Stage 0: column exclusions, weights, metadata declarations, column cards,
//! frame model, derived-column basis (phase A), structural hint accounting,
//! and cached axis ticks.

use std::collections::{BTreeMap, BTreeSet};

use onceover_core::{CardOut, TypedCol, TypedTable};
use onceover_deps::ColInfo;
use onceover_mines::WeightSeries;
use onceover_report::{PassInfo, ProfileHints};
use polars::prelude::DataFrame;
use rayon::prelude::*;

use crate::frame::{analyze_frame, CardFacts, FrameModel};

use super::helpers::{assign_timestamp_parts, dup_examples, scaffold_note, sync_card_roles};
use super::RunState;

/// Validated declared weights, owning the extracted f64 series so passes can
/// borrow `WeightSeries` without duplicating storage.
pub(super) struct ResolvedWeights {
    pub names: Vec<String>,
    pub values: Vec<Vec<Option<f64>>>,
    pub note: String,
}

impl ResolvedWeights {
    pub fn as_series(&self) -> Vec<WeightSeries<'_>> {
        self.names
            .iter()
            .zip(self.values.iter())
            .map(|(name, values)| WeightSeries {
                name: name.as_str(),
                values: values.as_slice(),
            })
            .collect()
    }
}

/// Validated declared metadata (entity, recorded_after, outcome declarations).
pub(super) struct ResolvedMetadata {
    pub declared_entity_idx: Option<usize>,
    pub recorded_after_idx: Vec<usize>,
    pub recorded_after_names: Vec<String>,
    pub outcome_decl: onceover_mines::OutcomeDecl,
}

/// Resolved frame model, column metadata, basis, exclusions, axis gate, and
/// once-computed axis ticks shared across all downstream passes.
pub(crate) struct PreparedFrame {
    pub info: Vec<ColInfo>,
    pub sentinel_candidates: Vec<usize>,
    pub card_facts: Vec<CardFacts>,
    pub frame: FrameModel,
    pub basis: crate::basis::Basis,
    pub declared_hints: crate::declared::DeclaredHintAccount,
    pub effective_level_order: BTreeMap<String, Vec<String>>,
    pub axis_ticks: Option<Vec<Option<i64>>>,
    pub axis_excluded: Vec<usize>,
    pub scaffold_excluded: Vec<usize>,
    pub exclusion_note: String,
    pub declared_axis: Option<String>,
    pub axis_gate_open: bool,
    pub default_axis_hint: Option<String>,
    pub disc_axes: Vec<usize>,
}

/// Apply `hints.exclude_columns`, returning an optional filtered DataFrame and
/// the coverage detail note when exclusions were declared.
pub(super) fn apply_column_exclusions(
    df: &DataFrame,
    hints: Option<&ProfileHints>,
) -> (Option<DataFrame>, Option<String>) {
    let Some(hints) = hints else {
        return (None, None);
    };
    if hints.exclude_columns.is_empty() {
        return (None, None);
    }
    let present: Vec<String> = df.columns().iter().map(|c| c.name().to_string()).collect();
    let mut excluded: Vec<String> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    for name in &hints.exclude_columns {
        if excluded.contains(name) || unknown.contains(name) {
            continue;
        }
        if present.contains(name) {
            excluded.push(name.clone());
        } else {
            unknown.push(name.clone());
        }
    }
    let kept: Vec<_> = df
        .columns()
        .iter()
        .filter(|c| !excluded.iter().any(|e| e == c.name().as_str()))
        .cloned()
        .collect();
    let filtered =
        DataFrame::new_infer_height(kept).expect("filtering columns preserves the frame");
    let mut detail = format!(
        "excluded by user declaration (hints.exclude_columns): {}",
        if excluded.is_empty() {
            "none".to_string()
        } else {
            excluded.join(", ")
        }
    );
    if !unknown.is_empty() {
        detail.push_str(&format!(
            "; declared names ignored (not columns of the table): {}",
            unknown.join(", ")
        ));
    }
    (Some(filtered), Some(detail))
}

pub(super) fn stamp_extraction(state: &mut RunState<'_>, table: &TypedTable) {
    let unsupported = table
        .dtypes
        .iter()
        .filter(|d| d.as_str() == "other")
        .count();
    let time_columns: Vec<&str> = table
        .dtypes
        .iter()
        .zip(table.names.iter())
        .filter(|(d, _)| d.as_str() == "time")
        .map(|(_, n)| n.as_str())
        .collect();
    state.stamp(
        "extraction",
        PassInfo::ran(format!(
            "{} rows × {} columns{}{}",
            table.n,
            table.cols.len(),
            if unsupported > 0 {
                format!(" ({unsupported} unsupported column type(s) opaque to the profiler)")
            } else {
                String::new()
            },
            if time_columns.is_empty() {
                String::new()
            } else {
                format!(
                    "; {} time-of-day column(s) counted but not analysed (a time of day is a circular quantity, so order statistics would mislead): {}",
                    time_columns.len(),
                    time_columns.join(", ")
                )
            }
        )),
    );
}

pub(super) fn resolve_weights(table: &TypedTable, hints: Option<&ProfileHints>) -> ResolvedWeights {
    let mut names: Vec<String> = Vec::new();
    let mut values: Vec<Vec<Option<f64>>> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    if let Some(hints) = hints {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for name in &hints.weights {
            if !seen.insert(name.as_str()) {
                notes.push(format!("weight '{name}' declared twice; applied once"));
                continue;
            }
            let Some(ci) = table.names.iter().position(|n| n == name) else {
                notes.push(format!(
                    "weight '{name}' not applied: not a column of the table"
                ));
                continue;
            };
            let col = &table.cols[ci];
            if !col.is_numeric() {
                notes.push(format!("weight '{name}' not applied: not a numeric column"));
                continue;
            }
            let series: Vec<Option<f64>> = (0..table.n).map(|r| col.numeric_at(r)).collect();
            let mut total = 0.0f64;
            let mut min = f64::INFINITY;
            let mut max = f64::NEG_INFINITY;
            let mut negatives = 0usize;
            for v in series.iter().flatten() {
                total += v;
                min = min.min(*v);
                max = max.max(*v);
                if *v < 0.0 {
                    negatives += 1;
                }
            }
            if negatives > 0 {
                notes.push(format!(
                    "weight '{name}' not applied: {negatives} rows carry negative values; shares need a non-negative measure"
                ));
                continue;
            }
            if total <= 0.0 {
                notes.push(format!(
                    "weight '{name}' not applied: its total over present values is zero"
                ));
                continue;
            }
            if min == max {
                notes.push(format!(
                    "weight '{name}' not applied: it is constant ({min}), so its shares restate row shares"
                ));
                continue;
            }
            names.push(name.clone());
            values.push(series);
        }
    }
    let note = if notes.is_empty() {
        String::new()
    } else {
        format!("; {}", notes.join("; "))
    };
    ResolvedWeights {
        names,
        values,
        note,
    }
}

pub(super) fn resolve_declared_metadata(
    state: &mut RunState<'_>,
    table: &TypedTable,
    exclusion_detail: Option<&str>,
    weights: &ResolvedWeights,
) -> ResolvedMetadata {
    let mut declared_entity_idx: Option<usize> = None;
    let mut recorded_after_idx: Vec<usize> = Vec::new();
    let mut recorded_after_names: Vec<String> = Vec::new();
    let mut outcome_decl = onceover_mines::OutcomeDecl::default();

    if let Some(hints) = &state.cfg.hints {
        if !hints.weights.is_empty() {
            if !weights.names.is_empty() {
                state.decl_notes.push(format!(
                    "weights: {} checked — usable non-negative measure(s): {}",
                    weights.names.len(),
                    weights.names.join(", ")
                ));
            }
            if !weights.note.is_empty() {
                state
                    .decl_notes
                    .push(weights.note.trim_start_matches("; ").to_string());
            }
        }
        if onceover_report::DeclaredMetadata::from_hints(hints).is_some() {
            let col_exists = |name: &str| table.names.iter().position(|n| n == name);
            if !hints.gloss.is_empty() {
                let (mut applied, mut refused) = (0usize, 0usize);
                for name in hints.gloss.keys() {
                    if col_exists(name).is_some() {
                        applied += 1;
                    } else {
                        refused += 1;
                        state.decl_notes.push(format!(
                            "gloss for '{name}' not carried: not a column of the table"
                        ));
                    }
                }
                state.decl_notes.push(format!(
                    "gloss: {applied} column meaning(s) carried unchecked into the frame account and stated in the pointed findings' prose{}",
                    if refused > 0 {
                        format!("; {refused} refused")
                    } else {
                        String::new()
                    }
                ));
            }
            if !hints.units.is_empty() {
                let (mut applied, mut refused) = (0usize, 0usize);
                for name in hints.units.keys() {
                    if col_exists(name).is_some() {
                        applied += 1;
                    } else {
                        refused += 1;
                        state.decl_notes.push(format!(
                            "unit for '{name}' not carried: not a column of the table"
                        ));
                    }
                }
                state.decl_notes.push(format!(
                    "units: {applied} declared unit(s) carried unchecked and stated through the pointed findings' subject unit field{}",
                    if refused > 0 {
                        format!("; {refused} refused")
                    } else {
                        String::new()
                    }
                ));
            }
            if let Some(entity) = &hints.entity {
                match col_exists(entity) {
                    Some(idx) => {
                        declared_entity_idx = Some(idx);
                        state.decl_notes.push(format!(
                            "entity: '{entity}' checked — it is a column of the table; the panel passes use it instead of the inferred entity"
                        ));
                    }
                    None => {
                        state.decl_notes.push(format!(
                            "entity: '{entity}' refused — not a column of the table; the inferred entity is used"
                        ));
                    }
                }
            }
            if !hints.recorded_after.is_empty() {
                let mut seen: BTreeSet<&str> = BTreeSet::new();
                for name in &hints.recorded_after {
                    if !seen.insert(name.as_str()) {
                        state.decl_notes.push(format!(
                            "recorded_after: '{name}' declared twice; applied once"
                        ));
                        continue;
                    }
                    match col_exists(name) {
                        Some(idx) => {
                            recorded_after_idx.push(idx);
                            recorded_after_names.push(name.clone());
                        }
                        None => {
                            state.decl_notes.push(format!(
                                "recorded_after: '{name}' refused — not a column of the table"
                            ));
                        }
                    }
                }
                if !recorded_after_names.is_empty() {
                    state.decl_notes.push(format!(
                        "recorded_after: {} checked — column(s) of the table: {}; the leakage screen checks them first as declared suspects, and they are excluded from the predictor pools of the model-failure and predictability screens",
                        recorded_after_names.len(),
                        recorded_after_names.join(", ")
                    ));
                }
            }
            if let Some(purpose) = &hints.purpose {
                state.decl_notes.push(format!(
                    "purpose carried unchecked into the frame account: \"{purpose}\""
                ));
            }
            if !hints.questions.is_empty() {
                state.decl_notes.push(format!(
                    "{} declared question(s) carried unchecked into the frame account; the questions coverage entry states how many findings answer each",
                    hints.questions.len()
                ));
            }
            outcome_decl.event = hints.event.clone();
            outcome_decl.nulls = true;
            if let Some(target) = &state.cfg.target {
                outcome_decl.unit = hints.units.get(target).cloned();
            }
            for (column, gloss) in &hints.gloss {
                if let onceover_report::Gloss::Levels(levels) = gloss {
                    outcome_decl
                        .level_gloss
                        .insert(column.clone(), levels.clone());
                }
            }
        }
    }

    if let Some(detail) = exclusion_detail {
        state.stamp("column_exclusions", PassInfo::ran(detail.to_string()));
    }

    if let Some(hints) = &state.cfg.hints {
        if !hints.deprecated_keys.is_empty() {
            let renamed: Vec<String> = hints
                .deprecated_keys
                .iter()
                .map(|k| match k.as_str() {
                    "axis" => "`axis` is now `ordered_axis`".to_string(),
                    "ordinal" => "`ordinal` is now `level_order`".to_string(),
                    other => format!("`{other}` was renamed"),
                })
                .collect();
            state.stamp(
                "hints",
                PassInfo::ran(format!(
                    "deprecated hint keys accepted and renamed: {}; the old names are accepted for one release",
                    renamed.join("; ")
                )),
            );
        }
    }

    if let Some(source) = &state.cfg.source {
        let detail = if source.basis == "sampled" {
            format!(
                "{} source of {} rows profiled on a stratified sample of {} rows (seed {}); \
                 every finding's evidence scope records the source population",
                source.kind,
                source.rows,
                source.rows_profiled,
                source.sample.as_ref().map(|s| s.seed).unwrap_or(0),
            )
        } else {
            format!(
                "{} source: {} rows, collected in full",
                source.kind, source.rows
            )
        };
        let info = if source.basis == "sampled" {
            PassInfo::sampled(detail)
        } else {
            PassInfo::ran(detail)
        };
        state.stamp("input", info);
    }

    ResolvedMetadata {
        declared_entity_idx,
        recorded_after_idx,
        recorded_after_names,
        outcome_decl,
    }
}

pub(super) fn prepare_frame(state: &mut RunState<'_>, table: &TypedTable) -> PreparedFrame {
    // ---------- column cards ----------
    let cards: Vec<CardOut> = table
        .cols
        .par_iter()
        .enumerate()
        .map(|(i, col)| onceover_core::column_card(&table.names[i], &table.dtypes[i], col))
        .collect();
    state.stamp(
        "column_cards",
        PassInfo::ran(format!("{} cards (exact)", cards.len())),
    );

    let info: Vec<ColInfo> = cards
        .iter()
        .enumerate()
        .map(|(i, c)| ColInfo {
            name: c.card.name.clone(),
            dtype: table.dtypes[i].clone(),
            card_role: c.card.card_role.clone(),
            n: c.card.n,
            null_count: c.card.null_count,
            distinct: c.card.distinct,
            dup_examples: dup_examples(table, i, c),
        })
        .collect();

    let sentinel_candidates: Vec<usize> = cards
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            c.counts
                .top
                .first()
                .is_some_and(|(_, count)| *count >= onceover_deps::sentinels::MIN_OCCURRENCES)
        })
        .map(|(i, _)| i)
        .collect();

    let card_facts: Vec<CardFacts> = cards
        .iter()
        .map(|c| CardFacts {
            card_role: c.card.card_role.clone(),
            distinct: c.card.distinct,
            null_count: c.card.null_count,
            numeric_min: c.card.numeric_stats.as_ref().and_then(|s| s.min),
            numeric_max: c.card.numeric_stats.as_ref().and_then(|s| s.max),
            integral: c.card.numeric_stats.as_ref().is_some_and(|s| s.integral),
        })
        .collect();

    let col_profile_enabled = state.enabled("column_profile");
    for c in cards {
        state.columns.insert(c.card.name.clone(), c.card);
        if col_profile_enabled {
            state.findings.extend(c.findings);
        }
    }

    // ---------- frame model: semantic roles + assembled axis ----------
    let mut frame = analyze_frame(table, &card_facts, state.cfg.hints.as_ref());
    frame.info.declarations = state
        .cfg
        .hints
        .as_ref()
        .and_then(onceover_report::DeclaredMetadata::from_hints);

    // ---------- derived-column basis, phase A (issue #374) ----------
    let mut basis = crate::basis::Basis::default();
    {
        let (axis_cols, derived) =
            crate::basis::axis_derived(table, &frame, &card_facts, state.cfg.hints.as_ref());
        basis.axis_cols = axis_cols;
        assign_timestamp_parts(&mut frame.roles, &mut frame.info.roles, &derived);
        basis.axis_derived = derived;
        for (f, x, evidence) in crate::basis::missingness_indicators(
            table,
            &frame,
            &card_facts,
            state.cfg.hints.as_ref(),
        ) {
            if let Some(r) = frame.info.roles.get_mut(f) {
                r.evidence = evidence;
            }
            basis.indicators.push((f, x));
        }
        state.tier.encode.skip_columns = basis.not_input();
    }
    sync_card_roles(&mut state.columns, table, &frame);
    state.findings.retain(|f| {
        f.finding_type.as_deref() != Some("categorical_role")
            || !f.subject.columns.iter().all(|c| {
                table
                    .names
                    .iter()
                    .position(|n| n == c)
                    .is_some_and(|i| frame.roles[i].as_deref() == Some("measure"))
            })
    });

    // ---------- structural hint accounting (issue #441) ----------
    let declared_hints = crate::declared::account(table, &frame, state.cfg.hints.as_ref());
    state
        .decl_notes
        .extend(declared_hints.notes.iter().cloned());
    let effective_level_order = declared_hints.level_order.clone();

    // Compute per-row axis ticks ONCE and share across all temporal & panel passes.
    let axis_ticks = crate::frame::axis_ticks_micros(table, &frame);

    let role_excluded: Vec<usize> = (0..table.cols.len())
        .filter(|&i| {
            matches!(info[i].card_role.as_str(), "constant" | "all_null")
                || matches!(
                    frame.roles[i].as_deref(),
                    Some("row_number" | "identifier" | "timestamp" | "timestamp_part")
                )
                || (frame.axis_source.as_deref() == Some("timestamp_parts")
                    && frame.axis_parts.contains(&table.names[i]))
        })
        .collect();
    let axis_source_columns: Vec<String> = frame
        .info
        .axis
        .as_ref()
        .map(|axis| {
            let mut cols: Vec<String> = axis.parts.clone();
            if let Some(src) = &axis.source_column {
                if !cols.contains(src) {
                    cols.push(src.clone());
                }
            }
            cols
        })
        .unwrap_or_default();
    let axis_excluded: Vec<usize> = (0..table.cols.len())
        .filter(|&i| {
            axis_source_columns.contains(&table.names[i])
                && !role_excluded.contains(&i)
                && !state
                    .cfg
                    .hints
                    .as_ref()
                    .is_some_and(|h| h.roles.contains_key(&table.names[i]))
        })
        .collect();
    let mut scaffold_excluded = role_excluded;
    scaffold_excluded.extend(axis_excluded.iter().copied());
    for (f, _) in &basis.indicators {
        if !scaffold_excluded.contains(f) {
            scaffold_excluded.push(*f);
        }
    }
    scaffold_excluded.sort_unstable();
    let exclusion_note = scaffold_note(
        table,
        &frame,
        &info,
        &scaffold_excluded,
        &axis_excluded,
        &basis,
    );

    // ---------- along-axis gate (issue #317, phase 3) ----------
    let declared_axis = declared_hints.ordered_axis.clone();
    let axis_declared = declared_axis.is_some();
    let axis_gate_open = frame.axis_name.is_some() || axis_declared;
    let default_axis_hint = declared_axis.clone().or_else(|| frame.axis_name.clone());
    let disc_axes: Vec<usize> = if axis_declared {
        declared_axis
            .as_deref()
            .and_then(|name| {
                table
                    .names
                    .iter()
                    .position(|n| n == name)
                    .filter(|&i| table.cols[i].is_numeric() && !scaffold_excluded.contains(&i))
            })
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };

    PreparedFrame {
        info,
        sentinel_candidates,
        card_facts,
        frame,
        basis,
        declared_hints,
        effective_level_order,
        axis_ticks,
        axis_excluded,
        scaffold_excluded,
        exclusion_note,
        declared_axis,
        axis_gate_open,
        default_axis_hint,
        disc_axes,
    }
}

pub(super) fn count_numeric_cols(table: &TypedTable) -> usize {
    table
        .cols
        .iter()
        .filter(|c| {
            matches!(
                c,
                TypedCol::F64 { .. }
                    | TypedCol::Decimal { .. }
                    | TypedCol::I64 { .. }
                    | TypedCol::U64 { .. }
            )
        })
        .count()
}
