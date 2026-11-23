//! Model-based, statistical mining, axis, and extension detector passes
//! (standard and deep tiers).

use onceover_core::TypedTable;
use onceover_mines::{
    association_rules, decision_tables, encode, hotspots, identities, prism_rules, profile_breaks,
    profile_discontinuities, profile_drift, profile_interactions, profile_regimes,
    profile_surprises, screen_predictability, EncodedMatrix, ModelBasis,
};
use onceover_report::PassInfo;

use crate::config::Budget;

use super::helpers::{entity_columns, identity_columns, merge_groups};
use super::setup::{count_numeric_cols, PreparedFrame, ResolvedMetadata, ResolvedWeights};
use super::structural::StructuralOutput;
use super::RunState;

#[allow(clippy::too_many_arguments)]
pub(super) fn run_model_passes(
    state: &mut RunState<'_>,
    table: &TypedTable,
    ttable: &TypedTable,
    prep: &PreparedFrame,
    meta: &ResolvedMetadata,
    weights: &ResolvedWeights,
    structural: &StructuralOutput,
    model_basis: &mut ModelBasis,
    extensions: &[&dyn crate::Detector],
) -> Option<EncodedMatrix> {
    let mut shared_encoding: Option<EncodedMatrix> = None;

    if state.cfg.budget != Budget::Quick {
        if !state.check_deadline("predictability_screen") {
            let (enc, enc_cov) = encode(table, &state.tier.encode);
            state.stamp("encoding", PassInfo::sampled(enc_cov));

            run_dependence_and_identities(state, table, ttable, prep, model_basis);
            let screen = run_predictability_and_rules(
                state,
                table,
                &enc,
                prep,
                meta,
                &structural.calendar_inputs,
                &model_basis.identity_groups,
            );
            run_regimes_and_interactions(state, table, &enc, &screen.results, prep, model_basis);
            drop(screen);

            run_axis_passes(state, table, ttable, prep, meta, structural);
            run_outliers_slices_disc_drift(state, table, ttable, &enc, prep, model_basis);
            run_assoc_hotspots_clusters(state, table, &enc, prep, weights);

            // Keep `enc` only if a downstream consumer actually needs it
            // (`extensions`, `model_failures`, or rule sampling flag).
            shared_encoding = Some(enc);
        } else {
            state.stamp(
                "predictability_screen",
                PassInfo::skipped("deadline reached"),
            );
            run_deadline_self_limiting_passes(state, table, prep, weights);
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
            state.stamp(pass, PassInfo::skipped("quick tier"));
        }
    }

    for pass in ["regimes", "association_rules"] {
        state.coverage.entry(pass.into()).or_insert_with(|| {
            PassInfo::skipped(if state.deadline_hit {
                "deadline reached"
            } else {
                "model screening unavailable"
            })
        });
    }

    if state.enabled("partition_scagnostics") && !state.check_deadline("partition_scagnostics") {
        let (shapes, info) =
            crate::partition_scagnostics::profile_pass(state, table, prep, model_basis);
        state.stamp("partition_scagnostics", info);
        state.findings.extend(shapes);
    }

    run_extensions(state, table, shared_encoding.as_ref(), extensions);

    shared_encoding
}

