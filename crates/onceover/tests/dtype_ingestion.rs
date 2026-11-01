//! Regression tests for issue #75: profile() must accept frames whose
//! columns use any Polars dtype. Series construction for the narrow
//! integer widths, decimal, struct and fixed-size array dtypes is
//! feature-gated in polars; with the gates off, ingestion failed with
//! "cannot create series from <dtype>" (and panicked for decimal).
//! The narrow widths must be profiled as numbers; decimal, struct and
//! array columns are opaque to the profiler but must not break it.

use onceover::profile_default;
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
