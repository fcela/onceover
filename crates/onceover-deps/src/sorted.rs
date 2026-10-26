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
        }
    }
    out
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
}
