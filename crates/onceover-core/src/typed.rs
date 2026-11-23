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
        NumericValuesIter::new(self)
    }
}

impl<T: NumericScalar> NumericValues<'_, T> {
    /// Count nulls without scanning dense borrowed buffers.
    pub fn null_count_fast(&self, check_non_finite: bool) -> usize {
        match self {
            Self::Borrowed(_) => 0,
            Self::Owned(v) => v.iter().filter(|x| x.is_none()).count(),
            Self::Chunked(chunks) => {
                if !check_non_finite {
                    chunks
                        .iter()
                        .map(|c| c.validity.map_or(0, |b| b.unset_bits()))
                        .sum()
                } else {
                    self.iter().filter(|v| v.is_none()).count()
                }
            }
        }
    }
}

#[derive(Clone)]
struct NumericValuesIter<'a, 's, T> {
    vals: &'s NumericValues<'a, T>,
    front: usize,
    back: usize,
    front_chunk: usize,
    back_chunk: usize,
}

impl<'a, 's, T: NumericScalar> NumericValuesIter<'a, 's, T> {
    fn new(vals: &'s NumericValues<'a, T>) -> Self {
        let len = vals.len();
        let back_chunk = match vals {
            NumericValues::Chunked(chunks) => chunks.len().saturating_sub(1),
            _ => 0,
        };
        Self {
            vals,
            front: 0,
            back: len,
            front_chunk: 0,
            back_chunk,
        }
    }
}

impl<'a, 's, T: NumericScalar> Iterator for NumericValuesIter<'a, 's, T> {
    type Item = Option<&'s T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.front >= self.back {
            return None;
        }
        let i = self.front;
        self.front += 1;
        Some(match self.vals {
            NumericValues::Borrowed(v) => Some(&v[i]),
            NumericValues::Owned(v) => v[i].as_ref(),
            NumericValues::Chunked(chunks) => {
                while self.front_chunk < chunks.len() && chunks[self.front_chunk].end <= i {
                    self.front_chunk += 1;
                }
                let c = &chunks[self.front_chunk];
                let local = i - (c.end - c.values.len());
                if c.validity.is_some_and(|valid| !valid.get_bit(local)) {
                    None
                } else {
                    c.values[local].canonical_ref()
                }
            }
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let rem = self.back.saturating_sub(self.front);
        (rem, Some(rem))
    }
}

impl<'a, 's, T: NumericScalar> DoubleEndedIterator for NumericValuesIter<'a, 's, T> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front >= self.back {
            return None;
        }
        self.back -= 1;
        let i = self.back;
        Some(match self.vals {
            NumericValues::Borrowed(v) => Some(&v[i]),
            NumericValues::Owned(v) => v[i].as_ref(),
            NumericValues::Chunked(chunks) => {
                while self.back_chunk > 0
                    && i < chunks[self.back_chunk].end - chunks[self.back_chunk].values.len()
                {
                    self.back_chunk -= 1;
                }
                let c = &chunks[self.back_chunk];
                let local = i - (c.end - c.values.len());
                if c.validity.is_some_and(|valid| !valid.get_bit(local)) {
                    None
                } else {
                    c.values[local].canonical_ref()
                }
            }
        })
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

