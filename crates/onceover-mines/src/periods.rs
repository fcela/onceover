//! Period-split associations: recompute pairwise Pearson correlations
//! within the periods of a date (or `year`) column and report the pairs
//! whose correlation changes materially between periods. A pooled
//! correlation can hide a sign flip or a large strengthening/weakening;
//! this pass exists to keep the pooled number from being quoted for a
//! pair it does not describe.
//!
//! Periods come from the first date/datetime column (calendar years when
//! 2–12 years each hold at least 30 rows, otherwise a two-way split at
//! the median date) or, failing that, from an integer column named
//! `year` whose values lie in 1900..=2100. Change is measured between
//! the two extreme periods: the difference in r must clear 0.3, the
//! Fisher z difference must clear 3 standard errors, and at least one
//! extreme period must itself reach |r| = 0.3. No p-value is reported;
//! the z difference and its ingredients are in the evidence.

use std::collections::{BTreeMap, BTreeSet};

use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;

use crate::dep::{pearson, sampled_pairs, PairDep};

/// Rows a period must hold (in the full frame) to take part.
const MIN_PERIOD_ROWS: usize = 30;
/// At most this many calendar-year periods; more falls back to a
/// two-way median split so period labels stay readable.
const MAX_YEAR_PERIODS: usize = 12;
/// Distinct values a `year` column may hold to define periods.
const MAX_YEAR_VALUES: usize = 15;
/// Pair universe at or below which every numeric pair is checked, so a
/// sign flip that cancels in the pooled screen is still caught. Above
/// it, only the dependence screen's pairs are checked (coverage says
/// so).
const ALL_PAIRS_LIMIT: usize = 2048;
const MAX_FINDINGS: usize = 10;
/// Minimum |r_max − r_min| between the extreme periods.
const DELTA_GATE: f64 = 0.3;
/// Minimum Fisher z difference, in standard errors.
const Z_GATE: f64 = 3.0;
/// At least one extreme period must reach this |r|.
const SIGNAL_FLOOR: f64 = 0.3;

pub enum PeriodsStatus {
    NotApplicable(String),
    Ran(String),
    Capped(String),
}

pub struct PeriodsOutcome {
    pub findings: Vec<Finding>,
    pub status: PeriodsStatus,
}

/// Column index, name, per-row period index, and period labels.
struct PeriodAxis {
    column: usize,
    labels: Vec<String>,
    row_period: Vec<Option<u32>>,
}

fn periods_from_days(days: &[Option<i64>]) -> Option<(Vec<String>, Vec<Option<u32>>)> {
    let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
    for d in days.iter().flatten() {
        *counts
            .entry(onceover_core::dateutil::civil_from_days(*d).0)
            .or_insert(0) += 1;
    }
    let usable: Vec<i64> = counts
        .iter()
        .filter(|(_, &c)| c >= MIN_PERIOD_ROWS)
        .map(|(&y, _)| y)
        .collect();
    if (2..=MAX_YEAR_PERIODS).contains(&usable.len()) {
        let index: BTreeMap<i64, u32> = usable
            .iter()
            .enumerate()
            .map(|(i, &y)| (y, i as u32))
            .collect();
        let row_period = days
            .iter()
            .map(|d| {
                d.and_then(|d| {
                    index
                        .get(&onceover_core::dateutil::civil_from_days(d).0)
                        .copied()
                })
            })
            .collect();
        return Some((usable.iter().map(i64::to_string).collect(), row_period));
    }
    // Median split: two half-periods at the median day.
    let mut sorted: Vec<i64> = days.iter().flatten().copied().collect();
    if sorted.len() < 2 * MIN_PERIOD_ROWS {
        return None;
    }
    sorted.sort_unstable();
    let threshold = sorted[sorted.len() / 2];
    let (mut lo, mut hi) = (0usize, 0usize);
    let row_period: Vec<Option<u32>> = days
        .iter()
        .map(|d| match d {
            Some(d) if *d < threshold => {
                lo += 1;
                Some(0)
            }
            Some(_) => {
                hi += 1;
                Some(1)
            }
            None => None,
        })
        .collect();
    if lo < MIN_PERIOD_ROWS || hi < MIN_PERIOD_ROWS {
        return None;
    }
    Some((
        vec![
            format!("before {}", onceover_core::dateutil::iso_date(threshold)),
            format!("from {}", onceover_core::dateutil::iso_date(threshold)),
        ],
        row_period,
    ))
}

