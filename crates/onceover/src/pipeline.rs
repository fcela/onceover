//! The pipeline: run passes in cost order under the budget, assemble the
//! findings ledger, overview and coverage block. Deterministic end to end
//! (unless an opt-in wall-clock deadline fires, which is reported honestly).

use onceover_report::{Method, Pattern};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use onceover_core::{extract, find_duplicate_rows, CardOut, TypedCol, TypedTable, VKey};
use onceover_deps::{analyze_keys, discover, sortedness, ColInfo, DepsOutcome};
use onceover_mines::{
    analyze_temporal, association_rules, decision_tables, encode, hotspots, identities,
    prism_rules, profile_breaks, profile_discontinuities, profile_drift, profile_interactions,
    profile_regimes, profile_surprises, profile_target_shapes, screen_leakage,
    screen_model_failures, screen_predictability, WeightSeries,
};
use onceover_report::{
    group_findings, select_findings_counted, Category, ColumnCard, Finding, Overview, PassInfo,
    ProfileReport, Sampling, SCHEMA_VERSION,
};
use polars::prelude::DataFrame;
use serde_json::json;

use crate::config::{Budget, ProfileConfig, TierConfig};
use crate::frame::{analyze_frame, build_temporal_df, CardFacts};

/// Profile a DataFrame. Returns the complete, deterministic report.
pub fn profile(df: &DataFrame, cfg: &ProfileConfig) -> ProfileReport {
    profile_with_detectors(df, cfg, &[])
}

