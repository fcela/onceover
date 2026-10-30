//! onceover-deps: keys and exact relationships — unique column
//! combinations, exact/approximate functional dependencies, key ranking,
//! and sortedness checks.

pub mod dupe;
pub mod fd;
pub mod granularity;
pub mod hashutil;
pub mod keys;
pub mod pivot;
pub mod sorted;

pub use dupe::{profile_duplicate_encodings, DupeConfig};
pub use fd::{discover, DepsOutcome, FdConfig};
pub use granularity::granularity_changes;
pub use keys::{analyze_keys, KeyOutcome};
pub use pivot::{analyze_pivot, PivotConfig, PivotOutcome, PivotSpec};
pub use sorted::sortedness;

/// Compact per-column info the dependency passes need (built by the
/// pipeline from column cards).
#[derive(Clone, Debug)]
pub struct ColInfo {
    pub name: String,
    pub dtype: String,
    pub role: String,
    pub n: usize,
    pub null_count: usize,
    pub distinct: usize,
    /// Example duplicated values (for dirty-key findings).
    pub dup_examples: Vec<String>,
}

impl ColInfo {
    pub fn n_nonnull(&self) -> usize {
        self.n - self.null_count
    }
    pub fn distinct_ratio(&self) -> f64 {
        self.distinct as f64 / self.n_nonnull().max(1) as f64
    }
    /// Number of non-null values that are duplicates of an earlier value.
    pub fn dup_count(&self) -> usize {
        self.n_nonnull().saturating_sub(self.distinct)
    }
}
