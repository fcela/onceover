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
//!   of integer-valued columns against uniformity (chi-square,
//!   df 9). Suppressed for a column where heaping already
//!   fired: one phenomenon, one finding — the heaping evidence
//!   carries the 0/5 digit shares.
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
use serde_json::json;
use std::collections::BTreeMap;

use onceover_core::{TypedCol, TypedTable};

const MIN_VALUES: usize = 500;
const MIN_DISTINCT: usize = 20;
/// chi-square critical values used as bars: df 9 at ~1e-6 and
/// df 8 at ~3e-5 — strict per-column bars, no per-table
/// correction needed at these levels for tens of columns.
const CHI2_DIGIT_BAR: f64 = 44.0;
const CHI2_BENFORD_BAR: f64 = 34.0;
const BENFORD_MAD_BAR: f64 = 0.012;

pub fn digit_fingerprints(table: &TypedTable) -> (Vec<Finding>, String) {
    let mut out = Vec::new();
    let mut scanned = 0usize;
    let mut benford_eligible = 0usize;
    for (i, col) in table.cols.iter().enumerate() {
        let vals: Vec<f64> = match col {
            TypedCol::F64 { vals } => vals
                .iter()
                .filter_map(|v| v.filter(|x| x.is_finite()).copied())
                .collect(),
            TypedCol::I64 { vals } => vals.iter().filter_map(|v| v.map(|x| *x as f64)).collect(),
            TypedCol::U64 { vals } => vals.iter().filter_map(|v| v.map(|x| *x as f64)).collect(),
            _ => continue,
        };
        if vals.len() < MIN_VALUES {
            continue;
        }
        scanned += 1;
        let name = &table.names[i];

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
        if benford(name, &vals, &mut out) {
            benford_eligible += 1;
        }
    }
    let cov = format!(
        "{} numeric columns digit-scanned; {} Benford-eligible (others fail the positivity, span, or distinctness eligibility recorded in the detector docs)",
        scanned, benford_eligible
    );
    (out, cov)
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

fn terminal_digit(name: &str, vals: &[f64], out: &mut Vec<Finding>) {
    let mut counts = [0usize; 10];
    for v in vals {
        counts[((*v as i64).unsigned_abs() % 10) as usize] += 1;
    }
    let n = vals.len() as f64;
    let expected = n / 10.0;
    let chi2: f64 = counts
        .iter()
        .map(|&c| (c as f64 - expected).powi(2) / expected)
        .sum();
    let (top_digit, top_count) = counts.iter().enumerate().max_by_key(|&(_, &c)| c).unwrap();
    let top_share = *top_count as f64 / n;
    if chi2 < CHI2_DIGIT_BAR || top_share < 0.15 {
        return;
    }
    out.push(Finding::new(
        Pattern::DigitPreference,
        BTreeMap::from([("column".into(), json!(name))]),
        format!(
            "column {} has a terminal-digit preference: digit {} ends {:.1}% of values against 10% under uniformity (chi-square {:.1} on 9 df, bar {:.0})",
            name,
            top_digit,
            100.0 * top_share,
            chi2,
            CHI2_DIGIT_BAR,
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
            (
                "method".into(),
                json!("last-digit distribution over all values vs uniform; chi-square on 9 df"),
            ),
        ]),
        Some(
            "a recording process favors this terminal digit; low-order digits carry less information than their precision suggests"
                .into(),
        ),
        Method::new(
            "digits",
            "terminal_digit_scan",
            "exact last-digit counts vs uniformity (onceover v0.1)",
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
    fn first_digits() {
        assert_eq!(first_digit(0.0032), 3);
        assert_eq!(first_digit(987.0), 9);
        assert_eq!(first_digit(1.0), 1);
        assert_eq!(first_digit(55.5), 5);
    }
}