fn period_axis(table: &TypedTable) -> Option<PeriodAxis> {
    if let Some(di) = table
        .cols
        .iter()
        .position(|c| matches!(c, TypedCol::Date { .. } | TypedCol::Datetime { .. }))
    {
        let days: Vec<Option<i64>> = match &table.cols[di] {
            TypedCol::Date { vals } => vals.iter().map(|v| v.map(i64::from)).collect(),
            TypedCol::Datetime { vals } => vals
                .iter()
                .map(|v| v.map(|us| us.div_euclid(86_400_000_000)))
                .collect(),
            _ => unreachable!(),
        };
        if let Some((labels, row_period)) = periods_from_days(&days) {
            return Some(PeriodAxis {
                column: di,
                labels,
                row_period,
            });
        }
    }
    // An integer column named `year` with plausible calendar values.
    for (i, col) in table.cols.iter().enumerate() {
        if !table.names[i].eq_ignore_ascii_case("year") {
            continue;
        }
        let TypedCol::I64 { vals } = col else {
            continue;
        };
        let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
        let mut plausible = true;
        for v in vals.iter().flatten() {
            if !(1900..=2100).contains(v) {
                plausible = false;
                break;
            }
            *counts.entry(*v).or_insert(0) += 1;
        }
        if !plausible
            || !(2..=MAX_YEAR_VALUES).contains(&counts.len())
            || counts.values().any(|&c| c < MIN_PERIOD_ROWS)
        {
            continue;
        }
        let years: Vec<i64> = counts.keys().copied().collect();
        let index: BTreeMap<i64, u32> = years
            .iter()
            .enumerate()
            .map(|(j, &y)| (y, j as u32))
            .collect();
        let row_period = vals
            .iter()
            .map(|v| v.and_then(|v| index.get(v).copied()))
            .collect();
        return Some(PeriodAxis {
            column: i,
            labels: years.iter().map(i64::to_string).collect(),
            row_period,
        });
    }
    None
}

fn fisher_z(r1: f64, n1: usize, r2: f64, n2: usize) -> f64 {
    // n >= 30 is guaranteed by dep::pearson returning Some.
    let at = |r: f64| r.clamp(-0.999_999_999, 0.999_999_999).atanh();
    let se = (1.0 / (n1 as f64 - 3.0) + 1.0 / (n2 as f64 - 3.0)).sqrt();
    (at(r1) - at(r2)) / se
}

struct ChangedPair {
    a: usize,
    b: usize,
    delta: f64,
    finding: Finding,
}

