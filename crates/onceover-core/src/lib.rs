//! onceover-core: typed extraction from polars, column cards, value counts,
//! concentration measures, and duplicate detection. The shared primitives of
//! the profiler.

pub mod card;
pub mod counts;
pub mod dateutil;
pub mod dup;
pub mod sampling;
pub mod typed;
pub use sampling::sample_rows;

pub use card::{column_card, CardOut};
pub use counts::{build_counts, ColCounts, VKey};
pub use dup::{find_duplicate_rows, DupInfo};
pub use typed::{
    dtype_name, extract, extract_series, NumericValues, TypedCol, TypedTable,
};

#[cfg(test)]
mod tests {
    use super::*;
    use polars::df;

    #[test]
    fn end_to_end_extraction() {
        let df = df! {
            "id" => [1i64, 2, 3, 4],
            "cat" => ["a", "b", "a", "b"],
        }
        .unwrap();
        let t = extract(&df);
        assert_eq!(t.n, 4);
        let out = column_card("id", "int64", &t.cols[0]);
        assert_eq!(out.card.role, "identifier");
        let out = column_card("cat", "string", &t.cols[1]);
        assert_eq!(out.card.role, "categorical");
    }
}