pub fn profile_with_detectors(
    df: &DataFrame,
    cfg: &ProfileConfig,
    extensions: &[&dyn crate::Detector],
) -> ProfileReport {
    profile_inner(df, cfg, extensions, 0)
}

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
fn entity_columns(
    table: &TypedTable,
    frame: &crate::frame::FrameModel,
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

fn profile_inner(
    df: &DataFrame,
    cfg: &ProfileConfig,
    extensions: &[&dyn crate::Detector],
    depth: u8,
) -> ProfileReport {
    let mut requested = cfg.detectors.clone();
    if let Some(ids) = &mut requested {
        loop {
            let n = ids.len();
            for detector in extensions {
                let d = detector.descriptor();
                if ids.contains(&d.id) {
                    ids.extend(d.requires);
                }
            }
            if ids.len() == n {
                break;
            }
        }
    }
    let mut active = crate::detectors::active_detectors(requested.as_ref());
    // Designating a target activates the target-aware pass when the
    // caller did not explicitly restrict the detector set.
    if cfg.target.is_some() && cfg.detectors.is_none() {
        active.insert("outcome_account".into());
        active.insert("outcome_contrasts".into());
        active.insert("target_shapes".into());
        active.insert("leakage".into());
        active.insert("model_failures".into());
    }
    let enabled = |id: &str| active.contains(id);
    let selected = |id: &str| cfg.detectors.as_ref().is_none_or(|ids| ids.contains(id));
    let activations: BTreeMap<String, (crate::detectors::Membership, crate::detectors::Trigger)> =
        crate::detectors::builtin_detectors()
            .into_iter()
            .map(|d| (d.id, (d.membership, d.trigger)))
            .collect();
    let mut shared_encoding = None;
    let start = Instant::now();
    let mut tier = TierConfig::for_budget(cfg.budget);
    if let Some(rows) = cfg.sample_rows {
        tier.encode.row_cap = rows.max(1);
        tier.fd.row_cap = rows.max(1);
        tier.screen.max_model_rows = rows.max(1);
        tier.regimes.row_cap = rows.max(1);
    }
    tier.views.row_cap = tier.encode.row_cap;
    tier.views.search_rows = tier.encode.row_cap;
    tier.slices.row_cap = tier.encode.row_cap;

    if let Some(predictors) = cfg.max_predictors {
        tier.screen.max_predictors = predictors.max(1);
        tier.encode.feature_cap = predictors.max(1);
    }
    if let Some(pairs) = cfg.max_pairs {
        tier.dep_pair_cap = pairs;
    }
    // One absolute deadline for every in-pass checkpoint
    // (issue #117, stage 2): profile start + the configured
    // duration — the same instant the between-pass checks
    // measure against, so a pass that starts before the
    // deadline stops inside its own loops at that instant
    // instead of overrunning by its whole runtime.
    let pass_deadline = cfg
        .deadline
        .map_or_else(onceover_mines::Deadline::none, |d| {
            onceover_mines::Deadline::at(start + d)
        });
    tier.disc.deadline = pass_deadline;
    tier.hotspots.deadline = pass_deadline;
    tier.surprises.deadline = pass_deadline;
    // A default run requires a categorical outlier statement's
    // deviating levels to be rare in their column overall
    // (issue #317, phase 3); naming the outliers detector
    // explicitly lifts the requirement.
    tier.surprises.require_rare_breakers = !cfg
        .detectors
        .as_ref()
        .is_some_and(|set| set.contains("outliers"));
    tier.slices.deadline = pass_deadline;
    tier.modelfail.deadline = pass_deadline;
    tier.dupe.deadline = cfg.deadline.map(|d| start + d);
    let mut coverage: BTreeMap<String, PassInfo> = BTreeMap::new();
    let mut last_mark = Instant::now();
    let timing_on = cfg.timing;

    let mut deadline_hit = false;
    let check_deadline = |coverage: &mut BTreeMap<String, PassInfo>, name: &str| -> bool {
        if let Some(d) = cfg.deadline {
            if start.elapsed() > d {
                coverage.insert(name.to_string(), PassInfo::skipped("deadline reached"));
                return true;
            }
        }
        false
    };

    // ---------- declared column exclusions (issue #207) ----------
    // Columns named by `hints.exclude_columns` are removed before
    // extraction, so no pass ever sees them: an excluded column
    // gets no card, no role, and no finding can name it. The
    // exclusion and its source are stated in the
    // `column_exclusions` coverage entry below; declared names
    // that are not columns of the table are ignored and stated
    // there too.
    let mut exclusion_detail: Option<String> = None;
    let df_storage: DataFrame;
    let df: &DataFrame = match cfg.hints.as_ref() {
        Some(hints) if !hints.exclude_columns.is_empty() => {
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
            df_storage =
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
            exclusion_detail = Some(detail);
            &df_storage
        }
        _ => df,
    };

    // ---------- extraction ----------
    let table = extract(df);
    let unsupported = table
        .dtypes
        .iter()
        .filter(|d| d.as_str() == "other")
        .count();
    // Time-of-day columns are counted but not analysed, by decision:
    // a time of day is a circular quantity, so the order statistics
    // the analysed kinds receive would mislead. State it here, at
    // the point of extraction, rather than leaving the columns
    // silently opaque.
    let time_columns: Vec<&str> = table
        .dtypes
        .iter()
        .zip(table.names.iter())
        .filter(|(d, _)| d.as_str() == "time")
        .map(|(_, n)| n.as_str())
        .collect();
    stamp_insert(
        &mut coverage,
        &mut last_mark,
        timing_on,
        "extraction".into(),
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

    // ---------- declared weights (issue #329) ----------
    // Resolve the `weights` hint against the extracted table:
    // each declared column is validated once here and handed
    // to the hotspots pass as a value series. A declaration
    // that cannot be applied is stated in the hotspots
    // coverage entry (through `weight_note` below), never
    // silently dropped and never raised.
    let mut weight_names: Vec<&str> = Vec::new();
    let mut weight_values: Vec<Vec<Option<f64>>> = Vec::new();
    let mut weight_notes: Vec<String> = Vec::new();
    if let Some(hints) = &cfg.hints {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for name in &hints.weights {
            if !seen.insert(name.as_str()) {
                weight_notes.push(format!("weight '{name}' declared twice; applied once"));
                continue;
            }
            let Some(ci) = table.names.iter().position(|n| n == name) else {
                weight_notes.push(format!(
                    "weight '{name}' not applied: not a column of the table"
                ));
                continue;
            };
            let col = &table.cols[ci];
            if !matches!(
                col,
                TypedCol::I64 { .. }
                    | TypedCol::U64 { .. }
                    | TypedCol::F64 { .. }
                    | TypedCol::Decimal { .. }
            ) {
                weight_notes.push(format!("weight '{name}' not applied: not a numeric column"));
                continue;
            }
            let values: Vec<Option<f64>> = (0..table.n).map(|r| col.numeric_at(r)).collect();
            let mut total = 0.0f64;
            let mut min = f64::INFINITY;
            let mut max = f64::NEG_INFINITY;
            let mut negatives = 0usize;
            for v in values.iter().flatten() {
                total += v;
                min = min.min(*v);
                max = max.max(*v);
                if *v < 0.0 {
                    negatives += 1;
                }
            }
            if negatives > 0 {
                weight_notes.push(format!(
                    "weight '{name}' not applied: {negatives} rows carry negative values; shares need a non-negative measure"
                ));
                continue;
            }
            if total <= 0.0 {
                weight_notes.push(format!(
                    "weight '{name}' not applied: its total over present values is zero"
                ));
                continue;
            }
            if min == max {
                weight_notes.push(format!(
                    "weight '{name}' not applied: it is constant ({min}), so its shares restate row shares"
                ));
                continue;
            }
            weight_names.push(name.as_str());
            weight_values.push(values);
        }
    }
    // The series borrow the owned value vectors; both live
    // until the hotspots dispatches below have run.
    let weight_series: Vec<WeightSeries> = weight_names
        .iter()
        .zip(weight_values.iter())
        .map(|(name, values)| WeightSeries {
            name,
            values: values.as_slice(),
        })
        .collect();
    let weight_note = if weight_notes.is_empty() {
        String::new()
    } else {
        format!("; {}", weight_notes.join("; "))
    };

    // ---------- declared metadata (issue #310) ----------
    // Resolve the metadata keys against the extracted table:
    // what can be checked is checked here (the entity and the
    // recorded-after columns exist; the declared event names a
    // level of the target — checked at the outcome dispatch,
    // which codes the target), and the rest is carried unchecked
    // into the frame account. Every declaration is accounted for
    // in `decl_notes`, stated in the report's `declarations`
    // coverage entry at assembly; nothing is silently dropped
    // and nothing raises.
    let mut decl_notes: Vec<String> = Vec::new();
    let mut declared_entity_idx: Option<usize> = None;
    let mut recorded_after_idx: Vec<usize> = Vec::new();
    let mut recorded_after_names: Vec<String> = Vec::new();
    let mut outcome_decl = onceover_mines::OutcomeDecl::default();
    if let Some(hints) = &cfg.hints {
        if onceover_report::DeclaredMetadata::from_hints(hints).is_some() {
            let col_exists = |name: &str| table.names.iter().position(|n| n == name);
            if !hints.gloss.is_empty() {
                let (mut applied, mut refused) = (0usize, 0usize);
                for name in hints.gloss.keys() {
                    if col_exists(name).is_some() {
                        applied += 1;
                    } else {
                        refused += 1;
                        decl_notes.push(format!(
                            "gloss for '{name}' not carried: not a column of the table"
                        ));
                    }
                }
                decl_notes.push(format!(
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
                        decl_notes.push(format!(
                            "unit for '{name}' not carried: not a column of the table"
                        ));
                    }
                }
                decl_notes.push(format!(
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
                        decl_notes.push(format!(
                            "entity: '{entity}' checked — it is a column of the table; the panel passes use it instead of the inferred entity"
                        ));
                    }
                    None => {
                        decl_notes.push(format!(
                            "entity: '{entity}' refused — not a column of the table; the inferred entity is used"
                        ));
                    }
                }
            }
            if !hints.recorded_after.is_empty() {
                let mut seen: BTreeSet<&str> = BTreeSet::new();
                for name in &hints.recorded_after {
                    if !seen.insert(name.as_str()) {
                        decl_notes.push(format!(
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
                            decl_notes.push(format!(
                                "recorded_after: '{name}' refused — not a column of the table"
                            ));
                        }
                    }
                }
                if !recorded_after_names.is_empty() {
                    decl_notes.push(format!(
                        "recorded_after: {} checked — column(s) of the table: {}; the leakage screen checks them first as declared suspects, and they are excluded from the predictor pools of the model-failure and predictability screens",
                        recorded_after_names.len(),
                        recorded_after_names.join(", ")
                    ));
                }
            }
            if let Some(purpose) = &hints.purpose {
                decl_notes.push(format!(
                    "purpose carried unchecked into the frame account: \"{purpose}\""
                ));
            }
            if !hints.questions.is_empty() {
                decl_notes.push(format!(
                    "{} declared question(s) carried unchecked into the frame account; the questions coverage entry states how many findings answer each",
                    hints.questions.len()
                ));
            }
            // The outcome screen's declaration bundle.
            outcome_decl.event = hints.event.clone();
            outcome_decl.nulls = true;
            if let Some(target) = &cfg.target {
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

    if let Some(detail) = &exclusion_detail {
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "column_exclusions".into(),
            PassInfo::ran(detail.clone()),
        );
    }

    if let Some(hints) = &cfg.hints {
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
            stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                "hints".into(),
                PassInfo::ran(format!(
                    "deprecated hint keys accepted and renamed: {}; the old names are accepted for one release",
                    renamed.join("; ")
                )),
            );
        }
    }

    if let Some(source) = &cfg.source {
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
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "input".into(),
            info,
        );
    }

    // ---------- column cards ----------
    use rayon::prelude::*;
    let cards: Vec<CardOut> = table
        .cols
        .par_iter()
        .enumerate()
        .map(|(i, col)| onceover_core::column_card(&table.names[i], &table.dtypes[i], col))
        .collect();
    stamp_insert(
        &mut coverage,
        &mut last_mark,
        timing_on,
        "column_cards".into(),
        PassInfo::ran(format!("{} cards (exact)", cards.len())),
    );

    // ColInfo for downstream passes.
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
            dup_examples: dup_examples(&table, i, c),
        })
        .collect();

    let mut findings: Vec<Finding> = Vec::new();
    // Every pair the association screens test, for the report's
    // associations table (issue #134); filled by the dependence
    // screen and the association pair screen below.
    let mut association_pairs: Vec<onceover_report::AssociationPair> = Vec::new();
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
    let mut columns: BTreeMap<String, ColumnCard> = BTreeMap::new();
    for c in cards {
        columns.insert(c.card.name.clone(), c.card.clone());
        if enabled("column_profile") {
            findings.extend(c.findings);
        }
    }

    // ---------- frame model: semantic roles + assembled axis ----------
    // Computed before any detector runs. Detectors consume it two
    // ways: scaffolding columns (index / identifier / timestamp, and
    // the parts of an assembled axis) are excluded as targets, and
    // the temporal passes receive the assembled axis — materialized
    // as a real date/datetime column in a cloned frame when the axis
    // comes from strings or parts, so their existing axis selection
    // finds it. Column indices are preserved by the materialization
    // (the axis replaces its source column in place).
    let mut frame = analyze_frame(&table, &card_facts, cfg.hints.as_ref());
    // The declared metadata (issue #310) rides the frame account
    // as declared; the declarations resolution below accounts for
    // what was checked and what was carried unchecked.
    frame.info.declarations = cfg
        .hints
        .as_ref()
        .and_then(onceover_report::DeclaredMetadata::from_hints);
    // ---------- derived-column basis, phase A (issue #374) ----------
    // Columns the axis determines are calendar parts: they get the
    // timestamp_part role here, before any detector runs, so no pass
    // restates the calendar (see `basis`).
    let mut basis = crate::basis::Basis::default();
    {
        let (axis_cols, derived) =
            crate::basis::axis_derived(&table, &frame, &card_facts, cfg.hints.as_ref());
        basis.axis_cols = axis_cols;
        assign_timestamp_parts(&mut frame.roles, &mut frame.info.roles, &derived);
        basis.axis_derived = derived;
        // Missingness indicators keep their role (a flag) and carry
        // the indicator evidence; they are no later pass's subject or
        // input.
        for (f, x, evidence) in
            crate::basis::missingness_indicators(&table, &frame, &card_facts, cfg.hints.as_ref())
        {
            if let Some(r) = frame.info.roles.get_mut(f) {
                r.evidence = evidence;
            }
            basis.indicators.push((f, x));
        }
        tier.encode.skip_columns = basis.not_input();
    }
    sync_card_roles(&mut columns, &table, &frame);
    // A numeric column the frame model reads as a measure (a value
    // grid too fine to group by) does not "behave like a category":
    // the card heuristic's statement would contradict the role the
    // passes use.
    findings.retain(|f| {
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
    // The four hint keys that steer the frame model and the
    // along-axis passes are instructions rather than metadata: they
    // do not ride `frame.declarations`, but every declaration must
    // still be accounted for. Resolve each one once, here, and add
    // its applied/refused decision to the same `declarations`
    // coverage entry as the metadata keys. The resolved values are
    // what the passes below consume, so a refused declaration does
    // not silently steer a scan.
    let declared_hints = crate::declared::account(&table, &frame, cfg.hints.as_ref());
    decl_notes.extend(declared_hints.notes.iter().cloned());
    let effective_level_order = declared_hints.level_order.clone();
    let temporal_df = build_temporal_df(df, &frame);
    let temporal_table = temporal_df.as_ref().map(|d| extract(d));
    let ttable: &TypedTable = temporal_table.as_ref().unwrap_or(&table);
    let frame_time_hint: Option<&str> = frame.axis_name.as_deref();
    let role_excluded: Vec<usize> = (0..table.cols.len())
        .filter(|&i| {
            matches!(info[i].card_role.as_str(), "constant" | "all_null")
                || matches!(
                    frame.roles[i].as_deref(),
                    Some("row_number")
                        | Some("identifier")
                        | Some("timestamp")
                        | Some("timestamp_part")
                )
                || (frame.axis_source.as_deref() == Some("timestamp_parts")
                    && frame.axis_parts.contains(&table.names[i]))
        })
        .collect();
    // The time axis is scaffolding by structure, not only by role
    // (issue #207): the column(s) the frame model assembled as the
    // report's time axis are excluded from target roles whatever
    // role they were assigned — a bare week integer can carry the
    // measure role and would otherwise stay target-eligible, and a
    // native timestamp was only protected because its role happens
    // to say so. A `hints.roles` entry for the axis column is an
    // explicit user declaration and wins over the structural
    // default.
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
                && !cfg
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
    let mut exclusion_note = scaffold_note(
        &table,
        &frame,
        &info,
        &scaffold_excluded,
        &axis_excluded,
        &basis,
    );

    // ---------- along-axis gate (issue #317, phase 3) ----------
    // In a default run the along-axis passes (breaks,
    // ordered_partitions, ordered_axis, discontinuities) scan along
    // one axis: a caller-declared `ordered_axis` when a pass can use
    // it, else the frame model's time axis. The registry's
    // membership/trigger vocabulary has no condition for "the frame
    // model found a time axis", so the gate lives here and each
    // gated pass records the skip in its coverage entry. Naming a
    // detector explicitly lifts the gate for that detector. An
    // already-expired deadline takes precedence over the gate for
    // the discontinuities screen: its dispatch is deadline-exempt
    // by design, and its coverage entry records the stop.
    // Only a checked and usable declaration opens the default
    // along-axis gate; an existing but unusable column was refused
    // above and behaves like a missing one (issue #441).
    let declared_axis = declared_hints.ordered_axis.as_deref();
    let axis_declared = declared_axis.is_some();
    let axis_gate_open = frame_time_hint.is_some() || axis_declared;
    let axis_explicit = |id: &str| cfg.detectors.as_ref().is_some_and(|set| set.contains(id));
    let axis_gate_reason = |pass: &str| {
        format!(
            "{pass} not run: a default run scans along the frame's time axis or an axis \
             declared with the ordered_axis hint, and this frame has neither; name the \
             detector explicitly or declare an axis to run it"
        )
    };
    // The axis a hinted-only pass is handed: the declared axis when
    // present, else the frame's time axis. A pass that cannot use
    // the named column says so in its own coverage text.
    let default_axis_hint = match declared_axis {
        Some(name) => Some(name),
        None => frame_time_hint,
    };
    // Discontinuities choose axes from the numeric pool, which the
    // time axis never joins (date and datetime columns are not
    // numeric to it), so for that pass only a declared axis opens
    // the gate in a default run.
    let disc_declared_axis: Option<usize> = if axis_declared {
        declared_axis.and_then(|name| {
            table
                .names
                .iter()
                .position(|n| n == name)
                .filter(|&i| table.cols[i].is_numeric() && !scaffold_excluded.contains(&i))
        })
    } else {
        None
    };
    let disc_axes: Vec<usize> = disc_declared_axis.into_iter().collect();

    // ---------- duplicate rows ----------
    if check_deadline(&mut coverage, "row_duplicates") {
        deadline_hit = true;
    }
    if !deadline_hit
        && enabled("row_duplicates")
        && table.cols.iter().any(|c| {
            !matches!(
                c,
                onceover_core::TypedCol::Null { .. } | onceover_core::TypedCol::Other { .. }
            )
        })
    {
        match if info
            .iter()
            .any(|c| c.null_count == 0 && c.distinct == table.n)
        {
            None
        } else {
            find_duplicate_rows(&table)
        } {
            Some(dup) => {
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "row_duplicates".into(),
                    PassInfo::ran(format!(
                        "{} groups, {} redundant rows (exact)",
                        dup.n_groups, dup.n_redundant
                    )),
                );
                let mut ev = BTreeMap::new();
                ev.insert("n".into(), json!(table.n));
                ev.insert("n_redundant".to_string(), json!(dup.n_redundant));
                ev.insert("n_groups".to_string(), json!(dup.n_groups));
                ev.insert("max_group_size".to_string(), json!(dup.max_group_size));
                ev.insert(
                    "examples".to_string(),
                    json!(dup.examples.iter().map(|(idx, _)| idx).collect::<Vec<_>>()),
                );
                // Bespoke evidence payload for this finding type: the
                // largest duplicate groups with their values rendered,
                // so the claim can be inspected without a follow-up
                // query. Values are the comparable columns (the same
                // set the subject lists), first 8, joined with " | ".
                let comparable: Vec<usize> = table
                    .cols
                    .iter()
                    .enumerate()
                    .filter(|(_, col)| {
                        !matches!(
                            col,
                            onceover_core::TypedCol::Null { .. }
                                | onceover_core::TypedCol::Other { .. }
                        )
                    })
                    .map(|(i, _)| i)
                    .collect();
                let mut groups_json = Vec::new();
                for (idx, size) in &dup.examples {
                    let Some(&first) = idx.first() else { continue };
                    let values: Vec<String> = comparable
                        .iter()
                        .take(8)
                        .map(|&c| table.cols[c].render_cell(first as usize))
                        .collect();
                    let mut joined = values.join(" | ");
                    if comparable.len() > 8 {
                        joined.push_str(" | …");
                    }
                    groups_json.push(json!({
                        "values": joined,
                        "rows": size,
                        "first_row": first,
                    }));
                }
                ev.insert("duplicate_groups".to_string(), json!(groups_json));
                // The support of the compared columns: how many
                // distinct rows they can form at all, the product of
                // their distinct counts (a column with missing
                // values counts one more value for the missing
                // state). When the table has realised nearly every
                // combination the support allows, the repeats are
                // forced by the support itself: three five-level
                // columns form 125 possible rows, so 5,000 draws
                // repeat earlier rows whatever the recording
                // process did. That is a cardinality fact, stated
                // as concentration, not a duplicate-rows claim.
                let distinct_rows = table.n - dup.n_redundant;
                let support: f64 = comparable
                    .iter()
                    .map(|&c| {
                        (info[c].distinct + usize::from(info[c].null_count > 0)).max(1) as f64
                    })
                    .product();
                ev.insert("distinct_rows".to_string(), json!(distinct_rows));
                ev.insert(
                    "support_size".to_string(),
                    json!(if support.is_finite() {
                        support
                    } else {
                        f64::MAX
                    }),
                );
                let saturated = table.n >= 100
                    && comparable.len() >= 2
                    && support <= table.n as f64
                    && distinct_rows as f64 >= 0.9 * support;
                if saturated {
                    findings.push(Finding::new(
                        Pattern::ValueConcentration,
                        BTreeMap::from([("columns".into(), json!(comparable.iter().map(|&c| table.names[c].clone()).collect::<Vec<_>>()))]),
                        format!(
                            "the compared columns form only {} distinct rows in {} rows: their {} possible combinations are nearly all realised, so repeated rows are expected from the small support",
                            distinct_rows, table.n, support as u64
                        ),
                        onceover_report::EvidenceBasis::Measured,
                        0.50,
                        ev,
                        Some("repeats are forced by the number of possible rows; read this as a cardinality fact about the compared columns, not as duplicated records".into()),
                        Method::new(
                            "row_duplicates",
                            "support_saturation",
                            "exact row hashing + verification; support size from the compared columns' distinct counts (onceover v0.1)",
                        ),
                    ));
                } else {
                    findings.push(Finding::new(
                    Pattern::Duplicates,
                    BTreeMap::from([("columns".into(),json!(table.names.iter().zip(&table.cols).filter(|(_,col)|!matches!(col,onceover_core::TypedCol::Null{..} | onceover_core::TypedCol::Other{..})).map(|(name,_)|name).collect::<Vec<_>>()))]),
                    format!(
                        "{} rows repeat earlier values across the compared columns ({} groups, largest {})",
                        dup.n_redundant, dup.n_groups, dup.max_group_size
                    ),
                    onceover_report::EvidenceBasis::Measured,
                    0.70,
                    ev,
                    Some("deduplicate before joins or model training".into()),
                    Method::new(
                        "row_duplicates",
                        "row_equality",
                        "exact row hashing + verification (onceover v0.1)",
                    ),
                ));
                }
            }
            None => {
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "row_duplicates".into(),
                    PassInfo::ran("no exact duplicate rows (exact)"),
                );
            }
        }
    }

    if enabled("row_duplicates")
        && table.cols.iter().all(|c| {
            matches!(
                c,
                onceover_core::TypedCol::Null { .. } | onceover_core::TypedCol::Other { .. }
            )
        })
    {
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "row_duplicates".into(),
            PassInfo::na("no comparable columns"),
        );
    }

    // ---------- keys & dependencies ----------
    let (deps, key_findings): (DepsOutcome, Vec<Finding>) =
        if cfg.budget == Budget::Quick || !enabled("functional_dependencies") {
            let d = if cfg.budget == Budget::Quick {
                quick_deps(&info, table.n)
            } else if enabled("keys") {
                onceover_deps::fd::discover_keys(&table, &info, &tier.fd)
            } else {
                DepsOutcome::default()
            };
            let reason = if cfg.budget == Budget::Quick {
                "quick tier"
            } else if enabled("keys") {
                "functional dependencies not enabled; key discovery only"
            } else {
                "detector not enabled"
            };
            stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                "functional_dependencies".into(),
                PassInfo::skipped(reason),
            );
            let k = analyze_keys(&info, &d);
            (d, k.findings)
        } else {
            if check_deadline(&mut coverage, "functional_dependencies") {
                deadline_hit = true;
            }
            if deadline_hit {
                (DepsOutcome::default(), Vec::new())
            } else {
                let d = discover(&table, &info, &tier.fd, &scaffold_excluded);
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "functional_dependencies".into(),
                    if !d.caps_bound.is_empty() {
                        PassInfo::capped(format!("{}{}", d.coverage, exclusion_note))
                    } else if d.sampled {
                        PassInfo::sampled(format!("{}{}", d.coverage, exclusion_note))
                    } else {
                        PassInfo::ran(format!("{}{}", d.coverage, exclusion_note))
                    },
                );
                let k = analyze_keys(&info, &d);
                let mut d = d;
                findings.extend(std::mem::take(&mut d.findings));
                (d, k.findings)
            }
        };
    let key_findings = if enabled("keys") {
        key_findings
    } else {
        Vec::new()
    };
    if !key_findings.is_empty() {
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "keys".into(),
            PassInfo::ran(format!(
                "{} single-column unique, {} composite",
                deps.single_unique.len(),
                deps.composite_uccs.len()
            )),
        );
        findings.extend(key_findings);
    } else {
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "keys".into(),
            PassInfo::na(format!(
                "{} single-column unique, {} composite",
                deps.single_unique.len(),
                deps.composite_uccs.len()
            )),
        );
    }

    // ---------- frame account (frame-model step 2) ----------
    // The grain and the dominant structure, computed from the frame
    // model and the dependency outcome. The grain finding replaces
    // the bare key finding for the same columns (the statement is
    // strictly richer: it carries the factorization); the near-key
    // finding states the axis's repeats when no exact key exists.
    {
        let account = crate::account::compute_account(&table, &frame, &card_facts, &deps);
        if !account.replaced_key_subjects.is_empty() {
            let replaced = &account.replaced_key_subjects;
            findings.retain(|f| {
                !(f.category == Category::Uniqueness
                    && f.evidence.iter().any(|e| e.method.detector == "keys")
                    && replaced
                        .iter()
                        .any(|s| *s == f.subject.columns.iter().cloned().collect()))
            });
        }
        findings.extend(account.findings);
        let mut detail = String::new();
        match &account.grain {
            Some(g) => detail.push_str(&format!("grain: {}", g.statement)),
            None if account.grain_unresolved => detail.push_str(
                "grain: no exact key found among the dimension columns and the time axis",
            ),
            None => detail.push_str("grain: no key candidates (no time axis or dimension columns)"),
        }
        if !account.replaced_key_subjects.is_empty() {
            detail
                .push_str("; the grain finding replaces the bare key finding for the same columns");
        }
        if account.dominant.is_empty() {
            detail.push_str("; dominant structure: none at the 0.10 variance-share bar");
        } else {
            let labels: Vec<String> = account.dominant.iter().map(|d| d.label.clone()).collect();
            detail.push_str(&format!("; dominant structure: {}", labels.join(", ")));
        }
        detail.push_str(
            "; columns the time axis determines are timestamp parts before the passes run and are no pass's subject; findings whose whole subject is scaffolding columns are demoted in ranking, never removed; the demotion counts are appended to this entry",
        );
        frame.info.grain = account.grain;
        frame.info.dominant = account.dominant;
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "frame_model".into(),
            PassInfo::ran(detail),
        );
    }

    // ---------- derived-column basis, phase B (issue #374) ----------
    // With the exact dependencies and the grain in hand: columns the
    // derived ones determine, and a grain factor that subdivides the
    // axis's period (an hour beside a date). Dependency statements
    // entirely inside the calendar family restate the derivation the
    // roles now carry, and are dropped.
    {
        crate::basis::close_and_sub_period(
            &mut basis,
            &table,
            &frame,
            &card_facts,
            &deps.exact_fds,
            cfg.hints.as_ref(),
        );
        assign_timestamp_parts(&mut frame.roles, &mut frame.info.roles, &basis.axis_derived);
        assign_timestamp_parts(&mut frame.roles, &mut frame.info.roles, &basis.sub_period);
        sync_card_roles(&mut columns, &table, &frame);
        let family: BTreeSet<String> = basis
            .calendar_family()
            .into_iter()
            .map(|i| table.names[i].clone())
            .collect();
        if family.len() > basis.axis_cols.len() {
            // A calendar part's "behaves like a category" card
            // statement is superseded by its role.
            findings.retain(|f| {
                !((f.evidence
                    .iter()
                    .any(|e| e.method.detector == "functional_dependencies")
                    || f.finding_type.as_deref() == Some("categorical_role"))
                    && f.subject.columns.iter().all(|c| family.contains(c)))
            });
        }
        for i in basis.not_subject() {
            if !scaffold_excluded.contains(&i) {
                scaffold_excluded.push(i);
            }
        }
        scaffold_excluded.sort_unstable();
        exclusion_note = scaffold_note(
            &table,
            &frame,
            &info,
            &scaffold_excluded,
            &axis_excluded,
            &basis,
        );
        // Cells over the axis and the columns it determines restate
        // the calendar. A sub-period is not determined by the axis
        // (an hour beside a date), so its cells with a derived flag
        // (hour x working day) are real concentrations and stay in.
        // A native date or datetime axis lends its calendar parts to
        // the models as features (hour of day, day of week, month of
        // year), so a measure driven by the hour or the weekday can
        // name it as a driver.
        if let [axis] = basis.axis_cols[..] {
            if matches!(
                table.cols[axis],
                onceover_core::TypedCol::Date { .. } | onceover_core::TypedCol::Datetime { .. }
            ) {
                tier.encode.calendar_axis = Some(axis);
            }
        }
        if !basis.axis_derived.is_empty() {
            let mut related = basis.axis_cols.clone();
            related.extend(basis.axis_derived.iter().map(|d| d.col));
            tier.hotspots.related_columns = vec![related];
        }
    }

    // ---------- per-period coverage ----------
    // Observed periods on the frame's axis whose volume falls far
    // below the typical period's. Runs on the account's grain (set
    // on the frame above) for its entity axis; a frame with no
    // time axis records the pass as skipped with the reason.
    if check_deadline(&mut coverage, "panel_coverage") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("panel_coverage") {
        let pc = crate::panel::panel_coverage(&table, &frame, cfg.hints.as_ref());
        let info = match pc.status {
            crate::panel::PanelCoverageStatus::Ran(d) => PassInfo::ran(d),
            crate::panel::PanelCoverageStatus::Capped(d) => PassInfo::capped(d),
            crate::panel::PanelCoverageStatus::Skipped(d) => PassInfo::skipped(d),
        };
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "panel_coverage".into(),
            info,
        );
        findings.extend(pc.findings);
    }

    // ---------- duplicate encodings ----------
    if check_deadline(&mut coverage, "duplicate_encodings") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("duplicate_encodings") {
        let ((f, cov), stopped) =
            onceover_deps::profile_duplicate_encodings_stopped(&table, &info, &tier.dupe);
        if stopped {
            deadline_hit = true;
        }
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "duplicate_encodings".into(),
            if stopped {
                PassInfo::capped(cov)
            } else {
                PassInfo::ran(cov)
            },
        );
        findings.extend(f);
    }

    // ---------- derived-column basis, phase C (issue #374) ----------
    // A verified one-to-one recoding is stated once, by the
    // duplicate-encoding finding (the ledger groups the dependency
    // statements between the two sides under it); the redundant side
    // is no later pass's subject or input.
    // Encodings and rollups among calendar parts (month rolls up to
    // season) restate the calendar.
    {
        let family: BTreeSet<String> = basis
            .calendar_family()
            .into_iter()
            .map(|i| table.names[i].clone())
            .collect();
        if family.len() > basis.axis_cols.len() {
            findings.retain(|f| {
                !(f.finding_type.as_deref() == Some("duplicate_encoding")
                    && f.subject.columns.iter().all(|c| family.contains(c)))
            });
        }
    }
    basis.recodings = crate::basis::recodings(&table, &findings);
    if !basis.recodings.is_empty() {
        let named: Vec<String> = basis
            .recodings
            .iter()
            .map(|(d, k)| format!("{} (recoding of {})", table.names[*d], table.names[*k]))
            .collect();
        if let Some(entry) = coverage.get_mut("duplicate_encodings") {
            entry.detail.push_str(&format!(
                "; excluded from later passes as redundant: {}",
                named.join(", ")
            ));
        }
        for i in basis.not_subject() {
            if !scaffold_excluded.contains(&i) {
                scaffold_excluded.push(i);
            }
        }
        scaffold_excluded.sort_unstable();
        tier.encode.skip_columns = basis.not_input();
        exclusion_note = scaffold_note(
            &table,
            &frame,
            &info,
            &scaffold_excluded,
            &axis_excluded,
            &basis,
        );
    }

    // Derived calendar columns and a sub-period are no pass's
    // subject but remain inputs: predictors, groupings, slice and
    // contrast variables (an hour or a working-day flag explains a
    // measure).
    let calendar_inputs: Vec<usize> = basis
        .axis_derived
        .iter()
        .chain(basis.sub_period.iter())
        .map(|d| d.col)
        .collect();
    let grouping_excluded: Vec<usize> = scaffold_excluded
        .iter()
        .copied()
        .filter(|i| !calendar_inputs.contains(i))
        .collect();
    // Along-axis series passes scan measures only: a flag or a
    // category code changes level whenever its value changes, so a
    // "level shift" or "spike" in one restates its values.
    let measure_excluded: Vec<usize> = {
        let mut v = scaffold_excluded.clone();
        for (i, r) in frame.roles.iter().enumerate() {
            if matches!(r.as_deref(), Some("flag" | "dimension")) && !v.contains(&i) {
                v.push(i);
            }
        }
        v.sort_unstable();
        v
    };

    // ---------- pivot (long format) ----------
    let mut pivot_spec: Option<onceover_deps::PivotSpec> = None;
    if enabled("pivot") {
        let p = onceover_deps::analyze_pivot(
            &table,
            &info,
            &onceover_deps::PivotConfig {
                max_levels: tier.pivot_max_levels,
                max_findings: tier.pivot_max_findings,
                ..Default::default()
            },
        );
        if p.checked == 0 {
            stamp_insert(&mut coverage, &mut last_mark, timing_on,
                "pivot".into(),
                PassInfo::na(
                    "needs exactly one continuous measure column and a categorical attribute candidate"
                        .to_string(),
                ),
            );
        } else {
            stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                "pivot".into(),
                PassInfo::ran(format!(
                    "{} attribute candidates checked, {} verified jointly unique",
                    p.checked, p.verified
                )),
            );
        }
        pivot_spec = p.specs.first().cloned();
        findings.extend(p.findings);
        // A functional dependency from a verified long-format
        // frame's measure to its attribute (value -> indicator)
        // holds only because the levels occupy disjoint value
        // ranges; it restates the layout rather than discovering a
        // constraint. Declare its demotion here, where the pivot
        // verification is in scope; the ledger applies it.
        if !p.reported.is_empty() {
            for f in &mut findings {
                // Three statements restate the layout rather than the
                // data: the measure -> attribute functional dependency
                // (disjoint level ranges), the attribute predicted
                // from the pooled measure, and the pooled measure
                // predicted with the attribute among its inputs (one
                // model across unrelated quantities).
                let is_encoding_fd = f.category == Category::Dependencies
                    && f.subject.target.as_deref().is_some_and(|t| {
                        p.reported.iter().any(|(attr, measure)| {
                            let inputs = &f.subject.inputs;
                            match f.finding_type.as_deref() {
                                Some("functional") => {
                                    attr.as_str() == t && inputs.len() == 1 && inputs[0] == *measure
                                }
                                Some("predictive") => {
                                    (attr.as_str() == t
                                        && inputs.len() == 1
                                        && inputs[0] == *measure)
                                        || (measure.as_str() == t
                                            && inputs.iter().any(|i| i == attr))
                                }
                                _ => false,
                            }
                        })
                    });
                if is_encoding_fd {
                    for e in &mut f.evidence {
                        e.metrics
                            .insert("triviality".into(), json!("long_format_encoding"));
                        e.metrics.insert("triviality_factor".into(), json!(0.6));
                    }
                }
            }
        }
    }

    // ---------- wide-form profile of a verified long frame ----------
    // The pivot finding says the frame is long; the relationships
    // between the attribute's levels only exist in the wide form, so
    // profile that too and merge its findings (tagged `source_form`
    // in their evidence). One level of recursion: the wide form is
    // never itself re-pivoted.
    if depth == 0 {
        match &pivot_spec {
            None => {}
            Some(spec) if cfg.budget == Budget::Quick => {
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "pivot_profile".into(),
                    PassInfo::na("quick budget: the pivoted wide form is not profiled".to_string()),
                );
            }
            Some(spec) => {
                let wide_cols = spec.index.len() + spec.levels.len();
                if wide_cols > 64 || spec.n_index < 8 {
                    stamp_insert(&mut coverage, &mut last_mark, timing_on,
                        "pivot_profile".into(),
                        PassInfo::na(format!(
                            "pivoted form would be {} rows x {} columns; outside the wide-profile limits (at least 8 index tuples, at most 64 columns)",
                            spec.n_index, wide_cols
                        )),
                    );
                } else if cfg
                    .deadline
                    .is_some_and(|d| start.elapsed() > d.mul_f64(0.6))
                {
                    stamp_insert(&mut coverage, &mut last_mark, timing_on,
                        "pivot_profile".into(),
                        PassInfo::skipped(
                            "most of the deadline was spent on the long form; the wide form was not profiled"
                                .to_string(),
                        ),
                    );
                } else {
                    match build_wide_frame(df, &table, spec) {
                        Some(wide) => {
                            let mut cfg2 = cfg.clone();
                            cfg2.target = None;
                            // The recursive profile starts its own clock.
                            // Carry the remaining budget, not the original
                            // one, or a wide form can run past the caller's
                            // deadline after the long form already spent
                            // most of it.
                            cfg2.deadline = cfg.deadline.map(|d| d.saturating_sub(start.elapsed()));
                            let wide_report = profile_inner(&wide, &cfg2, extensions, depth + 1);
                            let n_wide =
                                wide_report.findings.len() + wide_report.grouped_findings.len();
                            // The wide profile is a second examination of
                            // the same frame; its pass coverage is part of
                            // what was checked, and must not disappear when
                            // only its findings are lifted. Keep it under
                            // its own names rather than overwriting the
                            // long form's coverage.
                            for (pass, info) in &wide_report.coverage {
                                coverage
                                    .entry(format!("pivot_profile/{pass}"))
                                    .or_insert_with(|| info.clone());
                            }
                            // Grouped statements are lifted too (schema
                            // 0.6): a family inside the wide profile is
                            // still a statement about the data, and the
                            // outer ledger re-derives the grouping over
                            // the merged candidates, so the wide
                            // profile's own grouping marks are cleared.
                            for mut f in wide_report
                                .findings
                                .into_iter()
                                .chain(wide_report.grouped_findings)
                            {
                                f.grouped_members.clear();
                                f.grouped_under = None;
                                for e in &mut f.evidence {
                                    e.metrics
                                        .insert("source_form".into(), json!("pivoted wide form"));
                                }
                                findings.push(f);
                            }
                            stamp_insert(&mut coverage, &mut last_mark, timing_on,
                                "pivot_profile".into(),
                                PassInfo::ran(format!(
                                    "profiled the pivoted wide form ({} index columns, {} levels of {} -> {} rows x {} columns); {} findings included, tagged source_form in their evidence; nested pass coverage under pivot_profile/<pass>",
                                    spec.index.len(),
                                    spec.levels.len(),
                                    spec.attribute,
                                    spec.n_index,
                                    wide_cols,
                                    n_wide
                                )),
                            );
                        }
                        None => {
                            stamp_insert(&mut coverage, &mut last_mark, timing_on,
                                "pivot_profile".into(),
                                PassInfo::na(
                                    "the pivoted wide form could not be materialized (a level name collides with an index column, or the measure is not numeric)"
                                        .to_string(),
                                ),
                            );
                        }
                    }
                }
            }
        }
    }

    // ---------- sortedness ----------
    if check_deadline(&mut coverage, "sortedness") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("sortedness") {
        // A derived calendar column follows the axis's order by
        // construction (a month column cycles with the dates), and a
        // redundant recoding follows its label: neither ordering is
        // a fact about how the rows were written.
        let derived: BTreeSet<String> = basis
            .not_subject()
            .into_iter()
            .map(|i| table.names[i].clone())
            .collect();
        let sorted: Vec<Finding> = sortedness(&table)
            .into_iter()
            .filter(|f| !f.subject.columns.iter().any(|c| derived.contains(c)))
            .collect();
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "sortedness".into(),
            PassInfo::ran(format!("{} columns sorted by row order", sorted.len())),
        );
        findings.extend(sorted);
    }

    // ---------- granularity ----------
    if check_deadline(&mut coverage, "granularity") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("granularity") {
        let gran = onceover_deps::granularity_changes(&table);
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "granularity".into(),
            PassInfo::ran(format!(
                "{} float columns scanned for granularity changes",
                table
                    .cols
                    .iter()
                    .filter(|c| matches!(c, onceover_core::TypedCol::F64 { .. }))
                    .count()
            )),
        );
        findings.extend(gran);
    }

    // ---------- sentinels ----------
    if check_deadline(&mut coverage, "sentinels") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("sentinels") {
        let sent =
            onceover_deps::sentinels::sentinel_values_for_columns(&table, &sentinel_candidates);
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "sentinels".into(),
            PassInfo::ran(format!(
                "{} numeric columns scanned for sentinel values",
                table
                    .cols
                    .iter()
                    .filter(|c| matches!(
                        c,
                        onceover_core::TypedCol::F64 { .. }
                            | onceover_core::TypedCol::Decimal { .. }
                            | onceover_core::TypedCol::I64 { .. }
                            | onceover_core::TypedCol::U64 { .. }
                    ))
                    .count()
            )),
        );
        findings.extend(sent);
    }

    // ---------- missingness ----------
    if check_deadline(&mut coverage, "missingness") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("missingness") {
        let ticks = crate::frame::axis_ticks_micros(&table, &frame);
        let axis_name = frame.info.axis.as_ref().map(|a| a.name.clone());
        let axis = match (&ticks, &axis_name) {
            (Some(ticks), Some(name)) => Some(onceover_mines::MissingnessAxis { name, ticks }),
            _ => None,
        };
        let indicator_cap = match cfg.budget {
            Budget::Quick => 0,
            Budget::Standard => 8,
            Budget::Deep => 16,
        };
        let (mut f, detail) = {
            // A missingness indicator explains its column's missing
            // rows once, here: the one statement of that relation.
            let explainers: Vec<usize> = scaffold_excluded
                .iter()
                .copied()
                .filter(|i| !basis.indicators.iter().any(|(f, _)| f == i))
                .collect();
            onceover_mines::profile_missingness(&table, &explainers, axis, indicator_cap)
        };
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "missingness".into(),
            PassInfo::ran(detail),
        );
        findings.append(&mut f);
    }

    // ---------- series ----------
    if check_deadline(&mut coverage, "series") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("series") {
        let ticks = crate::frame::axis_ticks_micros(&table, &frame);
        let axis_name = frame.info.axis.as_ref().map(|a| a.name.clone());
        let columns_with_role = |role: &str| -> Vec<usize> {
            frame
                .roles
                .iter()
                .enumerate()
                .filter(|(_, r)| r.as_deref() == Some(role))
                .map(|(i, _)| i)
                .collect()
        };
        match (&ticks, &axis_name) {
            (Some(ticks), Some(axis_name)) => {
                // The axis column is never profiled as a measure of
                // its own series (issue #207): the structural
                // exclusion applies here by column, not by role, so
                // a measure-role axis column is removed by name. A
                // hints.roles declaration for the column already
                // kept it out of `axis_excluded`, so a declared
                // measure axis stays measurable.
                let mut measures = columns_with_role("measure");
                let axis_measures: Vec<String> = measures
                    .iter()
                    .filter(|i| axis_excluded.contains(i))
                    .map(|&i| table.names[i].clone())
                    .collect();
                measures.retain(|i| !axis_excluded.contains(i));
                let input = onceover_mines::SeriesInput {
                    axis_name,
                    ticks,
                    measures,
                    dimensions: columns_with_role("dimension"),
                    entities: entity_columns(
                        &table,
                        &frame,
                        &card_facts,
                        axis_name,
                        declared_entity_idx,
                    ),
                };
                let (mut f, mut detail) = onceover_mines::profile_series(&table, &input);
                if !axis_measures.is_empty() {
                    detail.push_str(&format!(
                        "; the time axis column {} is not profiled as a measure \
                         (structural exclusion; a hints.roles entry for the column \
                         overrides it)",
                        axis_measures.join(", ")
                    ));
                }
                // The dates detector states the same axis's gaps with its
                // own calendar when selected.
                if enabled("date_gaps") {
                    f.retain(|finding| finding.category != Category::Gaps);
                }
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "series".into(),
                    PassInfo::ran(detail),
                );
                findings.append(&mut f);
            }
            _ => stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                "series".into(),
                PassInfo::skipped("no date or datetime axis to describe series along".to_string()),
            ),
        }
    }

    // ---------- one-way flags ----------
    // Absorbing states on the frame's entity × time grain: a flag
    // that switches on within entities and never off. The check
    // rides on the series detector's enablement — it reads the same
    // axis — and records its own coverage entry, including the
    // no-grain case, where it does not apply.
    if check_deadline(&mut coverage, "one_way_flags") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("series") {
        let ticks = crate::frame::axis_ticks_micros(&table, &frame);
        let axis_name = frame.info.axis.as_ref().map(|a| a.name.clone());
        match (&ticks, &axis_name) {
            (Some(ticks), Some(axis_name)) => {
                let entities =
                    entity_columns(&table, &frame, &card_facts, axis_name, declared_entity_idx);
                let input = onceover_mines::PanelInput {
                    axis_name,
                    ticks,
                    entities,
                    measures: frame
                        .roles
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| r.as_deref() == Some("measure"))
                        .map(|(i, _)| i)
                        .collect(),
                };
                let (mut f, detail) = onceover_mines::panel_flags(&table, &input);
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "one_way_flags".into(),
                    PassInfo::ran(detail),
                );
                findings.append(&mut f);
            }
            _ => stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                "one_way_flags".into(),
                PassInfo::skipped(
                    "no entity × time grain: the frame has no time axis with per-row ticks"
                        .to_string(),
                ),
            ),
        }
    }

    // ---------- panel dynamics ----------
    // Entry blocks, exit blocks, and first activity on the same
    // entity × time grain: when entities arrive and leave, and
    // when their activity actually starts. The check rides on the
    // series detector's enablement — it reads the same axis and
    // grain — and records its own coverage entry, including the
    // no-grain case, where it does not apply.
    if check_deadline(&mut coverage, "panel_dynamics") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("series") {
        let ticks = crate::frame::axis_ticks_micros(&table, &frame);
        let axis_name = frame.info.axis.as_ref().map(|a| a.name.clone());
        match (&ticks, &axis_name) {
            (Some(ticks), Some(axis_name)) => {
                let input = onceover_mines::PanelInput {
                    axis_name,
                    ticks,
                    entities: entity_columns(
                        &table,
                        &frame,
                        &card_facts,
                        axis_name,
                        declared_entity_idx,
                    ),
                    measures: frame
                        .roles
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| r.as_deref() == Some("measure"))
                        .map(|(i, _)| i)
                        .collect(),
                };
                let (mut f, detail) = onceover_mines::panel_dynamics(&table, &input);
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "panel_dynamics".into(),
                    PassInfo::ran(detail),
                );
                findings.append(&mut f);
            }
            _ => stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                "panel_dynamics".into(),
                PassInfo::skipped(
                    "no entity × time grain: the frame has no time axis with per-row ticks"
                        .to_string(),
                ),
            ),
        }
    }

    // ---------- panel concentration ----------
    // Where a measure's mass sits among entities on the frame's
    // entity × time grain: the top entities' share of each
    // period's total (high in level, or rising), and entities
    // whose total sits far outside the other entities' totals.
    // Like the one-way flags it rides on the series detector's
    // enablement and records its own coverage entry, including
    // the no-grain case, where it does not apply.
    if check_deadline(&mut coverage, "panel_concentration") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("series") {
        let ticks = crate::frame::axis_ticks_micros(&table, &frame);
        let axis_name = frame.info.axis.as_ref().map(|a| a.name.clone());
        match (&ticks, &axis_name) {
            (Some(ticks), Some(axis_name)) => {
                let input = onceover_mines::PanelInput {
                    axis_name,
                    ticks,
                    entities: entity_columns(
                        &table,
                        &frame,
                        &card_facts,
                        frame.info.axis.as_ref().map_or("", |a| a.name.as_str()),
                        declared_entity_idx,
                    ),
                    measures: frame
                        .roles
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| r.as_deref() == Some("measure"))
                        .map(|(i, _)| i)
                        .collect(),
                };
                let (mut f, detail) = onceover_mines::panel_concentration(&table, &input);
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "panel_concentration".into(),
                    PassInfo::ran(detail),
                );
                findings.append(&mut f);
            }
            _ => stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                "panel_concentration".into(),
                PassInfo::skipped(
                    "no entity × time grain: the frame has no time axis with per-row ticks"
                        .to_string(),
                ),
            ),
        }
    }

    // ---------- content ----------
    if check_deadline(&mut coverage, "content_anomalies") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("content_anomalies") {
        let mut f = onceover_deps::column_content(&table);
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "content_anomalies".into(),
            PassInfo::ran(format!(
                "{} columns scanned for content; {} statements",
                table.cols.len(),
                f.len()
            )),
        );
        findings.append(&mut f);
    }

    // ---------- boundaries ----------
    if check_deadline(&mut coverage, "boundary_mass") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("boundary_mass") {
        let bnd = onceover_deps::boundary_mass(&table);
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "boundary_mass".into(),
            PassInfo::ran(format!(
                "{} numeric columns scanned for boundary mass",
                table
                    .cols
                    .iter()
                    .filter(|c| matches!(
                        c,
                        onceover_core::TypedCol::F64 { .. }
                            | onceover_core::TypedCol::Decimal { .. }
                            | onceover_core::TypedCol::I64 { .. }
                            | onceover_core::TypedCol::U64 { .. }
                    ))
                    .count()
            )),
        );
        findings.extend(bnd);
    }

    // ---------- order constraints ----------
    if check_deadline(&mut coverage, "order_constraints") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("order_constraints") {
        if tier.dep_pair_cap == 0 {
            stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                "order_constraints".into(),
                PassInfo::skipped(
                    "quick tier: the shared pair budget (max_pairs) is 0 at this tier",
                ),
            );
        } else {
            // Pairs between which an exact single-column FD holds
            // in either direction in this run are derived from one
            // another; the pass ranks their order findings below
            // unrelated pairs and says so in the evidence.
            let derived_pairs: std::collections::BTreeSet<(usize, usize)> = deps
                .exact_fds
                .iter()
                .filter(|(lhs, _)| lhs.len() == 1)
                .map(|(lhs, rhs)| (lhs[0].min(*rhs), lhs[0].max(*rhs)))
                .collect();
            // The pass's own role-based exclusions, plus the
            // structural time-axis exclusion (issue #207): pairs
            // involving the axis column are not order statements
            // about the data.
            let mut oc_excluded: BTreeSet<usize> = frame
                .roles
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    matches!(
                        r.as_deref(),
                        Some("row_number" | "identifier" | "timestamp_part")
                    )
                })
                .map(|(i, _)| i)
                .collect();
            let oc_axis: Vec<String> = axis_excluded
                .iter()
                .filter(|i| !oc_excluded.contains(i))
                .map(|&i| table.names[i].clone())
                .collect();
            oc_excluded.extend(axis_excluded.iter().copied());
            let out = onceover_deps::order_constraints(
                &table,
                &onceover_deps::OrderConfig {
                    max_pairs: tier.dep_pair_cap,
                    derived_pairs,
                    excluded: oc_excluded,
                },
            );
            let mut detail = format!(
                "{} of {} eligible column pairs screened; {} of {} that stayed near-universal verified on all rows on the shared max_pairs budget; {} reportable order directions, {} findings",
                out.pairs_screened,
                out.pairs_total,
                out.pairs_evaluated,
                out.pairs_surviving,
                out.pairs_passing,
                out.findings.len(),
            );
            if out.directions_unstated > 0 {
                detail.push_str(&format!(
                    "; {} reportable order {} not stated: no stated basis (not functionally related, not a temporal pair, no component relation, rank correlation below 0.5 on the screening sample)",
                    out.directions_unstated,
                    if out.directions_unstated == 1 {
                        "direction"
                    } else {
                        "directions"
                    },
                ));
            }
            if !oc_axis.is_empty() {
                detail.push_str(&format!(
                    "; pairs involving the time axis column {} excluded \
                     (structural; a hints.roles entry for the column overrides it)",
                    oc_axis.join(", ")
                ));
            }
            let info = if out.pairs_screened < out.pairs_total
                || out.pairs_evaluated < out.pairs_surviving
            {
                PassInfo::capped(detail)
            } else if out.screened {
                PassInfo::sampled(format!(
                    "{detail}; pairs screened on a deterministic row sample and verified on all rows"
                ))
            } else {
                PassInfo::ran(detail)
            };
            stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                "order_constraints".into(),
                info,
            );
            findings.extend(out.findings);
        }
    }

    // ---------- digits ----------
    if check_deadline(&mut coverage, "digit_fingerprints") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("digit_fingerprints") {
        let (dig, cov) = onceover_deps::digit_fingerprints(&table);
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "digit_fingerprints".into(),
            PassInfo::ran(cov),
        );
        findings.extend(dig);
    }

    // ---------- dates ----------
    if check_deadline(&mut coverage, "date_gaps") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("date_gaps") {
        // Columns with a declared gap check are skipped here: the
        // expectations pass reports their gaps against the declared
        // reference instead of the inferred one.
        let declared_skip = cfg
            .hints
            .as_ref()
            .map(|h| onceover_mines::expectations::declared_temporal_columns(&table, h))
            .unwrap_or_default();
        let d = analyze_temporal(ttable, &declared_skip);
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "date_gaps".into(),
            PassInfo::ran(d.coverage.clone()),
        );
        // Backfill modal intervals into cards.
        for (name, modal) in d.modal_intervals {
            if let Some(card) = columns.get_mut(&name) {
                if let Some(ts) = card.temporal_stats.as_mut() {
                    ts.modal_interval_days = Some(modal);
                }
            }
        }
        findings.extend(d.findings);
    }

    // ---------- declared expectations ----------
    if check_deadline(&mut coverage, "expectations") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("expectations") {
        match cfg.hints.as_ref() {
            Some(hints) if !hints.is_empty() => {
                let e = onceover_mines::expectations::analyze_expectations(&table, hints);
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "expectations".into(),
                    PassInfo::ran(e.coverage.clone()),
                );
                findings.extend(e.findings);
            }
            _ => {
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "expectations".into(),
                    PassInfo::skipped("no hints provided".to_string()),
                );
            }
        }
    }

    // ---------- declared group contrast ----------
    // One declared subset of the rows against the rest, column by
    // column. Hint-gated like expectations: without a declared
    // contrast the pass records itself skipped, and a declaration
    // that cannot name two populations (unknown column, empty
    // group, whole-table group) is a coverage entry, not an error.
    if check_deadline(&mut coverage, "group_contrast") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("group_contrast") {
        match cfg.hints.as_ref().and_then(|h| h.contrast.as_ref()) {
            Some(spec) => {
                let out = onceover_mines::group_contrast(&table, spec);
                match out.skipped {
                    Some(reason) => {
                        stamp_insert(
                            &mut coverage,
                            &mut last_mark,
                            timing_on,
                            "group_contrast".into(),
                            PassInfo::skipped(reason),
                        );
                    }
                    None => {
                        stamp_insert(
                            &mut coverage,
                            &mut last_mark,
                            timing_on,
                            "group_contrast".into(),
                            PassInfo::ran(out.detail),
                        );
                        findings.extend(out.findings);
                    }
                }
            }
            None => {
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "group_contrast".into(),
                    PassInfo::skipped("no group contrast declared".to_string()),
                );
            }
        }
    }

    // ---------- per-level contrasts ----------
    // The automatic counterpart of the declared group contrast:
    // every level of every low-cardinality categorical against
    // the pooled rest on every numeric measure (location,
    // scale, constant multiple), plus the calendar parts of
    // the frame's date axis as derived groupings. Exact
    // quantile work; runs at every budget like the other
    // exact scans.
    if check_deadline(&mut coverage, "level_contrasts") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("level_contrasts") {
        let axis_ticks = crate::frame::axis_ticks_micros(&table, &frame);
        let axis_name = frame.info.axis.as_ref().map(|a| a.name.as_str());
        // A designated numeric target's per-level contrasts are
        // the outcome screen's (issue #310); the level contrasts
        // pass does not restate them in its own vocabulary.
        let target_exclusion: Vec<&str> = match &cfg.target {
            Some(target)
                if table
                    .names
                    .iter()
                    .position(|n| n == target)
                    .is_some_and(|i| table.cols[i].is_numeric()) =>
            {
                vec![target.as_str()]
            }
            _ => Vec::new(),
        };
        let axis_derived: Vec<usize> = basis.axis_derived.iter().map(|d| d.col).collect();
        let level_excluded = &grouping_excluded;
        let input = onceover_mines::LevelContrastInput {
            axis_name,
            axis_ticks: axis_ticks.as_deref(),
            exclude_measures: &target_exclusion,
            exclude_columns: level_excluded,
            non_measures: &measure_excluded,
            axis_derived: &axis_derived,
        };
        let out = onceover_mines::profile_level_contrasts(&table, &input);
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "level_contrasts".into(),
            PassInfo::ran(out.detail),
        );
        findings.extend(out.findings);
    }

    // What every model-based pass shares with the predictability
    // screen (issue #446): the inputs no model may use, and the exact
    // identity groups, filled in once the identity scans have run.
    let mut model_basis = onceover_mines::ModelBasis {
        // The time axis itself stays an input: a level by time
        // interaction is a group's drift.
        excluded_inputs: scaffold_excluded
            .iter()
            .copied()
            .filter(|i| !calendar_inputs.contains(i) && !basis.axis_cols.contains(i))
            .collect(),
        identity_groups: Vec::new(),
    };

    // ---------- model-based passes (standard / deep) ----------
    if cfg.budget != Budget::Quick {
        if check_deadline(&mut coverage, "predictability_screen") {
            deadline_hit = true;
        }
        if !deadline_hit {
            let (enc, enc_cov) = encode(&table, &tier.encode);
            stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                "encoding".into(),
                PassInfo::sampled(enc_cov),
            );

            let mut identity_groups: Vec<Vec<usize>> = Vec::new();
            // Pairwise dependence (supporting role) + identities,
            // ahead of the predictability screen (issue #374): a
            // column an exact identity ties to a target (a part of a
            // total, the total of a part) is withheld from that
            // target's predictors, so the screen states what
            // explains the target rather than the arithmetic.
            // The screen polls the deadline inside its own loops,
            // so it is exempt from the dispatch skip, like the
            // discontinuities and hotspots passes: dispatched even
            // when the deadline has already expired, its first
            // poll stops it, and its coverage entry discloses the
            // stop (0 pairs screened). The between-pass check
            // still runs when the screen is not dispatched, so
            // the skip record is unchanged there.
            let dep_dispatched = tier.dep_pair_cap > 0 && enabled("numeric_dependence");
            if !dep_dispatched && check_deadline(&mut coverage, "dependence_screen") {
                deadline_hit = true;
            }
            if dep_dispatched {
                let dep_out = onceover_mines::dep::screen_sampled(
                    &table,
                    tier.dep_pair_cap,
                    tier.encode.row_cap,
                    &scaffold_excluded,
                    pass_deadline,
                );
                if dep_out.deadline_stopped {
                    deadline_hit = true;
                }
                let (deps_pair, dep_findings, dep_cov) =
                    (dep_out.deps, dep_out.findings, dep_out.coverage);
                association_pairs.extend(dep_out.records);
                if enabled("row_consistency") {
                    let measures: Vec<usize> = frame
                        .roles
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| r.as_deref() == Some("measure"))
                        .map(|(i, _)| i)
                        .collect();
                    let (mut f, detail) = onceover_mines::profile_row_consistency(
                        &table,
                        &deps_pair,
                        &scaffold_excluded,
                        &measures,
                    );
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "row_consistency".into(),
                        PassInfo::ran(detail),
                    );
                    findings.append(&mut f);
                }
                let numeric = table
                    .cols
                    .iter()
                    .filter(|col| {
                        matches!(
                            col,
                            onceover_core::TypedCol::I64 { .. }
                                | onceover_core::TypedCol::U64 { .. }
                                | onceover_core::TypedCol::F64 { .. }
                                | onceover_core::TypedCol::Decimal { .. }
                        )
                    })
                    .count();
                let possible_pairs = numeric * numeric.saturating_sub(1) / 2;
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "dependence_screen".into(),
                    if dep_out.deadline_stopped || deps_pair.len() < possible_pairs {
                        PassInfo::capped(dep_cov)
                    } else {
                        if table.n > tier.encode.row_cap {
                            PassInfo::sampled(dep_cov)
                        } else {
                            PassInfo::ran(dep_cov)
                        }
                    },
                );
                findings.extend(dep_findings);

                if tier.ident_pair_cap > 0 && enabled("identities") {
                    let (id_f, id_cov) =
                        identities(&table, &deps_pair, tier.ident_pair_cap, &scaffold_excluded);
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "arithmetic_identities".into(),
                        PassInfo::ran(format!("{id_cov}{exclusion_note}")),
                    );
                    identity_groups.extend(identity_columns(&table, &id_f));
                    findings.extend(id_f);
                }
                if enabled("period_associations") {
                    let out = onceover_mines::period_associations(
                        ttable,
                        &deps_pair,
                        tier.encode.row_cap,
                    );
                    let info = match out.status {
                        onceover_mines::PeriodsStatus::NotApplicable(d) => PassInfo::na(d),
                        onceover_mines::PeriodsStatus::Ran(d) => PassInfo::ran(d),
                        onceover_mines::PeriodsStatus::Capped(d) => PassInfo::capped(d),
                    };
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "period_associations".into(),
                        info,
                    );
                    findings.extend(out.findings);
                }

                if tier.ident_multiterm_cap > 0 && enabled("identities") {
                    let (mt_f, mt_cov) = onceover_mines::multiterm_identities(
                        &table,
                        tier.ident_multiterm_cap,
                        &scaffold_excluded,
                    );
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "multiterm_identities".into(),
                        PassInfo::ran(format!("{mt_cov}{exclusion_note}")),
                    );
                    identity_groups.extend(identity_columns(&table, &mt_f));
                    findings.extend(mt_f);
                }
            } else if deadline_hit {
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "dependence_screen".into(),
                    PassInfo::skipped("deadline reached"),
                );
            }

            merge_groups(&mut identity_groups);
            model_basis.identity_groups = identity_groups.clone();
            let mut screen = if enabled("predictability") {
                screen_predictability(
                    &table,
                    &enc,
                    &tier.screen,
                    &scaffold_excluded,
                    &calendar_inputs,
                    &recorded_after_idx,
                    &identity_groups,
                )
            } else {
                onceover_mines::ScreenOutcome {
                    findings: Vec::new(),
                    results: Vec::new(),
                    coverage: "not selected".into(),
                }
            };
            stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                "predictability_screen".into(),
                PassInfo::sampled(screen.coverage.clone()),
            );
            findings.extend(std::mem::take(&mut screen.findings));

            // Spend target budgets on columns with a predictive signal,
            // rather than letting near-majority baselines crowd them out.
            let mut cat_targets: Vec<&onceover_mines::TargetResult> = screen
                .results
                .iter()
                .filter(|r| {
                    r.is_categorical && r.metric >= r.baseline + tier.dtable.min_baseline_gain
                })
                .collect();
            cat_targets.sort_by(|a, b| b.metric.total_cmp(&a.metric).then(a.name.cmp(&b.name)));
            let dtable_eligible = cat_targets.len();
            cat_targets.truncate(if enabled("decision_tables") {
                tier.dtable_top_targets
            } else {
                0
            });
            if !cat_targets.is_empty() {
                let (f, cov) = decision_tables(&enc, &cat_targets, &tier.dtable);
                let detail = format!(
                    "{cov}; {} of {dtable_eligible} eligible targets selected (cap {})",
                    cat_targets.len(),
                    tier.dtable_top_targets
                );
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "decision_tables".into(),
                    if cat_targets.len() < dtable_eligible {
                        PassInfo::capped(detail)
                    } else {
                        PassInfo::sampled(detail)
                    },
                );
                findings.extend(f);
            } else if enabled("decision_tables") {
                // When the pass is not enabled, no entry is stamped
                // here; the closing coverage loop records the
                // on-request skip for it instead.
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "decision_tables".into(),
                    PassInfo::na("no categorical targets with predictive signal above baseline"),
                );
            }

            // PRISM rules: top targets by accuracy.
            let mut prism_targets: Vec<&onceover_mines::TargetResult> = screen
                .results
                .iter()
                .filter(|r| {
                    r.is_categorical
                        && r.metric >= tier.prism.min_target_accuracy
                        && r.baseline < 0.9
                        && r.cat_ids
                            .as_ref()
                            .is_some_and(|(_, labels)| labels.len() <= tier.prism.max_classes)
                })
                .collect();
            prism_targets.sort_by(|a, b| b.metric.total_cmp(&a.metric).then(a.name.cmp(&b.name)));
            let prism_eligible = prism_targets.len();
            prism_targets.truncate(if enabled("covering_rules") {
                tier.prism_top_targets
            } else {
                0
            });
            let (f, cov) = prism_rules(&enc, &prism_targets, &tier.prism);
            let detail = format!(
                "{cov}; {} of {prism_eligible} eligible targets selected (cap {})",
                prism_targets.len(),
                tier.prism_top_targets
            );
            stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                "prism_rules".into(),
                if prism_targets.len() < prism_eligible {
                    PassInfo::capped(detail)
                } else {
                    PassInfo::sampled(detail)
                },
            );
            findings.extend(f);

            if check_deadline(&mut coverage, "regimes") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("regimes") {
                // Target exclusions come from the frame model:
                // scaffolding columns are not regime targets.
                let excluded: &[usize] = &scaffold_excluded;
                let (f, cov) =
                    profile_regimes(&table, &enc, &screen.results, excluded, &tier.regimes);
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "regimes".into(),
                    PassInfo::sampled(format!("{cov}{exclusion_note}")),
                );
                findings.extend(f);
            }
            if check_deadline(&mut coverage, "interactions") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("interactions") {
                let (f, cov) = profile_interactions(
                    &table,
                    &enc,
                    &screen.results,
                    &tier.screen,
                    &tier.hstat,
                    &model_basis,
                    cfg.target
                        .as_deref()
                        .and_then(|t| table.names.iter().position(|n| n == t)),
                );
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "interactions".into(),
                    PassInfo::sampled(cov),
                );
                findings.extend(f);
            }
            if check_deadline(&mut coverage, "breaks") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("breaks") {
                // The frame model's axis (assembled where needed) is
                // the pass's time hint; scaffolding columns and
                // non-measures are excluded as series.
                let excluded: &[usize] = &measure_excluded;
                if !axis_explicit("breaks") && !axis_gate_open {
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "breaks".into(),
                        PassInfo::skipped(axis_gate_reason("break scan")),
                    );
                } else {
                    let (time_hint, hinted_only) = if axis_explicit("breaks") {
                        (
                            frame_time_hint
                                .or_else(|| cfg.hints.as_ref().and_then(|h| h.time.as_deref())),
                            false,
                        )
                    } else {
                        (default_axis_hint, true)
                    };
                    let (f, cov) =
                        profile_breaks(ttable, excluded, &tier.breaks, time_hint, hinted_only);
                    let cov = if cov.ends_with("skipped") {
                        cov
                    } else {
                        format!("{cov}{exclusion_note}")
                    };
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "breaks".into(),
                        PassInfo::sampled(cov),
                    );
                    findings.extend(f);
                }
            }
            if check_deadline(&mut coverage, "ordered_partitions") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("ordered_partitions") {
                // Non-measures may split the tree as dimensions; only
                // measures are partitioned as responses.
                let excluded: &[usize] = &grouping_excluded;
                tier.partitions.exclude_responses = measure_excluded.clone();
                if !axis_explicit("ordered_partitions") && !axis_gate_open {
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "ordered_partitions".into(),
                        PassInfo::skipped(axis_gate_reason("ordered partition scan")),
                    );
                } else {
                    let (time_hint, hinted_only) = if axis_explicit("ordered_partitions") {
                        (
                            frame_time_hint
                                .or_else(|| cfg.hints.as_ref().and_then(|h| h.time.as_deref())),
                            false,
                        )
                    } else {
                        (default_axis_hint, true)
                    };
                    let out = onceover_mines::ordered_partitions(
                        ttable,
                        excluded,
                        &tier.partitions,
                        time_hint,
                        hinted_only,
                    );
                    let info2 = match out.status {
                        onceover_mines::PartitionsStatus::NotApplicable(d) => PassInfo::na(d),
                        onceover_mines::PartitionsStatus::Ran(d) => {
                            PassInfo::ran(format!("{d}{exclusion_note}"))
                        }
                        onceover_mines::PartitionsStatus::Capped(d) => {
                            PassInfo::capped(format!("{d}{exclusion_note}"))
                        }
                    };
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "ordered_partitions".into(),
                        info2,
                    );
                    findings.extend(out.findings);
                }
            }
            if check_deadline(&mut coverage, "distribution_change") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("distribution_change") {
                // The frame model's axis and measure roles, as for
                // the series pass: the segmentation runs over the
                // measures jointly, and the axis column itself is
                // never a measure.
                let ticks = crate::frame::axis_ticks_micros(&table, &frame);
                let axis_name = frame.info.axis.as_ref().map(|a| a.name.clone());
                match (ticks, axis_name) {
                    (Some(ticks), Some(axis_name)) => {
                        let measures: Vec<usize> = frame
                            .roles
                            .iter()
                            .enumerate()
                            .filter(|(_, r)| r.as_deref() == Some("measure"))
                            .map(|(i, _)| i)
                            .filter(|i| !axis_excluded.contains(i))
                            .collect();
                        let input = onceover_mines::ChangeforestInput {
                            axis_name,
                            ticks,
                            measures,
                            axis_is_datetime: frame.axis_is_datetime,
                        };
                        let out =
                            onceover_mines::changeforest_pass(&table, &input, &tier.changeforest);
                        let info = match out.status {
                            onceover_mines::ChangeforestStatus::NotApplicable(d) => PassInfo::na(d),
                            onceover_mines::ChangeforestStatus::Ran(d) => {
                                PassInfo::ran(format!("{d}{exclusion_note}"))
                            }
                            onceover_mines::ChangeforestStatus::Capped(d) => {
                                PassInfo::capped(format!("{d}{exclusion_note}"))
                            }
                        };
                        stamp_insert(
                            &mut coverage,
                            &mut last_mark,
                            timing_on,
                            "distribution_change".into(),
                            info,
                        );
                        findings.extend(out.findings);
                    }
                    _ => stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "distribution_change".into(),
                        PassInfo::na("no date or datetime axis to segment along"),
                    ),
                }
            }
            if check_deadline(&mut coverage, "ordered_axis") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("ordered_axis") {
                // The ordered-axis battery (#273, #277) runs after
                // the breaks, ordered-partition, and changeforest
                // passes so their statements are in `findings`:
                // battery clusters inside an existing statement's
                // range attach as corroboration instead of
                // restating the event. Panel grain keys are
                // excluded from its slice family (the panel passes
                // own entity statements).
                // The account's resolved values are what the battery
                // sees: a refused axis or level order is not applied.
                let axis_hint = declared_axis;
                let ordinals = &effective_level_order;
                if !axis_explicit("ordered_axis") && !axis_gate_open {
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "ordered_axis".into(),
                        PassInfo::skipped(axis_gate_reason("ordered-axis scan")),
                    );
                } else {
                    let entity_names: Vec<String> = {
                        let axis_name = frame
                            .info
                            .axis
                            .as_ref()
                            .map(|a| a.name.clone())
                            .unwrap_or_default();
                        entity_columns(&table, &frame, &card_facts, &axis_name, declared_entity_idx)
                            .iter()
                            .map(|&i| table.names[i].clone())
                            .collect()
                    };
                    let (time_arg, hinted_only) = if axis_explicit("ordered_axis") {
                        (
                            frame_time_hint
                                .or_else(|| cfg.hints.as_ref().and_then(|h| h.time.as_deref())),
                            false,
                        )
                    } else {
                        (default_axis_hint, true)
                    };
                    let out = onceover_mines::profile_ordered_axis(
                        ttable,
                        &scaffold_excluded,
                        &tier.axis_battery,
                        axis_hint,
                        time_arg,
                        ordinals,
                        &entity_names,
                        &findings,
                        hinted_only,
                    );
                    let info = match out.status {
                        onceover_mines::AxisBatteryStatus::NotApplicable(d) => PassInfo::na(d),
                        onceover_mines::AxisBatteryStatus::Ran(d) => {
                            PassInfo::ran(format!("{d}{exclusion_note}"))
                        }
                        onceover_mines::AxisBatteryStatus::Capped(d) => {
                            PassInfo::capped(format!("{d}{exclusion_note}"))
                        }
                    };
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "ordered_axis".into(),
                        info,
                    );
                    findings.extend(out.findings);
                }
            }
            if check_deadline(&mut coverage, "outliers") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("outliers") {
                let excluded: &[usize] = &scaffold_excluded;
                let (f, cov, stopped) = profile_surprises(&table, excluded, &tier.surprises);
                if stopped {
                    deadline_hit = true;
                }
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "outliers".into(),
                    if stopped {
                        PassInfo::capped(format!("{cov}{exclusion_note}"))
                    } else {
                        PassInfo::ran(format!("{cov}{exclusion_note}"))
                    },
                );
                findings.extend(f);
            }
            if check_deadline(&mut coverage, "slice_shifts") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("slice_shifts") {
                let excluded: &[usize] = &scaffold_excluded;
                let out = onceover_mines::profile_slice_shifts(&table, excluded, &tier.slices);
                let info = match out.status {
                    onceover_mines::SliceStatus::NotApplicable(d) => PassInfo::na(d),
                    onceover_mines::SliceStatus::Ran(d) => {
                        PassInfo::ran(format!("{d}{exclusion_note}"))
                    }
                    onceover_mines::SliceStatus::Capped(d) => {
                        PassInfo::capped(format!("{d}{exclusion_note}"))
                    }
                };
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "slice_shifts".into(),
                    info,
                );
                findings.extend(out.findings);
            }
            // Discontinuities polls the deadline inside its own
            // loops, so it is exempt from the dispatch skip: it is
            // dispatched even when the deadline has already
            // expired, its first poll stops it, and its coverage
            // entry discloses the stop (0 targets screened)
            // instead of the pass vanishing from the report. The
            // between-pass check still runs when the pass is not
            // dispatched, so the skip record is unchanged there.
            if !enabled("discontinuities") && check_deadline(&mut coverage, "discontinuities") {
                deadline_hit = true;
            }
            if enabled("discontinuities") {
                // The screen's targets and axes both come from its
                // numeric pool, so the frame-role exclusions remove
                // scaffolding from both roles at once. Semantic
                // identifier never fires on float columns, so unique
                // real-valued measurements stay eligible — the
                // behaviour the old card-role carve-out protected.
                let excluded: &[usize] = &scaffold_excluded;
                // Jumps in a flag or a label say little; targets are measures.
                let measures: Vec<usize> = frame
                    .roles
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.as_deref() == Some("measure"))
                    .map(|(i, _)| i)
                    .collect();
                if !axis_explicit("discontinuities")
                    && disc_axes.is_empty()
                    && !tier.disc.deadline.exceeded()
                {
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "discontinuities".into(),
                        PassInfo::skipped(axis_gate_reason("discontinuity screen")),
                    );
                } else {
                    let axes_arg =
                        if axis_explicit("discontinuities") || tier.disc.deadline.exceeded() {
                            None
                        } else {
                            Some(disc_axes.as_slice())
                        };
                    let out = profile_discontinuities(
                        &table,
                        excluded,
                        Some(&measures),
                        axes_arg,
                        &tier.disc,
                    );
                    if out.deadline_stopped {
                        deadline_hit = true;
                    }
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "discontinuities".into(),
                        if out.deadline_stopped {
                            PassInfo::capped(format!("{}{exclusion_note}", out.coverage))
                        } else {
                            PassInfo::ran(format!("{}{exclusion_note}", out.coverage))
                        },
                    );
                    findings.extend(out.findings);
                }
            }
            if check_deadline(&mut coverage, "drift") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("drift") {
                // The time column is the declared hint when given,
                // else the frame model's assembled axis (the pass
                // receives the materialized table, so an assembled
                // axis name resolves). The encoding stays the
                // original table's: the axis column is excluded from
                // targets and predictors by index inside the pass,
                // and every other column is unchanged by the
                // materialization.
                let excluded: &[usize] = &scaffold_excluded;
                let time_column = cfg
                    .hints
                    .as_ref()
                    .and_then(|h| h.time.as_deref())
                    .or(frame_time_hint);
                let (f, cov) = profile_drift(
                    ttable,
                    &enc,
                    time_column,
                    excluded,
                    &tier.screen,
                    &tier.drift,
                    &model_basis,
                );
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "drift".into(),
                    PassInfo::ran(format!("{cov}{exclusion_note}")),
                );
                findings.extend(f);
            }
            if deadline_hit || check_deadline(&mut coverage, "association_rules") {
                deadline_hit = true;
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "association_rules".into(),
                    PassInfo::skipped("deadline reached"),
                );
            } else if enabled("association_rules") {
                let (f, cov) = association_rules(&enc, &tier.association, &scaffold_excluded);
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "association_rules".into(),
                    PassInfo::sampled(format!("{cov}{exclusion_note}")),
                );
                findings.extend(f);
                let (pf, pair_records, pcov) = onceover_mines::pair_associations(
                    &enc,
                    &table,
                    &tier.association,
                    &scaffold_excluded,
                );
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "association_pairs".into(),
                    PassInfo::sampled(format!("{pcov}{exclusion_note}")),
                );
                findings.extend(pf);
                association_pairs.extend(pair_records);
            }

            // Hotspots polls the deadline inside its own scan,
            // so it is exempt from the dispatch skip, like
            // discontinuities above: dispatched even when the
            // deadline has already expired, its first poll stops
            // it, and its coverage entry discloses the stop.
            if enabled("hotspots") {
                let out = hotspots(&enc, &tier.hotspots, &weight_series);
                if out.deadline_stopped {
                    deadline_hit = true;
                }
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "hotspots".into(),
                    if out.deadline_stopped {
                        PassInfo::capped(format!("{}{weight_note}", out.coverage))
                    } else {
                        PassInfo::sampled(format!("{}{weight_note}", out.coverage))
                    },
                );
                findings.extend(out.findings);
            } else if deadline_hit || check_deadline(&mut coverage, "hotspots") {
                deadline_hit = true;
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "hotspots".into(),
                    PassInfo::skipped("deadline reached"),
                );
            }

            // Greedy view/cluster pass (standard / deep only).
            if check_deadline(&mut coverage, "row_clusters") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("row_clusters") {
                let (vf, vout) = onceover_mines::views_pass(&table, &tier.views);
                // Dropped columns must be visible in coverage, not
                // only in finding metrics: with no views findings at
                // all, the exclusion would otherwise be silent. A
                // budget-truncated run reports as capped, like the
                // other truncated passes, not as a complete run.
                let mut detail = vout.coverage.clone();
                // Cap drops and extraction drops are stated
                // separately, each under its own wording and count:
                // extraction-dropped columns were never eligible,
                // so they were never subject to the cap, and
                // attributing them to it would overstate the cap's
                // share and misstate why they went unexamined.
                if !vout.cap_dropped_columns.is_empty() {
                    detail.push_str(&format!(
                        "; dropped {} column(s) past the {}-column cap: {}",
                        vout.cap_dropped_columns.len(),
                        tier.views.max_columns,
                        vout.cap_dropped_columns.join(", ")
                    ));
                }
                if !vout.extraction_dropped_columns.is_empty() {
                    detail.push_str(&format!(
                        "; dropped {} column(s) at extraction (unsupported type or no non-null values): {}",
                        vout.extraction_dropped_columns.len(),
                        vout.extraction_dropped_columns.join(", ")
                    ));
                }
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "row_clusters".into(),
                    if vout.capped {
                        PassInfo::capped(detail)
                    } else if table.n > tier.views.row_cap {
                        PassInfo::sampled(detail)
                    } else {
                        PassInfo::ran(detail)
                    },
                );
                findings.extend(vf);
            } else if deadline_hit {
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "row_clusters".into(),
                    PassInfo::skipped("deadline reached"),
                );
            }
            shared_encoding = Some(enc);
        } else {
            stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                "predictability_screen".into(),
                PassInfo::skipped("deadline reached"),
            );
            // The deadline had already expired when the
            // model-based block was reached, so its passes are
            // recorded skipped above — except the three
            // self-limiting passes. They poll the deadline inside
            // their own loops, so they are dispatched even now:
            // each stops at its first poll and its coverage entry
            // discloses the stop (0 examined) instead of the pass
            // vanishing from the report. Their downstream
            // consumers are not exempt and stay skipped.
            if enabled("discontinuities") {
                let excluded: &[usize] = &scaffold_excluded;
                let measures: Vec<usize> = frame
                    .roles
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.as_deref() == Some("measure"))
                    .map(|(i, _)| i)
                    .collect();
                if !axis_explicit("discontinuities")
                    && disc_axes.is_empty()
                    && !tier.disc.deadline.exceeded()
                {
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "discontinuities".into(),
                        PassInfo::skipped(axis_gate_reason("discontinuity screen")),
                    );
                } else {
                    let axes_arg =
                        if axis_explicit("discontinuities") || tier.disc.deadline.exceeded() {
                            None
                        } else {
                            Some(disc_axes.as_slice())
                        };
                    let out = profile_discontinuities(
                        &table,
                        excluded,
                        Some(&measures),
                        axes_arg,
                        &tier.disc,
                    );
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "discontinuities".into(),
                        if out.deadline_stopped {
                            PassInfo::capped(format!("{}{exclusion_note}", out.coverage))
                        } else {
                            PassInfo::ran(format!("{}{exclusion_note}", out.coverage))
                        },
                    );
                    findings.extend(out.findings);
                }
            }
            if enabled("hotspots") {
                // The scan reads the encoded table; build it for
                // this dispatch without stamping an encoding
                // entry (the block that normally produces it was
                // skipped).
                let (enc, _) = encode(&table, &tier.encode);
                let out = hotspots(&enc, &tier.hotspots, &weight_series);
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "hotspots".into(),
                    if out.deadline_stopped {
                        PassInfo::capped(format!("{}{weight_note}", out.coverage))
                    } else {
                        PassInfo::sampled(format!("{}{weight_note}", out.coverage))
                    },
                );
                findings.extend(out.findings);
            }
            if tier.dep_pair_cap > 0 && enabled("numeric_dependence") {
                let dep_out = onceover_mines::dep::screen_sampled(
                    &table,
                    tier.dep_pair_cap,
                    tier.encode.row_cap,
                    &scaffold_excluded,
                    pass_deadline,
                );
                let dep_stopped = dep_out.deadline_stopped;
                let dep_pairs = dep_out.deps.len();
                association_pairs.extend(dep_out.records);
                findings.extend(dep_out.findings);
                let numeric = table
                    .cols
                    .iter()
                    .filter(|col| {
                        matches!(
                            col,
                            onceover_core::TypedCol::I64 { .. }
                                | onceover_core::TypedCol::U64 { .. }
                                | onceover_core::TypedCol::F64 { .. }
                                | onceover_core::TypedCol::Decimal { .. }
                        )
                    })
                    .count();
                let possible_pairs = numeric * numeric.saturating_sub(1) / 2;
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "dependence_screen".into(),
                    if dep_stopped || dep_pairs < possible_pairs {
                        PassInfo::capped(dep_out.coverage)
                    } else if table.n > tier.encode.row_cap {
                        PassInfo::sampled(dep_out.coverage)
                    } else {
                        PassInfo::ran(dep_out.coverage)
                    },
                );
            }
        }
    } else {
        for pass in [
            "encoding",
            "predictability_screen",
            "decision_tables",
            "prism_rules",
            "regimes",
            "breaks",
            "ordered_partitions",
            "ordered_axis",
            "discontinuities",
            "distribution_change",
            "association_rules",
            "dependence_screen",
            "arithmetic_identities",
            "period_associations",
            "row_clusters",
            "slice_shifts",
            "drift",
        ] {
            stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                pass.into(),
                PassInfo::skipped("quick tier"),
            );
        }
    }

    for pass in ["regimes", "association_rules"] {
        coverage.entry(pass.into()).or_insert_with(|| {
            PassInfo::skipped(if deadline_hit {
                "deadline reached"
            } else {
                "model screening unavailable"
            })
        });
    }

    for detector in extensions {
        let descriptor = detector.descriptor();
        if cfg.detectors.is_some() && !active.contains(&descriptor.id) {
            continue;
        }
        if check_deadline(&mut coverage, &descriptor.id) {
            continue;
        }
        let context = crate::DetectorContext {
            table: &table,
            columns: &columns,
            encoded: shared_encoding.as_ref(),
            config: cfg,
        };
        match detector.discover(&context) {
            Ok(mut candidates) => {
                let invalid = candidates.iter().find_map(|f| {
                    if !descriptor.categories.contains(&f.category) {
                        return Some("undeclared category".to_string());
                    }
                    if f.subject.columns.iter().any(|c| !columns.contains_key(c)) {
                        return Some("unknown subject column".to_string());
                    }
                    f.validate().err()
                });
                if let Some(reason) = invalid {
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        descriptor.id.clone(),
                        PassInfo::skipped(format!("invalid detector output: {reason}")),
                    );
                    continue;
                }
                for f in &mut candidates {
                    for e in &mut f.evidence {
                        e.method.detector = descriptor.id.clone();
                    }
                }
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    descriptor.id.clone(),
                    PassInfo::ran(format!("{} candidate statements", candidates.len())),
                );
                findings.extend(candidates);
            }
            Err(message) => {
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    descriptor.id.clone(),
                    PassInfo::skipped(format!("detector failed: {message}")),
                );
            }
        }
    }
    // ---------- the outcome account and contrasts (issue #310) ----------
    // The pointed pass's first statements: the designated
    // outcome's own distribution, then its per-level contrasts
    // with stated nulls. The declared event is resolved against
    // the target's levels here, once, and the resolution is
    // accounted for in the declarations entry.
    if check_deadline(&mut coverage, "outcome_account") {
        deadline_hit = true;
    }
    if !deadline_hit && (enabled("outcome_account") || enabled("outcome_contrasts")) {
        match &cfg.target {
            Some(target) => {
                if let Some(event) = &outcome_decl.event {
                    match onceover_mines::target_event_label(&table, target, event) {
                        Some(label) => {
                            outcome_decl.event_label = Some(label.clone());
                            decl_notes.push(format!(
                                "event: '{label}' checked — it is a level of the target '{target}'; the outcome contrasts state event rates against it"
                            ));
                        }
                        None => {
                            decl_notes.push(format!(
                                "event: {event} refused — it does not name a level of the target '{target}' (a numeric target has no levels); the outcome contrasts take the per-level form and no event rate is stated"
                            ));
                        }
                    }
                }
                outcome_decl.excluded = grouping_excluded.clone();
                for &idx in &recorded_after_idx {
                    if !outcome_decl.excluded.contains(&idx) {
                        outcome_decl.excluded.push(idx);
                    }
                }
                if enabled("outcome_account") {
                    let (f, cov) = onceover_mines::outcome_account(&table, target, &outcome_decl);
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "outcome_account".into(),
                        PassInfo::ran(cov),
                    );
                    findings.extend(f);
                }
                if enabled("outcome_contrasts") {
                    let axis_ticks = crate::frame::axis_ticks_micros(&table, &frame);
                    let axis = frame
                        .info
                        .axis
                        .as_ref()
                        .map(|a| a.name.as_str())
                        .zip(axis_ticks.as_deref());
                    let (f, cov) =
                        onceover_mines::outcome_contrasts(&table, target, &outcome_decl, axis);
                    stamp_insert(
                        &mut coverage,
                        &mut last_mark,
                        timing_on,
                        "outcome_contrasts".into(),
                        PassInfo::ran(cov),
                    );
                    findings.extend(f);
                }
            }
            None => {
                for id in ["outcome_account", "outcome_contrasts"] {
                    if enabled(id) {
                        stamp_insert(
                            &mut coverage,
                            &mut last_mark,
                            timing_on,
                            id.into(),
                            PassInfo::skipped("no target designated".to_string()),
                        );
                    }
                }
            }
        }
    }

    // ---------- target-aware shapes (opt-in via a designated target) ----------
    if check_deadline(&mut coverage, "target_shapes") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("target_shapes") {
        match &cfg.target {
            Some(target) => {
                let (f, cov) = profile_target_shapes(&table, target, &tier.shapes, &model_basis);
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "target_shapes".into(),
                    PassInfo::ran(cov),
                );
                findings.extend(f);
            }
            None => {
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "target_shapes".into(),
                    PassInfo::skipped("no target designated".to_string()),
                );
            }
        }
    }

    // ---------- leakage signatures (opt-in via a designated target) ----------
    if check_deadline(&mut coverage, "leakage") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("leakage") {
        match &cfg.target {
            Some(target) => {
                let (f, cov) = screen_leakage(&table, target, &recorded_after_names);
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "leakage".into(),
                    PassInfo::ran(cov),
                );
                findings.extend(f);
            }
            None => {
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "leakage".into(),
                    PassInfo::skipped("no target designated".to_string()),
                );
            }
        }
    }

    // ---------- model-failure slices (target-gated) ----------
    if check_deadline(&mut coverage, "model_failures") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("model_failures") {
        match &cfg.target {
            Some(target) => {
                // The cross-fit fits on the shared encoding when the
                // model-based block produced one; at the quick tier
                // (or after a deadline skip) it encodes locally,
                // the fallback the hotspots pass uses.
                let local_enc;
                let enc = match &shared_encoding {
                    Some(enc) => enc,
                    None => {
                        local_enc = encode(&table, &tier.encode).0;
                        &local_enc
                    }
                };
                let mut mf_excluded = grouping_excluded.clone();
                // A column an exact identity ties to the target is its
                // arithmetic: as a model input it makes the model
                // fail exactly where the identity's other parts are
                // large; it is neither an input nor a slice variable.
                if let Some(ti) = table.names.iter().position(|n| n == target) {
                    for c in model_basis.excluded_for(ti) {
                        if !mf_excluded.contains(&c) {
                            mf_excluded.push(c);
                        }
                    }
                }
                for &idx in &recorded_after_idx {
                    if !mf_excluded.contains(&idx) {
                        mf_excluded.push(idx);
                    }
                }
                let (f, cov) = screen_model_failures(
                    &table,
                    enc,
                    target,
                    &tier.screen,
                    &tier.modelfail,
                    &mf_excluded,
                );
                let cov = if recorded_after_names.is_empty() {
                    cov
                } else {
                    format!(
                        "{cov}; declared recorded-after column(s) excluded from the model inputs: {}",
                        recorded_after_names.join(", ")
                    )
                };
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "model_failures".into(),
                    PassInfo::ran(cov),
                );
                findings.extend(f);
            }
            None => {
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "model_failures".into(),
                    PassInfo::skipped("no target designated".to_string()),
                );
            }
        }
    }

    findings.retain(|f| f.evidence.iter().any(|e| selected(&e.method.detector)));
    for f in &mut findings {
        for evidence in &mut f.evidence {
            evidence.scope.population_rows.get_or_insert(table.n);
            evidence.scope.missing_values=match evidence.method.detector.as_str(){
                "column_profile"|"keys"=>"non-missing values counted; missingness reported separately",
                "row_duplicates"=>"missing values compare equal; opaque fields excluded",
                "functional_dependencies"|"numeric_dependence"=>"rows complete on the evaluated inputs and target",
                "predictability"|"decision_tables"|"regimes"|"interactions"=>"missing targets excluded; predictor handling follows the recorded model",
                "drift"=>"rows missing the target or the time value excluded; period comparisons use equal row counts on both sides",
                "target_shapes"=>"complete cases per predictor: rows missing the predictor or the target excluded",
                "outcome_account"=>"rows missing the outcome are excluded and counted in the statement",
                "outcome_contrasts"=>"rows missing the grouping value or the outcome are excluded from that grouping's comparison",
                "leakage"=>"complete cases per predictor: rows missing the predictor or the target excluded from that predictor's evaluation",
                "model_failures"=>"rows missing the target are excluded from fitting and from the search; rows missing a condition value are outside the slice",
                "group_contrast"=>"rows missing the group column are excluded from both populations; rows missing the contrasted column are excluded from its comparison",
                "level_contrasts"=>"rows missing the grouping value or the measure are excluded from that pair's comparison",
                "breaks"=>"rows with a missing date or value are excluded from each series",
                "distribution_change"=>"rows missing the axis value or any measure value are excluded",
                "slice_shifts"=>"rows missing a condition value are outside the slice; rows missing the target count as not extreme and not positive",
                "ordered_axis"=>"rows missing the axis or the response are excluded from each series and pair; series aggregate rows sharing an axis value by their mean",
                "boundary_mass"=>"missing values excluded from the scan; a null is never a boundary value",
                "order_constraints"=>"rows where either column of a pair is missing are excluded from that pair's comparison",
                "missingness"=>"missing values are the subject; rows missing the explaining column are excluded from its comparison",
                "association_rules"|"covering_rules"=>"see discovery metrics; normalized rule evidence uses complete predicates and outcome",
                _=>evidence.scope.missing_values.as_str(),
            }.to_string();
            if evidence.scope.sampling == Sampling::Unspecified {
                evidence.scope.sampling = match evidence.method.detector.as_str() {
                    "column_profile" | "keys" | "row_duplicates" | "sortedness" | "date_gaps"
                    | "identities" | "missingness" | "content_anomalies" | "outcome_account"
                    | "outcome_contrasts" => Sampling::Full,
                    "association_rules" | "covering_rules" => {
                        if shared_encoding.as_ref().is_some_and(|enc| enc.n < table.n) {
                            Sampling::Sample
                        } else {
                            Sampling::Full
                        }
                    }
                    _ => match evidence.scope.rows_evaluated {
                        Some(n) if n < table.n => Sampling::Sample,
                        Some(_) => Sampling::Full,
                        None => Sampling::Unspecified,
                    },
                };
            }
        }
    }
    let candidate_count = findings.len();
    let rule_candidates = findings
        .iter()
        .filter(|f| f.category == Category::ConditionalRules)
        .count();
    crate::rules::evaluate_rules(&table, &mut findings, tier.encode.row_cap, &cfg.selection);
    let rules_kept = findings
        .iter()
        .filter(|f| f.category == Category::ConditionalRules)
        .count();
    if rule_candidates > 0 {
        stamp_insert(&mut coverage, &mut last_mark, timing_on, "rule_evaluation".into(),PassInfo::sampled(format!("{rules_kept} of {rule_candidates} candidates passed a common diagnostic partition; support and baseline evaluated on complete predicates and outcome; reused for selection")));
    }
    let verified_count = findings.len();
    let distinct_count = findings
        .iter()
        .map(Finding::canonical_key)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    // Every registered detector has a coverage entry under its
    // pass name, so a pass that did not run is never silently
    // absent: the loop below records why for each one.
    let pass_of = |id: &str| -> String {
        match id {
            "column_profile" => "column_cards",
            "predictability" => "predictability_screen",
            "numeric_dependence" => "dependence_screen",
            "identities" => "arithmetic_identities",
            "covering_rules" => "prism_rules",
            other => other,
        }
        .to_string()
    };
    let registry_passes: Vec<(String, String)> = activations
        .keys()
        .map(|id| (id.clone(), pass_of(id)))
        .collect();
    for (id, pass) in &registry_passes {
        let (id, pass) = (id.as_str(), pass.as_str());
        if !enabled(id) {
            // A pass that did not run says why. Opt-in detectors are
            // named as such. A deadline check can stamp a pass before
            // its enabled check; when the detector was off, being
            // disabled is the reason it did not run, so replace that
            // misleading entry.
            let detail = match activations.get(id) {
                Some((_, crate::detectors::Trigger::Target)) => {
                    "runs when a target is designated, or when explicitly selected together with a target".to_string()
                }
                Some((crate::detectors::Membership::OnRequest, _)) => format!(
                    "on-request detector; not in the default set — request it with detectors=[\"{id}\"]"
                ),
                _ => "detector not enabled".to_string(),
            };
            match coverage.get_mut(pass) {
                Some(info) if info.status == "skipped" && info.detail == "deadline reached" => {
                    info.detail = detail;
                }
                Some(_) => {}
                None => stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    pass.into(),
                    PassInfo::skipped(detail),
                ),
            }
        } else {
            if !coverage.contains_key(pass) {
                // Enabled but not run: the tier excludes it, the
                // deadline stopped the profile before it, or the
                // block it runs in did not run.
                let detail = if cfg.budget == Budget::Quick {
                    "quick tier"
                } else if deadline_hit {
                    "deadline reached"
                } else {
                    "not run: the passes it runs after did not run on this table"
                };
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    pass.into(),
                    PassInfo::skipped(detail),
                );
            }
            if !selected(id) {
                if let Some(info) = coverage.get_mut(pass) {
                    info.detail
                        .push_str("; required as a resource; findings not selected");
                }
            }
        }
    }
    // Frame-model step 2: demote restatements (calendar derivations
    // and scaffolding-only subjects) ahead of the ledger. Demotion
    // is a ranking change declared in the findings' evidence; the
    // counts are disclosed on the frame_model entry (and repeated
    // on the selection entry when a cap also binds). The selection
    // entry appears when a cap binds or when verified relationship
    // families grouped statements (schema 0.6), so both mechanisms
    // are disclosed, never silent.
    let demotions = crate::account::demote_restatements(&mut findings, &table, &frame);
    let demotion_note = if demotions.is_empty() {
        None
    } else {
        let total: usize = demotions.values().sum();
        let by_reason: Vec<String> = demotions
            .iter()
            .map(|(reason, n)| format!("{reason} ×{n}"))
            .collect();
        Some(format!(
            "frame model demoted {total} finding(s) in ranking ({}) — demoted, never removed; the reason is recorded in each finding's evidence",
            by_reason.join(", ")
        ))
    };
    if let Some(note) = &demotion_note {
        if let Some(info) = coverage.get_mut("frame_model") {
            info.detail.push_str("; ");
            info.detail.push_str(note);
        }
    }
    // Declared questions (issue #310): a finding answers a
    // declared question when the question names one of the
    // finding's subject columns (the outcome included, for
    // target-relative findings). The match is marked in the
    // finding's evidence, where the ledger's selection order
    // reads it: a marked finding is admitted ahead of an
    // unmatched finding of equal score. The questions coverage
    // entry, stamped after selection, states per question how
    // many selected findings answer it.
    let declared_questions: Vec<String> = cfg
        .hints
        .as_ref()
        .map(|h| h.questions.clone())
        .unwrap_or_default();
    if !declared_questions.is_empty() {
        let lowered: Vec<String> = declared_questions
            .iter()
            .map(|q| q.to_lowercase())
            .collect();
        for finding in findings.iter_mut() {
            let mut matched: Vec<usize> = Vec::new();
            let mut columns: Vec<&str> =
                finding.subject.columns.iter().map(String::as_str).collect();
            if let Some(t) = &finding.subject.target {
                if !columns.contains(&t.as_str()) {
                    columns.push(t.as_str());
                }
            }
            for (qi, q) in lowered.iter().enumerate() {
                if columns
                    .iter()
                    .any(|c| !c.is_empty() && q.contains(&c.to_lowercase()))
                {
                    matched.push(qi);
                }
            }
            if !matched.is_empty() {
                for evidence in &mut finding.evidence {
                    evidence
                        .metrics
                        .insert("declared_questions".into(), serde_json::json!(matched));
                }
            }
        }
    }
    let outcome = select_findings_counted(findings, &cfg.selection);
    let mut findings = outcome.selected;
    let mut grouped = outcome.grouped;
    if let Some(source) = &cfg.source {
        if source.basis == "sampled" {
            // Basis disclosure: on a sampled source no finding may
            // present sample numbers as population numbers. Every
            // evidence scope names the sample and the source
            // population it was drawn from.
            for finding in findings.iter_mut().chain(grouped.iter_mut()) {
                for evidence in &mut finding.evidence {
                    evidence.scope.sampling = Sampling::Sample;
                    evidence.scope.population_rows = Some(source.rows);
                }
            }
        }
    }
    // Column labels (hints.labels) are presentation, applied here —
    // after selection, so finding ids (derived from the canonical
    // statement, never the claim text), subjects, specifications,
    // and evidence all keep the raw column names. Only claim and
    // interpretation prose is rewritten, and each finding's prose is
    // rewritten with only its own statement's labels: a finding
    // substitutes the labels of the columns its subject and
    // specification name, never the whole map, so a column named
    // like an ordinary word is not replaced in prose that merely
    // uses the word or talks about another column. Unusable entries
    // fall back to the raw name and are stated in the labels
    // coverage entry.
    let resolved_labels = cfg.hints.as_ref().and_then(|hints| {
        if hints.labels.is_empty() {
            None
        } else {
            Some(onceover_report::resolve_labels(&hints.labels, &table.names))
        }
    });
    if let Some(resolved) = &resolved_labels {
        if !resolved.applied.is_empty() {
            for finding in findings.iter_mut().chain(grouped.iter_mut()) {
                let scoped = onceover_report::labels_for_finding(&resolved.applied, finding);
                if scoped.is_empty() {
                    continue;
                }
                finding.claim = onceover_report::substitute_labels(&finding.claim, &scoped);
                if let Some(interp) = &finding.interpretation {
                    finding.interpretation =
                        Some(onceover_report::substitute_labels(interp, &scoped));
                }
            }
        }
        let declared = cfg.hints.as_ref().map_or(0, |h| h.labels.len());
        let pairs: Vec<String> = resolved
            .applied
            .iter()
            .map(|(name, label)| format!("'{name}' as '{label}'"))
            .collect();
        let mut detail = format!(
            "{declared} label(s) declared, {} applied to claims and interpretations",
            resolved.applied.len()
        );
        if !pairs.is_empty() {
            detail.push_str(&format!(" ({})", pairs.join(", ")));
        }
        detail.push_str("; subjects, evidence, and metrics keep the raw column names");
        if !resolved.notes.is_empty() {
            detail.push_str("; ");
            detail.push_str(&resolved.notes.join("; "));
        }
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "labels".into(),
            PassInfo::ran(detail),
        );
    }
    // Declared gloss (issue #310): the caller's meaning of a
    // column, stated in the prose of findings whose subject is
    // that column — appended as a sentence naming the column, so
    // no measured claim is rewritten. Applied after selection,
    // like the labels; subjects and evidence keep the raw names.
    if let Some(hints) = &cfg.hints {
        if !hints.gloss.is_empty() {
            let mut applied = 0usize;
            for finding in findings.iter_mut().chain(grouped.iter_mut()) {
                let mut sentences: Vec<String> = Vec::new();
                for column in &finding.subject.columns {
                    if let Some(onceover_report::Gloss::Text(text)) = hints.gloss.get(column) {
                        sentences.push(format!(" The caller declares '{column}' to mean: {text}."));
                    }
                }
                if !sentences.is_empty() {
                    applied += 1;
                    for s in sentences {
                        finding.claim.push_str(&s);
                    }
                }
            }
            if applied > 0 {
                decl_notes.push(format!(
                    "gloss stated in the prose of {applied} selected finding(s) whose subject is a glossed column"
                ));
            }
        }
    }
    // The declarations account: everything the caller declared
    // under the metadata keys, what was checked, what was carried
    // unchecked, and the applied/refused decisions for the four
    // structural keys — one entry, never silent.
    if frame.info.declarations.is_some() || !decl_notes.is_empty() {
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "declarations".into(),
            PassInfo::ran(decl_notes.join("; ")),
        );
    }
    // Per declared question, how many selected findings answer
    // it — including zero, which is a stated result.
    if !declared_questions.is_empty() {
        let mut counts = vec![0usize; declared_questions.len()];
        for finding in findings.iter().chain(grouped.iter()) {
            let mut seen: BTreeSet<usize> = BTreeSet::new();
            for evidence in &finding.evidence {
                if let Some(list) = evidence
                    .metrics
                    .get("declared_questions")
                    .and_then(|v| v.as_array())
                {
                    for v in list {
                        if let Some(qi) = v.as_u64() {
                            seen.insert(qi as usize);
                        }
                    }
                }
            }
            for qi in seen {
                if qi < counts.len() {
                    counts[qi] += 1;
                }
            }
        }
        let parts: Vec<String> = declared_questions
            .iter()
            .zip(counts.iter())
            .map(|(q, n)| format!("\"{q}\": {n} finding(s)"))
            .collect();
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "questions".into(),
            PassInfo::ran(format!(
                "findings answering each declared question, matched by the columns the question names: {}",
                parts.join("; ")
            )),
        );
    }
    let capped = outcome.capped;
    let grouping_note = if grouped.is_empty() {
        None
    } else {
        Some(format!(
            "{} statement(s) grouped under {} verified relationship families — each lead finding carries the grouped ids in grouped_members; the statements remain in the report under grouped_findings",
            grouped.len(),
            outcome.families
        ))
    };
    let rationed = !outcome.group_capped.is_empty() || outcome.reserve_capped > 0;
    if !capped.is_empty()
        || grouping_note.is_some()
        || rationed
        || !outcome.dropped_invalid.is_empty()
    {
        // Per-detector report caps, the per-group cap, the
        // hygiene reserve, family grouping, and validation
        // failures are disclosed with counts, never applied
        // silently (the demote-never-suppress rule).
        let mut parts: Vec<String> = capped
            .iter()
            .map(|(detector, n)| {
                let cap = cfg
                    .selection
                    .max_findings_per_detector
                    .get(detector)
                    .or_else(|| cfg.selection.pressure_detector_caps.get(detector))
                    .copied()
                    .unwrap_or(0);
                format!("{detector}: {n} statement(s) past the per-detector report cap of {cap} not selected")
            })
            .collect();
        parts.extend(outcome.group_capped.iter().map(|(label, n)| {
            format!(
                "{label}: {n} statement(s) past the per-group report cap of {} not selected",
                cfg.selection.max_findings_per_group
            )
        }));
        parts.extend(
            outcome
                .group_capped_by_detector
                .iter()
                .map(|(detector, n)| {
                    format!(
                "{detector}: {n} statement(s) past the per-group report cap of {} not selected",
                cfg.selection.max_findings_per_group
            )
                }),
        );
        if outcome.reserve_capped > 0 {
            parts.push(format!(
                "hygiene class: {} statement(s) past the hygiene reserve of {} not selected",
                outcome.reserve_capped, cfg.selection.hygiene_reserve
            ));
        }
        if !outcome.dropped_invalid.is_empty() {
            let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
            for invalid in &outcome.dropped_invalid {
                let label = format!(
                    "{}/{}",
                    invalid.category.as_str(),
                    invalid.finding_type.as_deref().unwrap_or("none")
                );
                *by_type.entry(label).or_insert(0) += 1;
            }
            let breakdown: Vec<String> = by_type
                .iter()
                .map(|(label, n)| format!("{label} ×{n}"))
                .collect();
            parts.push(format!(
                "{} finding(s) failed validation and were dropped before selection ({})",
                outcome.dropped_invalid.len(),
                breakdown.join(", ")
            ));
        }
        if let Some(note) = &grouping_note {
            parts.push(note.clone());
        }
        if let Some(note) = &demotion_note {
            parts.push(note.clone());
        }
        let info = if capped.is_empty() && !rationed && outcome.dropped_invalid.is_empty() {
            // Grouping alone bound nothing: the entry is an exact
            // run record, not a cap disclosure.
            PassInfo::ran(parts.join("; "))
        } else {
            PassInfo::capped(parts.join("; "))
        };
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "selection".into(),
            info,
        );
    }
    let groups = group_findings(&findings);
    let selection = onceover_report::SelectionSummary {
        candidates: candidate_count,
        verified_candidates: verified_count,
        distinct_statements: distinct_count,
        selected: findings.len(),
        families: outcome.families,
        grouped_statements: grouped.len(),
        candidates_by_class: outcome.candidates_by_class.clone(),
        selected_by_class: outcome.selected_by_class.clone(),
        capped_by_class: outcome.capped_by_class.clone(),
    };
    let applied_labels: BTreeMap<String, String> = resolved_labels
        .as_ref()
        .map(|r| r.applied.clone())
        .unwrap_or_default();
    let mut overview = build_overview(&table, &columns, &findings, cfg, &applied_labels);
    if let Some(source) = &cfg.source {
        if source.basis == "sampled" {
            overview.headline.push_str(&format!(
                "; profiled on a stratified sample from {} source rows",
                source.rows
            ));
        }
    }
    if deadline_hit {
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "deadline".into(),
            PassInfo::skipped(format!(
                "wall-clock deadline {:.1}s reached; later passes skipped",
                start.elapsed().as_secs_f64()
            )),
        );
    }

    ProfileReport {
        schema_version: SCHEMA_VERSION.into(),
        frame: Some(frame.info),
        overview,
        columns,
        findings,
        grouped_findings: grouped,
        groups,
        selection,
        source: cfg.source.clone(),
        coverage,
        associations: association_pairs,
    }
}

