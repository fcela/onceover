//! What a measure does along the time axis, and whether the axis is complete.
//!
//! The change passes look for moments when a series departs from itself. This
//! pass describes the series as a whole: absent steps on the time axis, a
//! trend, a repeating cycle, values that persist from one step to the next, and
//! grouping levels that appear, disappear or shift share over time.
//!
//! A cycle or persistence measured on a series with a trend or level shift
//! would mostly echo that structure, so both are measured after removing it.

use std::collections::{BTreeMap, HashMap};

use onceover_core::{dateutil, TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use rayon::prelude::*;
use serde_json::{json, Value};

use crate::regimes::single_break_l2;
use crate::surprises::{code_column, Coded, SurpriseConfig};

const MIN_POINTS: usize = 60;
const MAX_POINTS: usize = 20_000;
const MAX_MEASURES: usize = 256;
const MICROS_PER_DAY: i64 = 86_400_000_000;
/// The axis is a grid only when most steps share one length.
const MIN_MODAL_STEP_SHARE: f64 = 0.5;
const MIN_ABSENT_SHARE: f64 = 0.001;
/// Share of variance a straight line must explain to be called a trend.
const MIN_TREND_R2: f64 = 0.3;
const MIN_LINE_OVER_STEP: f64 = 0.02;
/// A trend is stated only when the slope also clears this bar under
/// the autocorrelation-deflated t-test ([`trend_test`]). The bar is
/// stricter than the usual 0.01: the deflated t is approximate
/// under near-unit-root residuals, and the pass tests many series
/// per report with no other multiplicity control.
const TREND_ALPHA: f64 = 0.001;
/// Shortest per-entity series the trend test runs on: below twelve
/// points the deflated test has no power worth reporting.
const MIN_ENTITY_TREND_POINTS: usize = 12;
/// Shortest per-entity series the level-break scan runs on.
const MIN_ENTITY_BREAK_POINTS: usize = 24;
/// The per-entity pass reports only when at least this many entities
/// could be tested.
const MIN_ENTITY_TESTED: usize = 10;
/// A per-entity pattern is stated when at least this many entities
/// carry it.
const MIN_ENTITY_CARRIERS: usize = 3;
/// Minimum segment length of a per-entity level break.
const ENTITY_BREAK_MIN_SEGMENT: usize = 6;
/// A per-entity level break must explain this share of the entity
/// series' variance. There is no holdout at entity level, so the bar
/// sits above the aggregate pass's training-gain bar.
const ENTITY_BREAK_MIN_GAIN: f64 = 0.25;
/// Entity names spelled out in a claim before the rest are counted.
const MAX_NAMED_ENTITIES: usize = 5;
/// Structure explaining this share of variance is removed before measuring
/// cycles and persistence.
const REMOVE_STRUCTURE_R2: f64 = 0.3;
const MIN_CYCLE_ACF: f64 = 0.3;
const MIN_CYCLE_PROMINENCE: f64 = 0.2;
const MAX_SCANNED_LAG: usize = 400;
const MIN_PERSISTENCE_ACF: f64 = 0.5;
/// Autocorrelation must also clear this many standard errors of white noise.
const ACF_NOISE_MULTIPLE: f64 = 4.0;
const MIN_NEW_LEVEL_ROWS: usize = 20;
const MIN_NEW_LEVEL_SHARE: f64 = 0.005;
/// Absence before a level's first row must be this improbable at its later
/// rate: expected rows in the empty stretch.
const MIN_EXPECTED_ABSENT_ROWS: f64 = 10.0;
/// A level must be absent for this share of the rows to have arrived or left.
const MIN_ABSENT_STRETCH: f64 = 0.1;
const MIN_SHARE_SHIFT: f64 = 0.2;
/// Kendall tau of a level's share across the time slices: a real shift keeps
/// going one way, while a seasonal one comes back.
const MIN_SHIFT_MONOTONICITY: f64 = 0.6;
const FAMILY_ALPHA: f64 = 0.01;
const TIME_SLICES: usize = 10;

/// The time axis, as microsecond ticks aligned with the table's rows.
pub struct SeriesInput<'a> {
    pub axis_name: &'a str,
    pub ticks: &'a [Option<i64>],
    pub measures: Vec<usize>,
    pub dimensions: Vec<usize>,
    /// Entity columns of a panel grain, in the pipeline's priority
    /// order. The first is the grain the per-entity pass reads the
    /// principal measure against; empty when the frame has no
    /// entity × time grain.
    pub entities: Vec<usize>,
}

fn tick_label(micros: i64) -> String {
    if micros.rem_euclid(MICROS_PER_DAY) == 0 {
        dateutil::iso_date(micros.div_euclid(MICROS_PER_DAY))
    } else {
        dateutil::iso_datetime(micros)
    }
}

/// A step length as a reader says it.
fn duration_label(micros: i64) -> String {
    let plural = |n: i64, unit: &str| {
        if n == 1 {
            format!("1 {unit}")
        } else {
            format!("{n} {unit}s")
        }
    };
    let (hour, minute, second) = (3_600_000_000, 60_000_000, 1_000_000);
    if micros % MICROS_PER_DAY == 0 {
        plural(micros / MICROS_PER_DAY, "day")
    } else if micros % hour == 0 {
        plural(micros / hour, "hour")
    } else if micros % minute == 0 {
        plural(micros / minute, "minute")
    } else if micros % second == 0 {
        plural(micros / second, "second")
    } else {
        format!("{} ms", micros / 1000)
    }
}

/// Calendar months between two ticks when both start a month, else None.
fn months_apart(a: i64, b: i64) -> Option<i64> {
    let (ya, ma, da) = dateutil::civil_from_days(a.div_euclid(MICROS_PER_DAY));
    let (yb, mb, db) = dateutil::civil_from_days(b.div_euclid(MICROS_PER_DAY));
    (da == 1 && db == 1).then(|| (yb - ya) * 12 + mb as i64 - ma as i64)
}

