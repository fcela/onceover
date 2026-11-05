//! Simpson reversal check for strong association statements
//! (issue #134, item 5).
//!
//! A strong-tier association statement is unconditional: it
//! reports the pooled direction of a pair. This module checks
//! whether that direction reverses inside every usable stratum
//! of a categorical dimension. When it does, the caller appends
//! a qualification to the claim and attaches the `simpson`
//! evidence payload built here; the finding itself is never
//! removed or re-tiered.
//!
//! The criterion is the textbook Simpson form, fixed in
//! docs/benchmarks/2026-11-05-simpson-check.md before any corpus
//! measurement: every usable stratum's signed effect has the
//! opposite sign to the pooled signed effect. A stratum is
//! usable when it holds at least 30 pair-complete rows and the
//! effect is defined on them; a stratifier is evaluated only
//! when at least two strata are usable and together they cover
//! at least half of the pair-complete rows.

use crate::dep::{pearson, spearman};
use crate::encode::{EncodedMatrix, MISSING};
use onceover_core::{TypedCol, TypedTable};
use serde_json::{json, Map, Value};

/// Minimum pair-complete rows for a stratum to be usable.
pub const MIN_STRATUM_ROWS: usize = 30;
/// Stratifying columns carry 2..=8 observed levels.
pub const MAX_STRATIFIER_LEVELS: usize = 8;
/// At most this many stratifying columns are considered per pair.
pub const MAX_STRATIFIERS_PER_PAIR: usize = 24;
/// At most this many pair-by-stratifier evaluations run per screen.
pub const MAX_EVALUATIONS_PER_SCREEN: usize = 400;
/// A numeric pair's directional component must reach this
/// absolute value for its reversal to be material: below it the
/// statement's strength is the unsigned xi component and the pair
/// carries no direction worth qualifying.
pub const MIN_DIRECTIONAL_EFFECT: f64 = 0.3;

/// Which signed statistic a pair's direction is read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignedStat {
    Pearson,
    Spearman,
    PointBiserial,
    LogOddsRatio,
}

impl SignedStat {
    /// Payload key naming the statistic.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Pearson => "pearson",
            Self::Spearman => "spearman",
            Self::PointBiserial => "point_biserial",
            Self::LogOddsRatio => "log_odds_ratio",
        }
    }
    /// Claim wording naming the statistic.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pearson => "Pearson r",
            Self::Spearman => "Spearman rho",
            Self::PointBiserial => "point-biserial r",
            Self::LogOddsRatio => "log odds ratio",
        }
    }
    fn is_correlation(&self) -> bool {
        !matches!(self, Self::LogOddsRatio)
    }
}

/// A candidate stratifying column, with the stratum code of
/// every row in the caller's evaluated row universe, in the
/// universe's order.
pub struct Stratifier {
    /// Table column index; the identity used to skip a pair's
    /// own columns.
    pub col: usize,
    pub name: String,
    /// Level labels in code order.
    pub labels: Vec<String>,
    /// Stratum code per row of the universe; None where the
    /// row's value is missing.
    pub codes: Vec<Option<u16>>,
}

/// One usable stratum of a qualifying stratifier.
pub struct StratumEffect {
    pub level: String,
    pub n: usize,
    pub effect: f64,
}

/// A stratifier that reverses a pair's pooled direction.
pub struct SimpsonHit {
    pub stratifier: String,
    /// Stratum-weighted within effect (Fisher-z average for
    /// correlations, Woolf inverse-variance for log odds
    /// ratios); carries the reversed sign by construction.
    pub within: f64,
    pub strata: Vec<StratumEffect>,
    /// Observed levels of the stratifying column.
    pub levels_total: usize,
    /// Share of the pair-complete rows the usable strata cover.
    pub covered_share: f64,
}

/// The per-screen evaluation budget. `capped` records that a
/// bound stopped some check, for the coverage entry.
pub struct SimpsonBudget {
    evals_left: usize,
    pub capped: bool,
}

