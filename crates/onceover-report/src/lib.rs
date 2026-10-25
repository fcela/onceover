//! onceover-report: the findings ledger, report schema and serialization.
//!
//! The report is the product: a ranked ledger of evidence-backed findings,
//! per-column cards, and an honest coverage block. Determinism (P5) rules:
//! BTreeMaps everywhere, no HashMap iteration order may leak into output.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Schema version of the report contract. Additive-only evolution.
pub const SCHEMA_VERSION: &str = "0.1";

/// How a finding was computed. `Exact` claims are verified on full data;
/// `Sampled` claims ran on a deterministic subsample (n reported in evidence);
/// `Heuristic` claims are rule-of-thumb inferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Exact,
    Sampled,
    Heuristic,
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Confidence::Exact => "exact",
            Confidence::Sampled => "sampled",
            Confidence::Heuristic => "heuristic",
        }
    }
}

/// A single evidence-backed claim about the data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable id within the report, e.g. "functional_dependency-003".
    pub id: String,
    /// Kind taxonomy, e.g. "likely_primary_key", "threshold_relationship".
    pub kind: String,
    /// What the finding is about (columns, lhs/rhs, x/y, ...). Values are JSON.
    pub subject: BTreeMap<String, serde_json::Value>,
    /// One precise sentence an agent can quote verbatim.
    pub claim: String,
    /// How the claim was computed.
    pub confidence: Confidence,
    /// Deterministic interestingness in [0, 1]; drives ledger ranking.
    pub interestingness: f64,
    /// Machine-checkable evidence: support, effect size, counts, method.
    pub evidence: BTreeMap<String, serde_json::Value>,
    /// Suggested modeling/data-handling implication, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_hint: Option<String>,
    /// Provenance: algorithm + origin (paper/lineage).
    pub provenance: String,
}

impl Finding {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: &str,
        subject: BTreeMap<String, serde_json::Value>,
        claim: String,
        confidence: Confidence,
        interestingness: f64,
        evidence: BTreeMap<String, serde_json::Value>,
        action_hint: Option<String>,
        provenance: &str,
    ) -> Self {
        Finding {
            id: String::new(), // assigned by the ledger after ranking
            kind: kind.to_string(),
            subject,
            claim,
            confidence,
            interestingness: (interestingness * 100.0).round() / 100.0,
            evidence,
            action_hint,
            provenance: provenance.to_string(),
        }
    }
}

