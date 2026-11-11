//! onceover-mines: the model-based and statistical mining passes —
//! predictability screening (in-house histogram GBM), Kohavi decision
//! tables, PRISM rules, pairwise dependence (Pearson/Spearman/Chatterjee
//! ξ), local linear regimes, nominal associations, arithmetic identities,
//! and date/sequence intelligence.

pub mod association;
pub mod breaks;
pub mod changeforest;
pub mod contrast;
pub mod dates;
pub mod deadline;
pub mod dep;
pub mod disc;
pub mod drift;
pub mod dtable;
pub mod encode;
pub mod expectations;
pub mod gbm;
pub mod hotspots;
pub mod hstat;
pub mod ident;
pub mod missingness;
pub mod panels;
pub mod partitions;
pub mod periods;
pub mod prism;
pub mod regimes;
pub mod rows;
mod screen;
pub mod series;
pub mod shapes;
pub mod simpson;
pub mod surprises;
pub mod targets;
pub mod views;
pub use targets::TargetProfile;
pub use views::{views_pass, ViewsConfig, ViewsOutcome};

pub use association::{association_rules, pair_associations, AssociationConfig};
pub use breaks::{profile_breaks, BreaksConfig};
pub use changeforest::{
    changeforest_pass, ChangeforestConfig, ChangeforestInput, ChangeforestOutcome,
    ChangeforestStatus,
};
pub use contrast::{group_contrast, ContrastOutcome};
pub use dates::{analyze_temporal, DatesOutcome};
pub use deadline::Deadline;
pub use dep::PairDep;
pub use disc::{profile_discontinuities, DiscConfig, DiscOutcome};
pub use drift::{profile_drift, DriftConfig};
pub use dtable::{decision_tables, DtableConfig};
pub use encode::{encode, EncodeConfig, EncodedMatrix};
pub use gbm::{screen as screen_predictability, ScreenConfig, ScreenOutcome, TargetResult};
pub use hotspots::{hotspots, HotspotConfig, HotspotsOutcome};
pub use hstat::{profile_interactions, HConfig};
pub use ident::identities;
pub use ident::multiterm_identities;
pub use missingness::{profile_missingness, MissingnessAxis};
pub use panels::{panel_concentration, panel_dynamics, panel_flags, PanelInput};
pub use partitions::{ordered_partitions, PartitionsConfig, PartitionsOutcome, PartitionsStatus};
pub use periods::{period_associations, PeriodsOutcome, PeriodsStatus};
pub use prism::{prism_rules, PrismConfig};
pub use regimes::{profile_regimes, RegimesConfig};
pub use rows::profile_row_consistency;
pub use series::{profile_series, SeriesInput};
pub use shapes::{profile_target_shapes, shape_diagnostics, ShapeDiagnostics, ShapesConfig};
pub use surprises::{profile_surprises, SurpriseConfig};

/// Deterministic stratified subsample (one hashed offset per row interval) of row indices (0..n down to cap).
pub fn stride_rows(n: usize, cap: usize) -> Vec<u32> {
    onceover_core::sample_rows(n, cap)
}

/// Compact float display (for class labels).
pub fn fmt_float(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.6}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stride_rows_deterministic_and_bounded() {
        assert_eq!(stride_rows(10, 100).len(), 10);
        let s = stride_rows(1_000_000, 20_000);
        assert!(s.len() <= 20_000);
        assert!(s.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(s, stride_rows(1_000_000, 20_000));
    }
}
