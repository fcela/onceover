//! Temporal, panel, and entity-fingerprint passes, sharing the once-computed
//! per-row axis ticks from `PreparedFrame`.

use onceover_core::TypedTable;
use onceover_report::{Category, PassInfo};

use super::helpers::entity_columns;
use super::setup::{PreparedFrame, ResolvedMetadata, ResolvedWeights};
use super::RunState;

pub(super) fn run_panel_coverage(
    state: &mut RunState<'_>,
    table: &TypedTable,
    prep: &PreparedFrame,
) {
    if state.check_deadline("panel_coverage") {
        return;
    }
    if state.enabled("panel_coverage") {
        let pc = crate::panel::panel_coverage(table, &prep.frame, state.cfg.hints.as_ref());
        let info = match pc.status {
            crate::panel::PanelCoverageStatus::Ran(d) => PassInfo::ran(d),
            crate::panel::PanelCoverageStatus::Capped(d) => PassInfo::capped(d),
            crate::panel::PanelCoverageStatus::Skipped(d) => PassInfo::skipped(d),
        };
        state.stamp("panel_coverage", info);
        state.findings.extend(pc.findings);
    }
}

pub(super) fn run_series_and_panels(
    state: &mut RunState<'_>,
    table: &TypedTable,
    prep: &PreparedFrame,
    meta: &ResolvedMetadata,
    weights: &ResolvedWeights,
) {
    let axis_name = prep.frame.info.axis.as_ref().map(|a| a.name.as_str());
    let ticks = prep.axis_ticks.as_deref();

    let columns_with_role = |role: &str| -> Vec<usize> {
        prep.frame
            .roles
            .iter()
            .enumerate()
            .filter(|(_, r)| r.as_deref() == Some(role))
            .map(|(i, _)| i)
            .collect()
    };

    // ---------- series ----------
    if !state.check_deadline("series") && state.enabled("series") {
        match (ticks, axis_name) {
            (Some(ticks), Some(axis_name)) => {
                let mut measures = columns_with_role("measure");
                let axis_measures: Vec<String> = measures
                    .iter()
                    .filter(|i| prep.axis_excluded.contains(i))
                    .map(|&i| table.names[i].clone())
                    .collect();
                measures.retain(|i| !prep.axis_excluded.contains(i));
                let input = onceover_mines::SeriesInput {
                    axis_name,
                    ticks,
                    measures,
                    dimensions: columns_with_role("dimension"),
                    entities: entity_columns(
                        table,
                        &prep.frame,
                        &prep.card_facts,
                        axis_name,
                        meta.declared_entity_idx,
                    ),
                };
                let (mut f, mut detail) = onceover_mines::profile_series(table, &input);
                if !axis_measures.is_empty() {
                    detail.push_str(&format!(
                        "; the time axis column {} is not profiled as a measure \
                         (structural exclusion; a hints.roles entry for the column \
                         overrides it)",
                        axis_measures.join(", ")
                    ));
                }
                if state.enabled("date_gaps") {
                    f.retain(|finding| finding.category != Category::Gaps);
                }
                state.stamp("series", PassInfo::ran(detail));
                state.findings.append(&mut f);
            }
            _ => state.stamp(
                "series",
                PassInfo::skipped("no date or datetime axis to describe series along".to_string()),
            ),
        }
    }

    // ---------- one-way flags ----------
    if !state.check_deadline("one_way_flags") && state.enabled("series") {
        match (ticks, axis_name) {
            (Some(ticks), Some(axis_name)) => {
                let input = onceover_mines::PanelInput {
                    axis_name,
                    ticks,
                    entities: entity_columns(
                        table,
                        &prep.frame,
                        &prep.card_facts,
                        axis_name,
                        meta.declared_entity_idx,
                    ),
                    measures: columns_with_role("measure"),
                };
                let (mut f, detail) = onceover_mines::panel_flags(table, &input);
                state.stamp("one_way_flags", PassInfo::ran(detail));
                state.findings.append(&mut f);
            }
            _ => state.stamp(
                "one_way_flags",
                PassInfo::skipped(
                    "no entity × time grain: the frame has no time axis with per-row ticks"
                        .to_string(),
                ),
            ),
        }
    }

    // ---------- panel dynamics ----------
    if !state.check_deadline("panel_dynamics") && state.enabled("series") {
        match (ticks, axis_name) {
            (Some(ticks), Some(axis_name)) => {
                let input = onceover_mines::PanelInput {
                    axis_name,
                    ticks,
                    entities: entity_columns(
                        table,
                        &prep.frame,
                        &prep.card_facts,
                        axis_name,
                        meta.declared_entity_idx,
                    ),
                    measures: columns_with_role("measure"),
                };
                let (mut f, detail) = onceover_mines::panel_dynamics(table, &input);
                state.stamp("panel_dynamics", PassInfo::ran(detail));
                state.findings.append(&mut f);
            }
            _ => state.stamp(
                "panel_dynamics",
                PassInfo::skipped(
                    "no entity × time grain: the frame has no time axis with per-row ticks"
                        .to_string(),
                ),
            ),
        }
    }

    // ---------- panel concentration ----------
    if !state.check_deadline("panel_concentration") && state.enabled("series") {
        match (ticks, axis_name) {
            (Some(ticks), Some(axis_name)) => {
                let input = onceover_mines::PanelInput {
                    axis_name,
                    ticks,
                    entities: entity_columns(
                        table,
                        &prep.frame,
                        &prep.card_facts,
                        axis_name,
                        meta.declared_entity_idx,
                    ),
                    measures: columns_with_role("measure"),
                };
                let (mut f, detail) = onceover_mines::panel_concentration(table, &input);
                state.stamp("panel_concentration", PassInfo::ran(detail));
                state.findings.append(&mut f);
            }
            _ => state.stamp(
                "panel_concentration",
                PassInfo::skipped(
                    "no entity × time grain: the frame has no time axis with per-row ticks"
                        .to_string(),
                ),
            ),
        }
    }

    // ---------- entity fingerprints (issue #448) ----------
    if !state.check_deadline("entity_fingerprints") && state.enabled("entity_fingerprints") {
        let axis_name_str = axis_name.unwrap_or("");
        let entity =
            meta.declared_entity_idx.or_else(|| {
                entity_columns(table, &prep.frame, &prep.card_facts, axis_name_str, None)
                    .into_iter()
                    .next()
                    .filter(|&i| {
                        prep.frame.info.grain.as_ref().is_some_and(|g| {
                            g.factorization.iter().any(|f| f.column == table.names[i])
                        })
                    })
            });
        let weight_series = weights.as_series();
        let info = match entity {
            Some(entity) => {
                let ordinals = &prep.declared_hints.level_order;
                let periods = prep
                    .declared_axis
                    .as_deref()
                    .and_then(|name| {
                        onceover_mines::axisbreaks::fingerprint_periods(table, name, ordinals)
                    })
                    .or_else(|| {
                        let axis = prep.frame.info.axis.as_ref()?;
                        if let Some(ticks) = ticks {
                            let keys = ticks
                                .iter()
                                .map(|tick| tick.map_or(f64::NAN, |t| t as f64))
                                .collect();
                            Some(onceover_mines::FingerprintPeriods::new(
                                axis.name.clone(),
                                keys,
                                |key| onceover_mines::panels::tick_label(key as i64),
                            ))
                        } else {
                            onceover_mines::axisbreaks::fingerprint_periods(
                                table,
                                axis.source_column.as_deref()?,
                                ordinals,
                            )
                        }
                    });
                let input = onceover_mines::FingerprintInput {
                    entity,
                    dimensions: prep
                        .frame
                        .roles
                        .iter()
                        .enumerate()
                        .filter(|(i, r)| {
                            *i != entity
                                && periods
                                    .as_ref()
                                    .is_none_or(|axis| table.names[*i] != axis.name)
                                && r.as_deref() == Some("dimension")
                        })
                        .map(|(i, _)| i)
                        .collect(),
                    periods,
                    deadline: state.pass_deadline,
                    weights: &weight_series,
                    hierarchies: state
                        .cfg
                        .hints
                        .as_ref()
                        .map_or(&[][..], |h| h.hierarchies.as_slice()),
                };
                let (mut f, detail, capped) = onceover_mines::entity_fingerprints(table, &input);
                state.findings.append(&mut f);
                let detail = format!("{detail}{}", weights.note);
                if capped {
                    PassInfo::capped(detail)
                } else {
                    PassInfo::ran(detail)
                }
            }
            None => PassInfo::skipped(
                "no entity: none declared (hints.entity) and the grain has no single non-axis factor"
                    .to_string(),
            ),
        };
        state.stamp("entity_fingerprints", info);
    }
}