/// Statistics for numeric columns.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NumericStats {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub mean: Option<f64>,
    pub std: Option<f64>,
    pub q25: Option<f64>,
    pub q50: Option<f64>,
    pub q75: Option<f64>,
    pub zeros: usize,
    pub negatives: usize,
    /// All non-null values are integral (and within exact f64 range).
    pub integral: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StringStats {
    pub min_len: usize,
    pub max_len: usize,
    pub mean_len: f64,
    pub n_empty: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TemporalStats {
    /// ISO "YYYY-MM-DD".
    pub min: Option<String>,
    pub max: Option<String>,
    pub span_days: Option<i64>,
    pub modal_interval_days: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BoolStats {
    pub trues: usize,
    pub falses: usize,
}

/// Concentration of a column's value distribution.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Concentration {
    /// Herfindahl-Hirschman index: sum of squared shares, in [1/k, 1].
    pub hhi: f64,
    /// Shannon entropy in nats (nulls excluded).
    pub entropy_nats: f64,
    /// exp(entropy): "this column behaves like N distinct values".
    pub effective_cardinality: f64,
    pub top1_share: Option<f64>,
    pub top3_share: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ValueCount {
    pub value: String,
    pub count: usize,
    pub share: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PatternCount {
    pub pattern: String,
    pub count: usize,
    pub share: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
}

/// The per-column card: "what is this column, really?"
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ColumnCard {
    pub name: String,
    pub dtype: String,
    /// all_null | constant | boolean | categorical | continuous | identifier |
    /// free_text | date | datetime | other
    pub role: String,
    pub n: usize,
    pub null_count: usize,
    pub null_fraction: f64,
    /// Exact distinct count over non-null values (0 for all-null).
    pub distinct: usize,
    /// distinct / n over non-null values.
    pub distinct_ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numeric_stats: Option<NumericStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub string_stats: Option<StringStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporal_stats: Option<TemporalStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bool_stats: Option<BoolStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concentration: Option<Concentration>,
    /// Top values by frequency (low/moderate cardinality columns).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub top_values: Vec<ValueCount>,
    /// Format signature clusters (string columns).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub format_patterns: Vec<PatternCount>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub action_hints: Vec<String>,
}

/// Status of one profiling pass, for the epistemics block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PassInfo {
    /// ran_exact | ran_sampled | capped | skipped | not_applicable
    pub status: String,
    pub detail: String,
}

impl PassInfo {
    pub fn ran(detail: impl Into<String>) -> Self {
        PassInfo {
            status: "ran_exact".into(),
            detail: detail.into(),
        }
    }
    pub fn sampled(detail: impl Into<String>) -> Self {
        PassInfo {
            status: "ran_sampled".into(),
            detail: detail.into(),
        }
    }
    pub fn capped(detail: impl Into<String>) -> Self {
        PassInfo {
            status: "capped".into(),
            detail: detail.into(),
        }
    }
    pub fn skipped(detail: impl Into<String>) -> Self {
        PassInfo {
            status: "skipped".into(),
            detail: detail.into(),
        }
    }
    pub fn na(detail: impl Into<String>) -> Self {
        PassInfo {
            status: "not_applicable".into(),
            detail: detail.into(),
        }
    }
}

/// The one-glance summary an agent reads first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Overview {
    pub n_rows: usize,
    pub n_columns: usize,
    pub headline: String,
    pub dtype_counts: BTreeMap<String, usize>,
    pub role_counts: BTreeMap<String, usize>,
    /// Claims of the top findings (already ranked).
    pub top_findings: Vec<String>,
}

/// The complete profile report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileReport {
    pub schema_version: String,
    pub overview: Overview,
    pub columns: BTreeMap<String, ColumnCard>,
    /// The ranked findings ledger. Order is the product.
    pub findings: Vec<Finding>,
    /// What was tested, at what scale, what was skipped and why.
    pub coverage: BTreeMap<String, PassInfo>,
}

impl ProfileReport {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("report serialization cannot fail")
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).expect("report serialization cannot fail")
    }

    /// Compact, deterministic Markdown rendering (agent/human pasteable).
    pub fn to_markdown(&self) -> String {
        let mut md = String::with_capacity(4096);
        md.push_str("# onceover report\n\n");
        md.push_str(&format!("{}\n\n", self.overview.headline));
        md.push_str("## Overview\n\n");
        md.push_str(&format!(
            "- **rows × columns**: {} × {}\n",
            self.overview.n_rows, self.overview.n_columns
        ));
        let dtypes: Vec<String> = self
            .overview
            .dtype_counts
            .iter()
            .map(|(k, v)| format!("{}×{}", v, k))
            .collect();
        md.push_str(&format!("- **dtypes**: {}\n", dtypes.join(", ")));
        let roles: Vec<String> = self
            .overview
            .role_counts
            .iter()
            .map(|(k, v)| format!("{}×{}", v, k))
            .collect();
        md.push_str(&format!("- **roles**: {}\n", roles.join(", ")));
        md.push('\n');

        md.push_str("## Findings\n\n");
        if self.findings.is_empty() {
            md.push_str("_No notable findings at this budget.\n\n");
        }
        for f in &self.findings {
            md.push_str(&format!(
                "- **[{}·{}] {}** — {}\n",
                f.interestingness,
                f.confidence.as_str(),
                f.kind,
                f.claim
            ));
            if f.kind == "piecewise_relationship" {
                let target = f
                    .subject
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or("target");
                if let Some(leaves) = f.evidence.get("leaves").and_then(|v| v.as_array()) {
                    for (idx, leaf) in leaves.iter().enumerate() {
                        let Some(rendered) = render_piecewise_leaf(leaf, target) else {
                            continue;
                        };
                        md.push_str(&format!("  - Leaf {}: {}\n", idx + 1, rendered));
                    }
                }
            }
        }
        md.push('\n');

        md.push_str("## Columns\n\n");
        md.push_str(
            "| column | role | nulls | distinct | concentration (top1 / eff. cardinality) |\n",
        );
        md.push_str("|---|---|---|---|---|\n");
        for (name, c) in &self.columns {
            let conc = c
                .concentration
                .as_ref()
                .map(|k| {
                    format!(
                        "{:.3} / {:.2}",
                        k.top1_share.unwrap_or(0.0),
                        k.effective_cardinality
                    )
                })
                .unwrap_or_else(|| "—".into());
            md.push_str(&format!(
                "| {} | {} | {} ({:.1}%) | {} | {} |\n",
                name,
                c.role,
                c.null_count,
                100.0 * c.null_fraction,
                c.distinct,
                conc
            ));
        }
        md.push('\n');

        md.push_str("## Coverage\n\n");
        md.push_str("| pass | status | detail |\n|---|---|---|\n");
        for (pass, info) in &self.coverage {
            md.push_str(&format!(
                "| {} | {} | {} |\n",
                pass, info.status, info.detail
            ));
        }
        md
    }
}

