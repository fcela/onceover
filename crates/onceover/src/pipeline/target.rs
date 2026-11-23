//! Target-triggered passes: outcome account, outcome contrasts, target shapes,
//! leakage signatures, and model-failure slices.

use onceover_core::TypedTable;
use onceover_mines::{
    encode, profile_target_shapes, screen_leakage_with_entity, screen_model_failures,
    EncodedMatrix, ModelBasis,
};
use onceover_report::PassInfo;

use super::setup::{PreparedFrame, ResolvedMetadata};
use super::RunState;

pub(super) fn run_target_passes(
    state: &mut RunState<'_>,
    table: &TypedTable,
    prep: &PreparedFrame,
    meta: &mut ResolvedMetadata,
    grouping_excluded: &[usize],
    model_basis: &ModelBasis,
    shared_encoding: Option<&EncodedMatrix>,
) {
    // ---------- outcome account and contrasts (issue #310) ----------
    if !state.check_deadline("outcome_account")
        && (state.enabled("outcome_account") || state.enabled("outcome_contrasts"))
    {
        match state.cfg.target.clone() {
            Some(target) => {
                let target_is_categorical = table
                    .names
                    .iter()
                    .position(|n| n == &target)
                    .is_some_and(|tci| {
                        let role = prep.frame.roles.get(tci).and_then(|r| r.as_deref());
                        let declared_cat = state
                            .cfg
                            .hints
                            .as_ref()
                            .and_then(|h| h.roles.get(&target))
                            .is_some_and(|r| matches!(r.as_str(), "flag" | "dimension"));
                        role == Some("flag") || declared_cat
                    });
                meta.outcome_decl.target_is_categorical = target_is_categorical;
                if let Some(event) = &meta.outcome_decl.event {
                    match onceover_mines::target_event_label_with_role(
                        table,
                        &target,
                        event,
                        target_is_categorical,
                    ) {
                        Some(label) => {
                            meta.outcome_decl.event_label = Some(label.clone());
                            state.decl_notes.push(format!(
                                "event: '{label}' checked — it is a level of the target '{target}'; the outcome contrasts state event rates against it"
                            ));
                        }
                        None => {
                            state.decl_notes.push(format!(
                                "event: {event} refused — it does not name a level of the target '{target}' (a continuous numeric target has no levels); the outcome contrasts take the per-level form and no event rate is stated"
                            ));
                        }
                    }
                }
                meta.outcome_decl.excluded = grouping_excluded.to_vec();
                for &idx in &meta.recorded_after_idx {
                    if !meta.outcome_decl.excluded.contains(&idx) {
                        meta.outcome_decl.excluded.push(idx);
                    }
                }
                if state.enabled("outcome_account") {
                    let (f, cov) =
                        onceover_mines::outcome_account(table, &target, &meta.outcome_decl);
                    state.stamp("outcome_account", PassInfo::ran(cov));
                    state.findings.extend(f);
                }
                if state.enabled("outcome_contrasts") {
                    let axis = prep
                        .frame
                        .info
                        .axis
                        .as_ref()
                        .map(|a| a.name.as_str())
                        .zip(prep.axis_ticks.as_deref());
                    let (f, cov) =
                        onceover_mines::outcome_contrasts(table, &target, &meta.outcome_decl, axis);
                    state.stamp("outcome_contrasts", PassInfo::ran(cov));
                    state.findings.extend(f);
                }
            }
            None => {
                for id in ["outcome_account", "outcome_contrasts"] {
                    if state.enabled(id) {
                        state.stamp(id, PassInfo::skipped("no target designated".to_string()));
                    }
                }
            }
        }
    }

    // ---------- target-aware shapes ----------
    if !state.check_deadline("target_shapes") && state.enabled("target_shapes") {
        match state.cfg.target.clone() {
            Some(target) => {
                let (f, cov) =
                    profile_target_shapes(table, &target, &state.tier.shapes, model_basis);
                state.stamp("target_shapes", PassInfo::ran(cov));
                state.findings.extend(f);
            }
            None => state.stamp(
                "target_shapes",
                PassInfo::skipped("no target designated".to_string()),
            ),
        }
    }

    // ---------- leakage signatures ----------
    if !state.check_deadline("leakage") && state.enabled("leakage") {
        match state.cfg.target.clone() {
            Some(target) => {
                let declared_entity = meta
                    .declared_entity_idx
                    .map(|idx| table.names[idx].as_str());
                let (f, cov) = screen_leakage_with_entity(
                    table,
                    &target,
                    &meta.recorded_after_names,
                    declared_entity,
                );
                state.stamp("leakage", PassInfo::ran(cov));
                state.findings.extend(f);
            }
            None => state.stamp(
                "leakage",
                PassInfo::skipped("no target designated".to_string()),
            ),
        }
    }

    // ---------- model-failure slices ----------
    if !state.check_deadline("model_failures") && state.enabled("model_failures") {
        match state.cfg.target.clone() {
            Some(target) => {
                let local_enc;
                let enc = match shared_encoding {
                    Some(enc) => enc,
                    None => {
                        local_enc = encode(table, &state.tier.encode).0;
                        &local_enc
                    }
                };
                let mut mf_excluded = grouping_excluded.to_vec();
                if let Some(ti) = table.names.iter().position(|n| n == &target) {
                    for c in model_basis.excluded_for(ti) {
                        if !mf_excluded.contains(&c) {
                            mf_excluded.push(c);
                        }
                    }
                }
                for &idx in &meta.recorded_after_idx {
                    if !mf_excluded.contains(&idx) {
                        mf_excluded.push(idx);
                    }
                }
                let (f, cov) = screen_model_failures(
                    table,
                    enc,
                    &target,
                    &state.tier.screen,
                    &state.tier.modelfail,
                    &mf_excluded,
                );
                let cov = if meta.recorded_after_names.is_empty() {
                    cov
                } else {
                    format!(
                        "{cov}; declared recorded-after column(s) excluded from the model inputs: {}",
                        meta.recorded_after_names.join(", ")
                    )
                };
                state.stamp("model_failures", PassInfo::ran(cov));
                state.findings.extend(f);
            }
            None => state.stamp(
                "model_failures",
                PassInfo::skipped("no target designated".to_string()),
            ),
        }
    }
}
