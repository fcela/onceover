//! Zero-copy typed extraction from a polars `DataFrame` into onceover's
//! internal column representation. All heavier passes operate on these
//! compact columns; polars API surface is confined to this module.

use polars::prelude::*;
use polars_arrow::bitmap::Bitmap;

/// Int64/Float64 columns borrow dense or nullable Arrow buffers, including
/// multiple chunks. Conversions from other numeric dtypes use an owned fallback.
#[derive(Debug)]
pub enum NumericValues<'a, T> {
    Borrowed(&'a [T]),
    Chunked(Vec<NumericChunk<'a, T>>),
    Owned(Vec<Option<T>>),
}

#[derive(Debug)]
pub struct NumericChunk<'a, T> {
    end: usize,
    values: &'a [T],
    validity: Option<&'a Bitmap>,
}

pub trait NumericScalar: Copy {
    fn canonical_ref(&self) -> Option<&Self>;
}
impl NumericScalar for i64 {
    fn canonical_ref(&self) -> Option<&Self> {
        Some(self)
    }
}
impl NumericScalar for u64 {
    fn canonical_ref(&self) -> Option<&Self> {
        Some(self)
    }
}
impl NumericScalar for f64 {
    fn canonical_ref(&self) -> Option<&Self> {
        if !self.is_finite() {
            None
        } else if *self == 0.0 {
            Some(&0.0)
        } else {
            Some(self)
        }
    }
}

impl<'a, T: NumericScalar> NumericValues<'a, T> {
    pub fn len(&self) -> usize {
        match self {
            Self::Borrowed(v) => v.len(),
            Self::Owned(v) => v.len(),
            Self::Chunked(chunks) => chunks.last().map_or(0, |c| c.end),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn get_ref(&self, i: usize) -> Option<&T> {
        match self {
            Self::Borrowed(v) => Some(&v[i]),
            Self::Owned(v) => v[i].as_ref(),
            Self::Chunked(chunks) => {
                let c = &chunks[chunks.partition_point(|c| c.end <= i)];
                let local = i - (c.end - c.values.len());
                if c.validity.is_some_and(|valid| !valid.get_bit(local)) {
                    return None;
                }
                c.values[local].canonical_ref()
            }
        }
    }
    pub fn at(&self, i: usize) -> Option<T> {
        self.get_ref(i).copied()
    }
    pub fn get(&self, i: usize) -> Option<Option<T>> {
        (i < self.len()).then(|| self.at(i))
    }
    pub fn iter<'s>(
        &'s self,
    ) -> impl DoubleEndedIterator<Item = Option<&'s T>> + Clone + use<'a, 's, T> {
        (0..self.len()).map(move |i| self.get_ref(i))
    }
}

impl<T> From<Vec<Option<T>>> for NumericValues<'_, T> {
    fn from(v: Vec<Option<T>>) -> Self {
        Self::Owned(v)
    }
}

impl<T: NumericScalar + PartialEq> PartialEq<Vec<Option<T>>> for NumericValues<'_, T> {
    fn eq(&self, rhs: &Vec<Option<T>>) -> bool {
        self.len() == rhs.len() && self.iter().map(|v| v.copied()).eq(rhs.iter().copied())
    }
}

