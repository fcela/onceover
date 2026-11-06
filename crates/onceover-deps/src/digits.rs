//! Recording-process fingerprints in numeric columns: values
//! pile on round numbers when a human estimated them or a
//! process rounded them, terminal digits stop being uniform,
//! and scale-free quantities follow Benford's first-digit law
//! or visibly do not. Three checks, one pass:
//!
//! - **Heaping**: for integer-valued columns, the share of
//!   nonzero values divisible by 10 / 100 / 1000 against the
//!   1/U share a smooth recording implies, zeros reported
//!   separately (0 is a multiple of every unit and would swamp
//!   the test). The largest unit whose pile clears 3x the
//!   baseline at z >= 6 is reported.
//! - **Terminal-digit preference**: the last-digit distribution
//!   of integer-valued columns against the digits the column's
//!   value grid can end in (chi-square, df = attainable digits
//!   minus 1). The grid is the arithmetic progression from the
//!   minimum in steps of the values' common difference, so a
//!   column of even numbers is compared with the five even
//!   digits, not with all ten. The test runs only when the central
//!   90% of the values spans at least 3 cycles of the last digit;
//!   over a narrower spread the value distribution itself fixes the
//!   digit shares.
//!   Suppressed for a column where heaping already fired: one
//!   phenomenon, one finding — the heaping evidence carries the
//!   0/5 digit shares.
//! - **Benford deviation**: first digits of positive values
//!   against Benford's law, only for eligible columns —
//!   eligibility is conservative and recorded in coverage:
//!   at least 500 values, at least 95% of them positive,
//!   max/min >= 1000 (three orders of magnitude), at least 100
//!   distinct values, and, for integer-valued columns only, a
//!   distinct share < 50% (near-unique integer columns are
//!   identifier-like, not quantities; continuous quantities are
//!   near-unique by nature and stay eligible). A deviation
//!   reports only when it is both significant (chi-square,
//!   df 8) and material (mean absolute deviation >= 0.012, the
//!   published nonconformity boundary for first digits), so a
//!   conforming column stays quiet at any sample size.
//!
//! Fixed-precision non-integer columns (money with cents) are
//! not digit-tested in this version; that is a scope limit,
//! recorded here rather than in coverage per column.

use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use rayon::prelude::*;
use serde_json::json;
use std::collections::BTreeMap;

use onceover_core::{TypedCol, TypedTable};

const MIN_VALUES: usize = 500;
const MIN_DISTINCT: usize = 20;
/// chi-square critical values used as bars: df 9 at ~1e-6 and
/// df 8 at ~3e-5 — strict per-column bars, no per-table
/// correction needed at these levels for tens of columns.
const CHI2_DIGIT_BAR: f64 = 44.0;
/// The terminal-digit bar by degrees of freedom (index = df), at the
/// tail probability of `CHI2_DIGIT_BAR` on 9 df (1.4e-6).
const CHI2_DIGIT_BARS: [f64; 10] = [
    f64::INFINITY,
    23.3,
    26.9,
    30.0,
    32.6,
    35.1,
    37.5,
    39.7,
    41.9,
    CHI2_DIGIT_BAR,
];
/// The central 90% of the values (5th to 95th percentile) must span
/// at least this many cycles of the last digit for the terminal-digit
/// test to run. Over a narrower spread the shape of the distribution
/// decides which digits are common.
const MIN_DIGIT_CYCLES: f64 = 3.0;
const CHI2_BENFORD_BAR: f64 = 34.0;
const BENFORD_MAD_BAR: f64 = 0.012;

pub fn digit_fingerprints(table: &TypedTable) -> (Vec<Finding>, String) {
    let per_column: Vec<Option<(Vec<Finding>, bool)>> = table
        .cols
        .par_iter()
        .enumerate()
        .map(|(i, col)| digit_column(&table.names[i], col))
        .collect();
    let mut out = Vec::new();
    let mut scanned = 0usize;
    let mut benford_eligible = 0usize;
    for (findings, eligible) in per_column.into_iter().flatten() {
        scanned += 1;
        benford_eligible += usize::from(eligible);
        out.extend(findings);
    }
    let cov = format!(
        "{} numeric columns digit-scanned; {} Benford-eligible (others fail the positivity, span, or distinctness eligibility recorded in the detector docs)",
        scanned, benford_eligible
    );
    (out, cov)
}

