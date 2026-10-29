//! Budget tiers and per-pass configuration (the bounded-compute contract).

use std::time::Duration;

use onceover_deps::{DupeConfig, FdConfig};
use onceover_mines::{
    AssociationConfig, BreaksConfig, DiscConfig, DtableConfig, EncodeConfig, PrismConfig,
    RegimesConfig, ScreenConfig, SurpriseConfig, ViewsConfig,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Budget {
    Quick,
    Standard,
    Deep,
}

impl Budget {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "quick" => Ok(Budget::Quick),
            "standard" => Ok(Budget::Standard),
            "deep" => Ok(Budget::Deep),
            other => Err(format!(
                "unknown budget '{other}' (expected quick | standard | deep)"
            )),
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Budget::Quick => "quick",
            Budget::Standard => "standard",
            Budget::Deep => "deep",
        }
    }
}

/// Full profiling configuration. Deterministic by default: the wall-clock
/// deadline is the single opt-in source of nondeterminism (P5, P8).
#[derive(Clone, Debug)]
pub struct ProfileConfig {
    pub budget: Budget,
    /// Explicit detector IDs; None selects all built-ins. Dependencies run as resources.
    pub detectors: Option<std::collections::BTreeSet<String>>,
    pub selection: onceover_report::SelectionPolicy,
    pub deadline: Option<Duration>,
    /// Override sampled relationship rows (column summaries remain exact).
    pub sample_rows: Option<usize>,
    /// Maximum shortlisted predictors per fitted model, after screening all columns.
    pub max_predictors: Option<usize>,
    /// Maximum detailed numeric pairs, after screening every numeric pair.
    pub max_pairs: Option<usize>,
    /// User-declared structure (time axis, ordering, gap-free columns,
    /// cadences, dimensions, measures), checked by the expectations pass.
    pub hints: Option<onceover_report::ProfileHints>,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        ProfileConfig {
            budget: Budget::Standard,
            detectors: None,
            selection: Default::default(),
            deadline: None,
            sample_rows: None,
            max_predictors: None,
            max_pairs: None,
            hints: None,
        }
    }
}

/// Per-tier pass configurations.
pub struct TierConfig {
    pub fd: FdConfig,
    pub dupe: DupeConfig,
    pub encode: EncodeConfig,
    pub screen: ScreenConfig,
    pub dtable: DtableConfig,
    pub prism: PrismConfig,
    pub regimes: RegimesConfig,
    pub breaks: BreaksConfig,
    pub surprises: SurpriseConfig,
    pub disc: DiscConfig,
    pub association: AssociationConfig,
    pub views: ViewsConfig,
    pub dep_pair_cap: usize,
    pub ident_pair_cap: usize,
    pub ident_multiterm_cap: usize,
    pub dtable_top_targets: usize,
    pub prism_top_targets: usize,
    pub pivot_max_levels: usize,
    pub pivot_max_findings: usize,
}

impl TierConfig {
    pub fn for_budget(b: Budget) -> TierConfig {
        match b {
            Budget::Quick => TierConfig {
                fd: FdConfig::default(),
                dupe: DupeConfig::disabled(),
                encode: EncodeConfig {
                    row_cap: 10_000,
                    feature_cap: 32,
                    nominal_max_levels: 32,
                },
                screen: ScreenConfig::default(),
                dtable: DtableConfig::default(),
                prism: PrismConfig::default(),
                regimes: RegimesConfig::default(),
                breaks: BreaksConfig::default(),
                surprises: SurpriseConfig::default(),
                disc: DiscConfig::disabled(),
                association: AssociationConfig::default(),
                views: ViewsConfig::default(),
                dep_pair_cap: 0,
                ident_pair_cap: 0,
                ident_multiterm_cap: 0,
                dtable_top_targets: 0,
                prism_top_targets: 0,
                pivot_max_levels: 32,
                pivot_max_findings: 3,
            },
            Budget::Standard => TierConfig {
                fd: FdConfig {
                    lhs_cap: 16,
                    lhs3_cap: 12,
                    row_cap: 200_000,
                    max_lhs_size: 2,
                    afd_g1_max: 0.01,
                    key_pair_cap: 16,
                },
                dupe: DupeConfig::default(),
                encode: EncodeConfig {
                    row_cap: 20_000,
                    feature_cap: 48,
                    nominal_max_levels: 32,
                },
                screen: ScreenConfig::default(),
                dtable: DtableConfig::default(),
                prism: PrismConfig::default(),
                regimes: RegimesConfig::default(),
                breaks: BreaksConfig::default(),
                surprises: SurpriseConfig::default(),
                association: AssociationConfig {
                    max_findings: 20,
                    ..Default::default()
                },
                views: ViewsConfig::default(),
                disc: DiscConfig::default(),
                dep_pair_cap: 496,
                ident_pair_cap: 100,
                ident_multiterm_cap: 2_000,
                dtable_top_targets: 6,
                prism_top_targets: 6,
                pivot_max_levels: 32,
                pivot_max_findings: 3,
            },
            Budget::Deep => TierConfig {
                fd: FdConfig {
                    lhs_cap: 32,
                    lhs3_cap: 12,
                    row_cap: 500_000,
                    max_lhs_size: 3,
                    afd_g1_max: 0.01,
                    key_pair_cap: 24,
                },
                dupe: DupeConfig {
                    row_cap: 250_000,
                    ..DupeConfig::default()
                },
                encode: EncodeConfig {
                    row_cap: 50_000,
                    feature_cap: 64,
                    nominal_max_levels: 32,
                },
                screen: ScreenConfig {
                    max_rounds: 40,
                    max_predictors: 64,
                    max_model_rows: 10_000,
                    ..Default::default()
                },
                dtable: DtableConfig::default(),
                prism: PrismConfig::default(),
                breaks: BreaksConfig {
                    row_cap: 50_000,
                    series_cap: 32,
                    max_breaks: 4,
                    ..Default::default()
                },
                surprises: SurpriseConfig::default(),
                regimes: RegimesConfig {
                    row_cap: 10_000,
                    target_cap: 8,
                    model: onceover_mines::regimes::ModelTreeConfig {
                        max_depth: 4,
                        max_features: 10,
                        min_leaf: 50,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                disc: DiscConfig {
                    row_cap: 50_000,
                    max_per_target: 5,
                    ..Default::default()
                },
                association: AssociationConfig {
                    max_findings: 40,
                    ..Default::default()
                },
                views: ViewsConfig {
                    row_cap: 50_000,
                    ..Default::default()
                },
                dep_pair_cap: 2_016,
                ident_pair_cap: 200,
                ident_multiterm_cap: 8_000,
                dtable_top_targets: 10,
                prism_top_targets: 10,
                pivot_max_levels: 32,
                pivot_max_findings: 3,
            },
        }
    }
}