/// Absent steps on the axis, when its values sit on a regular grid.
fn axis_gaps(input: &SeriesInput, sorted: &[i64]) -> Option<Finding> {
    if sorted.len() < MIN_POINTS {
        return None;
    }
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for w in sorted.windows(2) {
        *counts.entry(w[1] - w[0]).or_default() += 1;
    }
    let (&modal, &modal_count) = counts
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))?;
    let steps = sorted.len() - 1;
    let monthly = (28 * MICROS_PER_DAY..=31 * MICROS_PER_DAY).contains(&modal)
        && sorted.iter().all(|&t| months_apart(t, sorted[0]).is_some());
    let on_grid = if monthly {
        true
    } else {
        modal_count as f64 / steps as f64 >= MIN_MODAL_STEP_SHARE
    };
    if !on_grid || modal <= 0 {
        return None;
    }
    let daily = modal == MICROS_PER_DAY;
    let mut absent = 0i64;
    let mut longest: Option<(i64, i64, i64)> = None;
    for w in sorted.windows(2) {
        let (a, b) = (w[0], w[1]);
        let missing = if monthly {
            months_apart(a, b).unwrap_or(1) - 1
        } else {
            (b - a) / modal - 1
        };
        if missing <= 0 {
            continue;
        }
        // Weekdays-only series skip weekends by design.
        if daily {
            let first = a.div_euclid(MICROS_PER_DAY) + 1;
            if (first..first + missing).all(dateutil::is_weekend) {
                continue;
            }
        }
        absent += missing;
        if longest.is_none_or(|(m, _, _)| missing > m) {
            longest = Some((missing, a, b));
        }
    }
    let (longest_steps, from, to) = longest?;
    let expected = sorted.len() as i64 + absent;
    let share = absent as f64 / expected as f64;
    if share < MIN_ABSENT_SHARE {
        return None;
    }
    let step = if monthly {
        "1 month".to_string()
    } else {
        duration_label(modal)
    };
    Some(Finding::new(
        Pattern::Gap,
        BTreeMap::from([("column".to_string(), json!(input.axis_name))]),
        format!(
            "the time axis {} is missing {absent} of {expected} steps of {step} ({:.2}%); the longest gap is {longest_steps} {}, between {} and {}",
            input.axis_name,
            100.0 * share,
            if longest_steps == 1 { "step" } else { "steps" },
            tick_label(from),
            tick_label(to),
        ),
        EvidenceBasis::Measured,
        0.72 + 0.2 * (share / 0.25).min(1.0),
        BTreeMap::from([
            ("absent_steps".to_string(), json!(absent)),
            ("expected_steps".to_string(), json!(expected)),
            ("longest_gap_steps".to_string(), json!(longest_steps)),
            ("from".to_string(), json!(tick_label(from))),
            ("to".to_string(), json!(tick_label(to))),
            ("reference".to_string(), json!("modal step of the axis")),
            ("step".to_string(), json!(step)),
        ]),
        Some("rows are absent for these periods; decide whether that is real before computing rates over time".into()),
        Method::new(
            "series",
            "axis_grid_scan",
            "distinct axis values against the grid of their modal step (onceover v0.1)",
        ),
    ))
}

/// The mean of a measure at each distinct tick, in time order.
fn series_of(table: &TypedTable, col: usize, ticks: &[Option<i64>]) -> (Vec<i64>, Vec<f64>) {
    let mut by_tick: BTreeMap<i64, (f64, usize)> = BTreeMap::new();
    for (r, t) in ticks.iter().enumerate() {
        if let (Some(t), Some(v)) = (t, table.cols[col].numeric_at(r)) {
            if v.is_finite() {
                let e = by_tick.entry(*t).or_insert((0.0, 0));
                e.0 += v;
                e.1 += 1;
            }
        }
    }
    let n = by_tick.len();
    let stride = n.div_ceil(MAX_POINTS).max(1);
    by_tick
        .into_iter()
        .step_by(stride)
        .map(|(t, (sum, c))| (t, sum / c as f64))
        .unzip()
}

/// Least-squares line over the ticks: fitted values and share of variance.
fn line_fit(x: &[f64], y: &[f64]) -> Option<(Vec<f64>, f64, f64)> {
    let n = x.len() as f64;
    let (mx, my) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        sxx += (a - mx) * (a - mx);
        sxy += (a - mx) * (b - my);
        syy += (b - my) * (b - my);
    }
    if sxx <= 0.0 || syy <= 0.0 {
        return None;
    }
    let slope = sxy / sxx;
    let fitted = x.iter().map(|a| my + slope * (a - mx)).collect();
    Some((fitted, slope, sxy * sxy / (sxx * syy)))
}

/// The outcome of testing a fitted line's slope against noise.
struct TrendTest {
    /// Two-sided p-value of the slope's t statistic after the
    /// autocorrelation deflation.
    p_value: f64,
    /// Lag-1 autocorrelation of the residuals, clamped to [0, 0.95].
    residual_autocorrelation: f64,
    /// Effective number of independent points after the deflation.
    effective_points: f64,
}

