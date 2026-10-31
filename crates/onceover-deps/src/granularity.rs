//! Granularity changes in float columns: a series measured (or
//! recorded) at full precision that becomes integer-valued partway
//! — or the reverse — has changed how it was produced or stored at
//! that row, and summaries that assume one regime misread the
//! other. The scan is exact: for each float column, the share of
//! integer-valued entries on each side of the best split of the
//! row-order sequence.

use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;
use std::collections::BTreeMap;

use onceover_core::{TypedCol, TypedTable};

/// Minimum non-null values before a split is judged, and the
/// minimum on each side of it.
const MIN_VALUES: usize = 400;
const MIN_SIDE: usize = 200;

pub fn granularity_changes(table: &TypedTable) -> Vec<Finding> {
    let mut out = Vec::new();
    for (i, col) in table.cols.iter().enumerate() {
        let TypedCol::F64 { vals } = col else {
            continue;
        };
        // (row, is_integer) over the non-null values in row order.
        let seq: Vec<(usize, bool)> = vals
            .iter()
            .enumerate()
            .filter_map(|(r, v)| {
                v.map(|x| {
                    (
                        r,
                        x.is_finite() && x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0,
                    )
                })
            })
            .collect();
        if seq.len() < MIN_VALUES {
            continue;
        }
        let total_int = seq.iter().filter(|&(_, b)| *b).count();
        let share = total_int as f64 / seq.len() as f64;
        if !(0.02..=0.98).contains(&share) {
            continue; // one regime throughout: nothing changed
        }
        let Some((k, sl, sr)) = best_split(&seq, total_int) else {
            continue;
        };
        let (lo, hi) = if sl <= sr { (sl, sr) } else { (sr, sl) };
        if lo > 0.25 || hi < 0.75 {
            continue; // no decisive regime change
        }
        let change_row = seq[k].0;
        let becomes = sr > sl;
        out.push(Finding::new(
            Pattern::FormatVariation,
            BTreeMap::from([("column".into(), json!(table.names[i]))]),
            format!(
                "column {} changes granularity at row {}: {} integer-valued from there (integer share {:.0}% before, {:.0}% after)",
                table.names[i],
                change_row,
                if becomes { "values are" } else { "values stop being" },
                sl * 100.0,
                sr * 100.0,
            ),
            EvidenceBasis::Measured,
            0.5,
            BTreeMap::from([
                ("change_row".into(), json!(change_row)),
                ("values_before".into(), json!(k)),
                ("values_after".into(), json!(seq.len() - k)),
                ("integer_share_before".into(), json!(sl)),
                ("integer_share_after".into(), json!(sr)),
                (
                    "method".into(),
                    json!("exact scan: best split of the integer-valued share over the row-order sequence, both sides >= 200 values"),
                ),
            ]),
            Some(
                "a recording or rounding regime changed at the seam; do not pool precision-sensitive summaries across it"
                    .into(),
            ),
            Method::new(
                "granularity",
                "integer_share_split",
                "exact integer-share split scan (onceover v0.1)",
            ),
        ));
    }
    out
}

/// Best split of the (row, is_integer) sequence by
/// |share_left - share_right|, both sides at least MIN_SIDE long.
/// Returns (split index into the sequence, share_left, share_right).
fn best_split(seq: &[(usize, bool)], total_int: usize) -> Option<(usize, f64, f64)> {
    let mut left_int = 0usize;
    let mut best: Option<(usize, f64, f64, f64)> = None; // (k, |sl - sr|, sl, sr)
    for k in 1..=seq.len() {
        if seq[k - 1].1 {
            left_int += 1;
        }
        if k < MIN_SIDE || seq.len() - k < MIN_SIDE {
            continue;
        }
        let sl = left_int as f64 / k as f64;
        let sr = (total_int - left_int) as f64 / (seq.len() - k) as f64;
        let d = (sl - sr).abs();
        if best.map(|(_, bd, _, _)| d > bd).unwrap_or(true) {
            best = Some((k, d, sl, sr));
        }
    }
    best.map(|(k, _, sl, sr)| (k, sl, sr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_finds_the_seam() {
        let mut seq: Vec<(usize, bool)> = (0..1000).map(|r| (r, false)).collect();
        seq.extend((1000..2000).map(|r| (r, true)));
        let (k, sl, sr) = best_split(&seq, 1000).unwrap();
        assert_eq!(k, 1000);
        assert!(sl < 0.01 && sr > 0.99);
        // No decisive change in a uniformly mixed sequence.
        let mixed: Vec<(usize, bool)> = (0..1000).map(|r| (r, r % 2 == 0)).collect();
        let (_, sl, sr) = best_split(&mixed, 500).unwrap();
        assert!((sl - sr).abs() < 0.1);
    }
}