fn run_dependence_and_identities(
    state: &mut RunState<'_>,
    table: &TypedTable,
    ttable: &TypedTable,
    prep: &PreparedFrame,
    model_basis: &mut ModelBasis,
) {
    let mut identity_groups: Vec<Vec<usize>> = Vec::new();
    let dep_dispatched = state.tier.dep_pair_cap > 0 && state.enabled("numeric_dependence");
    if !dep_dispatched && state.check_deadline("dependence_screen") {
        return;
    }
    if dep_dispatched {
        let dep_out = onceover_mines::dep::screen_sampled(
            table,
            state.tier.dep_pair_cap,
            state.tier.encode.row_cap,
            &prep.scaffold_excluded,
            state.pass_deadline,
        );
        if dep_out.deadline_stopped {
            state.deadline_hit = true;
        }
        let (deps_pair, dep_findings, dep_cov) = (dep_out.deps, dep_out.findings, dep_out.coverage);
        state.association_pairs.extend(dep_out.records);

        // Stamp `dependence_screen` BEFORE `row_consistency` so timing
        // attributes each pass's own work accurately.
        let numeric = count_numeric_cols(table);
        let possible_pairs = numeric * numeric.saturating_sub(1) / 2;
        state.stamp(
            "dependence_screen",
            if dep_out.deadline_stopped || deps_pair.len() < possible_pairs {
                PassInfo::capped(dep_cov)
            } else if table.n > state.tier.encode.row_cap {
                PassInfo::sampled(dep_cov)
            } else {
                PassInfo::ran(dep_cov)
            },
        );
        state.findings.extend(dep_findings);

        if state.enabled("row_consistency") {
            let measures: Vec<usize> = prep
                .frame
                .roles
                .iter()
                .enumerate()
                .filter(|(_, r)| r.as_deref() == Some("measure"))
                .map(|(i, _)| i)
                .collect();
            let (mut f, detail) = onceover_mines::profile_row_consistency(
                table,
                &deps_pair,
                &prep.scaffold_excluded,
                &measures,
            );
            state.stamp("row_consistency", PassInfo::ran(detail));
            state.findings.append(&mut f);
        }

        if state.tier.ident_pair_cap > 0 && state.enabled("identities") {
            let (id_f, id_cov) = identities(
                table,
                &deps_pair,
                state.tier.ident_pair_cap,
                &prep.scaffold_excluded,
            );
            state.stamp(
                "arithmetic_identities",
                PassInfo::ran(format!("{id_cov}{}", prep.exclusion_note)),
            );
            identity_groups.extend(identity_columns(table, &id_f));
            state.findings.extend(id_f);
        }

        if state.enabled("period_associations") {
            let out =
                onceover_mines::period_associations(ttable, &deps_pair, state.tier.encode.row_cap);
            let info = match out.status {
                onceover_mines::PeriodsStatus::NotApplicable(d) => PassInfo::na(d),
                onceover_mines::PeriodsStatus::Ran(d) => PassInfo::ran(d),
                onceover_mines::PeriodsStatus::Capped(d) => PassInfo::capped(d),
            };
            state.stamp("period_associations", info);
            state.findings.extend(out.findings);
        }

        if state.tier.ident_multiterm_cap > 0 && state.enabled("identities") {
            let (mt_f, mt_cov) = onceover_mines::multiterm_identities(
                table,
                state.tier.ident_multiterm_cap,
                &prep.scaffold_excluded,
            );
            state.stamp(
                "multiterm_identities",
                PassInfo::ran(format!("{mt_cov}{}", prep.exclusion_note)),
            );
            identity_groups.extend(identity_columns(table, &mt_f));
            state.findings.extend(mt_f);
        }
    } else if state.deadline_hit {
        state.stamp("dependence_screen", PassInfo::skipped("deadline reached"));
    }

    merge_groups(&mut identity_groups);
    model_basis.identity_groups = identity_groups;
}