impl Default for SimpsonBudget {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpsonBudget {
    pub fn new() -> Self {
        Self {
            evals_left: MAX_EVALUATIONS_PER_SCREEN,
            capped: false,
        }
    }
}

/// Log odds ratio of a 2x2 table given as (x, y) pairs with
/// x, y in {0, 1}: log((n00*n11)/(n01*n10)), with 0.5 added to
/// all four cells when any cell is zero. None when either side
/// is constant (a zero margin), where no contrast exists.
pub fn log_odds_ratio(pairs: &[(f64, f64)]) -> Option<f64> {
    log_odds_parts(pairs).map(|(v, _)| v)
}

fn log_odds_parts(pairs: &[(f64, f64)]) -> Option<(f64, f64)> {
    let (mut n00, mut n01, mut n10, mut n11) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for &(x, y) in pairs {
        match (x > 0.5, y > 0.5) {
            (false, false) => n00 += 1.0,
            (false, true) => n01 += 1.0,
            (true, false) => n10 += 1.0,
            (true, true) => n11 += 1.0,
        }
    }
    if n00 + n01 == 0.0 || n10 + n11 == 0.0 || n00 + n10 == 0.0 || n01 + n11 == 0.0 {
        return None;
    }
    let corr = if n00 == 0.0 || n01 == 0.0 || n10 == 0.0 || n11 == 0.0 {
        0.5
    } else {
        0.0
    };
    let (a, b, c, d) = (n00 + corr, n01 + corr, n10 + corr, n11 + corr);
    let value = (a * d / (b * c)).ln();
    let variance = 1.0 / a + 1.0 / b + 1.0 / c + 1.0 / d;
    Some((value, variance))
}

/// The signed effect of one stratum, with its combination
/// weight: (n - 3) for correlations (the Fisher-z weight), the
/// inverse Woolf variance for log odds ratios. None when the
/// effect is undefined on the stratum's rows — including the
/// correlation helpers' own n >= 30 requirement, which is the
/// usable-stratum row floor.
fn stratum_effect(stat: SignedStat, pairs: &[(f64, f64)]) -> Option<(f64, f64)> {
    match stat {
        SignedStat::Pearson | SignedStat::PointBiserial => {
            pearson(pairs).map(|r| (r, (pairs.len() as f64 - 3.0).max(1.0)))
        }
        SignedStat::Spearman => spearman(pairs).map(|r| (r, (pairs.len() as f64 - 3.0).max(1.0))),
        SignedStat::LogOddsRatio => log_odds_parts(pairs).map(|(v, var)| (v, 1.0 / var)),
    }
}

fn combine(stat: SignedStat, effects: &[(f64, f64)]) -> f64 {
    let (mut num, mut den) = (0.0f64, 0.0f64);
    for &(e, w) in effects {
        if stat.is_correlation() {
            num += w * e.clamp(-0.999999, 0.999999).atanh();
        } else {
            num += w * e;
        }
        den += w;
    }
    let v = num / den;
    if stat.is_correlation() {
        v.tanh()
    } else {
        v
    }
}

/// Check one pair against the candidate stratifiers, in the
/// order given (column order at both call sites), and return
/// the first stratifier that reverses the pooled direction.
///
/// `universe` holds one entry per row of the evaluated row
/// universe — the same universe the stratifiers' codes align
/// with — carrying the pair's values where both are present.
/// `skip_cols` holds the pair's own table column indices.
pub fn reversal_for_pair(
    stat: SignedStat,
    pooled: f64,
    universe: &[(Option<f64>, Option<f64>)],
    stratifiers: &[Stratifier],
    skip_cols: &[usize],
    budget: &mut SimpsonBudget,
) -> Option<SimpsonHit> {
    if pooled == 0.0 || !pooled.is_finite() {
        return None;
    }
    let pair_rows = universe
        .iter()
        .filter(|(x, y)| x.is_some() && y.is_some())
        .count();
    if pair_rows == 0 {
        return None;
    }
    let mut considered = 0usize;
    for strat in stratifiers {
        if skip_cols.contains(&strat.col) {
            continue;
        }
        if considered >= MAX_STRATIFIERS_PER_PAIR || budget.evals_left == 0 {
            budget.capped = true;
            break;
        }
        considered += 1;
        budget.evals_left -= 1;
        // Bucket the pair-complete rows by stratum.
        let mut buckets: Vec<Vec<(f64, f64)>> = vec![Vec::new(); strat.labels.len()];
        for (pos, (x, y)) in universe.iter().enumerate() {
            if let (Some(x), Some(y)) = (x, y) {
                if let Some(code) = strat.codes.get(pos).copied().flatten() {
                    if (code as usize) < buckets.len() {
                        buckets[code as usize].push((*x, *y));
                    }
                }
            }
        }
        let mut usable: Vec<StratumEffect> = Vec::new();
        let mut weights: Vec<(f64, f64)> = Vec::new();
        let mut covered = 0usize;
        for (code, rows) in buckets.iter().enumerate() {
            if rows.len() < MIN_STRATUM_ROWS {
                continue;
            }
            if let Some((effect, weight)) = stratum_effect(stat, rows) {
                covered += rows.len();
                usable.push(StratumEffect {
                    level: strat.labels[code].clone(),
                    n: rows.len(),
                    effect,
                });
                weights.push((effect, weight));
            }
        }
        if usable.len() < 2 || covered as f64 * 2.0 < pair_rows as f64 {
            continue;
        }
        // The textbook form: every usable stratum reverses.
        if usable.iter().all(|s| s.effect * pooled < 0.0) {
            return Some(SimpsonHit {
                stratifier: strat.name.clone(),
                within: combine(stat, &weights),
                strata: usable,
                levels_total: strat.labels.len(),
                covered_share: covered as f64 / pair_rows as f64,
            });
        }
    }
    None
}

/// The claim clause appended to a qualified finding's claim.
pub fn claim_clause(stat: SignedStat, pooled: f64, hit: &SimpsonHit) -> String {
    let lo = hit
        .strata
        .iter()
        .map(|s| s.effect)
        .fold(f64::INFINITY, f64::min);
    let hi = hit
        .strata
        .iter()
        .map(|s| s.effect)
        .fold(f64::NEG_INFINITY, f64::max);
    format!(
        "; the direction reverses within every level of {} (pooled {} {:+.3}; within-level {} from {:+.3} to {:+.3}, combined {:+.3})",
        hit.stratifier,
        stat.label(),
        pooled,
        stat.label(),
        lo,
        hi,
        hit.within
    )
}

/// The `simpson` evidence payload for a qualified finding.
/// Callers add the coding keys for binary statistics
/// (`coded_level` / `coded_levels`) to the returned map.
pub fn simpson_payload(stat: SignedStat, pooled: f64, hit: &SimpsonHit) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("statistic".into(), json!(stat.key()));
    m.insert("stratifier".into(), json!(hit.stratifier));
    m.insert("pooled_effect".into(), json!(pooled));
    m.insert("within_effect".into(), json!(hit.within));
    m.insert(
        "strata".into(),
        json!(hit
            .strata
            .iter()
            .map(|s| json!({"level": s.level, "n": s.n, "effect": s.effect}))
            .collect::<Vec<_>>()),
    );
    m.insert("strata_usable".into(), json!(hit.strata.len()));
    m.insert("strata_levels".into(), json!(hit.levels_total));
    m.insert("rows_covered_share".into(), json!(hit.covered_share));
    m
}

