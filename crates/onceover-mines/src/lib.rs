//! onceover-mines: the model-based and statistical mining passes —
//! predictability screening (in-house histogram GBM), Kohavi decision
//! tables, PRISM rules, pairwise dependence (Pearson/Spearman/Chatterjee
//! ξ), local linear regimes, nominal associations, arithmetic identities,
//! and date/sequence intelligence.

pub mod association;
pub mod dates;
pub mod dep;
pub mod dtable;
pub mod encode;
pub mod gbm;
pub mod ident;
pub mod prism;
pub mod regimes;
mod screen;
pub mod targets;
pub use targets::TargetProfile;

pub use association::{association_rules, AssociationConfig};
pub use dates::{analyze_temporal, DatesOutcome};
pub use dep::{screen as screen_dependence, PairDep};
pub use dtable::{decision_tables, DtableConfig};
pub use encode::{encode, EncodeConfig, EncodedMatrix};
pub use gbm::{screen as screen_predictability, ScreenConfig, ScreenOutcome, TargetResult};
pub use ident::identities;
pub use prism::{prism_rules, PrismConfig};
pub use regimes::{profile_regimes, RegimesConfig};

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
