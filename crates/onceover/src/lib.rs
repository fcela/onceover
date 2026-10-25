//! onceover: an evidence-first, Rust-native data profiler for Polars
//! DataFrames — the one-turn first look for humans and AI agents.
//!
//! - `profile(df, &config)` → a complete, deterministic
//!   [`onceover_report::ProfileReport`] (ranked findings ledger + column
//!   cards + coverage block).
//! - All profiling computation runs in Rust.

pub mod config;
pub mod pipeline;

pub use config::{Budget, ProfileConfig, TierConfig};
pub use pipeline::profile;

pub use onceover_report::{
    finalize_ledger, ColumnCard, Confidence, Finding, Overview, PassInfo, ProfileReport,
    SCHEMA_VERSION,
};

/// Convenience: profile with default (standard) configuration.
pub fn profile_default(df: &polars::prelude::DataFrame) -> ProfileReport {
    profile(df, &ProfileConfig::default())
}

/// Stride subsample helper (shared with mines).
pub fn stride_rows(n: usize, cap: usize) -> Vec<u32> {
    onceover_mines::stride_rows(n, cap)
}
