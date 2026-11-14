//! Model-failure slices (issue #299): where a model of the
//! designated target fails, stated as a searched slice of rows.
//!
//! The predictability screen reports how well the house GBM fits a
//! target overall. A good overall score can hide a slice of rows
//! where the model is systematically wrong — its errors there are
//! one-sided, or much larger than its errors elsewhere. This pass
//! states those slices. It is target-gated: it runs only when a
//! target is designated, alongside the shape pass.
//!
//! Method. The house GBM (the screen's model, same configuration)
//! is cross-fitted to the target in 5 folds, so every modelled
//! row's residual is out-of-fold: the row's prediction comes from
//! the fold model that held it out. In-sample residuals are never
//! used — they shrink where the model overfits and would hide or
//! manufacture failure slices. The slice search then runs over the
//! residuals with the slice-shift search's machinery and control
//! stack, unchanged: the same condition space (levels of
//! bounded-cardinality categorical columns, thresholds at the 75th
//! and 90th percentiles of numeric columns, conjunctions of depth
//! at most 2), the same deterministic split of the sample into a
//! search half and a confirmation half, split-sample confirmation,
//! and Benjamini–Hochberg control over the candidate family. The
//! new work is the target quantity (the residuals) and the
//! statement wording, not a new search.
//!
//! Two failure forms. Bias: the slice's mean residual differs
//! materially from the model's overall mean residual — the model
//! is one-sided there. Accuracy: the slice's mean absolute
//! residual is materially above the model's overall mean absolute
//! residual — the model errs more there. The effect-size floors
//! are stated on `ModelFailureConfig` and in the benchmark record:
//! a slice becomes a bias candidate when its search-half mean
//! residual is at least 0.5 × the overall mean absolute error from
//! the overall mean residual, and an accuracy candidate when its
//! search-half mean absolute error reaches 1.5 × the overall mean
//! absolute error. Confirmation requires the same form on the
//! held-out half (bias: same direction, at least 0.3 × the overall
//! mean absolute error, and at least half the search-half effect;
//! accuracy: ratio at least 1.25 and at least half the search-half
//! excess retained) plus Benjamini–Hochberg q ≤ 0.05 on the
//! confirmation-half Mann–Whitney p-value (slice against
//! complement, on signed residuals for bias and absolute residuals
//! for accuracy).
//!
//! Every statement names the slice, its size, its error, and the
//! model's overall error — no statement without its baseline.
//! Honest coverage: when the cross-fitted model's held-out fit is
//! at or near chance — pooled out-of-fold R² ≤ 0.01 for a numeric
//! target, pooled out-of-fold AUC ≤ 0.55 for a binary target — the
//! pass records that in its coverage entry and emits no failure
//! statements: where the model fails is not a meaningful question
//! for a model that does not fit anywhere. Categorical targets
//! with more than two classes are not evaluated by this pass (the
//! coverage entry says so); numeric targets and binary targets
//! are.
//!
//! The statistics are measured on the pass's residual set and
//! split, so `report.verify` marks the statements
//! `not_checkable`: a recount on the full frame reproduces neither
//! the cross-fitted residuals nor the split. The slice definition
//! is carried as structured predicates in the statement
//! definition, so the slice itself can be recounted on any row set
//! directly.

use std::collections::BTreeMap;

use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::{json, Value};

use crate::deadline::Deadline;
use crate::encode::EncodedMatrix;
use crate::gbm::{crossfit_residuals, CrossfitOutcome, ScreenConfig};
use crate::slices::{
    bits_and, bits_count, bits_for, bits_hash, bits_positions, level_json, level_labels,
    mann_whitney_p, quantile_sorted, splitmix64,
};

/// Configuration for the model-failure pass. The search fields
/// mirror `SliceConfig` (the search machinery is shared); the
/// pipeline sets `row_cap` from the tier's encoding row cap.
#[derive(Clone)]
pub struct ModelFailureConfig {
    /// Cross-fitting folds.
    pub folds: usize,
    /// Rows of the residual set the search runs on.
    pub row_cap: usize,
    /// Maximum conjunction depth (2).
    pub max_depth: usize,
    /// Maximum levels kept per categorical column, and the
    /// distinct-value ceiling for a column to be treated as
    /// categorical.
    pub max_levels: usize,
    /// Maximum categorical condition columns searched.
    pub max_condition_columns: usize,
    /// Maximum numeric columns contributing threshold predicates.
    pub max_threshold_columns: usize,
    /// Absolute support floor, on the full residual set.
    pub min_support: usize,
    /// Support floor as a fraction of the residual set; the
    /// effective floor is the larger of the two.
    pub min_support_frac: f64,
    /// Minimum slice rows in each half for a slice to be evaluated.
    pub half_support: usize,
    /// Bias search bar: |slice mean residual − overall mean
    /// residual| on the search half, as a multiple of the overall
    /// mean absolute error.
    pub bias_bar: f64,
    /// Bias confirmation floor (held-out half), same units; the
    /// confirmation also requires the same direction and at least
    /// half the search-half effect.
    pub bias_confirm_bar: f64,
    /// Accuracy search bar: slice mean absolute error on the
    /// search half as a multiple of the overall mean absolute
    /// error.
    pub acc_ratio_bar: f64,
    /// Accuracy confirmation floor (held-out half), same units;
    /// the confirmation also requires at least half the
    /// search-half excess over the overall error to be retained.
    pub acc_confirm_ratio: f64,
    /// Benjamini–Hochberg level for the confirmation family.
    pub bh_alpha: f64,
    /// Statement caps: in all, and per form.
    pub max_findings: usize,
    pub max_per_form: usize,
    /// Work budget, in the slice search's units (bitset words plus
    /// slice rows per evaluation).
    pub work_budget: u64,
    /// Chance floors for the model's pooled out-of-fold fit: at or
    /// below, no failure statements are emitted.
    pub min_r2: f64,
    pub min_auc: f64,
    pub deadline: Deadline,
}

