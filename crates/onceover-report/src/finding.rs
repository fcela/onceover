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
    Anomalies,
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
            Self::Anomalies => "anomalies",
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
    PredictableMissingness,
    ValueConcentration,
    ModalShare,
    SpikeCells,
    Hotspot,
    CategoricalRole,
    IdentifierRole,
    Uniqueness,
    Duplicates,
    FormatVariation,
    Heaping,
    DigitPreference,
    BenfordDeviation,
    BoundaryMass,
    /// A column's values against what the column appears to hold.
    ContentAnomaly,
    /// What a series does over its whole span along the time axis.
    SeriesStructure,
    /// Rows matching another on every column but one.
    NearDuplicateRows,
    /// Rows that disagree with a relation between columns.
    RowInconsistency,
    FunctionalDependency,
    AffineDependency,
    ArithmeticIdentity,
    PredictiveDependency,
    PiecewiseDependency,
    /// One column that on its own nearly determines a designated
    /// target: its chance-corrected share of the target's entropy
    /// (categorical target) or its directed dependence on the
    /// target (numeric target) clears a high bar and dwarfs every
    /// other column, or it determines the target exactly on the
    /// evaluated rows. The shape of a column derived from the
    /// outcome or recorded after it.
    LeakageSignature,
    OrderConstraint,
    /// A pair order that fails on a substantial, one-sided share of
    /// rows: too many violations for the near-universal order
    /// statement, too asymmetric to be two unrelated columns
    /// crossing. The complementary statement to OrderConstraint
    /// from the same scan.
    OrderViolation,
    /// A slice of rows where a cross-fitted model of the designated
    /// target fails: its out-of-fold errors inside the slice are
    /// one-sided (the bias form) or materially larger than the
    /// model's overall error (the accuracy form). Stated by the
    /// target-gated model-failure pass, against the model's overall
    /// error as the baseline.
    ModelFailure,
    ConditionalRule,
    StatisticalAssociation,
    PeriodAssociation,
    Gap,
    SequenceGap,
    MissingCombinations,
    /// An observed period on the time axis carrying a fraction of
    /// the typical period's volume (rows, entities).
    PartialPeriod,
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
    /// A change in the joint distribution of the measure columns
    /// along the time axis, located by classifier-based binary
    /// segmentation (the `changeforest` detector): the split is
    /// where a random forest best distinguishes the rows before
    /// from the rows after, kept when its permutation p-value
    /// clears the stated bar. Carries no per-coordinate summary:
    /// means, variances and trends of every measure can be
    /// unchanged across the split.
    DistributionChange,
    SlopeBreak,
    RegimeContrast,
    SpikeContrast,
    AxisShape,
    OrderedPartition,
    /// A boolean flag on an entity × time grain that switches on
    /// within entities and never switches off (an absorbing state),
    /// with the lag to a related activity column when one is
    /// associated.
    OneWayFlag,
    /// A block of entities first appearing in one period of an
    /// entity × time grain: far more arrivals in that period than
    /// the panel's typical period, and a material share of all
    /// entities.
    EntryBlock,
    /// A block of entities last appearing in one period of an
    /// entity × time grain: the mirror of EntryBlock on the exit
    /// side.
    ExitBlock,
    /// The distribution of the period in which entities first
    /// become active on a measure, relative to the period in which
    /// they are first observed: a material share of entities start
    /// activity after their first observed period.
    FirstActivity,
    /// A declared subset of the rows against the rest of the table
    /// on one column: the two populations' means, medians, or level
    /// shares, stated as a difference with its size. The group is
    /// declared, not inferred — the counterpart to a statistical
    /// association, which relates two columns over all rows.
    GroupContrast,
    /// A pooled association between two columns that does not
    /// survive conditioning on a categorical dimension: within
    /// every usable level of the stratifier the direction
    /// reverses (form `reversal`), or the association is absent
    /// (form `vanishing`). The pooled statement is an artefact
    /// of the stratifier's mix; the conditioned statement is
    /// the fact about the pair.
    ConditionedAssociation,
    /// One level of a categorical column against the pooled rest
    /// on a numeric measure: the level's median and interquartile
    /// range against the rest's, stated when the location shift
    /// clears a stated fraction of the pooled spread or the IQR
    /// ratio clears the scale bar (forms `location`, `scale`,
    /// `location_scale`), or when the level's values sit at a
    /// near-constant multiple of the rest's distribution (form
    /// `constant_multiple` — the unit-error shape). The grouping
    /// can also be a calendar part of the table's date axis
    /// (day of week, month of year, hour of day). Where the
    /// group contrast compares one declared subset with the
    /// rest, this pattern is the automatic scan: every level of
    /// every low-cardinality categorical, found not declared.
    LevelContrast,
    /// A conjunction of conditions (a slice), searched rather than
    /// declared, under which a target column's distribution differs
    /// from the table-wide one: a tail share, a median shift, or a
    /// categorical level share. The statement is stated only when
    /// the shift reappears on held-out rows the search never saw.
    SliceShift,
    /// The share of a measure held by the top entities in each
    /// period of an entity × time grain: high in level, or rising
    /// materially across the period range. A panel statement —
    /// the whole-column concentration check computes shares over
    /// the whole frame at once and cannot see the movement.
    ConcentrationOverTime,
    /// Entities whose total of a measure over an entity × time
    /// grain sits far outside the distribution of the other
    /// entities' totals. No single cell needs to be extreme:
    /// every row of such an entity is.
    OutlierEntity,
    /// The designated outcome's own distribution: for a numeric
    /// outcome its center, spread, and range in the outcome's own
    /// units; for a categorical outcome the share of each level
    /// (and, with an event declared, the event's rate over the
    /// whole file and on the event basis). The base every
    /// outcome-relative statement compares against.
    OutcomeDistribution,
    /// One level of a categorical column against the designated
    /// outcome: with an event declared, the level's event rate
    /// against the pooled event rate; for a numeric outcome, the
    /// level's median against the rest's. A grouping measured and
    /// found without a difference is stated the same way, as a
    /// null: the rates or medians and the absence.
    OutcomeContrast,
}
impl Pattern {
    /// Whether a statement of this pattern names a target or an
    /// outcome column: the `directed` input of facet category
    /// derivation. A finding's category is derived from its facets
    /// (Facets::derive_category) with this flag; there is no second,
    /// pattern-level assignment.
    pub fn names_target(self) -> bool {
        matches!(
            self,
            Self::FunctionalDependency
                | Self::AffineDependency
                | Self::ArithmeticIdentity
                | Self::PredictiveDependency
                | Self::PiecewiseDependency
                | Self::OrderConstraint
                | Self::OrderViolation
                | Self::ModelFailure
                | Self::LeakageSignature
                | Self::ConditionalRule
                | Self::OutcomeDistribution
                | Self::OutcomeContrast
        )
    }
    /// The finding `type`: every pattern carries one, so consumers
    /// can key on (category, type) without special-casing absent
    /// types. The type is part of the finding id; the strings are a
    /// compatibility surface.
    pub fn subtype(self) -> &'static str {
        match self {
            Self::MissingValues => "missing_values",
            Self::SentinelValues => "sentinel_values",
            Self::PredictableMissingness => "predictable_missingness",
            Self::ValueConcentration => "value_concentration",
            Self::ModalShare => "modal_share",
            Self::SpikeCells => "spike_cells",
            Self::Hotspot => "hotspot",
            Self::CategoricalRole => "categorical_role",
            Self::IdentifierRole => "identifier_role",
            Self::Uniqueness => "uniqueness",
            Self::Duplicates => "duplicate_rows",
            Self::FormatVariation => "format_variation",
            Self::Heaping => "heaping",
            Self::DigitPreference => "digit_preference",
            Self::BenfordDeviation => "benford_deviation",
            Self::BoundaryMass => "boundary_mass",
            Self::ContentAnomaly => "content_anomaly",
            Self::SeriesStructure => "series_structure",
            Self::NearDuplicateRows => "near_duplicate_rows",
            Self::RowInconsistency => "row_inconsistency",
            Self::FunctionalDependency => "functional",
            Self::AffineDependency => "affine",
            Self::ArithmeticIdentity => "arithmetic_identity",
            Self::PredictiveDependency | Self::PiecewiseDependency => "predictive",
            Self::LeakageSignature => "leakage_signature",
            Self::OrderConstraint => "order_constraint",
            Self::OrderViolation => "order_violation",
            Self::ModelFailure => "model_failure",
            Self::ConditionalRule => "if_then",
            Self::StatisticalAssociation => "statistical",
            Self::PeriodAssociation => "period_association",
            Self::GroupContrast => "group_contrast",
            Self::ConditionedAssociation => "conditioned_association",
            Self::LevelContrast => "level_contrast",
            Self::SliceShift => "slice_shift",
            Self::Gap => "gap",
            Self::SequenceGap => "sequence_gap",
            Self::MissingCombinations => "missing_combinations",
            Self::PartialPeriod => "partial_period",
            Self::IntervalVariation => "interval_variation",
            Self::ColumnOrdering => "column_ordering",
            Self::ResidualShift => "residual_shift",
            Self::LevelShift => "level_shift",
            Self::TrendChange => "trend_change",
            Self::VarianceChange => "variance_change",
            Self::LongFormat => "long_format",
            Self::ExtremeValue => "extreme_value",
            Self::CategoricalOutlier => "categorical_outlier",
            Self::TransientExcursion => "transient_excursion",
            Self::StepDiscontinuity => "step_discontinuity",
            Self::DistributionShift => "predictor_drift",
            Self::DistributionChange => "joint_distribution_change",
            Self::SlopeBreak => "slope_break",
            Self::RegimeContrast => "regime_contrast",
            Self::SpikeContrast => "spike_contrast",
            Self::AxisShape => "axis_shape",
            Self::OrderedPartition => "ordered_partition",
            Self::OneWayFlag => "one_way_flag",
            Self::EntryBlock => "entry_block",
            Self::ExitBlock => "exit_block",
            Self::FirstActivity => "first_activity",
            Self::ConcentrationOverTime => "concentration_over_time",
            Self::OutlierEntity => "outlier_entity",
            Self::OutcomeDistribution => "outcome_distribution",
            Self::OutcomeContrast => "outcome_contrast",
            Self::DuplicateEncoding => "duplicate_encoding",
        }
    }

    /// Every pattern, in declaration order. Validation derives each
    /// category's accepted type strings from this list, so a type
    /// string exists in exactly one place: `subtype()`.
    pub const ALL: &'static [Pattern] = &[
        Pattern::MissingValues,
        Pattern::SentinelValues,
        Pattern::PredictableMissingness,
        Pattern::ValueConcentration,
        Pattern::ModalShare,
        Pattern::SpikeCells,
        Pattern::Hotspot,
        Pattern::CategoricalRole,
        Pattern::IdentifierRole,
        Pattern::Uniqueness,
        Pattern::Duplicates,
        Pattern::FormatVariation,
        Pattern::Heaping,
        Pattern::DigitPreference,
        Pattern::BenfordDeviation,
        Pattern::BoundaryMass,
        Pattern::ContentAnomaly,
        Pattern::SeriesStructure,
        Pattern::NearDuplicateRows,
        Pattern::RowInconsistency,
        Pattern::FunctionalDependency,
        Pattern::AffineDependency,
        Pattern::ArithmeticIdentity,
        Pattern::PredictiveDependency,
        Pattern::PiecewiseDependency,
        Pattern::LeakageSignature,
        Pattern::OrderConstraint,
        Pattern::OrderViolation,
        Pattern::ModelFailure,
        Pattern::ConditionalRule,
        Pattern::StatisticalAssociation,
        Pattern::PeriodAssociation,
        Pattern::Gap,
        Pattern::SequenceGap,
        Pattern::MissingCombinations,
        Pattern::PartialPeriod,
        Pattern::IntervalVariation,
        Pattern::ColumnOrdering,
        Pattern::ResidualShift,
        Pattern::LevelShift,
        Pattern::TrendChange,
        Pattern::VarianceChange,
        Pattern::LongFormat,
        Pattern::ExtremeValue,
        Pattern::CategoricalOutlier,
        Pattern::TransientExcursion,
        Pattern::StepDiscontinuity,
        Pattern::DuplicateEncoding,
        Pattern::DistributionShift,
        Pattern::DistributionChange,
        Pattern::SlopeBreak,
        Pattern::RegimeContrast,
        Pattern::SpikeContrast,
        Pattern::AxisShape,
        Pattern::OrderedPartition,
        Pattern::OneWayFlag,
        Pattern::EntryBlock,
        Pattern::ExitBlock,
        Pattern::FirstActivity,
        Pattern::GroupContrast,
        Pattern::ConditionedAssociation,
        Pattern::LevelContrast,
        Pattern::SliceShift,
        Pattern::ConcentrationOverTime,
        Pattern::OutlierEntity,
        Pattern::OutcomeDistribution,
        Pattern::OutcomeContrast,
    ];

    /// The distinct type strings accepted for `category`, in
    /// declaration order, derived from `ALL`. Each pattern's category
    /// comes from the same facet derivation that assigns a finding's
    /// category, evaluated on the pattern's canonical facets (empty
    /// metrics, empty specification, two-column subject).
    pub fn types_for(category: Category) -> Vec<&'static str> {
        let mut types: Vec<&'static str> = Vec::new();
        for pattern in Self::ALL {
            let facets = Facets::for_pattern(
                *pattern,
                &Specification::default(),
                &Details::new(),
                &Subject::columns(vec!["a".to_string(), "b".to_string()]),
            );
            if facets.derive_category(pattern.names_target()) == category
                && !types.contains(&pattern.subtype())
            {
                types.push(pattern.subtype());
            }
        }
        types
    }
}

