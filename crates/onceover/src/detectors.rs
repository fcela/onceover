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
        }
    }

    pub fn with_activation(mut self, activation: Activation) -> Self {
        self.activation = activation;
        self
    }
}

pub fn builtin_detectors() -> Vec<DetectorDescriptor> {
    use Activation::*;
    use Category::*;
    let make = |id: &str, categories: Vec<Category>, requires: &[&str], activation: Activation| {
        DetectorDescriptor::new(
            id,
            categories,
            requires.iter().map(|s| s.to_string()).collect(),
        )
        .with_activation(activation)
    };
    vec![
        make(
            "column_profile",
            vec![Missingness, Concentration, ColumnRoles, Formats],
            &[],
            Default,
        ),
        make("row_duplicates", vec![Duplicates], &[], Default),
        make("keys", vec![Uniqueness], &[], Default),
        // Opt-in since the 2026-11-02 battery: zero findings in 40
        // measured runs (20 datasets x harness and default configs).
        // The detector is unchanged and runs when explicitly selected.
        make("pivot", vec![ColumnRoles], &[], OptIn),
        make("functional_dependencies", vec![Dependencies], &[], Default),
        make("order_constraints", vec![Dependencies], &[], Default),
        make("duplicate_encodings", vec![ColumnRoles], &[], Default),
        make("sortedness", vec![Ordering], &[], Default),
        // Opt-in since the 2026-11-02 battery: zero findings in 40
        // measured runs at first measurement.
        make("granularity", vec![Formats], &[], OptIn),
        make("sentinels", vec![Missingness], &[], Default),
        make("boundaries", vec![Formats], &[], Default),
        make("missingness", vec![Missingness], &[], Default),
        make("content", vec![Formats], &[], Default),
        make("series", vec![Gaps, Changes], &[], Default),
        make("digits", vec![Formats], &[], Default),
        // Opt-in since the 2026-11-02 battery: zero findings in 40
        // measured runs across two batteries (2026-10-29, 2026-11-02).
        make("dates", vec![Gaps, Intervals], &[], OptIn),
        // Hint-gated: a default-set member that engages only on
        // declared hints and otherwise records itself skipped.
        make("expectations", vec![Gaps, Ordering], &[], HintGated),
        make("gbm", vec![Dependencies], &[], Default),
        make("decision_tables", vec![Dependencies], &["gbm"], Default),
        make("prism", vec![ConditionalRules], &["gbm"], Default),
        make("regimes", vec![Dependencies, Changes], &["gbm"], Default),
        make("interactions", vec![Dependencies], &["gbm"], Default),
        make("target_shapes", vec![Dependencies], &[], TargetGated),
        make("breaks", vec![Changes], &[], Default),
        make("ordered_partitions", vec![Changes], &[], Default),
        make("surprises", vec![Intervals], &[], Default),
        make("discontinuities", vec![Changes], &[], Default),
        // Opt-in since the 2026-11-02 battery: zero findings in 40
        // measured runs; it engages on a time hint agent callers
        // rarely pass, and issue #95 tracks a panic on long-form
        // data with a time hint.
        make("drift", vec![Changes], &["gbm"], OptIn),
        make("association_rules", vec![ConditionalRules], &[], Default),
        make("hotspots", vec![Concentration], &[], Default),
        make("numeric_dependence", vec![Associations], &[], Default),
        make(
            "identities",
            vec![Dependencies],
            &["numeric_dependence"],
            Default,
        ),
        make(
            "period_associations",
            vec![Associations],
            &["numeric_dependence"],
            // Opt-in since the 2026-11-02 battery: zero findings in
            // 40 measured runs across two batteries.
            OptIn,
        ),
        // Opt-in: experimental until its criterion review closes.
        make(
            "views",
            vec![Associations, Changes, Concentration],
            &[],
            OptIn,
        ),
    ]
}

pub fn active_detectors(selected: Option<&BTreeSet<String>>) -> BTreeSet<String> {
    let specs = builtin_detectors();
    let mut active = selected.cloned().unwrap_or_else(|| {
        // The default set: detectors whose activation is Default or
        // HintGated. Opt-in detectors (views, dates, pivot,
        // period_associations, drift, granularity) run only when
        // explicitly requested via detectors=[...]; target-gated
        // target_shapes runs only when a target is designated (the
        // pipeline adds it then) or when explicitly requested
        // together with a target.
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
    fn default_set_is_the_27_measured_detectors() {
        let active = active_detectors(None);
        // 34 registered: 27 default (26 Default + hint-gated
        // expectations), 6 opt-in, 1 target-gated.
        assert_eq!(builtin_detectors().len(), 34);
        assert_eq!(active.len(), 27);
        for id in [
            "views",
            "dates",
            "pivot",
            "period_associations",
            "drift",
            "granularity",
            "target_shapes",
        ] {
            assert!(!active.contains(id), "{id} must not be in the default set");
        }
        assert!(active.contains("expectations"));
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
        assert_eq!(counts.get("Default"), Some(&26));
        assert_eq!(counts.get("OptIn"), Some(&6));
        assert_eq!(counts.get("HintGated"), Some(&1));
        assert_eq!(counts.get("TargetGated"), Some(&1));
    }
}