impl Default for ModelFailureConfig {
    fn default() -> Self {
        Self {
            folds: 5,
            row_cap: 20_000,
            max_depth: 2,
            max_levels: 25,
            max_condition_columns: 12,
            max_threshold_columns: 12,
            min_support: 50,
            min_support_frac: 0.01,
            half_support: 25,
            bias_bar: 0.5,
            bias_confirm_bar: 0.3,
            acc_ratio_bar: 1.5,
            acc_confirm_ratio: 1.25,
            bh_alpha: 0.05,
            max_findings: 20,
            max_per_form: 10,
            work_budget: 300_000_000,
            min_r2: 0.01,
            min_auc: 0.55,
            deadline: Deadline::none(),
        }
    }
}

// ---------------------------------------------------------------------
// Conditions (mirrors the slice-shift search's condition space).
// ---------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum CondKind {
    Level,
    Above,
}

struct Cond {
    col: usize,
    kind: CondKind,
    text: String,
    predicate: Value,
    bits: Vec<u64>,
}

// ---------------------------------------------------------------------
// Candidates.
// ---------------------------------------------------------------------

struct Candidate {
    /// Condition indices, sorted.
    conds: Vec<usize>,
    /// "bias" | "accuracy".
    form: &'static str,
    /// Slice rows: full set, search half, confirmation half.
    n_full: usize,
    n_search: usize,
    n_confirm: usize,
    /// Full-set slice bitset (row-set dedupe and overlap).
    bits: Vec<u64>,
    /// Mean signed residual in the slice (full / search / confirm).
    bias_full: f64,
    bias_search: f64,
    bias_confirm: f64,
    /// Mean absolute residual in the slice (full / search /
    /// confirm).
    mae_full: f64,
    mae_search: f64,
    mae_confirm: f64,
    /// Binary targets, bias form: the slice's event rate and mean
    /// predicted probability (full set).
    event_rate: f64,
    mean_pred: f64,
    /// Confirmation-half p-value for the candidate's form.
    p_confirm: f64,
    /// BH q-value over the confirmation family (filled in later).
    q_value: f64,
    /// Whether the confirmation-half effect bars cleared.
    confirmed: bool,
    /// Normalised absolute effect in [0, 1], for ranking.
    effect_norm: f64,
}

/// Mean signed / absolute residual, event rate and mean prediction
/// over the rows set in `bits`.
struct SliceStats {
    mean_signed: f64,
    mean_abs: f64,
    mean_actual: f64,
    mean_pred: f64,
}

fn slice_stats(bits: &[u64], cf: &CrossfitOutcome) -> SliceStats {
    let mut n = 0usize;
    let mut signed = 0.0f64;
    let mut abs = 0.0f64;
    let mut actual = 0.0f64;
    let mut pred = 0.0f64;
    for pos in bits_positions(bits) {
        n += 1;
        signed += cf.signed[pos];
        abs += cf.abs[pos];
        actual += cf.actual[pos];
        pred += cf.pred[pos];
    }
    let d = n.max(1) as f64;
    SliceStats {
        mean_signed: signed / d,
        mean_abs: abs / d,
        mean_actual: actual / d,
        mean_pred: pred / d,
    }
}

fn values_at(bits: &[u64], values: &[f64]) -> Vec<f64> {
    bits_positions(bits).map(|pos| values[pos]).collect()
}

// ---------------------------------------------------------------------
// The pass.
// ---------------------------------------------------------------------

/// Screen the designated target's cross-fitted model for failure
/// slices. Returns the findings and the coverage detail string;
/// every early exit is a stated reason in the detail.
pub fn screen_model_failures(
    table: &TypedTable,
    enc: &EncodedMatrix,
    target_name: &str,
    screen_cfg: &ScreenConfig,
    cfg: &ModelFailureConfig,
    excluded_cols: &[usize],
) -> (Vec<Finding>, String) {
    let Some(target_col) = table.names.iter().position(|n| n == target_name) else {
        return (
            Vec::new(),
            format!("designated target '{target_name}' is not a column of the frame"),
        );
    };
    let cf = match crossfit_residuals(table, enc, target_col, screen_cfg, excluded_cols, cfg.folds)
    {
        Ok(cf) => cf,
        Err(reason) => {
            return (
                Vec::new(),
                format!("target '{target_name}' designated; {reason}"),
            );
        }
    };
    let kind = if cf.categorical { "binary" } else { "numeric" };
    let skill_name = if cf.categorical { "AUC" } else { "R²" };
    let floor = if cf.categorical {
        cfg.min_auc
    } else {
        cfg.min_r2
    };
    let base = format!(
        "target '{target_name}' designated ({kind}); {}-fold cross-fitted GBM on {} rows, out-of-fold {} {} (majority baseline {}), overall mean residual {}, mean absolute error {}",
        cfg.folds,
        cf.positions.len(),
        skill_name,
        crate::fmt_float(cf.skill),
        crate::fmt_float(cf.baseline),
        crate::fmt_float(cf.overall_bias),
        crate::fmt_float(cf.overall_mae),
    );
    if cf.skill <= floor {
        return (
            Vec::new(),
            format!(
                "{base}; the model's out-of-fold fit is at or near chance ({} {} at or below the {} floor), so no failure slices are stated: where the model fails is not a meaningful question for a model that does not fit anywhere",
                skill_name,
                crate::fmt_float(cf.skill),
                crate::fmt_float(floor),
            ),
        );
    }
    let (findings, search_detail) =
        search_failure_slices(table, target_name, target_col, enc, &cf, cfg, excluded_cols);
    (findings, format!("{base}; {search_detail}"))
}

