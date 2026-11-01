//! Bounded M5-style model trees and retrospective residual shifts.
//!
//! Growth uses standard-deviation reduction, followed by corrected-error
//! pruning and ancestor smoothing. This is a profiler-oriented M5 variant,
//! not a claim of behavioral parity with Weka M5P. Numeric features and targets
//! are standardized on training data; reported equations use original units.

use onceover_report::{Method, Pattern};
use std::collections::BTreeMap;

use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding};
use serde_json::json;

use crate::{encode::EncodedMatrix, gbm::TargetResult};

mod tree;
pub use tree::{
    single_break_l2, BreakResult, ModelTree, ModelTreeConfig, RegimeLeaf, SplitCondition,
};

#[derive(Clone, Debug)]
pub struct RegimesConfig {
    pub model: ModelTreeConfig,
    pub row_cap: usize,
    pub target_cap: usize,
    pub min_holdout: usize,
    pub min_leaf_holdout: usize,
    pub min_r2: f64,
    pub min_error_reduction: f64,
}

impl Default for RegimesConfig {
    fn default() -> Self {
        Self {
            model: ModelTreeConfig::default(),
            row_cap: 5_000,
            target_cap: 4,
            min_holdout: 60,
            min_leaf_holdout: 10,
            min_r2: 0.7,
            min_error_reduction: 0.1,
        }
    }
}

fn numeric(col: &TypedCol<'_>, row: usize) -> Option<f64> {
    let v = match col {
        TypedCol::I64 { vals } => vals.get(row)?.map(|v| v as f64),
        TypedCol::U64 { vals } => vals.get(row)?.map(|v| v as f64),
        TypedCol::F64 { vals } => vals.get(row)?,
        _ => None,
    }?;
    v.is_finite().then_some(v)
}

fn time_at(col: &TypedCol<'_>, row: usize) -> Option<i64> {
    match col {
        TypedCol::Date { vals } => vals[row].map(i64::from),
        TypedCol::Datetime { .. } => col.datetime_micros(row),
        _ => None,
    }
}

fn holdout(row: u32) -> bool {
    let mut x = row as u64 ^ 0xD16A_0057_5EED;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    (x ^ (x >> 31)).is_multiple_of(5)
}

