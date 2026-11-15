//! Detector registration is separate from the public finding taxonomy.
use crate::ProfileConfig;
use onceover_core::TypedTable;
use onceover_mines::EncodedMatrix;
use onceover_report::{Category, ColumnCard, Finding};
use std::collections::{BTreeMap, BTreeSet};

/// How a detector joins a run. The default set is the detectors an
/// agent gets from one unadorned `profile()` call; the 2026-11-02
/// detector-effectiveness battery (docs/benchmarks/2026-11-02-detector-
/// opt-in.md) is the evidence base for which detectors hold which
/// activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Activation {
    /// Runs in the default set.
    Default,
    /// Registered and runnable, but only when explicitly selected via
    /// `detectors=[...]`. Opt-in detectors record a skipped coverage
    /// entry in default runs, so their absence is never silent.
    OptIn,
    /// In the default set, but engages only when the caller declares
    /// the structure it checks (profile hints); without hints it
    /// records a skipped coverage entry.
    HintGated,
    /// Runs only when a target is designated (or when explicitly
    /// selected together with a target).
    TargetGated,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct DetectorDescriptor {
    pub id: String,
    pub categories: Vec<Category>,
    pub requires: Vec<String>,
    pub activation: Activation,
    /// What the detector looks for, in one line. The registry is
    /// the single source for this text: the capability manifest
    /// and the book's registry table render it from here.
    pub purpose: String,
}
impl DetectorDescriptor {
    /// Extension detectors default to `Activation::Default`; the
    /// built-in registry overrides the activation per detector.
    pub fn new(id: &str, categories: Vec<Category>, requires: Vec<String>) -> Self {
        Self {
            id: id.into(),
            categories,
            requires,
            activation: Activation::Default,
            purpose: String::new(),
        }
    }

    pub fn with_activation(mut self, activation: Activation) -> Self {
        self.activation = activation;
        self
    }

    pub fn with_purpose(mut self, purpose: &str) -> Self {
        self.purpose = purpose.into();
        self
    }
}

