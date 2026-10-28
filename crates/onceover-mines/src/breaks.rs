/// The type of structural break detected in a series.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreakKind {
    /// A shift in the level (mean) of the series.
    Level,
    /// A change in the slope of the series.
    Trend,
    /// A change in the variance of the series.
    Variance,
}

/// Configuration parameters governing break detection.
pub struct BreaksConfig {
    /// Maximum number of rows processed per series.
    pub row_cap: usize,
    /// Maximum number of series analyzed.
    pub series_cap: usize,
    /// Maximum number of breaks reported per series.
    pub max_breaks: usize,
    /// Minimum number of observations in each segment around a break.
    pub min_segment: usize,
    /// Minimum gain on training data to accept a candidate break.
    pub min_train_gain: f64,
    /// Minimum gain on holdout data to accept a candidate break.
    pub min_holdout_gain: f64,
    /// Maximum number of grouping columns considered.
    pub max_groups: usize,
    /// Maximum levels per grouping column.
    pub max_group_levels: usize,
    /// Minimum rows required per group to be analyzed.
    pub min_group_rows: usize,
    /// Robust-z threshold for transient excursion points.
    pub excursion_z: f64,
    /// Rolling-median window (points) for the local baseline.
    pub excursion_window: usize,
    /// Unflagged points bridged when grouping excursion runs.
    pub excursion_merge_gap: usize,
    /// Maximum events reported per series.
    pub excursion_max_events: usize,
}

impl Default for BreaksConfig {
    fn default() -> Self {
        Self {
            row_cap: 10000,
            series_cap: 16,
            max_breaks: 3,
            min_segment: 30,
            min_train_gain: 0.10,
            min_holdout_gain: 0.15,
            max_groups: 2,
            max_group_levels: 8,
            min_group_rows: 60,
            excursion_z: 8.0,
            excursion_window: 97,
            excursion_merge_gap: 12,
            excursion_max_events: 10,
        }
    }
}

/// A single detected structural break in a series.
///
/// `left` and `right` hold segment statistics on either side of the break:
/// segment means for [`BreakKind::Level`], segment slopes for
/// [`BreakKind::Trend`], and segment variances for [`BreakKind::Variance`].
#[derive(Clone, Copy, Debug)]
pub struct FoundBreak {
    /// Index of the observation where the break occurs.
    pub index: usize,
    /// The kind of break detected.
    pub kind: BreakKind,
    /// Improvement on training data achieved by splitting at this break.
    pub train_gain: f64,
    /// Statistic of the left segment (mean, slope, or variance).
    pub left: f64,
    /// Statistic of the right segment (mean, slope, or variance).
    pub right: f64,
}

use crate::regimes::single_break_l2;

pub fn level_breaks(values: &[f64], cfg: &BreaksConfig) -> Vec<FoundBreak> {
    if values.len() < 2 * cfg.min_segment || cfg.max_breaks == 0 {
        return Vec::new();
    }
    if values.iter().any(|v| !v.is_finite()) {
        return Vec::new();
    }
    let mut out = Vec::new();
    scan(values, 0, values.len(), cfg, &mut out);
    out.sort_by_key(|b| b.index);
    out
}

fn scan(values: &[f64], start: usize, end: usize, cfg: &BreaksConfig, out: &mut Vec<FoundBreak>) {
    if out.len() >= cfg.max_breaks || end - start < 2 * cfg.min_segment {
        return;
    }
    let br = match single_break_l2(&values[start..end], cfg.min_segment) {
        Some(br) => br,
        None => return,
    };
    if br.gain_fraction < cfg.min_train_gain {
        return;
    }
    let left_slice = &values[start..start + br.index];
    let right_slice = &values[start + br.index..end];
    let left = left_slice.iter().sum::<f64>() / left_slice.len() as f64;
    let right = right_slice.iter().sum::<f64>() / right_slice.len() as f64;
    out.push(FoundBreak {
        index: start + br.index,
        kind: BreakKind::Level,
        train_gain: br.gain_fraction,
        left,
        right,
    });
    scan(values, start, start + br.index, cfg, out);
    scan(values, start + br.index, end, cfg, out);
}

