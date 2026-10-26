//! Consolidate equivalent statements, rank their evidence, and select a report.
use crate::{Category, Evidence, EvidenceBasis, Finding, FindingGroup, Sampling};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SelectionSummary {
    pub candidates: usize,
    pub verified_candidates: usize,
    pub distinct_statements: usize,
    pub selected: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelectionPolicy {
    pub max_findings: usize,
    pub min_rule_support: usize,
    pub min_rule_antecedent: usize,
    pub min_rule_gain: f64,
    pub min_rule_lift: f64,
    pub max_rules: usize,
    pub rules_per_outcome: usize,
    pub predictive_findings_per_target: usize,
}
impl Default for SelectionPolicy {
    fn default() -> Self {
        Self {
            max_findings: 100,
            min_rule_support: 20,
            min_rule_antecedent: 30,
            min_rule_gain: 0.02,
            min_rule_lift: 1.5,
            max_rules: 20,
            rules_per_outcome: 2,
            predictive_findings_per_target: 3,
        }
    }
}
fn metric(e: &Evidence, key: &str) -> Option<f64> {
    e.metrics.get(key).and_then(Value::as_f64)
}
fn quality(e: &Evidence) -> f64 {
    match e.validation.method.as_str() {
        "direct_comparison" => {
            if e.scope.sampling == Sampling::Full {
                1.0
            } else {
                0.85
            }
        }
        "cross_validation" | "diagnostic_holdout" | "diagnostic_partition" => 0.8,
        _ => match e.basis {
            EvidenceBasis::Measured => 0.85,
            EvidenceBasis::Estimated => 0.6,
            EvidenceBasis::Inferred => 0.4,
        },
    }
}
fn score(f: &Finding) -> f64 {
    let mut best: f64 = 0.0;
    for e in &f.evidence {
        if f.category == Category::ConditionalRules && !e.metrics.contains_key("conditional_rate") {
            continue;
        }
        let strength = match f.category {
            Category::ConditionalRules => {
                let p = metric(e, "conditional_rate")
                    .or_else(|| metric(e, "holdout_confidence"))
                    .or_else(|| metric(e, "precision"))
                    .unwrap_or(0.0);
                let base = metric(e, "baseline_rate").unwrap_or(0.5);
                let gain = ((p - base) / (1.0 - base).max(0.01)).clamp(0.0, 1.0);
                let support = metric(e, "joint_count")
                    .or_else(|| metric(e, "holdout_joint_count"))
                    .or_else(|| metric(e, "support"))
                    .unwrap_or(0.0);
                (0.5 * gain
                    + 0.5 * ((metric(e, "lift").unwrap_or(1.0) - 1.0) / 4.0).clamp(0.0, 1.0))
                    * (support / 30.0).min(1.0)
            }
            Category::Dependencies if f.finding_type.as_deref() == Some("predictive") => {
                let r2 = metric(e, "holdout_r2").or_else(|| {
                    (e.metrics.get("task").and_then(Value::as_str) == Some("regression"))
                        .then(|| metric(e, "metric"))
                        .flatten()
                });
                r2.unwrap_or_else(|| {
                    let p = metric(e, "cv_accuracy")
                        .or_else(|| metric(e, "metric"))
                        .unwrap_or(0.0);
                    let base = metric(e, "baseline").unwrap_or(0.0);
                    (p - base) / (1.0 - base).max(0.01)
                })
                .clamp(0.0, 1.0)
            }
            Category::Dependencies => {
                let n = e.scope.rows_evaluated.unwrap_or(1).max(1) as f64;
                1.0 - metric(e, "violations").unwrap_or(0.0) / n
            }
            Category::Uniqueness => 1.0,
            Category::Associations => ["pearson", "spearman", "xi_ab", "xi_ba"]
                .into_iter()
                .filter_map(|k| metric(e, k))
                .map(f64::abs)
                .fold(0.0, f64::max),
            Category::Concentration => metric(e, "top_share").unwrap_or(0.7),
            _ => 0.75,
        };
        let specificity = if f.specification.form.is_some() {
            1.0
        } else {
            0.5
        };
        let simplicity =
            1.0 / (1.0 + (f.subject.inputs.len() + f.specification.conditions.len()) as f64 / 8.0);
        best = best.max(
            0.45 * quality(e)
                + 0.35 * strength.clamp(0.0, 1.0)
                + 0.1 * specificity
                + 0.1 * simplicity,
        );
    }
    (best * 10_000.0).round() / 10_000.0
}

pub fn finalize_ledger(findings: Vec<Finding>) -> Vec<Finding> {
    select_findings(findings, &SelectionPolicy::default())
}

pub fn select_findings(findings: Vec<Finding>, policy: &SelectionPolicy) -> Vec<Finding> {
    let mut statements = BTreeMap::<String, Finding>::new();
    for mut finding in findings {
        finding.subject.canonicalize();
        if finding.validate().is_err() {
            continue;
        }
        let key = finding.canonical_key();
        if let Some(existing) = statements.get_mut(&key) {
            // Keep a statement description from the best evaluated candidate;
            // the metrics remain separate and are never averaged or pooled.
            let existing_rows = existing
                .evidence
                .iter()
                .filter_map(|e| e.scope.rows_evaluated)
                .max()
                .unwrap_or(0);
            let incoming_rows = finding
                .evidence
                .iter()
                .filter_map(|e| e.scope.rows_evaluated)
                .max()
                .unwrap_or(0);
            if incoming_rows > existing_rows {
                existing.claim = finding.claim;
            }
            existing.evidence.extend(finding.evidence);
            if existing.interpretation.is_none() {
                existing.interpretation = finding.interpretation;
            }
        } else {
            statements.insert(key, finding);
        }
    }
    let mut ranked: Vec<_> = statements.into_values().collect();
    for f in &mut ranked {
        f.evidence
            .sort_by_key(|e| serde_json::to_string(e).unwrap());
        f.evidence.dedup();
        f.interestingness = score(f);
    }
    ranked.sort_by(|a, b| {
        b.interestingness
            .total_cmp(&a.interestingness)
            .then(a.canonical_key().cmp(&b.canonical_key()))
    });
    let mut selected = Vec::new();
    let mut rules = 0;
    let mut outcomes = BTreeMap::<String, usize>::new();
    let mut targets = BTreeMap::<String, usize>::new();
    for mut f in ranked {
        if selected.len() >= policy.max_findings {
            break;
        }
        if f.category == Category::ConditionalRules {
            if rules >= policy.max_rules {
                continue;
            }
            let outcome = serde_json::to_string(&f.specification.outcome).unwrap();
            let count = outcomes.entry(outcome).or_default();
            if *count >= policy.rules_per_outcome {
                continue;
            }
            *count += 1;
            rules += 1;
        }
        if f.is(Category::Dependencies, Some("predictive")) {
            let target = f.subject.target.clone().unwrap_or_default();
            let count = targets.entry(target).or_default();
            if *count >= policy.predictive_findings_per_target {
                continue;
            }
            *count += 1;
        }
        // IDs depend on the statement, never its rank or detector enumeration.
        let mut hash = 0xcbf29ce484222325u64;
        for byte in f.canonical_key().bytes() {
            hash = (hash ^ byte as u64).wrapping_mul(0x100000001b3);
        }
        f.id = format!("{}-{hash:016x}", f.category.as_str());
        selected.push(f);
    }
    selected
}

/// A connected view over selected findings; this does not assert redundancy.
pub fn group_findings(findings: &[Finding]) -> Vec<FindingGroup> {
    let mut adjacency = BTreeMap::<String, BTreeSet<String>>::new();
    for f in findings.iter().filter(|f| {
        matches!(
            f.category,
            Category::Dependencies | Category::ConditionalRules | Category::Associations
        )
    }) {
        for a in &f.subject.columns {
            for b in &f.subject.columns {
                if a != b {
                    adjacency.entry(a.clone()).or_default().insert(b.clone());
                }
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut groups = Vec::new();
    for start in adjacency.keys() {
        if seen.contains(start) {
            continue;
        }
        let mut stack = vec![start.clone()];
        let mut columns = BTreeSet::new();
        while let Some(c) = stack.pop() {
            if !seen.insert(c.clone()) {
                continue;
            }
            columns.insert(c.clone());
            if let Some(next) = adjacency.get(&c) {
                stack.extend(next.iter().cloned());
            }
        }
        if columns.len() < 2 {
            continue;
        }
        let ids = findings
            .iter()
            .filter(|f| {
                f.subject.columns.len() > 1 && f.subject.columns.iter().all(|c| columns.contains(c))
            })
            .map(|f| f.id.clone())
            .collect();
        groups.push(FindingGroup{id:format!("group-{}",groups.len()+1),columns:columns.into_iter().collect(),finding_ids:ids,
            description:"Connected findings; inspect individual statements and directions before inferring redundancy.".into()});
    }
    groups
}
