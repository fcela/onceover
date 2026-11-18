//! Regression: sampled findings are a function of row content, not row
//! position. The fixture is built from repeated row blocks so the exact
//! duplicate tie-break is exercised as well as the cap itself.

use std::collections::BTreeSet;

use onceover::{profile, Budget, ProfileConfig, ProfileReport};
use polars::prelude::*;
use serde_json::Value;

fn frame(order: &[usize]) -> DataFrame {
    let n = order.len();
    let mut region = Vec::with_capacity(n);
    let mut x = Vec::with_capacity(n);
    let mut y = Vec::with_capacity(n);
    let mut z = Vec::with_capacity(n);
    let mut code = Vec::with_capacity(n);
    let mut label = Vec::with_capacity(n);

    for &i in order {
        // 200 distinct row contents, each repeated six times.
        let k = i % 200;
        let xv = (k as f64 / 200.0).sin() * 10.0;
        region.push(["emea", "amer", "apac", "emea"][k % 4]);
        x.push(Some(xv));
        y.push(Some(2.0 * xv + 1.0));
        z.push(Some(xv * xv));
        code.push(Some((k % 3) as i64));
        label.push(["low", "mid", "high"][k % 3]);
    }

    df! {
        "region" => region,
        "x" => x,
        "y" => y,
        "z" => z,
        "code" => code,
        "label" => label,
    }
    .unwrap()
}

fn shuffled_indices(n: usize) -> Vec<usize> {
    let mut out: Vec<usize> = (0..n).collect();
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    for i in (1..out.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        out.swap(i, j);
    }
    out
}

/// Remove parenthesized row-range annotations such as `(rows 8-1194)`.
/// They are literal original positions, so they are the one part of a
/// row-clusters claim that is expected to move with the input order.
fn strip_row_ranges(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("(rows ") {
        out.push_str(&rest[..start]);
        match rest[start..].find(')') {
            Some(end) => rest = &rest[start + end + 1..],
            None => {
                rest = &rest[start..];
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Drop fields that deliberately describe original positions or are
/// content-addressed from them. Everything else in the ledger — finding
/// membership, statements, evidence metrics, coverage, associations —
/// must be identical for a permutation of the same rows.
fn normalize(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                normalize(item);
            }
        }
        Value::Object(map) => {
            for key in [
                "id",
                "grouped_members",
                "grouped_under",
                "finding_ids",
                "rows_min",
                "rows_max",
            ] {
                map.remove(key);
            }
            for (key, child) in map.iter_mut() {
                if key == "claim" || key == "detail" {
                    if let Value::String(text) = child {
                        *text = strip_row_ranges(text);
                    }
                } else {
                    normalize(child);
                }
            }
        }
        _ => {}
    }
}

fn invariant_view(report: &ProfileReport) -> Value {
    let mut value: Value = serde_json::from_str(&report.to_json()).unwrap();
    // Column cards include first-seen example values, which are exact
    // positional observations rather than sampled findings.
    value.as_object_mut().unwrap().remove("columns");
    // Groups are derived from stable ids that embed those row ranges.
    value.as_object_mut().unwrap().remove("groups");
    normalize(&mut value);
    value
}

#[test]
fn sampled_report_is_invariant_to_row_permutation() {
    let detectors: BTreeSet<String> = [
        "numeric_dependence",
        "association_rules",
        "hotspots",
        "functional_dependencies",
        "order_constraints",
        "duplicate_encodings",
        "predictability",
        "decision_tables",
        "covering_rules",
        "missingness",
        "slice_shifts",
        "row_clusters",
        "identities",
        "interactions",
        "regimes",
        "row_consistency",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let config = ProfileConfig {
        budget: Budget::Standard,
        detectors: Some(detectors),
        sample_rows: Some(400),
        ..Default::default()
    };

    let original = frame(&(0..1200).collect::<Vec<_>>());
    let permuted = frame(&shuffled_indices(1200));

    let a = profile(&original, &config);
    let b = profile(&permuted, &config);
    assert_eq!(invariant_view(&a), invariant_view(&b));
    // The cap must actually bind, or the test would not exercise sampling.
    assert!(a
        .coverage
        .values()
        .any(|c| c.status == "ran_sampled" || c.status == "capped"));
}