pub fn builtin_detectors() -> Vec<DetectorDescriptor> {
    use Activation::*;
    use Category::*;
    let make = |id: &str,
                categories: Vec<Category>,
                requires: &[&str],
                activation: Activation,
                purpose: &str| {
        DetectorDescriptor::new(
            id,
            categories,
            requires.iter().map(|s| s.to_string()).collect(),
        )
        .with_activation(activation)
        .with_purpose(purpose)
    };
    vec![
        make(
            "column_profile",
            vec![Missingness, Concentration, ColumnRoles, Formats],
            &[],
            Default,
        "Column summaries, role suggestions, missingness, concentration, string formats"),
        make("row_duplicates", vec![Duplicates], &[], Default, "Repeated rows under the stated comparison and missing-value policy"),
        make("keys", vec![Uniqueness], &[], Default, "Single-column, composite, and nearly unique keys"),
        // Opt-in since the 2026-11-02 battery: zero findings in 40
        // measured runs (20 datasets x harness and default configs).
        // The detector is unchanged and runs when explicitly selected.
        make("pivot", vec![ColumnRoles], &[], OptIn, "Verified long layout and relationships in its wide form"),
        make("functional_dependencies", vec![Dependencies], &[], Default, "Exact or approximate value mappings, including multi-column determinants"),
        make("order_constraints", vec![Dependencies], &[], Default, "Pairwise order between two columns (a <= b), exact or with counted violations"),
        make("duplicate_encodings", vec![ColumnRoles], &[], Default, "Alternative labels for a grouping and finer-to-coarser rollups"),
        make("sortedness", vec![Ordering], &[], Default, "Order in the row sequence and late appended backfills"),
        // Opt-in since the 2026-11-02 battery: zero findings in 40
        // measured runs at first measurement.
        make("granularity", vec![Formats], &[], OptIn, "Changes between fractional and integer-valued recording"),
        make("sentinels", vec![Missingness], &[], Default, "Repeated, windowed candidate missing-value codes"),
        make("boundaries", vec![Formats], &[], Default, "Mass piled at a numeric column's minimum or maximum, and interior mass points in zero-inflated columns"),
        make("missingness", vec![Missingness], &[], Default, "Each column's missing rate, the grouping level, threshold, or time point it follows, and columns missing on the same rows"),
        make("content", vec![Formats], &[], Default, "What text columns hold (numbers, dates, recognized formats), label variants and likely misspellings, repeated extremes, stray signs, zero inflation, separated modes, and far-out dates"),
        make("series", vec![Gaps, Changes], &[], Default, "Absent steps on the time axis; trend, cycle and persistence of each measure; grouping levels that appear, end or shift share over time"),
        make(
            "row_consistency",
            vec![Duplicates, Intervals],
            &["numeric_dependence"],
            Default,
        "Rows repeating another on every column but one, rows far from the line two tight columns follow, and rows with two columns' values swapped"),
        make("digits", vec![Formats], &[], Default, "Heaping, terminal-digit preferences, and eligible Benford deviations"),
        // Opt-in since the 2026-11-02 battery: zero findings in 40
        // measured runs across two batteries (2026-10-29, 2026-11-02).
        make("dates", vec![Gaps, Intervals], &[], OptIn, "Inferred calendar gaps and interval irregularity"),
        // Hint-gated: a default-set member that engages only on
        // declared hints and otherwise records itself skipped.
        make("expectations", vec![Gaps, Ordering], &[], HintGated, "Declared sequences, calendars, ordering, and panel completeness"),
        // Per-period coverage on the frame's time axis: observed
        // periods whose row, entity, or principal-measure volume
        // falls far below the typical period's, stated with their
        // position.
        make("panel_coverage", vec![Gaps], &[], Default, "Per-period row and entity volume on the time axis, against the typical period"),
        // Hint-gated like expectations: engages only when the
        // caller declares a group contrast in the hints.
        make("group_contrast", vec![Associations], &[], HintGated, "How one declared subset of the rows differs from the rest of the table"),
        // The automatic per-level scan (issue #275): every level
        // of every low-cardinality categorical against the pooled
        // rest on every numeric measure — location, scale, and
        // the constant-multiple form — with rank-sum tests under
        // one Benjamini-Hochberg family. The declared group
        // contrast's undeclared counterpart. Its scans are exact
        // quantile work in the breaks cost class, so it runs in
        // the default set like the mixed-pair screen whose
        // scale/shape blindness it covers.
        make("level_contrasts", vec![Associations], &[], Default, "Each level of a categorical column against the pooled rest on each numeric measure: location, scale, and constant-multiple contrasts, plus the date axis's calendar parts"),
        make("gbm", vec![Dependencies], &[], Default, "Predictability of eligible columns from the other columns"),
        make("decision_tables", vec![Dependencies], &["gbm"], Default, "Compact predictive tables selected by cross-validation"),
        make("prism", vec![ConditionalRules], &["gbm"], Default, "Multi-condition rules for categorical targets"),
        make("regimes", vec![Dependencies, Changes], &["gbm"], Default, "Local linear equations and dated reference-model residual shifts"),
        make("interactions", vec![Dependencies], &["gbm"], Default, "Non-additive predictor pairs under a fitted target model"),
        make("target_shapes", vec![Dependencies], &[], TargetGated, "Line, curve, and step comparisons for a designated numeric target"),
        make("leakage", vec![Dependencies], &[], TargetGated, "A column that determines the designated target exactly, or relates to it far more strongly than any other column does — the signature of a column derived from the outcome or recorded after it"),
        make("model_failures", vec![Dependencies], &[], TargetGated, "Slices where a cross-fitted model of the designated target fails: out-of-fold errors one-sided or materially larger than the model's overall error"),
        make("breaks", vec![Changes, Gaps], &[], Default, "Sustained level, trend, variance changes, and transient excursions"),
        make("ordered_partitions", vec![Changes], &[], Default, "Model-based recursive partitions of a numeric response along a time axis or a categorical dimension"),
        make("surprises", vec![Intervals], &[], Default, "Isolated numeric values and context-breaking categorical values"),
        make("discontinuities", vec![Changes], &[], Default, "Axis-aligned and diagonal threshold jumps"),
        // Opt-in since the 2026-11-02 battery: zero findings in 40
        // measured runs; it engages on a time hint agent callers
        // rarely pass, and issue #95 tracks a panic on long-form
        // data with a time hint.
        make("drift", vec![Changes], &["gbm"], OptIn, "Reference-period model-routing changes along a declared time axis"),
        // Opt-in: a model-based segmentation over the joint
        // measure distribution; stated splits carry permutation
        // p-values, and the pass is heavier than the breaks scan.
        make("changeforest", vec![Changes], &[], OptIn, "Distribution changes in the measures jointly along the time axis, from classifier-based changepoint detection"),
        // Opt-in: a searched conjunction-conditional distribution
        // statement (issue #272). The search evaluates every
        // supported conjunction of conditions against every
        // target on the shared sample — the heaviest single pass
        // in the registry — and its statements are the only ones
        // in the report whose selection is itself the risk the
        // evidence has to answer for, so the control stack
        // (split-sample confirmation, BH over the confirmation
        // family, candidate count in coverage) is part of the
        // pass. Same cost and inference class as views and
        // changeforest: explicit selection.
        make("slice_shifts", vec![Associations], &[], OptIn, "Searched conjunctions of conditions under which a target column's distribution shifts, confirmed on held-out rows"),
        // The ordered-axis battery (#273, #277): shape statements
        // along any ordered axis and located breaks in a
        // relationship or inside a slice, deduped against the
        // breaks/changeforest/partition statements. Its scans are
        // the breaks scans' cost class (prefix-sum scans plus
        // shuffle calibration on aggregated series), so it runs
        // in the default set like breaks and ordered_partitions.
        make("ordered_axis", vec![Changes, Associations], &[], Default, "Shape of a response along an ordered axis (monotonicity, reversal blocks, saturation) and located level, trend, slope, regime, and spike events along it, including inside one categorical level"),
        make(
            "association_rules",
            vec![ConditionalRules, Associations],
            &[],
            Default,
        "Pairwise nominal rules with train and diagnostic evidence"),
        make("hotspots", vec![Concentration], &[], Default, "Combinations concentrated beyond their marginal frequencies"),
        make("numeric_dependence", vec![Associations], &[], Default, "Pearson, Spearman, and both directions of Chatterjee's xi"),
        make(
            "identities",
            vec![Dependencies],
            &["numeric_dependence"],
            Default,
        "Affine and small multi-term arithmetic relationships"),
        make(
            "period_associations",
            vec![Associations],
            &["numeric_dependence"],
            // Opt-in since the 2026-11-02 battery: zero findings in
            // 40 measured runs across two batteries.
            OptIn,
        "Changes in pairwise correlation across calendar or year periods"),
        // Opt-in: experimental until its criterion review closes.
        make(
            "views",
            vec![Associations, Changes, Concentration, Gaps],
            &[],
            OptIn,
        "Experimental shared row partitions, cluster differences, and density findings"),
    ]
}