/// A column extracted into onceover's normalized representation.
/// `Str` borrows directly from the source DataFrame (zero-copy).
#[derive(Debug)]
pub enum TypedCol<'a> {
    /// Signed integers and uint8..32 (all represented losslessly as i64).
    I64 {
        vals: NumericValues<'a, i64>,
    },
    /// Unsigned integers retain all 64 bits for exact comparisons.
    U64 {
        vals: NumericValues<'a, u64>,
    },
    /// Floats (NaN/inf treated as null; -0.0 normalized to +0.0).
    /// Decimal columns are converted into this variant at extraction,
    /// and so are Duration columns (converted to seconds).
    F64 {
        vals: NumericValues<'a, f64>,
    },
    /// Strings (borrowed, zero-copy).
    Str {
        vals: Vec<Option<&'a str>>,
    },
    /// Strings materialized from categorical/enum columns (owned copies).
    StrOwned {
        vals: Vec<Option<String>>,
    },
    Bool {
        vals: Vec<Option<bool>>,
    },
    /// Date, as days since epoch.
    Date {
        vals: Vec<Option<i32>>,
    },
    /// Datetime ticks retain the source unit for exact comparisons.
    Datetime {
        vals: Vec<Option<i64>>,
        unit: TimeUnit,
    },
    /// A column that is entirely null.
    Null {
        n: usize,
    },
    /// Unsupported dtype (list/struct/binary/...) — opaque to the profiler.
    /// Time-of-day columns also land here, by decision rather than by
    /// omission: a time of day is a circular quantity (23:00 is closer
    /// to 01:00 than to 12:00), so the order statistics and measures
    /// the numeric kinds receive would mislead. Time columns are
    /// counted for rows and nulls, and coverage states that they are
    /// not analysed.
    Other {
        n: usize,
        null_count: usize,
    },
}

impl<'a> TypedCol<'a> {
    pub fn len(&self) -> usize {
        match self {
            TypedCol::I64 { vals } => vals.len(),
            TypedCol::U64 { vals } => vals.len(),
            TypedCol::F64 { vals } => vals.len(),
            TypedCol::Str { vals } => vals.len(),
            TypedCol::StrOwned { vals } => vals.len(),
            TypedCol::Bool { vals } => vals.len(),
            TypedCol::Date { vals } => vals.len(),
            TypedCol::Datetime { vals, .. } => vals.len(),
            TypedCol::Null { n } | TypedCol::Other { n, .. } => *n,
        }
    }

    pub fn null_count(&self) -> usize {
        match self {
            TypedCol::I64 { vals } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::U64 { vals } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::F64 { vals } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::Str { vals } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::StrOwned { vals } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::Bool { vals } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::Date { vals } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::Datetime { vals, .. } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::Null { n } => *n,
            TypedCol::Other { null_count, .. } => *null_count,
        }
    }

    /// True when the value at row `r` is null. Null and
    /// non-comparable columns are null at every row.
    pub fn is_null_at(&self, r: usize) -> bool {
        match self {
            TypedCol::I64 { vals } => vals.at(r).is_none(),
            TypedCol::U64 { vals } => vals.at(r).is_none(),
            TypedCol::F64 { vals } => vals.at(r).is_none(),
            TypedCol::Str { vals } => vals[r].is_none(),
            TypedCol::StrOwned { vals } => vals[r].is_none(),
            TypedCol::Bool { vals } => vals[r].is_none(),
            TypedCol::Date { vals } => vals[r].is_none(),
            TypedCol::Datetime { vals, .. } => vals[r].is_none(),
            TypedCol::Null { .. } => true,
            TypedCol::Other { .. } => true,
        }
    }

    /// The value at row `r` as an f64, for the numeric column
    /// kinds (int, uint, float); `None` for nulls and for
    /// non-numeric columns.
    pub fn numeric_at(&self, r: usize) -> Option<f64> {
        match self {
            TypedCol::I64 { vals } => vals.at(r).map(|v| v as f64),
            TypedCol::U64 { vals } => vals.at(r).map(|v| v as f64),
            TypedCol::F64 { vals } => vals.at(r),
            _ => None,
        }
    }

    /// True for borrowed or owned string columns.
    pub fn is_string(&self) -> bool {
        matches!(self, TypedCol::Str { .. } | TypedCol::StrOwned { .. })
    }

    /// String value accessor for both string variants.
    pub fn str_at(&self, i: usize) -> Option<&str> {
        match self {
            TypedCol::Str { vals } => vals[i],
            TypedCol::StrOwned { vals } => vals[i].as_deref(),
            _ => None,
        }
    }

