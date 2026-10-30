//! Algorithm-independent statements and the evidence supporting them.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub type Details = BTreeMap<String, Value>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Missingness,
    Concentration,
    ColumnRoles,
    Uniqueness,
    Duplicates,
    Formats,
    Dependencies,
    ConditionalRules,
    Associations,
    Gaps,
    Intervals,
    Ordering,
    Changes,
}
impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Missingness => "missingness",
            Self::Concentration => "concentration",
            Self::ColumnRoles => "column_roles",
            Self::Uniqueness => "uniqueness",
            Self::Duplicates => "duplicates",
            Self::Formats => "formats",
            Self::Dependencies => "dependencies",
            Self::ConditionalRules => "conditional_rules",
            Self::Associations => "associations",
            Self::Gaps => "gaps",
            Self::Intervals => "intervals",
            Self::Ordering => "ordering",
            Self::Changes => "changes",
        }
    }
}

/// The logical statement, independent of the detector's name or crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    MissingValues,
    SentinelValues,
    ValueConcentration,
    Hotspot,
    CategoricalRole,
    IdentifierRole,
    Uniqueness,
    Duplicates,
    FormatVariation,
    Heaping,
    DigitPreference,
    BenfordDeviation,
    FunctionalDependency,
    AffineDependency,
    ArithmeticIdentity,
    PredictiveDependency,
    PiecewiseDependency,
    ConditionalRule,
    StatisticalAssociation,
    PeriodAssociation,
    Gap,
    SequenceGap,
    MissingCombinations,
    IntervalVariation,
    ColumnOrdering,
    ResidualShift,
    LevelShift,
    TrendChange,
    VarianceChange,
    LongFormat,
    ExtremeValue,
    CategoricalOutlier,
    TransientExcursion,
    StepDiscontinuity,
    DuplicateEncoding,
    DistributionShift,
}
impl Pattern {
    pub fn category(self) -> Category {
        match self {
            Self::MissingValues | Self::SentinelValues => Category::Missingness,
            Self::ValueConcentration | Self::Hotspot => Category::Concentration,
            Self::CategoricalRole | Self::IdentifierRole => Category::ColumnRoles,
            Self::Uniqueness => Category::Uniqueness,
            Self::Duplicates => Category::Duplicates,
            Self::FormatVariation | Self::Heaping | Self::DigitPreference
            | Self::BenfordDeviation => Category::Formats,
            Self::FunctionalDependency
            | Self::AffineDependency
            | Self::ArithmeticIdentity
            | Self::PredictiveDependency
            | Self::PiecewiseDependency => Category::Dependencies,
            Self::ConditionalRule => Category::ConditionalRules,
            Self::StatisticalAssociation | Self::PeriodAssociation => Category::Associations,
            Self::Gap => Category::Gaps,
            Self::SequenceGap | Self::MissingCombinations => Category::Gaps,
            Self::IntervalVariation => Category::Intervals,
            Self::ColumnOrdering => Category::Ordering,
            Self::ResidualShift | Self::LevelShift | Self::TrendChange | Self::VarianceChange => {
                Category::Changes
            }
            Self::LongFormat => Category::ColumnRoles,
            Self::ExtremeValue | Self::CategoricalOutlier => Category::Intervals,
            Self::TransientExcursion => Category::Changes,
            Self::StepDiscontinuity => Category::Changes,
            Self::DuplicateEncoding => Category::ColumnRoles,
            Self::DistributionShift => Category::Changes,
        }
    }
    /// The finding `type`: every pattern carries one, so consumers
    /// can key on (category, type) without special-casing absent
    /// types. The type is part of the finding id; the strings are a
    /// compatibility surface.
    pub fn subtype(self) -> &'static str {
        match self {
            Self::MissingValues => "missing_values",
            Self::SentinelValues => "sentinel_values",
            Self::ValueConcentration => "value_concentration",
            Self::Hotspot => "hotspot",
            Self::CategoricalRole => "categorical_role",
            Self::IdentifierRole => "identifier_role",
            Self::Uniqueness => "uniqueness",
            Self::Duplicates => "duplicate_rows",
            Self::FormatVariation => "format_variation",
            Self::Heaping => "heaping",
            Self::DigitPreference => "digit_preference",
            Self::BenfordDeviation => "benford_deviation",
            Self::FunctionalDependency | Self::AffineDependency | Self::ArithmeticIdentity => {
                "functional"
            }
            Self::PredictiveDependency | Self::PiecewiseDependency => "predictive",
            Self::ConditionalRule => "if_then",
            Self::StatisticalAssociation => "statistical",
            Self::PeriodAssociation => "period_association",
            Self::Gap => "gap",
            Self::SequenceGap => "sequence_gap",
            Self::MissingCombinations => "missing_combinations",
            Self::IntervalVariation => "interval_variation",
            Self::ColumnOrdering => "column_ordering",
            Self::ResidualShift => "residual_level",
            Self::LevelShift => "level_shift",
            Self::TrendChange => "trend_change",
            Self::VarianceChange => "variance_change",
            Self::LongFormat => "long_format",
            Self::ExtremeValue => "extreme_value",
            Self::CategoricalOutlier => "categorical_outlier",
            Self::TransientExcursion => "transient_excursion",
            Self::StepDiscontinuity => "step_discontinuity",
            Self::DistributionShift => "distribution_shift",
            Self::DuplicateEncoding => "duplicate_encoding",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subject {
    pub columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}
impl Subject {
    pub fn columns(columns: impl IntoIterator<Item = String>) -> Self {
        let mut s = Self {
            columns: columns.into_iter().collect(),
            ..Self::default()
        };
        s.canonicalize();
        s
    }
    pub fn dependency(inputs: Vec<String>, target: String) -> Self {
        let mut columns = inputs.clone();
        columns.push(target.clone());
        let mut s = Self {
            columns,
            inputs,
            target: Some(target),
            unit: None,
        };
        s.canonicalize();
        s
    }
    pub fn canonicalize(&mut self) {
        self.columns.sort();
        self.columns.dedup();
        self.inputs.sort();
        self.inputs.dedup();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Predicate {
    pub column: String,
    pub operator: Operator,
    pub value: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<String>,
}
impl Predicate {
    pub fn canonicalize(&mut self) {
        if self.value.as_number().is_some_and(|n| n.is_f64()) {
            if let Some(x) = self.value.as_f64() {
                if x.fract() == 0.0 && x.abs() <= 9_007_199_254_740_992.0 {
                    self.value = json!(x as i64);
                }
            }
        }
    }

    pub fn equal(column: impl Into<String>, value: Value) -> Self {
        Self {
            column: column.into(),
            operator: Operator::Eq,
            value,
            transform: None,
        }
    }
}

/// Scope and form are parts of the statement; measurements live in evidence.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Specification {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<Predicate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Predicate>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub definition: Details,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceBasis {
    Measured,
    Estimated,
    Inferred,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sampling {
    Full,
    Sample,
    #[default]
    Unspecified,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EvidenceScope {
    pub sampling: Sampling,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub population_rows: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows_evaluated: Option<usize>,
    pub missing_values: String,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Validation {
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training_rows: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation_rows: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folds: Option<usize>,
    pub used_for_selection: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Method {
    pub detector: String,
    pub algorithm: String,
    pub provenance: String,
}
impl Method {
    pub fn new(detector: &str, algorithm: &str, provenance: &str) -> Self {
        Self {
            detector: detector.into(),
            algorithm: algorithm.into(),
            provenance: provenance.into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub method: Method,
    pub basis: EvidenceBasis,
    pub scope: EvidenceScope,
    pub validation: Validation,
    pub metrics: Details,
}
impl Evidence {
    pub fn new(method: Method, basis: EvidenceBasis, metrics: Details) -> Self {
        let count = |k: &str| metrics.get(k).and_then(Value::as_u64).map(|n| n as usize);
        let training = count("n_train").or_else(|| count("train_pair_n"));
        let holdout = count("n_holdout").or_else(|| count("holdout_pair_n"));
        let folds = count("folds");
        let n = training
            .zip(holdout)
            .map(|(a, b)| a + b)
            .or_else(|| count("n_used"))
            .or_else(|| count("n"))
            .or_else(|| count("rows_used"))
            .or_else(|| count("n_values"));
        let validation = if folds.is_some() {
            "cross_validation"
        } else if holdout.is_some() {
            "diagnostic_holdout"
        } else if basis == EvidenceBasis::Measured {
            "direct_comparison"
        } else {
            "none"
        };
        let sampled = metrics
            .get("row_sampled")
            .or_else(|| metrics.get("sampled"))
            .and_then(Value::as_bool);
        let population = count("population_rows");
        Self {
            method,
            basis,
            scope: EvidenceScope {
                sampling: match sampled {
                    Some(true) => Sampling::Sample,
                    Some(false) => Sampling::Full,
                    None => Sampling::Unspecified,
                },
                population_rows: population,
                rows_evaluated: n,
                missing_values: "see detector scope".into(),
            },
            validation: Validation {
                method: validation.into(),
                training_rows: training,
                evaluation_rows: holdout.or_else(|| folds.and(n)),
                folds,
                used_for_selection: holdout.is_some() || folds.is_some(),
            },
            metrics,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub category: Category,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub finding_type: Option<String>,
    pub subject: Subject,
    pub specification: Specification,
    pub claim: String,
    pub evidence: Vec<Evidence>,
    pub interestingness: f64,
    /// The score the producing detector assigned, recorded by the ledger
    /// before selection scoring. `interestingness` holds the selection
    /// score; this field keeps the detector's own judgment on record so
    /// the replacement is never silent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detector_interestingness: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpretation: Option<String>,
}
impl Finding {
    /// Convenience constructor for the built-in detectors. New plugins can use
    /// `from_statement` directly without depending on a detector-specific schema.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pattern: Pattern,
        raw: Details,
        claim: String,
        basis: EvidenceBasis,
        score: f64,
        metrics: Details,
        interpretation: Option<String>,
        method: Method,
    ) -> Self {
        let strings = |k: &str| {
            raw.get(k)
                .and_then(Value::as_array)
                .map(|v| {
                    v.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        let string = |k: &str| raw.get(k).and_then(Value::as_str).map(str::to_owned);
        let mut inputs = strings("inputs");
        if inputs.is_empty() {
            inputs = strings("lhs");
        }
        if inputs.is_empty() {
            inputs = strings("predictors");
        }
        let dependency = pattern.category() == Category::Dependencies;
        if dependency && inputs.is_empty() {
            inputs = metrics
                .get("inputs")
                .or_else(|| metrics.get("features"))
                .and_then(Value::as_array)
                .map(|v| {
                    v.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
        }
        if pattern == Pattern::AffineDependency {
            inputs = string("x").into_iter().collect();
        }
        let target = if pattern == Pattern::AffineDependency {
            string("y")
        } else if dependency {
            string("target")
                .or_else(|| string("rhs"))
                .or_else(|| string("column"))
        } else {
            None
        };
        let mut columns = strings("columns");
        for k in [
            "column",
            "x",
            "y",
            "time_column",
            "target",
            "antecedent_feature",
            "consequent_feature",
        ] {
            if let Some(c) = string(k) {
                columns.push(c);
            }
        }
        columns.extend(inputs.clone());
        if let Some(t) = &target {
            columns.push(t.clone());
        }
        let mut subject = Subject {
            columns,
            inputs,
            target,
            unit: None,
        };
        subject.canonicalize();
        let mut spec = Specification::default();
        match pattern {
            Pattern::AffineDependency => {
                spec.form = Some("affine".into());
                spec.definition.insert(
                    "missing_values".into(),
                    json!("exclude incomplete inputs and target"),
                );
                for k in [
                    "a",
                    "b",
                    "tolerance",
                    "absolute_tolerance",
                    "relative_tolerance",
                ] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
            }
            Pattern::ArithmeticIdentity => {
                if let Some(v) = metrics.get("form").and_then(Value::as_str) {
                    spec.form = Some(v.to_owned());
                }
                spec.definition.insert(
                    "missing_values".into(),
                    json!("exclude incomplete inputs and target"),
                );
                for k in [
                    "coefficients",
                    "tolerance",
                    "absolute_tolerance",
                    "relative_tolerance",
                ] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
            }
            Pattern::PiecewiseDependency => {
                spec.form = Some("piecewise_linear".into());
                if let Some(v) = metrics.get("leaves") {
                    spec.definition.insert("pieces".into(), v.clone());
                }
            }
            Pattern::FunctionalDependency => {
                spec.form = Some("value_mapping".into());
                spec.definition.insert(
                    "missing_values".into(),
                    json!("exclude incomplete inputs and target"),
                );
                spec.definition
                    .insert("comparison".into(), json!("canonical_equality"));
            }
            Pattern::CategoricalRole => {
                spec.definition.insert("role".into(), json!("categorical"));
            }
            Pattern::IdentifierRole => {
                spec.definition.insert("role".into(), json!("identifier"));
            }
            Pattern::Duplicates => {
                subject.unit = Some("rows".into());
                spec.definition
                    .insert("comparison".into(), json!("exact_equality"));
                spec.definition
                    .insert("missing_values_equal".into(), json!(true));
            }
            Pattern::Uniqueness => {
                subject.unit = Some(
                    if subject.columns.len() > 1 {
                        "tuples"
                    } else {
                        "column_values"
                    }
                    .into(),
                );
                spec.definition
                    .insert("comparison".into(), json!("canonical_equality"));
            }
            Pattern::Gap => {
                subject.unit = Some("values".into());
                spec.definition.insert("reference".into(),json!({"calendar":metrics.get("reference"),"cadence_days":metrics.get("cadence_days"),"holidays":"not modeled"}));
                spec.definition.insert("domain".into(), json!("temporal"));
                for k in ["from", "to"] {
                    if let Some(v) = raw.get(k).or_else(|| metrics.get(k)) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
            }
            Pattern::SequenceGap => {
                subject.unit = Some("values".into());
                spec.definition.insert("domain".into(), json!("sequence"));
                spec.definition.insert(
                    "reference".into(),
                    json!({"step": metrics.get("step"), "source": "declared"}),
                );
                if let Some(v) = metrics.get("absent_values") {
                    spec.definition.insert("absent_values".into(), v.clone());
                }
                for k in ["from", "to"] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
            }
            Pattern::MissingCombinations => {
                subject.unit = Some("cells".into());
                spec.definition
                    .insert("domain".into(), json!("combinations"));
                spec.definition.insert(
                    "reference".into(),
                    json!({"axis": metrics.get("axis"), "cadence": metrics.get("cadence"), "source": "declared"}),
                );
                if let Some(v) = metrics.get("absent_values") {
                    spec.definition.insert("absent_values".into(), v.clone());
                }
            }
            Pattern::ColumnOrdering => {
                spec.definition
                    .insert("direction".into(), json!("ascending"));
                spec.definition
                    .insert("missing_values".into(), json!("ignored"));
            }
            Pattern::PeriodAssociation => {
                spec.form = Some("period_split".into());
                if let Some(pc) = string("period_column") {
                    if !subject.columns.contains(&pc) {
                        subject.columns.push(pc.clone());
                        subject.canonicalize();
                    }
                    spec.definition.insert("period_column".into(), json!(pc));
                }
                if let Some(v) = raw.get("periods") {
                    spec.definition.insert("periods".into(), v.clone());
                }
                spec.definition.insert("statistic".into(), json!("pearson"));
                spec.definition.insert(
                    "missing_values".into(),
                    json!("rows without a period, or with a missing value in either column, are excluded"),
                );
            }
            Pattern::ResidualShift => {
                subject.target = string("target");
                if let Some(model) = metrics.get("reference_model") {
                    spec.definition
                        .insert("reference_model".into(), model.clone());
                }
                if let Some(v) = metrics.get("breakpoint") {
                    spec.definition.insert("breakpoint".into(), v.clone());
                }
            }
            Pattern::LevelShift | Pattern::TrendChange | Pattern::VarianceChange => {
                if let Some(v) = metrics.get("breakpoint") {
                    spec.definition.insert("breakpoint".into(), v.clone());
                }
            }
            Pattern::LongFormat => {
                spec.form = Some("pivot".into());
                for k in ["index", "attribute", "measure"] {
                    if let Some(v) = raw.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
                spec.definition.insert(
                    "missing_values".into(),
                    json!("index and attribute columns complete; measure values may be missing"),
                );
            }
            Pattern::TransientExcursion => {
                spec.form = Some("excursion".into());
                for k in ["event_start", "event_end", "peak_time"] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
            }
            Pattern::StepDiscontinuity => {
                spec.form = Some("step".into());
                for k in [
                    "kind",
                    "threshold",
                    "direction_deg",
                    "boundary_slope",
                    "boundary_intercept",
                ] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
                spec.definition.insert(
                    "missing_values".into(),
                    json!("exclude rows incomplete on target or axis columns"),
                );
            }
            Pattern::DistributionShift => {
                spec.form = Some("drift".into());
                for k in ["time_column", "reference_period", "max_period"] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
                spec.definition.insert(
                    "method".into(),
                    json!("gain-weighted mean chi-square of per-split routing counts, comparison period vs reference period, over the reference model's trees; the finding requires the maximum score to exceed a label-permutation maximum"),
                );
                spec.definition.insert(
                    "missing_values".into(),
                    json!("exclude rows missing the target or the time value"),
                );
            }
            Pattern::DuplicateEncoding => {
                spec.form = Some(
                    metrics
                        .get("form")
                        .and_then(Value::as_str)
                        .unwrap_or("duplicate_encoding")
                        .to_string(),
                );
                spec.definition
                    .insert("comparison".into(), json!("canonical_equality"));
                spec.definition.insert(
                    "missing_values".into(),
                    json!("exclude rows missing either column"),
                );
                for k in ["finer", "coarser"] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
            }
            Pattern::PredictiveDependency => {
                if metrics.get("form").and_then(Value::as_str) == Some("interaction_h") {
                    spec.form = Some("interaction_h".into());
                    spec.definition.insert(
                        "statistic".into(),
                        json!("Friedman-Popescu H on the surrogate's partial dependences, gated on the surrogate beating an additive refit out of sample"),
                    );
                    spec.definition.insert(
                        "missing_values".into(),
                        json!("rows missing the target excluded; predictor handling follows the recorded model"),
                    );
                } else if let Some(form @ ("step_shape" | "curved_shape")) =
                    metrics.get("form").and_then(Value::as_str)
                {
                    spec.form = Some(form.into());
                    spec.definition.insert(
                        "statistic".into(),
                        json!("shape decomposition against the designated target: linear, binned-curve (df-adjusted) and best single-step R² shares"),
                    );
                    spec.definition.insert(
                        "missing_values".into(),
                        json!("complete cases per predictor: rows missing the predictor or the target excluded"),
                    );
                }
            }
            Pattern::ExtremeValue => {
                spec.form = Some("isolation_gap".into());
                for k in ["condition_column", "condition_value"] {
                    if let Some(v) = raw.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
            }
            _ => {}
        }
        let basis = if matches!(pattern, Pattern::CategoricalRole | Pattern::IdentifierRole) {
            EvidenceBasis::Inferred
        } else {
            basis
        };
        let evidence = Evidence::new(method, basis, metrics);
        Self::from_statement(
            pattern.category(),
            Some(pattern.subtype().to_owned()),
            subject,
            spec,
            claim,
            evidence,
            score,
            interpretation,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn from_statement(
        category: Category,
        finding_type: Option<String>,
        subject: Subject,
        specification: Specification,
        claim: String,
        evidence: Evidence,
        score: f64,
        interpretation: Option<String>,
    ) -> Self {
        Self {
            id: String::new(),
            category,
            finding_type,
            subject,
            specification,
            claim,
            evidence: vec![evidence],
            interestingness: score.clamp(0.0, 1.0),
            detector_interestingness: None,
            interpretation,
        }
    }
    pub fn with_rule(mut self, mut conditions: Vec<Predicate>, outcome: Predicate) -> Self {
        for p in &mut conditions {
            p.canonicalize();
        }
        let mut outcome = outcome;
        outcome.canonicalize();
        conditions.sort_by_key(|c| serde_json::to_string(c).unwrap());
        conditions.dedup();
        self.subject = Subject::dependency(
            conditions.iter().map(|c| c.column.clone()).collect(),
            outcome.column.clone(),
        );
        self.specification.definition.insert(
            "missing_values".into(),
            json!("complete predicates and outcome"),
        );
        self.specification.conditions = conditions;
        self.specification.outcome = Some(outcome);
        self
    }
    pub fn is(&self, category: Category, subtype: Option<&str>) -> bool {
        self.category == category && subtype.is_none_or(|t| self.finding_type.as_deref() == Some(t))
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.evidence.is_empty() {
            return Err("a finding must carry evidence".into());
        }
        if self.subject.columns.is_empty() {
            return Err("a finding must identify its columns".into());
        }
        if self.subject.columns.iter().any(|c| c.is_empty()) {
            return Err("empty column name".into());
        }
        if self.finding_type.is_none() {
            return Err("a finding must carry a type".into());
        }
        match self.category {
            Category::Dependencies => {
                if !matches!(
                    self.finding_type.as_deref(),
                    Some("functional" | "predictive")
                ) {
                    return Err("dependency type must be functional or predictive".into());
                }
                if self.subject.inputs.is_empty() || self.subject.target.is_none() {
                    return Err("a dependency requires inputs and a target".into());
                }
                if self
                    .subject
                    .target
                    .as_ref()
                    .is_some_and(|t| self.subject.inputs.contains(t))
                {
                    return Err("the target cannot be its own predictor".into());
                }
            }
            Category::ConditionalRules => {
                if self.finding_type.as_deref() != Some("if_then") {
                    return Err("conditional rule type must be if_then".into());
                }
                if self.specification.conditions.is_empty() || self.specification.outcome.is_none()
                {
                    return Err("a rule requires predicates and an outcome".into());
                }
                for p in self
                    .specification
                    .conditions
                    .iter()
                    .chain(self.specification.outcome.iter())
                {
                    if p.value.is_null() || p.value.is_array() || p.value.is_object() {
                        return Err("rule predicates require non-missing scalar values".into());
                    }
                    if p.transform.as_ref().is_some_and(|t| t != "day_index") {
                        return Err("unsupported predicate transform".into());
                    }
                    if !self.subject.columns.contains(&p.column) {
                        return Err("predicate column absent from subject".into());
                    }
                }
            }
            Category::Associations
                if !matches!(
                    self.finding_type.as_deref(),
                    Some("statistical" | "period_association")
                ) =>
            {
                return Err("association type must be statistical or period_association".into());
            }
            Category::Changes
                if !matches!(
                    self.finding_type.as_deref(),
                    Some(
                        "residual_level"
                            | "level_shift"
                            | "trend_change"
                            | "variance_change"
                            | "transient_excursion"
                            | "step_discontinuity"
                            | "distribution_shift",
                    )
                ) =>
            {
                return Err("change type must be residual_level, level_shift, trend_change, variance_change, transient_excursion, step_discontinuity, or distribution_shift".into());
            }
            Category::Duplicates
                if self.subject.unit.is_none()
                    || !self.specification.definition.contains_key("comparison") =>
            {
                return Err("duplicates require a unit and comparison criterion".into());
            }
            Category::Gaps => {
                let d = &self.specification.definition;
                if !d.contains_key("domain") || !d.contains_key("reference") {
                    return Err("gaps require a domain and reference".into());
                }
                if !(d.contains_key("absent_values")
                    || d.contains_key("from") && d.contains_key("to"))
                {
                    return Err("gaps require interval bounds or absent values".into());
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn primary_evidence(&self) -> &Evidence {
        self.evidence
            .iter()
            .max_by_key(|e| {
                (
                    u8::from(
                        e.metrics.contains_key("conditional_rate")
                            && e.validation.method == "diagnostic_partition",
                    ),
                    u8::from(
                        e.validation.method == "direct_comparison"
                            && e.scope.sampling == Sampling::Full,
                    ),
                    u8::from(
                        e.validation.method == "cross_validation"
                            || e.validation.method == "diagnostic_holdout",
                    ),
                    e.scope.rows_evaluated.unwrap_or(0),
                )
            })
            .expect("a finding requires supporting evidence")
    }
    pub fn metrics(&self) -> &Details {
        &self.primary_evidence().metrics
    }
    pub fn canonical_key(&self) -> String {
        let mut subject = self.subject.clone();
        subject.canonicalize();
        let mut specification = self.specification.clone();
        for p in &mut specification.conditions {
            p.canonicalize();
        }
        if let Some(p) = &mut specification.outcome {
            p.canonicalize();
        }
        specification
            .conditions
            .sort_by_key(|p| serde_json::to_string(p).unwrap());
        serde_json::to_string(&(self.category, &self.finding_type, subject, specification)).unwrap()
    }
    pub fn detectors(&self) -> BTreeSet<&str> {
        self.evidence
            .iter()
            .map(|e| e.method.detector.as_str())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FindingGroup {
    pub id: String,
    pub columns: Vec<String>,
    pub finding_ids: Vec<String>,
    pub description: String,
}
