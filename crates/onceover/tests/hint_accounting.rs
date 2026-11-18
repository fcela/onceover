//! Regression tests for the declared-hint contract (issue #441):
//! every declared `roles`, `level_order`, `time`, and
//! `ordered_axis` item is applied or refused in the declarations
//! coverage entry, including when the named column exists but is
//! unusable for the declaration.

use onceover::{profile, ProfileConfig};
use onceover_report::ProfileHints;
use polars::prelude::*;
use std::collections::BTreeMap;

fn fixture() -> DataFrame {
    let n = 60;
    let cat: Vec<Option<&str>> = (0..n)
        .map(|i| Some(if i % 2 == 0 { "g0" } else { "g1" }))
        .collect();
    let dose: Vec<Option<i64>> = (0..n).map(|i| Some(i as i64)).collect();
    let value: Vec<Option<f64>> = (0..n).map(|i| Some(i as f64 * 0.5)).collect();
    DataFrame::new_infer_height(vec![
        Series::new("cat".into(), cat).into_column(),
        Series::new("dose".into(), dose).into_column(),
        Series::new("value".into(), value).into_column(),
    ])
    .unwrap()
}

fn declarations_detail(hints: ProfileHints) -> String {
    let report = profile(
        &fixture(),
        &ProfileConfig {
            hints: Some(hints),
            ..Default::default()
        },
    );
    report
        .coverage
        .get("declarations")
        .expect("structural hints must be accounted in coverage")
        .detail
        .clone()
}

#[test]
fn unusable_hints_are_refused_in_declarations_coverage() {
    let detail = declarations_detail(ProfileHints {
        roles: BTreeMap::from([
            ("cat".to_string(), "bogus_role".to_string()),
            ("nope".to_string(), "measure".to_string()),
        ]),
        level_order: BTreeMap::from([
            (
                "cat".to_string(),
                vec!["g0".to_string(), "nope_level".to_string()],
            ),
            ("nope".to_string(), vec!["x".to_string()]),
        ]),
        time: Some("cat".to_string()),
        ordered_axis: Some("cat".to_string()),
        ..Default::default()
    });

    assert!(
        detail.contains("roles: 0 declaration(s) applied"),
        "{detail}"
    );
    assert!(detail.contains("'cat' declares 'bogus_role'"), "{detail}");
    assert!(
        detail.contains("'nope' (not a column of the table)"),
        "{detail}"
    );
    assert!(
        detail.contains("level_order: 0 declaration(s) applied"),
        "{detail}"
    );
    assert!(
        detail.contains("level(s) absent from the column: nope_level"),
        "{detail}"
    );
    assert!(
        detail.contains("time: 'cat' refused — an existing column"),
        "{detail}"
    );
    assert!(
        detail.contains("ordered_axis: 'cat' refused — an existing column"),
        "{detail}"
    );
}

#[test]
fn usable_hints_are_applied_in_declarations_coverage() {
    let detail = declarations_detail(ProfileHints {
        roles: BTreeMap::from([("dose".to_string(), "measure".to_string())]),
        level_order: BTreeMap::from([(
            "cat".to_string(),
            vec!["g0".to_string(), "g1".to_string()],
        )]),
        time: Some("dose".to_string()),
        ordered_axis: Some("dose".to_string()),
        ..Default::default()
    });

    assert!(
        detail.contains("roles: 1 declaration(s) applied"),
        "{detail}"
    );
    assert!(
        detail.contains("level_order: 1 declaration(s) applied"),
        "{detail}"
    );
    assert!(detail.contains("time: 'dose' applied"), "{detail}");
    assert!(detail.contains("ordered_axis: 'dose' applied"), "{detail}");
    assert!(!detail.contains("refused"), "{detail}");
}