/// The slice search over one cross-fit's residuals, separated from
/// the fitting so the search can be exercised on synthetic
/// residuals directly.
#[allow(clippy::too_many_arguments)]
fn search_failure_slices(
    table: &TypedTable,
    target_name: &str,
    target_col: usize,
    enc: &EncodedMatrix,
    cf: &CrossfitOutcome,
    cfg: &ModelFailureConfig,
    excluded_cols: &[usize],
) -> (Vec<Finding>, String) {
    // The search sample: the residual rows, as table rows.
    let sample: Vec<u32> = cf.positions.iter().map(|&p| enc.rows[p as usize]).collect();
    let n_sample = sample.len();
    if n_sample < 100 {
        return (
            Vec::new(),
            format!("fewer than 100 modelled rows to search ({n_sample})"),
        );
    }
    let words = n_sample.div_ceil(64);
    let min_support = cfg
        .min_support
        .max((cfg.min_support_frac * n_sample as f64).ceil() as usize);
    // Deterministic split, exactly as the slice-shift search: the
    // parity of a fixed hash of the sample position, independent
    // of the values, so shuffled residuals face the same split.
    let half_a = bits_for(n_sample, |i| splitmix64(i as u64).is_multiple_of(2));
    let half_b: Vec<u64> = {
        let mut b: Vec<u64> = half_a.iter().map(|w| !w).collect();
        let rem = n_sample % 64;
        if rem != 0 {
            let last = b.len() - 1;
            b[last] &= (1u64 << rem) - 1;
        }
        b
    };
    let n_half_a = bits_count(&half_a);
    let n_half_b = n_sample - n_half_a;

    let mut excluded_set: std::collections::BTreeSet<usize> =
        excluded_cols.iter().copied().collect();
    excluded_set.insert(target_col);

    // ---- condition columns: categorical levels -------------------
    let mut conds: Vec<Cond> = Vec::new();
    let mut cat_cols: Vec<usize> = Vec::new();
    let mut level_rows: Vec<(usize, String, usize)> = Vec::new();
    for col in 0..table.names.len() {
        if excluded_set.contains(&col) {
            continue;
        }
        let Some((labels, _distinct)) = level_labels(table, col, &sample, cfg.max_levels) else {
            continue;
        };
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for l in labels.iter().flatten() {
            *counts.entry(l.clone()).or_default() += 1;
        }
        let kept: Vec<(String, usize)> = counts
            .into_iter()
            .filter(|(_, c)| *c >= min_support)
            .collect();
        if kept.len() < 2 {
            continue;
        }
        cat_cols.push(col);
        for (label, count) in kept {
            level_rows.push((col, label, count));
        }
    }
    let mut cat_rank: Vec<usize> = cat_cols.clone();
    cat_rank.sort_by_key(|&col| {
        let n_levels = level_rows.iter().filter(|(c, _, _)| *c == col).count();
        (std::cmp::Reverse(n_levels), col)
    });
    let capped_cat_cols: std::collections::BTreeSet<usize> = cat_rank
        .iter()
        .take(cfg.max_condition_columns)
        .copied()
        .collect();
    let cat_cols_capped = cat_rank.len() > capped_cat_cols.len();
    level_rows.retain(|(c, _, _)| capped_cat_cols.contains(c));
    level_rows.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));
    level_rows.truncate(cfg.max_condition_columns * cfg.max_levels);
    {
        let mut labels_by_col: BTreeMap<usize, Vec<Option<String>>> = BTreeMap::new();
        for (col, _, _) in &level_rows {
            if !labels_by_col.contains_key(col) {
                let (labels, _) = level_labels(table, *col, &sample, cfg.max_levels)
                    .expect("column was eligible above");
                labels_by_col.insert(*col, labels);
            }
        }
        for (col, label, _) in &level_rows {
            let labels = &labels_by_col[col];
            let bits = bits_for(n_sample, |i| labels[i].as_deref() == Some(label.as_str()));
            conds.push(Cond {
                col: *col,
                kind: CondKind::Level,
                text: format!("{} = '{}'", table.names[*col], label),
                predicate: json!({
                    "column": table.names[*col],
                    "op": "eq",
                    "value": level_json(table, *col, label),
                }),
                bits,
            });
        }
    }

    // ---- condition columns: numeric thresholds -------------------
    let thr_cols_capped;
    {
        let mut numeric_cols: Vec<usize> = Vec::new();
        for col in 0..table.names.len() {
            if excluded_set.contains(&col) {
                continue;
            }
            if matches!(
                table.cols[col],
                TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. }
            ) {
                numeric_cols.push(col);
            }
        }
        let sample_values = |col: usize| -> Vec<f64> {
            sample
                .iter()
                .filter_map(|&r| table.cols[col].numeric_at(r as usize))
                .collect()
        };
        let mut thr_rank: Vec<(usize, usize)> = numeric_cols
            .iter()
            .map(|&col| {
                let vals = sample_values(col);
                let mut sorted = vals.clone();
                sorted.sort_by(|a, b| a.total_cmp(b));
                sorted.dedup();
                (col, sorted.len())
            })
            .filter(|(_, distinct)| *distinct >= 20)
            .collect();
        thr_rank.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        thr_cols_capped = thr_rank.len() > cfg.max_threshold_columns;
        thr_rank.truncate(cfg.max_threshold_columns);
        for (col, _) in &thr_rank {
            let col = *col;
            let mut sorted = sample_values(col);
            sorted.sort_by(|a, b| a.total_cmp(b));
            let max_v = *sorted.last().unwrap();
            for q in [0.75, 0.90] {
                let t = quantile_sorted(&sorted, q);
                if t.partial_cmp(&max_v) != Some(std::cmp::Ordering::Less) {
                    continue;
                }
                let bits = bits_for(n_sample, |i| {
                    table.cols[col]
                        .numeric_at(sample[i] as usize)
                        .is_some_and(|v| v > t)
                });
                if bits_count(&bits) < min_support {
                    continue;
                }
                conds.push(Cond {
                    col,
                    kind: CondKind::Above,
                    text: format!("{} > {}", table.names[col], crate::fmt_float(t)),
                    predicate: json!({
                        "column": table.names[col],
                        "op": "gt",
                        "value": t,
                    }),
                    bits,
                });
            }
        }
    }

    if conds.is_empty() {
        return (
            Vec::new(),
            "no eligible condition columns (categorical levels or numeric thresholds)".into(),
        );
    }

    // ---- slice enumeration (depth 1 and 2) ------------------------
    let level_conds: Vec<usize> = conds
        .iter()
        .enumerate()
        .filter(|(_, c)| c.kind == CondKind::Level)
        .map(|(i, _)| i)
        .collect();
    let thr_conds: Vec<usize> = conds
        .iter()
        .enumerate()
        .filter(|(_, c)| c.kind == CondKind::Above)
        .map(|(i, _)| i)
        .collect();
    let mut slice_defs: Vec<Vec<usize>> = Vec::new();
    for &i in &level_conds {
        slice_defs.push(vec![i]);
    }
    for &i in &thr_conds {
        slice_defs.push(vec![i]);
    }
    if cfg.max_depth >= 2 {
        for (a, &i) in level_conds.iter().enumerate() {
            for &j in &level_conds[a + 1..] {
                if conds[i].col != conds[j].col {
                    slice_defs.push(vec![i, j]);
                }
            }
            for &j in &thr_conds {
                if conds[i].col != conds[j].col {
                    slice_defs.push(vec![i, j]);
                }
            }
        }
    }
    let total_slices = slice_defs.len();

    // ---- search + confirmation ------------------------------------
    let overall_bias = cf.overall_bias;
    let overall_mae = cf.overall_mae;
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut slices_examined = 0usize;
    let mut slices_with_support = 0usize;
    let mut pairs_evaluated = 0u64;
    let mut work: u64 = 0;
    let mut stopped = false;

    for slice in &slice_defs {
        if cfg.deadline.exceeded() {
            stopped = true;
            break;
        }
        slices_examined += 1;
        let mut bits = conds[slice[0]].bits.clone();
        for &ci in &slice[1..] {
            bits = bits_and(&bits, &conds[ci].bits);
        }
        work += words as u64;
        let n_full = bits_count(&bits);
        if n_full < min_support {
            continue;
        }
        let bits_search = bits_and(&bits, &half_a);
        let bits_confirm = bits_and(&bits, &half_b);
        let n_search = bits_count(&bits_search);
        let n_confirm = bits_count(&bits_confirm);
        if n_search < cfg.half_support || n_confirm < cfg.half_support {
            continue;
        }
        slices_with_support += 1;
        pairs_evaluated += 2;
        work += 2 * n_full as u64;
        if work > cfg.work_budget {
            stopped = true;
            break;
        }
        let s = slice_stats(&bits_search, cf);
        let bias_effect_search = s.mean_signed - overall_bias;
        let bias_hit = overall_mae > 0.0 && bias_effect_search.abs() >= cfg.bias_bar * overall_mae;
        let acc_hit = overall_mae > 0.0 && s.mean_abs >= cfg.acc_ratio_bar * overall_mae;
        if !bias_hit && !acc_hit {
            continue;
        }
        let c = slice_stats(&bits_confirm, cf);
        let f = slice_stats(&bits, cf);
        let comp_bits: Vec<u64> = bits_confirm
            .iter()
            .zip(half_b.iter())
            .map(|(sb, h)| h & !sb)
            .collect();
        if bias_hit {
            let bias_effect_confirm = c.mean_signed - overall_bias;
            let confirmed = bias_effect_confirm.signum() == bias_effect_search.signum()
                && bias_effect_confirm.abs() >= cfg.bias_confirm_bar * overall_mae
                && bias_effect_confirm.abs() >= 0.5 * bias_effect_search.abs();
            let p = mann_whitney_p(
                &values_at(&bits_confirm, &cf.signed),
                &values_at(&comp_bits, &cf.signed),
            );
            let effect_norm =
                ((f.mean_signed - overall_bias).abs() / (2.0 * overall_mae)).clamp(0.0, 1.0);
            candidates.push(Candidate {
                conds: slice.to_vec(),
                form: "bias",
                n_full,
                n_search,
                n_confirm,
                bits: bits.clone(),
                bias_full: f.mean_signed,
                bias_search: s.mean_signed,
                bias_confirm: c.mean_signed,
                mae_full: f.mean_abs,
                mae_search: s.mean_abs,
                mae_confirm: c.mean_abs,
                event_rate: f.mean_actual,
                mean_pred: f.mean_pred,
                p_confirm: p,
                q_value: 1.0,
                confirmed,
                effect_norm,
            });
        }
        if acc_hit {
            let excess_search = s.mean_abs - overall_mae;
            let excess_confirm = c.mean_abs - overall_mae;
            let confirmed = c.mean_abs >= cfg.acc_confirm_ratio * overall_mae
                && excess_confirm >= 0.5 * excess_search;
            let p = mann_whitney_p(
                &values_at(&bits_confirm, &cf.abs),
                &values_at(&comp_bits, &cf.abs),
            );
            let effect_norm = ((f.mean_abs / overall_mae - 1.0) / 1.5).clamp(0.0, 1.0);
            candidates.push(Candidate {
                conds: slice.to_vec(),
                form: "accuracy",
                n_full,
                n_search,
                n_confirm,
                bits: bits.clone(),
                bias_full: f.mean_signed,
                bias_search: s.mean_signed,
                bias_confirm: c.mean_signed,
                mae_full: f.mean_abs,
                mae_search: s.mean_abs,
                mae_confirm: c.mean_abs,
                event_rate: f.mean_actual,
                mean_pred: f.mean_pred,
                p_confirm: p,
                q_value: 1.0,
                confirmed,
                effect_norm,
            });
        }
    }

    // ---- BH over the confirmation family --------------------------
    let family = candidates.len();
    if family > 0 {
        let pvals: Vec<Option<f64>> = candidates.iter().map(|c| Some(c.p_confirm)).collect();
        let qs = crate::dep::bh_qvalues(&pvals);
        for (c, q) in candidates.iter_mut().zip(qs) {
            c.q_value = q.unwrap_or(1.0);
        }
    }
    let mut kept: Vec<Candidate> = candidates
        .into_iter()
        .filter(|c| c.confirmed && c.q_value <= cfg.bh_alpha)
        .collect();
    let confirmed_count = kept.len();

    // ---- row-set dedupe (per form) ---------------------------------
    let cond_text = |cands: &[usize]| -> String {
        cands
            .iter()
            .map(|&ci| conds[ci].text.clone())
            .collect::<Vec<_>>()
            .join(" and ")
    };
    kept.sort_by(|a, b| {
        a.conds
            .len()
            .cmp(&b.conds.len())
            .then(b.effect_norm.total_cmp(&a.effect_norm))
            .then(cond_text(&a.conds).cmp(&cond_text(&b.conds)))
    });
    let mut seen_sets: BTreeMap<(&str, u64), ()> = BTreeMap::new();
    let mut deduped: Vec<Candidate> = Vec::new();
    for c in kept {
        let key = (c.form, bits_hash(&c.bits));
        if seen_sets.insert(key, ()).is_none() {
            deduped.push(c);
        }
    }

    // ---- restatement demotion (per form) ----------------------------
    let mut demoted: Vec<bool> = vec![false; deduped.len()];
    for i in 0..deduped.len() {
        for j in 0..deduped.len() {
            if deduped[j].conds.len() >= deduped[i].conds.len() {
                continue;
            }
            if deduped[j].form != deduped[i].form {
                continue;
            }
            let inter: usize = deduped[i]
                .bits
                .iter()
                .zip(deduped[j].bits.iter())
                .map(|(a, b)| (a & b).count_ones() as usize)
                .sum();
            let union: usize = deduped[i]
                .bits
                .iter()
                .zip(deduped[j].bits.iter())
                .map(|(a, b)| (a | b).count_ones() as usize)
                .sum();
            if union > 0 && inter as f64 / union as f64 >= 0.9 {
                demoted[i] = true;
                break;
            }
        }
    }

    // ---- ranking by absolute effect, then support -------------------
    let mut ranked: Vec<(Candidate, bool)> = deduped.into_iter().zip(demoted).collect();
    ranked.sort_by(|a, b| {
        b.0.effect_norm
            .total_cmp(&a.0.effect_norm)
            .then(b.0.n_full.cmp(&a.0.n_full))
            .then(cond_text(&a.0.conds).cmp(&cond_text(&b.0.conds)))
    });
    let mut per_form: BTreeMap<&str, usize> = BTreeMap::new();
    let mut capped: Vec<(Candidate, bool)> = Vec::new();
    for item in ranked {
        let count = per_form.entry(item.0.form).or_default();
        if *count >= cfg.max_per_form {
            continue;
        }
        *count += 1;
        capped.push(item);
        if capped.len() >= cfg.max_findings {
            break;
        }
    }
    let stated_count = capped.len();
    let ranked = capped;

    // ---- findings ----------------------------------------------------
    let pct = |x: f64| format!("{:.1}", 100.0 * x);
    let mut findings: Vec<Finding> = Vec::new();
    for (cand, is_demoted) in &ranked {
        let cond_texts: Vec<String> = cand
            .conds
            .iter()
            .map(|&ci| conds[ci].text.clone())
            .collect();
        let cond_json: Vec<Value> = cand
            .conds
            .iter()
            .map(|&ci| conds[ci].predicate.clone())
            .collect();
        let slice_desc = cond_texts.join(" and ");
        let mut cond_cols: Vec<String> = cand
            .conds
            .iter()
            .map(|&ci| table.names[conds[ci].col].clone())
            .collect();
        cond_cols.sort();
        cond_cols.dedup();

        let ratio = cand.mae_full / overall_mae;
        let claim = if cand.form == "bias" {
            if cf.categorical {
                format!(
                    "within {} ({} rows), the model of '{}' is one-sided: '{}' occurs for {}% of rows there but the model's mean predicted probability is {}%, against {}% occurring and {}% predicted over all {} modelled rows",
                    slice_desc,
                    cand.n_full,
                    target_name,
                    cf.labels[1],
                    pct(cand.event_rate),
                    pct(cand.mean_pred),
                    pct(cf.event_rate),
                    pct(cf.mean_pred),
                    n_sample,
                )
            } else {
                format!(
                    "within {} ({} rows), the model of '{}' is one-sided: its mean out-of-fold residual there is {} against {} over all {} modelled rows (residual = actual - predicted; its overall mean absolute error is {})",
                    slice_desc,
                    cand.n_full,
                    target_name,
                    crate::fmt_float(cand.bias_full),
                    crate::fmt_float(overall_bias),
                    n_sample,
                    crate::fmt_float(overall_mae),
                )
            }
        } else if cf.categorical {
            format!(
                "within {} ({} rows), the model of '{}' errs more: its mean absolute probability error there is {} against {} over all {} modelled rows ({}x)",
                slice_desc,
                cand.n_full,
                target_name,
                crate::fmt_float(cand.mae_full),
                crate::fmt_float(overall_mae),
                n_sample,
                crate::fmt_float(ratio),
            )
        } else {
            format!(
                "within {} ({} rows), the model of '{}' errs more: its mean absolute out-of-fold error there is {} against {} over all {} modelled rows ({}x)",
                slice_desc,
                cand.n_full,
                target_name,
                crate::fmt_float(cand.mae_full),
                crate::fmt_float(overall_mae),
                n_sample,
                crate::fmt_float(ratio),
            )
        };
        let mut metrics = BTreeMap::from([
            ("form".into(), json!(cand.form)),
            ("conditions".into(), json!(cond_json)),
            ("slice_rows".into(), json!(cand.n_full)),
            ("slice_rows_search".into(), json!(cand.n_search)),
            ("slice_rows_confirm".into(), json!(cand.n_confirm)),
            ("n_train".into(), json!(n_half_a)),
            ("n_holdout".into(), json!(n_half_b)),
            ("depth".into(), json!(cand.conds.len())),
            ("slice_bias".into(), json!(cand.bias_full)),
            ("search_bias".into(), json!(cand.bias_search)),
            ("confirm_bias".into(), json!(cand.bias_confirm)),
            ("overall_bias".into(), json!(overall_bias)),
            ("slice_mae".into(), json!(cand.mae_full)),
            ("search_mae".into(), json!(cand.mae_search)),
            ("confirm_mae".into(), json!(cand.mae_confirm)),
            ("overall_mae".into(), json!(overall_mae)),
            ("mae_ratio".into(), json!(ratio)),
            (
                "model_skill".into(),
                json!(if cf.categorical { "auc" } else { "r2" }),
            ),
            ("skill".into(), json!(cf.skill)),
            ("skill_baseline".into(), json!(cf.baseline)),
            ("folds".into(), json!(cfg.folds)),
            ("p_value".into(), json!(cand.p_confirm)),
            ("q_value".into(), json!(cand.q_value)),
            ("search_candidates".into(), json!(family)),
            ("candidates_evaluated".into(), json!(pairs_evaluated)),
        ]);
        if cf.categorical {
            metrics.insert("event_label".into(), json!(cf.labels[1]));
            metrics.insert("slice_event_rate".into(), json!(cand.event_rate));
            metrics.insert("slice_mean_pred".into(), json!(cand.mean_pred));
            metrics.insert("overall_event_rate".into(), json!(cf.event_rate));
            metrics.insert("overall_mean_pred".into(), json!(cf.mean_pred));
        }
        if *is_demoted {
            metrics.insert("triviality".into(), json!("restates_parent_slice"));
            metrics.insert("triviality_factor".into(), json!(0.6));
        }
        let mut columns = vec![target_name.to_string()];
        columns.extend(cond_cols.iter().cloned());
        let mut params: BTreeMap<String, Value> = BTreeMap::new();
        params.insert("columns".into(), json!(columns));
        params.insert("inputs".into(), json!(cond_cols));
        params.insert("target".into(), json!(target_name));
        params.insert("conditions".into(), json!(cond_json));
        let support_factor = 0.8 + 0.2 * ((cand.n_full as f64).log10() / 3.0).min(1.0);
        let score = ((0.5 + 0.45 * cand.effect_norm) * support_factor).clamp(0.0, 0.95);
        findings.push(Finding::new(
            Pattern::ModelFailure,
            params,
            claim,
            EvidenceBasis::Estimated,
            score,
            metrics,
            Some(
                "a searched failure slice, not a cause: the slice was selected because the model's error is large or one-sided there, and confirmed on held-out rows; the residuals are out-of-fold, so the slice is not an artifact of the model fitting those rows"
                    .into(),
            ),
            Method::new(
                "model_failures",
                "cross_fitted_gbm_residual_slice_search",
                "cross-fitted (5-fold) house-GBM residuals searched with the slice conjunction space, split-sample confirmation and Benjamini-Hochberg control (issue #299)",
            ),
        ));
    }

    // ---- coverage ------------------------------------------------------
    let mut detail = format!(
        "searched {} slice x form candidates over {} supported slices ({} of {} slice definitions examined, depth <= {}) on {} modelled rows split {} / {} for search and confirmation; {} cleared the search bars, {} confirmed on the held-out half at Benjamini-Hochberg q <= {}, {} stated (cap {})",
        pairs_evaluated,
        slices_with_support,
        slices_examined,
        total_slices,
        cfg.max_depth,
        n_sample,
        n_half_a,
        n_half_b,
        family,
        confirmed_count,
        crate::fmt_float(cfg.bh_alpha),
        stated_count,
        cfg.max_findings,
    );
    if table.n > n_sample {
        detail.push_str(&format!(
            "; the modelled rows are {} of {} rows",
            n_sample, table.n
        ));
    }
    if cat_cols_capped {
        detail.push_str(&format!(
            "; condition columns capped at {}",
            cfg.max_condition_columns
        ));
    }
    if thr_cols_capped {
        detail.push_str(&format!(
            "; threshold columns capped at {}",
            cfg.max_threshold_columns
        ));
    }
    if stopped {
        detail.push_str(
            "; stopped early (work budget or deadline): the examined slices are a prefix of the deterministic enumeration, so the negative is partial",
        );
    }
    (findings, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::NumericValues;

    /// A frame with a categorical column `g` (levels A/B/C/D) and a
    /// numeric column `x`; residuals are fabricated directly, so
    /// the search is exercised without the GBM.
    fn fixture(
        residual: impl Fn(usize, &str, f64) -> f64,
    ) -> (TypedTable<'static>, CrossfitOutcome) {
        let n = 2400usize;
        let mut rng_state = 0x1234_5678_9abc_def0u64;
        let mut next = move || {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            (rng_state >> 11) as f64 / (1u64 << 53) as f64
        };
        let levels = ["A", "B", "C", "D"];
        let mut g: Vec<Option<&str>> = Vec::with_capacity(n);
        let mut x: Vec<Option<f64>> = Vec::with_capacity(n);
        let mut y: Vec<Option<f64>> = Vec::with_capacity(n);
        for i in 0..n {
            let lev = levels[i % 4];
            g.push(Some(lev));
            let xv = next();
            x.push(Some(xv));
            y.push(Some(0.0));
        }
        let mut signed = Vec::with_capacity(n);
        let mut abs = Vec::with_capacity(n);
        let mut actual = Vec::with_capacity(n);
        let mut pred = Vec::with_capacity(n);
        for i in 0..n {
            let r = residual(i, g[i].unwrap(), x[i].unwrap());
            signed.push(r);
            abs.push(r.abs());
            actual.push(0.0);
            pred.push(-r);
        }
        let mae = abs.iter().sum::<f64>() / n as f64;
        let bias = signed.iter().sum::<f64>() / n as f64;
        let table = TypedTable {
            n,
            names: vec!["y".into(), "g".into(), "x".into()],
            dtypes: vec!["float64".into(), "string".into(), "float64".into()],
            cols: vec![
                TypedCol::F64 {
                    vals: NumericValues::from(y),
                },
                TypedCol::Str { vals: g },
                TypedCol::F64 {
                    vals: NumericValues::from(x),
                },
            ],
        };
        let cf = CrossfitOutcome {
            positions: (0..n as u32).collect(),
            signed,
            abs,
            actual,
            pred,
            categorical: false,
            labels: Vec::new(),
            skill: 0.8,
            baseline: 0.0,
            overall_bias: bias,
            overall_mae: mae,
            event_rate: 0.0,
            mean_pred: 0.0,
            fold_rounds: vec![10; 5],
        };
        (table, cf)
    }

    fn enc_for(table: &TypedTable) -> EncodedMatrix {
        EncodedMatrix {
            rows: (0..table.n as u32).collect(),
            n: table.n,
            feat_cols: Vec::new(),
            feat_names: Vec::new(),
            feat_dtypes: Vec::new(),
            feat_nominal: Vec::new(),
            bins: Vec::new(),
            n_bins: Vec::new(),
            edges: Vec::new(),
            levels: Vec::new(),
        }
    }

    #[test]
    fn planted_bias_slice_fires() {
        // Residuals are +3 inside g = 'D' (one-sided model failure)
        // and small symmetric noise elsewhere.
        let (table, cf) = fixture(|i, lev, _x| {
            let noise = ((i * 2654435761 % 97) as f64 / 97.0 - 0.5) * 0.4;
            if lev == "D" {
                3.0 + noise
            } else {
                noise
            }
        });
        let enc = enc_for(&table);
        let (findings, _detail) = search_failure_slices(
            &table,
            "y",
            0,
            &enc,
            &cf,
            &ModelFailureConfig::default(),
            &[],
        );
        let bias: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.evidence[0].metrics["form"] == json!("bias"))
            .collect();
        assert!(
            !bias.is_empty(),
            "expected a bias finding, got {} findings",
            findings.len()
        );
        let claim = &bias[0].claim;
        assert!(claim.contains("g = 'D'"), "claim: {claim}");
    }

    #[test]
    fn planted_accuracy_slice_fires() {
        // Residuals are large in both directions inside g = 'C'
        // (inaccurate but unbiased) and small elsewhere.
        let (table, cf) = fixture(|i, lev, _x| {
            let base = ((i * 2654435761 % 89) as f64 / 89.0 - 0.5) * 0.4;
            if lev == "C" {
                if i % 2 == 0 {
                    4.0 + base
                } else {
                    -4.0 + base
                }
            } else {
                base
            }
        });
        let enc = enc_for(&table);
        let (findings, _detail) = search_failure_slices(
            &table,
            "y",
            0,
            &enc,
            &cf,
            &ModelFailureConfig::default(),
            &[],
        );
        let acc: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.evidence[0].metrics["form"] == json!("accuracy"))
            .collect();
        assert!(!acc.is_empty(), "expected an accuracy finding");
        assert!(acc[0].claim.contains("g = 'C'"), "claim: {}", acc[0].claim);
    }

    #[test]
    fn uniform_residuals_are_silent() {
        let (table, cf) = fixture(|i, _lev, _x| ((i * 2654435761 % 97) as f64 / 97.0 - 0.5) * 0.4);
        let enc = enc_for(&table);
        let (findings, _detail) = search_failure_slices(
            &table,
            "y",
            0,
            &enc,
            &cf,
            &ModelFailureConfig::default(),
            &[],
        );
        assert!(
            findings.is_empty(),
            "got {} findings, first: {}",
            findings.len(),
            findings.first().map(|f| f.claim.as_str()).unwrap_or("")
        );
    }

    #[test]
    fn shuffled_residuals_are_silent() {
        // The planted-bias residuals, permuted against the rows:
        // the same residual values, no slice structure.
        let (table, mut cf) = fixture(|i, lev, _x| {
            let noise = ((i * 2654435761 % 97) as f64 / 97.0 - 0.5) * 0.4;
            if lev == "D" {
                3.0 + noise
            } else {
                noise
            }
        });
        // Fisher–Yates with a fixed-seed LCG: a genuine permutation
        // of the residual values against the rows.
        let mut order: Vec<usize> = (0..cf.signed.len()).collect();
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for i in (1..order.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let j = (state % (i as u64 + 1)) as usize;
            order.swap(i, j);
        }
        let shuffled: Vec<f64> = order.iter().map(|&i| cf.signed[i]).collect();
        cf.signed = shuffled;
        cf.abs = cf.signed.iter().map(|r| r.abs()).collect();
        let enc = enc_for(&table);
        let (findings, _detail) = search_failure_slices(
            &table,
            "y",
            0,
            &enc,
            &cf,
            &ModelFailureConfig::default(),
            &[],
        );
        assert!(
            findings.is_empty(),
            "got {} findings, first: {}",
            findings.len(),
            findings.first().map(|f| f.claim.as_str()).unwrap_or("")
        );
    }
}

