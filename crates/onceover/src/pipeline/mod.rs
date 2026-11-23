//! The profiling pipeline: runs passes in cost order under the configured
//! budget and deadline, then assembles the ranked findings ledger, overview,
//! and coverage block. Deterministic end to end unless an opt-in wall-clock
//! deadline fires (disclosed in coverage).
//!
//! Organized by stage:
//! - [`setup`] — column exclusions, weights, metadata, column cards, frame
//!   model, derived-column basis (phase A), and once-computed axis ticks.
//! - [`structural`] — duplicates, keys/FDs, frame account, basis phases B/C,
//!   duplicate encodings, pivot/wide-form, and exact column/pair checks.
//! - [`temporal`] — per-period coverage, series, panel dynamics/concentration,
//!   and entity fingerprints.
//! - [`models`] — shared encoding, dependence, identities, predictability,
//!   regimes, interactions, along-axis passes, outliers, slices, clusters, and
//!   extension detectors.
//! - [`target`] — target-triggered outcome, shape, leakage, and model-failure
//!   passes.
//! - [`finalize`] — scope normalization, rule evaluation, coverage completion,
//!   selection, labels/gloss, and report assembly.

mod finalize;
mod helpers;
mod models;
mod setup;
mod structural;
mod target;
mod temporal;

pub(crate) use setup::PreparedFrame;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use onceover_core::{extract, TypedTable};
use onceover_mines::ModelBasis;
use onceover_report::{AssociationPair, ColumnCard, Finding, PassInfo, ProfileReport};
use polars::prelude::DataFrame;

use crate::config::{ProfileConfig, TierConfig};
use crate::frame::build_temporal_df;

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

/// Shared mutable state carried across pipeline stages.
pub(crate) struct RunState<'cfg> {
    pub cfg: &'cfg ProfileConfig,
    pub active: BTreeSet<String>,
    pub tier: TierConfig,
    pub pass_deadline: onceover_mines::Deadline,
    pub start: Instant,
    pub last_mark: Instant,
    pub deadline_hit: bool,
    pub coverage: BTreeMap<String, PassInfo>,
    pub findings: Vec<Finding>,
    pub association_pairs: Vec<AssociationPair>,
    pub columns: BTreeMap<String, ColumnCard>,
    pub decl_notes: Vec<String>,
}

impl<'cfg> RunState<'cfg> {
    fn new(cfg: &'cfg ProfileConfig, extensions: &[&dyn crate::Detector]) -> Self {
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
        if cfg.target.is_some() && cfg.detectors.is_none() {
            active.insert("outcome_account".into());
            active.insert("outcome_contrasts".into());
            active.insert("target_shapes".into());
            active.insert("leakage".into());
            active.insert("model_failures".into());
        }

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

        let pass_deadline = cfg
            .deadline
            .map_or_else(onceover_mines::Deadline::none, |d| {
                onceover_mines::Deadline::at(start + d)
            });
        tier.disc.deadline = pass_deadline;
        tier.hotspots.deadline = pass_deadline;
        tier.surprises.deadline = pass_deadline;
        tier.surprises.require_rare_breakers = !cfg
            .detectors
            .as_ref()
            .is_some_and(|set| set.contains("outliers"));
        tier.slices.deadline = pass_deadline;
        tier.modelfail.deadline = pass_deadline;
        tier.dupe.deadline = cfg.deadline.map(|d| start + d);

        Self {
            cfg,
            active,
            tier,
            pass_deadline,
            start,
            last_mark: Instant::now(),
            deadline_hit: false,
            coverage: BTreeMap::new(),
            findings: Vec::new(),
            association_pairs: Vec::new(),
            columns: BTreeMap::new(),
            decl_notes: Vec::new(),
        }
    }

    #[inline]
    pub fn enabled(&self, id: &str) -> bool {
        self.active.contains(id)
    }

    #[inline]
    pub fn selected(&self, id: &str) -> bool {
        self.cfg
            .detectors
            .as_ref()
            .is_none_or(|ids| ids.contains(id))
    }

    #[inline]
    pub fn axis_explicit(&self, id: &str) -> bool {
        self.cfg
            .detectors
            .as_ref()
            .is_some_and(|set| set.contains(id))
    }

    pub fn axis_gate_reason(pass: &str) -> String {
        format!(
            "{pass} not run: a default run scans along the frame's time axis or an axis \
             declared with the ordered_axis hint, and this frame has neither; name the \
             detector explicitly or declare an axis to run it"
        )
    }

    pub fn stamp(&mut self, key: impl Into<String>, mut info: PassInfo) {
        let now = Instant::now();
        if self.cfg.timing {
            info.elapsed_ms = Some(now.duration_since(self.last_mark).as_secs_f64() * 1e3);
        }
        self.last_mark = now;
        self.coverage.insert(key.into(), info);
    }

    /// Returns true if the deadline has expired (marking `deadline_hit` and
    /// recording a skipped coverage entry for `name`).
    pub fn check_deadline(&mut self, name: &str) -> bool {
        if let Some(d) = self.cfg.deadline {
            if self.start.elapsed() > d {
                self.coverage
                    .insert(name.to_string(), PassInfo::skipped("deadline reached"));
                self.deadline_hit = true;
                return true;
            }
        }
        self.deadline_hit
    }
}

pub(super) fn profile_inner(
    df: &DataFrame,
    cfg: &ProfileConfig,
    extensions: &[&dyn crate::Detector],
    depth: u8,
) -> ProfileReport {
    let mut state = RunState::new(cfg, extensions);

    let (filtered_df, exclusion_detail) = setup::apply_column_exclusions(df, cfg.hints.as_ref());
    let df: &DataFrame = filtered_df.as_ref().unwrap_or(df);

    let table = extract(df);
    setup::stamp_extraction(&mut state, &table);

    let weights = setup::resolve_weights(&table, cfg.hints.as_ref());
    let mut meta =
        setup::resolve_declared_metadata(&mut state, &table, exclusion_detail.as_deref(), &weights);
    let mut prep = setup::prepare_frame(&mut state, &table);

    let temporal_df = build_temporal_df(df, &prep.frame);
    let temporal_table = temporal_df.as_ref().map(extract);
    let ttable: &TypedTable = temporal_table.as_ref().unwrap_or(&table);

    let structural = structural::run_structural_and_temporal(
        &mut state, df, &table, ttable, &mut prep, &meta, &weights, extensions, depth,
    );

    let mut model_basis = ModelBasis {
        excluded_inputs: prep
            .scaffold_excluded
            .iter()
            .copied()
            .filter(|i| {
                !structural.calendar_inputs.contains(i) && !prep.basis.axis_cols.contains(i)
            })
            .collect(),
        identity_groups: Vec::new(),
    };

    let shared_encoding = models::run_model_passes(
        &mut state,
        &table,
        ttable,
        &prep,
        &meta,
        &weights,
        &structural,
        &mut model_basis,
        extensions,
    );

    // Release temporal frame & weight series once model passes have finished.
    drop(temporal_table);
    drop(temporal_df);
    drop(weights);

    let encoding_sampled = shared_encoding.as_ref().is_some_and(|enc| enc.n < table.n);
    target::run_target_passes(
        &mut state,
        &table,
        &prep,
        &mut meta,
        &structural.grouping_excluded,
        &model_basis,
        shared_encoding.as_ref(),
    );
    drop(shared_encoding);

    finalize::finalize_report(state, &table, prep, encoding_sampled)
}