fn format_equation_number(x: f64) -> String {
    if !x.is_finite() {
        return x.to_string();
    }
    if x != 0.0 && (x.abs() < 0.0001 || x.abs() >= 1_000_000.0) {
        format!("{x:.3e}")
    } else {
        let s = format!("{x:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn render_piecewise_leaf(leaf: &serde_json::Value, target: &str) -> Option<String> {
    let conditions = leaf.get("conditions")?.as_array()?;
    let intercept = leaf.get("intercept")?.as_f64()?;
    let coefficients = leaf.get("coefficients")?.as_object()?;
    let n_train = leaf.get("n_train")?.as_u64()?;
    let n_holdout = leaf.get("n_holdout")?.as_u64()?;
    let mut conds = Vec::new();
    for c in conditions {
        let column = c.get("column")?.as_str()?;
        let operator = c.get("operator")?.as_str()?;
        let threshold = c.get("threshold")?.as_f64()?;
        conds.push(format!(
            "{} {} {}",
            column,
            operator,
            format_equation_number(threshold)
        ));
    }
    let mut terms = vec![format_equation_number(intercept)];
    for (column, value) in coefficients {
        let coef = value.as_f64()?;
        if coef == 0.0 {
            continue;
        }
        let sign = if coef < 0.0 { " - " } else { " + " };
        terms.push(format!(
            "{}{}·{}",
            sign,
            format_equation_number(coef.abs()),
            column
        ));
    }
    let equation = terms.join("");
    let condition_text = if conds.is_empty() {
        "otherwise".to_string()
    } else {
        conds.join(" and ")
    };
    Some(format!(
        "if {}; {} ≈ {}; train support {}, holdout support {}",
        condition_text, target, equation, n_train, n_holdout
    ))
}

/// Assign stable ids and deterministic ranking to a findings vector.
/// Sorts by (interestingness desc, kind asc, subject asc) and renumbers
/// ids as `{kind}-{nnn}` within each kind.
pub fn finalize_ledger(mut findings: Vec<Finding>) -> Vec<Finding> {
    findings.sort_by(|a, b| {
        b.interestingness
            .total_cmp(&a.interestingness)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| {
                serde_json::to_string(&a.subject)
                    .unwrap_or_default()
                    .cmp(&serde_json::to_string(&b.subject).unwrap_or_default())
            })
    });
    let mut counters: BTreeMap<String, usize> = BTreeMap::new();
    for f in findings.iter_mut() {
        let c = counters.entry(f.kind.clone()).or_insert(0);
        *c += 1;
        f.id = format!("{}-{:03}", f.kind, *c);
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_finding(kind: &str, interest: f64, subject_key: &str) -> Finding {
        let mut subject = BTreeMap::new();
        subject.insert(subject_key.to_string(), serde_json::json!("x"));
        Finding::new(
            kind,
            subject,
            format!("claim {}", subject_key),
            Confidence::Exact,
            interest,
            BTreeMap::new(),
            None,
            "test",
        )
    }

    #[test]
    fn ledger_orders_deterministically() {
        let f1 = test_finding("a_kind", 0.5, "z");
        let f2 = test_finding("b_kind", 0.9, "x");
        let f3 = test_finding("a_kind", 0.5, "y");
        let out = finalize_ledger(vec![f1, f2, f3]);
        assert_eq!(out[0].kind, "b_kind");
        assert_eq!(out[0].id, "b_kind-001");
        assert_eq!(out[1].subject["y"], serde_json::json!("x"));
        assert_eq!(out[1].id, "a_kind-001");
        assert_eq!(out[2].id, "a_kind-002");
    }

    #[test]
    fn json_roundtrip() {
        let mut ev = BTreeMap::new();
        ev.insert("violations".to_string(), serde_json::json!(0));
        let f = Finding::new(
            "functional_dependency",
            BTreeMap::new(),
            "a → b".into(),
            Confidence::Exact,
            0.85,
            ev,
            Some("join key".into()),
            "tane",
        );
        let ledger = finalize_ledger(vec![f]);
        let report = ProfileReport {
            schema_version: SCHEMA_VERSION.into(),
            overview: Overview {
                n_rows: 10,
                n_columns: 2,
                headline: "test".into(),
                dtype_counts: BTreeMap::new(),
                role_counts: BTreeMap::new(),
                top_findings: vec![],
            },
            columns: BTreeMap::new(),
            findings: ledger,
            coverage: BTreeMap::new(),
        };
        let js = report.to_json();
        let back: ProfileReport = serde_json::from_str(&js).unwrap();
        assert_eq!(back, report);
        assert!(report.to_markdown().contains("## Findings"));
    }

    #[test]
    fn markdown_renders_piecewise_leaf_equations_and_skips_malformed() {
        let mut evidence = BTreeMap::new();
        evidence.insert("leaves".into(), serde_json::json!([
            {"conditions":[{"column":"age","operator":"<=","threshold":42.5}],"intercept":1.0,"coefficients":{"income":0.00000012,"score":-2.0},"n_train":80,"n_holdout":20},
            {"conditions":[{"column":"age","operator":">","threshold":42.5}],"intercept":3.0,"coefficients":{"income":4.0},"n_train":70,"n_holdout":18},
            {"conditions":"bad","intercept":"bad"}
        ]));
        let finding = Finding::new(
            "piecewise_relationship",
            BTreeMap::from([("target".into(), serde_json::json!("y"))]),
            "piecewise fit".into(),
            Confidence::Sampled,
            0.8,
            evidence,
            None,
            "test",
        );
        let report = ProfileReport {
            schema_version: SCHEMA_VERSION.into(),
            overview: Overview {
                n_rows: 100,
                n_columns: 3,
                headline: "test".into(),
                dtype_counts: BTreeMap::new(),
                role_counts: BTreeMap::new(),
                top_findings: vec![],
            },
            columns: BTreeMap::new(),
            findings: vec![finding],
            coverage: BTreeMap::new(),
        };
        let md = report.to_markdown();
        assert!(md.contains("age <= 42.5"));
        assert!(md.contains("y ≈ 1 + 1.200e-7·income - 2·score"));
        assert!(md.contains("train support 80, holdout support 20"));
        assert!(md.contains("age > 42.5"));
        assert!(md.contains("3 + 4·income"));
        assert_eq!(md.matches("Leaf ").count(), 2);
    }
}