fn quick_deps(info: &[ColInfo], n: usize) -> DepsOutcome {
    DepsOutcome {
        single_unique: (0..info.len())
            .filter(|&i| info[i].null_count == 0 && info[i].distinct == n && n > 0)
            .collect(),
        coverage: "quick tier: single-column uniqueness only".into(),
        ..Default::default()
    }
}

fn keys_pk(findings: &[Finding]) -> Option<String> {
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

/// Duplicate value examples for dirty-key findings (dtype-aware display).
/// Materialize the pivoted wide form of a verified long-format
/// frame: one row per index tuple (first-appearance order), the
/// index columns with their original dtypes (a take of their
/// first-occurrence rows), and one column per attribute level holding
/// the measure with its original dtype. None when a level name collides with an
/// index column or the measure is not numeric.
fn build_wide_frame(
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

fn dup_examples(table: &TypedTable, i: usize, c: &CardOut) -> Vec<String> {
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

fn build_overview(
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
    let headline=format!("{} rows × {} columns ({} budget){}; {} functional dependencies, {} predictive dependencies, {} conditional rules",
        table.n,table.cols.len(),cfg.budget.as_str(),pk,n_functional,n_predictive,n_rules);

    Overview {
        n_rows: table.n,
        n_columns: table.cols.len(),
        headline,
        dtype_counts,
        role_counts,
        top_findings: findings.iter().take(5).map(|f| f.claim.clone()).collect(),
    }
}

/// Record a pass's coverage entry with its attributed wall time:
/// the interval since the previous entry was recorded. The
/// pipeline runs passes sequentially and records each entry when
/// its pass finishes, so the delta attributes the pass's own work;
/// the first entry also carries extraction and the column cards,
/// and work after the last pass (selection, grouping) is not
/// attributed to any pass.
fn stamp_insert(
    coverage: &mut BTreeMap<String, PassInfo>,
    mark: &mut Instant,
    on: bool,
    key: String,
    mut info: PassInfo,
) {
    let now = Instant::now();
    if on {
        info.elapsed_ms = Some(now.duration_since(*mark).as_secs_f64() * 1e3);
    }
    *mark = now;
    coverage.insert(key, info);
}

/// Give the derived columns the `timestamp_part` role, with their
/// evidence, in the frame model's role vectors (the fields, so the
/// rest of the frame model can stay borrowed).
fn assign_timestamp_parts(
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
fn sync_card_roles(
    columns: &mut BTreeMap<String, ColumnCard>,
    table: &TypedTable,
    frame: &crate::frame::FrameModel,
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
fn scaffold_note(
    table: &TypedTable,
    frame: &crate::frame::FrameModel,
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
fn merge_groups(groups: &mut Vec<Vec<usize>>) {
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
fn identity_columns(table: &TypedTable, findings: &[Finding]) -> Vec<Vec<usize>> {
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
