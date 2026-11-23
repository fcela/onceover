//! Final stage: evidence scope normalization, rule evaluation, coverage
//! completion, frame-model demotions, declared questions, ledger selection,
//! labels/gloss presentation, overview, and `ProfileReport` assembly.

use std::collections::{BTreeMap, BTreeSet};

use onceover_core::TypedTable;
use onceover_report::{
    group_findings, select_findings_counted, Category, Finding, PassInfo, ProfileReport, Sampling,
    SelectionSummary, SCHEMA_VERSION,
};

use crate::config::Budget;
use crate::detectors::{Membership, Trigger};

use super::helpers::build_overview;
use super::setup::PreparedFrame;
use super::RunState;

pub(super) fn finalize_report(
    mut state: RunState<'_>,
    table: &TypedTable,
    prep: PreparedFrame,
    encoding_sampled: bool,
) -> ProfileReport {
    let activations: BTreeMap<String, (Membership, Trigger)> =
        crate::detectors::builtin_detectors()
            .into_iter()
            .map(|d| (d.id, (d.membership, d.trigger)))
            .collect();

    let selected_ids = state.cfg.detectors.as_ref();
    state.findings.retain(|f| {
        f.evidence
            .iter()
            .any(|e| selected_ids.is_none_or(|ids| ids.contains(&e.method.detector)))
    });
    for f in &mut state.findings {
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
                        if encoding_sampled {
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

    let candidate_count = state.findings.len();
    let rule_candidates = state
        .findings
        .iter()
        .filter(|f| f.category == Category::ConditionalRules)
        .count();
    crate::rules::evaluate_rules(
        table,
        &mut state.findings,
        state.tier.encode.row_cap,
        &state.cfg.selection,
    );
    let rules_kept = state
        .findings
        .iter()
        .filter(|f| f.category == Category::ConditionalRules)
        .count();
    if rule_candidates > 0 {
        state.stamp(
            "rule_evaluation",
            PassInfo::sampled(format!(
                "{rules_kept} of {rule_candidates} candidates passed a common diagnostic partition; support and baseline evaluated on complete predicates and outcome; reused for selection"
            )),
        );
    }

    let verified_count = state.findings.len();
    let distinct_count = state
        .findings
        .iter()
        .map(Finding::canonical_key)
        .collect::<BTreeSet<_>>()
        .len();

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
        if !state.enabled(id) {
            let detail = match activations.get(id) {
                Some((_, Trigger::Target)) => {
                    "runs when a target is designated, or when explicitly selected together with a target".to_string()
                }
                Some((Membership::OnRequest, _)) => format!(
                    "on-request detector; not in the default set — request it with detectors=[\"{id}\"]"
                ),
                _ => "detector not enabled".to_string(),
            };
            match state.coverage.get_mut(pass) {
                Some(info) if info.status == "skipped" && info.detail == "deadline reached" => {
                    info.detail = detail;
                }
                Some(_) => {}
                None => state.stamp(pass, PassInfo::skipped(detail)),
            }
        } else {
            if !state.coverage.contains_key(pass) {
                let detail = if state.cfg.budget == Budget::Quick {
                    "quick tier"
                } else if state.deadline_hit {
                    "deadline reached"
                } else {
                    "not run: the passes it runs after did not run on this table"
                };
                state.stamp(pass, PassInfo::skipped(detail));
            }
            if !state.selected(id) {
                if let Some(info) = state.coverage.get_mut(pass) {
                    info.detail
                        .push_str("; required as a resource; findings not selected");
                }
            }
        }
    }

    let demotions = crate::account::demote_restatements(&mut state.findings, table, &prep.frame);
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
        if let Some(info) = state.coverage.get_mut("frame_model") {
            info.detail.push_str("; ");
            info.detail.push_str(note);
        }
    }

    let declared_questions: Vec<String> = state
        .cfg
        .hints
        .as_ref()
        .map(|h| h.questions.clone())
        .unwrap_or_default();
    if !declared_questions.is_empty() {
        let lowered: Vec<String> = declared_questions
            .iter()
            .map(|q| q.to_lowercase())
            .collect();
        for finding in state.findings.iter_mut() {
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

    let outcome =
        select_findings_counted(std::mem::take(&mut state.findings), &state.cfg.selection);
    let mut findings = outcome.selected;
    let mut grouped = outcome.grouped;

    if let Some(source) = &state.cfg.source {
        if source.basis == "sampled" {
            for finding in findings.iter_mut().chain(grouped.iter_mut()) {
                for evidence in &mut finding.evidence {
                    evidence.scope.sampling = Sampling::Sample;
                    evidence.scope.population_rows = Some(source.rows);
                }
            }
        }
    }

    let resolved_labels = state.cfg.hints.as_ref().and_then(|hints| {
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
        let declared = state.cfg.hints.as_ref().map_or(0, |h| h.labels.len());
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
        state.stamp("labels", PassInfo::ran(detail));
    }

    if let Some(hints) = &state.cfg.hints {
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
                state.decl_notes.push(format!(
                    "gloss stated in the prose of {applied} selected finding(s) whose subject is a glossed column"
                ));
            }
        }
    }

    if prep.frame.info.declarations.is_some() || !state.decl_notes.is_empty() {
        let joined = state.decl_notes.join("; ");
        state.stamp("declarations", PassInfo::ran(joined));
    }

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
        state.stamp(
            "questions",
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
        let mut parts: Vec<String> = capped
            .iter()
            .map(|(detector, n)| {
                let cap = state
                    .cfg
                    .selection
                    .max_findings_per_detector
                    .get(detector)
                    .or_else(|| state.cfg.selection.pressure_detector_caps.get(detector))
                    .copied()
                    .unwrap_or(0);
                format!("{detector}: {n} statement(s) past the per-detector report cap of {cap} not selected")
            })
            .collect();
        parts.extend(outcome.group_capped.iter().map(|(label, n)| {
            format!(
                "{label}: {n} statement(s) past the per-group report cap of {} not selected",
                state.cfg.selection.max_findings_per_group
            )
        }));
        parts.extend(
            outcome
                .group_capped_by_detector
                .iter()
                .map(|(detector, n)| {
                    format!(
                        "{detector}: {n} statement(s) past the per-group report cap of {} not selected",
                        state.cfg.selection.max_findings_per_group
                    )
                }),
        );
        if outcome.reserve_capped > 0 {
            parts.push(format!(
                "hygiene class: {} statement(s) past the hygiene reserve of {} not selected",
                outcome.reserve_capped, state.cfg.selection.hygiene_reserve
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
            PassInfo::ran(parts.join("; "))
        } else {
            PassInfo::capped(parts.join("; "))
        };
        state.stamp("selection", info);
    }

    let groups = group_findings(&findings);
    let selection = SelectionSummary {
        candidates: candidate_count,
        verified_candidates: verified_count,
        distinct_statements: distinct_count,
        selected: findings.len(),
        families: outcome.families,
        grouped_statements: grouped.len(),
        candidates_by_class: outcome.candidates_by_class,
        selected_by_class: outcome.selected_by_class,
        capped_by_class: outcome.capped_by_class,
    };
    let applied_labels: BTreeMap<String, String> = resolved_labels
        .as_ref()
        .map(|r| r.applied.clone())
        .unwrap_or_default();
    let mut overview = build_overview(table, &state.columns, &findings, state.cfg, &applied_labels);
    if let Some(source) = &state.cfg.source {
        if source.basis == "sampled" {
            overview.headline.push_str(&format!(
                "; profiled on a stratified sample from {} source rows",
                source.rows
            ));
        }
    }
    if state.deadline_hit {
        let elapsed = state.start.elapsed().as_secs_f64();
        state.stamp(
            "deadline",
            PassInfo::skipped(format!(
                "wall-clock deadline {elapsed:.1}s reached; later passes skipped"
            )),
        );
    }

    ProfileReport {
        schema_version: SCHEMA_VERSION.into(),
        frame: Some(prep.frame.info),
        overview,
        columns: state.columns,
        findings,
        grouped_findings: grouped,
        groups,
        selection,
        source: state.cfg.source.clone(),
        coverage: state.coverage,
        associations: state.association_pairs,
    }
}