    /// Render one cell as a display string for evidence payloads
    /// (example values in findings). Nulls and non-comparable columns
    /// render as "null".
    pub fn render_cell(&self, i: usize) -> String {
        match self {
            TypedCol::I64 { vals } => vals
                .at(i)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".into()),
            TypedCol::U64 { vals } => vals
                .at(i)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".into()),
            TypedCol::F64 { vals } => vals
                .at(i)
                .map(|v| format!("{v}"))
                .unwrap_or_else(|| "null".into()),
            TypedCol::Str { vals } => vals[i].unwrap_or("null").to_string(),
            TypedCol::StrOwned { vals } => vals[i].as_deref().unwrap_or("null").to_string(),
            TypedCol::Bool { vals } => vals[i]
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".into()),
            TypedCol::Date { vals } => vals[i]
                .map(|v| crate::dateutil::iso_date(v as i64))
                .unwrap_or_else(|| "null".into()),
            TypedCol::Datetime { vals, unit } => vals[i]
                .map(|v| crate::dateutil::iso_timestamp(v, *unit))
                .unwrap_or_else(|| "null".into()),
            TypedCol::Null { .. } | TypedCol::Other { .. } => "null".into(),
        }
    }

    /// Calendar projection is separate from exact source ticks.
    pub fn datetime_days(&self, row: usize) -> Option<i64> {
        match self {
            Self::Datetime { vals, unit } => {
                vals[row].map(|v| crate::dateutil::timestamp_days(v, *unit))
            }
            _ => None,
        }
    }

    /// Microseconds for legacy temporal models; unrepresentable epochs
    /// remain in the exact integrity passes but are excluded from models.
    pub fn datetime_micros(&self, row: usize) -> Option<i64> {
        match self {
            Self::Datetime { vals, unit } => vals[row].and_then(|v| match unit {
                TimeUnit::Milliseconds => v.checked_mul(1000),
                TimeUnit::Microseconds => Some(v),
                TimeUnit::Nanoseconds => Some(v.div_euclid(1000)),
            }),
            _ => None,
        }
    }

    /// Nanoseconds per tick for a datetime column (1, 1_000, or
    /// 1_000_000 by unit); `None` for other column kinds. Exact
    /// passes that must not confuse ticks of different units read
    /// the unit through here rather than naming the polars type.
    pub fn datetime_unit_ns(&self) -> Option<u64> {
        match self {
            Self::Datetime { unit, .. } => Some(match unit {
                TimeUnit::Nanoseconds => 1,
                TimeUnit::Microseconds => 1_000,
                TimeUnit::Milliseconds => 1_000_000,
            }),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// True when this column can act as a numeric feature (for screening).
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. }
        )
    }
}

/// Whether a column is a two-valued flag: a boolean column with
/// both states present, or a numeric column whose non-null values
/// are all 0 or 1 with both values present. Nulls are not a state.
/// This is the one definition of a flag in the library: the frame
/// model's role assignment and the panel passes' flag scan both
/// read it, so the two cannot drift apart.
pub fn is_two_valued_flag(table: &TypedTable, col: usize) -> bool {
    two_valued_flag_states(table, col).is_some()
}

/// A column's per-row flag states when it is a two-valued flag (see
/// [`is_two_valued_flag`]); `None` otherwise.
pub fn two_valued_flag_states(table: &TypedTable, col: usize) -> Option<Vec<Option<bool>>> {
    match &table.cols[col] {
        TypedCol::Bool { vals } => {
            let (mut on, mut off) = (false, false);
            for v in vals.iter().flatten() {
                if *v {
                    on = true;
                } else {
                    off = true;
                }
            }
            (on && off).then(|| vals.clone())
        }
        TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. } => {
            let (mut on, mut off) = (false, false);
            let mut out = Vec::with_capacity(table.n);
            for r in 0..table.n {
                match table.cols[col].numeric_at(r) {
                    Some(0.0) => {
                        off = true;
                        out.push(Some(false));
                    }
                    Some(1.0) => {
                        on = true;
                        out.push(Some(true));
                    }
                    Some(_) => return None,
                    None => out.push(None),
                }
            }
            (on && off).then_some(out)
        }
        _ => None,
    }
}

