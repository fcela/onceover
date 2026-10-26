//! Shared target descriptions consumed by prediction and explanation detectors.

/// Per-target screening result (also feeds decision tables / PRISM).
#[derive(Clone, Debug)]
pub struct TargetProfile {
    pub col: usize,
    pub name: String,
    pub is_categorical: bool,
    /// R² (numeric) or accuracy (categorical).
    pub metric: f64,
    /// Majority baseline accuracy (categorical).
    pub baseline: f64,
    pub n_train: usize,
    pub n_holdout: usize,
    pub rounds: usize,
    /// (feature name, cumulative gain share) — top features.
    pub top_features: Vec<(String, f64)>,
    /// Every predictor used in the fitted model, distinct from the top gain terms.
    pub inputs: Vec<String>,
    /// Categorical targets: (class ids aligned to enc rows, labels).
    pub cat_ids: Option<(Vec<u8>, Vec<String>)>,
}