/// The column's findings and whether it was Benford-eligible, or None when
/// the column is not numeric or too short to scan.
fn digit_column(name: &str, col: &TypedCol) -> Option<(Vec<Finding>, bool)> {
    let vals: Vec<f64> = match col {
        TypedCol::F64 { vals } => vals
            .iter()
            .filter_map(|v| v.filter(|x| x.is_finite()).copied())
            .collect(),
        TypedCol::I64 { vals } => vals.iter().filter_map(|v| v.map(|x| *x as f64)).collect(),
        TypedCol::U64 { vals } => vals.iter().filter_map(|v| v.map(|x| *x as f64)).collect(),
        _ => return None,
    };
    if vals.len() < MIN_VALUES {
        return None;
    }
    let mut out = Vec::new();

    // ---- integer-valued checks: heaping + terminal digit ----
    if vals
        .iter()
        .all(|v| v.fract() == 0.0 && v.abs() < 9_007_199_254_740_992.0)
    {
        let distinct: std::collections::HashSet<i64> = vals.iter().map(|v| *v as i64).collect();
        if distinct.len() >= MIN_DISTINCT {
            let heaped = heaping(name, &vals, &mut out);
            if !heaped {
                terminal_digit(name, &vals, &mut out);
            }
        }
    }

    // ---- Benford ----
    let eligible = benford(name, &vals, &mut out);
    Some((out, eligible))
}

