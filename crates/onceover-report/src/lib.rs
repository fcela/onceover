//! onceover-report: the findings ledger, report schema and serialization.
//!
//! The report is the product: a ranked ledger of evidence-backed findings,
//! per-column cards, and an honest coverage block. Determinism (P5) rules:
//! BTreeMaps everywhere, no HashMap iteration order may leak into output.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

mod finding;
mod ledger;
pub use finding::*;
pub use ledger::{
    finalize_ledger, group_findings, select_findings, SelectionPolicy, SelectionSummary,
};

/// Breaking report-schema revision: statements are independent of detectors.
pub const SCHEMA_VERSION: &str = "0.2";

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
    #[serde(default)]
    pub groups: Vec<FindingGroup>,
    #[serde(default)]
    pub selection: SelectionSummary,
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
            let label = format!(
                "{}{}{}",
                f.category.as_str(),
                f.finding_type
                    .as_ref()
                    .map(|t| format!("/{t}"))
                    .unwrap_or_default(),
                f.specification
                    .form
                    .as_ref()
                    .map(|form| format!(" ({form})"))
                    .unwrap_or_default()
            );
            md.push_str(&format!(
                "- **[{}·{}] {}** — {}\n",
                f.interestingness,
                format!("{:?}", f.primary_evidence().basis).to_lowercase(),
                label,
                f.claim
            ));
            if f.specification.form.as_deref() == Some("piecewise_linear") {
                let target = f.subject.target.as_deref().unwrap_or("target");
                if let Some(leaves) = f.metrics().get("leaves").and_then(|v| v.as_array()) {
                    for (idx, leaf) in leaves.iter().enumerate() {
                        let Some(rendered) = render_piecewise_leaf(leaf, target) else {
                            continue;
                        };
                        md.push_str(&format!("  - Leaf {}: {}\n", idx + 1, rendered));
                    }
                }
            }
            for e in &f.evidence {
                md.push_str(&render_evidence(f, e));
            }
        }
        md.push('\n');

        if !self.groups.is_empty() {
            md.push_str("## Related finding groups\n\n");
            for group in &self.groups {
                md.push_str(&format!(
                    "- {}: {}. {}\n",
                    group.id,
                    group.columns.join(", "),
                    group.description
                ));
            }
            md.push('\n');
        }
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

/// One evidence record rendered under its finding's claim: which
/// detector produced it, how it was validated, the rows it covers, its
/// scalar metrics, and a result table for each metric that holds rows
/// of values (per-period coefficients, group summaries, ...).
fn render_evidence(f: &Finding, e: &Evidence) -> String {
    let mut out = String::new();
    let mut head = format!(
        "  - evidence: {} ({}), {}",
        e.method.detector, e.method.algorithm, e.validation.method
    );
    if let Some(n) = e.scope.rows_evaluated {
        head.push_str(&format!(", {n} rows evaluated"));
    }
    match e.scope.sampling {
        Sampling::Sample => match e.scope.population_rows {
            Some(p) => head.push_str(&format!(", sampled from {p} rows")),
            None => head.push_str(", sampled"),
        },
        Sampling::Full => head.push_str(", full data"),
        Sampling::Unspecified => {}
    }
    out.push_str(&head);
    out.push('\n');
    let mut scalars: Vec<String> = Vec::new();
    let mut tables: Vec<(&String, &Value)> = Vec::new();
    for (k, v) in &e.metrics {
        // Piecewise leaves already render as equations above the
        // evidence block; do not render them a second time as a table.
        if k == "leaves" && f.specification.form.as_deref() == Some("piecewise_linear") {
            continue;
        }
        if is_flat_row_table(v) {
            tables.push((k, v));
        } else if v.is_object() || v.is_array() {
            scalars.push(format!("{k}={}", compact_json(v)));
        } else {
            scalars.push(format!("{k}={}", render_scalar(v)));
        }
    }
    const MAX_SCALARS: usize = 12;
    if !scalars.is_empty() {
        let shown = scalars.len().min(MAX_SCALARS);
        let mut line = format!("    - metrics: {}", scalars[..shown].join(", "));
        if scalars.len() > shown {
            line.push_str(&format!(" (+{} more in JSON)", scalars.len() - shown));
        }
        out.push_str(&line);
        out.push('\n');
    }
    for (k, v) in tables {
        out.push_str(&format!("    - {k}:\n"));
        out.push_str(&render_metric_table(v));
    }
    out
}

fn is_flat_row_table(v: &Value) -> bool {
    let Some(rows) = v.as_array() else {
        return false;
    };
    !rows.is_empty()
        && rows.iter().all(|r| {
            r.as_object()
                .is_some_and(|o| o.values().all(|c| !c.is_object() && !c.is_array()))
        })
}

fn render_metric_table(v: &Value) -> String {
    let rows = v.as_array().expect("checked by is_flat_row_table");
    let mut columns: Vec<&str> = Vec::new();
    for r in rows {
        for k in r.as_object().expect("checked").keys() {
            if !columns.contains(&k.as_str()) {
                columns.push(k.as_str());
            }
        }
    }
    columns.sort_unstable();
    let mut out = String::new();
    out.push_str(&format!(
        "      | {} |\n      |{}|\n",
        columns.join(" | "),
        columns.iter().map(|_| "---").collect::<Vec<_>>().join("|")
    ));
    const MAX_ROWS: usize = 20;
    for r in rows.iter().take(MAX_ROWS) {
        let obj = r.as_object().expect("checked");
        let cells: Vec<String> = columns
            .iter()
            .map(|c| {
                obj.get(*c)
                    .map(|cell| render_scalar(cell).replace('|', "\\|"))
                    .unwrap_or_default()
            })
            .collect();
        out.push_str(&format!("      | {} |\n", cells.join(" | ")));
    }
    if rows.len() > MAX_ROWS {
        out.push_str(&format!(
            "      ({} more rows in JSON)\n",
            rows.len() - MAX_ROWS
        ));
    }
    out
}

fn render_scalar(v: &Value) -> String {
    match v {
        Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                u.to_string()
            } else if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(x) = n.as_f64() {
                format_equation_number(x)
            } else {
                n.to_string()
            }
        }
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => compact_json(other),
    }
}

fn compact_json(v: &Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_default();
    if s.chars().count() > 120 {
        let cut: String = s.chars().take(117).collect();
        format!("{cut}...")
    } else {
        s
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

#[cfg(test)]
mod tests;
