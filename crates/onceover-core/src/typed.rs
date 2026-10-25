//! Zero-copy typed extraction from a polars `DataFrame` into onceover's
//! internal column representation. All heavier passes operate on these
//! compact columns; polars API surface is confined to this module.

use polars::prelude::*;

/// A column extracted into onceover's normalized representation.
/// `Str` borrows directly from the source DataFrame (zero-copy).
#[derive(Debug)]
pub enum TypedCol<'a> {
    /// All integer dtypes (int8..64, uint8..32; uint64 when values fit i64).
    I64 {
        vals: Vec<Option<i64>>,
    },
    /// Floats (NaN/inf treated as null; -0.0 normalized to +0.0).
    F64 {
        vals: Vec<Option<f64>>,
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
    /// Datetime normalized to microseconds since epoch (naive).
    Datetime {
        vals: Vec<Option<i64>>,
    },
    /// A column that is entirely null.
    Null {
        n: usize,
    },
    /// Unsupported dtype (list/struct/binary/...) — opaque to the profiler.
    Other {
        n: usize,
        null_count: usize,
    },
}

impl<'a> TypedCol<'a> {
    pub fn len(&self) -> usize {
        match self {
            TypedCol::I64 { vals } => vals.len(),
            TypedCol::F64 { vals } => vals.len(),
            TypedCol::Str { vals } => vals.len(),
            TypedCol::StrOwned { vals } => vals.len(),
            TypedCol::Bool { vals } => vals.len(),
            TypedCol::Date { vals } => vals.len(),
            TypedCol::Datetime { vals } => vals.len(),
            TypedCol::Null { n } | TypedCol::Other { n, .. } => *n,
        }
    }

    pub fn null_count(&self) -> usize {
        match self {
            TypedCol::I64 { vals } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::F64 { vals } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::Str { vals } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::StrOwned { vals } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::Bool { vals } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::Date { vals } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::Datetime { vals } => vals.iter().filter(|v| v.is_none()).count(),
            TypedCol::Null { n } => *n,
            TypedCol::Other { null_count, .. } => *null_count,
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

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// True when this column can act as a numeric feature (for screening).
    pub fn is_numeric(&self) -> bool {
        matches!(self, TypedCol::I64 { .. } | TypedCol::F64 { .. })
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
            TypedCol::I64 { vals }
        }
        DataType::UInt64 => {
            // Collect as u64 first; fall back to f64 if any value overflows i64.
            let mut raw: Vec<Option<u64>> = Vec::with_capacity(s.len());
            let mut overflow = false;
            for av in s.iter() {
                match av {
                    AnyValue::UInt64(v) => {
                        if v > i64::MAX as u64 {
                            overflow = true;
                        }
                        raw.push(Some(v));
                    }
                    AnyValue::Null => raw.push(None),
                    _ => raw.push(None),
                }
            }
            if overflow {
                let vals = raw
                    .into_iter()
                    .map(|o| o.and_then(|v| norm_f64(v as f64)))
                    .collect();
                TypedCol::F64 { vals }
            } else {
                let vals = raw.into_iter().map(|o| o.map(|v| v as i64)).collect();
                TypedCol::I64 { vals }
            }
        }
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
            TypedCol::F64 { vals }
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
        DataType::Datetime(_, _) => {
            let mut vals = Vec::with_capacity(s.len());
            for av in s.iter() {
                vals.push(match av {
                    AnyValue::Datetime(v, tu, _) => Some(match tu {
                        TimeUnit::Milliseconds => v * 1_000,
                        TimeUnit::Microseconds => v,
                        TimeUnit::Nanoseconds => v / 1_000,
                    }),
                    AnyValue::Null => None,
                    _ => None,
                });
            }
            TypedCol::Datetime { vals }
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
}