/// Test whether a fitted line's slope is distinguishable from noise.
///
/// The classical slope t-test assumes independent residuals. On a
/// time series the residuals of a spurious fit — a random walk read
/// as a drift — are strongly autocorrelated, and the classical test
/// calls such a slope significant. The statistic is therefore
/// deflated by the residuals' lag-1 autocorrelation rho: the
/// effective sample size is n(1-rho)/(1+rho), the t statistic scales
/// by sqrt(n_eff/n), and the p-value uses n_eff - 2 degrees of
/// freedom. A series whose movement is a real drift over weakly
/// correlated noise keeps a small p-value; a random walk's does
/// not.
fn trend_test(x: &[f64], y: &[f64], fitted: &[f64], slope: f64) -> Option<TrendTest> {
    let n = y.len();
    if n < 4 {
        return None;
    }
    let nf = n as f64;
    let mx = x.iter().sum::<f64>() / nf;
    let sxx: f64 = x.iter().map(|a| (a - mx) * (a - mx)).sum();
    if sxx <= 0.0 {
        return None;
    }
    let resid: Vec<f64> = y.iter().zip(fitted).map(|(a, b)| a - b).collect();
    let sse: f64 = resid.iter().map(|r| r * r).sum();
    if sse <= 0.0 {
        // A line through every point: the slope is the series.
        return Some(TrendTest {
            p_value: 0.0,
            residual_autocorrelation: 0.0,
            effective_points: nf,
        });
    }
    let se = (sse / (nf - 2.0) / sxx).sqrt();
    if se <= 0.0 || !se.is_finite() {
        return None;
    }
    let t = slope / se;
    let rho = autocorrelation(&resid, 1).clamp(0.0, 0.95);
    let n_eff = (nf * (1.0 - rho) / (1.0 + rho)).clamp(3.0, nf);
    let t_adj = t * (n_eff / nf).sqrt();
    let df = (n_eff - 2.0).max(1.0);
    let t2 = t_adj * t_adj;
    Some(TrendTest {
        p_value: crate::dep::beta_inc(df / (df + t2), df / 2.0, 0.5),
        residual_autocorrelation: rho,
        effective_points: n_eff,
    })
}

/// The best single level shift: fitted values and share of variance.
fn step_fit(y: &[f64]) -> Option<(Vec<f64>, f64)> {
    let n = y.len();
    let total: f64 = y.iter().sum();
    let mean = total / n as f64;
    let syy: f64 = y.iter().map(|v| (v - mean) * (v - mean)).sum();
    if syy <= 0.0 {
        return None;
    }
    let min_side = (n / 20).max(5);
    let (mut left, mut best) = (0.0, None::<(usize, f64)>);
    for k in 1..n {
        left += y[k - 1];
        if k < min_side || n - k < min_side {
            continue;
        }
        let right = total - left;
        let between =
            left * left / k as f64 + right * right / (n - k) as f64 - total * total / n as f64;
        if best.is_none_or(|(_, b)| between > b) {
            best = Some((k, between));
        }
    }
    let (k, between) = best?;
    let (ml, mr) = (
        y[..k].iter().sum::<f64>() / k as f64,
        y[k..].iter().sum::<f64>() / (n - k) as f64,
    );
    let fitted = (0..n).map(|i| if i < k { ml } else { mr }).collect();
    Some((fitted, between / syy))
}

fn autocorrelation(r: &[f64], lag: usize) -> f64 {
    let n = r.len();
    if lag == 0 || lag >= n {
        return 0.0;
    }
    let mean = r.iter().sum::<f64>() / n as f64;
    let den: f64 = r.iter().map(|v| (v - mean) * (v - mean)).sum();
    if den <= 0.0 {
        return 0.0;
    }
    (0..n - lag)
        .map(|i| (r[i] - mean) * (r[i + lag] - mean))
        .sum::<f64>()
        / den
}

/// The strongest repeating lag: a peak standing above the trough before it, so
/// the smooth decay of a persistent series does not qualify.
fn dominant_cycle(r: &[f64], extra_lags: &[usize]) -> Option<(usize, f64)> {
    let n = r.len();
    let limit = MAX_SCANNED_LAG.min(n / 3);
    if limit < 4 {
        return None;
    }
    let acf: Vec<f64> = (0..=limit + 1).map(|l| autocorrelation(r, l)).collect();
    let bar = MIN_CYCLE_ACF.max(ACF_NOISE_MULTIPLE / (n as f64).sqrt());
    let mut best: Option<(usize, f64)> = None;
    let mut trough = f64::INFINITY;
    for l in 1..=limit {
        trough = trough.min(acf[l]);
        if l >= 2
            && acf[l] >= bar
            && acf[l] > acf[l - 1]
            && acf[l] >= acf[l + 1]
            && acf[l] - trough >= MIN_CYCLE_PROMINENCE
            // A harmonic must beat the fundamental clearly to replace it.
            && best.is_none_or(|(_, b)| acf[l] > b + 0.05)
        {
            best = Some((l, acf[l]));
        }
    }
    for &l in extra_lags {
        if l >= 2 && l < n / 2 {
            let at = |k: usize| autocorrelation(r, k);
            let v = at(l);
            let local = at(l - 1).min(at(l + 1));
            let early = at(l / 2);
            if v >= bar
                && v >= local
                && v - early >= MIN_CYCLE_PROMINENCE
                && best.is_none_or(|(_, b)| v > b + 0.05)
            {
                best = Some((l, v));
            }
        }
    }
    best
}

fn series_statement(
    name: &str,
    kind: &str,
    claim: String,
    score: f64,
    mut metrics: BTreeMap<String, Value>,
    interpretation: &str,
) -> Finding {
    metrics.insert("kind".to_string(), json!(kind));
    Finding::new(
        Pattern::SeriesStructure,
        BTreeMap::from([("column".to_string(), json!(name))]),
        claim,
        EvidenceBasis::Estimated,
        score,
        metrics,
        Some(interpretation.to_string()),
        Method::new(
            "series",
            "series_description",
            "line and level-shift fits on the per-tick mean; a trend is stated only when the slope clears an autocorrelation-deflated t-test at p < 0.001; cycles and persistence are read from the autocorrelation of what the fits leave (onceover v0.1)",
        ),
    )
}