/// Stratifying columns of a typed table over a row sample:
/// string and boolean columns with 2-8 observed levels,
/// excluding the columns the caller withholds. Level order is
/// sorted label order, so the result does not depend on the
/// sample's row order.
pub fn stratifiers_from_table(
    table: &TypedTable,
    rows: &[u32],
    excluded: &[usize],
) -> Vec<Stratifier> {
    let mut out = Vec::new();
    for (ci, col) in table.cols.iter().enumerate() {
        if excluded.contains(&ci) {
            continue;
        }
        let value_at: Vec<Option<String>> = match col {
            TypedCol::Str { .. } | TypedCol::StrOwned { .. } => rows
                .iter()
                .map(|&r| col.str_at(r as usize).map(str::to_string))
                .collect(),
            TypedCol::Bool { vals } => rows
                .iter()
                .map(|&r| vals[r as usize].map(|b| b.to_string()))
                .collect(),
            _ => continue,
        };
        let mut labels: Vec<String> = Vec::new();
        for v in value_at.iter().flatten() {
            if !labels.contains(v) {
                labels.push(v.clone());
            }
        }
        if labels.len() < 2 || labels.len() > MAX_STRATIFIER_LEVELS {
            continue;
        }
        labels.sort();
        let codes: Vec<Option<u16>> = value_at
            .iter()
            .map(|v| {
                v.as_ref()
                    .and_then(|v| labels.iter().position(|l| l == v).map(|p| p as u16))
            })
            .collect();
        out.push(Stratifier {
            col: ci,
            name: table.names[ci].clone(),
            labels,
            codes,
        });
    }
    out
}

