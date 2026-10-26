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
        make("functional_dependencies", vec![Dependencies], &[]),
        make("sortedness", vec![Ordering], &[]),
        make("dates", vec![Gaps, Intervals], &[]),
        make("gbm", vec![Dependencies], &[]),
        make("decision_tables", vec![Dependencies], &["gbm"]),
        make("prism", vec![ConditionalRules], &["gbm"]),
        make("regimes", vec![Dependencies, Changes], &["gbm"]),
        make("association_rules", vec![ConditionalRules], &[]),
        make("numeric_dependence", vec![Associations], &[]),
        make("identities", vec![Dependencies], &["numeric_dependence"]),
    ]
}

pub fn active_detectors(selected: Option<&BTreeSet<String>>) -> BTreeSet<String> {
    let specs = builtin_detectors();
    let mut active = selected
        .cloned()
        .unwrap_or_else(|| specs.iter().map(|d| d.id.clone()).collect());
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