/// Numeric-target explanations, plus date-conditioned residual shifts. The
/// excluded columns are identifier/constant columns inferred by column cards.
pub fn profile_regimes(
    table: &TypedTable<'_>,
    enc: &EncodedMatrix,
    targets: &[TargetResult],
    excluded: &[usize],
    cfg: &RegimesConfig,
) -> (Vec<Finding>, String) {
    let mut candidates: Vec<_> = targets
        .iter()
        .filter(|r| {
            !r.is_categorical
                && r.metric >= 0.2
                && !excluded.contains(&r.col)
                && table.cols.get(r.col).is_some_and(|c| c.is_numeric())
        })
        .collect();
    candidates.sort_by(|a, b| b.metric.total_cmp(&a.metric).then(a.name.cmp(&b.name)));
    let eligible = candidates.len();
    candidates.truncate(cfg.target_cap);
    let sampled: Vec<u32> = crate::stride_rows(enc.rows.len(), cfg.row_cap.max(1))
        .into_iter()
        .map(|p| enc.rows[p as usize])
        .collect();
    let date = table
        .cols
        .iter()
        .position(|c| matches!(c, TypedCol::Date { .. } | TypedCol::Datetime { .. }));
    let (mut findings, mut fitted, mut incomplete, mut skipped) =
        (Vec::new(), 0usize, 0usize, 0usize);
    for res in &candidates {
        let mut features: Vec<usize> = res
            .top_features
            .iter()
            .filter_map(|(n, _)| table.names.iter().position(|v| v == n))
            .filter(|&i| i != res.col && !excluded.contains(&i) && table.cols[i].is_numeric())
            .collect();
        features.dedup();
        features.truncate(cfg.model.max_features);
        if features.is_empty() {
            features = enc
                .feat_cols
                .iter()
                .copied()
                .filter(|&i| i != res.col && !excluded.contains(&i) && table.cols[i].is_numeric())
                .take(cfg.model.max_features)
                .collect();
        }
        if features.is_empty() {
            skipped += 1;
            continue;
        }
        let (mut x, mut y, mut rows) = (Vec::new(), Vec::new(), Vec::new());
        for &r in &sampled {
            let vals: Option<Vec<f64>> = features
                .iter()
                .map(|&i| numeric(&table.cols[i], r as usize))
                .collect();
            if let (Some(vals), Some(v)) = (vals, numeric(&table.cols[res.col], r as usize)) {
                x.push(vals);
                y.push(v);
                rows.push(r);
            } else {
                incomplete += 1;
            }
        }
        let (train, test): (Vec<usize>, Vec<usize>) =
            (0..rows.len()).partition(|&i| !holdout(rows[i]));
        if train.len() / 2 < cfg.model.min_leaf || test.len() < cfg.min_holdout {
            skipped += 1;
            continue;
        }
        let xt: Vec<_> = train.iter().map(|&i| x[i].clone()).collect();
        let yt: Vec<_> = train.iter().map(|&i| y[i]).collect();
        let Ok(model) = ModelTree::fit(&xt, &yt, &cfg.model) else {
            skipped += 1;
            continue;
        };
        fitted += 1;
        let predictions: Option<Vec<_>> = test
            .iter()
            .map(|&i| {
                Some((
                    model.predict(&x[i])?,
                    model.predict_linear(&x[i])?,
                    model.leaf_index(&x[i])?,
                ))
            })
            .collect();
        let Some(predictions) = predictions else {
            skipped += 1;
            continue;
        };
        let test_mean = test.iter().map(|&i| y[i]).sum::<f64>() / test.len() as f64;
        let variance = test
            .iter()
            .map(|&i| ((y[i] - test_mean) / model.y_scale).powi(2))
            .sum::<f64>();
        let mut supports = vec![0usize; model.leaf_count()];
        let (mut error, mut baseline) = (0.0, 0.0);
        for (&i, &(p, b, leaf)) in test.iter().zip(&predictions) {
            supports[leaf] += 1;
            error += ((y[i] - p) / model.y_scale).powi(2);
            baseline += ((y[i] - b) / model.y_scale).powi(2);
        }
        if let Some(time_col) = date {
            if let Some(f) = residual_shift(
                table, res, &model, &x, &y, &rows, &train, &test, time_col, &features, cfg,
            ) {
                findings.push(f);
            }
        }
        if variance <= 1e-12 || baseline <= 1e-12 {
            continue;
        }
        let r2 = 1.0 - error / variance;
        let linear_r2 = 1.0 - baseline / variance;
        let gain = (baseline - error) / baseline;
        if model.leaf_count() < 2
            || r2 < cfg.min_r2
            || gain < cfg.min_error_reduction
            || supports.iter().any(|&n| n < cfg.min_leaf_holdout)
        {
            continue;
        }
        let names: Vec<_> = features.iter().map(|&i| table.names[i].clone()).collect();
        let leaves = model.leaves();
        if leaves
            .iter()
            .any(|l| !l.intercept.is_finite() || l.coefficients.iter().any(|v| !v.is_finite()))
        {
            continue;
        }
        let first = &leaves[0].conditions[0];
        let leaf_evidence: Vec<_> = leaves.iter().enumerate().map(|(j, l)| json!({
            "conditions": l.conditions.iter().map(|c| json!({"column": names[c.feature], "operator": if c.less_equal { "<=" } else { ">" }, "threshold": c.threshold})).collect::<Vec<_>>(),
            "intercept": l.intercept, "coefficients": names.iter().zip(&l.coefficients).map(|(n, c)| (n.clone(), *c)).collect::<BTreeMap<_, _>>(), "n_train": l.n_train, "n_holdout": supports[j]
        })).collect();
        findings.push(Finding::new(
Pattern::PiecewiseDependency,
BTreeMap::from([("target".into(), json!(res.name)), ("predictors".into(), json!(names))]),
format!("{} follows {} local linear regimes (first split {} ≤ {:.6}; holdout R² {:.3} versus {:.3} for one linear model)", res.name, leaves.len(), names[first.feature], first.threshold, r2, linear_r2),
EvidenceBasis::Estimated,
(0.6 + 0.25 * gain).min(0.9),
BTreeMap::from([("n_train".into(), json!(train.len())), ("n_holdout".into(), json!(test.len())), ("holdout_r2".into(), json!(r2)), ("global_linear_holdout_r2".into(), json!(linear_r2)), ("relative_error_reduction".into(), json!(gain)), ("leaves".into(), json!(leaf_evidence)), ("depth".into(), json!(model.depth())), ("pruning".into(), json!(cfg.model.prune)), ("smoothing".into(), json!(cfg.model.smoothing)), ("validation".into(), json!("deterministic diagnostic holdout; targets/features shortlisted by predictability screen; no inferential p-value"))]),
Some("inspect the regime conditions and equations before assuming a single global relationship".into()),
Method::new("regimes", "local_linear_model_tree", "bounded M5-style tree: SDR growth, QR linear leaves, corrected-error pruning, ancestor smoothing (onceover)")
));
    }
    let coverage = format!("{} of {eligible} eligible numeric targets (cap {}), {fitted} fitted over at most {} rows / {} predictors / depth {}; {} incomplete row-target cases dropped; {skipped} insufficient/invalid targets; {} findings (numeric predictors only; first date column if present; diagnostic holdout)", candidates.len(), cfg.target_cap, cfg.row_cap, cfg.model.max_features, cfg.model.max_depth, incomplete, findings.len());
    (findings, coverage)
}