fn measure_findings(table: &TypedTable, col: usize, input: &SeriesInput) -> Vec<Finding> {
    let (ticks, y) = series_of(table, col, input.ticks);
    let n = y.len();
    if n < MIN_POINTS {
        return Vec::new();
    }
    let name = table.names[col].as_str();
    let origin = ticks[0];
    let years: Vec<f64> = ticks
        .iter()
        .map(|t| (t - origin) as f64 / (365.25 * MICROS_PER_DAY as f64))
        .collect();
    let Some((line, slope, line_r2)) = line_fit(&years, &y) else {
        return Vec::new();
    };
    let (step, step_r2) = step_fit(&y).unwrap_or((vec![0.0; n], 0.0));
    let mut out = Vec::new();
    let test = trend_test(&years, &y, &line, slope);
    let is_trend = line_r2 >= MIN_TREND_R2
        && line_r2 >= step_r2 + MIN_LINE_OVER_STEP
        && test.as_ref().is_some_and(|t| t.p_value < TREND_ALPHA);
    if is_trend {
        let t = test.as_ref().expect("the trend gate checked the test");
        out.push(series_statement(
            name,
            "trend",
            format!(
                "{name} {} along {} by about {:.4} per year; a straight line explains {:.0}% of the variance in its per-step mean, and the slope is distinguishable from noise (autocorrelation-deflated t-test, p {})",
                if slope > 0.0 { "rises" } else { "falls" },
                input.axis_name,
                slope.abs(),
                100.0 * line_r2,
                crate::dep::fmt_q(t.p_value),
            ),
            0.6 + 0.2 * line_r2,
            BTreeMap::from([
                ("points".to_string(), json!(n)),
                ("slope_per_year".to_string(), json!(slope)),
                ("r2".to_string(), json!(line_r2)),
                ("level_shift_r2".to_string(), json!(step_r2)),
                ("slope_p_value".to_string(), json!(t.p_value)),
                (
                    "residual_autocorrelation".to_string(),
                    json!(t.residual_autocorrelation),
                ),
                ("effective_points".to_string(), json!(t.effective_points)),
            ]),
            "compare like periods rather than whole-series averages",
        ));
    }
    let base: &[f64] = if line_r2 >= step_r2 { &line } else { &step };
    let explained = line_r2.max(step_r2);
    let residual: Vec<f64> = if explained >= REMOVE_STRUCTURE_R2 {
        y.iter().zip(base).map(|(a, b)| a - b).collect()
    } else {
        y.clone()
    };

    let (modal_step, regular) = {
        let mut diffs: Vec<i64> = ticks.windows(2).map(|w| w[1] - w[0]).collect();
        diffs.sort_unstable();
        let median = diffs[diffs.len() / 2];
        let at_median = diffs.iter().filter(|d| **d == median).count();
        (
            median,
            at_median as f64 / diffs.len() as f64 >= MIN_MODAL_STEP_SHARE,
        )
    };
    // A lag counts steps, so it only means a period on an evenly spaced axis.
    if !regular {
        return out;
    }
    // A yearly cycle in daily or hourly data lies beyond the lag scan.
    let span = ticks[n - 1] - origin;
    let year = (365.25 * MICROS_PER_DAY as f64) as i64;
    let mut extra: Vec<usize> = Vec::new();
    if modal_step > 0 && span >= 5 * year / 2 {
        extra.push((year / modal_step) as usize);
    } else if modal_step == 7 * MICROS_PER_DAY && span >= 2 * year {
        // A weekly axis carries its annual cycle at exactly lag 52.
        // Two full cycles are enough to test that lag directly,
        // ahead of the two and a half years the general yearly
        // probe waits for.
        extra.push(52);
    }
    let cycle = dominant_cycle(&residual, &extra);
    if let Some((lag, acf)) = cycle {
        let period = lag as i64 * modal_step;
        let label = if (period - year).abs() <= year / 40 {
            "1 year".to_string()
        } else {
            duration_label(period)
        };
        out.push(series_statement(
            name,
            "cycle",
            format!(
                "{name} repeats every {label} along {}: autocorrelation {acf:.2} at that lag",
                input.axis_name,
            ),
            0.58 + 0.25 * acf.clamp(0.0, 1.0),
            BTreeMap::from([
                ("points".to_string(), json!(n)),
                ("lag_steps".to_string(), json!(lag)),
                ("period".to_string(), json!(label)),
                ("autocorrelation".to_string(), json!(acf)),
            ]),
            "compare same phases of the cycle; a raw difference between two dates may only reflect it",
        ));
    } else {
        let lag1 = autocorrelation(&residual, 1);
        if lag1 >= MIN_PERSISTENCE_ACF.max(ACF_NOISE_MULTIPLE / (n as f64).sqrt()) {
            out.push(series_statement(
                name,
                "persistence",
                format!(
                    "{name} persists from one step to the next along {}: lag-1 autocorrelation {lag1:.2}",
                    input.axis_name,
                ),
                0.5 + 0.1 * lag1.clamp(0.0, 1.0),
                BTreeMap::from([
                    ("points".to_string(), json!(n)),
                    ("autocorrelation".to_string(), json!(lag1)),
                    ("structure_removed".to_string(), json!(explained >= REMOVE_STRUCTURE_R2)),
                ]),
                "neighbouring rows are not independent; random row splits and naive standard errors will mislead",
            ));
        }
    }
    out
}

/// Render one entity key value as a reader-facing label.
fn entity_label(table: &TypedTable, col: usize, row: usize) -> Option<String> {
    match &table.cols[col] {
        TypedCol::Str { vals } => vals[row].map(|s| s.to_string()),
        TypedCol::StrOwned { vals } => vals[row].clone(),
        TypedCol::I64 { vals } => vals.at(row).map(|v| v.to_string()),
        TypedCol::U64 { vals } => vals.at(row).map(|v| v.to_string()),
        TypedCol::F64 { vals } => vals.at(row).map(crate::fmt_float),
        TypedCol::Bool { vals } => vals[row].map(|v| v.to_string()),
        TypedCol::Date { vals } => vals[row].map(|d| dateutil::iso_date(i64::from(d))),
        TypedCol::Datetime { .. } => table.cols[col].datetime_micros(row).map(tick_label),
        _ => None,
    }
}

