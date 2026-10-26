//! PRISM (Cendrowska 1987) rule induction for the most predictable
//! categorical targets: compact modular covering rules with precision and
//! support — the quoteable "how is this column determined".
//!
//! Hot path uses precomputed bitset masks: candidate evaluation is a few
//! hundred word-ANDs instead of row scans.

use onceover_report::{Method, Pattern};
use std::collections::BTreeMap;

use onceover_report::{EvidenceBasis, Finding};
use serde_json::json;

use crate::encode::{EncodedMatrix, MISSING};
use crate::gbm::TargetResult;

pub struct PrismConfig {
    pub max_rules_per_class: usize,
    pub max_conditions: usize,
    pub min_support: usize,
    pub min_precision: f64,
    /// Features used for rules: top-K by GBM gain.
    pub max_features: usize,
    pub min_target_accuracy: f64,
    pub max_classes: usize,
}

impl Default for PrismConfig {
    fn default() -> Self {
        PrismConfig {
            max_rules_per_class: 3,
            max_conditions: 4,
            min_support: 20,
            min_precision: 0.8,
            max_features: 12,
            min_target_accuracy: 0.9,
            max_classes: 10,
        }
    }
}

/// Bitset over enc.n positions.
struct Mask {
    words: Vec<u64>,
}

impl Mask {
    fn new(n: usize) -> Mask {
        Mask {
            words: vec![0; n.div_ceil(64)],
        }
    }
    fn set(&mut self, i: usize) {
        self.words[i / 64] |= 1u64 << (i % 64);
    }
    fn count(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }
    fn and(&self, other: &Mask) -> Mask {
        Mask {
            words: self
                .words
                .iter()
                .zip(other.words.iter())
                .map(|(a, b)| a & b)
                .collect(),
        }
    }
    fn and_not(&self, other: &Mask) -> Mask {
        Mask {
            words: self
                .words
                .iter()
                .zip(other.words.iter())
                .map(|(a, b)| a & !b)
                .collect(),
        }
    }
}

