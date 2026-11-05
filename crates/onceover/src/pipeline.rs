//! The pipeline: run passes in cost order under the budget, assemble the
//! findings ledger, overview and coverage block. Deterministic end to end
//! (unless an opt-in wall-clock deadline fires, which is reported honestly).

use onceover_report::{Method, Pattern};
use std::collections::BTreeMap;
use std::time::Instant;

use onceover_core::{extract, find_duplicate_rows, CardOut, TypedTable, VKey};
use onceover_deps::{analyze_keys, discover, sortedness, ColInfo, DepsOutcome};
use onceover_mines::{
    analyze_temporal, association_rules, decision_tables, encode, hotspots, identities,
    prism_rules, profile_breaks, profile_discontinuities, profile_drift, profile_interactions,
    profile_regimes, profile_surprises, profile_target_shapes, screen_predictability,
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
        active.insert("target_shapes".into());
    }
    let enabled = |id: &str| active.contains(id);
    let selected = |id: &str| cfg.detectors.as_ref().is_none_or(|ids| ids.contains(id));
    let activations: BTreeMap<String, crate::detectors::Activation> =
        crate::detectors::builtin_detectors()
            .into_iter()
            .map(|d| (d.id, d.activation))
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
    if let Some(predictors) = cfg.max_predictors {
        tier.screen.max_predictors = predictors.max(1);
        tier.encode.feature_cap = predictors.max(1);
    }
    if let Some(pairs) = cfg.max_pairs {
        tier.dep_pair_cap = pairs;
    }
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

    // ---------- extraction ----------
    let table = extract(df);
    let unsupported = table
        .dtypes
        .iter()
        .filter(|d| d.as_str() == "other")
        .count();
    stamp_insert(
        &mut coverage,
        &mut last_mark,
        timing_on,
        "extraction".into(),
        PassInfo::ran(format!(
            "{} rows × {} columns{}",
            table.n,
            table.cols.len(),
            if unsupported > 0 {
                format!(" ({unsupported} unsupported column type(s) opaque to the profiler)")
            } else {
                String::new()
            }
        )),
    );

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
            role: c.card.role.clone(),
            n: c.card.n,
            null_count: c.card.null_count,
            distinct: c.card.distinct,
            dup_examples: dup_examples(&table, i, c),
        })
        .collect();

    let mut findings: Vec<Finding> = Vec::new();
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
            card_role: c.card.role.clone(),
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
    for (i, name) in table.names.iter().enumerate() {
        if let Some(card) = columns.get_mut(name) {
            card.semantic_role = frame.roles[i].clone();
            if frame.roles[i].is_some() {
                card.semantic_role_evidence = Some(frame.info.roles[i].evidence.clone());
            }
        }
    }
    let temporal_df = build_temporal_df(df, &frame);
    let temporal_table = temporal_df.as_ref().map(|d| extract(d));
    let ttable: &TypedTable = temporal_table.as_ref().unwrap_or(&table);
    let frame_time_hint: Option<&str> = frame.axis_name.as_deref();
    let scaffold_excluded: Vec<usize> = (0..table.cols.len())
        .filter(|&i| {
            matches!(info[i].role.as_str(), "constant" | "all_null")
                || matches!(
                    frame.roles[i].as_deref(),
                    Some("index") | Some("identifier") | Some("timestamp")
                )
                || (frame.axis_source.as_deref() == Some("timestamp_parts")
                    && frame.axis_parts.contains(&table.names[i]))
        })
        .collect();
    let exclusion_note = if scaffold_excluded.is_empty() {
        String::new()
    } else {
        let parts: Vec<String> = scaffold_excluded
            .iter()
            .map(|&i| {
                format!(
                    "{} ({})",
                    table.names[i],
                    frame.roles[i].as_deref().unwrap_or(info[i].role.as_str())
                )
            })
            .collect();
        format!("; frame-role target exclusions: {}", parts.join(", "))
    };

    // ---------- duplicate rows ----------
    if check_deadline(&mut coverage, "row_duplicates") {
        deadline_hit = true;
    }
    if !deadline_hit
        && enabled("row_duplicates")
        && table
            .cols
            .iter()
            .any(|c| !matches!(c, onceover_core::TypedCol::Other { .. }))
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
                    .filter(|(_, col)| !matches!(col, onceover_core::TypedCol::Other { .. }))
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
                findings.push(Finding::new(
                    Pattern::Duplicates,
                    BTreeMap::from([("columns".into(),json!(table.names.iter().zip(&table.cols).filter(|(_,col)|!matches!(col,onceover_core::TypedCol::Other{..})).map(|(name,_)|name).collect::<Vec<_>>()))]),
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
        && table
            .cols
            .iter()
            .all(|c| matches!(c, onceover_core::TypedCol::Other { .. }))
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
            stamp_insert(
                &mut coverage,
                &mut last_mark,
                timing_on,
                "functional_dependencies".into(),
                PassInfo::skipped("quick tier"),
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
                    PassInfo::capped(format!("{}{}", d.coverage, exclusion_note)),
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
            "; findings that restate the calendar derivation or the scaffolding columns are demoted in ranking, never removed; the demotion counts are appended to this entry",
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

    // ---------- duplicate encodings ----------
    if check_deadline(&mut coverage, "duplicate_encodings") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("duplicate_encodings") {
        let (f, cov) = onceover_deps::profile_duplicate_encodings(&table, &info, &tier.dupe);
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "duplicate_encodings".into(),
            PassInfo::ran(cov),
        );
        findings.extend(f);
    }

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
                            let wide_report = profile_inner(&wide, &cfg2, extensions, depth + 1);
                            let n_wide =
                                wide_report.findings.len() + wide_report.grouped_findings.len();
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
                                    "profiled the pivoted wide form ({} index columns, {} levels of {} -> {} rows x {} columns); {} findings included, tagged source_form in their evidence",
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
        let sorted = sortedness(&table);
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
                            | onceover_core::TypedCol::I64 { .. }
                            | onceover_core::TypedCol::U64 { .. }
                    ))
                    .count()
            )),
        );
        findings.extend(sent);
    }

    // ---------- digits ----------
    if check_deadline(&mut coverage, "digits") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("digits") {
        let (dig, cov) = onceover_deps::digit_fingerprints(&table);
        stamp_insert(
            &mut coverage,
            &mut last_mark,
            timing_on,
            "digits".into(),
            PassInfo::ran(cov),
        );
        findings.extend(dig);
    }

    // ---------- dates ----------
    if check_deadline(&mut coverage, "dates") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("dates") {
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
            "dates".into(),
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

            let mut screen = if enabled("gbm") {
                screen_predictability(&table, &enc, &tier.screen, &scaffold_excluded)
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
            } else {
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
            prism_targets.truncate(if enabled("prism") {
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
                let (f, cov) =
                    profile_interactions(&table, &enc, &screen.results, &tier.screen, &tier.hstat);
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
                // the pass's time hint; scaffolding columns are
                // excluded as series.
                let excluded: &[usize] = &scaffold_excluded;
                let time_hint =
                    frame_time_hint.or_else(|| cfg.hints.as_ref().and_then(|h| h.time.as_deref()));
                let (f, cov) = profile_breaks(ttable, excluded, &tier.breaks, time_hint);
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
            if check_deadline(&mut coverage, "ordered_partitions") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("ordered_partitions") {
                let excluded: &[usize] = &scaffold_excluded;
                let time_hint =
                    frame_time_hint.or_else(|| cfg.hints.as_ref().and_then(|h| h.time.as_deref()));
                let out = onceover_mines::ordered_partitions(
                    ttable,
                    excluded,
                    &tier.partitions,
                    time_hint,
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
            if check_deadline(&mut coverage, "surprises") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("surprises") {
                let excluded: &[usize] = &scaffold_excluded;
                let (f, cov) = profile_surprises(&table, excluded, &tier.surprises);
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "surprises".into(),
                    PassInfo::ran(format!("{cov}{exclusion_note}")),
                );
                findings.extend(f);
            }
            if check_deadline(&mut coverage, "discontinuities") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("discontinuities") {
                // The screen's targets and axes both come from its
                // numeric pool, so the frame-role exclusions remove
                // scaffolding from both roles at once. Semantic
                // identifier never fires on float columns, so unique
                // real-valued measurements stay eligible — the
                // behaviour the old card-role carve-out protected.
                let excluded: &[usize] = &scaffold_excluded;
                let (f, cov) = profile_discontinuities(&table, excluded, &tier.disc);
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "discontinuities".into(),
                    PassInfo::ran(format!("{cov}{exclusion_note}")),
                );
                findings.extend(f);
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
            }

            if deadline_hit || check_deadline(&mut coverage, "hotspots") {
                deadline_hit = true;
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "hotspots".into(),
                    PassInfo::skipped("deadline reached"),
                );
            } else if enabled("hotspots") {
                let (f, cov) = hotspots(&enc, &tier.hotspots);
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "hotspots".into(),
                    PassInfo::sampled(cov),
                );
                findings.extend(f);
            }

            // Pairwise dependence (supporting role) + identities.
            if check_deadline(&mut coverage, "dependence_screen") {
                deadline_hit = true;
            }
            if !deadline_hit && tier.dep_pair_cap > 0 && enabled("numeric_dependence") {
                let (deps_pair, dep_findings, dep_cov) = onceover_mines::dep::screen_sampled(
                    &table,
                    tier.dep_pair_cap,
                    tier.encode.row_cap,
                    &scaffold_excluded,
                );
                let numeric = table
                    .cols
                    .iter()
                    .filter(|col| {
                        matches!(
                            col,
                            onceover_core::TypedCol::I64 { .. }
                                | onceover_core::TypedCol::U64 { .. }
                                | onceover_core::TypedCol::F64 { .. }
                        )
                    })
                    .count();
                let possible_pairs = numeric * numeric.saturating_sub(1) / 2;
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "dependence_screen".into(),
                    if deps_pair.len() < possible_pairs {
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

            // Greedy view/cluster pass (standard / deep only).
            if check_deadline(&mut coverage, "views") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("views") {
                let (vf, vout) = onceover_mines::views_pass(&table, &tier.views);
                // Dropped columns must be visible in coverage, not
                // only in finding metrics: with no views findings at
                // all, the exclusion would otherwise be silent. A
                // budget-truncated run reports as capped, like the
                // other truncated passes, not as a complete run.
                let mut detail = vout.coverage.clone();
                if !vout.dropped_columns.is_empty() {
                    detail.push_str(&format!(
                        "; dropped {} column(s) past the {}-column cap: {}",
                        vout.dropped_columns.len(),
                        tier.views.max_columns,
                        vout.dropped_columns.join(", ")
                    ));
                }
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    "views".into(),
                    if vout.budget_hit {
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
                    "views".into(),
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
            "association_rules",
            "dependence_screen",
            "arithmetic_identities",
            "period_associations",
            "views",
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
    // ---------- target-aware shapes (opt-in via a designated target) ----------
    if check_deadline(&mut coverage, "target_shapes") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("target_shapes") {
        match &cfg.target {
            Some(target) => {
                let (f, cov) = profile_target_shapes(&table, target, &tier.shapes);
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

    findings.retain(|f| f.evidence.iter().any(|e| selected(&e.method.detector)));
    for f in &mut findings {
        for evidence in &mut f.evidence {
            evidence.scope.population_rows.get_or_insert(table.n);
            evidence.scope.missing_values=match evidence.method.detector.as_str(){
                "column_profile"|"keys"=>"non-missing values counted; missingness reported separately",
                "row_duplicates"=>"missing values compare equal; opaque fields excluded",
                "functional_dependencies"|"numeric_dependence"=>"rows complete on the evaluated inputs and target",
                "gbm"|"decision_tables"|"regimes"|"interactions"=>"missing targets excluded; predictor handling follows the recorded model",
                "drift"=>"rows missing the target or the time value excluded; period comparisons use equal row counts on both sides",
                "target_shapes"=>"complete cases per predictor: rows missing the predictor or the target excluded",
                "breaks"=>"rows with a missing date or value are excluded from each series",
                "association_rules"|"prism"=>"see discovery metrics; normalized rule evidence uses complete predicates and outcome",
                _=>evidence.scope.missing_values.as_str(),
            }.to_string();
            if evidence.scope.sampling == Sampling::Unspecified {
                evidence.scope.sampling = match evidence.method.detector.as_str() {
                    "column_profile" | "keys" | "row_duplicates" | "sortedness" | "dates"
                    | "identities" => Sampling::Full,
                    "association_rules" | "prism" => {
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
    for (id, pass) in [
        ("row_duplicates", "row_duplicates"),
        ("keys", "keys"),
        ("pivot", "pivot"),
        ("functional_dependencies", "functional_dependencies"),
        ("sortedness", "sortedness"),
        ("granularity", "granularity"),
        ("dates", "dates"),
        ("gbm", "predictability_screen"),
        ("decision_tables", "decision_tables"),
        ("prism", "prism_rules"),
        ("regimes", "regimes"),
        ("interactions", "interactions"),
        ("breaks", "breaks"),
        ("association_rules", "association_rules"),
        ("numeric_dependence", "dependence_screen"),
        ("identities", "arithmetic_identities"),
        ("period_associations", "period_associations"),
        ("views", "views"),
        ("target_shapes", "target_shapes"),
        ("drift", "drift"),
    ] {
        if !enabled(id) {
            // A pass that did not run says why. Opt-in detectors are
            // named as such (an existing entry, e.g. a deadline skip,
            // is more specific and is kept).
            if !coverage.contains_key(pass) {
                let detail = match activations.get(id) {
                    Some(crate::detectors::Activation::OptIn) => format!(
                        "opt-in detector; not in the default set — request it with detectors=[\"{id}\"]"
                    ),
                    Some(crate::detectors::Activation::TargetGated) => {
                        "runs when a target is designated, or when explicitly selected together with a target".to_string()
                    }
                    _ => "detector not enabled".to_string(),
                };
                stamp_insert(
                    &mut coverage,
                    &mut last_mark,
                    timing_on,
                    pass.into(),
                    PassInfo::skipped(detail),
                );
            }
        } else if !selected(id) {
            if let Some(info) = coverage.get_mut(pass) {
                info.detail
                    .push_str("; required as a resource; findings not selected");
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
    let demotions = crate::account::demote_restatements(&mut findings, &table, &frame, &deps);
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
    let outcome = select_findings_counted(findings, &cfg.selection);
    let findings = outcome.selected;
    let grouped = outcome.grouped;
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
    if !capped.is_empty() || grouping_note.is_some() {
        // Per-detector report caps and family grouping are
        // disclosed with counts, never applied silently (the
        // demote-never-suppress rule).
        let mut parts: Vec<String> = capped
            .iter()
            .map(|(detector, n)| {
                let cap = cfg
                    .selection
                    .max_findings_per_detector
                    .get(detector)
                    .copied()
                    .unwrap_or(0);
                format!("{detector}: {n} statement(s) past the per-detector report cap of {cap} not selected")
            })
            .collect();
        if let Some(note) = &grouping_note {
            parts.push(note.clone());
        }
        if let Some(note) = &demotion_note {
            parts.push(note.clone());
        }
        let info = if capped.is_empty() {
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
    };
    let overview = build_overview(&table, &columns, &findings, cfg);
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
        coverage,
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
) -> Overview {
    let mut dtype_counts: BTreeMap<String, usize> = BTreeMap::new();
    for d in &table.dtypes {
        *dtype_counts.entry(d.clone()).or_insert(0) += 1;
    }
    let mut role_counts: BTreeMap<String, usize> = BTreeMap::new();
    for c in columns.values() {
        *role_counts.entry(c.role.clone()).or_insert(0) += 1;
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
        .map(|k| format!("; key candidate: {k}"))
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