#[allow(clippy::too_many_arguments)]
fn residual_shift(
    table: &TypedTable<'_>,
    res: &TargetResult,
    model: &ModelTree,
    x: &[Vec<f64>],
    y: &[f64],
    rows: &[u32],
    train: &[usize],
    test: &[usize],
    time_col: usize,
    features: &[usize],
    cfg: &RegimesConfig,
) -> Option<Finding> {
    let mut ordered: Vec<_> = train
        .iter()
        .filter_map(|&i| {
            Some((
                time_at(&table.cols[time_col], rows[i] as usize)?,
                (y[i] - model.predict_linear(&x[i])?) / model.y_scale,
            ))
        })
        .collect();
    ordered.sort_by_key(|v| v.0);
    let min_segment = cfg.model.min_leaf.max(ordered.len() / 10);
    let shift = single_break_l2(
        &ordered.iter().map(|v| v.1).collect::<Vec<_>>(),
        min_segment,
    )?;
    let threshold = ordered[shift.index - 1].0;
    if threshold >= ordered[shift.index].0 || shift.gain_fraction < 0.15 {
        return None;
    }
    let (mut before, mut after, mut base, mut corrected) = (0usize, 0usize, 0.0, 0.0);
    for &i in test {
        let Some(t) = time_at(&table.cols[time_col], rows[i] as usize) else {
            continue;
        };
        let residual = (y[i] - model.predict_linear(&x[i])?) / model.y_scale;
        let correction = if t <= threshold {
            before += 1;
            shift.left_mean
        } else {
            after += 1;
            shift.right_mean
        };
        base += residual * residual;
        corrected += (residual - correction).powi(2);
    }
    if before < cfg.min_leaf_holdout || after < cfg.min_leaf_holdout || base <= 1e-12 {
        return None;
    }
    let gain = (base - corrected) / base;
    if gain < 0.15 {
        return None;
    }
    let display = match &table.cols[time_col] {
        TypedCol::Date { .. } => onceover_core::dateutil::iso_date(threshold),
        _ => onceover_core::dateutil::iso_datetime(threshold),
    };
    let names: Vec<_> = features.iter().map(|&i| table.names[i].clone()).collect();
    let (intercept, coefficients) = model.global_linear_model();
    let coefficients: BTreeMap<_, _> = names.iter().cloned().zip(coefficients).collect();
    Some(Finding::new(
Pattern::ResidualShift,
BTreeMap::from([("target".into(), json!(res.name)), ("time_column".into(), json!(table.names[time_col])), ("predictors".into(),json!(names))]),
format!("{} shifts after {} along {}: a training-selected residual correction reduces holdout squared error by {:.1}%", res.name, display, table.names[time_col], 100.0 * gain),
EvidenceBasis::Estimated,
(0.6 + 0.25 * gain).min(0.9),
BTreeMap::from([("breakpoint".into(), json!(display)), ("breakpoint_raw".into(), json!(threshold)), ("train_before".into(), json!(shift.index)), ("train_after".into(), json!(ordered.len() - shift.index)), ("holdout_before".into(), json!(before)), ("holdout_after".into(), json!(after)), ("residual_mean_before".into(), json!(shift.left_mean * model.y_scale)), ("residual_mean_after".into(), json!(shift.right_mean * model.y_scale)), ("holdout_error_reduction".into(), json!(gain)), ("baseline".into(), json!("global linear numeric model")), ("reference_model".into(),json!({"form":"linear","intercept":intercept,"coefficients":coefficients})), ("n_train".into(),json!(ordered.len())), ("n_holdout".into(),json!(before+after)), ("validation".into(), json!("breakpoint and offsets learned on training rows; fixed correction evaluated on holdout; no p-value"))]),
Some("check for changes in collection, processing, or business behavior around this date".into()),
Method::new("regimes", "residual_breakpoint", "centered cumulative-sum one-break L2 scan of linear-model residuals; holdout validation (onceover)")
))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(n: usize, pure_linear: bool) -> (Vec<Vec<f64>>, Vec<f64>) {
        let mut r = crate::gbm::Lcg(0x12345678);
        let mut x = Vec::new();
        let mut y = Vec::new();
        for _ in 0..n {
            let a = 2.0 * r.next_f64() - 1.0;
            let b = 2.0 * r.next_f64() - 1.0;
            x.push(vec![a, b, b]);
            y.push(if pure_linear || a < 0.0 {
                1.0 + 2.0 * b
            } else {
                -2.0 - 3.0 * b
            });
        }
        (x, y)
    }

    #[test]
    fn pruning_removes_spurious_linear_splits_and_handles_collinearity() {
        let (x, y) = data(800, true);
        let cfg = ModelTreeConfig {
            smoothing: 0.0,
            ..Default::default()
        };
        let fit = ModelTree::fit(&x, &y, &cfg).unwrap();
        assert_eq!(fit.leaf_count(), 1);
        for i in 0..x.len() {
            assert!((fit.predict(&x[i]).unwrap() - y[i]).abs() < 1e-5);
        }
        let unpruned = ModelTree::fit(
            &x,
            &y,
            &ModelTreeConfig {
                prune: false,
                ..cfg
            },
        )
        .unwrap();
        assert!(unpruned.leaf_count() > fit.leaf_count());
    }

    #[test]
    fn scale_equivariance_and_exported_smoothed_equations() {
        let (x, y) = data(1000, false);
        let cfg = ModelTreeConfig::default();
        let fit = ModelTree::fit(&x, &y, &cfg).unwrap();
        assert!(fit.leaf_count() >= 2);
        let leaves = fit.leaves();
        for row in &x {
            let leaf = &leaves[fit.leaf_index(row).unwrap()];
            let from_equation = leaf.intercept
                + leaf
                    .coefficients
                    .iter()
                    .zip(row)
                    .map(|(a, b)| a * b)
                    .sum::<f64>();
            assert!((from_equation - fit.predict(row).unwrap()).abs() < 1e-9);
        }
        for scale in [1e-6, 1e6] {
            let ys: Vec<_> = y.iter().map(|v| v * scale).collect();
            let m = ModelTree::fit(&x, &ys, &cfg).unwrap();
            assert_eq!(m.leaf_count(), fit.leaf_count());
            for row in &x {
                assert!((m.predict(row).unwrap() / scale - fit.predict(row).unwrap()).abs() < 1e-6);
            }
        }
        let no_smooth = ModelTree::fit(
            &x,
            &y,
            &ModelTreeConfig {
                smoothing: 0.0,
                ..cfg
            },
        )
        .unwrap();
        assert!(x
            .iter()
            .any(|row| (no_smooth.predict(row).unwrap() - fit.predict(row).unwrap()).abs() > 1e-6));
    }

    #[test]
    fn invalid_and_constant_inputs_are_explicit() {
        assert!(ModelTree::fit(
            &[vec![f64::NAN], vec![1.0]],
            &[1.0, 2.0],
            &Default::default()
        )
        .is_err());
        assert!(ModelTree::fit(&[vec![0.0], vec![]], &[1.0, 2.0], &Default::default()).is_err());
        let x = vec![vec![1e9, 1.0]; 80];
        let y = vec![5.0; 80];
        let m = ModelTree::fit(&x, &y, &Default::default()).unwrap();
        assert_eq!(m.predict(&[1e9, 1.0]), Some(5.0));
        assert!(m.predict(&[f64::INFINITY, 1.0]).is_none());
        assert!(ModelTree::fit(
            &x,
            &y,
            &ModelTreeConfig {
                ridge: 1e300,
                ..Default::default()
            }
        )
        .is_err());
        assert_eq!(
            ModelTree::fit(
                &x,
                &y,
                &ModelTreeConfig {
                    min_leaf: usize::MAX,
                    ..Default::default()
                }
            )
            .unwrap()
            .leaf_count(),
            1
        );
    }

    #[test]
    fn repeated_numeric_gate_values_are_valid_split_candidates() {
        let (mut x, _) = data(1001, false);
        for (i, row) in x.iter_mut().enumerate() {
            row[0] = (i % 2) as f64;
        }
        let y: Vec<_> = x
            .iter()
            .map(|r| {
                if r[0] == 0.0 {
                    1.0 + 2.0 * r[1]
                } else {
                    -2.0 - 3.0 * r[1]
                }
            })
            .collect();
        let m = ModelTree::fit(&x, &y, &Default::default()).unwrap();
        assert_eq!(m.leaf_count(), 2);
        assert_eq!(m.leaves()[0].conditions[0].feature, 0);
    }

    #[test]
    fn one_break_rejects_invalid_data_and_finds_offset_signal() {
        let mut v = vec![1e12; 500];
        v.extend(vec![1e12 + 5.0; 500]);
        let b = single_break_l2(&v, 30).unwrap();
        assert_eq!(b.index, 500);
        assert!(b.gain_fraction > 0.9999);
        assert!(single_break_l2(&[1.0; 100], 10).is_none());
        assert!(single_break_l2(&[0.0, f64::NAN, 1.0, 2.0], 2).is_none());
        assert!(single_break_l2(&[0.0; 4], usize::MAX).is_none());
    }

    fn target() -> TargetResult {
        TargetResult {
            col: 2,
            name: "y".into(),
            is_categorical: false,
            metric: 0.95,
            baseline: 0.0,
            n_train: 0,
            n_holdout: 0,
            rounds: 0,
            inputs: Vec::new(),
            top_features: vec![("gate".into(), 0.6), ("x".into(), 0.4)],
            cat_ids: None,
        }
    }

    #[test]
    fn report_requires_holdout_gain_and_retains_original_unit_equations() {
        let (features, values) = data(2400, false);
        let table = TypedTable {
            n: values.len(),
            names: vec!["gate".into(), "x".into(), "y".into()],
            dtypes: vec!["float64".into(); 3],
            cols: vec![
                TypedCol::F64 {
                    vals: features
                        .iter()
                        .enumerate()
                        .map(|(i, r)| (i % 17 != 0).then_some(r[0]))
                        .collect::<Vec<_>>()
                        .into(),
                },
                TypedCol::F64 {
                    vals: features
                        .iter()
                        .map(|r| Some(r[1]))
                        .collect::<Vec<_>>()
                        .into(),
                },
                TypedCol::F64 {
                    vals: values.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
            ],
        };
        let (enc, _) = crate::encode::encode(&table, &crate::EncodeConfig::default());
        let cfg = RegimesConfig {
            row_cap: 600,
            ..Default::default()
        };
        let (f, cov) = profile_regimes(&table, &enc, &[target()], &[], &cfg);
        assert_eq!(f.len(), 1, "{cov}");
        assert_eq!(f[0].specification.form.as_deref(), Some("piecewise_linear"));
        assert!(f[0].metrics()["holdout_r2"].as_f64().unwrap() > 0.95);
        let train = f[0].metrics()["n_train"].as_u64().unwrap();
        let held = f[0].metrics()["n_holdout"].as_u64().unwrap();
        assert!(train > 0 && held > 0 && train + held < table.n as u64);
        assert!(train + held <= cfg.row_cap as u64);
        let text = serde_json::to_string(&f).unwrap();
        assert_eq!(
            text,
            serde_json::to_string(&profile_regimes(&table, &enc, &[target()], &[], &cfg).0)
                .unwrap()
        );
        assert!(
            profile_regimes(&table, &enc, &[target()], &[0, 1], &Default::default())
                .0
                .is_empty()
        );
    }

    #[test]
    fn noise_target_is_suppressed_even_if_shortlisted() {
        let (features, _) = data(1800, false);
        let mut rng = crate::gbm::Lcg(0xDEADBEEF);
        let table = TypedTable {
            n: features.len(),
            names: vec!["gate".into(), "x".into(), "y".into()],
            dtypes: vec!["float64".into(); 3],
            cols: vec![
                TypedCol::F64 {
                    vals: features
                        .iter()
                        .map(|r| Some(r[0]))
                        .collect::<Vec<_>>()
                        .into(),
                },
                TypedCol::F64 {
                    vals: features
                        .iter()
                        .map(|r| Some(r[1]))
                        .collect::<Vec<_>>()
                        .into(),
                },
                TypedCol::F64 {
                    vals: features
                        .iter()
                        .map(|_| Some(rng.next_f64()))
                        .collect::<Vec<_>>()
                        .into(),
                },
            ],
        };
        let (enc, _) = crate::encode::encode(&table, &crate::EncodeConfig::default());
        assert!(
            profile_regimes(&table, &enc, &[target()], &[], &Default::default())
                .0
                .is_empty()
        );
    }

    #[test]
    fn date_residual_shift_uses_training_break_and_holdout_correction() {
        let n = 2000;
        let mut rng = crate::gbm::Lcg(1234);
        let x: Vec<_> = (0..n).map(|_| rng.next_f64()).collect();
        let table = TypedTable {
            n,
            names: vec!["gate".into(), "x".into(), "y".into(), "date".into()],
            dtypes: vec![
                "float64".into(),
                "float64".into(),
                "float64".into(),
                "date".into(),
            ],
            cols: vec![
                TypedCol::F64 {
                    vals: x.iter().map(|v| Some(v * 2.0)).collect::<Vec<_>>().into(),
                },
                TypedCol::F64 {
                    vals: x.iter().copied().map(Some).collect::<Vec<_>>().into(),
                },
                TypedCol::F64 {
                    vals: x
                        .iter()
                        .enumerate()
                        .map(|(i, v)| Some(v * 3.0 + if i < n / 2 { 0.0 } else { 5.0 }))
                        .collect::<Vec<_>>()
                        .into(),
                },
                TypedCol::Date {
                    vals: (0..n).map(|i| Some(20000 + i as i32)).collect(),
                },
            ],
        };
        let (enc, _) = crate::encode::encode(&table, &crate::EncodeConfig::default());
        let (f, _) = profile_regimes(&table, &enc, &[target()], &[], &Default::default());
        let shift = f
            .iter()
            .find(|f| f.category == onceover_report::Category::Changes)
            .expect("planted date shift");
        assert!(shift.metrics()["holdout_error_reduction"].as_f64().unwrap() > 0.9);
        assert!((shift.metrics()["breakpoint_raw"].as_i64().unwrap() - 21000).abs() < 10);
    }
}