/// One registered specification form: the form string, the finding
/// type it refines, and what the form states. The `form` field of a
/// specification was a free string; this registry is its vocabulary.
/// A finding whose form is not registered for its type fails
/// validation, and a test pins the set against the form strings the
/// detectors assign.
pub struct FormSpec {
    pub form: &'static str,
    pub finding_type: &'static str,
    pub definition: &'static str,
}

pub const FORMS: &[FormSpec] = &[
    FormSpec { form: "value_mapping", finding_type: "functional", definition: "Each combination of the inputs' values carries one target value, up to the recorded violations." },
    FormSpec { form: "affine", finding_type: "affine", definition: "target = a * input + b, fitted by least squares and holding within tolerance." },
    FormSpec { form: "sum", finding_type: "arithmetic_identity", definition: "The target equals the sum of the inputs." },
    FormSpec { form: "difference", finding_type: "arithmetic_identity", definition: "The target equals the inputs combined with unit coefficients and signs (a difference of terms)." },
    FormSpec { form: "product", finding_type: "arithmetic_identity", definition: "The target equals the product of the inputs." },
    FormSpec { form: "ratio", finding_type: "arithmetic_identity", definition: "The target equals the ratio of the inputs." },
    FormSpec { form: "linear_combination", finding_type: "arithmetic_identity", definition: "The target equals a linear combination of the inputs with the recorded coefficients, holding within tolerance." },
    FormSpec { form: "piecewise_linear", finding_type: "predictive", definition: "The dependence of the target on the input is fitted in segments by a piecewise-linear surrogate." },
    FormSpec { form: "interaction_h", finding_type: "predictive", definition: "The dependence is stated through the Friedman-Popescu interaction statistic H for the named pair of inputs." },
    FormSpec { form: "step_shape", finding_type: "predictive", definition: "The fitted dependence of the target on the input is a step function." },
    FormSpec { form: "curved_shape", finding_type: "predictive", definition: "The fitted dependence of the target on the input is a smooth curve." },
    FormSpec { form: "pairwise_order", finding_type: "order_constraint", definition: "One column is at most the other on every row, compared row by row." },
    FormSpec { form: "pairwise_order", finding_type: "order_violation", definition: "The rows where one column exceeds the other, compared row by row." },
    FormSpec { form: "period_split", finding_type: "period_association", definition: "The association is stated separately in two periods of the time axis." },
    FormSpec { form: "recursive_partition", finding_type: "ordered_partition", definition: "The ordered axis is split recursively where the target's level changes." },
    FormSpec { form: "regime_contrast", finding_type: "regime_contrast", definition: "The target's level in one regime of the ordered axis against another regime." },
    FormSpec { form: "spike_contrast", finding_type: "spike_contrast", definition: "The target's level in spike periods against the surrounding periods." },
    FormSpec { form: "monotone", finding_type: "axis_shape", definition: "The binned target rises (or falls) monotonically along the ordered axis." },
    FormSpec { form: "monotone_with_reversal_blocks", finding_type: "axis_shape", definition: "The binned target is monotone along the ordered axis apart from recorded reversal blocks." },
    FormSpec { form: "non_monotone_reversal", finding_type: "axis_shape", definition: "The binned target reverses direction along the ordered axis." },
    FormSpec { form: "saturating", finding_type: "axis_shape", definition: "The binned target rises along the ordered axis and then flattens, the shape confirmed on bootstrap resamples." },
    FormSpec { form: "rises_then_flattens", finding_type: "axis_shape", definition: "The binned target rises along the ordered axis and then flattens (the point-estimate shape, without bootstrap confirmation)." },
    FormSpec { form: "pivot", finding_type: "long_format", definition: "The frame is a pivot of a wider table: an attribute column's levels and a measure column's values." },
    FormSpec { form: "excursion", finding_type: "transient_excursion", definition: "A stretch of periods where the series leaves its typical band and returns." },
    FormSpec { form: "step", finding_type: "step_discontinuity", definition: "The series' level jumps at one point of the ordered axis and stays at the new level." },
    FormSpec { form: "drift", finding_type: "predictor_drift", definition: "A predictor's distribution in a recent window against its distribution in an earlier window." },
    FormSpec { form: "duplicate_encoding", finding_type: "duplicate_encoding", definition: "Two columns encode the same values under a value mapping." },
    FormSpec { form: "rollup", finding_type: "duplicate_encoding", definition: "One column is the rollup of another under a mapping (continents of countries, categories of items)." },
    FormSpec { form: "hierarchy", finding_type: "duplicate_encoding", definition: "The mapping is the declared hierarchy; the statement leads its verified family of mappings." },
    FormSpec { form: "isolation_gap", finding_type: "extreme_value", definition: "The value is separated from the rest of the column by a wide empty gap." },
    FormSpec { form: "tail_shift", finding_type: "slice_shift", definition: "The slice's rows sit in the target's upper tail at a higher rate than the table's rows." },
    FormSpec { form: "median_shift", finding_type: "slice_shift", definition: "The slice's median of the target against the table-wide median." },
    FormSpec { form: "level_share", finding_type: "slice_shift", definition: "The slice's share of one level of a categorical target against its share of rows." },
    FormSpec { form: "level_shares", finding_type: "outcome_distribution", definition: "The share of the designated outcome's rows at each of its levels, with the declared event's rate over the whole file and on the event basis when an event is declared." },
    FormSpec { form: "center_spread_range", finding_type: "outcome_distribution", definition: "The designated numeric outcome's center (mean and median), spread (the middle half), and range, in the outcome's own units." },
    FormSpec { form: "event_rate", finding_type: "outcome_contrast", definition: "One level's rate of the declared event against the pooled event rate, on the event basis (rows carrying the event or the reference level)." },
    FormSpec { form: "event_rate_null", finding_type: "outcome_contrast", definition: "A grouping measured against the declared event and found without a difference: every level's event rate beside the pooled rate, stated as a null." },
    FormSpec { form: "level_rate", finding_type: "outcome_contrast", definition: "One level's share of one level of the categorical outcome against that level's pooled share, stated when no event is declared." },
    FormSpec { form: "location", finding_type: "outcome_contrast", definition: "One level's median of the designated numeric outcome against the pooled median." },
    FormSpec { form: "location_null", finding_type: "outcome_contrast", definition: "A grouping measured against the designated numeric outcome and found without a difference: every level's median beside the pooled median, stated as a null." },
    FormSpec { form: "location", finding_type: "level_contrast", definition: "The level's median of the measure differs from the rest's median." },
    FormSpec { form: "scale", finding_type: "level_contrast", definition: "The level's interquartile range of the measure differs from the rest's." },
    FormSpec { form: "location_scale", finding_type: "level_contrast", definition: "The level's median and interquartile range of the measure both differ from the rest's." },
    FormSpec { form: "constant_multiple", finding_type: "level_contrast", definition: "The level's values sit at a near-constant multiple of the rest's distribution." },
    FormSpec { form: "profile", finding_type: "level_contrast", definition: "Several levels of one grouping differ from their pooled rest on one measure: the measure's profile over the grouping, stated once, with every tested level's median and tests in the evidence's levels table." },
    FormSpec { form: "bias", finding_type: "model_failure", definition: "The model's residuals in the slice are biased: their mean differs from zero beyond the stated bar." },
    FormSpec { form: "accuracy", finding_type: "model_failure", definition: "The model's error in the slice exceeds its table-wide error beyond the stated bar." },
    FormSpec { form: "absorbing_state", finding_type: "one_way_flag", definition: "The flag switches on within entities and never switches off." },
    FormSpec { form: "entry_block", finding_type: "entry_block", definition: "A block of entities first appears in one period of the panel." },
    FormSpec { form: "exit_block", finding_type: "exit_block", definition: "A block of entities last appears in one period of the panel." },
    FormSpec { form: "first_activity", finding_type: "first_activity", definition: "Entities first become active on the measure after their first observed period." },
    FormSpec { form: "per_period_top_share", finding_type: "concentration_over_time", definition: "The share of the measure held by the top entities in each period." },
    FormSpec { form: "entity_total_tail", finding_type: "outlier_entity", definition: "The entity's total of the measure sits in the tail of the entity totals." },
    FormSpec { form: "nominal_pair", finding_type: "statistical", definition: "The association of two categorical columns." },
    FormSpec { form: "mixed_pair", finding_type: "statistical", definition: "The association of a categorical column and a numeric column." },
    FormSpec { form: "reversal", finding_type: "conditioned_association", definition: "The pooled association reverses direction within every usable level of the stratifier." },
    FormSpec { form: "vanishing", finding_type: "conditioned_association", definition: "The pooled association is absent within the stratifier's levels." },
];

