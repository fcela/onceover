//! Drift along a declared time axis (issue #50): fit each numeric
//! target's screen model on the earliest period, then score every
//! later period by how its rows route through that model's trees —
//! the gain-weighted mean chi-square of per-split routing counts,
//! reference period vs scored period (`FittedGbm::drift_score`).
//! Under no drift the statistic averages ~1; it grows with the size
//! of the shift and with row counts, so each period is compared at a
//! row count equal on both sides.
//!
//! A finding additionally has to beat a permutation null: the
//! reference rows and every period's compared rows are pooled once,
//! reassigned to a pseudo-reference group and pseudo-period groups
//! of the original sizes, every period is rescored against the
//! shared pseudo-reference, and the maximum over periods is
//! recomputed — the null distribution of the reported statistic
//! itself, including the max-over-periods selection and the fact
//! that all periods share one reference (an atypical reference draw
//! elevates every real comparison at once, and the replicates
//! reproduce that). Under no drift the real labeling is
//! exchangeable with the replicates, so a real maximum that does
//! not exceed the replicate maxima is sampling fluctuation, however
//! large it looks (a model whose splits all repeat one partition,
//! e.g. a single binary predictor, produces large raw scores on
//! stable data).
//!
//! The pass engages only when the caller declares a time axis
//! (`hints.time`); without one there is nothing to compare against.

use crate::encode::EncodedMatrix;
use crate::gbm::{fit_gbm_model_restricted, Lcg, ScreenConfig};
use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct DriftConfig {
    pub enabled: bool,
    /// Mean chi-square at or above which a target's drift is reported.
    pub min_score: f64,
    /// Periods are contiguous ranges of the time axis with roughly
    /// equal row counts, at most this many.
    pub max_periods: usize,
    /// A period comparison needs at least this many rows on each side.
    pub min_period_rows: usize,
    /// The reference-period model must reach this holdout R² for its
    /// splits to be a meaningful lens (the regimes pass uses the same
    /// 0.2 candidate gate against the screen metric).
    pub min_reference_r2: f64,
    /// Label shuffles for the permutation null a finding must beat.
    pub permutations: usize,
}

impl Default for DriftConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_score: 5.0,
            max_periods: 8,
            min_period_rows: 100,
            min_reference_r2: 0.2,
            permutations: 16,
        }
    }
}

impl DriftConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Orderable time key per row (NaN where the value is missing or the
/// column type carries no order). Dates arrive as days since epoch,
/// datetimes as microseconds — both order correctly as f64.
fn time_keys(table: &TypedTable, ci: usize) -> Vec<f64> {
    let n = table.n;
    let mut out = vec![f64::NAN; n];
    match &table.cols[ci] {
        TypedCol::I64 { vals } => {
            for (i, v) in vals.iter().enumerate() {
                if let Some(v) = v {
                    out[i] = *v as f64;
                }
            }
        }
        TypedCol::U64 { vals } => {
            for (i, v) in vals.iter().enumerate() {
                if let Some(v) = v {
                    out[i] = *v as f64;
                }
            }
        }
        TypedCol::F64 { vals } => {
            for (i, v) in vals.iter().enumerate() {
                if let Some(v) = v {
                    out[i] = *v;
                }
            }
        }
        TypedCol::Date { vals } => {
            for (i, v) in vals.iter().enumerate() {
                if let Some(v) = v {
                    out[i] = *v as f64;
                }
            }
        }
        TypedCol::Datetime { .. } => {
            for (i, key) in out.iter_mut().enumerate() {
                if let Some(v) = table.cols[ci].datetime_micros(i) {
                    *key = v as f64;
                }
            }
        }
        _ => {}
    }
    out
}

/// Numeric target values (NaN where missing), mirroring the I64/F64
/// arms the other passes read.
fn column_f64(table: &TypedTable, idx: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; table.n];
    match &table.cols[idx] {
        TypedCol::I64 { vals } => {
            for (i, v) in vals.iter().enumerate() {
                if let Some(v) = v {
                    out[i] = *v as f64;
                }
            }
        }
        TypedCol::U64 { vals } => {
            for (i, v) in vals.iter().enumerate() {
                if let Some(v) = v {
                    out[i] = *v as f64;
                }
            }
        }
        TypedCol::F64 { vals } => {
            for (i, v) in vals.iter().enumerate() {
                if let Some(v) = v {
                    out[i] = *v;
                }
            }
        }
        _ => {}
    }
    out
}

