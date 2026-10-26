//! Bounded pairwise association rules for nominal encoded features.

use crate::encode::{EncodedMatrix, MISSING};
use onceover_report::{EvidenceBasis, Finding};
use onceover_report::{Method, Pattern};
use serde_json::json;
use std::collections::BTreeMap;

pub struct AssociationConfig {
    pub max_levels: usize,
    pub min_support: usize,
    pub min_lift: f64,
    pub min_confidence: f64,
    pub max_findings: usize,
}

impl Default for AssociationConfig {
    fn default() -> Self {
        Self {
            max_levels: 16,
            min_support: 30,
            min_lift: 1.5,
            min_confidence: 0.75,
            max_findings: 100,
        }
    }
}

#[derive(Clone)]
struct Mask(Vec<u64>);
impl Mask {
    fn new(n: usize) -> Self {
        Self(vec![0; n.div_ceil(64)])
    }
    fn set(&mut self, i: usize) {
        self.0[i / 64] |= 1u64 << (i % 64);
    }
    fn count(&self) -> usize {
        self.0.iter().map(|w| w.count_ones() as usize).sum()
    }
    fn and_count(&self, rhs: &Self) -> usize {
        self.0
            .iter()
            .zip(&rhs.0)
            .map(|(a, b)| (a & b).count_ones() as usize)
            .sum()
    }
    fn intersection(&self, rhs: &Self) -> Self {
        Self(self.0.iter().zip(&rhs.0).map(|(a, b)| a & b).collect())
    }
}

fn holdout(row: u32) -> bool {
    // Stable integer mix of the original row id. The split is independent of
    // the deterministic stratified sample used to construct EncodedMatrix.
    let mut x = row as u64 + 0x9e3779b97f4a7c15;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^= x >> 31;
    x.is_multiple_of(5)
}