/// Find the single best trend (slope) change point by comparing a one-line OLS
/// fit over the whole range against the best two-line OLS fit, in O(n) via
/// prefix sums. Returns `None` when no split clears the configured gain
/// threshold or when the input is unusable.
pub fn trend_break(values: &[f64], times: &[f64], cfg: &BreaksConfig) -> Option<FoundBreak> {
    let n = values.len();
    if n != times.len() {
        return None;
    }
    if n < cfg.min_segment.saturating_mul(2) {
        return None;
    }
    if values.iter().any(|v| !v.is_finite()) || times.iter().any(|t| !t.is_finite()) {
        return None;
    }

    // Shift times so the first sample is at the origin for numerical stability.
    let t0 = times.first().copied().unwrap_or(0.0);

    // Prefix sums (length n + 1) for u, y, u*u, u*y, y*y.
    let mut pu = vec![0.0f64; n + 1];
    let mut py = vec![0.0f64; n + 1];
    let mut puu = vec![0.0f64; n + 1];
    let mut puy = vec![0.0f64; n + 1];
    let mut pyy = vec![0.0f64; n + 1];
    for i in 0..n {
        let u = times[i] - t0;
        let y = values[i];
        pu[i + 1] = pu[i] + u;
        py[i + 1] = py[i] + y;
        puu[i + 1] = puu[i] + u * u;
        puy[i + 1] = puy[i] + u * y;
        pyy[i + 1] = pyy[i] + y * y;
    }

    // OLS fit of the segment [a, b) from the prefix sums.
    // Returns (slope, sse), or None when the fit is invalid.
    let seg_fit = |a: usize, b: usize| -> Option<(f64, f64)> {
        let n_seg = (b - a) as f64;
        let su = pu[b] - pu[a];
        let sy = py[b] - py[a];
        let suu = puu[b] - puu[a];
        let suy = puy[b] - puy[a];
        let syy = pyy[b] - pyy[a];
        let denom = n_seg * suu - su * su;
        if !denom.is_finite() || denom <= 0.0 {
            return None;
        }
        let slope = (n_seg * suy - su * sy) / denom;
        let intercept = (sy - slope * su) / n_seg;
        let sse = (syy - intercept * sy - slope * suy).max(0.0);
        Some((slope, sse))
    };

    // One-line fit over the whole range.
    let (_, sse_one) = seg_fit(0, n)?;
    if sse_one <= 1e-12 {
        return None;
    }

    // Best two-line fit over all admissible split points.
    let mut best_total = f64::INFINITY;
    let mut best_k: Option<usize> = None;
    let mut best_left = 0.0f64;
    let mut best_right = 0.0f64;

    for k in cfg.min_segment..=n - cfg.min_segment {
        let (slope_l, sse_l) = match seg_fit(0, k) {
            Some(v) => v,
            None => continue,
        };
        let (slope_r, sse_r) = match seg_fit(k, n) {
            Some(v) => v,
            None => continue,
        };
        let total = sse_l + sse_r;
        if total < best_total {
            best_total = total;
            best_k = Some(k);
            best_left = slope_l;
            best_right = slope_r;
        }
    }

    let k_best = best_k?;
    let gain = (sse_one - best_total) / sse_one;
    if gain < cfg.min_train_gain {
        return None;
    }

    Some(FoundBreak {
        index: k_best,
        kind: BreakKind::Trend,
        train_gain: gain,
        left: best_left,
        right: best_right,
    })
}

pub fn variance_break(values: &[f64], cfg: &BreaksConfig) -> Option<FoundBreak> {
    if values.len() < 2 * cfg.min_segment {
        return None;
    }
    if values.iter().any(|v| !v.is_finite()) {
        return None;
    }

    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let sq: Vec<f64> = values.iter().map(|v| (v - mean) * (v - mean)).collect();

    let br = crate::regimes::single_break_l2(&sq, cfg.min_segment)?;

    if br.gain_fraction < cfg.min_train_gain {
        return None;
    }

    let pop_var = |seg: &[f64]| -> f64 {
        if seg.is_empty() {
            return 0.0;
        }
        let m = seg.len() as f64;
        let seg_mean = seg.iter().sum::<f64>() / m;
        seg.iter()
            .map(|v| (v - seg_mean) * (v - seg_mean))
            .sum::<f64>()
            / m
    };

    let left = pop_var(&values[..br.index]);
    let right = pop_var(&values[br.index..]);

    Some(FoundBreak {
        index: br.index,
        kind: BreakKind::Variance,
        train_gain: br.gain_fraction,
        left,
        right,
    })
}

use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;
use std::collections::BTreeMap;

fn time_at(col: &TypedCol, row: usize) -> Option<i64> {
    match col {
        TypedCol::Date { vals } => vals[row].map(i64::from),
        TypedCol::Datetime { vals } => vals[row],
        _ => None,
    }
}

fn numeric_at(col: &TypedCol, row: usize) -> Option<f64> {
    let v = match col {
        TypedCol::I64 { vals } => vals.get(row)?.map(|v| v as f64),
        TypedCol::F64 { vals } => vals.get(row)?,
        _ => None,
    }?;
    v.is_finite().then_some(v)
}

