//! onceover-report: the findings ledger, report schema and serialization.
//!
//! The report is the product: a ranked ledger of evidence-backed findings,
//! per-column cards, and an honest coverage block. Determinism (P5) rules:
//! BTreeMaps everywhere, no HashMap iteration order may leak into output.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

mod finding;
mod hints;
mod ledger;
pub use finding::*;
pub use hints::{resolve_labels, substitute_labels, Cadence, ProfileHints, ResolvedLabels};
pub use ledger::{
    finalize_ledger, group_findings, report_band, select_findings, select_findings_counted,
    SelectionOutcome, SelectionPolicy, SelectionSummary,
};

/// Breaking report-schema revision: statements are independent of detectors.
/// 0.7 adds the associations table: the report gains a top-level
/// `associations` list carrying every pair the association screens
/// tested, with effect sizes, raw p-values, Benjamini–Hochberg
/// q-values, and reporting tiers; additive over 0.6.
/// 0.6 adds verified relationship families: the report gains a
/// top-level `grouped_findings` list (statements the ledger grouped
/// under a family lead; each carries `grouped_under`, and the lead
/// carries `grouped_members`), and `selection` gains the `families`
/// and `grouped_statements` counts. Grouped statements no longer
/// appear in `findings`; every other field is additive over 0.5.
/// 0.5 adds the frame account: axis span fields on `frame.axis`, the
/// `frame.grain` record (the frame's key, factorized when the frame is
/// a balanced cross of its dimensions and axis) and `frame.dominant`
/// (the grouping that organises the largest share of variance);
/// additive over 0.4. 0.4 adds the `frame` section (semantic column
/// roles and the assembled time axis) and the per-card
/// `semantic_role` fields; additive over 0.3.
pub const SCHEMA_VERSION: &str = "0.7";

/// Round to four decimal places — the report's display precision
/// for shares and means carried in evidence payloads.
pub fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
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
    /// The frame model's semantic role: identifier | index | timestamp |
    /// timestamp_part | dimension | measure | text. Assigned before the
    /// detectors run, with the evidence in `semantic_role_evidence`;
    /// absent when no assignment was made (opaque, empty, or constant
    /// columns). The dtype/cardinality `role` above is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_role_evidence: Option<String>,
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
    /// Wall time attributed to this pass, in milliseconds: the
    /// interval between the previous coverage entry being recorded
    /// and this one (the pipeline runs passes sequentially and
    /// records each pass's entry when it finishes). The first
    /// recorded entry also carries table extraction and the column
    /// cards. Recorded only on timing-enabled runs (the pipeline's
    /// opt-in `timing` config); absent otherwise, and on deadline
    /// skips recorded by the deadline check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<f64>,
}

impl PassInfo {
    pub fn ran(detail: impl Into<String>) -> Self {
        PassInfo {
            status: "ran_exact".into(),
            detail: detail.into(),
            elapsed_ms: None,
        }
    }
    pub fn sampled(detail: impl Into<String>) -> Self {
        PassInfo {
            status: "ran_sampled".into(),
            detail: detail.into(),
            elapsed_ms: None,
        }
    }
    pub fn capped(detail: impl Into<String>) -> Self {
        PassInfo {
            status: "capped".into(),
            detail: detail.into(),
            elapsed_ms: None,
        }
    }
    pub fn skipped(detail: impl Into<String>) -> Self {
        PassInfo {
            status: "skipped".into(),
            detail: detail.into(),
            elapsed_ms: None,
        }
    }
    pub fn na(detail: impl Into<String>) -> Self {
        PassInfo {
            status: "not_applicable".into(),
            detail: detail.into(),
            elapsed_ms: None,
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

/// One column's semantic role assignment in the frame model, with
/// the evidence the assignment rests on. `role` is null when no
/// assignment was made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FrameColumnRole {
    pub column: String,
    pub role: Option<String>,
    pub evidence: String,
}

/// The frame's time axis as the temporal passes receive it: a native
/// or declared column, a string column parsed as timestamps, a
/// timestamp assembled from part columns, or a year-like integer
/// column recorded as a low-confidence candidate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FrameAxis {
    /// The axis column's name. For an assembled axis this is the
    /// part names joined with "/" (e.g. "year/month/day/hour"); the
    /// materialized axis column carries this name in the table the
    /// temporal passes receive.
    pub name: String,
    /// hint | native | string_parse | timestamp_parts | year_candidate
    pub source: String,
    /// One-line statement of what the axis is, for readers.
    pub description: String,
    /// declared | inferred | low
    pub confidence: String,
    /// The part columns, when the axis was assembled from parts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<String>,
    /// The source column, when the axis is or was parsed from one
    /// column of the profiled frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_column: Option<String>,
    /// The axis's measured span: the smallest axis value, ISO format
    /// (date, or datetime when the axis carries a time of day).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
    /// The largest axis value, same format as `min`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<String>,
    /// Distinct axis values present in the frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distinct_values: Option<usize>,
    /// The modal step between consecutive distinct axis values, as a
    /// reader-facing label ("1 hour", "1 day", "1 month"). Absent
    /// when the axis has fewer than two distinct values or its steps
    /// have no modal value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
}

impl FrameAxis {
    /// An axis record whose measured span is not filled in yet:
    /// the span fields are measured once, after the axis source
    /// is settled, and written over the `None`s this leaves.
    pub fn unmeasured(
        name: String,
        source: &str,
        description: String,
        confidence: &str,
        parts: Vec<String>,
        source_column: Option<String>,
    ) -> Self {
        Self {
            name,
            source: source.into(),
            description,
            confidence: confidence.into(),
            parts,
            source_column,
            min: None,
            max: None,
            distinct_values: None,
            step: None,
        }
    }
}

/// One component of a factorized grain: a dimension column or the
/// time axis, with the number of distinct values it contributes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FrameGrainFactor {
    /// The dimension column's name, or the axis's name when the
    /// component is the time axis.
    pub column: String,
    pub distinct: usize,
}