pub fn association_rules(enc: &EncodedMatrix, cfg: &AssociationConfig) -> (Vec<Finding>, String) {
    if cfg.min_support == 0
        || !cfg.min_lift.is_finite()
        || cfg.min_lift <= 0.0
        || !cfg.min_confidence.is_finite()
        || !(0.0..=1.0).contains(&cfg.min_confidence)
        || cfg.max_levels > 254
        || cfg.max_findings > 1000
    {
        return (Vec::new(), "invalid association config: require min_support >= 1, finite positive min_lift, min_confidence in [0,1], max_levels <= 254, max_findings <= 1000".into());
    }
    let feats: Vec<usize> = (0..enc.bins.len())
        .filter(|&f| {
            enc.feat_nominal.get(f) == Some(&true)
                && enc.n_bins.get(f).copied().unwrap_or(usize::MAX) <= cfg.max_levels
                && enc.levels.get(f).map(|x| !x.is_empty()).unwrap_or(false)
        })
        .collect();
    let nominal = enc.feat_nominal.iter().filter(|&&v| v).count();
    let mut masks: Vec<Vec<(Mask, Mask)>> = Vec::new(); // per feature, level: train, holdout
    let mut valid: Vec<(Mask, Mask)> = Vec::new();
    let (mut ntr, mut nho) = (0usize, 0usize);
    for j in 0..enc.n {
        if holdout(*enc.rows.get(j).unwrap_or(&(j as u32))) {
            nho += 1
        } else {
            ntr += 1
        }
    }
    for &f in &feats {
        let mut by = vec![(Mask::new(enc.n), Mask::new(enc.n)); enc.n_bins[f]];
        let (mut tr_valid, mut ho_valid) = (Mask::new(enc.n), Mask::new(enc.n));
        for j in 0..enc.n {
            let c = enc.bins[f][j];
            if c == MISSING || c as usize >= by.len() {
                continue;
            }
            if holdout(*enc.rows.get(j).unwrap_or(&(j as u32))) {
                by[c as usize].1.set(j);
                ho_valid.set(j);
            } else {
                by[c as usize].0.set(j);
                tr_valid.set(j);
            }
        }
        masks.push(by);
        valid.push((tr_valid, ho_valid));
    }
    type RuleKey = (String, String, String, String);
    let mut found: Vec<(f64, RuleKey, Finding)> = Vec::new();
    for ai in 0..feats.len() {
        for bi in ai + 1..feats.len() {
            let (a, b) = (feats[ai], feats[bi]);
            let observed = (
                valid[ai].0.intersection(&valid[bi].0),
                valid[ai].1.intersection(&valid[bi].1),
            );
            let (tr_n, ho_n) = (observed.0.count(), observed.1.count());
            let train_a: Vec<_> = masks[ai]
                .iter()
                .map(|m| m.0.and_count(&observed.0))
                .collect();
            let hold_a: Vec<_> = masks[ai]
                .iter()
                .map(|m| m.1.and_count(&observed.1))
                .collect();
            let train_b: Vec<_> = masks[bi]
                .iter()
                .map(|m| m.0.and_count(&observed.0))
                .collect();
            let hold_b: Vec<_> = masks[bi]
                .iter()
                .map(|m| m.1.and_count(&observed.1))
                .collect();
            for ac in 0..masks[ai].len() {
                for bc in 0..masks[bi].len() {
                    let (jl, jh) = (
                        masks[ai][ac].0.and_count(&masks[bi][bc].0),
                        masks[ai][ac].1.and_count(&masks[bi][bc].1),
                    );
                    if jl < cfg.min_support || jh < cfg.min_support || tr_n == 0 || ho_n == 0 {
                        continue;
                    }
                    for rev in [false, true] {
                        let (lhs, rhs, lcode, rcode, lc, lh, rc, rh) = if rev {
                            (
                                b,
                                a,
                                bc,
                                ac,
                                train_b[bc],
                                hold_b[bc],
                                train_a[ac],
                                hold_a[ac],
                            )
                        } else {
                            (
                                a,
                                b,
                                ac,
                                bc,
                                train_a[ac],
                                hold_a[ac],
                                train_b[bc],
                                hold_b[bc],
                            )
                        };
                        let (tc, tl) = (
                            jl as f64 / lc as f64,
                            jl as f64 * tr_n as f64 / (lc as f64 * rc as f64),
                        );
                        let (hc, hl) = (
                            jh as f64 / lh as f64,
                            jh as f64 * ho_n as f64 / (lh as f64 * rh as f64),
                        );
                        if tc < cfg.min_confidence
                            || hc < cfg.min_confidence
                            || tl < cfg.min_lift
                            || hl < cfg.min_lift
                        {
                            continue;
                        }
                        if rc as f64 / tr_n as f64 >= 0.9 || rh as f64 / ho_n as f64 >= 0.9 {
                            continue;
                        }
                        let lv = enc.levels[lhs][lcode].clone();
                        let rv = enc.levels[rhs][rcode].clone();
                        let mut ev = BTreeMap::new();
                        for (k, v) in [
                            ("train_n", json!(ntr)),
                            ("holdout_n", json!(nho)),
                            ("train_pair_n", json!(tr_n)),
                            ("holdout_pair_n", json!(ho_n)),
                            ("train_joint_count", json!(jl)),
                            ("holdout_joint_count", json!(jh)),
                            ("train_support", json!(jl as f64 / tr_n as f64)),
                            ("holdout_support", json!(jh as f64 / ho_n as f64)),
                            ("train_confidence", json!(tc)),
                            ("holdout_confidence", json!(hc)),
                            ("train_lift", json!(tl)),
                            ("holdout_lift", json!(hl)),
                            ("antecedent", json!(lv)),
                            ("consequent", json!(rv)),
                            ("validation", json!("Diagnostic holdout across many candidate rules; no inferential p-value or multiple-testing guarantee.")),
                        ] {
                            ev.insert(k.into(), v);
                        }
                        let score = hc * hl.min(5.0);
                        let f = Finding::new(
Pattern::ConditionalRule,
BTreeMap::from([
                                ("antecedent_feature".into(), json!(enc.feat_names[lhs])),
                                ("antecedent_value".into(), json!(lv)),
                                ("consequent_feature".into(), json!(enc.feat_names[rhs])),
                                ("consequent_value".into(), json!(rv)),
                            ]),
format!("{} = {} is associated with {} = {} (holdout confidence {:.2}, lift {:.2})", enc.feat_names[lhs], lv, enc.feat_names[rhs], rv, hc, hl),
EvidenceBasis::Estimated,
(hc * 0.5 + ((hl - 1.0) / 4.0).min(1.0) * 0.5).min(1.0),
ev,
None,
Method::new("association_rules", "nominal_pair_counts", "Pairwise nominal association with deterministic row holdout")
).with_rule(
    vec![onceover_report::Predicate::equal(enc.feat_names[lhs].clone(),enc.literal(lhs,&lv))],
    onceover_report::Predicate::equal(enc.feat_names[rhs].clone(),enc.literal(rhs,&rv))
);
                        found.push((
                            score,
                            (
                                enc.feat_names[lhs].clone(),
                                lv.clone(),
                                enc.feat_names[rhs].clone(),
                                rv.clone(),
                            ),
                            f,
                        ));
                    }
                }
            }
        }
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    found.dedup_by(|a, b| a.1 == b.1);
    let count = found.len();
    let findings: Vec<Finding> = found
        .into_iter()
        .take(cfg.max_findings)
        .map(|x| x.2)
        .collect();
    let emitted = findings.len();
    (
        findings,
        format!(
            "{} of {nominal} nominal features (all eligible columns, level cap {}); {ntr} train / {nho} diagnostic holdout rows; {count} rules passing criteria, {emitted} emitted (cap {})",
            feats.len(), cfg.max_levels, cfg.max_findings
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn matrix(n: usize, nominal: Vec<bool>, bins: Vec<Vec<u8>>, names: Vec<&str>) -> EncodedMatrix {
        let levels = bins
            .iter()
            .enumerate()
            .map(|(f, _)| {
                if f == 2 {
                    vec![]
                } else {
                    vec!["no".into(), "yes".into()]
                }
            })
            .collect();
        EncodedMatrix {
            rows: (0..n as u32).collect(),
            n,
            feat_cols: (0..bins.len()).collect(),
            feat_dtypes: vec!["string".into(); bins.len()],
            feat_names: names.into_iter().map(str::to_string).collect(),
            feat_nominal: nominal,
            n_bins: bins
                .iter()
                .map(|x| if x.contains(&1) { 2 } else { 1 })
                .collect(),
            bins,
            edges: vec![Vec::new(); 3],
            levels,
        }
    }
    fn cfg() -> AssociationConfig {
        AssociationConfig {
            min_support: 30,
            min_lift: 1.5,
            min_confidence: 0.75,
            ..Default::default()
        }
    }
    #[test]
    fn finds_planted_directional_rule_and_ignores_numeric_bins() {
        let n = 1500;
        let a = (0..n)
            .map(|i| if i % 2 == 0 { 0 } else { 1 })
            .collect::<Vec<_>>();
        let b = (0..n)
            .map(|i| if i % 2 == 0 { 1 } else { 0 })
            .collect::<Vec<_>>();
        let numeric = vec![0; n];
        let enc = matrix(
            n,
            vec![true, true, false],
            vec![a, b, numeric],
            vec!["a", "b", "numeric"],
        );
        let (f, _) = association_rules(&enc, &cfg());
        assert!(!f.is_empty());
        assert!(f
            .iter()
            .any(|x| x.subject.inputs == ["a"] && x.subject.target.as_deref() == Some("b")));
    }
    #[test]
    fn missing_values_are_excluded_from_pair_denominator_and_order_is_deterministic() {
        let n = 1800;
        let a = (0..n)
            .map(|i| {
                if i % 11 == 0 {
                    MISSING
                } else if i % 2 == 0 {
                    0
                } else {
                    1
                }
            })
            .collect::<Vec<_>>();
        let b = (0..n)
            .map(|i| {
                if i % 13 == 0 {
                    MISSING
                } else if i % 2 == 0 {
                    1
                } else {
                    0
                }
            })
            .collect::<Vec<_>>();
        let enc = matrix(
            n,
            vec![true, true, false],
            vec![a, b, vec![0; n]],
            vec!["a", "b", "numeric"],
        );
        let (x, _) = association_rules(&enc, &cfg());
        let (y, _) = association_rules(&enc, &cfg());
        assert_eq!(x, y);
        assert!(!x.is_empty());
        assert!(
            x[0].metrics()["train_pair_n"].as_u64().unwrap()
                < x[0].metrics()["train_n"].as_u64().unwrap()
        );
    }
    #[test]
    fn no_nominal_features_returns_empty() {
        let enc = matrix(
            100,
            vec![false, false, false],
            vec![vec![0; 100]; 3],
            vec!["x", "y", "z"],
        );
        assert!(association_rules(&enc, &cfg()).0.is_empty());
    }

    #[test]
    fn holdout_reversal_suppresses_training_only_association() {
        let n = 3000;
        let a: Vec<u8> = (0..n).map(|i| (i % 2) as u8).collect();
        let b: Vec<u8> = (0..n)
            .map(|i| if holdout(i as u32) { 1 - a[i] } else { a[i] })
            .collect();
        let enc = matrix(
            n,
            vec![true, true, false],
            vec![a, b, vec![0; n]],
            vec!["a", "b", "numeric"],
        );
        assert!(association_rules(&enc, &cfg()).0.is_empty());
    }

    #[test]
    fn invalid_config_is_rejected_without_findings() {
        let enc = matrix(
            100,
            vec![true, true, false],
            vec![vec![0; 100]; 3],
            vec!["a", "b", "n"],
        );
        let mut bad = cfg();
        bad.min_support = 0;
        let (findings, coverage) = association_rules(&enc, &bad);
        assert!(findings.is_empty());
        assert!(coverage.contains("invalid association config"));

        let mut bad = cfg();
        bad.min_lift = f64::NAN;
        assert!(association_rules(&enc, &bad).0.is_empty());

        let mut bad = cfg();
        bad.min_confidence = f64::NAN;
        assert!(association_rules(&enc, &bad).0.is_empty());
    }
}