/// The extracted table. Borrows string data from the source DataFrame.
#[derive(Debug)]
pub struct TypedTable<'a> {
    pub n: usize,
    pub names: Vec<String>,
    /// Friendly dtype names for reporting: int64, float64, string, ...
    pub dtypes: Vec<String>,
    pub cols: Vec<TypedCol<'a>>,
}

/// Friendly, stable dtype names for the report.
pub fn dtype_name(dtype: &DataType) -> String {
    match dtype {
        DataType::Boolean => "boolean",
        DataType::Int8 => "int8",
        DataType::Int16 => "int16",
        DataType::Int32 => "int32",
        DataType::Int64 => "int64",
        DataType::UInt8 => "uint8",
        DataType::UInt16 => "uint16",
        DataType::UInt32 => "uint32",
        DataType::UInt64 => "uint64",
        DataType::Float32 => "float32",
        DataType::Float64 => "float64",
        DataType::Decimal(_, _) => "decimal",
        DataType::Duration(_) => "duration",
        DataType::Time => "time",
        DataType::String => "string",
        DataType::Categorical(_, _) => "categorical",
        DataType::Enum(_, _) => "enum",
        DataType::Date => "date",
        DataType::Datetime(_, _) => "datetime",
        DataType::Null => "null",
        _ => "other",
    }
    .to_string()
}

fn norm_f64(v: f64) -> Option<f64> {
    // NaN/inf -> null; -0.0 -> +0.0 (so hashing/comparisons behave).
    if v.is_nan() || v.is_infinite() {
        None
    } else if v == 0.0 {
        Some(0.0)
    } else {
        Some(v)
    }
}

/// Extract all columns of a DataFrame into `TypedTable`.
pub fn extract(df: &DataFrame) -> TypedTable<'_> {
    let n = df.height();
    let mut names = Vec::with_capacity(df.width());
    let mut dtypes = Vec::with_capacity(df.width());
    let mut cols = Vec::with_capacity(df.width());

    for col in df.columns() {
        let s = col.as_materialized_series();
        let name = s.name().to_string();
        let dtype = dtype_name(s.dtype());
        let tc = extract_series(s);
        names.push(name);
        dtypes.push(dtype);
        cols.push(tc);
    }

    TypedTable {
        n,
        names,
        dtypes,
        cols,
    }
}