fn run_predictability_and_rules(
    state: &mut RunState<'_>,
    table: &TypedTable,
    enc: &EncodedMatrix,
    prep: &PreparedFrame,
    meta: &ResolvedMetadata,
    calendar_inputs: &[usize],
    identity_groups: &[Vec<usize>],
) -> onceover_mines::ScreenOutcome {
    let mut screen = if state.enabled("predictability") {
        screen_predictability(
            table,
            enc,
            &state.tier.screen,
            &prep.scaffold_excluded,
            calendar_inputs,
            &meta.recorded_after_idx,
            identity_groups,
        )
    } else {
        onceover_mines::ScreenOutcome {
            findings: Vec::new(),
            results: Vec::new(),
            coverage: "not selected".into(),
        }
    };
    state.stamp(
        "predictability_screen",
        PassInfo::sampled(screen.coverage.clone()),
    );
    state.findings.extend(std::mem::take(&mut screen.findings));

    let designated_name = state.cfg.target.as_deref();
    let mut cat_targets: Vec<&onceover_mines::TargetResult> = screen
        .results
        .iter()
        .filter(|r| {
            r.is_categorical && r.metric >= r.baseline + state.tier.dtable.min_baseline_gain
        })
        .collect();
    cat_targets.sort_by(|a, b| {
        let a_des = designated_name == Some(a.name.as_str());
        let b_des = designated_name == Some(b.name.as_str());
        b_des
            .cmp(&a_des)
            .then_with(|| b.metric.total_cmp(&a.metric))
            .then_with(|| a.name.cmp(&b.name))
    });
    let dtable_eligible = cat_targets.len();
    cat_targets.truncate(if state.enabled("decision_tables") {
        state.tier.dtable_top_targets
    } else {
        0
    });
    if !cat_targets.is_empty() {
        let (f, cov) = decision_tables(enc, &cat_targets, &state.tier.dtable);
        let detail = format!(
            "{cov}; {} of {dtable_eligible} eligible targets selected (cap {})",
            cat_targets.len(),
            state.tier.dtable_top_targets
        );
        state.stamp(
            "decision_tables",
            if cat_targets.len() < dtable_eligible {
                PassInfo::capped(detail)
            } else {
                PassInfo::sampled(detail)
            },
        );
        state.findings.extend(f);
    } else if state.enabled("decision_tables") {
        state.stamp(
            "decision_tables",
            PassInfo::na("no categorical targets with predictive signal above baseline"),
        );
    }

    let mut prism_targets: Vec<&onceover_mines::TargetResult> = screen
        .results
        .iter()
        .filter(|r| {
            r.is_categorical
                && r.metric >= state.tier.prism.min_target_accuracy
                && r.baseline < 0.9
                && r.cat_ids
                    .as_ref()
                    .is_some_and(|(_, labels)| labels.len() <= state.tier.prism.max_classes)
        })
        .collect();
    prism_targets.sort_by(|a, b| {
        let a_des = designated_name == Some(a.name.as_str());
        let b_des = designated_name == Some(b.name.as_str());
        b_des
            .cmp(&a_des)
            .then_with(|| b.metric.total_cmp(&a.metric))
            .then_with(|| a.name.cmp(&b.name))
    });
    let prism_eligible = prism_targets.len();
    prism_targets.truncate(if state.enabled("covering_rules") {
        state.tier.prism_top_targets
    } else {
        0
    });
    let (f, cov) = prism_rules(enc, &prism_targets, &state.tier.prism);
    let detail = format!(
        "{cov}; {} of {prism_eligible} eligible targets selected (cap {})",
        prism_targets.len(),
        state.tier.prism_top_targets
    );
    state.stamp(
        "prism_rules",
        if prism_targets.len() < prism_eligible {
            PassInfo::capped(detail)
        } else {
            PassInfo::sampled(detail)
        },
    );
    state.findings.extend(f);

    screen
}

fn run_regimes_and_interactions(
    state: &mut RunState<'_>,
    table: &TypedTable,
    enc: &EncodedMatrix,
    results: &[onceover_mines::TargetResult],
    prep: &PreparedFrame,
    model_basis: &ModelBasis,
) {
    let designated = state
        .cfg
        .target
        .as_deref()
        .and_then(|t| table.names.iter().position(|n| n == t));
    if !state.check_deadline("regimes") && state.enabled("regimes") {
        let (f, cov) = profile_regimes(
            table,
            enc,
            results,
            &prep.scaffold_excluded,
            &state.tier.regimes,
            designated,
        );
        state.stamp(
            "regimes",
            PassInfo::sampled(format!("{cov}{}", prep.exclusion_note)),
        );
        state.findings.extend(f);
    }
    if !state.check_deadline("interactions") && state.enabled("interactions") {
        let (f, cov) = profile_interactions(
            table,
            enc,
            results,
            &state.tier.screen,
            &state.tier.hstat,
            model_basis,
            designated,
        );
        state.stamp("interactions", PassInfo::sampled(cov));
        state.findings.extend(f);
    }
}