pub fn prism_rules(
    enc: &EncodedMatrix,
    targets: &[&TargetResult],
    cfg: &PrismConfig,
) -> (Vec<Finding>, String) {
    let mut findings = Vec::new();
    let mut searched = 0;

    for res in targets {
        let Some((ids, labels)) = res.cat_ids.as_ref() else {
            continue;
        };
        if res.metric < cfg.min_target_accuracy || labels.len() > cfg.max_classes {
            continue;
        }
        // A near-majority target has nothing to explain; PRISM on it only
        // finds lucky high-precision pockets (precision-first guard).
        if res.baseline >= 0.9 {
            continue;
        }
        searched += 1;

        // Feature pool: top by GBM gain; the target's own column is never
        // a condition (in either path — self-conditions are trivial rules).
        let own_feat: Option<usize> = enc.feat_cols.iter().position(|&c| c == res.col);
        let mut feat_pool: Vec<usize> = res
            .top_features
            .iter()
            .filter_map(|(n, _)| enc.feat_names.iter().position(|f| f == n))
            .filter(|&f| Some(f) != own_feat)
            .collect();
        if feat_pool.len() < 2 {
            feat_pool = (0..enc.bins.len())
                .filter(|&f| Some(f) != own_feat)
                .collect();
        }
        feat_pool.truncate(cfg.max_features);
        if feat_pool.is_empty() {
            continue;
        }

        let positions: Vec<u32> = (0..enc.n)
            .filter(|&j| ids[j] != MISSING)
            .map(|j| j as u32)
            .collect();

        // Precompute masks: one per (feature, code) over `positions`,
        // plus one per class.
        let n_pos = positions.len();
        let mut masks: Vec<Vec<Mask>> = Vec::with_capacity(feat_pool.len()); // [feat][code]
        for &f in &feat_pool {
            let mut by_code: Vec<Mask> = Vec::new();
            for _ in 0..256 {
                by_code.push(Mask::new(n_pos));
            }
            for (i, &p) in positions.iter().enumerate() {
                let c = enc.bins[f][p as usize] as usize;
                if c != 255 {
                    by_code[c].set(i);
                }
            }
            masks.push(by_code);
        }
        let mut class_masks: Vec<Mask> = Vec::with_capacity(labels.len());
        for _ in 0..labels.len() {
            class_masks.push(Mask::new(n_pos));
        }
        for (i, &p) in positions.iter().enumerate() {
            class_masks[ids[p as usize] as usize].set(i);
        }

        // Classes by frequency (deterministic).
        let mut class_counts = vec![0u32; labels.len()];
        for &p in &positions {
            class_counts[ids[p as usize] as usize] += 1;
        }
        let mut class_order: Vec<usize> = (0..labels.len()).collect();
        class_order.sort_by(|&a, &b| class_counts[b].cmp(&class_counts[a]).then(a.cmp(&b)));
        class_order.truncate(5);

        for &class in &class_order {
            if (class_counts[class] as usize) < cfg.min_support {
                continue;
            }
            let mut covered = Mask::new(n_pos);
            for _ in 0..cfg.max_rules_per_class {
                // Uncovered positives for this class.
                let uncovered_pos = class_masks[class].and_not(&covered);
                let remaining_count = uncovered_pos.count();
                if (remaining_count as usize) < cfg.min_support {
                    break;
                }

                // Grow a rule via bitset ANDs.
                let mut conds: Vec<(usize, u8)> = Vec::new(); // (feat pool idx, code)
                let mut cur = Mask::new(n_pos);
                for w in cur.words.iter_mut() {
                    *w = u64::MAX;
                }
                let mut acc = 0.0f64;
                let mut support = 0usize;
                for _ in 0..cfg.max_conditions {
                    let mut best: Option<(f64, u32, usize, u8)> = None; // (acc, cov, feat_i, code)
                    for (fi, &_f) in feat_pool.iter().enumerate() {
                        if conds.iter().any(|(cf, _)| *cf == fi) {
                            continue;
                        }
                        for (code, m) in masks[fi].iter().enumerate() {
                            if m.count() == 0 {
                                continue;
                            }
                            let m = cur.and(&masks[fi][code]);
                            let hits = m.count();
                            if hits == 0 {
                                continue;
                            }
                            let pos_hits = m.and(&uncovered_pos).count();
                            // Further conditions can only remove positives. A tiny
                            // pure pocket must not displace an emittable rule.
                            if (pos_hits as usize) < cfg.min_support {
                                continue;
                            }
                            let a = pos_hits as f64 / hits as f64;
                            let better = match best {
                                Some((ba, bc, _, _)) => {
                                    a > ba + 1e-9 || (a > ba - 1e-9 && pos_hits > bc)
                                }
                                None => true,
                            };
                            if better {
                                best = Some((a, pos_hits, fi, code as u8));
                            }
                        }
                    }
                    let Some((a, cov, fi, code)) = best else {
                        break;
                    };
                    if conds.is_empty() && a < 0.05 {
                        break;
                    }
                    if !conds.is_empty() && a <= acc + 1e-9 {
                        break;
                    }
                    conds.push((fi, code));
                    let m = cur.and(&masks[fi][code as usize]);
                    cur = m;
                    acc = a;
                    support = cov as usize;
                    if acc >= 1.0 - 1e-9 {
                        break;
                    }
                }
                if conds.is_empty() || acc < cfg.min_precision || support < cfg.min_support {
                    break;
                }

                // Emit the rule.
                let cond_strs: Vec<String> = conds
                    .iter()
                    .map(|(fi, c)| {
                        let f = feat_pool[*fi];
                        let name = &enc.feat_names[f];
                        if enc.feat_nominal[f] {
                            format!("{name} = {}", enc.bin_label(f, *c))
                        } else {
                            format!("{name} {}", enc.bin_label(f, *c))
                        }
                    })
                    .collect();
                findings.push(
                    Finding::new(
                        Pattern::ConditionalRule,
                        BTreeMap::from([
                            ("column".into(), json!(res.name)),
                            ("class".into(), json!(labels[class])),
                        ]),
                        format!(
                            "{} = '{}' when {} (precision {:.2}, support {})",
                            res.name,
                            labels[class],
                            cond_strs.join(" and "),
                            acc,
                            support
                        ),
                        EvidenceBasis::Estimated,
                        0.55 + 0.25 * acc,
                        BTreeMap::from([
                            ("conditions".into(), json!(cond_strs)),
                            ("discovery_score".into(), json!(acc)),
                            ("newly_covered_positives".into(), json!(support)),
                            ("rows_used".into(), json!(enc.n)),
                        ]),
                        None,
                        Method::new(
                            "prism",
                            "prism_covering",
                            "PRISM covering algorithm (Cendrowska 1987; onceover v0.1)",
                        ),
                    )
                    .with_rule(
                        conds
                            .iter()
                            .flat_map(|(fi, code)| enc.predicates(feat_pool[*fi], *code))
                            .collect(),
                        onceover_report::Predicate::equal(
                            res.name.clone(),
                            enc.literal(own_feat.expect("encoded target"), &labels[class]),
                        ),
                    ),
                );
                // Mark covered positives.
                for (i, &p) in positions.iter().enumerate() {
                    if ids[p as usize] as usize == class
                        && cur.words[i / 64] & (1u64 << (i % 64)) != 0
                    {
                        covered.set(i);
                    }
                }
            }
        }
    }
    let coverage = format!("{searched} targets eligible, {} rules", findings.len());
    (findings, coverage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::{encode, EncodeConfig};
    use onceover_core::extract;
    use polars::df;

    #[test]
    fn tiny_pure_pockets_do_not_displace_supported_noisy_rules() {
        let n = 200;
        let signal: Vec<_> = (0..n).map(|i| if i < 100 { "a" } else { "b" }).collect();
        let ids: Vec<u8> = (0..n)
            .map(|i| ((i >= 100) ^ (i % 100 >= 90)) as u8)
            .collect();
        // Each class has a pure five-row pocket, versus a 90%-precise
        // hundred-row signal. Precision alone prefers the unusable pockets.
        let pocket: Vec<_> = (0..n)
            .map(|i| match i {
                0..5 => "rare_a",
                100..105 => "rare_b",
                _ => "common",
            })
            .collect();
        let df = df! {
            "signal" => signal,
            "pocket" => pocket,
            "target" => ids.iter().map(|&v| i64::from(v)).collect::<Vec<_>>(),
        }
        .unwrap();
        let (enc, _) = encode(&extract(&df), &EncodeConfig::default());
        let target = TargetResult {
            col: 2,
            name: "target".into(),
            is_categorical: true,
            metric: 0.9,
            baseline: 0.5,
            n_train: 160,
            n_holdout: 40,
            rounds: 1,
            inputs: Vec::new(),
            top_features: vec![],
            cat_ids: Some((ids, vec!["0".into(), "1".into()])),
        };
        let cfg = PrismConfig {
            max_rules_per_class: 1,
            ..Default::default()
        };
        let (rules, _) = prism_rules(&enc, &[&target], &cfg);
        assert_eq!(rules.len(), 2);
        for rule in &rules {
            assert_eq!(rule.metrics()["newly_covered_positives"], json!(90));
            assert_eq!(rule.metrics()["discovery_score"], json!(0.9));
            let conditions = rule.metrics()["conditions"].as_array().unwrap();
            assert_eq!(conditions.len(), 1);
            assert!(conditions[0].as_str().unwrap().starts_with("signal = "));
        }
    }
}
