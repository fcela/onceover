//! Bounded search for reproducible scatterplot shapes across observed groups.

mod metrics;
mod search;

pub use metrics::Metric;
pub use search::{
    default_metrics, run, run_sampled, run_sampled_with_config, sample_rows, sample_rows_capped,
    Finding, Input, Objective, ResultSet, Rule, ScanPlan, SearchConfig, DEFAULT_DESCRIPTOR_CAP,
    DEFAULT_PAIR_CAP, DEFAULT_ROW_CAP, DEFAULT_RULE_CAP,
};

/// Components shared with the research benchmark.
#[doc(hidden)]
pub mod benchmark {
    pub use super::metrics::{Cloud, Scale, Scratch};
    pub use super::search::Rng;
}
