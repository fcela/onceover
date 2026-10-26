//! Kohavi-style decision tables (Weka `DecisionTable` lineage): small
//! feature subsets + cell-majority lookup, cross-validated. The predictive
//! twin of approximate FDs — "these columns determine this one".
//!
//! Hot path is allocation-free: coarse codes are precomputed once per
//! feature and cell keys are packed into u64.

use onceover_report::{Method, Pattern};
use std::collections::BTreeMap;

use onceover_report::{EvidenceBasis, Finding};
use rustc_hash::FxHashMap;
use serde_json::json;

use crate::encode::{EncodedMatrix, MISSING};
use crate::gbm::TargetResult;

pub struct DtableConfig {
    pub max_subset: usize,
    pub min_gain: f64,
    pub min_accuracy: f64,
    pub min_baseline_gain: f64,
    pub folds: usize,
    /// Restrict the search to the top-K features by GBM gain.
    pub max_features: usize,
}

impl Default for DtableConfig {
    fn default() -> Self {
        DtableConfig {
            max_subset: 3,
            min_gain: 0.01,
            min_accuracy: 0.85,
            min_baseline_gain: 0.05,
            folds: 5,
            max_features: 16,
        }
    }
}

/// Coarse feature codes (precomputed once): numeric fine bins -> 8 buckets.
fn coarse_code(enc: &EncodedMatrix, f: usize, pos: usize) -> u8 {
    let v = enc.bins[f][pos];
    if v == MISSING {
        return MISSING;
    }
    if enc.feat_nominal[f] {
        v
    } else {
        v / 4
    }
}

/// CV accuracy of a cell-majority table over `subset` (u64-packed keys).
fn cv_accuracy(
    coarse: &[Vec<u8>],
    fold_of: &[u8],
    target_ids: &[u8],
    labels_k: usize,
    subset: &[usize],
    positions: &[u32],
    folds: usize,
) -> f64 {
    if positions.len() < folds * 10 {
        return 0.0;
    }
    let pack = |subset: &[usize], p: u32| -> u64 {
        // 0 = missing/global; codes are 1-based in the packed key.
        let mut key: u64 = 0;
        for (i, &feat) in subset.iter().enumerate() {
            let c = coarse[feat][p as usize];
            if c == MISSING {
                return 0;
            }
            key |= (c as u64 + 1) << (8 * i);
        }
        key
    };

    let mut correct = 0usize;
    let mut total = 0usize;
    for f in 0..folds {
        let mut table: FxHashMap<u64, Vec<u32>> = FxHashMap::default();
        let mut global = vec![0u32; labels_k];
        for &p in positions {
            let class = target_ids[p as usize];
            if class == MISSING {
                continue;
            }
            if fold_of[p as usize] as usize == f {
                continue; // holdout
            }
            global[class as usize] += 1;
            let key = pack(subset, p);
            if key != 0 {
                table.entry(key).or_insert_with(|| vec![0; labels_k])[class as usize] += 1;
            }
        }
        let global_mode = global
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.cmp(b.1).then(a.0.cmp(&b.0)))
            .map(|(c, _)| c)
            .unwrap_or(0);
        for &p in positions {
            if fold_of[p as usize] as usize != f {
                continue;
            }
            let class = target_ids[p as usize];
            if class == MISSING {
                continue;
            }
            total += 1;
            let key = pack(subset, p);
            let pred = if key == 0 {
                global_mode
            } else {
                match table.get(&key) {
                    Some(counts) => counts
                        .iter()
                        .enumerate()
                        .max_by(|a, b| a.1.cmp(b.1).then(a.0.cmp(&b.0)))
                        .map(|(c, _)| c)
                        .unwrap_or(global_mode),
                    None => global_mode,
                }
            };
            if pred == class as usize {
                correct += 1;
            }
        }
    }
    correct as f64 / total.max(1) as f64
}