fn is_holdout(row: u32) -> bool {
    let mut x = row as u64 ^ 0xD16A_0057_5EED;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    (x ^ (x >> 31)).is_multiple_of(5)
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn pop_variance(values: &[f64], mean: f64) -> f64 {
    values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / values.len() as f64
}

fn ols(ts: &[f64], ys: &[f64]) -> Option<(f64, f64)> {
    if ts.len() < 2 {
        return None;
    }
    let n = ts.len() as f64;
    let st: f64 = ts.iter().sum();
    let sy: f64 = ys.iter().sum();
    let stt: f64 = ts.iter().map(|t| t * t).sum();
    let sty: f64 = ts.iter().zip(ys).map(|(t, y)| t * y).sum();
    let denom = n * stt - st * st;
    if denom <= 0.0 || !denom.is_finite() {
        return None;
    }
    let slope = (n * sty - st * sy) / denom;
    let intercept = (sy - slope * st) / n;
    Some((slope, intercept))
}

fn line_sse(ts: &[f64], ys: &[f64], slope: f64, intercept: f64) -> f64 {
    ts.iter()
        .zip(ys)
        .map(|(t, y)| {
            let d = y - (slope * t + intercept);
            d * d
        })
        .sum()
}

fn display_time(is_datetime: bool, t: i64) -> String {
    if is_datetime {
        onceover_core::dateutil::iso_datetime(t)
    } else {
        onceover_core::dateutil::iso_date(t)
    }
}

#[allow(clippy::too_many_arguments)]
fn make_finding(
    pattern: Pattern,
    column: &str,
    time_column: &str,
    claim: String,
    holdout_gain: f64,
    metrics: BTreeMap<String, serde_json::Value>,
    algorithm: &str,
    provenance: &str,
) -> Finding {
    Finding::new(
        pattern,
        BTreeMap::from([
            ("column".into(), json!(column)),
            ("time_column".into(), json!(time_column)),
        ]),
        claim,
        EvidenceBasis::Estimated,
        (0.6 + 0.25 * holdout_gain).min(0.9),
        metrics,
        Some(
            "check for changes in collection, processing, or business behavior around this date"
                .into(),
        ),
        Method::new("breaks", algorithm, provenance),
    )
}

fn level_findings(
    column: &str,
    time_column: &str,
    is_datetime: bool,
    train: &[(i64, f64)],
    holdout: &[(i64, f64)],
    cfg: &BreaksConfig,
) -> Vec<Finding> {
    level_findings_grouped(column, time_column, is_datetime, train, holdout, cfg, None)
}

fn level_findings_grouped(
    column: &str,
    time_column: &str,
    is_datetime: bool,
    train: &[(i64, f64)],
    holdout: &[(i64, f64)],
    cfg: &BreaksConfig,
    group: Option<(&str, &str)>,
) -> Vec<Finding> {
    let train_values: Vec<f64> = train.iter().map(|&(_, v)| v).collect();
    let breaks = level_breaks(&train_values, cfg);
    if breaks.is_empty() {
        return Vec::new();
    }

    let boundaries: Vec<i64> = breaks.iter().map(|b| train[b.index].0).collect();

    let n_segments = breaks.len() + 1;
    let mut seg_means: Vec<f64> = Vec::with_capacity(n_segments);
    for j in 0..n_segments {
        let start = if j == 0 { 0 } else { breaks[j - 1].index };
        let end = if j < breaks.len() {
            breaks[j].index
        } else {
            train_values.len()
        };
        seg_means.push(mean(&train_values[start..end]));
    }

    let assignment: Vec<usize> = holdout
        .iter()
        .map(|&(t, _)| boundaries.iter().filter(|&&bt| t >= bt).count())
        .collect();

    let mut holdout_counts = vec![0usize; n_segments];
    for &seg in &assignment {
        holdout_counts[seg] += 1;
    }
    if holdout_counts.iter().any(|&c| c < 10) {
        return Vec::new();
    }

    let global_mean = mean(&train_values);
    let mut sse_base = 0.0;
    let mut sse_piece = 0.0;
    for (&seg, &(_, y)) in assignment.iter().zip(holdout.iter()) {
        sse_base += (y - global_mean) * (y - global_mean);
        let m = seg_means[seg];
        sse_piece += (y - m) * (y - m);
    }
    if sse_base <= 1e-12 {
        return Vec::new();
    }
    let gain = (sse_base - sse_piece) / sse_base;
    if gain < cfg.min_holdout_gain {
        return Vec::new();
    }

    let provenance = if group.is_some() {
        "binary segmentation over one-break L2 scans within a subgroup; holdout validation (onceover)"
    } else {
        "binary segmentation over one-break L2 scans; holdout validation (onceover)"
    };

    let mut findings = Vec::with_capacity(breaks.len());
    for j in 0..breaks.len() {
        let display = display_time(is_datetime, boundaries[j]);
        let claim = if let Some((gcol, gval)) = group {
            format!(
                "{} level shifts after {} along {} within {} = {}: mean {} before, {} after; the training-selected segmentation reduces holdout squared error by {:.1}%",
                column,
                display,
                time_column,
                gcol,
                gval,
                crate::fmt_float(seg_means[j]),
                crate::fmt_float(seg_means[j + 1]),
                100.0 * gain
            )
        } else {
            format!(
                "{} level shifts after {} along {}: mean {} before, {} after; the training-selected segmentation reduces holdout squared error by {:.1}%",
                column,
                display,
                time_column,
                crate::fmt_float(seg_means[j]),
                crate::fmt_float(seg_means[j + 1]),
                100.0 * gain
            )
        };
        let mut metrics = BTreeMap::from([
            ("breakpoint".into(), json!(display)),
            ("breakpoint_raw".into(), json!(boundaries[j])),
            ("mean_before".into(), json!(seg_means[j])),
            ("mean_after".into(), json!(seg_means[j + 1])),
            ("train_gain".into(), json!(breaks[j].train_gain)),
            ("holdout_error_reduction".into(), json!(gain)),
            ("n_breaks".into(), json!(breaks.len())),
            ("n_train".into(), json!(train.len())),
            ("n_holdout".into(), json!(holdout.len())),
            (
                "validation".into(),
                json!("breakpoints and segment means learned on training rows; fixed segmentation evaluated on holdout; no p-value")
            ),
        ]);
        if let Some((gcol, gval)) = group {
            metrics.insert("group_column".into(), json!(gcol));
            metrics.insert("group_value".into(), json!(gval));
        }
        findings.push(make_finding(
            Pattern::LevelShift,
            column,
            time_column,
            claim,
            gain,
            metrics,
            "binary_segmentation",
            provenance,
        ));
    }
    findings
}

fn trend_finding(
    column: &str,
    time_column: &str,
    is_datetime: bool,
    train: &[(i64, f64)],
    holdout: &[(i64, f64)],
    cfg: &BreaksConfig,
) -> Option<Finding> {
    let train_values: Vec<f64> = train.iter().map(|(_, v)| *v).collect();
    let train_times_raw: Vec<f64> = train.iter().map(|(t, _)| *t as f64).collect();
    let tb = trend_break(&train_values, &train_times_raw, cfg)?;

    let t0 = train[0].0;
    let train_ts: Vec<f64> = train.iter().map(|(t, _)| (*t - t0) as f64).collect();

    let (slope_all, int_all) = ols(&train_ts, &train_values)?;
    let (slope_l, int_l) = ols(&train_ts[..tb.index], &train_values[..tb.index])?;
    let (slope_r, int_r) = ols(&train_ts[tb.index..], &train_values[tb.index..])?;

    let boundary = train[tb.index].0;

    let mut hold_all_ts: Vec<f64> = Vec::with_capacity(holdout.len());
    let mut hold_all_vals: Vec<f64> = Vec::with_capacity(holdout.len());
    let mut hold_left_ts: Vec<f64> = Vec::new();
    let mut hold_left_vals: Vec<f64> = Vec::new();
    let mut hold_right_ts: Vec<f64> = Vec::new();
    let mut hold_right_vals: Vec<f64> = Vec::new();

    for (t, v) in holdout {
        let ts = (*t - t0) as f64;
        hold_all_ts.push(ts);
        hold_all_vals.push(*v);
        if *t < boundary {
            hold_left_ts.push(ts);
            hold_left_vals.push(*v);
        } else {
            hold_right_ts.push(ts);
            hold_right_vals.push(*v);
        }
    }

    if hold_left_ts.len() < 10 || hold_right_ts.len() < 10 {
        return None;
    }

    let sse_base = line_sse(&hold_all_ts, &hold_all_vals, slope_all, int_all);
    let sse_piece = line_sse(&hold_left_ts, &hold_left_vals, slope_l, int_l)
        + line_sse(&hold_right_ts, &hold_right_vals, slope_r, int_r);

    if sse_base <= 1e-12 {
        return None;
    }
    let gain = (sse_base - sse_piece) / sse_base;
    if gain < cfg.min_holdout_gain {
        return None;
    }

    let display = display_time(is_datetime, boundary);
    let claim = format!(
        "{} trend changes after {} along {}: slope {} before, {} after; the training-selected two-line model reduces holdout squared error by {:.1}%",
        column,
        display,
        time_column,
        crate::fmt_float(slope_l),
        crate::fmt_float(slope_r),
        100.0 * gain
    );

    let metrics = BTreeMap::from([
        ("breakpoint".into(), json!(display)),
        ("breakpoint_raw".into(), json!(boundary)),
        ("slope_before".into(), json!(slope_l)),
        ("slope_after".into(), json!(slope_r)),
        ("train_gain".into(), json!(tb.train_gain)),
        ("holdout_error_reduction".into(), json!(gain)),
        ("n_train".into(), json!(train.len())),
        ("n_holdout".into(), json!(holdout.len())),
        (
            "validation".into(),
            json!("breakpoint and slopes learned on training rows; fixed two-line model evaluated on holdout; no p-value"),
        ),
    ]);

    Some(make_finding(
        Pattern::TrendChange,
        column,
        time_column,
        claim,
        gain,
        metrics,
        "piecewise_linear_scan",
        "prefix-sum two-line OLS scan; holdout validation (onceover)",
    ))
}

fn variance_finding(
    column: &str,
    time_column: &str,
    is_datetime: bool,
    train: &[(i64, f64)],
    holdout: &[(i64, f64)],
    cfg: &BreaksConfig,
) -> Option<Finding> {
    let train_values: Vec<f64> = train.iter().map(|&(_, v)| v).collect();
    let vb = variance_break(&train_values, cfg)?;

    let boundary = train[vb.index].0;
    let left = &train_values[..vb.index];
    let right = &train_values[vb.index..];
    let m_l = mean(left);
    let m_r = mean(right);
    let v_l = pop_variance(left, m_l);
    let v_r = pop_variance(right, m_r);
    let m_g = mean(&train_values);
    let v_g = pop_variance(&train_values, m_g);

    let mut left_holdout: Vec<f64> = Vec::new();
    let mut right_holdout: Vec<f64> = Vec::new();
    for &(t, y) in holdout {
        if t < boundary {
            left_holdout.push(y);
        } else {
            right_holdout.push(y);
        }
    }
    if left_holdout.len() < 10 || right_holdout.len() < 10 {
        return None;
    }

    let sse_base: f64 = holdout
        .iter()
        .map(|&(_, y)| {
            let d = (y - m_g) * (y - m_g) - v_g;
            d * d
        })
        .sum();
    let sse_piece: f64 = left_holdout
        .iter()
        .map(|&y| {
            let d = (y - m_l) * (y - m_l) - v_l;
            d * d
        })
        .sum::<f64>()
        + right_holdout
            .iter()
            .map(|&y| {
                let d = (y - m_r) * (y - m_r) - v_r;
                d * d
            })
            .sum::<f64>();

    if sse_base <= 1e-12 {
        return None;
    }
    let gain = (sse_base - sse_piece) / sse_base;
    if gain < cfg.min_holdout_gain {
        return None;
    }

    let display = display_time(is_datetime, boundary);
    let claim = format!(
        "{} variance changes after {} along {}: variance {} before, {} after; the training-selected split reduces holdout squared-deviation error by {:.1}%",
        column,
        display,
        time_column,
        crate::fmt_float(v_l),
        crate::fmt_float(v_r),
        100.0 * gain
    );

    let metrics = BTreeMap::from([
        ("breakpoint".into(), json!(display)),
        ("breakpoint_raw".into(), json!(boundary)),
        ("variance_before".into(), json!(v_l)),
        ("variance_after".into(), json!(v_r)),
        ("train_gain".into(), json!(vb.train_gain)),
        ("holdout_error_reduction".into(), json!(gain)),
        ("n_train".into(), json!(train.len())),
        ("n_holdout".into(), json!(holdout.len())),
        (
            "validation".into(),
            json!("breakpoint, segment means and variances learned on training rows; fixed split evaluated on holdout; no p-value"),
        ),
    ]);

    Some(make_finding(
        Pattern::VarianceChange,
        column,
        time_column,
        claim,
        gain,
        metrics,
        "squared_deviation_scan",
        "one-break L2 scan of squared deviations; holdout validation (onceover)",
    ))
}

pub fn profile_breaks(
    table: &TypedTable,
    excluded: &[usize],
    cfg: &BreaksConfig,
) -> (Vec<Finding>, String) {
    let time_col = table
        .cols
        .iter()
        .position(|c| matches!(c, TypedCol::Date { .. } | TypedCol::Datetime { .. }));
    let time_col = match time_col {
        Some(c) => c,
        None => {
            return (
                Vec::new(),
                "no date or datetime column; break scan skipped".to_string(),
            )
        }
    };

    let is_datetime = matches!(table.cols[time_col], TypedCol::Datetime { .. });
    let time_name = table.names[time_col].as_str();

    let mut timed: Vec<(i64, u32)> = (0..table.n)
        .filter_map(|r| time_at(&table.cols[time_col], r).map(|t| (t, r as u32)))
        .collect();
    timed.sort_by_key(|&(t, _)| t);

    if timed.len() > cfg.row_cap {
        let pos = crate::stride_rows(timed.len(), cfg.row_cap);
        timed = pos.iter().map(|&p| timed[p as usize]).collect();
    }

    let series: Vec<usize> = (0..table.cols.len())
        .filter(|&i| table.cols[i].is_numeric() && i != time_col && !excluded.contains(&i))
        .collect();
    let eligible = series.len();
    let scanned_count = eligible.min(cfg.series_cap);

    let mut findings: Vec<Finding> = Vec::new();
    let mut skipped = 0usize;

    for col in series.iter().take(cfg.series_cap) {
        let name = table.names[*col].as_str();
        let points: Vec<(i64, u32, f64)> = timed
            .iter()
            .filter_map(|&(t, r)| numeric_at(&table.cols[*col], r as usize).map(|v| (t, r, v)))
            .collect();
        if points.len() < 2 * cfg.min_segment + 40 {
            skipped += 1;
            continue;
        }
        findings.extend(excursion_findings(
            name,
            time_name,
            is_datetime,
            &points,
            cfg,
        ));
        let train: Vec<(i64, f64)> = points
            .iter()
            .filter(|&&(_, r, _)| !is_holdout(r))
            .map(|&(t, _, v)| (t, v))
            .collect();
        let holdout: Vec<(i64, f64)> = points
            .iter()
            .filter(|&&(_, r, _)| is_holdout(r))
            .map(|&(t, _, v)| (t, v))
            .collect();
        if train.len() < 2 * cfg.min_segment || holdout.len() < 20 {
            skipped += 1;
            continue;
        }
        let level_fs = level_findings(name, time_name, is_datetime, &train, &holdout, cfg);
        let had_global_level = !level_fs.is_empty();
        findings.extend(level_fs);
        if let Some(f) = trend_finding(name, time_name, is_datetime, &train, &holdout, cfg) {
            findings.push(f);
        }
        if let Some(f) = variance_finding(name, time_name, is_datetime, &train, &holdout, cfg) {
            findings.push(f);
        }
        findings.extend(subgroup_findings(
            table,
            time_name,
            is_datetime,
            name,
            &points,
            had_global_level,
            cfg,
        ));
    }

    let coverage = format!(
        "{} of {} numeric series scanned along {} over {} time-ordered rows, {} skipped, {} findings; training-selected, holdout-validated",
        scanned_count,
        eligible,
        time_name,
        timed.len(),
        skipped,
        findings.len()
    );

    (findings, coverage)
}

fn group_key(col: &TypedCol, row: usize) -> Option<String> {
    match col {
        TypedCol::Str { vals } => vals.get(row).copied().flatten().map(str::to_owned),
        TypedCol::StrOwned { vals } => vals.get(row).cloned().flatten(),
        _ => None,
    }
}

fn subgroup_findings(
    table: &TypedTable,
    time_column: &str,
    is_datetime: bool,
    series_name: &str,
    points: &[(i64, u32, f64)],
    had_global_level: bool,
    cfg: &BreaksConfig,
) -> Vec<Finding> {
    if cfg.max_groups == 0 || had_global_level {
        return Vec::new();
    }
    let mut findings: Vec<Finding> = Vec::new();
    let mut used = 0usize;
    for (gi, gcol) in table.cols.iter().enumerate() {
        if used >= cfg.max_groups {
            break;
        }
        if !matches!(gcol, TypedCol::Str { .. } | TypedCol::StrOwned { .. }) {
            continue;
        }
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (pos, &(_, r, _)) in points.iter().enumerate() {
            if let Some(k) = group_key(gcol, r as usize) {
                groups.entry(k).or_default().push(pos);
            }
        }
        if groups.len() < 2 || groups.len() > cfg.max_group_levels {
            continue;
        }
        used += 1;
        let gname = table.names[gi].as_str();
        for (key, positions) in &groups {
            if positions.len() < cfg.min_group_rows {
                continue;
            }
            let gtrain: Vec<(i64, f64)> = positions
                .iter()
                .copied()
                .filter(|&p| !is_holdout(points[p].1))
                .map(|p| (points[p].0, points[p].2))
                .collect();
            let ghost: Vec<(i64, f64)> = positions
                .iter()
                .copied()
                .filter(|&p| is_holdout(points[p].1))
                .map(|p| (points[p].0, points[p].2))
                .collect();
            if gtrain.len() < 2 * cfg.min_segment || ghost.len() < 20 {
                continue;
            }
            findings.extend(level_findings_grouped(
                series_name,
                time_column,
                is_datetime,
                &gtrain,
                &ghost,
                cfg,
                Some((gname, key.as_str())),
            ));
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Lcg(u64);

    impl Lcg {
        fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545F4914F6CDD1D)
        }

        fn next_f64(&mut self) -> f64 {
            (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
        }

        fn noise(&mut self, scale: f64) -> f64 {
            (self.next_f64() + self.next_f64() + self.next_f64() - 1.5) * scale
        }
    }

    fn test_cfg() -> BreaksConfig {
        BreaksConfig {
            min_segment: 10,
            min_train_gain: 0.10,
            ..Default::default()
        }
    }

    #[test]
    fn level_shift_found_at_known_index() {
        let mut rng = Lcg(42);
        let values: Vec<f64> = (0..100)
            .map(|i| {
                if i < 50 {
                    rng.noise(0.6)
                } else {
                    5.0 + rng.noise(0.6)
                }
            })
            .collect();
        let breaks = level_breaks(&values, &test_cfg());
        assert_eq!(breaks.len(), 1);
        assert!((breaks[0].index as i64 - 50).abs() <= 3);
        assert!((breaks[0].left - 0.0).abs() < 0.5);
        assert!((breaks[0].right - 5.0).abs() < 0.5);
        assert!(breaks[0].kind == BreakKind::Level);
    }

    #[test]
    fn two_level_shifts_found() {
        let mut rng = Lcg(7);
        let values: Vec<f64> = (0..150)
            .map(|i| {
                let level = if i < 50 {
                    0.0
                } else if i < 100 {
                    4.0
                } else {
                    1.0
                };
                level + rng.noise(0.6)
            })
            .collect();
        let breaks = level_breaks(&values, &test_cfg());
        assert_eq!(breaks.len(), 2);
        assert!((breaks[0].index as i64 - 50).abs() <= 3);
        assert!((breaks[1].index as i64 - 100).abs() <= 3);
    }

    #[test]
    fn noise_only_yields_no_level_break() {
        let cfg = BreaksConfig {
            min_train_gain: 0.25,
            ..test_cfg()
        };
        let mut rng = Lcg(99);
        let values: Vec<f64> = (0..200).map(|_| rng.noise(2.0)).collect();
        assert!(level_breaks(&values, &cfg).is_empty());
    }

    #[test]
    fn trend_change_found() {
        let mut rng = Lcg(3);
        let times: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let values: Vec<f64> = (0..100)
            .map(|i| {
                let t = i as f64;
                if i < 50 {
                    t + rng.noise(1.2)
                } else {
                    100.0 - t + rng.noise(1.2)
                }
            })
            .collect();
        let br = trend_break(&values, &times, &test_cfg()).expect("trend break");
        assert!((br.index as i64 - 50).abs() <= 4);
        assert!(br.left > 0.5);
        assert!(br.right < -0.5);
        assert!(br.kind == BreakKind::Trend);
    }

    #[test]
    fn variance_change_found() {
        let mut rng = Lcg(11);
        let values: Vec<f64> = (0..100)
            .map(|i| {
                if i < 50 {
                    rng.noise(0.7)
                } else {
                    rng.noise(4.2)
                }
            })
            .collect();
        let br = variance_break(&values, &test_cfg()).expect("variance break");
        assert!((br.index as i64 - 50).abs() <= 6);
        assert!(br.right > 4.0 * br.left);
        assert!(br.kind == BreakKind::Variance);
    }

    #[test]
    fn constant_series_yields_nothing() {
        let values = vec![3.0; 100];
        let times: Vec<f64> = (0..100).map(|i| i as f64).collect();
        assert!(level_breaks(&values, &test_cfg()).is_empty());
        assert!(variance_break(&values, &test_cfg()).is_none());
        assert!(trend_break(&values, &times, &test_cfg()).is_none());
    }

    #[test]
    fn empty_and_short_inputs() {
        let cfg = test_cfg();
        assert!(level_breaks(&[], &cfg).is_empty());
        assert!(level_breaks(&[1.0, 2.0, 3.0], &cfg).is_empty());
        assert!(variance_break(&[1.0, 2.0], &cfg).is_none());
        assert!(trend_break(&[1.0, 2.0], &[0.0], &cfg).is_none());
    }
}

// ---------------------------------------------------------------------
// Transient excursions (issue #32): short deviations from a local
// baseline, grouped into events. The level/trend machinery above
// targets sustained shifts; the labeled events in the corpus's
// anomaly pack are excursions of a few points to a few days that the
// sustained-shift scans do not report.

fn median_of(vals: &[f64]) -> f64 {
    if vals.is_empty() {
        return 0.0;
    }
    let mut v: Vec<f64> = vals.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = v.len() / 2;
    if v.len() % 2 == 1 {
        v[mid]
    } else {
        0.5 * (v[mid - 1] + v[mid])
    }
}

/// Points per week implied by the median interval between
/// consecutive timestamps, when the cadence divides a week evenly.
fn weekly_period(points: &[(i64, u32, f64)], is_datetime: bool) -> Option<usize> {
    if points.len() < 3 {
        return None;
    }
    let mut diffs: Vec<f64> = points
        .windows(2)
        .filter_map(|w| {
            let d = w[1].0 - w[0].0;
            (d > 0).then_some(d as f64)
        })
        .collect();
    if diffs.is_empty() {
        return None;
    }
    diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let step = diffs[diffs.len() / 2];
    let week = if is_datetime {
        7.0 * 86_400.0 * 1_000_000.0
    } else {
        7.0
    };
    let period = week / step;
    let rounded = period.round();
    if rounded >= 48.0 && (period - rounded).abs() < 0.01 {
        Some(rounded as usize)
    } else {
        None
    }
}

struct Excursion {
    start: usize,
    end: usize,
    peak: usize,
    peak_z: f64,
}

fn excursion_scan(
    points: &[(i64, u32, f64)],
    is_datetime: bool,
    cfg: &BreaksConfig,
) -> Vec<Excursion> {
    let n = points.len();
    if n < 50 {
        return Vec::new();
    }
    let ys: Vec<f64> = points.iter().map(|p| p.2).collect();
    // Baseline: seasonal (same time of week in neighbouring weeks)
    // when the cadence supports it and the series spans at least 2.5
    // periods; otherwise a centered rolling median. Seasonal
    // neighbours are matched by timestamp, not by index: the series
    // may be a stride sample of the full grid or have gaps, and an
    // index offset drifts out of phase as soon as a row is missing.
    let period = weekly_period(points, is_datetime).filter(|&p| (n as f64) >= 2.5 * p as f64);
    let residuals: Vec<f64> = match period {
        Some(p) => {
            let times: Vec<i64> = points.iter().map(|pt| pt.0).collect();
            let week: i64 = if is_datetime {
                7 * 86_400 * 1_000_000
            } else {
                7
            };
            let step = week / p as i64;
            (0..n)
                .map(|i| {
                    let mut neigh: Vec<f64> = Vec::new();
                    for k in 1..=3i64 {
                        for sgn in [1i64, -1] {
                            let target = times[i] + sgn * k * week;
                            let idx = times.partition_point(|&t| t < target);
                            for cand in [idx.saturating_sub(1), idx.min(n - 1)] {
                                if (times[cand] - target).abs() <= step / 2 && cand != i {
                                    neigh.push(ys[cand]);
                                    break;
                                }
                            }
                        }
                    }
                    if neigh.len() >= 2 {
                        ys[i] - median_of(&neigh)
                    } else {
                        0.0
                    }
                })
                .collect()
        }
        None => {
            let half = cfg.excursion_window / 2;
            (0..n)
                .map(|i| {
                    let lo = i.saturating_sub(half);
                    let hi = (i + half + 1).min(n);
                    ys[i] - median_of(&ys[lo..hi])
                })
                .collect()
        }
    };
    let abs_res: Vec<f64> = residuals.iter().map(|r| r.abs()).collect();
    let mad = median_of(&abs_res);
    let mut scale = 1.4826 * mad;
    if scale <= 0.0 {
        // Degenerate scale: at least half the series sits exactly on
        // its baseline (quantized or constant stretches). Fall back
        // to 1% of the series' central range so a genuine excursion
        // in such a series still registers; a constant series has
        // range 0 and is skipped below.
        let mut sorted = ys.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p = |q: f64| sorted[((q * (n - 1) as f64) as usize).min(n - 1)];
        scale = 0.01 * (p(0.99) - p(0.01));
    }
    if scale <= 0.0 {
        return Vec::new();
    }
    let flagged: Vec<bool> = residuals
        .iter()
        .map(|r| r.abs() / scale >= cfg.excursion_z)
        .collect();
    let max_span = match period {
        Some(p) => p,
        None => cfg.excursion_window / 2,
    };
    let mut events: Vec<Excursion> = Vec::new();
    let mut i = 0usize;
    while i < n {
        if !flagged[i] {
            i += 1;
            continue;
        }
        let mut last = i;
        let mut j = i;
        while j < n {
            if flagged[j] {
                last = j;
            } else if j - last > cfg.excursion_merge_gap {
                break;
            }
            j += 1;
        }
        if last - i < max_span {
            let peak = (i..=last)
                .max_by(|&a, &b| {
                    residuals[a]
                        .abs()
                        .partial_cmp(&residuals[b].abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or(i);
            events.push(Excursion {
                start: i,
                end: last,
                peak,
                peak_z: residuals[peak] / scale,
            });
        }
        i = last + 1;
    }
    events.sort_by(|a, b| {
        b.peak_z
            .abs()
            .partial_cmp(&a.peak_z.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    events.truncate(cfg.excursion_max_events);
    events
}

fn excursion_findings(
    column: &str,
    time_column: &str,
    is_datetime: bool,
    points: &[(i64, u32, f64)],
    cfg: &BreaksConfig,
) -> Vec<Finding> {
    excursion_scan(points, is_datetime, cfg)
        .iter()
        .map(|ev| {
            let start = display_time(is_datetime, points[ev.start].0);
            let end = display_time(is_datetime, points[ev.end].0);
            let peak_t = display_time(is_datetime, points[ev.peak].0);
            let peak_v = points[ev.peak].2;
            let direction = if ev.peak_z >= 0.0 { "spike" } else { "dip" };
            let claim = format!(
                "{} {} between {} and {}: peak value {} at {}, {:.1} robust z from the local baseline",
                column,
                if direction == "spike" { "spikes" } else { "dips" },
                start,
                end,
                crate::fmt_float(peak_v),
                peak_t,
                ev.peak_z.abs(),
            );
            let mut metrics: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            metrics.insert("event_start".into(), json!(start));
            metrics.insert("event_end".into(), json!(end));
            metrics.insert("peak_time".into(), json!(peak_t));
            metrics.insert("peak_value".into(), json!(peak_v));
            metrics.insert("peak_z".into(), json!(ev.peak_z));
            metrics.insert("direction".into(), json!(direction));
            metrics.insert(
                "duration_points".into(),
                json!((ev.end - ev.start + 1) as u64),
            );
            metrics.insert("n".into(), json!(points.len() as u64));
            Finding::new(
                Pattern::TransientExcursion,
                BTreeMap::from([
                    ("column".into(), json!(column)),
                    ("time_column".into(), json!(time_column)),
                ]),
                claim,
                EvidenceBasis::Estimated,
                (0.55 + 0.02 * ev.peak_z.abs()).min(0.9),
                metrics,
                Some(
                    "check for incidents, holidays, outages, or collection changes in this window"
                        .into(),
                ),
                Method::new(
                    "breaks",
                    "transient excursion vs local baseline",
                    "onceover transient events v1 (issue #32)",
                ),
            )
        })
        .collect()
}

#[cfg(test)]
mod excursion_tests {
    use super::*;

    fn cfg() -> BreaksConfig {
        BreaksConfig::default()
    }

    fn series(ys: &[f64]) -> Vec<(i64, u32, f64)> {
        ys.iter()
            .enumerate()
            .map(|(i, &v)| (i as i64 * 300_000_000, i as u32, v))
            .collect()
    }

    #[test]
    fn planted_dip_is_an_event() {
        let mut ys = vec![100.0; 600];
        for (k, v) in ys.iter_mut().enumerate().skip(300).take(6) {
            let _ = k;
            *v = 40.0;
        }
        let ev = excursion_scan(&series(&ys), true, &cfg());
        assert_eq!(ev.len(), 1);
        assert!(ev[0].peak_z < 0.0);
        assert!((300..306).contains(&ev[0].peak));
    }

    #[test]
    fn level_shift_is_not_an_excursion() {
        let mut ys = vec![100.0; 600];
        for v in ys.iter_mut().skip(300) {
            *v = 160.0;
        }
        let ev = excursion_scan(&series(&ys), true, &cfg());
        assert!(
            ev.is_empty(),
            "sustained shift must not flag: {} events",
            ev.len()
        );
    }

    #[test]
    fn constant_series_has_no_events() {
        let ys = vec![7.0; 600];
        assert!(excursion_scan(&series(&ys), true, &cfg()).is_empty());
    }
}

#[cfg(test)]
mod excursion_debug_tests {
    use super::*;

    #[test]
    fn seasonal_series_with_dip() {
        // Weekly-seasonal series at 30-min cadence (336 pts/week),
        // 30 weeks, with a 1-day demand collapse in week 20.
        let mut ys: Vec<f64> = Vec::new();
        for i in 0..(336 * 30) {
            let tod = (i % 48) as f64;
            let dow = ((i / 48) % 7) as f64;
            let daily = 1000.0 + 400.0 * ((tod / 48.0) * std::f64::consts::TAU).sin().abs();
            let weekly = if dow >= 5.0 { 0.6 } else { 1.0 };
            ys.push(daily * weekly + (i % 13) as f64);
        }
        for v in ys.iter_mut().skip(336 * 20 + 100).take(48) {
            *v *= 0.3;
        }
        let pts: Vec<(i64, u32, f64)> = ys
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as i64 * 1_800_000_000, i as u32, v))
            .collect();
        let ev = excursion_scan(&pts, true, &BreaksConfig::default());
        assert!(!ev.is_empty(), "expected the planted collapse to be found");
        assert!(ev
            .iter()
            .any(|e| (336 * 20 + 90..336 * 20 + 160).contains(&e.peak)));
    }
}
