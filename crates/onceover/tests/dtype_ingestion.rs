//! Regression tests for issue #75: profile() must accept frames whose
//! columns use any Polars dtype. Series construction for the narrow
//! integer widths, decimal, struct and fixed-size array dtypes is
//! feature-gated in polars; with the gates off, ingestion failed with
//! "cannot create series from <dtype>" (and panicked for decimal).
//! The narrow widths must be profiled as numbers; struct and array
//! columns are opaque to the profiler but must not break it.
//!
//! Issue #211: decimal columns must be analysed as numbers, not
//! merely accepted — a numeric card, a measure role, and visibility
//! to the numeric passes.

use onceover::profile_default;
use onceover::Category;
use polars::prelude::*;

fn cast_col(name: &str, values: &[i32], dtype: DataType) -> Column {
    Series::new(name.into(), values)
        .cast(&dtype)
        .unwrap()
        .into_column()
}

#[test]
fn narrow_integer_widths_profile_as_numbers() {
    for dtype in [
        DataType::Int8,
        DataType::Int16,
        DataType::UInt8,
        DataType::UInt16,
    ] {
        let df = DataFrame::new_infer_height(vec![cast_col(
            "x",
            &[1, 2, 3, 4, 5, 6, 7, 8],
            dtype.clone(),
        )])
        .unwrap();
        let rep = profile_default(&df);
        let card = &rep.columns["x"];
        assert_eq!(card.name, "x");
        assert_eq!(card.n, 8);
        assert_eq!(card.distinct, 8, "{dtype:?} column must be read as values");
    }
}

#[test]
fn opaque_dtypes_do_not_break_profiling() {
    let values: Vec<i32> = (0..8).collect();
    let plain = Series::new("n".into(), &values).into_column();
    let strukt =
        StructChunked::from_series("s".into(), 8, [Series::new("a".into(), &values)].iter())
            .unwrap()
            .into_column();
    let df = DataFrame::new_infer_height(vec![plain, strukt]).unwrap();
    let rep = profile_default(&df);
    assert_eq!(rep.columns.len(), 2);
}

fn decimal_col(name: &str, values: &[String], scale: usize) -> Column {
    Series::new(
        name.into(),
        values.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .cast(&DataType::Decimal(12, scale))
    .unwrap()
    .into_column()
}

/// 120 money-like values: 40 distinct quarter steps, each seen 3 times.
fn decimal_fixture() -> (Vec<f64>, Column) {
    let vals: Vec<f64> = (0..120).map(|i| (i % 40) as f64 * 0.25 + 0.50).collect();
    let strs: Vec<String> = vals.iter().map(|v| format!("{v:.2}")).collect();
    (vals, decimal_col("price", &strs, 2))
}

#[test]
fn decimal_profiles_as_a_number() {
    let (vals, col) = decimal_fixture();
    let df = DataFrame::new_infer_height(vec![col]).unwrap();
    let rep = profile_default(&df);
    let card = &rep.columns["price"];
    assert_eq!(card.dtype, "decimal");
    assert_eq!(card.n, 120);
    assert_eq!(card.null_count, 0);
    assert_eq!(
        card.distinct, 40,
        "decimal values must be read, not dropped"
    );
    assert_eq!(card.role, "continuous");
    assert_eq!(card.semantic_role.as_deref(), Some("measure"));
    let stats = card
        .numeric_stats
        .as_ref()
        .expect("a decimal column must carry numeric statistics");
    let expected_mean = vals.iter().sum::<f64>() / vals.len() as f64;
    assert_eq!(stats.min, Some(0.5));
    assert_eq!(stats.max, Some(10.25));
    assert_eq!(stats.mean, Some(expected_mean));
}

#[test]
fn decimal_with_nulls_profiles_and_counts_nulls() {
    let s = Series::new(
        "price".into(),
        &[
            Some("1.50"),
            None,
            Some("2.50"),
            None,
            Some("3.50"),
            Some("4.50"),
        ],
    )
    .cast(&DataType::Decimal(10, 2))
    .unwrap()
    .into_column();
    let df = DataFrame::new_infer_height(vec![s]).unwrap();
    let rep = profile_default(&df);
    let card = &rep.columns["price"];
    assert_eq!(card.n, 6);
    assert_eq!(card.null_count, 2);
    assert_eq!(card.distinct, 4);
    let stats = card
        .numeric_stats
        .as_ref()
        .expect("a decimal column with nulls must still carry statistics");
    assert_eq!(stats.mean, Some(3.0));
}

#[test]
fn decimal_affine_identity_is_detected() {
    // double_price = 2 * price exactly, both decimal: the identity
    // pass must see the pair, as it does for float columns.
    let prices: Vec<f64> = (0..240)
        .map(|i| (i * 37 % 40) as f64 * 0.25 + 1.0)
        .collect();
    let price_strs: Vec<String> = prices.iter().map(|v| format!("{v:.2}")).collect();
    let double_strs: Vec<String> = prices.iter().map(|v| format!("{:.2}", v * 2.0)).collect();
    let df = DataFrame::new_infer_height(vec![
        decimal_col("price", &price_strs, 2),
        decimal_col("double_price", &double_strs, 2),
    ])
    .unwrap();
    let rep = profile_default(&df);
    let found = rep
        .findings
        .iter()
        .chain(rep.grouped_findings.iter())
        .any(|f| {
            f.is(Category::Dependencies, Some("functional"))
                && f.specification.form.as_deref() == Some("affine")
                && f.subject.target.as_deref() == Some("double_price")
                && f.subject.inputs == ["price"]
        });
    assert!(
        found,
        "the affine identity between two decimal columns must be reported"
    );
}