/// The first few entity labels, with the rest counted.
fn name_list(labels: &[String]) -> String {
    let shown = labels.len().min(MAX_NAMED_ENTITIES);
    let mut s = labels[..shown].join(", ");
    if labels.len() > shown {
        s.push_str(&format!(" and {} more", labels.len() - shown));
    }
    s
}

/// What the per-entity pass leaves behind: its findings, the trend
/// account the aggregate statement is qualified against, and the
/// coverage fragment.
struct EntityOutcome {
    findings: Vec<Finding>,
    /// Principal measure column, entities tested for a trend, and
    /// how many of them rise / fall significantly on their own.
    /// Present only when at least [`MIN_ENTITY_TESTED`] entities
    /// could be tested.
    principal_trend: Option<(usize, usize, usize, usize)>,
    detail: String,
}

/// The trend and level-break checks run per entity on the principal
/// measure — the measure with the most entity × tick observations.
///
/// The aggregate series averages entities together, so a movement
/// carried by a minority of entities reads as a general one, and a
/// movement no single entity shows can appear from the mix. The
/// per-entity statements name the entities carrying each pattern;
/// [`profile_series`] qualifies the aggregate trend statement when
/// the per-entity evidence is mixed. Per-entity series are shorter,
/// so the bars are stated for them: a trend needs at least
/// [`MIN_ENTITY_TREND_POINTS`] points and the same gates as the
/// aggregate (line fit, line over step, deflated t-test at
/// p < 0.001); a level break needs at least
/// [`MIN_ENTITY_BREAK_POINTS`] points and a single-break gain of
/// [`ENTITY_BREAK_MIN_GAIN`] with segments of at least
/// [`ENTITY_BREAK_MIN_SEGMENT`] — there is no holdout at entity
/// level, so the gain bar sits above the aggregate pass's. An
/// entity carrying a significant trend is not break-scanned: a
/// steady ramp reads as a level shift to a two-mean scan, and its
/// movement is already stated as a trend.
fn entity_series(table: &TypedTable, input: &SeriesInput, measures: &[usize]) -> EntityOutcome {
    let empty = |detail: String| EntityOutcome {
        findings: Vec::new(),
        principal_trend: None,
        detail,
    };
    let Some(&ecol) = input.entities.first() else {
        return empty(String::new());
    };
    if measures.is_empty() {
        return empty(String::new());
    }
    let ename = table.names[ecol].as_str();
    let (codes, _) = match crate::panels::entity_codes(table, ecol) {
        Ok(v) => v,
        Err(_) => {
            return empty(format!(
                "; per-entity series not run: the entity column {ename} could not key entities"
            ))
        }
    };
    let mut labels: Vec<Option<String>> = Vec::new();
    for (r, c) in codes.iter().enumerate() {
        if let Some(c) = c {
            let c = *c as usize;
            if labels.len() <= c {
                labels.resize(c + 1, None);
            }
            if labels[c].is_none() {
                labels[c] = entity_label(table, ecol, r);
            }
        }
    }
    let label_of = |c: u32| -> String {
        labels
            .get(c as usize)
            .cloned()
            .flatten()
            .unwrap_or_else(|| format!("entity #{c}"))
    };

    // The principal measure: the one with the most observations on
    // the entity × time grain.
    let mut principal = measures[0];
    let mut best_count = 0usize;
    for &m in measures {
        let count = (0..table.n)
            .filter(|&r| {
                codes[r].is_some()
                    && input.ticks[r].is_some()
                    && table.cols[m].numeric_at(r).is_some_and(|v| v.is_finite())
            })
            .count();
        if count > best_count {
            best_count = count;
            principal = m;
        }
    }
    let mname = table.names[principal].as_str();

    // Per-entity per-tick means of the principal measure.
    let mut by_entity: HashMap<u32, BTreeMap<i64, (f64, usize)>> = HashMap::new();
    for (r, code) in codes.iter().enumerate() {
        if let (Some(c), Some(t), Some(v)) = (
            *code,
            input.ticks[r],
            table.cols[principal].numeric_at(r),
        ) {
            if v.is_finite() {
                let e = by_entity.entry(c).or_default().entry(t).or_insert((0.0, 0));
                e.0 += v;
                e.1 += 1;
            }
        }
    }
    let origin = input.ticks.iter().flatten().min().copied().unwrap_or(0);
    let year_micros = 365.25 * MICROS_PER_DAY as f64;

    let mut tested = 0usize;
    let mut rises: Vec<String> = Vec::new();
    let mut falls: Vec<String> = Vec::new();
    let mut break_tested = 0usize;
    let mut break_groups: BTreeMap<(i64, i32), Vec<String>> = BTreeMap::new();
    let mut entity_order: Vec<u32> = by_entity.keys().copied().collect();
    entity_order.sort_unstable();
    for c in entity_order {
        let series = &by_entity[&c];
        let ticks: Vec<i64> = series.keys().copied().collect();
        let y: Vec<f64> = series.values().map(|(s, k)| s / *k as f64).collect();
        let label = label_of(c);
        let mut has_trend = false;
        if y.len() >= MIN_ENTITY_TREND_POINTS {
            tested += 1;
            let years: Vec<f64> = ticks
                .iter()
                .map(|t| (*t - origin) as f64 / year_micros)
                .collect();
            if let Some((fitted, slope, r2)) = line_fit(&years, &y) {
                let (_, step_r2) = step_fit(&y).unwrap_or((Vec::new(), 0.0));
                let significant = r2 >= MIN_TREND_R2
                    && r2 >= step_r2 + MIN_LINE_OVER_STEP
                    && trend_test(&years, &y, &fitted, slope)
                        .is_some_and(|t| t.p_value < TREND_ALPHA);
                if significant {
                    has_trend = true;
                    if slope > 0.0 {
                        rises.push(label.clone());
                    } else {
                        falls.push(label.clone());
                    }
                }
            }
        }
        // An entity carrying a significant trend is not break-
        // scanned: a steady ramp reads as a level shift to a
        // two-mean scan, and the movement is already stated as a
        // trend.
        if y.len() >= MIN_ENTITY_BREAK_POINTS && !has_trend {
            break_tested += 1;
            if let Some(br) = single_break_l2(&y, ENTITY_BREAK_MIN_SEGMENT) {
                let direction =
                    (br.right_mean > br.left_mean) as i32 - (br.right_mean < br.left_mean) as i32;
                if br.gain_fraction >= ENTITY_BREAK_MIN_GAIN && direction != 0 {
                    break_groups
                        .entry((ticks[br.index], direction))
                        .or_default()
                        .push(label);
                }
            }
        }
    }

    let mut findings = Vec::new();
    if tested >= MIN_ENTITY_TESTED {
        for (carriers, direction, verb) in [(&rises, "rise", "rises"), (&falls, "fall", "falls")] {
            if carriers.len() >= MIN_ENTITY_CARRIERS {
                findings.push(series_statement(
                    mname,
                    "entity_trend",
                    format!(
                        "{mname} {verb} along {} for {} of the {tested} {ename} entities tested: {}",
                        input.axis_name,
                        carriers.len(),
                        name_list(carriers),
                    ),
                    0.55 + 0.25 * carriers.len() as f64 / tested as f64,
                    BTreeMap::from([
                        ("entity_column".to_string(), json!(ename)),
                        ("direction".to_string(), json!(direction)),
                        (
                            "entities_with_pattern".to_string(),
                            json!(carriers.len()),
                        ),
                        ("entities_tested".to_string(), json!(tested)),
                        (
                            "entities".to_string(),
                            json!(carriers[..carriers.len().min(MAX_NAMED_ENTITIES)]),
                        ),
                    ]),
                    "the named entities carry this movement on their own series; the aggregate statement averages all entities together",
                ));
            }
        }
    }
    if break_tested >= MIN_ENTITY_TESTED {
        if let Some((&(tick, direction), carriers)) =
            break_groups.iter().max_by_key(|(_, v)| v.len())
        {
            if carriers.len() >= MIN_ENTITY_CARRIERS {
                findings.push(series_statement(
                    mname,
                    "entity_break",
                    format!(
                        "{mname} shifts {} along {} at {} for {} of the {break_tested} {ename} entities tested: {}",
                        if direction > 0 { "up" } else { "down" },
                        input.axis_name,
                        tick_label(tick),
                        carriers.len(),
                        name_list(carriers),
                    ),
                    0.6,
                    BTreeMap::from([
                        ("entity_column".to_string(), json!(ename)),
                        (
                            "direction".to_string(),
                            json!(if direction > 0 { "up" } else { "down" }),
                        ),
                        ("breakpoint".to_string(), json!(tick_label(tick))),
                        ("breakpoint_raw".to_string(), json!(tick)),
                        (
                            "entities_with_pattern".to_string(),
                            json!(carriers.len()),
                        ),
                        ("entities_tested".to_string(), json!(break_tested)),
                        (
                            "entities".to_string(),
                            json!(carriers[..carriers.len().min(MAX_NAMED_ENTITIES)]),
                        ),
                    ]),
                    "a level shift at the same period in the named entities' own series; entities not named show no comparable break",
                ));
            }
        }
    }

    EntityOutcome {
        findings,
        principal_trend: (tested >= MIN_ENTITY_TESTED)
            .then_some((principal, tested, rises.len(), falls.len())),
        detail: format!(
            "; per-entity series of {mname} by {ename}: {tested} entities tested for a trend ({} rising, {} falling), {break_tested} for a level break",
            rises.len(),
            falls.len(),
        ),
    }
}

