//! Structural, integrity, and exact-pass stage: row duplicates, keys & FDs,
//! frame account, basis phases B & C, duplicate encodings, pivot + wide-form,
//! sortedness, granularity, sentinels, missingness, content, boundaries,
//! order constraints, digits, dates, expectations, and contrasts.

use std::collections::{BTreeMap, BTreeSet};

use onceover_core::{find_duplicate_rows, TypedCol, TypedTable};
use onceover_deps::{analyze_keys, discover, sortedness, DepsOutcome};
use onceover_mines::analyze_temporal;
use onceover_report::{Category, EvidenceBasis, Finding, Method, PassInfo, Pattern};
use polars::prelude::DataFrame;
use serde_json::json;

use crate::config::Budget;

use super::helpers::{
    assign_timestamp_parts, build_wide_frame, quick_deps, scaffold_note, sync_card_roles,
};
use super::setup::{count_numeric_cols, PreparedFrame, ResolvedMetadata, ResolvedWeights};
use super::temporal::{run_panel_coverage, run_series_and_panels};
use super::{profile_inner, RunState};

pub(super) struct StructuralOutput {
    pub calendar_inputs: Vec<usize>,
    pub grouping_excluded: Vec<usize>,
    pub measure_excluded: Vec<usize>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run_structural_and_temporal(
    state: &mut RunState<'_>,
    df: &DataFrame,
    table: &TypedTable,
    ttable: &TypedTable,
    prep: &mut PreparedFrame,
    meta: &ResolvedMetadata,
    weights: &ResolvedWeights,
    extensions: &[&dyn crate::Detector],
    depth: u8,
) -> StructuralOutput {
    run_row_duplicates(state, table, prep);
    let deps = run_keys_and_fds(state, table, prep);
    run_frame_account(state, table, prep, &deps);
    run_basis_phase_b(state, table, prep, &deps);
    run_panel_coverage(state, table, prep);
    run_duplicate_encodings(state, table, prep);
    let (calendar_inputs, grouping_excluded, measure_excluded) =
        run_basis_phase_c(state, table, prep);
    run_pivot_and_wide(state, df, table, prep, extensions, depth);
    run_sortedness(state, table, prep);
    run_granularity(state, table);
    run_sentinels(state, table, prep);
    run_missingness(state, table, prep);
    run_series_and_panels(state, table, prep, meta, weights);
    run_content_and_boundaries(state, table);
    run_order_constraints(state, table, prep, &deps);
    run_digits_and_dates(state, table, ttable);
    run_expectations_and_contrasts(state, table, prep, &grouping_excluded, &measure_excluded);
    StructuralOutput {
        calendar_inputs,
        grouping_excluded,
        measure_excluded,
    }
}

fn run_row_duplicates(state: &mut RunState<'_>, table: &TypedTable, prep: &PreparedFrame) {
    if state.check_deadline("row_duplicates") {
        return;
    }
    let any_comparable = table
        .cols
        .iter()
        .any(|c| !matches!(c, TypedCol::Null { .. } | TypedCol::Other { .. }));
    if !state.enabled("row_duplicates") {
        return;
    }
    if !any_comparable {
        state.stamp("row_duplicates", PassInfo::na("no comparable columns"));
        return;
    }
    let dup_opt = if prep
        .info
        .iter()
        .any(|c| c.null_count == 0 && c.distinct == table.n)
    {
        None
    } else {
        find_duplicate_rows(table)
    };
    match dup_opt {
        Some(dup) => {
            state.stamp(
                "row_duplicates",
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
            let comparable: Vec<usize> = table
                .cols
                .iter()
                .enumerate()
                .filter(|(_, col)| !matches!(col, TypedCol::Null { .. } | TypedCol::Other { .. }))
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
            let distinct_rows = table.n - dup.n_redundant;
            let support: f64 = comparable
                .iter()
                .map(|&c| {
                    (prep.info[c].distinct + usize::from(prep.info[c].null_count > 0)).max(1) as f64
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
            let comp_names: Vec<String> =
                comparable.iter().map(|&c| table.names[c].clone()).collect();
            if saturated {
                state.findings.push(Finding::new(
                    Pattern::ValueConcentration,
                    BTreeMap::from([("columns".into(), json!(comp_names))]),
                    format!(
                        "the compared columns form only {} distinct rows in {} rows: their {} possible combinations are nearly all realised, so repeated rows are expected from the small support",
                        distinct_rows, table.n, support as u64
                    ),
                    EvidenceBasis::Measured,
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
                state.findings.push(Finding::new(
                    Pattern::Duplicates,
                    BTreeMap::from([("columns".into(), json!(comp_names))]),
                    format!(
                        "{} rows repeat earlier values across the compared columns ({} groups, largest {})",
                        dup.n_redundant, dup.n_groups, dup.max_group_size
                    ),
                    EvidenceBasis::Measured,
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
            state.stamp(
                "row_duplicates",
                PassInfo::ran("no exact duplicate rows (exact)"),
            );
        }
    }
}

fn run_keys_and_fds(
    state: &mut RunState<'_>,
    table: &TypedTable,
    prep: &PreparedFrame,
) -> DepsOutcome {
    let (deps, key_findings): (DepsOutcome, Vec<Finding>) =
        if state.cfg.budget == Budget::Quick || !state.enabled("functional_dependencies") {
            let d = if state.cfg.budget == Budget::Quick {
                quick_deps(&prep.info, table.n)
            } else if state.enabled("keys") {
                onceover_deps::fd::discover_keys(table, &prep.info, &state.tier.fd)
            } else {
                DepsOutcome::default()
            };
            let reason = if state.cfg.budget == Budget::Quick {
                "quick tier"
            } else if state.enabled("keys") {
                "functional dependencies not enabled; key discovery only"
            } else {
                "detector not enabled"
            };
            state.stamp("functional_dependencies", PassInfo::skipped(reason));
            let k = analyze_keys(&prep.info, &d);
            (d, k.findings)
        } else if state.check_deadline("functional_dependencies") {
            (DepsOutcome::default(), Vec::new())
        } else {
            let mut d = discover(table, &prep.info, &state.tier.fd, &prep.scaffold_excluded);
            state.stamp(
                "functional_dependencies",
                if !d.caps_bound.is_empty() {
                    PassInfo::capped(format!("{}{}", d.coverage, prep.exclusion_note))
                } else if d.sampled {
                    PassInfo::sampled(format!("{}{}", d.coverage, prep.exclusion_note))
                } else {
                    PassInfo::ran(format!("{}{}", d.coverage, prep.exclusion_note))
                },
            );
            let k = analyze_keys(&prep.info, &d);
            state.findings.extend(std::mem::take(&mut d.findings));
            (d, k.findings)
        };

    let key_findings = if state.enabled("keys") {
        key_findings
    } else {
        Vec::new()
    };
    let key_detail = format!(
        "{} single-column unique, {} composite",
        deps.single_unique.len(),
        deps.composite_uccs.len()
    );
    if !key_findings.is_empty() {
        state.stamp("keys", PassInfo::ran(key_detail));
        state.findings.extend(key_findings);
    } else {
        state.stamp("keys", PassInfo::na(key_detail));
    }
    deps
}

fn run_frame_account(
    state: &mut RunState<'_>,
    table: &TypedTable,
    prep: &mut PreparedFrame,
    deps: &DepsOutcome,
) {
    let account = crate::account::compute_account(table, &prep.frame, &prep.card_facts, deps);
    if !account.replaced_key_subjects.is_empty() {
        let replaced = &account.replaced_key_subjects;
        state.findings.retain(|f| {
            !(f.category == Category::Uniqueness
                && f.evidence.iter().any(|e| e.method.detector == "keys")
                && replaced
                    .iter()
                    .any(|s| *s == f.subject.columns.iter().cloned().collect()))
        });
    }
    state.findings.extend(account.findings);
    let mut detail = String::new();
    match &account.grain {
        Some(g) => detail.push_str(&format!("grain: {}", g.statement)),
        None if account.grain_unresolved => detail
            .push_str("grain: no exact key found among the dimension columns and the time axis"),
        None => detail.push_str("grain: no key candidates (no time axis or dimension columns)"),
    }
    if !account.replaced_key_subjects.is_empty() {
        detail.push_str("; the grain finding replaces the bare key finding for the same columns");
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
    prep.frame.info.grain = account.grain;
    prep.frame.info.dominant = account.dominant;
    state.stamp("frame_model", PassInfo::ran(detail));
}

fn run_basis_phase_b(
    state: &mut RunState<'_>,
    table: &TypedTable,
    prep: &mut PreparedFrame,
    deps: &DepsOutcome,
) {
    crate::basis::close_and_sub_period(
        &mut prep.basis,
        table,
        &prep.frame,
        &prep.card_facts,
        &deps.exact_fds,
        state.cfg.hints.as_ref(),
    );
    assign_timestamp_parts(
        &mut prep.frame.roles,
        &mut prep.frame.info.roles,
        &prep.basis.axis_derived,
    );
    assign_timestamp_parts(
        &mut prep.frame.roles,
        &mut prep.frame.info.roles,
        &prep.basis.sub_period,
    );
    sync_card_roles(&mut state.columns, table, &prep.frame);
    let family: BTreeSet<String> = prep
        .basis
        .calendar_family()
        .into_iter()
        .map(|i| table.names[i].clone())
        .collect();
    if family.len() > prep.basis.axis_cols.len() {
        state.findings.retain(|f| {
            !((f.evidence
                .iter()
                .any(|e| e.method.detector == "functional_dependencies")
                || f.finding_type.as_deref() == Some("categorical_role"))
                && f.subject.columns.iter().all(|c| family.contains(c)))
        });
    }
    for i in prep.basis.not_subject() {
        if !prep.scaffold_excluded.contains(&i) {
            prep.scaffold_excluded.push(i);
        }
    }
    prep.scaffold_excluded.sort_unstable();
    prep.exclusion_note = scaffold_note(
        table,
        &prep.frame,
        &prep.info,
        &prep.scaffold_excluded,
        &prep.axis_excluded,
        &prep.basis,
    );
    if let [axis] = prep.basis.axis_cols[..] {
        if matches!(
            table.cols[axis],
            TypedCol::Date { .. } | TypedCol::Datetime { .. }
        ) {
            state.tier.encode.calendar_axis = Some(axis);
        }
    }
    if !prep.basis.axis_derived.is_empty() {
        let mut related = prep.basis.axis_cols.clone();
        related.extend(prep.basis.axis_derived.iter().map(|d| d.col));
        state.tier.hotspots.related_columns = vec![related];
    }
}

fn run_duplicate_encodings(state: &mut RunState<'_>, table: &TypedTable, prep: &PreparedFrame) {
    if state.check_deadline("duplicate_encodings") {
        return;
    }
    if state.enabled("duplicate_encodings") {
        let ((f, cov), stopped) =
            onceover_deps::profile_duplicate_encodings_stopped(table, &prep.info, &state.tier.dupe);
        if stopped {
            state.deadline_hit = true;
        }
        state.stamp(
            "duplicate_encodings",
            if stopped {
                PassInfo::capped(cov)
            } else {
                PassInfo::ran(cov)
            },
        );
        state.findings.extend(f);
    }
}

fn run_basis_phase_c(
    state: &mut RunState<'_>,
    table: &TypedTable,
    prep: &mut PreparedFrame,
) -> (Vec<usize>, Vec<usize>, Vec<usize>) {
    let family: BTreeSet<String> = prep
        .basis
        .calendar_family()
        .into_iter()
        .map(|i| table.names[i].clone())
        .collect();
    if family.len() > prep.basis.axis_cols.len() {
        state.findings.retain(|f| {
            !(f.finding_type.as_deref() == Some("duplicate_encoding")
                && f.subject.columns.iter().all(|c| family.contains(c)))
        });
    }
    prep.basis.recodings = crate::basis::recodings(table, &state.findings);
    if !prep.basis.recodings.is_empty() {
        let named: Vec<String> = prep
            .basis
            .recodings
            .iter()
            .map(|(d, k)| format!("{} (recoding of {})", table.names[*d], table.names[*k]))
            .collect();
        if let Some(entry) = state.coverage.get_mut("duplicate_encodings") {
            entry.detail.push_str(&format!(
                "; excluded from later passes as redundant: {}",
                named.join(", ")
            ));
        }
        for i in prep.basis.not_subject() {
            if !prep.scaffold_excluded.contains(&i) {
                prep.scaffold_excluded.push(i);
            }
        }
        prep.scaffold_excluded.sort_unstable();
        state.tier.encode.skip_columns = prep.basis.not_input();
        prep.exclusion_note = scaffold_note(
            table,
            &prep.frame,
            &prep.info,
            &prep.scaffold_excluded,
            &prep.axis_excluded,
            &prep.basis,
        );
    }

    let calendar_inputs: Vec<usize> = prep
        .basis
        .axis_derived
        .iter()
        .chain(prep.basis.sub_period.iter())
        .map(|d| d.col)
        .collect();
    let grouping_excluded: Vec<usize> = prep
        .scaffold_excluded
        .iter()
        .copied()
        .filter(|i| !calendar_inputs.contains(i))
        .collect();
    let mut measure_excluded = prep.scaffold_excluded.clone();
    for (i, r) in prep.frame.roles.iter().enumerate() {
        if matches!(r.as_deref(), Some("flag" | "dimension")) && !measure_excluded.contains(&i) {
            measure_excluded.push(i);
        }
    }
    measure_excluded.sort_unstable();
    (calendar_inputs, grouping_excluded, measure_excluded)
}

fn run_pivot_and_wide(
    state: &mut RunState<'_>,
    df: &DataFrame,
    table: &TypedTable,
    prep: &PreparedFrame,
    extensions: &[&dyn crate::Detector],
    depth: u8,
) {
    let mut pivot_spec: Option<onceover_deps::PivotSpec> = None;
    if state.enabled("pivot") {
        let p = onceover_deps::analyze_pivot(
            table,
            &prep.info,
            &onceover_deps::PivotConfig {
                max_levels: state.tier.pivot_max_levels,
                max_findings: state.tier.pivot_max_findings,
                ..Default::default()
            },
        );
        if p.checked == 0 {
            state.stamp(
                "pivot",
                PassInfo::na(
                    "needs exactly one continuous measure column and a categorical attribute candidate"
                        .to_string(),
                ),
            );
        } else {
            state.stamp(
                "pivot",
                PassInfo::ran(format!(
                    "{} attribute candidates checked, {} verified jointly unique",
                    p.checked, p.verified
                )),
            );
        }
        pivot_spec = p.specs.first().cloned();
        state.findings.extend(p.findings);
        if !p.reported.is_empty() {
            for f in &mut state.findings {
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

    if depth == 0 {
        match &pivot_spec {
            None => {}
            Some(_) if state.cfg.budget == Budget::Quick => {
                state.stamp(
                    "pivot_profile",
                    PassInfo::na("quick budget: the pivoted wide form is not profiled".to_string()),
                );
            }
            Some(spec) => {
                let wide_cols = spec.index.len() + spec.levels.len();
                if wide_cols > 64 || spec.n_index < 8 {
                    state.stamp(
                        "pivot_profile",
                        PassInfo::na(format!(
                            "pivoted form would be {} rows x {} columns; outside the wide-profile limits (at least 8 index tuples, at most 64 columns)",
                            spec.n_index, wide_cols
                        )),
                    );
                } else if state
                    .cfg
                    .deadline
                    .is_some_and(|d| state.start.elapsed() > d.mul_f64(0.6))
                {
                    state.stamp(
                        "pivot_profile",
                        PassInfo::skipped(
                            "most of the deadline was spent on the long form; the wide form was not profiled"
                                .to_string(),
                        ),
                    );
                } else {
                    match build_wide_frame(df, table, spec) {
                        Some(wide) => {
                            let mut cfg2 = state.cfg.clone();
                            cfg2.target = None;
                            cfg2.deadline = state
                                .cfg
                                .deadline
                                .map(|d| d.saturating_sub(state.start.elapsed()));
                            let wide_report = profile_inner(&wide, &cfg2, extensions, depth + 1);
                            let n_wide =
                                wide_report.findings.len() + wide_report.grouped_findings.len();
                            for (pass, info) in &wide_report.coverage {
                                state
                                    .coverage
                                    .entry(format!("pivot_profile/{pass}"))
                                    .or_insert_with(|| info.clone());
                            }
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
                                state.findings.push(f);
                            }
                            state.stamp(
                                "pivot_profile",
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
                            state.stamp(
                                "pivot_profile",
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
}

fn run_sortedness(state: &mut RunState<'_>, table: &TypedTable, prep: &PreparedFrame) {
    if !state.check_deadline("sortedness") && state.enabled("sortedness") {
        let derived: BTreeSet<String> = prep
            .basis
            .not_subject()
            .into_iter()
            .map(|i| table.names[i].clone())
            .collect();
        let sorted: Vec<Finding> = sortedness(table)
            .into_iter()
            .filter(|f| !f.subject.columns.iter().any(|c| derived.contains(c)))
            .collect();
        state.stamp(
            "sortedness",
            PassInfo::ran(format!("{} columns sorted by row order", sorted.len())),
        );
        state.findings.extend(sorted);
    }
}

fn run_granularity(state: &mut RunState<'_>, table: &TypedTable) {
    if !state.check_deadline("granularity") && state.enabled("granularity") {
        let gran = onceover_deps::granularity_changes(table);
        let n_f64 = table
            .cols
            .iter()
            .filter(|c| matches!(c, TypedCol::F64 { .. }))
            .count();
        state.stamp(
            "granularity",
            PassInfo::ran(format!(
                "{n_f64} float columns scanned for granularity changes"
            )),
        );
        state.findings.extend(gran);
    }
}

fn run_sentinels(state: &mut RunState<'_>, table: &TypedTable, prep: &PreparedFrame) {
    if !state.check_deadline("sentinels") && state.enabled("sentinels") {
        let sent =
            onceover_deps::sentinels::sentinel_values_for_columns(table, &prep.sentinel_candidates);
        state.stamp(
            "sentinels",
            PassInfo::ran(format!(
                "{} numeric columns scanned for sentinel values",
                count_numeric_cols(table)
            )),
        );
        state.findings.extend(sent);
    }
}

fn run_missingness(state: &mut RunState<'_>, table: &TypedTable, prep: &PreparedFrame) {
    if !state.check_deadline("missingness") && state.enabled("missingness") {
        let axis_name = prep.frame.info.axis.as_ref().map(|a| a.name.as_str());
        let axis = match (prep.axis_ticks.as_deref(), axis_name) {
            (Some(ticks), Some(name)) => Some(onceover_mines::MissingnessAxis { name, ticks }),
            _ => None,
        };
        let indicator_cap = match state.cfg.budget {
            Budget::Quick => 0,
            Budget::Standard => 8,
            Budget::Deep => 16,
        };
        let explainers: Vec<usize> = prep
            .scaffold_excluded
            .iter()
            .copied()
            .filter(|i| !prep.basis.indicators.iter().any(|(f, _)| f == i))
            .collect();
        let (mut f, detail) =
            onceover_mines::profile_missingness(table, &explainers, axis, indicator_cap);
        state.stamp("missingness", PassInfo::ran(detail));
        state.findings.append(&mut f);
    }
}

fn run_content_and_boundaries(state: &mut RunState<'_>, table: &TypedTable) {
    if !state.check_deadline("content_anomalies") && state.enabled("content_anomalies") {
        let mut f = onceover_deps::column_content(table);
        state.stamp(
            "content_anomalies",
            PassInfo::ran(format!(
                "{} columns scanned for content; {} statements",
                table.cols.len(),
                f.len()
            )),
        );
        state.findings.append(&mut f);
    }
    if !state.check_deadline("boundary_mass") && state.enabled("boundary_mass") {
        let bnd = onceover_deps::boundary_mass(table);
        state.stamp(
            "boundary_mass",
            PassInfo::ran(format!(
                "{} numeric columns scanned for boundary mass",
                count_numeric_cols(table)
            )),
        );
        state.findings.extend(bnd);
    }
}

fn run_order_constraints(
    state: &mut RunState<'_>,
    table: &TypedTable,
    prep: &PreparedFrame,
    deps: &DepsOutcome,
) {
    if state.check_deadline("order_constraints") || !state.enabled("order_constraints") {
        return;
    }
    if state.tier.dep_pair_cap == 0 {
        state.stamp(
            "order_constraints",
            PassInfo::skipped("quick tier: the shared pair budget (max_pairs) is 0 at this tier"),
        );
        return;
    }
    let derived_pairs: BTreeSet<(usize, usize)> = deps
        .exact_fds
        .iter()
        .filter(|(lhs, _)| lhs.len() == 1)
        .map(|(lhs, rhs)| (lhs[0].min(*rhs), lhs[0].max(*rhs)))
        .collect();
    let mut oc_excluded: BTreeSet<usize> = prep
        .frame
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
    let oc_axis: Vec<String> = prep
        .axis_excluded
        .iter()
        .filter(|i| !oc_excluded.contains(i))
        .map(|&i| table.names[i].clone())
        .collect();
    oc_excluded.extend(prep.axis_excluded.iter().copied());
    let out = onceover_deps::order_constraints(
        table,
        &onceover_deps::OrderConfig {
            max_pairs: state.tier.dep_pair_cap,
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
    let info = if out.pairs_screened < out.pairs_total || out.pairs_evaluated < out.pairs_surviving
    {
        PassInfo::capped(detail)
    } else if out.screened {
        PassInfo::sampled(format!(
            "{detail}; pairs screened on a deterministic row sample and verified on all rows"
        ))
    } else {
        PassInfo::ran(detail)
    };
    state.stamp("order_constraints", info);
    state.findings.extend(out.findings);
}

fn run_digits_and_dates(state: &mut RunState<'_>, table: &TypedTable, ttable: &TypedTable) {
    if !state.check_deadline("digit_fingerprints") && state.enabled("digit_fingerprints") {
        let (dig, cov) = onceover_deps::digit_fingerprints(table);
        state.stamp("digit_fingerprints", PassInfo::ran(cov));
        state.findings.extend(dig);
    }
    if !state.check_deadline("date_gaps") && state.enabled("date_gaps") {
        let declared_skip = state
            .cfg
            .hints
            .as_ref()
            .map(|h| onceover_mines::expectations::declared_temporal_columns(table, h))
            .unwrap_or_default();
        let d = analyze_temporal(ttable, &declared_skip);
        state.stamp("date_gaps", PassInfo::ran(d.coverage.clone()));
        for (name, modal) in d.modal_intervals {
            if let Some(card) = state.columns.get_mut(&name) {
                if let Some(ts) = card.temporal_stats.as_mut() {
                    ts.modal_interval_days = Some(modal);
                }
            }
        }
        state.findings.extend(d.findings);
    }
}

fn run_expectations_and_contrasts(
    state: &mut RunState<'_>,
    table: &TypedTable,
    prep: &PreparedFrame,
    grouping_excluded: &[usize],
    measure_excluded: &[usize],
) {
    if !state.check_deadline("expectations") && state.enabled("expectations") {
        match state.cfg.hints.as_ref() {
            Some(hints) if !hints.is_empty() => {
                let e = onceover_mines::expectations::analyze_expectations(table, hints);
                state.stamp("expectations", PassInfo::ran(e.coverage.clone()));
                state.findings.extend(e.findings);
            }
            _ => state.stamp("expectations", PassInfo::skipped("no hints provided")),
        }
    }

    if !state.check_deadline("group_contrast") && state.enabled("group_contrast") {
        match state.cfg.hints.as_ref().and_then(|h| h.contrast.as_ref()) {
            Some(spec) => {
                let out = onceover_mines::group_contrast(table, spec);
                match out.skipped {
                    Some(reason) => state.stamp("group_contrast", PassInfo::skipped(reason)),
                    None => {
                        state.stamp("group_contrast", PassInfo::ran(out.detail));
                        state.findings.extend(out.findings);
                    }
                }
            }
            None => state.stamp(
                "group_contrast",
                PassInfo::skipped("no group contrast declared"),
            ),
        }
    }

    if !state.check_deadline("level_contrasts") && state.enabled("level_contrasts") {
        let axis_name = prep.frame.info.axis.as_ref().map(|a| a.name.as_str());
        let target_exclusion: Vec<&str> = match &state.cfg.target {
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
        let axis_derived: Vec<usize> = prep.basis.axis_derived.iter().map(|d| d.col).collect();
        let input = onceover_mines::LevelContrastInput {
            axis_name,
            axis_ticks: prep.axis_ticks.as_deref(),
            exclude_measures: &target_exclusion,
            exclude_columns: grouping_excluded,
            non_measures: measure_excluded,
            axis_derived: &axis_derived,
        };
        let out = onceover_mines::profile_level_contrasts(table, &input);
        state.stamp("level_contrasts", PassInfo::ran(out.detail));
        state.findings.extend(out.findings);
    }
}