/// Contiguous periods over the sorted time keys: cut only at key
/// boundaries, aiming for equal row counts, at most `max_periods`.
/// Returns (first row position, last row position) index ranges into
/// the sorted order, plus the sorted order itself.
fn build_periods(
    keys: &[f64],
    max_periods: usize,
    min_period_rows: usize,
) -> (Vec<(usize, usize)>, Vec<u32>) {
    let mut order: Vec<u32> = (0..keys.len())
        .filter(|&i| keys[i].is_finite())
        .map(|i| i as u32)
        .collect();
    order.sort_by(|&a, &b| keys[a as usize].total_cmp(&keys[b as usize]));
    let total = order.len();
    if total < 2 * min_period_rows {
        return (Vec::new(), order);
    }
    let target = total.div_ceil(max_periods).max(min_period_rows);
    let mut periods: Vec<(usize, usize)> = Vec::new();
    let mut start = 0usize;
    while start < total {
        let mut end = (start + target).min(total);
        // Do not split rows that share a time key across periods.
        while end < total && keys[order[end] as usize] == keys[order[end - 1] as usize] {
            end += 1;
        }
        // A short tail merges into the previous period.
        if total - end < min_period_rows && !periods.is_empty() {
            end = total;
        }
        periods.push((start, end));
        start = end;
    }
    if periods.len() < 2 {
        return (Vec::new(), order);
    }
    (periods, order)
}

/// Deterministic subsample of `rows` to `k` entries (seeded shuffle,
/// the screen's Lcg), or the rows themselves when already short.
fn subsample(rows: &[u32], k: usize, seed: u64) -> Vec<u32> {
    if rows.len() <= k {
        return rows.to_vec();
    }
    let mut out = rows.to_vec();
    let mut rng = Lcg(seed);
    for i in (1..out.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        out.swap(i, j);
    }
    out.truncate(k);
    out
}

/// Maximum over periods of the drift score, recomputed over
/// `perms` replicates that pool the reference rows and every
/// period's compared rows, then reassign them to a pseudo-reference
/// group of the reference's size and disjoint pseudo-period groups
/// of the compared sizes. Every period in a replicate is scored
/// against the same pseudo-reference, as in the real comparisons.
/// Under no drift the real labeling is exchangeable with these
/// replicates, so their maximum is the null bar the real maximum
/// must clear.
fn permuted_max(
    model: &crate::gbm::FittedGbm,
    reference: &[Vec<u8>],
    period_sets: &[(usize, Vec<Vec<u8>>)],
    perms: usize,
    seed: u64,
) -> f64 {
    if period_sets.is_empty() || perms == 0 || reference.is_empty() {
        return f64::INFINITY;
    }
    let r = reference.len();
    let total = r + period_sets
        .iter()
        .map(|(_, rows)| rows.len())
        .sum::<usize>();
    let mut pool: Vec<&Vec<u8>> = Vec::with_capacity(total);
    pool.extend(reference.iter());
    for (_, rows) in period_sets {
        pool.extend(rows.iter());
    }
    let mut rng = Lcg(seed);
    let mut best = 0.0f64;
    for _ in 0..perms {
        let mut idx: Vec<usize> = (0..total).collect();
        for i in (1..idx.len()).rev() {
            let j = (rng.next_u64() % (i as u64 + 1)) as usize;
            idx.swap(i, j);
        }
        let pseudo_ref: Vec<Vec<u8>> = idx[..r].iter().map(|&i| pool[i].clone()).collect();
        let mut cursor = r;
        for (n_cmp, rows) in period_sets {
            let pseudo_period: Vec<Vec<u8>> = idx[cursor..cursor + rows.len()]
                .iter()
                .map(|&i| pool[i].clone())
                .collect();
            cursor += rows.len();
            let n = (*n_cmp).min(pseudo_ref.len());
            if n == 0 {
                continue;
            }
            best = best.max(model.drift_score(&pseudo_ref[..n], &pseudo_period, false));
        }
    }
    best
}

fn pack_rows(enc: &EncodedMatrix, selected: &[usize], rows: &[u32]) -> Vec<Vec<u8>> {
    rows.iter()
        .map(|&r| {
            selected
                .iter()
                .map(|&f| enc.bins[f][r as usize])
                .collect::<Vec<u8>>()
        })
        .collect()
}