pub fn active_detectors(selected: Option<&BTreeSet<String>>) -> BTreeSet<String> {
    let specs = builtin_detectors();
    let mut active = selected.cloned().unwrap_or_else(|| {
        // The default set: detectors whose activation is Default or
        // HintGated. Opt-in detectors (views, dates, pivot,
        // period_associations, drift, granularity, changeforest,
        // slice_shifts) run only when
        // explicitly requested via detectors=[...]; the target-gated
        // detectors (target_shapes, model_failures) run only when a
        // target is designated (the pipeline adds them then) or
        // when explicitly requested together with a target.
        specs
            .iter()
            .filter(|d| matches!(d.activation, Activation::Default | Activation::HintGated))
            .map(|d| d.id.clone())
            .collect()
    });
    loop {
        let previous = active.len();
        for d in &specs {
            if active.contains(&d.id) {
                active.extend(d.requires.iter().cloned());
            }
        }
        if active.len() == previous {
            return active;
        }
    }
}

pub struct DetectorContext<'a, 'data> {
    pub table: &'a TypedTable<'data>,
    pub columns: &'a BTreeMap<String, ColumnCard>,
    pub encoded: Option<&'a EncodedMatrix>,
    pub config: &'a ProfileConfig,
}

/// Extensions return statements and evidence, not detector-named public kinds.
pub trait Detector: Send + Sync {
    fn descriptor(&self) -> DetectorDescriptor;
    fn discover(&self, context: &DetectorContext<'_, '_>) -> Result<Vec<Finding>, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_set_is_the_30_measured_detectors() {
        let active = active_detectors(None);
        // 43 registered: 32 default (30 Default + hint-gated
        // expectations and group_contrast), 8 opt-in, 3 target-gated.
        assert_eq!(builtin_detectors().len(), 43);
        assert_eq!(active.len(), 32);
        for id in [
            "views",
            "dates",
            "pivot",
            "period_associations",
            "drift",
            "granularity",
            "changeforest",
            "slice_shifts",
            "target_shapes",
            "model_failures",
        ] {
            assert!(!active.contains(id), "{id} must not be in the default set");
        }
        assert!(active.contains("expectations"));
        assert!(active.contains("group_contrast"));
        assert!(active.contains("discontinuities"));
    }