pub fn period_associations(table: &TypedTable, deps: &[PairDep], row_cap: usize) -> PeriodsOutcome {
    let Some(axis) = period_axis(table) else {
        return PeriodsOutcome {
            findings: Vec::new(),
            status: PeriodsStatus::NotApplicable(
                "no date column or `year` column defines at least two periods of 30+ rows".into(),
            ),
        };
    };
    let k = axis.labels.len();
    let period_name = table.names[axis.column].clone();

    let numeric: Vec<usize> = (0..table.cols.len())
        .filter(|&i| {
            i != axis.column && matches!(table.cols[i], TypedCol::I64 { .. } | TypedCol::F64 { .. })
        })
        .collect();
    let universe = numeric.len() * numeric.len().saturating_sub(1) / 2;
    let mut candidates: BTreeSet<(usize, usize)> = deps
        .iter()
        .map(|d| (d.a.min(d.b), d.a.max(d.b)))
        .filter(|&(a, b)| a != axis.column && b != axis.column)
        .collect();
    let all_pairs = universe <= ALL_PAIRS_LIMIT;
    if all_pairs {
        for (i, &a) in numeric.iter().enumerate() {
            for &b in numeric.iter().skip(i + 1) {
                candidates.insert((a, b));
            }
        }
    }

    let rows = crate::stride_rows(table.n, row_cap);
    let sampled = rows.len() < table.n;
    let mut period_rows: Vec<Vec<u32>> = vec![Vec::new(); k];
    for &r in &rows {
        if let Some(p) = axis.row_period[r as usize] {
            period_rows[p as usize].push(r);
        }
    }
    let pooled_pearson: BTreeMap<(usize, usize), Option<f64>> = deps
        .iter()
        .map(|d| ((d.a.min(d.b), d.a.max(d.b)), d.pearson))
        .collect();

    let mut changed: Vec<ChangedPair> = Vec::new();
    for &(a, b) in &candidates {
        let mut stats: Vec<Option<(usize, f64)>> = Vec::with_capacity(k);
        for rows_p in &period_rows {
            let pairs = sampled_pairs(&table.cols[a], &table.cols[b], rows_p);
            stats.push(pearson(&pairs).map(|r| (pairs.len(), r)));
        }
        let qualified: Vec<(usize, usize, f64)> = stats
            .iter()
            .enumerate()
            .filter_map(|(p, s)| s.map(|(n, r)| (p, n, r)))
            .collect();
        if qualified.len() < 2 {
            continue;
        }
        let &(p_min, n_min, r_min) = qualified.iter().min_by(|x, y| x.2.total_cmp(&y.2)).unwrap();
        let &(p_max, n_max, r_max) = qualified.iter().max_by(|x, y| x.2.total_cmp(&y.2)).unwrap();
        let delta = r_max - r_min;
        if delta < DELTA_GATE || r_max.abs().max(r_min.abs()) < SIGNAL_FLOOR {
            continue;
        }
        let z = fisher_z(r_max, n_max, r_min, n_min);
        if z.abs() < Z_GATE {
            continue;
        }
        let pooled = pooled_pearson
            .get(&(a, b))
            .copied()
            .flatten()
            .or_else(|| pearson(&sampled_pairs(&table.cols[a], &table.cols[b], &rows)));
        let (a_name, b_name) = (&table.names[a], &table.names[b]);
        let mut claim = format!(
            "correlation between {a_name} and {b_name} ranges from {r_min:.2} in {} (n = {n_min}) to {r_max:.2} in {} (n = {n_max}) across {k} periods",
            axis.labels[p_min], axis.labels[p_max]
        );
        if let Some(p) = pooled {
            claim.push_str(&format!("; pooled correlation is {p:.2}"));
        }
        let period_stats: Vec<serde_json::Value> = qualified
            .iter()
            .map(|&(p, n, r)| json!({"label": axis.labels[p], "n": n, "pearson": r}))
            .collect();
        let total_n: usize = qualified.iter().map(|q| q.1).sum();
        let metrics = BTreeMap::from([
            ("variables".into(), json!([a_name, b_name])),
            ("period_column".into(), json!(period_name)),
            ("periods".into(), json!(period_stats)),
            ("delta_r".into(), json!(delta)),
            ("fisher_z".into(), json!(z)),
            ("pooled_pearson".into(), json!(pooled)),
            ("n".into(), json!(total_n)),
            ("population_rows".into(), json!(table.n)),
            ("row_sampled".into(), json!(sampled)),
        ]);
        let raw = BTreeMap::from([
            ("x".into(), json!(a_name)),
            ("y".into(), json!(b_name)),
            ("period_column".into(), json!(period_name)),
            ("periods".into(), json!(axis.labels)),
        ]);
        let finding = Finding::new(
            Pattern::PeriodAssociation,
            raw,
            claim,
            EvidenceBasis::Estimated,
            0.35 + 0.4 * (delta / 2.0).min(1.0),
            metrics,
            Some(format!(
                "the pooled coefficient describes neither extreme period; split the analysis by {period_name} or include a period interaction before quoting a single correlation for this pair"
            )),
            Method::new(
                "period_associations",
                "period_pearson_fisher",
                "Per-period Pearson correlations on a disclosed row sample; extremes compared by a Fisher z difference stated in standard errors; no p-value is reported",
            ),
        );
        changed.push(ChangedPair {
            a,
            b,
            delta,
            finding,
        });
    }
    changed.sort_by(|x, y| {
        y.delta
            .total_cmp(&x.delta)
            .then(table.names[x.a].cmp(&table.names[y.a]))
            .then(table.names[x.b].cmp(&table.names[y.b]))
    });
    let found = changed.len();
    changed.truncate(MAX_FINDINGS);
    let detail = format!(
        "{} pairs checked across {} periods of {} ({}); {} changed associations{}",
        candidates.len(),
        k,
        period_name,
        axis.labels.join(", "),
        found,
        if all_pairs {
            String::new()
        } else {
            format!(
                "; pair universe {universe} exceeds {ALL_PAIRS_LIMIT}, so only the dependence screen's pairs were checked"
            )
        },
    );
    PeriodsOutcome {
        findings: changed.into_iter().map(|c| c.finding).collect(),
        status: if all_pairs {
            PeriodsStatus::Ran(detail)
        } else {
            PeriodsStatus::Capped(detail)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(names: Vec<&str>, cols: Vec<TypedCol<'static>>, n: usize) -> TypedTable<'static> {
        TypedTable {
            n,
            dtypes: names.iter().map(|_| "test".to_string()).collect(),
            names: names.iter().map(|s| s.to_string()).collect(),
            cols,
        }
    }

    fn f64s(v: Vec<f64>) -> TypedCol<'static> {
        TypedCol::F64 {
            vals: v.into_iter().map(Some).collect::<Vec<_>>().into(),
        }
    }

    fn dates(days: Vec<i32>) -> TypedCol<'static> {
        TypedCol::Date {
            vals: days.into_iter().map(Some).collect(),
        }
    }

    /// 200 rows in 2020 and 200 in 2021; y tracks x in 2020 and −x in 2021.
    fn flip_table() -> TypedTable<'static> {
        let (mut d, mut x, mut y) = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..200 {
            d.push(18_262 + (i % 200)); // 2020
            x.push(i as f64);
            y.push(i as f64);
        }
        for i in 0..200 {
            d.push(18_628 + (i % 200)); // 2021
            x.push(i as f64);
            y.push(-(i as f64));
        }
        table(
            vec!["when", "x", "y"],
            vec![dates(d), f64s(x), f64s(y)],
            400,
        )
    }

    #[test]
    fn sign_flip_across_years_is_reported() {
        let t = flip_table();
        let out = period_associations(&t, &[], 20_000);
        assert_eq!(out.findings.len(), 1);
        let f = &out.findings[0];
        assert_eq!(f.finding_type.as_deref(), Some("period_association"));
        f.validate().unwrap();
        let m = f.metrics();
        assert!((m["delta_r"].as_f64().unwrap() - 2.0).abs() < 0.01);
        assert_eq!(m["period_column"], json!("when"));
        let periods = m["periods"].as_array().unwrap();
        assert_eq!(periods.len(), 2);
        assert_eq!(periods[0]["label"], json!("2020"));
        assert_eq!(periods[1]["label"], json!("2021"));
        assert!(matches!(out.status, PeriodsStatus::Ran(_)));
    }

    #[test]
    fn stable_relationship_is_not_flagged() {
        let (mut d, mut x, mut y) = (Vec::new(), Vec::new(), Vec::new());
        for (start, _) in [(18_262, 0), (18_628, 0)] {
            for i in 0..200 {
                d.push(start + (i % 200));
                x.push(i as f64);
                y.push(2.0 * i as f64 + (i % 3) as f64);
            }
        }
        let t = table(
            vec!["when", "x", "y"],
            vec![dates(d), f64s(x), f64s(y)],
            400,
        );
        let out = period_associations(&t, &[], 20_000);
        assert!(out.findings.is_empty());
    }

    #[test]
    fn no_period_axis_is_not_applicable() {
        let t = table(
            vec!["x", "y"],
            vec![f64s(vec![1.0; 100]), f64s(vec![2.0; 100])],
            100,
        );
        let out = period_associations(&t, &[], 20_000);
        assert!(matches!(out.status, PeriodsStatus::NotApplicable(_)));
    }

    #[test]
    fn integer_year_column_defines_periods() {
        let (mut yr, mut x, mut y) = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..200 {
            yr.push(2019);
            x.push(i as f64);
            y.push(i as f64);
        }
        for i in 0..200 {
            yr.push(2020);
            x.push(i as f64);
            y.push(-(i as f64));
        }
        let t = table(
            vec!["year", "x", "y"],
            vec![
                TypedCol::I64 {
                    vals: yr.into_iter().map(Some).collect::<Vec<_>>().into(),
                },
                f64s(x),
                f64s(y),
            ],
            400,
        );
        let out = period_associations(&t, &[], 20_000);
        assert_eq!(out.findings.len(), 1);
        assert_eq!(out.findings[0].metrics()["period_column"], json!("year"));
    }

    #[test]
    fn strengthening_without_flip_is_reported() {
        // r ≈ 0 in 2020 (noise), r = 1 in 2021.
        let (mut d, mut x, mut y) = (Vec::new(), Vec::new(), Vec::new());
        let mut s = 0x12345678u64;
        let mut noise = move || {
            s ^= s << 13;
            s ^= s >> 7;
            (s % 1000) as f64 / 10.0
        };
        for i in 0..200 {
            d.push(18_262 + (i % 200));
            x.push(i as f64);
            y.push(noise());
        }
        for i in 0..200 {
            d.push(18_628 + (i % 200));
            x.push(i as f64);
            y.push(i as f64);
        }
        let t = table(
            vec!["when", "x", "y"],
            vec![dates(d), f64s(x), f64s(y)],
            400,
        );
        let out = period_associations(&t, &[], 20_000);
        assert_eq!(out.findings.len(), 1);
        let m = out.findings[0].metrics();
        assert!(m["delta_r"].as_f64().unwrap() > 0.8);
    }
}