#[cfg(test)]
mod crossfit_tests {
    use super::*;
    use onceover_core::NumericValues;

    #[test]
    fn binary_crossfit_learns_a_determined_target() {
        // y (yes/no) is determined by a > 0.5 on 80% of rows and a
        // coin flip on the rest: a cross-fit should reach AUC ~0.85.
        let n = 2000usize;
        let mut state = 0x853c_49e6_748f_ea9bu64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut a: Vec<Option<f64>> = Vec::new();
        let mut g: Vec<Option<&str>> = Vec::new();
        let mut y: Vec<Option<&str>> = Vec::new();
        for i in 0..n {
            let av = next();
            let wild = i % 5 == 0;
            let event = if wild { next() < 0.5 } else { av > 0.5 };
            a.push(Some(av));
            g.push(Some(if wild { "wild" } else { "calm" }));
            y.push(Some(if event { "yes" } else { "no" }));
        }
        let table = TypedTable {
            n,
            names: vec!["a".into(), "g".into(), "y".into()],
            dtypes: vec!["float64".into(), "string".into(), "string".into()],
            cols: vec![
                TypedCol::F64 {
                    vals: NumericValues::from(a),
                },
                TypedCol::Str { vals: g },
                TypedCol::Str { vals: y },
            ],
        };
        let (enc, _) = crate::encode::encode(&table, &crate::encode::EncodeConfig::default());
        let cf = crossfit_residuals(&table, &enc, 2, &ScreenConfig::default(), &[], 5)
            .expect("crossfit should succeed");
        assert!(cf.skill > 0.7, "skill {}", cf.skill);
        assert!(cf.overall_mae < 0.3, "mae {}", cf.overall_mae);
    }
}