/// Returns true when a heaping finding was emitted.
fn heaping(name: &str, vals: &[f64], out: &mut Vec<Finding>) -> bool {
    let zeros = vals.iter().filter(|&&v| v == 0.0).count();
    let nonzero: Vec<i64> = vals
        .iter()
        .filter(|&&v| v != 0.0)
        .map(|v| *v as i64)
        .collect();
    if nonzero.len() < MIN_VALUES {
        return false;
    }
    let n = nonzero.len() as f64;
    let max_abs = nonzero.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
    let mut digit_counts = [0usize; 10];
    for v in &nonzero {
        digit_counts[(v.unsigned_abs() % 10) as usize] += 1;
    }
    for &u in &[1000i64, 100, 10] {
        if max_abs < 20 * (u as u64) {
            continue; // the unit must be small against the range
        }
        let hits = nonzero.iter().filter(|v| *v % u == 0).count();
        let share = hits as f64 / n;
        let base = 1.0 / u as f64;
        if share < 3.0 * base {
            continue;
        }
        let z = (share - base) / (base * (1.0 - base) / n).sqrt();
        if z < 6.0 {
            continue;
        }
        out.push(Finding::new(
            Pattern::Heaping,
            BTreeMap::from([("column".into(), json!(name))]),
            format!(
                "column {} heaps on multiples of {}: {:.1}% of nonzero values are divisible by {} against the {:.1}% a smooth recording implies (z = {:.1}); zeros are {:.1}% of the column and are counted separately",
                name,
                u,
                100.0 * share,
                u,
                100.0 * base,
                z,
                100.0 * zeros as f64 / vals.len() as f64,
            ),
            EvidenceBasis::Measured,
            0.5,
            BTreeMap::from([
                ("unit".into(), json!(u)),
                ("share_divisible".into(), json!(share)),
                ("share_baseline".into(), json!(base)),
                ("z".into(), json!(z)),
                ("zero_share".into(), json!(zeros as f64 / vals.len() as f64)),
                (
                    "digit_zero_share".into(),
                    json!(digit_counts[0] as f64 / n),
                ),
                (
                    "digit_five_share".into(),
                    json!(digit_counts[5] as f64 / n),
                ),
                (
                    "method".into(),
                    json!("share of nonzero values divisible by the unit vs 1/unit under a smooth recording; largest firing unit reported; zeros excluded from the multiples test and reported separately"),
                ),
            ]),
            Some(
                "values were estimated or rounded to this unit before recording; treat the column's precision as the unit, not the integer"
                    .into(),
            ),
            Method::new(
                "digits",
                "heaping_scan",
                "exact divisibility shares vs the smooth-recording baseline (onceover v0.1)",
            ),
        ));
        return true;
    }
    false
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Expected terminal-digit shares for values spread evenly over the
/// grid lo, lo + step, ..., hi: within one cycle of the last digit
/// (10 / gcd(step, 10) grid points) each attainable digit occurs
/// once, and nonnegative and negative points (whose |x| ends in a
/// different digit set) are weighted by how many of each the grid
/// holds. Also returns the number of grid points in one digit cycle.
fn grid_digit_shares(lo: i64, hi: i64, step: u64) -> ([f64; 10], u64) {
    let points = ((hi as i128 - lo as i128) / step as i128 + 1) as f64;
    // gcd(0, 10) = 10, so a step that is a multiple of 10 has
    // period 1: every grid point ends in the same digit.
    let period = 10 / gcd(step % 10, 10);
    let mut shares = [0.0f64; 10];
    // Grid points below zero and at or above zero.
    let negative = if lo < 0 {
        (((-1i128 - lo as i128) / step as i128) + 1).min(points as i128) as f64
    } else {
        0.0
    };
    let nonnegative = points - negative;
    let first_nonneg = if lo >= 0 {
        lo as i128
    } else {
        lo as i128 + negative as i128 * step as i128
    };
    for (start, weight) in [(lo as i128, negative), (first_nonneg, nonnegative)] {
        if weight <= 0.0 {
            continue;
        }
        for k in 0..period as i128 {
            let x = start + k * step as i128;
            shares[(x.unsigned_abs() % 10) as usize] += weight / points / period as f64;
        }
    }
    (shares, period)
}

fn terminal_digit(name: &str, vals: &[f64], out: &mut Vec<Finding>) {
    let ints: Vec<i64> = vals.iter().map(|v| *v as i64).collect();
    let lo = *ints.iter().min().unwrap();
    let hi = *ints.iter().max().unwrap();
    let step = ints
        .iter()
        .fold(0u64, |g, &v| gcd(g, (v as i128 - lo as i128) as u64));
    if step == 0 {
        return;
    }
    let (expected_share, period) = grid_digit_shares(lo, hi, step);
    let mut sorted = ints.clone();
    sorted.sort_unstable();
    let q = |p: f64| sorted[((p * (sorted.len() - 1) as f64) as usize).min(sorted.len() - 1)];
    let central_points = (q(0.95) as i128 - q(0.05) as i128) as f64 / step as f64 + 1.0;
    if central_points / (period as f64) < MIN_DIGIT_CYCLES {
        return;
    }
    let mut counts = [0usize; 10];
    for v in &ints {
        counts[(v.unsigned_abs() % 10) as usize] += 1;
    }
    let n = vals.len() as f64;
    let attainable = expected_share.iter().filter(|&&p| p > 0.0).count();
    let chi2: f64 = counts
        .iter()
        .zip(expected_share)
        .filter(|(_, p)| *p > 0.0)
        .map(|(&c, p)| (c as f64 - n * p).powi(2) / (n * p))
        .sum();
    let bar = CHI2_DIGIT_BARS[attainable.saturating_sub(1)];
    // The digit most over its expected share.
    let (top_digit, _) = counts
        .iter()
        .zip(expected_share)
        .enumerate()
        .filter(|(_, (_, p))| *p > 0.0)
        .map(|(d, (&c, p))| (d, c as f64 / n - p))
        .fold((0usize, f64::NEG_INFINITY), |best, cur| {
            if cur.1 > best.1 {
                cur
            } else {
                best
            }
        });
    let top_share = counts[top_digit] as f64 / n;
    let top_expected = expected_share[top_digit];
    if chi2 < bar || top_share < 1.5 * top_expected {
        return;
    }
    let against = if attainable == 10 {
        format!("{:.0}% under uniformity", 100.0 * top_expected)
    } else {
        format!(
            "{:.0}% expected over the {} digits multiples of {} can end in",
            100.0 * top_expected,
            attainable,
            step
        )
    };
    out.push(Finding::new(
        Pattern::DigitPreference,
        BTreeMap::from([("column".into(), json!(name))]),
        format!(
            "column {} has a terminal-digit preference: digit {} ends {:.1}% of values against {} (chi-square {:.1} on {} df, bar {:.1})",
            name,
            top_digit,
            100.0 * top_share,
            against,
            chi2,
            attainable - 1,
            bar,
        ),
        EvidenceBasis::Measured,
        0.5,
        BTreeMap::from([
            (
                "digit_shares".into(),
                json!(counts.iter().map(|&c| c as f64 / n).collect::<Vec<f64>>()),
            ),
            ("chi_square".into(), json!(chi2)),
            ("top_digit".into(), json!(top_digit)),
            ("top_digit_share".into(), json!(top_share)),
            ("expected_digit_shares".into(), json!(expected_share)),
            ("grid_step".into(), json!(step)),
            ("degrees_of_freedom".into(), json!(attainable - 1)),
            (
                "method".into(),
                json!("last-digit distribution over all values vs the digits of the value grid (minimum, step = common difference); chi-square on attainable digits - 1 df; run when the range spans at least 10 digit cycles"),
            ),
        ]),
        Some(
            "a recording process favors this terminal digit; low-order digits carry less information than their precision suggests"
                .into(),
        ),
        Method::new(
            "digits",
            "terminal_digit_scan",
            "exact last-digit counts vs the value grid's digits (onceover v0.1)",
        ),
    ));
}

/// Returns true when the column was Benford-eligible (whether
/// or not a deviation was reported).
fn benford(name: &str, vals: &[f64], out: &mut Vec<Finding>) -> bool {
    let positives: Vec<f64> = vals.iter().copied().filter(|&v| v > 0.0).collect();
    if positives.len() < MIN_VALUES || positives.len() as f64 / vals.len() as f64 <= 0.95 {
        return false;
    }
    let (mut lo, mut hi) = (f64::INFINITY, 0.0f64);
    let mut distinct = std::collections::HashSet::new();
    for &v in &positives {
        lo = lo.min(v);
        hi = hi.max(v);
        distinct.insert(v.to_bits());
    }
    // The near-unique exclusion targets identifier-like columns;
    // identifiers are integer-valued, while genuine continuous
    // quantities are near-unique by nature and stay eligible.
    let integer_valued = positives.iter().all(|v| v.fract() == 0.0);
    if hi / lo < 1000.0
        || distinct.len() < 100
        || (integer_valued && distinct.len() as f64 / positives.len() as f64 >= 0.5)
    {
        return false;
    }
    let n = positives.len() as f64;
    let mut counts = [0usize; 9];
    for &v in &positives {
        let d = first_digit(v);
        counts[(d - 1) as usize] += 1;
    }
    let benford_p: Vec<f64> = (1..=9).map(|d| (1.0 + 1.0 / d as f64).log10()).collect();
    let chi2: f64 = counts
        .iter()
        .zip(&benford_p)
        .map(|(&c, &p)| (c as f64 - n * p).powi(2) / (n * p))
        .sum();
    let mad: f64 = counts
        .iter()
        .zip(&benford_p)
        .map(|(&c, &p)| (c as f64 / n - p).abs())
        .sum::<f64>()
        / 9.0;
    if chi2 < CHI2_BENFORD_BAR || mad < BENFORD_MAD_BAR {
        return true; // eligible, conforming (or immaterial): quiet
    }
    out.push(Finding::new(
        Pattern::BenfordDeviation,
        BTreeMap::from([("column".into(), json!(name))]),
        format!(
            "column {} deviates from Benford's first-digit law: mean absolute deviation {:.3} (nonconformity boundary {:.3}), chi-square {:.1} on 8 df (bar {:.0}); digit 1 leads {:.1}% of values against Benford's 30.1%",
            name,
            mad,
            BENFORD_MAD_BAR,
            chi2,
            CHI2_BENFORD_BAR,
            100.0 * counts[0] as f64 / n,
        ),
        EvidenceBasis::Measured,
        0.5,
        BTreeMap::from([
            (
                "first_digit_shares".into(),
                json!(counts.iter().map(|&c| c as f64 / n).collect::<Vec<f64>>()),
            ),
            ("benford_shares".into(), json!(benford_p)),
            ("chi_square".into(), json!(chi2)),
            ("mad".into(), json!(mad)),
            ("values".into(), json!(positives.len())),
            (
                "method".into(),
                json!("first significant digits of positive values vs Benford proportions; eligibility: >= 500 values, >= 95% positive, max/min >= 1000, >= 100 distinct, distinct share < 50% for integer-valued columns"),
            ),
        ]),
        Some(
            "the quantity does not behave like a scale-free measured amount; check for caps, floors, assigned values, or a generating process that is not multiplicative"
                .into(),
        ),
        Method::new(
            "digits",
            "benford_scan",
            "exact first-digit counts vs Benford's law (onceover v0.1)",
        ),
    ));
    true
}

fn first_digit(v: f64) -> u32 {
    let mut x = v;
    while x >= 10.0 {
        x /= 10.0;
    }
    while x < 1.0 {
        x *= 10.0;
    }
    x as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_digit_shares_follow_the_grid() {
        // Every integer: ten digits, equal shares.
        let (shares, period) = grid_digit_shares(0, 999, 1);
        assert!(shares.iter().all(|p| (p - 0.1).abs() < 1e-12), "{shares:?}");
        assert_eq!(period, 10);
        // Even numbers: five even digits.
        let (shares, _) = grid_digit_shares(2, 2000, 2);
        for d in 0..10 {
            let want = if d % 2 == 0 { 0.2 } else { 0.0 };
            assert!((shares[d] - want).abs() < 1e-12, "{d}: {shares:?}");
        }
        // Multiples of 5 from 15: last digits 5 and 0.
        let (shares, _) = grid_digit_shares(15, 1015, 5);
        assert!((shares[0] - 0.5).abs() < 1e-12 && (shares[5] - 0.5).abs() < 1e-12);
        // A grid straddling zero in steps of 5 from -998: the
        // negative points end in 8 or 3, the nonnegative ones in 2
        // or 7, weighted by how many of each there are.
        let (shares, _) = grid_digit_shares(-998, 1002, 5);
        let total: f64 = shares.iter().sum();
        assert!((total - 1.0).abs() < 1e-12);
        assert!(shares[3] > 0.0 && shares[8] > 0.0 && shares[2] > 0.0 && shares[7] > 0.0);
        assert_eq!(shares[0] + shares[5], 0.0);
    }

    #[test]
    fn first_digits() {
        assert_eq!(first_digit(0.0032), 3);
        assert_eq!(first_digit(987.0), 9);
        assert_eq!(first_digit(1.0), 1);
        assert_eq!(first_digit(55.5), 5);
    }
}