/// Whether a column's level is fixed by a calendar field of the axis (year,
/// month, month of year, weekday, hour). Such a column says nothing new when
/// its levels come and go.
fn follows_calendar(ordered: &[(i64, u32)]) -> bool {
    type Field = fn(i64) -> i64;
    let fields: [Field; 5] = [
        |t| dateutil::civil_from_days(t.div_euclid(MICROS_PER_DAY)).0,
        |t| {
            let (y, m, _) = dateutil::civil_from_days(t.div_euclid(MICROS_PER_DAY));
            y * 12 + m as i64
        },
        |t| dateutil::civil_from_days(t.div_euclid(MICROS_PER_DAY)).1 as i64,
        |t| (t.div_euclid(MICROS_PER_DAY) + 4).rem_euclid(7),
        |t| t.rem_euclid(MICROS_PER_DAY) / 3_600_000_000,
    ];
    fields.iter().any(|field| {
        let mut level_of: HashMap<i64, u32> = HashMap::new();
        for &(t, c) in ordered {
            if *level_of.entry(field(t)).or_insert(c) != c {
                return false;
            }
        }
        // A field with a key per row would fit anything.
        level_of.len() * 2 <= ordered.len()
    })
}

/// Levels that appear or disappear part way through, and shifting shares.
fn dimension_findings(
    table: &TypedTable,
    col: usize,
    coded: &Coded,
    rows_in_time: &[(i64, u32)],
    bar: f64,
    input: &SeriesInput,
) -> Vec<Finding> {
    let k = coded.labels.len();
    let ordered: Vec<(i64, u32)> = rows_in_time
        .iter()
        .filter_map(|&(t, r)| coded.codes[r as usize].map(|c| (t, c)))
        .collect();
    let n = ordered.len();
    if n < MIN_POINTS {
        return Vec::new();
    }
    if follows_calendar(&ordered) {
        return Vec::new();
    }
    let name = table.names[col].as_str();
    let mut out = Vec::new();
    let mut totals = vec![0usize; k];
    let (mut first, mut last) = (vec![usize::MAX; k], vec![0usize; k]);
    let mut widest_gap = vec![0usize; k];
    for (i, &(_, c)) in ordered.iter().enumerate() {
        let c = c as usize;
        totals[c] += 1;
        if first[c] != usize::MAX {
            widest_gap[c] = widest_gap[c].max(i - last[c]);
        }
        first[c] = first[c].min(i);
        last[c] = i;
    }
    let floor = MIN_NEW_LEVEL_ROWS.max((MIN_NEW_LEVEL_SHARE * n as f64) as usize);
    for level in 0..k {
        if totals[level] < floor {
            continue;
        }
        let label = &coded.labels[level];
        let active = last[level] - first[level] + 1;
        let rate = totals[level] as f64 / active as f64;
        // A level absent for a stretch no longer than its own later absences
        // (a season, a recurring state) has not arrived or left.
        let edge = (MIN_ABSENT_STRETCH * n as f64) as usize;
        let starts_late = first[level] >= edge && first[level] > widest_gap[level];
        let ends_early = n - 1 - last[level] >= edge && n - 1 - last[level] > widest_gap[level];
        if starts_late && first[level] as f64 * rate >= MIN_EXPECTED_ABSENT_ROWS {
            out.push(series_statement(
                name,
                "new_level",
                format!(
                    "{name} = {label} first appears at {} on {}, {:.0}% of the way through the rows; none of the {} rows before it have this level",
                    tick_label(ordered[first[level]].0),
                    input.axis_name,
                    100.0 * first[level] as f64 / n as f64,
                    first[level],
                ),
                0.72,
                BTreeMap::from([
                    ("level".to_string(), json!(label)),
                    ("value".to_string(), json!(label)),
                    ("first_seen".to_string(), json!(tick_label(ordered[first[level]].0))),
                    ("rows_before".to_string(), json!(first[level])),
                    ("rows".to_string(), json!(totals[level])),
                ]),
                "a level appearing mid-series often marks a process or coding change",
            ));
        }
        let after = n - 1 - last[level];
        if ends_early && after as f64 * rate >= MIN_EXPECTED_ABSENT_ROWS {
            out.push(series_statement(
                name,
                "ended_level",
                format!(
                    "{name} = {label} last appears at {} on {}; none of the {after} rows after it have this level",
                    tick_label(ordered[last[level]].0),
                    input.axis_name,
                ),
                0.7,
                BTreeMap::from([
                    ("level".to_string(), json!(label)),
                    ("value".to_string(), json!(label)),
                    ("last_seen".to_string(), json!(tick_label(ordered[last[level]].0))),
                    ("rows_after".to_string(), json!(after)),
                    ("rows".to_string(), json!(totals[level])),
                ]),
                "a level ending mid-series often marks a process or coding change",
            ));
        }
    }
    // Share of the most-shifted level, first tenth against last tenth.
    let slice = n / TIME_SLICES;
    if slice >= 30 {
        let counts = |rows: &[(i64, u32)]| {
            let mut c = vec![0usize; k];
            for &(_, l) in rows {
                c[l as usize] += 1;
            }
            c
        };
        let (early, late) = (counts(&ordered[..slice]), counts(&ordered[n - slice..]));
        let slices: Vec<Vec<usize>> = (0..TIME_SLICES)
            .map(|s| {
                let end = if s + 1 == TIME_SLICES {
                    n
                } else {
                    (s + 1) * slice
                };
                counts(&ordered[s * slice..end])
            })
            .collect();
        let tau = |level: usize| -> f64 {
            let (mut up, mut down) = (0i32, 0i32);
            for a in 0..TIME_SLICES {
                for b in a + 1..TIME_SLICES {
                    match slices[b][level].cmp(&slices[a][level]) {
                        std::cmp::Ordering::Greater => up += 1,
                        std::cmp::Ordering::Less => down += 1,
                        std::cmp::Ordering::Equal => {}
                    }
                }
            }
            (up - down).abs() as f64 / (TIME_SLICES * (TIME_SLICES - 1) / 2) as f64
        };
        let mut best: Option<(usize, f64, f64, f64)> = None;
        for level in 0..k {
            let (a, b) = (early[level] as f64, late[level] as f64);
            let s = slice as f64;
            let (pa, pb) = (a / s, b / s);
            if (pb - pa).abs() < MIN_SHARE_SHIFT || tau(level) < MIN_SHIFT_MONOTONICITY {
                continue;
            }
            let pooled = (a + b) / (2.0 * s);
            let var = pooled * (1.0 - pooled) * 2.0 / s;
            if var <= 0.0 {
                continue;
            }
            let chi2 = (pb - pa).powi(2) / var;
            if chi2 >= bar && best.is_none_or(|(_, _, _, c)| chi2 > c) {
                best = Some((level, pa, pb, chi2));
            }
        }
        if let Some((level, pa, pb, chi2)) = best {
            let label = &coded.labels[level];
            out.push(series_statement(
                name,
                "share_shift",
                format!(
                    "the share of {name} = {label} {} from {:.0}% in the first tenth of rows along {} to {:.0}% in the last tenth",
                    if pb > pa { "rises" } else { "falls" },
                    100.0 * pa,
                    input.axis_name,
                    100.0 * pb,
                ),
                0.66 + 0.2 * (pb - pa).abs(),
                BTreeMap::from([
                    ("level".to_string(), json!(label)),
                    ("value".to_string(), json!(label)),
                    ("early_share".to_string(), json!(pa)),
                    ("late_share".to_string(), json!(pb)),
                    ("slice_rows".to_string(), json!(slice)),
                    ("chi2".to_string(), json!(chi2)),
                ]),
                "the mix changes over time, so overall rates depend on the period chosen",
            ));
        }
    }
    out
}