/// A near key: a candidate the account tested that almost identifies
/// rows. Recorded when no exact key exists, so the report states how
/// close the frame came instead of staying silent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FrameNearKey {
    /// The candidate's columns (the axis's source columns).
    pub columns: Vec<String>,
    /// Distinct values of the candidate present in the frame.
    pub distinct: usize,
    /// Rows whose candidate value already appeared on an earlier row.
    pub duplicate_rows: usize,
}

/// The frame's grain: the combination of columns whose values
/// identify a row, stated as account content. When the frame is a
/// cross of dimension columns and the time axis, the grain carries
/// the factorization (the distinct count of every component) and
/// whether every combination occurs (`balanced`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FrameGrain {
    /// The exact key's columns; empty when no exact key was found.
    pub key_columns: Vec<String>,
    /// The key's components with their distinct counts, when the
    /// key crosses dimension columns with the time axis.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub factorization: Vec<FrameGrainFactor>,
    /// Every combination of the factors occurs exactly once: the
    /// product of the factor distinct counts equals the row count.
    #[serde(default)]
    pub balanced: bool,
    /// The product of the factor distinct counts, when factorized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combinations: Option<usize>,
    /// The closest candidate, when no exact key exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub near_key: Option<FrameNearKey>,
    /// The account sentence, exactly as the digest and the grain
    /// finding state it.
    pub statement: String,
}

/// One measure's share of variance organised by a dominant grouping:
/// the between-group sum of squares over the total.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FrameStructureShare {
    pub measure: String,
    pub share: f64,
}

/// The peak of a dominant grouping for its strongest measure: the
/// group whose mean is highest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FrameStructurePeak {
    /// The peak group's label ("2020-04" for a monthly time pattern,
    /// "April" for an annual cycle, "17:00" for a daily cycle, the
    /// level itself for a dimension).
    pub label: String,
    pub measure: String,
    pub mean: f64,
}

/// One dominant structure: a grouping — the time axis itself, a
/// calendar cycle of the axis, or a dimension column — that organises
/// a large share of the variance of the frame's measures.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FrameDominant {
    /// time | cycle | dimension
    pub kind: String,
    /// Reader-facing name: "shared time pattern", "annual cycle",
    /// "daily cycle", or the dimension column's name.
    pub label: String,
    /// The grouping measured: "axis value", "month of year",
    /// "hour of day", "day of week", or the dimension column's name.
    pub grouping: String,
    /// The measures the grouping organises, strongest first (at
    /// most three; each share is the between-group variance share).
    pub shares: Vec<FrameStructureShare>,
    /// The grouping's peak for its strongest measure, when a peak
    /// is meaningful (time, cycle, and dimension groupings).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak: Option<FrameStructurePeak>,
    /// The account sentence, exactly as the digest states it.
    pub statement: String,
}

/// The frame model: what the dataset is, computed before the
/// detectors run — semantic column roles with their evidence, and
/// the assembled time axis — plus the account computed from it
/// (step 2): the grain and the dominant structure, measured over
/// the frame once the dependency pass has run. Detectors consume
/// this model (scaffolding columns are excluded as targets; temporal
/// passes receive the axis); the account leads the digest.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FrameInfo {
    pub roles: Vec<FrameColumnRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<FrameAxis>,
    /// The grain statement; absent when no key, near key, or
    /// factorization could be measured (the account says so only
    /// when a candidate was tested — see `FrameGrain::statement`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grain: Option<FrameGrain>,
    /// The dominant structures, strongest first (at most two).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dominant: Vec<FrameDominant>,
}

/// The complete profile report.
/// How the profiled frame was obtained from a lazy or file source:
/// the report's statement of basis for everything it contains.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceInfo {
    /// lazyframe | parquet | csv | ipc — how the source was read.
    pub kind: String,
    /// Exact row count of the source.
    pub rows: usize,
    /// Rows actually collected and profiled.
    pub rows_profiled: usize,
    /// "full" when the whole source was collected, "sampled" when
    /// rows_profiled is a source sample of rows.
    pub basis: String,
    /// The sample design when basis == "sampled".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<SourceSample>,
}

