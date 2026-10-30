//! Sentinel values used as missing-data encodings: one exact
//! value repeated many times, concentrated in a narrow row
//! window, and sitting far outside the column's central bulk
//! (-999 in a Gaussian column is the canonical case). A sentinel
//! is a code, not a measurement: it pollutes every statistic
//! computed over the column while the null rate reports the
//! column as fully observed. The scan is exact over occurrence
//! positions; the bulk test is a Tukey far-out fence
//! (q1 - 3*IQR, q3 + 3*IQR) computed over the column's other
//! values, plus a sign-boundary rule (a negative value in a
//! non-negative column, or a repeated 0 against strictly
//! positive values, is a code however wide the fence is), so a
//! merely common value in a skewed column (in-bulk) and a
//! scattered far-tail value (no window) both stay quiet.

use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};

use onceover_core::{TypedCol, TypedTable};

/// Minimum occurrences before a value is a sentinel candidate,
/// and the minimum number of other values before a bulk fence is
/// meaningful.
const MIN_OCCURRENCES: usize = 20;
const MIN_OTHERS: usize = 40;
/// The window test: the shortest run of rows covering at least
/// this share of the value's occurrences must span at most
/// WINDOW_MAX_SHARE of the column.
const WINDOW_COVER_SHARE: f64 = 0.80;
const WINDOW_MAX_SHARE: f64 = 0.25;
/// Findings per column are capped (a column with several codes
/// reports its largest few).
const MAX_PER_COLUMN: usize = 3;
/// Above this share of the column, a candidate's own mass moves
/// the quartiles materially, so the fence is recomputed exactly
/// over the other values; below it the whole-column fence is
/// used (the candidate cannot move it enough to matter).
const EXACT_FENCE_SHARE: f64 = 0.05;