/// Extract a single Series into a TypedCol. Categorical/Enum columns are
/// materialized to strings (they are low-cardinality by construction).
pub fn extract_series(s: &Series) -> TypedCol<'_> {
    if let Ok(ca) = s.u64() {
        if let Ok(values) = ca.cont_slice() {
            return TypedCol::U64 {
                vals: NumericValues::Borrowed(values),
            };
        }
        let mut end = 0;
        let chunks = ca
            .downcast_iter()
            .map(|array| {
                end += array.len();
                NumericChunk {
                    end,
                    values: array.values().as_slice(),
                    validity: array.validity(),
                }
            })
            .collect();
        return TypedCol::U64 {
            vals: NumericValues::Chunked(chunks),
        };
    }
    if let Ok(ca) = s.i64() {
        if let Ok(values) = ca.cont_slice() {
            return TypedCol::I64 {
                vals: NumericValues::Borrowed(values),
            };
        }
    }
    if let Ok(ca) = s.f64() {
        if let Ok(values) = ca.cont_slice() {
            if values
                .iter()
                .all(|v| v.is_finite() && (*v != 0.0 || !v.is_sign_negative()))
            {
                return TypedCol::F64 {
                    vals: NumericValues::Borrowed(values),
                };
            }
        }
    }
    if let Ok(ca) = s.i64() {
        let mut end = 0;
        let chunks = ca
            .downcast_iter()
            .map(|array| {
                end += array.len();
                NumericChunk {
                    end,
                    values: array.values().as_slice(),
                    validity: array.validity(),
                }
            })
            .collect();
        return TypedCol::I64 {
            vals: NumericValues::Chunked(chunks),
        };
    }
    if let Ok(ca) = s.f64() {
        let mut end = 0;
        let chunks = ca
            .downcast_iter()
            .map(|array| {
                end += array.len();
                NumericChunk {
                    end,
                    values: array.values().as_slice(),
                    validity: array.validity(),
                }
            })
            .collect();
        return TypedCol::F64 {
            vals: NumericValues::Chunked(chunks),
        };
    }
    match s.dtype() {
        DataType::Boolean => {
            let mut vals = Vec::with_capacity(s.len());
            for av in s.iter() {
                vals.push(match av {
                    AnyValue::Boolean(b) => Some(b),
                    AnyValue::Null => None,
                    _ => None,
                });
            }
            TypedCol::Bool { vals }
        }
        DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32 => {
            let mut vals = Vec::with_capacity(s.len());
            for av in s.iter() {
                vals.push(match av {
                    AnyValue::Int8(v) => Some(v as i64),
                    AnyValue::Int16(v) => Some(v as i64),
                    AnyValue::Int32(v) => Some(v as i64),
                    AnyValue::Int64(v) => Some(v),
                    AnyValue::UInt8(v) => Some(v as i64),
                    AnyValue::UInt16(v) => Some(v as i64),
                    AnyValue::UInt32(v) => Some(v as i64),
                    AnyValue::Null => None,
                    _ => None,
                });
            }
            TypedCol::I64 { vals: vals.into() }
        }
        DataType::UInt64 => TypedCol::U64 {
            vals: s
                .iter()
                .map(|v| match v {
                    AnyValue::UInt64(v) => Some(v),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .into(),
        },
        DataType::Float32 | DataType::Float64 => {
            let mut vals = Vec::with_capacity(s.len());
            for av in s.iter() {
                vals.push(match av {
                    AnyValue::Float32(v) => norm_f64(v as f64),
                    AnyValue::Float64(v) => norm_f64(v),
                    AnyValue::Null => None,
                    _ => None,
                });
            }
            TypedCol::F64 { vals: vals.into() }
        }
        DataType::Decimal(_, scale) => {
            // A decimal value is a scaled integer: the stored i128
            // divided by 10^scale. Convert to f64 at extraction — the
            // same projection polars itself applies in a decimal-to-
            // float cast — so every downstream pass analyses the
            // column as a number. Digits beyond f64 precision are not
            // retained, as with the u64 -> f64 projection.
            let divisor = 10f64.powi(*scale as i32);
            let mut vals = Vec::with_capacity(s.len());
            for av in s.iter() {
                vals.push(match av {
                    AnyValue::Decimal(v, _, _) => norm_f64(v as f64 / divisor),
                    AnyValue::Null => None,
                    _ => None,
                });
            }
            TypedCol::F64 { vals: vals.into() }
        }
        DataType::Duration(unit) => {
            // A duration is a count of ticks at the column's time
            // unit. Convert to seconds at extraction — the span
            // itself, with the tick encoding divided out, the same
            // projection the Decimal arm applies to its scaled
            // integer — so every downstream pass analyses the column
            // as a number, and every value it presents is in seconds.
            // Spans beyond f64 precision are not retained exactly,
            // as with the u64 -> f64 projection.
            let per_second = crate::dateutil::ticks_per_second(*unit) as f64;
            let mut vals = Vec::with_capacity(s.len());
            for av in s.iter() {
                vals.push(match av {
                    AnyValue::Duration(v, _) => norm_f64(v as f64 / per_second),
                    AnyValue::Null => None,
                    _ => None,
                });
            }
            TypedCol::F64 { vals: vals.into() }
        }
        DataType::String => {
            let mut vals = Vec::with_capacity(s.len());
            for av in s.iter() {
                vals.push(match av {
                    AnyValue::String(v) => Some(v),
                    AnyValue::Null => None,
                    _ => None,
                });
            }
            TypedCol::Str { vals }
        }
        DataType::Categorical(_, _) | DataType::Enum(_, _) => {
            // Materialize category strings (owned copies). Categoricals are
            // low-cardinality by construction, so this is cheap.
            let casted = s.cast(&DataType::String).unwrap_or_else(|_| {
                Series::full_null(s.name().clone(), s.len(), &DataType::String)
            });
            let mut vals = Vec::with_capacity(casted.len());
            for av in casted.iter() {
                vals.push(match av {
                    AnyValue::String(v) => Some(v.to_string()),
                    AnyValue::Null => None,
                    _ => None,
                });
            }
            TypedCol::StrOwned { vals }
        }
        DataType::Date => {
            let mut vals = Vec::with_capacity(s.len());
            for av in s.iter() {
                vals.push(match av {
                    AnyValue::Date(d) => Some(d),
                    AnyValue::Null => None,
                    _ => None,
                });
            }
            TypedCol::Date { vals }
        }
        DataType::Datetime(unit, _) => {
            let vals = s
                .iter()
                .map(|v| match v {
                    AnyValue::Datetime(v, _, _) => Some(v),
                    _ => None,
                })
                .collect();
            TypedCol::Datetime { vals, unit: *unit }
        }
        DataType::Null => TypedCol::Null { n: s.len() },
        _ => TypedCol::Other {
            n: s.len(),
            null_count: s.null_count(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars::df;

    #[test]
    fn extracts_basic_types() {
        let df = df! {
            "i" => [1i32, 2, 3],
            "f" => [1.5f64, f64::NAN, 3.0],
            "s" => [Some("a"), Some("b"), None],
            "b" => [true, false, true],
        }
        .unwrap();
        let t = extract(&df);
        assert_eq!(t.n, 3);
        assert_eq!(t.names, vec!["i", "f", "s", "b"]);
        assert_eq!(t.dtypes, vec!["int32", "float64", "string", "boolean"]);
        match &t.cols[0] {
            TypedCol::I64 { vals } => assert_eq!(vals, &vec![Some(1i64), Some(2), Some(3)]),
            TypedCol::U64 { vals } => assert_eq!(vals, &vec![Some(1u64), Some(2), Some(3)]),
            _ => panic!("i should be I64"),
        }
        match &t.cols[1] {
            TypedCol::F64 { vals } => {
                assert_eq!(vals, &vec![Some(1.5f64), None, Some(3.0)]);
            }
            _ => panic!("f should be F64"),
        }
        match &t.cols[2] {
            TypedCol::Str { vals } => assert_eq!(vals[2], None as Option<&str>),
            _ => panic!("s should be Str"),
        }
        match &t.cols[3] {
            TypedCol::Bool { vals } => assert_eq!(vals, &vec![Some(true), Some(false), Some(true)]),
            _ => panic!("b should be Bool"),
        }
    }

    #[test]
    fn extracts_dates_and_categoricals() {
        let s = Series::new("d".into(), [100i32, 101, 102]);
        let date_s = s.cast(&DataType::Date).unwrap();
        let t = extract_series(&date_s);
        match t {
            TypedCol::Date { vals } => assert_eq!(vals, vec![Some(100), Some(101), Some(102)]),
            _ => panic!("expected Date"),
        }
    }

    #[test]
    fn extracts_decimal_as_float() {
        // Decimal(10, 2): the stored integers are the values x100.
        let s = Series::new(
            "d".into(),
            &[
                Some("1.50"),
                Some("2.50"),
                None,
                Some("-0.25"),
                Some("100.00"),
            ],
        )
        .cast(&DataType::Decimal(10, 2))
        .unwrap();
        assert_eq!(dtype_name(s.dtype()), "decimal");
        let t = extract_series(&s);
        match &t {
            TypedCol::F64 { vals } => {
                assert_eq!(
                    vals,
                    &vec![Some(1.5f64), Some(2.5), None, Some(-0.25), Some(100.0)]
                );
            }
            _ => panic!("decimal should extract as F64"),
        }
        assert!(t.is_numeric());
        assert_eq!(t.null_count(), 1);
        assert_eq!(t.numeric_at(3), Some(-0.25));
    }

    #[test]
    fn extracts_decimal_with_zero_and_large_scales() {
        let whole = Series::new("w".into(), &["7", "42"])
            .cast(&DataType::Decimal(10, 0))
            .unwrap();
        match extract_series(&whole) {
            TypedCol::F64 { vals } => assert_eq!(vals, vec![Some(7.0f64), Some(42.0)]),
            _ => panic!("scale-0 decimal should extract as F64"),
        }
        let fine = Series::new("f".into(), &["0.000001", "1.000001"])
            .cast(&DataType::Decimal(12, 6))
            .unwrap();
        match extract_series(&fine) {
            TypedCol::F64 { vals } => {
                assert_eq!(vals, vec![Some(0.000001f64), Some(1.000001)]);
            }
            _ => panic!("scale-6 decimal should extract as F64"),
        }
    }

    #[test]
    fn extracts_duration_as_float_seconds() {
        // Duration('ms'): the stored integers are millisecond ticks.
        // Extraction presents the span in seconds.
        let s = Series::new(
            "d".into(),
            &[Some(1500i64), Some(2500), None, Some(-500), Some(0)],
        )
        .cast(&DataType::Duration(TimeUnit::Milliseconds))
        .unwrap();
        assert_eq!(dtype_name(s.dtype()), "duration");
        let t = extract_series(&s);
        match &t {
            TypedCol::F64 { vals } => {
                assert_eq!(
                    vals,
                    &vec![Some(1.5f64), Some(2.5), None, Some(-0.5), Some(0.0)]
                );
            }
            _ => panic!("duration should extract as F64"),
        }
        assert!(t.is_numeric());
        assert_eq!(t.null_count(), 1);
        assert_eq!(t.numeric_at(3), Some(-0.5));
    }

    #[test]
    fn extracts_duration_in_each_time_unit() {
        // The same span (2 seconds) stored at each time unit must
        // extract to the same measure.
        for (unit, ticks) in [
            (TimeUnit::Nanoseconds, 2_000_000_000i64),
            (TimeUnit::Microseconds, 2_000_000),
            (TimeUnit::Milliseconds, 2_000),
        ] {
            let s = Series::new("d".into(), &[ticks])
                .cast(&DataType::Duration(unit))
                .unwrap();
            match extract_series(&s) {
                TypedCol::F64 { vals } => assert_eq!(vals, vec![Some(2.0f64)]),
                _ => panic!("duration ({unit}) should extract as F64"),
            }
        }
    }

    #[test]
    fn extracts_time_as_opaque() {
        // Time-of-day stays opaque by decision (a circular quantity);
        // it is counted, and its dtype is named, but no pass analyses
        // it as a number.
        let s = Series::new(
            "t".into(),
            &[Some(3_600_000_000_000i64), None, Some(86_399_000_000_000)],
        )
        .cast(&DataType::Time)
        .unwrap();
        assert_eq!(dtype_name(s.dtype()), "time");
        let t = extract_series(&s);
        match &t {
            TypedCol::Other { n, null_count } => {
                assert_eq!(*n, 3);
                assert_eq!(*null_count, 1);
            }
            _ => panic!("time should extract as Other"),
        }
        assert!(!t.is_numeric());
        assert_eq!(t.len(), 3);
        assert_eq!(t.null_count(), 1);
    }
}
