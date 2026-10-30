//! Sortedness checks (OD-lite): positive claims only — a column being
//! non-decreasing with row order is a strong structural signal (e.g.,
//! "rows are in time order") that agents can rely on.

use onceover_report::{Method, Pattern};
use std::cmp::Ordering;

use onceover_report::{EvidenceBasis, Finding};
use serde_json::json;

use onceover_core::{TypedCol, TypedTable};

pub fn sortedness(table: &TypedTable) -> Vec<Finding> {
    let mut out = Vec::new();
    for (i, col) in table.cols.iter().enumerate() {
        let n_nonnull = col.len() - col.null_count();
        if n_nonnull < 100 {
            continue;
        }
        let is_sorted = match col {
            TypedCol::I64 { vals } => check_sorted(vals.iter().flatten().copied()),
            TypedCol::F64 { vals } => check_sorted(vals.iter().flatten().copied()),
            TypedCol::Date { vals } => check_sorted(vals.iter().flatten().copied()),
            TypedCol::Datetime { vals } => check_sorted(vals.iter().flatten().copied()),
            _ => continue,
        };
        if let Some(strictly) = is_sorted {
            out.push(Finding::new(
                Pattern::ColumnOrdering,
                std::iter::once(("column".to_string(), json!(table.names[i]))).collect(),
                format!(
                    "column {} is {} with row order",
                    table.names[i],
                    if strictly {
                        "strictly increasing"
                    } else {
                        "non-decreasing"
                    }
                ),
                EvidenceBasis::Measured,
                0.28,
                evidence_map(strictly),
                if strictly {
                    Some("row order mirrors this column; safe for range-scan reasoning".into())
                } else {
                    Some("rows appear ordered by this column".into())
                },
                Method::new(
                    "sortedness",
                    "adjacent_value_scan",
                    "OD-lite sortedness check (onceover v0.1)",
                ),
            ));
        } else {
            // Not sorted — but a column that is ordered except for a
            // late block whose values return to earlier territory is
            // the shape of rows appended out of order (a backfill),
            // which is worth a finding of its own: by-row-order
            // reasoning misplaces the block.
            let scan = match col {
                TypedCol::I64 { vals } => backfill_scan(
                    &vals
                        .iter()
                        .enumerate()
                        .filter_map(|(r, v)| v.map(|x| (r, *x as i128)))
                        .collect::<Vec<_>>(),
                ),
                TypedCol::F64 { vals } => backfill_scan(
                    &vals
                        .iter()
                        .enumerate()
                        .filter_map(|(r, v)| v.map(|x| (r, x)))
                        .collect::<Vec<_>>(),
                ),
                TypedCol::Date { vals } => backfill_scan(
                    &vals
                        .iter()
                        .enumerate()
                        .filter_map(|(r, v)| (*v).map(|x| (r, x as i128)))
                        .collect::<Vec<_>>(),
                ),
                TypedCol::Datetime { vals } => backfill_scan(
                    &vals
                        .iter()
                        .enumerate()
                        .filter_map(|(r, v)| (*v).map(|x| (r, x as i128)))
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            };
            if let Some(b) = scan {
                out.push(Finding::new(
                    Pattern::ColumnOrdering,
                    std::iter::once(("column".to_string(), json!(table.names[i])))
                        .collect(),
                    format!(
                        "column {} is ordered except for a late block: from row {} the values return to earlier territory ({} out-of-order seam{}, {} rows) — the shape of rows appended out of order",
                        table.names[i],
                        b.first_descent_row,
                        b.descents,
                        if b.descents == 1 { "" } else { "s" },
                        b.tail_rows,
                    ),
                    EvidenceBasis::Measured,
                    0.55,
                    std::collections::BTreeMap::from([
                        ("descents".to_string(), json!(b.descents)),
                        (
                            "first_descent_row".to_string(),
                            json!(b.first_descent_row),
                        ),
                        ("tail_rows".to_string(), json!(b.tail_rows)),
                        (
                            "method".to_string(),
                            json!("exact scan of adjacent non-null values: at most 3 descents, and no value after the first descent exceeds the peak before it"),
                        ),
                    ]),
                    Some(
                        "if this column is an axis, analysis by row order will misplace the late block; sort by the column or treat the block separately"
                            .into(),
                    ),
                    Method::new(
                        "sortedness",
                        "backfill_scan",
                        "OD-lite sortedness check (onceover v0.1)",
                    ),
                ));
            }
        }
    }
    out
}

struct Backfill {
    descents: usize,
    first_descent_row: usize,
    tail_rows: usize,
}

/// The backfill signature over a (row, key) sequence in row order:
/// at least one descent but at most three, a tail of at least three
/// values after the first descent, and no tail value above the peak
/// reached before that descent (the prefix before the first descent
/// is non-decreasing, so its last value is its peak). A genuinely
/// unsorted column fails the descent cap; a single noisy dip that
/// later climbs to new highs fails the peak rule.
fn backfill_scan<K: PartialOrd + Copy>(seq: &[(usize, K)]) -> Option<Backfill> {
    let n = seq.len();
    if n < 100 {
        return None;
    }
    let mut descents = 0usize;
    let mut first: Option<usize> = None;
    for j in 1..n {
        if seq[j].1 < seq[j - 1].1 {
            descents += 1;
            if first.is_none() {
                first = Some(j);
            }
        }
    }
    if descents == 0 || descents > 3 {
        return None;
    }
    let first = first?;
    let tail_rows = n - first;
    if tail_rows < 3 {
        return None;
    }
    let peak = seq[first - 1].1;
    if seq[first..].iter().any(|&(_, k)| k > peak) {
        return None;
    }
    Some(Backfill {
        descents,
        first_descent_row: seq[first].0,
        tail_rows,
    })
}

fn evidence_map(strictly: bool) -> std::collections::BTreeMap<String, serde_json::Value> {
    std::collections::BTreeMap::from([
        ("strictly_increasing".to_string(), json!(strictly)),
        (
            "method".to_string(),
            json!("exact scan of adjacent non-null values"),
        ),
    ])
}

/// Returns Some(strictly_increasing) when sorted (non-decreasing);
/// None when not sorted.
fn check_sorted<V, I>(vals: I) -> Option<bool>
where
    V: PartialOrd,
    I: Iterator<Item = V>,
{
    let mut prev: Option<V> = None;
    let mut saw_equal = false;
    for v in vals {
        if let Some(p) = &prev {
            match v.partial_cmp(p) {
                Some(Ordering::Less) => return None,
                Some(Ordering::Equal) => saw_equal = true,
                _ => {}
            }
        }
        prev = Some(v);
    }
    Some(!saw_equal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_sorted_and_unsorted() {
        assert_eq!(check_sorted(vec![1, 2, 2, 3].into_iter()), Some(false));
        assert_eq!(check_sorted(vec![1, 2, 3, 4].into_iter()), Some(true));
        assert_eq!(check_sorted(vec![1, 3, 2].into_iter()), None);
        assert_eq!(check_sorted(Vec::<i64>::new().into_iter()), Some(true));
    }

    fn seq(vals: &[i64]) -> Vec<(usize, i128)> {
        vals.iter().enumerate().map(|(r, v)| (r, *v as i128)).collect()
    }

    #[test]
    fn backfill_signature_fires_and_rejects() {
        // 120 ordered values, then a 10-value block from earlier
        // territory appended at the end: one seam at row 120.
        let mut vals: Vec<i64> = (0..120).collect();
        vals.extend(10..20);
        let b = backfill_scan(&seq(&vals)).expect("backfill should fire");
        assert_eq!(b.descents, 1);
        assert_eq!(b.first_descent_row, 120);
        assert_eq!(b.tail_rows, 10);
        // Fully sorted: no descents.
        assert!(backfill_scan(&seq(&(0..130).collect::<Vec<_>>())).is_none());
        // A dip that later climbs to new highs is not a backfill.
        let mut dip: Vec<i64> = (0..120).collect();
        dip[60] = 5;
        assert!(backfill_scan(&seq(&dip)).is_none());
        // Many descents (shuffled): rejected by the cap.
        let shuffled: Vec<i64> = (0..130).map(|i| (i * 37) % 130).collect();
        assert!(backfill_scan(&seq(&shuffled)).is_none());
        // Too short to judge.
        assert!(backfill_scan(&seq(&vals[..50])).is_none());
    }
}