/// The source sample design: a deterministic stratified sample —
/// one row per equal stratum of the source, hash-chosen within the
/// stratum.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceSample {
    pub method: String,
    pub seed: u64,
    pub cap: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileReport {
    pub schema_version: String,
    pub overview: Overview,
    /// The frame model (roles + assembled axis); absent in reports
    /// built outside the pipeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<FrameInfo>,
    /// The lazy or file source the profiled frame was collected
    /// from; absent for in-memory DataFrame inputs, whose reports
    /// are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
    pub columns: BTreeMap<String, ColumnCard>,
    /// The ranked findings ledger. Order is the product.
    pub findings: Vec<Finding>,
    /// Statements grouped under a verified relationship family's
    /// lead (schema 0.6): full finding records, each carrying
    /// `grouped_under` with the lead's id. They spend no digest
    /// slot and no selection slot; the lead's `grouped_members`
    /// names them. Empty when no family formed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grouped_findings: Vec<Finding>,
    #[serde(default)]
    pub groups: Vec<FindingGroup>,
    #[serde(default)]
    pub selection: SelectionSummary,
    /// What was tested, at what scale, what was skipped and why.
    pub coverage: BTreeMap<String, PassInfo>,
    /// Every pair the association screens tested (issue #134):
    /// effect sizes, the raw per-pair p-value, the
    /// Benjamini–Hochberg q-value over the run's tested family,
    /// and the reporting tier the pair earned. Findings state the
    /// strong and weak pairs; this table is the full screen,
    /// including the pairs that earned no statement. Absent
    /// (empty) when no association screen ran.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub associations: Vec<AssociationPair>,
}

/// One screened pair in the report's associations table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssociationPair {
    /// First column of the pair (the conditioning column for the
    /// directional statistics and for mixed pairs).
    pub a: String,
    /// Second column of the pair.
    pub b: String,
    /// Which screen tested the pair: "numeric" (two numeric
    /// columns), "nominal" (two categorical columns), or "mixed"
    /// (categorical `a` against numeric `b`).
    pub kind: String,
    /// Pair-complete rows the statistics were computed on.
    pub n: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pearson: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spearman: Option<f64>,
    /// Chatterjee's ξ(a → b): how functional b is of a.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xi_ab: Option<f64>,
    /// Chatterjee's ξ(b → a).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xi_ba: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cramers_v: Option<f64>,
    /// Correlation ratio η of `b` across the levels of `a`
    /// (mixed pairs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eta: Option<f64>,
    /// Raw per-pair p-value, before multiple-testing correction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p_value: Option<f64>,
    /// Benjamini–Hochberg adjusted p-value over the pairs tested
    /// by the same screen in this run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q_value: Option<f64>,
    /// "strong" (significant and at or above the effect bar),
    /// "weak" (significant, below the bar), or "none".
    pub tier: String,
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

        if let Some(frame) = &self.frame {
            md.push_str("## Frame\n\n");
            if let Some(axis) = &frame.axis {
                md.push_str(&format!(
                    "- **time axis**: {} ({}, confidence {})\n",
                    axis.description, axis.source, axis.confidence
                ));
                if let (Some(min), Some(max)) = (&axis.min, &axis.max) {
                    let mut span = format!("- **axis span**: {min} to {max}");
                    if let Some(step) = &axis.step {
                        span.push_str(&format!(", in {step} steps"));
                    }
                    if let Some(d) = axis.distinct_values {
                        span.push_str(&format!(" ({d} distinct values)"));
                    }
                    span.push('\n');
                    md.push_str(&span);
                }
            }
            if let Some(grain) = &frame.grain {
                md.push_str(&format!("- **grain**: {}\n", grain.statement));
            }
            for structure in &frame.dominant {
                md.push_str(&format!(
                    "- **dominant structure**: {}\n",
                    structure.statement
                ));
            }
            let assigned: Vec<String> = frame
                .roles
                .iter()
                .filter_map(|r| {
                    r.role
                        .as_ref()
                        .map(|role| format!("{}: {}", r.column, role))
                })
                .collect();
            if !assigned.is_empty() {
                md.push_str(&format!("- **semantic roles**: {}\n", assigned.join(", ")));
            }
            md.push('\n');
        }

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

        if !self.grouped_findings.is_empty() {
            md.push_str("## Grouped statements\n\n");
            md.push_str(
                "Statements that restate a verified relationship family's lead \
                 finding, grouped under it (schema 0.6). They spend no digest \
                 slot; each is listed with its lead and its evidence.\n\n",
            );
            for f in &self.grouped_findings {
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
                    "- **[{}·{}] {}** — {} (grouped under {})\n",
                    f.interestingness,
                    format!("{:?}", f.primary_evidence().basis).to_lowercase(),
                    label,
                    f.claim,
                    f.grouped_under.as_deref().unwrap_or("?"),
                ));
                for e in &f.evidence {
                    md.push_str(&render_evidence(f, e));
                }
            }
            md.push('\n');
        }

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