/// Stratifying columns of an encoded matrix: nominal features
/// with 2-8 levels, excluding the features whose table columns
/// the caller withholds. Codes and labels are the encoding's.
pub fn stratifiers_from_enc(enc: &EncodedMatrix, excluded: &[usize]) -> Vec<Stratifier> {
    let mut out = Vec::new();
    for f in 0..enc.bins.len() {
        if enc.feat_nominal.get(f) != Some(&true) || enc.levels[f].is_empty() {
            continue;
        }
        let col = enc.feat_cols[f];
        if excluded.contains(&col) {
            continue;
        }
        let n_levels = enc.n_bins[f];
        if !(2..=MAX_STRATIFIER_LEVELS).contains(&n_levels) || enc.levels[f].len() != n_levels {
            continue;
        }
        let codes: Vec<Option<u16>> = enc.bins[f]
            .iter()
            .map(|&c| if c == MISSING { None } else { Some(c as u16) })
            .collect();
        out.push(Stratifier {
            col,
            name: enc.feat_names[f].clone(),
            labels: enc.levels[f].clone(),
            codes,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    type Universe = Vec<(Option<f64>, Option<f64>)>;

    fn stratifier(name: &str, col: usize, labels: &[&str], codes: &[Option<u16>]) -> Stratifier {
        Stratifier {
            col,
            name: name.into(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            codes: codes.to_vec(),
        }
    }

    /// Two strata of 100 rows each; within each, y falls as x
    /// rises (r = -1 exactly, points on a line). The second
    /// stratum's x range is shifted up and its y baseline sits
    /// higher still, so the pooled r is positive: the textbook
    /// reversal.
    fn simpson_universe() -> (Universe, Vec<Option<u16>>) {
        let mut universe = Vec::new();
        let mut codes = Vec::new();
        for g in 0..2u16 {
            for i in 0..100 {
                let x = 100.0 * g as f64 + i as f64;
                let y = 400.0 * g as f64 - x;
                universe.push((Some(x), Some(y)));
                codes.push(Some(g));
            }
        }
        (universe, codes)
    }

    fn pooled_pearson(universe: &Universe) -> f64 {
        pearson(
            &universe
                .iter()
                .map(|(x, y)| (x.unwrap(), y.unwrap()))
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    #[test]
    fn detects_full_reversal() {
        let (universe, codes) = simpson_universe();
        let pooled = pooled_pearson(&universe);
        assert!(pooled > 0.0, "pooled = {pooled}");
        let strat = stratifier("g", 9, &["lo", "hi"], &codes);
        let mut budget = SimpsonBudget::new();
        let hit = reversal_for_pair(
            SignedStat::Pearson,
            pooled,
            &universe,
            &[strat],
            &[0, 1],
            &mut budget,
        )
        .expect("reversal must fire");
        assert_eq!(hit.stratifier, "g");
        assert_eq!(hit.strata.len(), 2);
        assert!(hit.within < 0.0);
        assert!((hit.covered_share - 1.0).abs() < 1e-12);
    }

    #[test]
    fn homogeneous_strata_do_not_fire() {
        // Same-sign within strata: y rises with x in both.
        let mut universe = Vec::new();
        let mut codes = Vec::new();
        for g in 0..2u16 {
            for i in 0..100 {
                universe.push((Some(i as f64), Some(50.0 * g as f64 + i as f64)));
                codes.push(Some(g));
            }
        }
        let pooled = pooled_pearson(&universe);
        let strat = stratifier("g", 9, &["lo", "hi"], &codes);
        let mut budget = SimpsonBudget::new();
        assert!(reversal_for_pair(
            SignedStat::Pearson,
            pooled,
            &universe,
            &[strat],
            &[0, 1],
            &mut budget
        )
        .is_none());
    }

    #[test]
    fn one_reversing_stratum_of_two_does_not_fire() {
        // Stratum 0 reverses, stratum 1 agrees with the pooled
        // sign: the textbook form requires every usable stratum.
        let mut universe = Vec::new();
        let mut codes = Vec::new();
        for i in 0..100 {
            universe.push((Some(i as f64), Some(200.0 - i as f64)));
            codes.push(Some(0));
        }
        for i in 0..100 {
            universe.push((Some(i as f64), Some(300.0 + i as f64)));
            codes.push(Some(1));
        }
        let strat = stratifier("g", 9, &["lo", "hi"], &codes);
        let mut budget = SimpsonBudget::new();
        assert!(reversal_for_pair(
            SignedStat::Pearson,
            0.5,
            &universe,
            &[strat],
            &[0, 1],
            &mut budget
        )
        .is_none());
    }

    #[test]
    fn single_usable_stratum_does_not_fire() {
        let (universe, mut codes) = simpson_universe();
        // Null out all but 29 of stratum 1's codes: only one
        // usable stratum remains.
        for c in codes.iter_mut().skip(129) {
            *c = None;
        }
        let strat = stratifier("g", 9, &["lo", "hi"], &codes);
        let mut budget = SimpsonBudget::new();
        assert!(reversal_for_pair(
            SignedStat::Pearson,
            0.5,
            &universe,
            &[strat],
            &[0, 1],
            &mut budget
        )
        .is_none());
    }

    #[test]
    fn pair_columns_are_never_their_own_stratifier() {
        let (universe, codes) = simpson_universe();
        let strat = stratifier("x", 0, &["lo", "hi"], &codes);
        let mut budget = SimpsonBudget::new();
        assert!(reversal_for_pair(
            SignedStat::Pearson,
            0.5,
            &universe,
            &[strat],
            &[0, 1],
            &mut budget
        )
        .is_none());
    }

    #[test]
    fn log_odds_ratio_orientation_and_correction() {
        // x=1 rows mostly y=1: positive log OR.
        let mut pairs = Vec::new();
        for _ in 0..40 {
            pairs.push((0.0, 0.0));
            pairs.push((1.0, 1.0));
        }
        for _ in 0..10 {
            pairs.push((0.0, 1.0));
            pairs.push((1.0, 0.0));
        }
        let v = log_odds_ratio(&pairs).unwrap();
        assert!(v > 0.0, "log OR = {v}");
        // A zero cell is corrected, not infinite.
        let mut sparse = vec![(0.0, 0.0); 50];
        sparse.extend(vec![(1.0, 1.0); 50]);
        sparse.extend(vec![(1.0, 0.0); 5]);
        let v = log_odds_ratio(&sparse).unwrap();
        assert!(v.is_finite() && v > 0.0);
        // A constant side has no contrast.
        assert!(log_odds_ratio(&[(0.0, 0.0), (0.0, 1.0)]).is_none());
    }

    #[test]
    fn budget_exhaustion_marks_capped() {
        let (universe, codes) = simpson_universe();
        let strat = stratifier("g", 9, &["lo", "hi"], &codes);
        let mut budget = SimpsonBudget {
            evals_left: 0,
            capped: false,
        };
        assert!(reversal_for_pair(
            SignedStat::Pearson,
            0.5,
            &universe,
            &[strat],
            &[0, 1],
            &mut budget
        )
        .is_none());
        assert!(budget.capped);
    }

    #[test]
    fn claim_clause_names_stratifier_and_signs() {
        let hit = SimpsonHit {
            stratifier: "region".into(),
            within: -0.45,
            strata: vec![
                StratumEffect {
                    level: "north".into(),
                    n: 120,
                    effect: -0.52,
                },
                StratumEffect {
                    level: "south".into(),
                    n: 130,
                    effect: -0.38,
                },
            ],
            levels_total: 2,
            covered_share: 1.0,
        };
        let clause = claim_clause(SignedStat::Pearson, 0.62, &hit);
        assert!(clause.contains("region"), "{clause}");
        assert!(clause.contains("+0.620"), "{clause}");
        assert!(clause.contains("-0.520"), "{clause}");
        assert!(clause.contains("-0.450"), "{clause}");
    }
}
