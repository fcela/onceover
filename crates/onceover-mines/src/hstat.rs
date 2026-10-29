//! Friedman–Popescu H statistic for the strongest predictor pairs
//! of the targets the predictability screen fits.
//!
//! The pairwise dependence measures elsewhere in the profiler are
//! model-free: they say two columns are related. H answers a
//! different question for a specific target: of the variance in
//! the pair's joint effect on the target (under the screen's GBM
//! surrogate), how much is interaction — the part the sum of the
//! two single effects does not explain?
//!
//! The construction follows mboostrs-dev's
//! `h_statistic_with_variance`: partial dependences of the
//! surrogate evaluated at the sample points over the empirical
//! background. One addition, measured (see the benchmarks entry
//! for this pass): a target is only eligible when the full
//! surrogate beats an additive (depth-1) refit of the same target
//! on the same split by a stated holdout-R² margin. A tree
//! ensemble's staircase fit of a purely additive surface is not
//! additive as a function, and its partial dependences read as
//! interaction (H² = 0.12, interaction variance 11% of the target
//! variance, on this pass's additive test fixture) — large enough
//! to clear any plausible reporting bar. The gate screens the
//! target out before pairs are evaluated: interactions must earn
//! their place out of sample, not only in the fitted function.
//!
//! A reported pair must clear three further bars — the H² share,
//! the interaction variance's separation from zero (the standard
//! error is of the interaction variance, the mean of the squared
//! per-point interaction terms), and the interaction variance as
//! a share of the target's own variance — because H² alone can be
//! large for a pair whose joint effect is negligible.

use onceover_core::TypedTable;
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;
use std::collections::BTreeMap;

use crate::encode::EncodedMatrix;
use crate::gbm::{fit_gbm_model, FittedGbm, ScreenConfig, TargetResult};

#[derive(Clone, Debug)]
pub struct HConfig {
    /// Partial-dependence evaluation rows: the sample whose points
    /// the dependences are evaluated at and whose rows form the
    /// empirical background. 0 disables the pass.
    pub pd_rows: usize,
    /// Strongest predictors per target whose pairs are evaluated.
    pub top_features: usize,
    pub max_targets: usize,
    pub max_pairs_per_target: usize,
    /// The surrogate must predict the target at least this well
    /// (holdout R²) before any interaction claim is made from it.
    pub min_holdout_r2: f64,
    /// The full surrogate must beat an additive (depth-1) refit of
    /// the same target, same split, by at least this holdout-R²
    /// margin before any pair of that target is reported.
    pub additive_gain_min: f64,
    /// Reporting bars: H² share, interaction-variance z, and
    /// interaction variance as a share of the target's variance.
    pub h2_min: f64,
    pub z_min: f64,
    pub var_share_min: f64,
}

impl Default for HConfig {
    fn default() -> Self {
        Self {
            pd_rows: 128,
            top_features: 6,
            max_targets: 3,
            max_pairs_per_target: 5,
            min_holdout_r2: 0.10,
            additive_gain_min: 0.02,
            h2_min: 0.05,
            z_min: 4.0,
            var_share_min: 0.01,
        }
    }
}

impl HConfig {
    pub fn disabled() -> Self {
        Self {
            pd_rows: 0,
            ..Self::default()
        }
    }
    pub fn is_enabled(&self) -> bool {
        self.pd_rows > 0
    }
}

/// H² and its companion quantities for one pair, from the three
/// partial-dependence vectors (joint, first, second — evaluated
/// at the same sample points). Variances are means of squares of
/// the centred vectors; the standard error belongs to the
/// interaction variance (the mean of the per-point squared
/// interaction terms).
struct HStats {
    h2: f64,
    interaction_variance: f64,
    interaction_se: f64,
    interaction_z: f64,
    joint_var: f64,
    first_var: f64,
    second_var: f64,
}