    #[test]
    fn explicit_selection_still_reaches_opt_in_detectors() {
        let selected: BTreeSet<String> = ["drift".to_string()].into_iter().collect();
        let active = active_detectors(Some(&selected));
        // drift plus its gbm resource.
        assert!(active.contains("drift"));
        assert!(active.contains("gbm"));
    }

    #[test]
    fn activations_partition_the_registry() {
        let mut counts = BTreeMap::new();
        for d in builtin_detectors() {
            *counts.entry(format!("{:?}", d.activation)).or_insert(0) += 1;
        }
        assert_eq!(counts.get("Default"), Some(&30));
        assert_eq!(counts.get("OptIn"), Some(&8));
        assert_eq!(counts.get("HintGated"), Some(&2));
        assert_eq!(counts.get("TargetGated"), Some(&3));
    }

    #[test]
    fn every_builtin_detector_declares_a_purpose() {
        for d in builtin_detectors() {
            assert!(
                !d.purpose.trim().is_empty(),
                "detector {} declares no purpose",
                d.id
            );
        }
    }

    #[test]
    fn registry_and_pipeline_dispatch_agree() {
        let src = include_str!("pipeline.rs");
        let mut dispatched: BTreeSet<String> = BTreeSet::new();
        let mut rest = src;
        while let Some(i) = rest.find("enabled(\"") {
            let after = &rest[i + "enabled(\"".len()..];
            let end = after.find('"').expect("dispatch literal terminates");
            dispatched.insert(after[..end].to_string());
            rest = &after[end..];
        }
        let registered: BTreeSet<String> =
            builtin_detectors().iter().map(|d| d.id.clone()).collect();
        assert_eq!(
            dispatched, registered,
            "every registry entry must have a dispatch site in pipeline.rs and every dispatch site must name a registry entry"
        );
    }

    #[test]
    fn pipeline_statement_wording_keys_are_registered() {
        let src = include_str!("pipeline.rs");
        let region = src
            .split("missing_values=match")
            .nth(1)
            .expect("wording match present")
            .split("let candidate_count")
            .next()
            .expect("region bounded");
        let registered: BTreeSet<String> =
            builtin_detectors().iter().map(|d| d.id.clone()).collect();
        let mut keys: BTreeSet<String> = BTreeSet::new();
        let mut buf = String::new();
        for line in region.lines() {
            buf.push_str(line);
            if buf.contains("=>") {
                let head = buf.split("=>").next().unwrap_or("");
                let mut rest = head;
                while let Some(i) = rest.find('"') {
                    let after = &rest[i + 1..];
                    match after.find('"') {
                        Some(end) => {
                            keys.insert(after[..end].to_string());
                            rest = &after[end + 1..];
                        }
                        None => break,
                    }
                }
                buf.clear();
            }
        }
        assert!(!keys.is_empty(), "wording/sampling keys extracted");
        for key in &keys {
            assert!(
                registered.contains(key),
                "pipeline wording/sampling key {key} is not a registered detector"
            );
        }
    }
}
