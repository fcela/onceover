//! Detector registration is separate from the public finding taxonomy.
use crate::ProfileConfig;
use onceover_core::TypedTable;
use onceover_mines::EncodedMatrix;
use onceover_report::{Category, ColumnCard, Finding};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, serde::Serialize)]
pub struct DetectorDescriptor {
    pub id: String,
    pub categories: Vec<Category>,
    pub requires: Vec<String>,
}
impl DetectorDescriptor {
    pub fn new(id: &str, categories: Vec<Category>, requires: Vec<String>) -> Self {
        Self {
            id: id.into(),
            categories,
            requires,
        }
    }
}

pub fn builtin_detectors() -> Vec<DetectorDescriptor> {
    use Category::*;
    let make = |id: &str, categories: Vec<Category>, requires: &[&str]| {
        DetectorDescriptor::new(
            id,
            categories,
            requires.iter().map(|s| s.to_string()).collect(),
        )
    };
    vec![
        make(
            "column_profile",
            vec![Missingness, Concentration, ColumnRoles, Formats],
            &[],
        ),
        make("row_duplicates", vec![Duplicates], &[]),
        make("keys", vec![Uniqueness], &[]),
        make("pivot", vec![ColumnRoles], &[]),
        make("functional_dependencies", vec![Dependencies], &[]),
        make("duplicate_encodings", vec![ColumnRoles], &[]),
        make("sortedness", vec![Ordering], &[]),
        make("dates", vec![Gaps, Intervals], &[]),
        make("expectations", vec![Gaps, Ordering], &[]),
        make("gbm", vec![Dependencies], &[]),
        make("decision_tables", vec![Dependencies], &["gbm"]),
        make("prism", vec![ConditionalRules], &["gbm"]),
        make("regimes", vec![Dependencies, Changes], &["gbm"]),
        make("interactions", vec![Dependencies], &["gbm"]),
        make("target_shapes", vec![Dependencies], &[]),
        make("breaks", vec![Changes], &[]),
        make("surprises", vec![Intervals], &[]),
        make("discontinuities", vec![Changes], &[]),
        make("association_rules", vec![ConditionalRules], &[]),
        make("numeric_dependence", vec![Associations], &[]),
        make("identities", vec![Dependencies], &["numeric_dependence"]),
        make(
            "period_associations",
            vec![Associations],
            &["numeric_dependence"],
        ),
        make("views", vec![Associations, Changes, Concentration], &[]),
    ]
}

pub fn active_detectors(selected: Option<&BTreeSet<String>>) -> BTreeSet<String> {
    let specs = builtin_detectors();
    let mut active = selected.cloned().unwrap_or_else(|| {
        // The greedy views pass is opt-in until its criterion review
        // closes: it is experimental, and running it by default would
        // change every standard/deep report. Request it explicitly via
        // detectors=["views"]. The target_shapes pass is likewise
        // excluded: it runs only when a target is designated (the
        // pipeline adds it then) or when explicitly requested
        // together with a target.
        specs
            .iter()
            .map(|d| d.id.clone())
            .filter(|id| id != "views" && id != "target_shapes")
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