fn h_stats(pd_jk: &[f64], pd_j: &[f64], pd_k: &[f64]) -> HStats {
    let m = pd_jk.len() as f64;
    let centre = |v: &[f64]| -> Vec<f64> {
        let mean = v.iter().sum::<f64>() / m;
        v.iter().map(|x| x - mean).collect()
    };
    let (jk, j, k) = (centre(pd_jk), centre(pd_j), centre(pd_k));
    let mean_sq = |v: &[f64]| v.iter().map(|x| x * x).sum::<f64>() / m;
    let joint_var = mean_sq(&jk);
    let mut num = 0.0;
    let mut e: Vec<f64> = Vec::with_capacity(pd_jk.len());
    for i in 0..pd_jk.len() {
        let d = jk[i] - j[i] - k[i];
        e.push(d * d);
        num += d * d;
    }
    let interaction_variance = num / m;
    let den: f64 = jk.iter().map(|v| v * v).sum();
    let h2 = if den > 0.0 {
        (num / den).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let se = if pd_jk.len() > 1 {
        let var_e = e
            .iter()
            .map(|x| (x - interaction_variance) * (x - interaction_variance))
            .sum::<f64>()
            / (m - 1.0);
        (var_e / m).sqrt()
    } else {
        0.0
    };
    HStats {
        h2,
        interaction_variance,
        interaction_se: se,
        // A zero standard error with a positive variance means the
        // squared interaction terms were constant across points —
        // no measurable sampling spread; report the separation as
        // a large capped value rather than zero or an infinity
        // the report cannot serialize.
        interaction_z: if se > 0.0 {
            interaction_variance / se
        } else if interaction_variance > 0.0 {
            1e6
        } else {
            0.0
        },
        joint_var,
        first_var: mean_sq(&j),
        second_var: mean_sq(&k),
    }
}

/// Partial dependence of the model on the packed-feature
/// positions in `set` (one or two positions), evaluated at the
/// sample rows' own values: for each sample point g, the mean
/// prediction over all sample rows with the `set` positions
/// replaced by point g's values (the empirical-background
/// construction mboostrs-dev's preexplore uses).
fn partial_dependence(model: &FittedGbm, rows: &[Vec<u8>], set: &[usize]) -> Vec<f64> {
    let m = rows.len();
    let mut out = Vec::with_capacity(m);
    let mut scratch: Vec<u8> = Vec::new();
    for g in 0..m {
        let mut sum = 0.0;
        for b in 0..m {
            scratch.clone_from(&rows[b]);
            for &pos in set {
                scratch[pos] = rows[g][pos];
            }
            sum += model.predict_row(&scratch);
        }
        out.push(sum / m as f64);
    }
    out
}

pub fn profile_interactions(
    table: &TypedTable,
    enc: &EncodedMatrix,
    results: &[TargetResult],
    screen_cfg: &ScreenConfig,
    cfg: &HConfig,
) -> (Vec<Finding>, String) {
    if !cfg.is_enabled() {
        return (Vec::new(), "disabled at this budget".into());
    }
    let mut targets: Vec<&TargetResult> = results
        .iter()
        .filter(|r| !r.is_categorical && r.metric >= cfg.min_holdout_r2)
        .collect();
    targets.sort_by(|a, b| b.metric.total_cmp(&a.metric).then(a.name.cmp(&b.name)));
    let n_eligible = targets.len();
    targets.truncate(cfg.max_targets);

    let mut findings: Vec<Finding> = Vec::new();
    let mut n_fitted = 0usize;
    let mut n_gated = 0usize;
    let mut n_pairs = 0usize;
    for target in targets {
        let Some(model) = fit_gbm_model(table, enc, target.col, screen_cfg) else {
            continue;
        };
        n_fitted += 1;
        // Strongest predictors by split gain at the best round.
        let mut ranked: Vec<usize> = (0..model.selected.len())
            .filter(|&p| model.best_gains[p] > 0.0)
            .collect();
        ranked.sort_by(|&a, &b| {
            model.best_gains[b]
                .total_cmp(&model.best_gains[a])
                .then(a.cmp(&b))
        });
        ranked.truncate(cfg.top_features);
        if ranked.len() < 2 {
            continue;
        }
        // Additive gate: a depth-1 refit of the same target on the
        // same split (with ten times the rounds — stumps converge
        // slowly, and an underfit additive refit would fake the
        // margin) must lose to the full surrogate by the stated
        // holdout-R² margin, or this target's interactions are not
        // reported at all.
        let mut add_cfg = screen_cfg.clone();
        add_cfg.max_depth = 1;
        add_cfg.max_rounds = screen_cfg.max_rounds.saturating_mul(10);
        let additive_r2 = fit_gbm_model(table, enc, target.col, &add_cfg)
            .map(|m| m.holdout_r2)
            .unwrap_or(0.0);
        if model.holdout_r2 - additive_r2 < cfg.additive_gain_min {
            continue;
        }
        n_gated += 1;
        // Evaluation sample: a deterministic stride sample
        // of the target's present rows, in encoded space (the rows
        // `enc.bins` is indexed by — a row sample of the table at
        // larger sizes, so table-space indices would run off the
        // end of the bin columns).
        let present = &model.present;
        if present.len() < 30 {
            continue;
        }
        let sample_idx = onceover_core::sample_rows(present.len(), cfg.pd_rows);
        let sample: Vec<(u32, f64)> = sample_idx
            .iter()
            .map(|&i| {
                let r = present[i as usize];
                (r, model.y[r as usize] as f64)
            })
            .collect();
        let m = sample.len();
        let y_mean = sample.iter().map(|s| s.1).sum::<f64>() / m as f64;
        let target_variance = sample
            .iter()
            .map(|s| (s.1 - y_mean) * (s.1 - y_mean))
            .sum::<f64>()
            / m as f64;
        if target_variance <= 0.0 {
            continue;
        }
        let rows: Vec<Vec<u8>> = sample
            .iter()
            .map(|&(r, _)| {
                model
                    .selected
                    .iter()
                    .map(|&f| enc.bins[f][r as usize])
                    .collect()
            })
            .collect();

        let singles: Vec<Vec<f64>> = ranked
            .iter()
            .map(|&p| partial_dependence(&model, &rows, &[p]))
            .collect();
        let mut pair_stats: Vec<(usize, usize, HStats)> = Vec::new();
        for a in 0..ranked.len() {
            for b in (a + 1)..ranked.len() {
                let joint = partial_dependence(&model, &rows, &[ranked[a], ranked[b]]);
                let st = h_stats(&joint, &singles[a], &singles[b]);
                n_pairs += 1;
                if st.h2 >= cfg.h2_min
                    && st.interaction_z >= cfg.z_min
                    && st.interaction_variance >= cfg.var_share_min * target_variance
                {
                    pair_stats.push((ranked[a], ranked[b], st));
                }
            }
        }
        pair_stats.sort_by(|x, y| {
            y.2.interaction_variance
                .total_cmp(&x.2.interaction_variance)
                .then(x.0.cmp(&y.0))
                .then(x.1.cmp(&y.1))
        });
        pair_stats.truncate(cfg.max_pairs_per_target);
        for (pa, pb, st) in pair_stats {
            findings.push(make_finding(
                table,
                enc,
                target,
                &model,
                pa,
                pb,
                &st,
                target_variance,
                m,
                additive_r2,
            ));
        }
    }

    let coverage = format!(
        "{} numeric targets above the holdout R² bar ({}), {} fitted, {} clearing the additive-refit gate, {} pairs evaluated on {} sample rows; {} interactions reported",
        n_eligible,
        cfg.min_holdout_r2,
        n_fitted,
        n_gated,
        n_pairs,
        cfg.pd_rows,
        findings.len()
    );
    (findings, coverage)
}

#[allow(clippy::too_many_arguments)]
fn make_finding(
    table: &TypedTable,
    enc: &EncodedMatrix,
    target: &TargetResult,
    model: &FittedGbm,
    pa: usize,
    pb: usize,
    st: &HStats,
    target_variance: f64,
    pd_rows: usize,
    additive_r2: f64,
) -> Finding {
    let col_a = enc.feat_cols[model.selected[pa]];
    let col_b = enc.feat_cols[model.selected[pb]];
    let (name_a, name_b) = (&table.names[col_a], &table.names[col_b]);
    let tname = &target.name;

    let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    params.insert(
        "columns".into(),
        json!([name_a.clone(), name_b.clone(), tname.clone()]),
    );
    params.insert("inputs".into(), json!([name_a.clone(), name_b.clone()]));
    params.insert("target".into(), json!(tname.clone()));
    params.insert("column".into(), json!(tname.clone()));

    let share = st.interaction_variance / target_variance;
    let mut metrics: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    metrics.insert("form".into(), json!("interaction_h"));
    metrics.insert("h2".into(), json!(st.h2));
    metrics.insert(
        "interaction_variance".into(),
        json!(st.interaction_variance),
    );
    metrics.insert("interaction_se".into(), json!(st.interaction_se));
    metrics.insert("interaction_z".into(), json!(st.interaction_z));
    metrics.insert("joint_variance".into(), json!(st.joint_var));
    metrics.insert("first_variance".into(), json!(st.first_var));
    metrics.insert("second_variance".into(), json!(st.second_var));
    metrics.insert("target_variance".into(), json!(target_variance));
    metrics.insert("interaction_share_of_target".into(), json!(share));
    metrics.insert("holdout_r2".into(), json!(model.holdout_r2));
    metrics.insert("additive_holdout_r2".into(), json!(additive_r2));
    metrics.insert("n_holdout".into(), json!(model.n_holdout));
    metrics.insert("n_used".into(), json!(pd_rows));
    metrics.insert("pd_rows".into(), json!(pd_rows));
    metrics.insert("rounds".into(), json!(model.rounds));

    let claim = format!(
        "{} depends on {} and {} jointly: {:.1}% of the pair's joint partial-dependence variance is interaction (H² = {:.2}); the interaction variance is {} against a target variance of {} ({:.1}%), from a surrogate with holdout R² {:.2} against {:.2} for an additive refit",
        tname,
        name_a,
        name_b,
        100.0 * st.h2,
        st.h2,
        crate::fmt_float(st.interaction_variance),
        crate::fmt_float(target_variance),
        100.0 * share,
        model.holdout_r2,
        additive_r2
    );
    Finding::new(
        Pattern::PredictiveDependency,
        params,
        claim,
        EvidenceBasis::Measured,
        0.5 + 0.3 * st.h2,
        metrics,
        Some(format!(
            "an additive model in {name_a} and {name_b} leaves this interaction unmodelled; include the pair's interaction term or use a learner that represents interactions"
        )),
        Method::new(
            "interactions",
            "Friedman-Popescu H on partial dependences of the screen GBM, gated on the surrogate beating an additive refit out of sample",
            "onceover interaction screen v1 (issue #42)",
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h_stats_pure_interaction_is_one() {
        // Joint component = product term only: single components
        // are flat, so all of the joint variance is interaction.
        let jk: Vec<f64> = vec![1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0];
        let zero: Vec<f64> = vec![0.0; 8];
        let st = h_stats(&jk, &zero, &zero);
        assert!((st.h2 - 1.0).abs() < 1e-9, "h2 = {}", st.h2);
        assert!(st.interaction_variance > 0.0);
        assert!(st.interaction_z > 4.0, "z = {}", st.interaction_z);
    }

    #[test]
    fn h_stats_additive_is_zero() {
        let j: Vec<f64> = vec![0.0, 1.0, 2.0, 3.0, 0.0, 1.0, 2.0, 3.0];
        let k: Vec<f64> = vec![5.0, 5.0, 5.0, 5.0, 7.0, 7.0, 7.0, 7.0];
        let jk: Vec<f64> = j.iter().zip(&k).map(|(a, b)| a + b).collect();
        let st = h_stats(&jk, &j, &k);
        assert!(st.h2 < 1e-9, "h2 = {}", st.h2);
        assert_eq!(st.interaction_variance, 0.0);
    }
}