fn category_noun(category: Category) -> &'static str {
    match category {
        Category::Missingness => "missingness",
        Category::Concentration => "concentration",
        Category::ColumnRoles => "column role",
        Category::Uniqueness => "uniqueness",
        Category::Duplicates => "duplicates",
        Category::Formats => "format",
        Category::Dependencies => "dependency",
        Category::ConditionalRules => "conditional rule",
        Category::Associations => "association",
        Category::Gaps => "gap",
        Category::Anomalies => "anomaly",
        Category::Ordering => "ordering",
        Category::Changes => "change",
    }
}

fn join_types(types: &[&str]) -> String {
    match types {
        [] => String::new(),
        [only] => (*only).to_string(),
        [head @ .., last] => format!("{}, or {}", head.join(", "), last),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Column,
    Columns,
    Rows,
    Sequence,
    Frame,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Object {
    Distribution,
    Relation,
    Structure,
    TemporalBehaviour,
    Role,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportKind {
    Exact,
    Approximate,
    ModelBased,
    Inferred,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Conditionality {
    Unconditional,
    ValueConditioned,
    SegmentConditioned,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Temporality {
    Atemporal,
    Ordered,
    ChangeAcrossSegments,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityDimension {
    Completeness,
    Uniqueness,
    Validity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Regularity,
    Deviation,
}

/// The finding's properties, stated as fields: what the statement
/// ranges over, what kind of statement it is, how it is supported,
/// whether it carries a condition, the role of order, the data-quality
/// dimension when it is a quality statement, and whether it reports a
/// structure that holds or a departure from an expectation. The
/// category is derived from the facets by `derive_category`; the
/// design and the derivation table live in
/// docs/taxonomy-v1-design.md.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facets {
    pub scope: Scope,
    pub object: Object,
    pub support: SupportKind,
    pub conditionality: Conditionality,
    pub temporality: Temporality,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_dimension: Option<QualityDimension>,
    pub status: Status,
}
impl Facets {
    /// Placeholder for findings deserialized from pre-0.3 reports,
    /// which carry no facets. Validation rejects the placeholder
    /// combination for any stated category, so an old report is never
    /// silently treated as faceted.
    pub fn unspecified() -> Self {
        Self {
            scope: Scope::Column,
            object: Object::Distribution,
            support: SupportKind::Exact,
            conditionality: Conditionality::Unconditional,
            temporality: Temporality::Atemporal,
            quality_dimension: None,
            status: Status::Regularity,
        }
    }

    /// The category these facets derive, per the ordered table in
    /// docs/taxonomy-v1-design.md. `directed` states whether the
    /// relation names a target (the subject's target or the
    /// specification's outcome).
    pub fn derive_category(&self, directed: bool) -> Category {
        use Conditionality as C;
        use Object as O;
        use QualityDimension as Q;
        use Status as S;
        use SupportKind as E;
        use Temporality as T;
        match self.quality_dimension {
            Some(Q::Completeness) => {
                return if self.object == O::Distribution {
                    Category::Missingness
                } else {
                    Category::Gaps
                };
            }
            Some(Q::Uniqueness) => {
                return if self.status == S::Regularity {
                    Category::Uniqueness
                } else {
                    Category::Duplicates
                };
            }
            Some(Q::Validity) => return Category::Formats,
            None => {}
        }
        if self.object == O::Role {
            return Category::ColumnRoles;
        }
        if self.object == O::Relation && self.conditionality == C::ValueConditioned {
            return Category::ConditionalRules;
        }
        if self.temporality == T::ChangeAcrossSegments && self.object != O::Relation {
            return Category::Changes;
        }
        if self.object == O::Relation {
            if self.status == S::Deviation {
                return Category::Changes;
            }
            if directed {
                return Category::Dependencies;
            }
            return if self.support == E::Exact {
                Category::ColumnRoles
            } else {
                Category::Associations
            };
        }
        if self.object == O::TemporalBehaviour {
            return Category::Changes;
        }
        if self.object == O::Distribution {
            if self.temporality == T::Ordered || self.status == S::Deviation {
                return Category::Anomalies;
            }
            return Category::Concentration;
        }
        // Object::Structure
        if self.temporality == T::Ordered {
            Category::Ordering
        } else {
            Category::Gaps
        }
    }

    /// Facets for a built-in pattern, refined by the finding's own
    /// record where the record decides a field: violation counts for
    /// dependencies and rules, the subset condition for extreme
    /// values, the subject's width for uniqueness.
    pub fn for_pattern(
        pattern: Pattern,
        spec: &Specification,
        metrics: &Details,
        subject: &Subject,
    ) -> Self {
        use Conditionality as C;
        use Object as O;
        use QualityDimension as Q;
        use Scope as Sc;
        use Status as S;
        use SupportKind as E;
        use Temporality as T;
        let violations = metrics
            .get("violations")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let exact_or_approx = if violations == 0 {
            E::Exact
        } else {
            E::Approximate
        };
        let (scope, object, support, conditionality, temporality, quality, status) = match pattern {
            Pattern::MissingValues | Pattern::SentinelValues => (
                Sc::Column,
                O::Distribution,
                E::Exact,
                C::Unconditional,
                T::Atemporal,
                Some(Q::Completeness),
                S::Deviation,
            ),
            Pattern::PredictableMissingness => (
                Sc::Column,
                O::Distribution,
                E::ModelBased,
                C::Unconditional,
                T::Atemporal,
                Some(Q::Completeness),
                S::Regularity,
            ),
            Pattern::ValueConcentration | Pattern::ModalShare | Pattern::SpikeCells => (
                Sc::Column,
                O::Distribution,
                E::Exact,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::ConcentrationOverTime | Pattern::OutlierEntity => (
                Sc::Columns,
                O::Distribution,
                E::Exact,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::Hotspot => (
                Sc::Columns,
                O::Distribution,
                E::Approximate,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::CategoricalRole | Pattern::IdentifierRole => (
                Sc::Column,
                O::Role,
                E::Inferred,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::Uniqueness => (
                if subject.columns.len() > 1 {
                    Sc::Columns
                } else {
                    Sc::Column
                },
                O::Relation,
                E::Exact,
                C::Unconditional,
                T::Atemporal,
                Some(Q::Uniqueness),
                S::Regularity,
            ),
            Pattern::Duplicates | Pattern::NearDuplicateRows => (
                Sc::Rows,
                O::Structure,
                E::Exact,
                C::Unconditional,
                T::Atemporal,
                Some(Q::Uniqueness),
                S::Deviation,
            ),
            Pattern::FormatVariation => (
                Sc::Column,
                O::Distribution,
                E::Exact,
                C::Unconditional,
                T::Atemporal,
                Some(Q::Validity),
                S::Deviation,
            ),
            Pattern::Heaping | Pattern::DigitPreference | Pattern::BenfordDeviation => (
                Sc::Column,
                O::Distribution,
                E::Approximate,
                C::Unconditional,
                T::Atemporal,
                Some(Q::Validity),
                S::Deviation,
            ),
            Pattern::RowInconsistency => (
                Sc::Columns,
                O::Distribution,
                E::Exact,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Deviation,
            ),
            Pattern::SeriesStructure => (
                Sc::Column,
                O::TemporalBehaviour,
                E::Approximate,
                C::Unconditional,
                T::Ordered,
                None,
                S::Regularity,
            ),
            Pattern::OneWayFlag => (
                Sc::Columns,
                O::TemporalBehaviour,
                E::Exact,
                C::Unconditional,
                T::Ordered,
                None,
                S::Regularity,
            ),
            Pattern::EntryBlock | Pattern::ExitBlock | Pattern::FirstActivity => (
                Sc::Columns,
                O::TemporalBehaviour,
                E::Exact,
                C::Unconditional,
                T::Ordered,
                None,
                S::Regularity,
            ),
            Pattern::BoundaryMass | Pattern::ContentAnomaly => (
                Sc::Column,
                O::Distribution,
                E::Exact,
                C::Unconditional,
                T::Atemporal,
                Some(Q::Validity),
                S::Deviation,
            ),
            Pattern::FunctionalDependency => (
                Sc::Columns,
                O::Relation,
                exact_or_approx,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::OrderConstraint | Pattern::OrderViolation => (
                Sc::Columns,
                O::Relation,
                exact_or_approx,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::AffineDependency | Pattern::ArithmeticIdentity => (
                Sc::Columns,
                O::Relation,
                E::Approximate,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::PredictiveDependency => (
                Sc::Columns,
                O::Relation,
                E::ModelBased,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::LeakageSignature => (
                Sc::Columns,
                O::Relation,
                E::Approximate,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::PiecewiseDependency => (
                Sc::Columns,
                O::Relation,
                E::ModelBased,
                C::SegmentConditioned,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::ModelFailure => (
                Sc::Columns,
                O::Relation,
                E::ModelBased,
                C::SegmentConditioned,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::ConditionalRule => (
                Sc::Columns,
                O::Relation,
                exact_or_approx,
                C::ValueConditioned,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::StatisticalAssociation => (
                Sc::Columns,
                O::Relation,
                E::Approximate,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::PeriodAssociation => (
                Sc::Columns,
                O::Relation,
                E::Approximate,
                C::SegmentConditioned,
                T::ChangeAcrossSegments,
                None,
                S::Regularity,
            ),
            Pattern::GroupContrast => (
                Sc::Columns,
                O::Relation,
                E::Approximate,
                C::SegmentConditioned,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::ConditionedAssociation | Pattern::SliceShift => (
                Sc::Columns,
                O::Relation,
                E::Approximate,
                C::SegmentConditioned,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::LevelContrast => (
                Sc::Columns,
                O::Relation,
                E::Approximate,
                C::SegmentConditioned,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::OutcomeDistribution => (
                Sc::Column,
                O::Distribution,
                E::Exact,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::OutcomeContrast => (
                Sc::Columns,
                O::Relation,
                E::Approximate,
                C::SegmentConditioned,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::Gap | Pattern::SequenceGap => (
                Sc::Sequence,
                O::Structure,
                E::Exact,
                C::Unconditional,
                T::Ordered,
                Some(Q::Completeness),
                S::Deviation,
            ),
            Pattern::MissingCombinations => (
                Sc::Columns,
                O::Structure,
                E::Exact,
                C::Unconditional,
                T::Atemporal,
                Some(Q::Completeness),
                S::Deviation,
            ),
            Pattern::PartialPeriod => (
                Sc::Sequence,
                O::Structure,
                E::Exact,
                C::Unconditional,
                T::Ordered,
                Some(Q::Completeness),
                S::Deviation,
            ),
            Pattern::IntervalVariation => (
                Sc::Sequence,
                O::Distribution,
                E::Exact,
                C::Unconditional,
                T::Ordered,
                None,
                S::Deviation,
            ),
            Pattern::ColumnOrdering => (
                Sc::Column,
                O::Structure,
                E::Exact,
                C::Unconditional,
                T::Ordered,
                None,
                S::Regularity,
            ),
            Pattern::ResidualShift
            | Pattern::LevelShift
            | Pattern::TrendChange
            | Pattern::VarianceChange => (
                Sc::Sequence,
                O::TemporalBehaviour,
                E::ModelBased,
                C::Unconditional,
                T::ChangeAcrossSegments,
                None,
                S::Regularity,
            ),
            Pattern::LongFormat => (
                Sc::Frame,
                O::Role,
                E::Inferred,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::ExtremeValue => (
                Sc::Column,
                O::Distribution,
                E::Exact,
                if spec.definition.contains_key("condition_column") {
                    C::ValueConditioned
                } else {
                    C::Unconditional
                },
                T::Atemporal,
                None,
                S::Deviation,
            ),
            Pattern::CategoricalOutlier => (
                Sc::Columns,
                O::Distribution,
                E::Approximate,
                C::ValueConditioned,
                T::Atemporal,
                None,
                S::Deviation,
            ),
            Pattern::TransientExcursion => (
                Sc::Sequence,
                O::TemporalBehaviour,
                E::Approximate,
                C::Unconditional,
                T::ChangeAcrossSegments,
                None,
                S::Deviation,
            ),
            Pattern::StepDiscontinuity => (
                Sc::Columns,
                O::Relation,
                E::Approximate,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Deviation,
            ),
            Pattern::DuplicateEncoding => (
                Sc::Columns,
                O::Relation,
                E::Exact,
                C::Unconditional,
                T::Atemporal,
                None,
                S::Regularity,
            ),
            Pattern::DistributionShift => (
                Sc::Columns,
                O::Distribution,
                E::ModelBased,
                C::Unconditional,
                T::ChangeAcrossSegments,
                None,
                S::Regularity,
            ),
            Pattern::DistributionChange => (
                Sc::Sequence,
                O::Distribution,
                E::ModelBased,
                C::Unconditional,
                T::ChangeAcrossSegments,
                None,
                S::Regularity,
            ),
            Pattern::OrderedPartition => (
                Sc::Sequence,
                O::TemporalBehaviour,
                E::ModelBased,
                C::SegmentConditioned,
                T::ChangeAcrossSegments,
                None,
                S::Regularity,
            ),
            Pattern::SlopeBreak => (
                Sc::Sequence,
                O::TemporalBehaviour,
                E::ModelBased,
                C::Unconditional,
                T::ChangeAcrossSegments,
                None,
                S::Regularity,
            ),
            Pattern::RegimeContrast => (
                Sc::Sequence,
                O::TemporalBehaviour,
                E::ModelBased,
                C::Unconditional,
                T::ChangeAcrossSegments,
                None,
                S::Regularity,
            ),
            Pattern::SpikeContrast => (
                Sc::Sequence,
                O::TemporalBehaviour,
                E::Approximate,
                C::Unconditional,
                T::ChangeAcrossSegments,
                None,
                S::Deviation,
            ),
            Pattern::AxisShape => (
                Sc::Columns,
                O::Relation,
                E::Approximate,
                C::Unconditional,
                T::Ordered,
                None,
                S::Regularity,
            ),
        };
        Self {
            scope,
            object,
            support,
            conditionality,
            temporality,
            quality_dimension: quality,
            status,
        }
    }

    /// Facets for a finding stated through `from_statement`, mapped
    /// from its category and type (and the specification's form where
    /// the type covers several patterns). The stated category is
    /// checked against the derivation in `validate`.
    pub fn for_statement(
        category: Category,
        finding_type: Option<&str>,
        spec: &Specification,
        metrics: &Details,
        subject: &Subject,
    ) -> Self {
        let pattern = match (category, finding_type) {
            (Category::Missingness, Some("sentinel_values")) => Pattern::SentinelValues,
            (Category::Missingness, Some("predictable_missingness")) => {
                Pattern::PredictableMissingness
            }
            (Category::Missingness, _) => Pattern::MissingValues,
            (Category::Concentration, Some("hotspot")) => Pattern::Hotspot,
            (Category::Concentration, Some("concentration_over_time")) => {
                Pattern::ConcentrationOverTime
            }
            (Category::Concentration, Some("outlier_entity")) => Pattern::OutlierEntity,
            (Category::Concentration, Some("outcome_distribution")) => Pattern::OutcomeDistribution,
            (Category::Concentration, Some("modal_share")) => Pattern::ModalShare,
            (Category::Concentration, Some("spike_cells")) => Pattern::SpikeCells,
            (Category::Concentration, _) => Pattern::ValueConcentration,
            (Category::ColumnRoles, Some("identifier_role")) => Pattern::IdentifierRole,
            (Category::ColumnRoles, Some("long_format")) => Pattern::LongFormat,
            (Category::ColumnRoles, Some("duplicate_encoding")) => Pattern::DuplicateEncoding,
            (Category::ColumnRoles, _) => Pattern::CategoricalRole,
            (Category::Uniqueness, _) => Pattern::Uniqueness,
            (Category::Duplicates, Some("near_duplicate_rows")) => Pattern::NearDuplicateRows,
            (Category::Duplicates, _) => Pattern::Duplicates,
            (Category::Formats, Some("heaping")) => Pattern::Heaping,
            (Category::Formats, Some("digit_preference")) => Pattern::DigitPreference,
            (Category::Formats, Some("benford_deviation")) => Pattern::BenfordDeviation,
            (Category::Formats, Some("boundary_mass")) => Pattern::BoundaryMass,
            (Category::Formats, Some("content_anomaly")) => Pattern::ContentAnomaly,
            (Category::Formats, _) => Pattern::FormatVariation,
            (Category::Dependencies, Some("order_constraint")) => Pattern::OrderConstraint,
            (Category::Dependencies, Some("order_violation")) => Pattern::OrderViolation,
            (Category::Dependencies, Some("model_failure")) => Pattern::ModelFailure,
            (Category::Dependencies, Some("leakage_signature")) => Pattern::LeakageSignature,
            (Category::Dependencies, Some("outcome_contrast")) => Pattern::OutcomeContrast,
            (Category::Dependencies, Some("predictive")) => {
                if spec.form.as_deref() == Some("piecewise_linear") {
                    Pattern::PiecewiseDependency
                } else {
                    Pattern::PredictiveDependency
                }
            }
            (Category::Dependencies, Some("affine")) => Pattern::AffineDependency,
            (Category::Dependencies, Some("arithmetic_identity")) => Pattern::ArithmeticIdentity,
            (Category::Dependencies, _) => Pattern::FunctionalDependency,
            (Category::ConditionalRules, _) => Pattern::ConditionalRule,
            (Category::Associations, Some("period_association")) => Pattern::PeriodAssociation,
            (Category::Associations, Some("group_contrast")) => Pattern::GroupContrast,
            (Category::Associations, Some("conditioned_association")) => {
                Pattern::ConditionedAssociation
            }
            (Category::Associations, Some("level_contrast")) => Pattern::LevelContrast,
            (Category::Associations, Some("slice_shift")) => Pattern::SliceShift,
            (Category::Associations, Some("axis_shape")) => Pattern::AxisShape,
            (Category::Associations, _) => Pattern::StatisticalAssociation,
            (Category::Gaps, Some("sequence_gap")) => Pattern::SequenceGap,
            (Category::Gaps, Some("missing_combinations")) => Pattern::MissingCombinations,
            (Category::Gaps, Some("partial_period")) => Pattern::PartialPeriod,
            (Category::Gaps, _) => Pattern::Gap,
            (Category::Anomalies, Some("row_inconsistency")) => Pattern::RowInconsistency,
            (Category::Anomalies, Some("extreme_value")) => Pattern::ExtremeValue,
            (Category::Anomalies, Some("categorical_outlier")) => Pattern::CategoricalOutlier,
            (Category::Anomalies, _) => Pattern::IntervalVariation,
            (Category::Ordering, _) => Pattern::ColumnOrdering,
            (Category::Changes, Some("series_structure")) => Pattern::SeriesStructure,
            (Category::Changes, Some("level_shift")) => Pattern::LevelShift,
            (Category::Changes, Some("trend_change")) => Pattern::TrendChange,
            (Category::Changes, Some("variance_change")) => Pattern::VarianceChange,
            (Category::Changes, Some("transient_excursion")) => Pattern::TransientExcursion,
            (Category::Changes, Some("step_discontinuity")) => Pattern::StepDiscontinuity,
            (Category::Changes, Some("predictor_drift")) => Pattern::DistributionShift,
            (Category::Changes, Some("joint_distribution_change")) => Pattern::DistributionChange,
            (Category::Changes, Some("slope_break")) => Pattern::SlopeBreak,
            (Category::Changes, Some("regime_contrast")) => Pattern::RegimeContrast,
            (Category::Changes, Some("spike_contrast")) => Pattern::SpikeContrast,
            (Category::Changes, Some("ordered_partition")) => Pattern::OrderedPartition,
            (Category::Changes, Some("one_way_flag")) => Pattern::OneWayFlag,
            (Category::Changes, Some("entry_block")) => Pattern::EntryBlock,
            (Category::Changes, Some("exit_block")) => Pattern::ExitBlock,
            (Category::Changes, Some("first_activity")) => Pattern::FirstActivity,
            (Category::Changes, _) => Pattern::ResidualShift,
        };
        Self::for_pattern(pattern, spec, metrics, subject)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub category: Category,
    #[serde(default = "Facets::unspecified")]
    pub facets: Facets,
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
    /// Ids of the statements the ledger grouped under this finding
    /// (schema 0.6): this finding is the lead of a verified
    /// relationship family, and the named statements — full records
    /// in the report's `grouped_findings` — restate the same exact
    /// relation it states. Empty for non-leads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grouped_members: Vec<String>,
    /// The lead finding's id, when the ledger grouped this
    /// statement under a verified relationship family (schema 0.6).
    /// Grouped statements live in the report's `grouped_findings`,
    /// not in `findings`: they spend no digest slot and no
    /// selection slot, and stay retrievable by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grouped_under: Option<String>,
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
        let dependency = pattern.names_target();
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
            Pattern::OrderConstraint | Pattern::OrderViolation => {
                spec.form = Some("pairwise_order".into());
                spec.definition.insert(
                    "missing_values".into(),
                    json!("exclude rows where either column is missing"),
                );
                spec.definition
                    .insert("comparison".into(), json!("exact_order"));
                if let Some(v) = metrics.get("relation") {
                    spec.definition.insert("relation".into(), v.clone());
                }
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
            Pattern::PartialPeriod => {
                subject.unit = Some("periods".into());
                spec.definition.insert("domain".into(), json!("temporal"));
                spec.definition.insert(
                    "reference".into(),
                    json!({"source": "observed_periods", "typical": "median of the observed periods"}),
                );
                if let Some(v) = metrics.get("period") {
                    spec.definition.insert("from".into(), v.clone());
                    spec.definition.insert("to".into(), v.clone());
                }
            }
            Pattern::OneWayFlag => {
                spec.form = Some("absorbing_state".into());
                spec.definition.insert(
                    "missing_values".into(),
                    json!("rows missing the entity, flag, or time value are not observations"),
                );
                if let Some(v) = metrics.get("entity_column") {
                    spec.definition.insert("entity".into(), v.clone());
                }
            }
            Pattern::EntryBlock | Pattern::ExitBlock => {
                spec.form = Some(if pattern == Pattern::EntryBlock {
                    "entry_block".into()
                } else {
                    "exit_block".into()
                });
                spec.definition.insert(
                    "missing_values".into(),
                    json!("rows missing the entity or time value are not observations"),
                );
                if let Some(v) = metrics.get("entity_column") {
                    spec.definition.insert("entity".into(), v.clone());
                }
                if let Some(v) = metrics.get("period") {
                    spec.definition.insert("period".into(), v.clone());
                }
            }
            Pattern::FirstActivity => {
                spec.form = Some("first_activity".into());
                spec.definition.insert(
                    "missing_values".into(),
                    json!("rows missing the entity or time value are not observations; a missing measure value is not activity"),
                );
                if let Some(v) = metrics.get("entity_column") {
                    spec.definition.insert("entity".into(), v.clone());
                }
                if let Some(v) = metrics.get("activity_column") {
                    spec.definition.insert("activity".into(), v.clone());
                }
            }
            Pattern::ConcentrationOverTime => {
                spec.form = Some("per_period_top_share".into());
                spec.definition.insert(
                    "missing_values".into(),
                    json!("rows missing the entity, measure, or time value are not observations"),
                );
                if let Some(v) = metrics.get("entity_column") {
                    spec.definition.insert("entity".into(), v.clone());
                }
                spec.definition.insert(
                    "top_group".into(),
                    json!("the largest entities holding the top share of each period's total: ceil(1% of the entities), at least one"),
                );
            }
            Pattern::OutlierEntity => {
                spec.form = Some("entity_total_tail".into());
                spec.definition.insert(
                    "missing_values".into(),
                    json!("rows missing the entity, measure, or time value are not observations"),
                );
                if let Some(v) = metrics.get("entity_column") {
                    spec.definition.insert("entity".into(), v.clone());
                }
                spec.definition.insert(
                    "aggregation".into(),
                    json!("the measure summed over all periods, per entity"),
                );
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
            Pattern::OrderedPartition => {
                spec.form = Some("recursive_partition".into());
                if let Some(oc) = string("time_column") {
                    spec.definition.insert("ordering_column".into(), json!(oc));
                }
                if let Some(m) = string("model") {
                    spec.definition.insert("model".into(), json!(m));
                }
                spec.definition.insert("statistic".into(), json!("split_f"));
                spec.definition.insert(
                    "missing_values".into(),
                    json!("rows without an ordering value, or with a missing or non-finite response, are excluded"),
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
            Pattern::LevelShift
            | Pattern::TrendChange
            | Pattern::VarianceChange
            | Pattern::DistributionChange
            | Pattern::SlopeBreak => {
                if let Some(v) = metrics.get("breakpoint") {
                    spec.definition.insert("breakpoint".into(), v.clone());
                }
            }
            Pattern::RegimeContrast => {
                spec.form = Some("regime_contrast".into());
                for k in ["window_start", "window_end"] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
            }
            Pattern::SpikeContrast => {
                spec.form = Some("spike_contrast".into());
                for k in ["window_start", "window_end"] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
            }
            Pattern::AxisShape => {
                if let Some(f) = metrics.get("shape_form").and_then(|v| v.as_str()) {
                    spec.form = Some(f.to_string());
                }
                if let Some(v) = metrics.get("axis") {
                    spec.definition.insert("axis".into(), v.clone());
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
            Pattern::RowInconsistency | Pattern::NearDuplicateRows => {
                if pattern == Pattern::NearDuplicateRows {
                    subject.unit = Some("rows".into());
                    spec.definition
                        .insert("comparison".into(), json!("equal on all but one column"));
                }
                for k in ["kind", "varying_column"] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
            }
            Pattern::SeriesStructure => {
                // One column can hold several statements: a trend and a
                // cycle, or several levels of a grouping column. The
                // per-entity statements are keyed by their grain and
                // direction as well, so a rise carried by one entity
                // column never merges with a fall on another.
                for k in ["kind", "value", "period", "entity_column", "direction"] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
            }
            Pattern::ContentAnomaly => {
                // One column can hold several statements of different
                // kinds, and of one kind at both ends of its range.
                for k in ["kind", "value", "side", "extreme"] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
            }
            Pattern::BoundaryMass => {
                // The statement is about one piled value: naming it
                // in the definition keeps each pile its own
                // statement (the ledger merges findings that share
                // a canonical key).
                for k in ["value", "boundary"] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
                spec.definition.insert(
                    "comparison".into(),
                    json!("exact value counts against the median count of the nearest distinct values"),
                );
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
            Pattern::SliceShift => {
                if let Some(v) = metrics.get("form").and_then(Value::as_str) {
                    spec.form = Some(v.to_owned());
                }
                if let Some(conds) = raw.get("conditions").and_then(Value::as_array) {
                    for c in conds {
                        let (Some(column), Some(op), Some(value)) = (
                            c.get("column").and_then(Value::as_str),
                            c.get("op").and_then(Value::as_str),
                            c.get("value"),
                        ) else {
                            continue;
                        };
                        let operator = match op {
                            "eq" => Operator::Eq,
                            "gt" => Operator::Gt,
                            _ => continue,
                        };
                        let mut p = Predicate {
                            column: column.to_owned(),
                            operator,
                            value: value.clone(),
                            transform: None,
                        };
                        p.canonicalize();
                        spec.conditions.push(p);
                    }
                    spec.conditions
                        .sort_by_key(|p| serde_json::to_string(p).unwrap());
                }
                if let Some(v) = metrics.get("target_level") {
                    spec.definition.insert("target_level".into(), v.clone());
                }
                spec.definition.insert(
                    "missing_values".into(),
                    json!("rows missing a condition value are outside the slice; rows missing the target count as not extreme and not positive"),
                );
            }
            Pattern::LevelContrast => {
                // The level is part of the statement's identity:
                // naming it in the definition keeps each level's
                // contrast its own statement (the ledger merges
                // findings that share a canonical key).
                if let Some(f) = metrics.get("form").and_then(Value::as_str) {
                    spec.form = Some(f.to_owned());
                }
                for k in ["level", "calendar_part", "axis"] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
                spec.definition.insert(
                    "missing_values".into(),
                    json!("rows missing the grouping value or the measure are excluded from that pair's comparison"),
                );
            }
            Pattern::OutcomeDistribution => {
                if let Some(f) = metrics.get("form").and_then(Value::as_str) {
                    spec.form = Some(f.to_owned());
                }
                if let Some(v) = metrics.get("event") {
                    spec.definition.insert("event".into(), v.clone());
                }
                spec.definition.insert(
                    "missing_values".into(),
                    json!("rows missing the outcome are excluded and counted in the statement"),
                );
            }
            Pattern::OutcomeContrast => {
                // The level is part of the statement's identity,
                // as with the level contrasts: each level's rate
                // against the outcome is its own statement, and a
                // grouping's null names the grouping.
                if let Some(f) = metrics.get("form").and_then(Value::as_str) {
                    spec.form = Some(f.to_owned());
                }
                for k in ["level", "grouping_column", "event"] {
                    if let Some(v) = metrics.get(k) {
                        spec.definition.insert(k.into(), v.clone());
                    }
                }
                spec.definition.insert(
                    "missing_values".into(),
                    json!("rows missing the grouping value or the outcome are excluded from that grouping's comparison"),
                );
            }
            Pattern::ModelFailure => {
                if let Some(v) = metrics.get("form").and_then(Value::as_str) {
                    spec.form = Some(v.to_owned());
                }
                if let Some(conds) = raw.get("conditions").and_then(Value::as_array) {
                    for c in conds {
                        let (Some(column), Some(op), Some(value)) = (
                            c.get("column").and_then(Value::as_str),
                            c.get("op").and_then(Value::as_str),
                            c.get("value"),
                        ) else {
                            continue;
                        };
                        let operator = match op {
                            "eq" => Operator::Eq,
                            "gt" => Operator::Gt,
                            _ => continue,
                        };
                        let mut p = Predicate {
                            column: column.to_owned(),
                            operator,
                            value: value.clone(),
                            transform: None,
                        };
                        p.canonicalize();
                        spec.conditions.push(p);
                    }
                    spec.conditions
                        .sort_by_key(|p| serde_json::to_string(p).unwrap());
                }
                spec.definition.insert(
                    "missing_values".into(),
                    json!("rows missing the target are excluded from fitting and from the search; rows missing a condition value are outside the slice"),
                );
            }
            _ => {}
        }
        let basis = if matches!(pattern, Pattern::CategoricalRole | Pattern::IdentifierRole) {
            EvidenceBasis::Inferred
        } else {
            basis
        };
        let evidence = Evidence::new(method, basis, metrics);
        let facets = Facets::for_pattern(pattern, &spec, &evidence.metrics, &subject);
        let directed = subject.target.is_some() || spec.outcome.is_some();
        Self::from_statement(
            facets.derive_category(directed),
            Some(pattern.subtype().to_owned()),
            subject,
            spec,
            claim,
            evidence,
            score,
            interpretation,
        )
        .with_facets(facets)
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
        let facets = Facets::for_statement(
            category,
            finding_type.as_deref(),
            &specification,
            &evidence.metrics,
            &subject,
        );
        Self {
            id: String::new(),
            category,
            facets,
            finding_type,
            subject,
            specification,
            claim,
            evidence: vec![evidence],
            interestingness: score.clamp(0.0, 1.0),
            detector_interestingness: None,
            grouped_members: Vec::new(),
            grouped_under: None,
            interpretation,
        }
    }
    pub fn with_facets(mut self, facets: Facets) -> Self {
        self.facets = facets;
        self
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
        let directed = self.subject.target.is_some() || self.specification.outcome.is_some();
        if self.facets.derive_category(directed) != self.category {
            return Err("the finding's facets derive a different category than it states".into());
        }
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
        let allowed = Pattern::types_for(self.category);
        if !allowed.contains(&self.finding_type.as_deref().unwrap()) {
            return Err(format!(
                "{} type must be {}",
                category_noun(self.category),
                join_types(&allowed)
            ));
        }
        if let Some(form) = self.specification.form.as_deref() {
            let finding_type = self.finding_type.as_deref().unwrap();
            if !FORMS
                .iter()
                .any(|f| f.form == form && f.finding_type == finding_type)
            {
                return Err(format!(
                    "form {form} is not registered for type {finding_type}"
                ));
            }
        }
        match self.category {
            Category::Dependencies => {
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

/// The taxonomy as JSON: every pattern with its type, its derived
/// category, and its facets, plus the registered forms and the
/// facet vocabularies. The facets are the canonical evaluation
/// `types_for` uses (a two-column subject, an empty specification,
/// no metrics). Three evaluations vary with the statement, and an
/// entry carries the varied value beside the canonical one when it
/// differs: `support_with_violations` (a violations metric
/// present), `scope_one_column` (a one-column subject), and
/// `conditionality_conditioned` (a specification declaring a
/// condition column). This is the export the findings-document
/// generator renders from, so the published taxonomy tables cannot
/// drift from the lists validation enforces.
pub fn taxonomy_json() -> String {
    let spec = Specification::default();
    let details = Details::new();
    let two_columns = Subject::columns(vec!["a".to_string(), "b".to_string()]);
    let one_column = Subject::columns(vec!["a".to_string()]);
    let mut violations = Details::new();
    violations.insert("violations".to_string(), json!(1));
    let mut conditioned_spec = Specification::default();
    conditioned_spec
        .definition
        .insert("condition_column".to_string(), json!("c"));

    let patterns: Vec<Value> = Pattern::ALL
        .iter()
        .map(|pattern| {
            let facets = Facets::for_pattern(*pattern, &spec, &details, &two_columns);
            let mut entry = json!({
                "pattern": format!("{pattern:?}"),
                "type": pattern.subtype(),
                "category": facets.derive_category(pattern.names_target()).as_str(),
                "facets": facets,
            });
            let with_violations = Facets::for_pattern(*pattern, &spec, &violations, &two_columns);
            if with_violations.support != facets.support {
                entry["support_with_violations"] = json!(with_violations.support);
            }
            let one = Facets::for_pattern(*pattern, &spec, &details, &one_column);
            if one.scope != facets.scope {
                entry["scope_one_column"] = json!(one.scope);
            }
            let conditioned =
                Facets::for_pattern(*pattern, &conditioned_spec, &details, &two_columns);
            if conditioned.conditionality != facets.conditionality {
                entry["conditionality_conditioned"] = json!(conditioned.conditionality);
            }
            entry
        })
        .collect();
    let forms: Vec<Value> = FORMS
        .iter()
        .map(|spec| {
            json!({
                "form": spec.form,
                "type": spec.finding_type,
                "definition": spec.definition,
            })
        })
        .collect();
    let facet_values = json!({
        "scope": [Scope::Column, Scope::Columns, Scope::Rows, Scope::Sequence, Scope::Frame],
        "object": [Object::Distribution, Object::Relation, Object::Structure, Object::TemporalBehaviour, Object::Role],
        "support": [SupportKind::Exact, SupportKind::Approximate, SupportKind::ModelBased, SupportKind::Inferred],
        "conditionality": [Conditionality::Unconditional, Conditionality::ValueConditioned, Conditionality::SegmentConditioned],
        "temporality": [Temporality::Atemporal, Temporality::Ordered, Temporality::ChangeAcrossSegments],
        "quality_dimension": [QualityDimension::Completeness, QualityDimension::Uniqueness, QualityDimension::Validity],
        "status": [Status::Regularity, Status::Deviation],
    });
    json!({
        "patterns": patterns,
        "forms": forms,
        "facet_values": facet_values,
    })
    .to_string()
}