/// Render an exact scaled integer, including negative values and leading zeros.
pub fn format_decimal(value: i128, scale: usize) -> String {
    let digits = value.unsigned_abs().to_string();
    let sign = if value < 0 { "-" } else { "" };
    if scale == 0 {
        return format!("{sign}{digits}");
    }
    let digits = format!("{:0>width$}", digits, width = scale + 1);
    let split = digits.len() - scale;
    format!("{sign}{}.{}", &digits[..split], &digits[split..])
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
    /// Duration columns are converted to seconds at extraction.
    F64 {
        vals: NumericValues<'a, f64>,
    },
    /// Exact scaled integers; numeric models use an explicit f64 projection.
    Decimal {
        vals: Vec<Option<i128>>,
        scale: usize,
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
            TypedCol::Decimal { vals, .. } => vals.len(),
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
            TypedCol::I64 { vals } => vals.null_count_fast(false),
            TypedCol::U64 { vals } => vals.null_count_fast(false),
            TypedCol::F64 { vals } => vals.null_count_fast(true),
            TypedCol::Decimal { vals, .. } => vals.iter().filter(|v| v.is_none()).count(),
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
            TypedCol::Decimal { vals, .. } => vals[r].is_none(),
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
            TypedCol::Decimal { vals, scale } => {
                vals[r].map(|v| v as f64 / 10f64.powi(*scale as i32))
            }
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
            TypedCol::Decimal { vals, scale } => vals[i]
                .map(|v| format_decimal(v, *scale))
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
            TypedCol::I64 { .. }
                | TypedCol::U64 { .. }
                | TypedCol::F64 { .. }
                | TypedCol::Decimal { .. }
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
        TypedCol::Decimal { vals, scale } => {
            let one = 10i128.checked_pow(*scale as u32)?;
            let (mut on, mut off) = (false, false);
            let mut out = Vec::with_capacity(vals.len());
            for value in vals {
                out.push(match value {
                    Some(0) => {
                        off = true;
                        Some(false)
                    }
                    Some(v) if *v == one => {
                        on = true;
                        Some(true)
                    }
                    Some(_) => return None,
                    None => None,
                });
            }
            (on && off).then_some(out)
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

/// The extracted table. Borrows string and dense numeric data from the
/// source DataFrame and lazily caches its content-derived [`crate::RowKeys`]
/// so sampled passes share one computation across the run.
#[derive(Debug)]
pub struct TypedTable<'a> {
    pub n: usize,
    pub names: Vec<String>,
    /// Friendly dtype names for reporting: int64, float64, string, ...
    pub dtypes: Vec<String>,
    pub cols: Vec<TypedCol<'a>>,
    #[doc(hidden)]
    pub row_keys_cache: std::sync::OnceLock<crate::RowKeys>,
}

impl<'a> TypedTable<'a> {
    pub fn new(n: usize, names: Vec<String>, dtypes: Vec<String>, cols: Vec<TypedCol<'a>>) -> Self {
        Self {
            n,
            names,
            dtypes,
            cols,
            row_keys_cache: std::sync::OnceLock::new(),
        }
    }

    /// Content-derived row keys for this table, computed once and cached.
    pub fn row_keys(&self) -> &crate::RowKeys {
        self.row_keys_cache
            .get_or_init(|| crate::RowKeys::for_table(self))
    }
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
    use rayon::prelude::*;

    let n = df.height();
    let raw_cols = df.columns();
    let width = raw_cols.len();
    let mut names = Vec::with_capacity(width);
    let mut dtypes = Vec::with_capacity(width);
    let mut series_refs = Vec::with_capacity(width);

    for col in raw_cols {
        let s = col.as_materialized_series();
        names.push(s.name().to_string());
        dtypes.push(dtype_name(s.dtype()));
        series_refs.push(s);
    }

    let cols: Vec<TypedCol<'_>> = if width >= 4 && n >= 4_096 {
        series_refs.par_iter().map(|s| extract_series(s)).collect()
    } else {
        series_refs.iter().map(|s| extract_series(s)).collect()
    };

    TypedTable::new(n, names, dtypes, cols)
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
            if let Ok(ca) = s.bool() {
                let mut vals = Vec::with_capacity(s.len());
                for arr in ca.downcast_iter() {
                    vals.extend(arr.iter());
                }
                TypedCol::Bool { vals }
            } else {
                TypedCol::Bool {
                    vals: vec![None; s.len()],
                }
            }
        }
        DataType::Int8 => {
            let ca = s.i8().expect("int8 dtype");
            let mut vals = Vec::with_capacity(s.len());
            for arr in ca.downcast_iter() {
                vals.extend(arr.iter().map(|v| v.map(|x| *x as i64)));
            }
            TypedCol::I64 { vals: vals.into() }
        }
        DataType::Int16 => {
            let ca = s.i16().expect("int16 dtype");
            let mut vals = Vec::with_capacity(s.len());
            for arr in ca.downcast_iter() {
                vals.extend(arr.iter().map(|v| v.map(|x| *x as i64)));
            }
            TypedCol::I64 { vals: vals.into() }
        }
        DataType::Int32 => {
            let ca = s.i32().expect("int32 dtype");
            let mut vals = Vec::with_capacity(s.len());
            for arr in ca.downcast_iter() {
                vals.extend(arr.iter().map(|v| v.map(|x| *x as i64)));
            }
            TypedCol::I64 { vals: vals.into() }
        }
        DataType::UInt8 => {
            let ca = s.u8().expect("uint8 dtype");
            let mut vals = Vec::with_capacity(s.len());
            for arr in ca.downcast_iter() {
                vals.extend(arr.iter().map(|v| v.map(|x| *x as i64)));
            }
            TypedCol::I64 { vals: vals.into() }
        }
        DataType::UInt16 => {
            let ca = s.u16().expect("uint16 dtype");
            let mut vals = Vec::with_capacity(s.len());
            for arr in ca.downcast_iter() {
                vals.extend(arr.iter().map(|v| v.map(|x| *x as i64)));
            }
            TypedCol::I64 { vals: vals.into() }
        }
        DataType::UInt32 => {
            let ca = s.u32().expect("uint32 dtype");
            let mut vals = Vec::with_capacity(s.len());
            for arr in ca.downcast_iter() {
                vals.extend(arr.iter().map(|v| v.map(|x| *x as i64)));
            }
            TypedCol::I64 { vals: vals.into() }
        }
        DataType::Int64 => {
            let mut vals = Vec::with_capacity(s.len());
            for av in s.iter() {
                vals.push(match av {
                    AnyValue::Int64(v) => Some(v),
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
        DataType::Float32 => {
            let ca = s.f32().expect("float32 dtype");
            let mut vals = Vec::with_capacity(s.len());
            for arr in ca.downcast_iter() {
                vals.extend(arr.iter().map(|v| v.and_then(|x| norm_f64(*x as f64))));
            }
            TypedCol::F64 { vals: vals.into() }
        }
        DataType::Float64 => {
            let mut vals = Vec::with_capacity(s.len());
            for av in s.iter() {
                vals.push(match av {
                    AnyValue::Float64(v) => norm_f64(v),
                    _ => None,
                });
            }
            TypedCol::F64 { vals: vals.into() }
        }
        DataType::Decimal(_, scale) => {
            let vals = if let Ok(ca) = s.decimal() {
                let mut out = Vec::with_capacity(s.len());
                for arr in ca.physical().downcast_iter() {
                    out.extend(arr.iter().map(|v| v.copied()));
                }
                out
            } else {
                s.iter()
                    .map(|av| match av {
                        AnyValue::Decimal(v, _, _) => Some(v),
                        _ => None,
                    })
                    .collect()
            };
            TypedCol::Decimal {
                vals,
                scale: *scale,
            }
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
            if let Ok(ca) = s.duration() {
                for arr in ca.physical().downcast_iter() {
                    vals.extend(
                        arr.iter()
                            .map(|v| v.and_then(|x| norm_f64(*x as f64 / per_second))),
                    );
                }
            } else {
                for av in s.iter() {
                    vals.push(match av {
                        AnyValue::Duration(v, _) => norm_f64(v as f64 / per_second),
                        _ => None,
                    });
                }
            }
            TypedCol::F64 { vals: vals.into() }
        }
        DataType::String => {
            let mut vals = Vec::with_capacity(s.len());
            if let Ok(ca) = s.str() {
                for arr in ca.downcast_iter() {
                    vals.extend(arr.iter());
                }
            } else {
                for av in s.iter() {
                    vals.push(match av {
                        AnyValue::String(v) => Some(v),
                        _ => None,
                    });
                }
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
            if let Ok(ca) = casted.str() {
                for arr in ca.downcast_iter() {
                    vals.extend(arr.iter().map(|v| v.map(str::to_string)));
                }
            } else {
                for av in casted.iter() {
                    vals.push(match av {
                        AnyValue::String(v) => Some(v.to_string()),
                        _ => None,
                    });
                }
            }
            TypedCol::StrOwned { vals }
        }
        DataType::Date => {
            let mut vals = Vec::with_capacity(s.len());
            if let Ok(ca) = s.date() {
                for arr in ca.physical().downcast_iter() {
                    vals.extend(arr.iter().map(|v| v.copied()));
                }
            } else {
                for av in s.iter() {
                    vals.push(match av {
                        AnyValue::Date(d) => Some(d),
                        _ => None,
                    });
                }
            }
            TypedCol::Date { vals }
        }
        DataType::Datetime(unit, _) => {
            let vals = if let Ok(ca) = s.datetime() {
                let mut out = Vec::with_capacity(s.len());
                for arr in ca.physical().downcast_iter() {
                    out.extend(arr.iter().map(|v| v.copied()));
                }
                out
            } else {
                s.iter()
                    .map(|v| match v {
                        AnyValue::Datetime(v, _, _) => Some(v),
                        _ => None,
                    })
                    .collect()
            };
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
    fn extracts_decimal_exactly() {
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
            TypedCol::Decimal { vals, scale } => {
                assert_eq!(*scale, 2);
                assert_eq!(
                    vals,
                    &vec![Some(150), Some(250), None, Some(-25), Some(10000)]
                );
            }
            _ => panic!("decimal should retain scaled integers"),
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
            TypedCol::Decimal { vals, scale } => {
                assert_eq!(scale, 0);
                assert_eq!(vals, vec![Some(7), Some(42)]);
            }
            _ => panic!("scale-0 decimal should retain scaled integers"),
        }
        let fine = Series::new("f".into(), &["0.000001", "1.000001"])
            .cast(&DataType::Decimal(12, 6))
            .unwrap();
        match extract_series(&fine) {
            TypedCol::Decimal { vals, scale } => {
                assert_eq!(scale, 6);
                assert_eq!(vals, vec![Some(1), Some(1000001)]);
            }
            _ => panic!("scale-6 decimal should retain scaled integers"),
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