pub fn sentinel_values(table: &TypedTable) -> Vec<Finding> {
    let mut out = Vec::new();
    for (i, col) in table.cols.iter().enumerate() {
        // (row, exact key, value) over the non-null entries.
        let entries: Vec<(usize, u64, f64)> = match col {
            TypedCol::F64 { vals } => vals
                .iter()
                .enumerate()
                .filter_map(|(r, v)| v.filter(|x| x.is_finite()).map(|x| (r, x.to_bits(), *x)))
                .collect(),
            TypedCol::I64 { vals } => vals
                .iter()
                .enumerate()
                .filter_map(|(r, v)| v.map(|x| (r, *x as u64, *x as f64)))
                .collect(),
            _ => continue,
        };
        if entries.len() < MIN_OCCURRENCES + MIN_OTHERS {
            continue;
        }
        let mut positions: HashMap<u64, Vec<usize>> = HashMap::new();
        let mut value_of: HashMap<u64, f64> = HashMap::new();
        for &(r, key, x) in &entries {
            positions.entry(key).or_default().push(r);
            value_of.insert(key, x);
        }
        let mut candidates: Vec<(u64, &Vec<usize>)> = positions
            .iter()
            .filter(|(_, p)| p.len() >= MIN_OCCURRENCES)
            .map(|(k, p)| (*k, p))
            .collect();
        candidates.sort_by_key(|(_, p)| std::cmp::Reverse(p.len()));

        let all_values: Vec<f64> = entries.iter().map(|&(_, _, x)| x).collect();
        let whole_fence = fence(&all_values);
        let n_rows = table.n.max(1) as f64;

        let mut emitted = 0;
        for (key, pos) in candidates {
            if emitted >= MAX_PER_COLUMN {
                break;
            }
            let v = value_of[&key];
            let count = pos.len();
            // Shortest window covering >= WINDOW_COVER_SHARE of
            // the occurrences.
            let k = ((count as f64 * WINDOW_COVER_SHARE).ceil() as usize).max(1);
            let (mut best_span, mut best_lo) = (usize::MAX, 0usize);
            for s in 0..=(count - k) {
                let span = pos[s + k - 1] - pos[s] + 1;
                if span < best_span {
                    best_span = span;
                    best_lo = s;
                }
            }
            if best_span as f64 > WINDOW_MAX_SHARE * n_rows {
                continue; // spread out: not a block encoding
            }
            // Bulk fence: exact over the other values when the
            // candidate's mass is material, else the whole column.
            let share = count as f64 / entries.len() as f64;
            let (lo_f, hi_f) = if share > EXACT_FENCE_SHARE {
                let others: Vec<f64> = entries
                    .iter()
                    .filter(|&(_, kk, _)| *kk != key)
                    .map(|&(_, _, x)| x)
                    .collect();
                if others.len() < MIN_OTHERS {
                    continue;
                }
                fence(&others)
            } else {
                whole_fence
            };
            // Boundary rules: a value on the impossible side of
            // the other values' sign boundary is a code however
            // wide the fence is (-1 in a non-negative measure;
            // 0 repeated against strictly positive values).
            let (mut omin, mut omax) = (f64::INFINITY, f64::NEG_INFINITY);
            for &(_, kk, x) in &entries {
                if kk != key {
                    omin = omin.min(x);
                    omax = omax.max(x);
                }
            }
            let boundary = (omin >= 0.0 && v < 0.0)
                || (omax <= 0.0 && v > 0.0)
                || (v == 0.0 && (omin > 0.0 || omax < 0.0));
            if !boundary && v >= lo_f && v <= hi_f {
                continue; // in-bulk: a common value, not a code
            }
            let others_mean = {
                let (mut s, mut n) = (0.0f64, 0usize);
                for &(_, kk, x) in &entries {
                    if kk != key {
                        s += x;
                        n += 1;
                    }
                }
                if n > 0 {
                    s / n as f64
                } else {
                    f64::NAN
                }
            };
            let mean_with = all_values.iter().sum::<f64>() / all_values.len() as f64;
            let win_start = pos[best_lo];
            let win_end = pos[best_lo + k - 1];
            out.push(Finding::new(
                Pattern::SentinelValues,
                BTreeMap::from([("column".into(), json!(table.names[i]))]),
                format!(
                    "column {} uses {} as a probable missing-data sentinel: {} occurrences ({:.1}% of {} rows), {} of them inside rows {}-{} (a {}-row window, {:.1}% of the column); the value sits outside the rest of the column's bulk [{}, {}]; excluding it the column mean is {} (vs {} with it)",
                    table.names[i],
                    fmt_num(v),
                    count,
                    100.0 * count as f64 / n_rows,
                    table.n,
                    k,
                    win_start,
                    win_end,
                    best_span,
                    100.0 * best_span as f64 / n_rows,
                    fmt_num(lo_f),
                    fmt_num(hi_f),
                    fmt_num(others_mean),
                    fmt_num(mean_with),
                ),
                EvidenceBasis::Measured,
                0.5,
                BTreeMap::from([
                    ("value".into(), json!(v)),
                    ("occurrences".into(), json!(count)),
                    ("share".into(), json!(count as f64 / n_rows)),
                    ("window_start_row".into(), json!(win_start)),
                    ("window_end_row".into(), json!(win_end)),
                    ("window_span_rows".into(), json!(best_span)),
                    ("window_occurrences".into(), json!(k)),
                    ("bulk_fence_low".into(), json!(lo_f)),
                    ("bulk_fence_high".into(), json!(hi_f)),
                    ("mean_with".into(), json!(mean_with)),
                    ("mean_excluding".into(), json!(others_mean)),
                    (
                        "method".into(),
                        json!("exact scan: shortest window covering >= 80% of a repeated value's occurrences spans <= 25% of the column, and the value lies outside the Tukey far-out fence (q1 - 3*IQR, q3 + 3*IQR) of the column's other values or across their sign boundary"),
                    ),
                ]),
                Some(
                    "treat this value as missing before computing statistics or fitting models; the column's null rate understates its missingness by the value's share"
                        .into(),
                ),
                Method::new(
                    "sentinels",
                    "sentinel_window_scan",
                    "exact occurrence-window scan with a Tukey far-out fence (onceover v0.1)",
                ),
            ));
            emitted += 1;
        }
    }
    out
}

/// (q1 - 3*IQR, q3 + 3*IQR) over the given values, quartiles by
/// linear interpolation (type-7).
fn fence(values: &[f64]) -> (f64, f64) {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| -> f64 {
        if sorted.is_empty() {
            return f64::NAN;
        }
        let idx = p * (sorted.len() - 1) as f64;
        let lo = idx.floor() as usize;
        let hi = (lo + 1).min(sorted.len() - 1);
        sorted[lo] + (sorted[hi] - sorted[lo]) * (idx - lo as f64)
    };
    let (q1, q3) = (q(0.25), q(0.75));
    let iqr = q3 - q1;
    (q1 - 3.0 * iqr, q3 + 3.0 * iqr)
}

fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.4}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_excludes_a_far_value() {
        let vals: Vec<f64> = (0..1000).map(|i| (i % 50) as f64).collect();
        let (lo, hi) = fence(&vals);
        assert!(lo < 0.0 && hi > 49.0);
        assert!(-999.0 < lo);
    }

    #[test]
    fn fence_of_a_constant_bulk_is_the_point() {
        let vals = vec![5.0; 500];
        let (lo, hi) = fence(&vals);
        assert_eq!((lo, hi), (5.0, 5.0));
    }
}