/// Describe the axis and every series along it.
pub fn profile_series(table: &TypedTable, input: &SeriesInput) -> (Vec<Finding>, String) {
    let mut sorted: Vec<i64> = input.ticks.iter().flatten().copied().collect();
    sorted.sort_unstable();
    sorted.dedup();
    let mut findings: Vec<Finding> = axis_gaps(input, &sorted).into_iter().collect();

    let measures: Vec<usize> = input.measures.iter().copied().take(MAX_MEASURES).collect();
    let per_measure: Vec<Vec<Finding>> = measures
        .par_iter()
        .map(|&c| measure_findings(table, c, input))
        .collect();
    findings.extend(per_measure.into_iter().flatten());

    let mut rows_in_time: Vec<(i64, u32)> = input
        .ticks
        .iter()
        .enumerate()
        .filter_map(|(r, t)| t.map(|t| (t, r as u32)))
        .collect();
    rows_in_time.sort_unstable();
    let cfg = SurpriseConfig::default();
    let coded: Vec<(usize, Coded)> = input
        .dimensions
        .iter()
        .filter_map(|&c| code_column(table, c, &cfg).map(|coded| (c, coded)))
        .collect();
    let levels: usize = coded.iter().map(|(_, c)| c.labels.len()).sum();
    let bar = 2.0 * (2.0 * levels.max(1) as f64 / FAMILY_ALPHA).ln();
    let per_dimension: Vec<Vec<Finding>> = coded
        .par_iter()
        .map(|(c, coded)| dimension_findings(table, *c, coded, &rows_in_time, bar, input))
        .collect();
    findings.extend(per_dimension.into_iter().flatten());

    // Per-entity series on the panel grain, and the qualification
    // of the principal measure's aggregate trend when the
    // per-entity evidence is mixed: an aggregate movement no
    // entity shows on its own, or one a minority carries, is not
    // stated as a general movement.
    let entity = entity_series(table, input, &measures);
    findings.extend(entity.findings);
    if let Some((mcol, tested, rises, falls)) = entity.principal_trend {
        let ename = table.names[input.entities[0]].clone();
        let mname = table.names[mcol].clone();
        for f in findings.iter_mut() {
            let is_aggregate_trend = f.finding_type.as_deref() == Some("series_structure")
                && f.subject.columns.contains(&mname)
                && f.evidence
                    .first()
                    .and_then(|e| e.metrics.get("kind"))
                    .and_then(Value::as_str)
                    == Some("trend");
            if !is_aggregate_trend {
                continue;
            }
            let slope = f.evidence[0]
                .metrics
                .get("slope_per_year")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let (carriers, word) = if slope > 0.0 {
                (rises, "rise")
            } else {
                (falls, "fall")
            };
            if carriers == 0 {
                f.claim.push_str(&format!(
                    "; no individual {ename} series shows a significant {word} of its own"
                ));
            } else if carriers * 2 < tested {
                f.claim.push_str(&format!(
                    "; the {word} is carried by a minority of entities: {carriers} of {tested} {ename} series show a significant {word} of their own"
                ));
            } else {
                continue;
            }
            f.evidence[0]
                .metrics
                .insert("entities_with_trend".into(), json!(carriers));
            f.evidence[0]
                .metrics
                .insert("entities_tested".into(), json!(tested));
        }
    }

    let mut detail = format!(
        "{} measures and {} grouping columns described along {}; {} statements",
        measures.len(),
        coded.len(),
        input.axis_name,
        findings.len()
    );
    if input.measures.len() > MAX_MEASURES {
        detail.push_str(&format!(
            "; the first {MAX_MEASURES} of {} measures in column order",
            input.measures.len()
        ));
    }
    detail.push_str(&entity.detail);
    (findings, detail)
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

    fn years(n: usize) -> Vec<f64> {
        (0..n).map(|i| i as f64 / 52.0).collect()
    }

    fn p_value_of(x: &[f64], y: &[f64]) -> f64 {
        let (fitted, slope, _) = line_fit(x, y).expect("line fit");
        trend_test(x, y, &fitted, slope)
            .expect("trend test")
            .p_value
    }

    #[test]
    fn trend_test_accepts_a_real_drift() {
        let n = 156;
        let mut rng = Lcg(11);
        let y: Vec<f64> = (0..n).map(|i| 0.05 * i as f64 + rng.noise(1.0)).collect();
        let p = p_value_of(&years(n), &y);
        assert!(p < TREND_ALPHA, "a real drift must clear the bar, p = {p}");
    }

    #[test]
    fn trend_test_rejects_a_random_walk() {
        // A random walk's line fit explains a large share of its
        // variance — the descriptive gates alone call it a trend —
        // but its residuals are near-unit-root autocorrelated, so
        // the deflated test must not call the slope significant.
        let n = 156;
        for seed in [5, 17, 23] {
            let mut rng = Lcg(seed);
            let mut y = Vec::with_capacity(n);
            let mut level = 0.0;
            for _ in 0..n {
                level += rng.noise(1.0);
                y.push(level);
            }
            let p = p_value_of(&years(n), &y);
            assert!(
                p >= TREND_ALPHA,
                "seed {seed}: a random walk must not clear the bar, p = {p}"
            );
        }
    }

    #[test]
    fn trend_test_rejects_strongly_autocorrelated_noise() {
        let n = 156;
        let mut rng = Lcg(31);
        let mut y = Vec::with_capacity(n);
        let mut level = 0.0;
        for _ in 0..n {
            level = 0.95 * level + rng.noise(1.0);
            y.push(level);
        }
        let p = p_value_of(&years(n), &y);
        assert!(
            p >= TREND_ALPHA,
            "AR(1) noise must not clear the bar, p = {p}"
        );
    }
}