fn run_axis_passes(
    state: &mut RunState<'_>,
    table: &TypedTable,
    ttable: &TypedTable,
    prep: &PreparedFrame,
    meta: &ResolvedMetadata,
    structural: &StructuralOutput,
) {
    let frame_time_hint = prep.frame.axis_name.as_deref();
    let default_axis_hint = prep.default_axis_hint.as_deref();

    if !state.check_deadline("breaks") && state.enabled("breaks") {
        if !state.axis_explicit("breaks") && !prep.axis_gate_open {
            state.stamp(
                "breaks",
                PassInfo::skipped(RunState::axis_gate_reason("break scan")),
            );
        } else {
            let (time_hint, hinted_only) = if state.axis_explicit("breaks") {
                (
                    frame_time_hint
                        .or_else(|| state.cfg.hints.as_ref().and_then(|h| h.time.as_deref())),
                    false,
                )
            } else {
                (default_axis_hint, true)
            };
            let (f, cov) = profile_breaks(
                ttable,
                &structural.measure_excluded,
                &state.tier.breaks,
                time_hint,
                hinted_only,
            );
            let cov = if cov.ends_with("skipped") {
                cov
            } else {
                format!("{cov}{}", prep.exclusion_note)
            };
            state.stamp("breaks", PassInfo::sampled(cov));
            state.findings.extend(f);
        }
    }

    if !state.check_deadline("ordered_partitions") && state.enabled("ordered_partitions") {
        state.tier.partitions.exclude_responses = structural.measure_excluded.clone();
        if !state.axis_explicit("ordered_partitions") && !prep.axis_gate_open {
            state.stamp(
                "ordered_partitions",
                PassInfo::skipped(RunState::axis_gate_reason("ordered partition scan")),
            );
        } else {
            let (time_hint, hinted_only) = if state.axis_explicit("ordered_partitions") {
                (
                    frame_time_hint
                        .or_else(|| state.cfg.hints.as_ref().and_then(|h| h.time.as_deref())),
                    false,
                )
            } else {
                (default_axis_hint, true)
            };
            let out = onceover_mines::ordered_partitions(
                ttable,
                &structural.grouping_excluded,
                &state.tier.partitions,
                time_hint,
                hinted_only,
            );
            let info2 = match out.status {
                onceover_mines::PartitionsStatus::NotApplicable(d) => PassInfo::na(d),
                onceover_mines::PartitionsStatus::Ran(d) => {
                    PassInfo::ran(format!("{d}{}", prep.exclusion_note))
                }
                onceover_mines::PartitionsStatus::Capped(d) => {
                    PassInfo::capped(format!("{d}{}", prep.exclusion_note))
                }
            };
            state.stamp("ordered_partitions", info2);
            state.findings.extend(out.findings);
        }
    }

    if !state.check_deadline("distribution_change") && state.enabled("distribution_change") {
        let axis_name = prep.frame.info.axis.as_ref().map(|a| a.name.clone());
        match (prep.axis_ticks.as_ref(), axis_name) {
            (Some(ticks), Some(axis_name)) => {
                let measures: Vec<usize> = prep
                    .frame
                    .roles
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.as_deref() == Some("measure"))
                    .map(|(i, _)| i)
                    .filter(|i| !prep.axis_excluded.contains(i))
                    .collect();
                let input = onceover_mines::ChangeforestInput {
                    axis_name,
                    ticks: ticks.clone(),
                    measures,
                    axis_is_datetime: prep.frame.axis_is_datetime,
                };
                let out =
                    onceover_mines::changeforest_pass(table, &input, &state.tier.changeforest);
                let info = match out.status {
                    onceover_mines::ChangeforestStatus::NotApplicable(d) => PassInfo::na(d),
                    onceover_mines::ChangeforestStatus::Ran(d) => {
                        PassInfo::ran(format!("{d}{}", prep.exclusion_note))
                    }
                    onceover_mines::ChangeforestStatus::Capped(d) => {
                        PassInfo::capped(format!("{d}{}", prep.exclusion_note))
                    }
                };
                state.stamp("distribution_change", info);
                state.findings.extend(out.findings);
            }
            _ => state.stamp(
                "distribution_change",
                PassInfo::na("no date or datetime axis to segment along"),
            ),
        }
    }

    if !state.check_deadline("ordered_axis") && state.enabled("ordered_axis") {
        if !state.axis_explicit("ordered_axis") && !prep.axis_gate_open {
            state.stamp(
                "ordered_axis",
                PassInfo::skipped(RunState::axis_gate_reason("ordered-axis scan")),
            );
        } else {
            let axis_name = prep
                .frame
                .info
                .axis
                .as_ref()
                .map(|a| a.name.as_str())
                .unwrap_or("");
            let entity_names: Vec<String> = entity_columns(
                table,
                &prep.frame,
                &prep.card_facts,
                axis_name,
                meta.declared_entity_idx,
            )
            .iter()
            .map(|&i| table.names[i].clone())
            .collect();
            let (time_arg, hinted_only) = if state.axis_explicit("ordered_axis") {
                (
                    frame_time_hint
                        .or_else(|| state.cfg.hints.as_ref().and_then(|h| h.time.as_deref())),
                    false,
                )
            } else {
                (default_axis_hint, true)
            };
            let out = onceover_mines::profile_ordered_axis(
                ttable,
                &prep.scaffold_excluded,
                &state.tier.axis_battery,
                prep.declared_axis.as_deref(),
                time_arg,
                &prep.effective_level_order,
                &entity_names,
                &state.findings,
                hinted_only,
            );
            let info = match out.status {
                onceover_mines::AxisBatteryStatus::NotApplicable(d) => PassInfo::na(d),
                onceover_mines::AxisBatteryStatus::Ran(d) => {
                    PassInfo::ran(format!("{d}{}", prep.exclusion_note))
                }
                onceover_mines::AxisBatteryStatus::Capped(d) => {
                    PassInfo::capped(format!("{d}{}", prep.exclusion_note))
                }
            };
            state.stamp("ordered_axis", info);
            state.findings.extend(out.findings);
        }
    }
}

