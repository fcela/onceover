//! onceover: an evidence-first, Rust-native data profiler for Polars
//! DataFrames — the one-turn first look for humans and AI agents.
//!
//! - `profile(df, &config)` → a complete, deterministic
//!   [`onceover_report::ProfileReport`] (ranked findings ledger + column
//!   cards + coverage block).
//! - All profiling computation runs in Rust.

pub mod account;
pub mod basis;
pub mod config;
pub(crate) mod declared;
pub mod detectors;
pub mod frame;
pub mod many;
pub mod panel;
pub mod partition_scagnostics;
pub mod pipeline;
mod rules;
pub mod spec;
pub use detectors::{
    builtin_detectors, Detector, DetectorContext, DetectorDescriptor, Membership, Trigger,
};

pub use config::{Budget, ProfileConfig, TierConfig};
pub use many::{log_choose, profile_many, MultiTableReport};
pub use partition_scagnostics::partition_scagnostics;
pub use pipeline::{profile, profile_with_detectors};
pub use spec::{apply_spec, hierarchy_candidates, infer_spec};

/// IDs of all registered `on_request` detectors (derived from the registry).
pub fn on_request_detector_ids() -> Vec<String> {
    builtin_detectors()
        .into_iter()
        .filter(|d| d.membership == Membership::OnRequest)
        .map(|d| d.id)
        .collect()
}

pub use onceover_report::{
    finalize_ledger, Category, ColumnCard, Evidence, EvidenceBasis, Finding, FindingGroup, Method,
    Operator, Overview, PassInfo, Pattern, Predicate, ProfileReport, SelectionPolicy,
    Specification, Subject, SCHEMA_VERSION,
};

/// Convenience: profile with default (standard) configuration.
pub fn profile_default(df: &polars::prelude::DataFrame) -> ProfileReport {
    profile(df, &ProfileConfig::default())
}