/// Run decision-table search on categorical targets (results must carry
/// their class encodings; the pipeline selects which targets to include).
pub fn decision_tables(
    enc: &EncodedMatrix,
    targets: &[&TargetResult],
    cfg: &DtableConfig,
) -> (Vec<Finding>, String) {
    let mut findings = Vec::new();

    // Precompute coarse codes for all features once.
    let coarse: Vec<Vec<u8>> = (0..enc.bins.len())
        .map(|f| (0..enc.n).map(|pos| coarse_code(enc, f, pos)).collect())
        .collect();

    for res in targets {
        let Some((ids, labels)) = res.cat_ids.as_ref() else {
            continue;
        };
        let positions: Vec<u32> = (0..enc.n)
            .filter(|&j| ids[j] != MISSING)
            .map(|j| j as u32)
            .collect();
        let k = labels.len();
        let mut counts = vec![0u32; k];
        for &p in &positions {
            counts[ids[p as usize] as usize] += 1;
        }
        let baseline = *counts.iter().max().unwrap_or(&0) as f64 / positions.len().max(1) as f64;
        let own_feat = enc.feat_cols.iter().position(|&c| c == res.col);

        // Feature pool: top by GBM gain, fallback to all.
        let mut pool: Vec<usize> = res
            .top_features
            .iter()
            .filter_map(|(n, _)| enc.feat_names.iter().position(|f| f == n))
            .filter(|&f| Some(f) != own_feat)
            .collect();
        if pool.len() < 2 {
            pool = (0..enc.bins.len())
                .filter(|&f| Some(f) != own_feat)
                .collect();
        }
        pool.truncate(cfg.max_features);
        if pool.is_empty() {
            continue;
        }

        // Deterministic folds.
        let fold_of: Vec<u8> = {
            let mut rng = crate::gbm::Lcg(0xD1CE_B0CA_5EED ^ (res.col as u64));
            let mut perm: Vec<u32> = positions.clone();
            for i in (1..perm.len()).rev() {
                let j = (rng.next_u64() % (i as u64 + 1)) as usize;
                perm.swap(i, j);
            }
            let mut fold = vec![0u8; enc.n];
            for (i, &p) in perm.iter().enumerate() {
                fold[p as usize] = (i % cfg.folds) as u8;
            }
            fold
        };

        // Greedy forward selection.
        let mut subset: Vec<usize> = Vec::new();
        let mut acc = baseline;
        loop {
            let mut best_gain = 0.0;
            let mut best_feat = usize::MAX;
            let mut best_acc = acc;
            for &f in &pool {
                if subset.contains(&f) {
                    continue;
                }
                let mut trial = subset.clone();
                trial.push(f);
                let a = cv_accuracy(&coarse, &fold_of, ids, k, &trial, &positions, cfg.folds);
                let gain = a - acc;
                if gain > best_gain + 1e-9 {
                    best_gain = gain;
                    best_feat = f;
                    best_acc = a;
                }
            }
            if best_feat == usize::MAX || best_gain < cfg.min_gain || subset.len() >= cfg.max_subset
            {
                break;
            }
            subset.push(best_feat);
            subset.sort_unstable();
            acc = best_acc;
            if acc >= 0.999 {
                break; // already a perfect lookup
            }
        }

        if acc >= cfg.min_accuracy && acc >= baseline + cfg.min_baseline_gain && !subset.is_empty()
        {
            let feats: Vec<&str> = subset.iter().map(|&f| enc.feat_names[f].as_str()).collect();
            findings.push(Finding::new(
Pattern::PredictiveDependency,
BTreeMap::from([("column".into(), json!(res.name))]),
format!(
                    "{} is predicted from [{}] with {:.3} cross-validated accuracy (majority baseline {:.3})",
                    res.name,
                    feats.join(", "),
                    acc,
                    baseline
                ),
EvidenceBasis::Estimated,
0.80,
BTreeMap::from([
                    ("features".into(), json!(feats)),
                    ("cv_accuracy".into(), json!(acc)),
                    ("baseline".into(), json!(baseline)),
                    ("n".into(), json!(positions.len())),
                    ("folds".into(), json!(cfg.folds)),
                ]),
Some(format!(
                    "inspect the validation of {} predicted from {}",
                    res.name,
                    feats.join(", ")
                )),
Method::new("decision_tables", "cross_validated_lookup", "Kohavi decision-table search (Weka DecisionTable lineage; onceover v0.1)")
));
        }
    }
    let coverage = format!(
        "{} categorical targets searched, {} tables found",
        targets.len(),
        findings.len()
    );
    (findings, coverage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::{encode, EncodeConfig};
    use onceover_core::extract;
    use polars::df;

    #[test]
    fn cell_keys_preserve_every_level_and_feature() {
        let n = 400;
        let coarse = vec![
            (0..n).map(|i| (i % 4) as u8).collect(),
            (0..n).map(|i| ((i / 4) % 2) as u8).collect(),
        ];
        let ids: Vec<_> = (0..n).map(|i| (i % 8) as u8).collect();
        let fold: Vec<_> = (0..n).map(|i| ((i / 8) % 5) as u8).collect();
        let positions: Vec<_> = (0..n as u32).collect();
        assert_eq!(
            cv_accuracy(&coarse, &fold, &ids, 8, &[0, 1], &positions, 5),
            1.0
        );
        assert_eq!(
            cv_accuracy(&coarse, &fold, &ids, 8, &[1, 0], &positions, 5),
            1.0
        );
    }

    #[test]
    fn perfect_lookup_with_missing_target_rows_is_emitted() {
        let n = 400;
        let feature: Vec<_> = (0..n).map(|i| if i % 2 == 0 { "a" } else { "b" }).collect();
        // Missing targets at both ends keep positions sparse, including a
        // valid row index greater than the number of non-missing targets.
        let ids: Vec<_> = (0..n)
            .map(|i| if i % 13 == 0 { MISSING } else { (i % 2) as u8 })
            .collect();
        let df = df! { "feature" => feature }.unwrap();
        let (enc, _) = encode(&extract(&df), &EncodeConfig::default());
        let target = TargetResult {
            col: 1,
            name: "target".into(),
            is_categorical: true,
            metric: 1.0,
            baseline: 0.5,
            n_train: 300,
            n_holdout: 100,
            rounds: 1,
            inputs: Vec::new(),
            top_features: vec![],
            cat_ids: Some((ids, vec!["0".into(), "1".into()])),
        };
        let (tables, _) = decision_tables(&enc, &[&target], &DtableConfig::default());
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].metrics()["features"], json!(["feature"]));
        assert_eq!(tables[0].metrics()["cv_accuracy"], json!(1.0));
        assert_eq!(tables[0].metrics()["n"], json!(369));
    }
}