fn run_discontinuities_inner(state: &mut RunState<'_>, table: &TypedTable, prep: &PreparedFrame) {
    let measures: Vec<usize> = prep
        .frame
        .roles
        .iter()
        .enumerate()
        .filter(|(_, r)| r.as_deref() == Some("measure"))
        .map(|(i, _)| i)
        .collect();
    if !state.axis_explicit("discontinuities")
        && prep.disc_axes.is_empty()
        && !state.tier.disc.deadline.exceeded()
    {
        state.stamp(
            "discontinuities",
            PassInfo::skipped(
                "discontinuity screen not run: a default run scans along a usable numeric \
                 axis declared with the ordered_axis hint (date/datetime time axes are not \
                 numeric), and this frame has none; name the detector explicitly or declare \
                 a numeric ordered_axis to run it",
            ),
        );
    } else {
        let axes_arg =
            if state.axis_explicit("discontinuities") || state.tier.disc.deadline.exceeded() {
                None
            } else {
                Some(prep.disc_axes.as_slice())
            };
        let out = profile_discontinuities(
            table,
            &prep.scaffold_excluded,
            Some(&measures),
            axes_arg,
            &state.tier.disc,
        );
        if out.deadline_stopped {
            state.deadline_hit = true;
        }
        state.stamp(
            "discontinuities",
            if out.deadline_stopped {
                PassInfo::capped(format!("{}{}", out.coverage, prep.exclusion_note))
            } else {
                PassInfo::ran(format!("{}{}", out.coverage, prep.exclusion_note))
            },
        );
        state.findings.extend(out.findings);
    }
}