fn fmt_key(table: &TypedTable, ci: usize, key: f64) -> String {
    match &table.cols[ci] {
        TypedCol::Date { .. } => iso_date(key as i64),
        TypedCol::Datetime { .. } => iso_date((key / 86_400_000_000.0) as i64),
        _ => crate::fmt_float(key),
    }
}

/// Days since epoch -> YYYY-MM-DD (civil-from-days algorithm).
fn iso_date(days: i64) -> String {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn profile_drift(
    table: &TypedTable,
    enc: &EncodedMatrix,
    time_column: Option<&str>,
    excluded: &[usize],
    screen_cfg: &ScreenConfig,
    cfg: &DriftConfig,
) -> (Vec<Finding>, String) {
    if !cfg.is_enabled() {
        return (Vec::new(), "disabled at this budget".into());
    }
    let Some(time_name) = time_column else {
        return (Vec::new(), "no time axis declared".into());
    };
    let Some(time_ci) = table.names.iter().position(|n| n == time_name) else {
        return (Vec::new(), format!("time column '{time_name}' not found"));
    };
    let keys = time_keys(table, time_ci);
    let (periods, order) = build_periods(&keys, cfg.max_periods, cfg.min_period_rows);
    if periods.len() < 2 {
        return (Vec::new(), "fewer than two usable periods".into());
    }
    let period_label = |p: usize| -> String {
        let (s, e) = periods[p];
        let lo = keys[order[s] as usize];
        let hi = keys[order[e - 1] as usize];
        format!(
            "{} to {}",
            fmt_key(table, time_ci, lo),
            fmt_key(table, time_ci, hi)
        )
    };

    let targets: Vec<usize> = (0..table.cols.len())
        .filter(|&i| i != time_ci && table.cols[i].is_numeric() && !excluded.contains(&i))
        .collect();
    let mut findings = Vec::new();
    for &ci in &targets {
        let ys = column_f64(table, ci);
        let present_in = |p: usize| -> Vec<u32> {
            let (s, e) = periods[p];
            order[s..e]
                .iter()
                .copied()
                .filter(|&r| ys[r as usize].is_finite())
                .collect()
        };
        let reference_rows: BTreeSet<u32> = {
            let (s, e) = periods[0];
            order[s..e].iter().copied().collect()
        };
        let mut exclude: Vec<usize> = excluded.to_vec();
        exclude.push(time_ci);
        let Some(model) =
            fit_gbm_model_restricted(table, enc, ci, screen_cfg, Some(&reference_rows), &exclude)
        else {
            continue;
        };
        if model.holdout_r2 < cfg.min_reference_r2 {
            continue;
        }
        let ref_present = model.present.clone();
        let ref_packed_full = pack_rows(enc, &model.selected, &ref_present);
        let mut series: Vec<Value> = Vec::new();
        let mut max_score = f64::MIN;
        let mut max_period = 0usize;
        // Compared row sets of every scored period, kept for the
        // permutation null.
        let mut period_sets: Vec<(usize, Vec<Vec<u8>>)> = Vec::new();
        for p in 1..periods.len() {
            let rows_p = present_in(p);
            let n_cmp = ref_present.len().min(rows_p.len());
            let (score, concept) = if n_cmp < cfg.min_period_rows {
                (None, None)
            } else {
                let seed = (ci as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (p as u64);
                let ref_sub = subsample(&ref_present, n_cmp, seed);
                let p_sub = subsample(&rows_p, n_cmp, seed ^ 0xA5A5A5A5);
                let ref_packed = pack_rows(enc, &model.selected, &ref_sub);
                let p_packed = pack_rows(enc, &model.selected, &p_sub);
                let s = model.drift_score(&ref_packed, &p_packed, false);
                let c = model.drift_score(&ref_packed, &p_packed, true);
                period_sets.push((n_cmp, p_packed));
                (Some(s), Some(c))
            };
            if let Some(s) = score {
                if s > max_score {
                    max_score = s;
                    max_period = p;
                }
            }
            series.push(json!({
                "period": p,
                "range": period_label(p),
                "rows": rows_p.len(),
                "compared_rows": n_cmp,
                "drift": score,
                "concept_drift": concept,
            }));
        }
        if max_score < cfg.min_score {
            continue;
        }
        // Permutation null of the maximum itself: pool the
        // reference and period rows, reassign them to groups of the
        // original sizes, rescore all periods against the shared
        // pseudo-reference, retake the maximum. The finding stands
        // only if the real maximum beats every replicate's maximum.
        let null_max = permuted_max(
            &model,
            &ref_packed_full,
            &period_sets,
            cfg.permutations,
            (ci as u64).wrapping_mul(0xD1B54A32D192ED03) ^ (max_period as u64),
        );
        if max_score <= null_max {
            continue;
        }
        let target_name = table.names[ci].clone();
        let mut metrics = BTreeMap::new();
        metrics.insert("time_column".into(), json!(time_name));
        metrics.insert("reference_period".into(), json!(period_label(0)));
        metrics.insert("reference_rows".into(), json!(ref_present.len()));
        metrics.insert("reference_r2".into(), json!(model.holdout_r2));
        metrics.insert("periods".into(), Value::Array(series));
        metrics.insert("max_score".into(), json!(max_score));
        metrics.insert("max_period".into(), json!(max_period));
        metrics.insert("null_max".into(), json!(null_max));
        let mut raw = BTreeMap::new();
        raw.insert("target".into(), json!(target_name));
        raw.insert("time_column".into(), json!(time_name));
        let claim = format!(
            "the predictors of {target_name} drift over {time_name}: split-routing drift score \
             reaches {max_score:.1} in period {max_period} ({}), against a no-drift scale of ~1",
            period_label(max_period),
        );
        let score = 0.5 + 0.45 * (max_score / (max_score + 10.0));
        findings.push(Finding::new(
            Pattern::DistributionShift,
            raw,
            claim,
            EvidenceBasis::Measured,
            score,
            metrics,
            Some(format!(
                "the joint distribution behind {target_name} changes across {time_name}; \
                 relationships fit on the reference period ({}) may not hold later — \
                 check for population, instrument, or coding changes",
                period_label(0),
            )),
            Method::new(
                "drift",
                "reference-period GBM; per-split routing-count chi-square per period",
                "onceover drift screen v1 (issue #50)",
            ),
        ));
    }
    let note = if findings.is_empty() {
        format!("{} targets scored, none over the drift bar", targets.len())
    } else {
        format!(
            "{} drift findings over {} targets",
            findings.len(),
            targets.len()
        )
    };
    (findings, note)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::{encode, EncodeConfig};
    use onceover_core::extract;
    use polars::df;

    fn shifted_frame(shift: bool) -> (Vec<Finding>, String) {
        let mut t: Vec<Option<i64>> = Vec::new();
        let mut x: Vec<Option<f64>> = Vec::new();
        let mut y: Vec<Option<f64>> = Vec::new();
        let mut rng = Lcg(99);
        for i in 0..4000i64 {
            let xv = rng.next_f64() * 2.0 - 1.0 + if shift && i >= 2000 { 2.0 } else { 0.0 };
            t.push(Some(i));
            x.push(Some(xv));
            y.push(Some(xv + 0.05 * rng.next_f64()));
        }
        let df = df! { "t" => t, "x" => x, "y" => y }.unwrap();
        let table = extract(&df);
        let (enc, _) = encode(&table, &EncodeConfig::default());
        profile_drift(
            &table,
            &enc,
            Some("t"),
            &[],
            &ScreenConfig::default(),
            &DriftConfig::default(),
        )
    }

    #[test]
    fn detects_planted_shift() {
        let (findings, _) = shifted_frame(true);
        assert!(
            findings
                .iter()
                .any(|f| f.finding_type.as_deref() == Some("distribution_shift")),
            "expected a drift finding, got {} findings",
            findings.len()
        );
    }

    #[test]
    fn quiet_when_stable() {
        let (findings, _) = shifted_frame(false);
        assert!(
            findings
                .iter()
                .all(|f| f.finding_type.as_deref() != Some("distribution_shift")),
            "stable data must not produce drift findings"
        );
    }

    #[test]
    fn no_time_axis_no_findings() {
        let df = df! {
            "x" => vec![1.0, 2.0, 3.0],
            "y" => vec![1.0, 2.0, 3.0],
        }
        .unwrap();
        let table = extract(&df);
        let (enc, _) = encode(&table, &EncodeConfig::default());
        let (findings, note) = profile_drift(
            &table,
            &enc,
            None,
            &[],
            &ScreenConfig::default(),
            &DriftConfig::default(),
        );
        assert!(findings.is_empty());
        assert_eq!(note, "no time axis declared");
    }

    #[test]
    fn iso_date_known_days() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(iso_date(19_000), "2022-01-08");
    }
}
