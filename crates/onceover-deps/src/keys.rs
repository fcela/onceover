//! Key analysis: primary-key candidates, composite keys, dirty keys.
//! Candidates are ranked by completeness, type affinity, and name patterns.

use onceover_report::{Method, Pattern};
use std::collections::BTreeMap;

use onceover_report::{EvidenceBasis, Finding};
use serde_json::json;

use crate::fd::{key_score, DepsOutcome};
use crate::ColInfo;

#[derive(Default, Debug)]
pub struct KeyOutcome {
    pub findings: Vec<Finding>,
    /// Name of the top primary-key candidate (for downstream suppression
    /// of its identifier_like finding).
    pub pk: Option<String>,
}

pub fn analyze_keys(info: &[ColInfo], deps: &DepsOutcome) -> KeyOutcome {
    let mut out = KeyOutcome::default();

    // ---- primary key ----
    if let Some(&best) = deps.single_unique.first() {
        let score = key_score(&info[best]);
        out.pk = Some(info[best].name.clone());
        out.findings.push(Finding::new(
            Pattern::Uniqueness,
            BTreeMap::from([("column".into(), json!(info[best].name))]),
            format!(
                "{} uniquely identifies rows (primary key candidate)",
                info[best].name
            ),
            EvidenceBasis::Measured,
            // Rank by the plausibility the metrics already record: an
            // integer `*_id` column keeps the top score, while an
            // all-distinct float measurement (plausibility ≈ 0.48 from
            // type affinity alone) lands near 0.61 instead of tying
            // with real identifiers at a flat 0.95.
            0.3 + 0.65 * score,
            BTreeMap::from([
                ("distinct".into(), json!(info[best].distinct)),
                ("n".into(), json!(info[best].n)),
                ("plausibility_score".into(), json!(score)),
            ]),
            Some(if score < 0.6 {
                "all values are distinct, but the column's type and name do not look like an identifier; treat this as a distinct-count check, not a join key".into()
            } else {
                "use as join key; exclude from numeric features".into()
            }),
            Method::new(
                "keys",
                "uniqueness",
                "exact uniqueness + folklore ranking (onceover v0.1)",
            ),
        ));
    }

    // ---- composite key ----
    if let Some(best) = deps.composite_uccs.iter().min_by(|a, b| {
        let sa: f64 = a.iter().map(|&i| key_score(&info[i])).sum();
        let sb: f64 = b.iter().map(|&i| key_score(&info[i])).sum();
        sa.total_cmp(&sb).then(a.cmp(b))
    }) {
        let names: Vec<&str> = best.iter().map(|&i| info[i].name.as_str()).collect();
        let mean_plausibility =
            best.iter().map(|&i| key_score(&info[i])).sum::<f64>() / best.len() as f64;
        out.findings.push(Finding::new(
            Pattern::Uniqueness,
            BTreeMap::from([("columns".into(), json!(names))]),
            format!(
                "({}) jointly identify rows (composite key)",
                names.join(", ")
            ),
            EvidenceBasis::Measured,
            // Same plausibility scaling as the primary-key candidate:
            // an all-identifier composite keeps the historical 0.88.
            0.3 + 0.58 * mean_plausibility,
            BTreeMap::from([
                ("n_columns".into(), json!(best.len())),
                ("n".into(), json!(info[best[0]].n)),
            ]),
            Some("use the pair as join key".into()),
            Method::new(
                "keys",
                "composite_uniqueness",
                "exact composite uniqueness (onceover v0.1)",
            ),
        ));
    }

    // ---- dirty keys: near-unique columns with a few duplicates ----
    for c in info {
        let dup = c.dup_count();
        let n = c.n_nonnull();
        if dup == 0 || n < 100 {
            continue;
        }
        let threshold = (5.0_f64).max(0.000_5 * c.n as f64) as usize;
        let col_name = &c.name;
        if dup <= threshold && c.distinct_ratio() >= 0.995 {
            let mut evidence =
                BTreeMap::from([("duplicates".into(), json!(dup)), ("n".into(), json!(n))]);
            if !c.dup_examples.is_empty() {
                evidence.insert("examples".into(), json!(c.dup_examples));
            }
            out.findings.push(Finding::new(
                Pattern::Uniqueness,
                BTreeMap::from([("column".into(), json!(c.name))]),
                format!(
                    "column {} is nearly unique with {} repeated non-null value{}",
                    c.name,
                    dup,
                    if dup == 1 { "" } else { "s" }
                ),
                EvidenceBasis::Measured,
                0.70,
                evidence,
                Some(format!("if {col_name} is expected to be unique, inspect the repeated values before using it as a key")),
                Method::new(
                    "keys",
                    "near_uniqueness",
                    "exact distinct-count analysis (onceover v0.1)",
                ),
            ));
        }
    }

    out
}