fn run_outliers_slices_disc_drift(
    state: &mut RunState<'_>,
    table: &TypedTable,
    ttable: &TypedTable,
    enc: &EncodedMatrix,
    prep: &PreparedFrame,
    model_basis: &ModelBasis,
) {
    if !state.check_deadline("outliers") && state.enabled("outliers") {
        let out = profile_surprises(table, &prep.scaffold_excluded, &state.tier.surprises);
        if out.deadline_stopped {
            state.deadline_hit = true;
        }
        state.stamp(
            "outliers",
            if out.deadline_stopped || out.work_capped {
                PassInfo::capped(format!("{}{}", out.coverage, prep.exclusion_note))
            } else {
                PassInfo::ran(format!("{}{}", out.coverage, prep.exclusion_note))
            },
        );
        state.findings.extend(out.findings);
    }

    if !state.check_deadline("slice_shifts") && state.enabled("slice_shifts") {
        let mut slice_cfg = state.tier.slices.clone();
        slice_cfg.designated_target = state
            .cfg
            .target
            .as_deref()
            .and_then(|t| table.names.iter().position(|n| n == t));
        let out = onceover_mines::profile_slice_shifts(table, &prep.scaffold_excluded, &slice_cfg);
        let info = match out.status {
            onceover_mines::SliceStatus::NotApplicable(d) => PassInfo::na(d),
            onceover_mines::SliceStatus::Ran(d) => {
                PassInfo::ran(format!("{d}{}", prep.exclusion_note))
            }
            onceover_mines::SliceStatus::Capped(d) => {
                PassInfo::capped(format!("{d}{}", prep.exclusion_note))
            }
        };
        state.stamp("slice_shifts", info);
        state.findings.extend(out.findings);
    }

    if !state.enabled("discontinuities") {
        state.check_deadline("discontinuities");
    } else {
        run_discontinuities_inner(state, table, prep);
    }

    if !state.check_deadline("drift") && state.enabled("drift") {
        let time_column = state
            .cfg
            .hints
            .as_ref()
            .and_then(|h| h.time.as_deref())
            .or(prep.frame.axis_name.as_deref());
        let (f, cov) = profile_drift(
            ttable,
            enc,
            time_column,
            &prep.scaffold_excluded,
            &state.tier.screen,
            &state.tier.drift,
            model_basis,
        );
        state.stamp(
            "drift",
            PassInfo::ran(format!("{cov}{}", prep.exclusion_note)),
        );
        state.findings.extend(f);
    }
}

fn run_assoc_hotspots_clusters(
    state: &mut RunState<'_>,
    table: &TypedTable,
    enc: &EncodedMatrix,
    prep: &PreparedFrame,
    weights: &ResolvedWeights,
) {
    if state.deadline_hit || state.check_deadline("association_rules") {
        state.deadline_hit = true;
        state.stamp("association_rules", PassInfo::skipped("deadline reached"));
    } else if state.enabled("association_rules") {
        let (f, cov) = association_rules(enc, &state.tier.association, &prep.scaffold_excluded);
        state.stamp(
            "association_rules",
            PassInfo::sampled(format!("{cov}{}", prep.exclusion_note)),
        );
        state.findings.extend(f);
        let (pf, pair_records, pcov) = onceover_mines::pair_associations(
            enc,
            table,
            &state.tier.association,
            &prep.scaffold_excluded,
        );
        state.stamp(
            "association_pairs",
            PassInfo::sampled(format!("{pcov}{}", prep.exclusion_note)),
        );
        state.findings.extend(pf);
        state.association_pairs.extend(pair_records);
    }

    let weight_series = weights.as_series();
    if state.enabled("hotspots") {
        let out = hotspots(enc, &state.tier.hotspots, &weight_series);
        if out.deadline_stopped {
            state.deadline_hit = true;
        }
        state.stamp(
            "hotspots",
            if out.deadline_stopped {
                PassInfo::capped(format!("{}{}", out.coverage, weights.note))
            } else {
                PassInfo::sampled(format!("{}{}", out.coverage, weights.note))
            },
        );
        state.findings.extend(out.findings);
    } else if state.deadline_hit || state.check_deadline("hotspots") {
        state.deadline_hit = true;
        state.stamp("hotspots", PassInfo::skipped("deadline reached"));
    }

    if !state.check_deadline("row_clusters") && state.enabled("row_clusters") {
        let (vf, vout) = onceover_mines::views_pass(table, &state.tier.views);
        let mut detail = vout.coverage.clone();
        if !vout.cap_dropped_columns.is_empty() {
            detail.push_str(&format!(
                "; dropped {} column(s) past the {}-column cap: {}",
                vout.cap_dropped_columns.len(),
                state.tier.views.max_columns,
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
        state.stamp(
            "row_clusters",
            if vout.capped {
                PassInfo::capped(detail)
            } else if table.n > state.tier.views.row_cap {
                PassInfo::sampled(detail)
            } else {
                PassInfo::ran(detail)
            },
        );
        state.findings.extend(vf);
    } else if state.deadline_hit {
        state.stamp("row_clusters", PassInfo::skipped("deadline reached"));
    }
}

/// Self-limiting passes dispatched even after the deadline expires so each
/// stops at its first poll and discloses `capped (0 examined)` in coverage.
fn run_deadline_self_limiting_passes(
    state: &mut RunState<'_>,
    table: &TypedTable,
    prep: &PreparedFrame,
    weights: &ResolvedWeights,
) {
    if state.enabled("discontinuities") {
        run_discontinuities_inner(state, table, prep);
    }
    if state.enabled("hotspots") {
        let (enc, _) = encode(table, &state.tier.encode);
        let weight_series = weights.as_series();
        let out = hotspots(&enc, &state.tier.hotspots, &weight_series);
        state.stamp(
            "hotspots",
            if out.deadline_stopped {
                PassInfo::capped(format!("{}{}", out.coverage, weights.note))
            } else {
                PassInfo::sampled(format!("{}{}", out.coverage, weights.note))
            },
        );
        state.findings.extend(out.findings);
    }
    if state.tier.dep_pair_cap > 0 && state.enabled("numeric_dependence") {
        let dep_out = onceover_mines::dep::screen_sampled(
            table,
            state.tier.dep_pair_cap,
            state.tier.encode.row_cap,
            &prep.scaffold_excluded,
            state.pass_deadline,
        );
        let dep_stopped = dep_out.deadline_stopped;
        let dep_pairs = dep_out.deps.len();
        state.association_pairs.extend(dep_out.records);
        state.findings.extend(dep_out.findings);
        let numeric = count_numeric_cols(table);
        let possible_pairs = numeric * numeric.saturating_sub(1) / 2;
        state.stamp(
            "dependence_screen",
            if dep_stopped || dep_pairs < possible_pairs {
                PassInfo::capped(dep_out.coverage)
            } else if table.n > state.tier.encode.row_cap {
                PassInfo::sampled(dep_out.coverage)
            } else {
                PassInfo::ran(dep_out.coverage)
            },
        );
    }
}

fn run_extensions(
    state: &mut RunState<'_>,
    table: &TypedTable,
    shared_encoding: Option<&EncodedMatrix>,
    extensions: &[&dyn crate::Detector],
) {
    for detector in extensions {
        let descriptor = detector.descriptor();
        if state.cfg.detectors.is_some() && !state.active.contains(&descriptor.id) {
            continue;
        }
        if state.check_deadline(&descriptor.id) {
            continue;
        }
        let context = crate::DetectorContext {
            table,
            columns: &state.columns,
            encoded: shared_encoding,
            config: state.cfg,
        };
        match detector.discover(&context) {
            Ok(mut candidates) => {
                let invalid = candidates.iter().find_map(|f| {
                    if !descriptor.categories.contains(&f.category) {
                        return Some("undeclared category".to_string());
                    }
                    if f.subject
                        .columns
                        .iter()
                        .any(|c| !state.columns.contains_key(c))
                    {
                        return Some("unknown subject column".to_string());
                    }
                    f.validate().err()
                });
                if let Some(reason) = invalid {
                    state.stamp(
                        &descriptor.id,
                        PassInfo::skipped(format!("invalid detector output: {reason}")),
                    );
                    continue;
                }
                for f in &mut candidates {
                    for e in &mut f.evidence {
                        e.method.detector = descriptor.id.clone();
                    }
                }
                state.stamp(
                    &descriptor.id,
                    PassInfo::ran(format!("{} candidate statements", candidates.len())),
                );
                state.findings.extend(candidates);
            }
            Err(message) => {
                state.stamp(
                    &descriptor.id,
                    PassInfo::skipped(format!("detector failed: {message}")),
                );
            }
        }
    }
}
