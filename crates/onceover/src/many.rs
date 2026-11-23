//! Multi-table profiling: candidate foreign keys and partial value overlaps
//! across a set of Polars DataFrames (`profile_many` / `MultiTableReport`).

use onceover_core::dateutil;
use polars::prelude::{DataFrame, DataType, TimeUnit};
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

const PROBE_LIMIT: usize = 4096;
const ORPHAN_EXAMPLES: usize = 10;
const REPORT_VERSION: &str = "1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Family {
    Integer,
    Float,
    String,
    Temporal,
}

impl Family {
    fn as_str(self) -> &'static str {
        match self {
            Family::Integer => "integer",
            Family::Float => "float",
            Family::String => "string",
            Family::Temporal => "temporal",
        }
    }
}

fn family_of(dt: &DataType) -> Option<Family> {
    match dt {
        DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64 => Some(Family::Integer),
        DataType::Float32 | DataType::Float64 => Some(Family::Float),
        DataType::String | DataType::Categorical(_, _) | DataType::Enum(_, _) => {
            Some(Family::String)
        }
        DataType::Date | DataType::Datetime(_, _) => Some(Family::Temporal),
        _ => None,
    }
}

fn skip_reason(dt: &DataType) -> String {
    match dt {
        DataType::Boolean => {
            "boolean columns are not examined: a two-value domain cannot carry reference evidence"
                .to_string()
        }
        DataType::Time => "time-of-day columns are not examined".to_string(),
        other => format!("columns of type {other} are not examined"),
    }
}

/// Natural log of C(n, k) accurate to ~1e-14 even for huge spans (`2^64`).
/// Direct summation for k <= 128 avoids catastrophic cancellation of
/// `ln_gamma(n+1) - ln_gamma(n-k+1)`; Stirling's expansion around the
/// binomial entropy handles k > 128.
pub fn log_choose(n: u128, k: u128) -> f64 {
    if k > n {
        return f64::NEG_INFINITY;
    }
    let k = k.min(n - k);
    if k == 0 {
        return 0.0;
    }
    if k <= 128 {
        let mut sum = 0.0f64;
        let mut c = 0.0f64;
        for i in 0..k {
            let term = ((n - i) as f64).ln() - ((i + 1) as f64).ln();
            let y = term - c;
            let t = sum + y;
            c = (t - sum) - y;
            sum = t;
        }
        return sum;
    }
    let nf = n as f64;
    let kf = k as f64;
    let rest = (n - k) as f64;
    let corr = |x: f64| -> f64 {
        let inv = 1.0 / x;
        inv / 12.0 - inv.powi(3) / 360.0 + inv.powi(5) / 1260.0
    };
    let terms = [
        kf * (nf / kf).ln(),
        rest * (kf / rest).ln_1p(),
        0.5 * (nf.ln() - kf.ln() - rest.ln() - (2.0 * std::f64::consts::PI).ln()),
        corr(nf),
        -corr(kf),
        -corr(rest),
    ];
    terms.iter().sum()
}

fn chance_from_logs(log_num: f64, log_den: f64) -> f64 {
    if log_den == f64::NEG_INFINITY {
        return 1.0;
    }
    (log_num - log_den).exp().min(1.0)
}

fn names_agree(child: &str, parent: &str) -> bool {
    if child.is_empty() {
        return false;
    }
    let norm = |s: &str| -> String {
        s.chars()
            .filter(|&c| c != '_' && c != ' ')
            .flat_map(char::to_lowercase)
            .collect()
    };
    norm(child) == norm(parent)
}

enum KeyStorage {
    Int {
        /// Sorted distinct non-null values as i128 (covers full i64 and u64).
        distinct: Vec<i128>,
        set: FxHashSet<i128>,
        /// Value counts for exact orphan-row counting on overlaps.
        counts: FxHashMap<i128, usize>,
        span_min: Option<i128>,
        span_max: Option<i128>,
    },
    Float {
        /// Sorted distinct non-null finite/inf values (normalized -0.0 -> 0.0).
        distinct: Vec<f64>,
        set: FxHashSet<u64>,
        counts: FxHashMap<u64, usize>,
    },
    Str {
        distinct: Vec<String>,
        set: FxHashSet<String>,
        counts: FxHashMap<String, usize>,
    },
    Temporal {
        /// Microseconds since 1970-01-01T00:00:00Z.
        distinct: Vec<i64>,
        set: FxHashSet<i64>,
        counts: FxHashMap<i64, usize>,
    },
}

struct ExaminedCol {
    table: String,
    name: String,
    family: Family,
    rows: usize,
    nonnull_rows: usize,
    distinct_count: usize,
    unique: bool,
    text_keys: bool,
    keys: KeyStorage,
}

fn extract_column(table: &str, df: &DataFrame, name: &str, int_text_keys: bool) -> ExaminedCol {
    let col = df.column(name).expect("column exists");
    let series = col.as_materialized_series();
    let dt = series.dtype();
    let family = family_of(dt).expect("supported family");
    let rows = df.height();
    let mut nonnull_rows = 0usize;

    let keys = match family {
        Family::Integer => {
            let mut counts: FxHashMap<i128, usize> = FxHashMap::default();
            macro_rules! scan_int {
                ($ca:expr) => {
                    for chunk in $ca.downcast_iter() {
                        for v in chunk.iter().flatten() {
                            nonnull_rows += 1;
                            *counts.entry(*v as i128).or_insert(0) += 1;
                        }
                    }
                };
            }
            match dt {
                DataType::Int8 => scan_int!(series.i8().unwrap()),
                DataType::Int16 => scan_int!(series.i16().unwrap()),
                DataType::Int32 => scan_int!(series.i32().unwrap()),
                DataType::Int64 => scan_int!(series.i64().unwrap()),
                DataType::UInt8 => scan_int!(series.u8().unwrap()),
                DataType::UInt16 => scan_int!(series.u16().unwrap()),
                DataType::UInt32 => scan_int!(series.u32().unwrap()),
                DataType::UInt64 => scan_int!(series.u64().unwrap()),
                _ => unreachable!(),
            }
            let mut distinct: Vec<i128> = counts.keys().copied().collect();
            distinct.sort_unstable();
            let span_min = distinct.first().copied();
            let span_max = distinct.last().copied();
            let set: FxHashSet<i128> = distinct.iter().copied().collect();
            KeyStorage::Int {
                distinct,
                set,
                counts,
                span_min,
                span_max,
            }
        }
        Family::Float => {
            let mut counts: FxHashMap<u64, usize> = FxHashMap::default();
            match dt {
                DataType::Float32 => {
                    for chunk in series.f32().unwrap().downcast_iter() {
                        for &v in chunk.iter().flatten() {
                            if !v.is_nan() {
                                let vf = if v == 0.0 { 0.0f64 } else { v as f64 };
                                nonnull_rows += 1;
                                *counts.entry(vf.to_bits()).or_insert(0) += 1;
                            }
                        }
                    }
                }
                DataType::Float64 => {
                    for chunk in series.f64().unwrap().downcast_iter() {
                        for &v in chunk.iter().flatten() {
                            if !v.is_nan() {
                                let vf = if v == 0.0 { 0.0f64 } else { v };
                                nonnull_rows += 1;
                                *counts.entry(vf.to_bits()).or_insert(0) += 1;
                            }
                        }
                    }
                }
                _ => unreachable!(),
            }
            let mut distinct: Vec<f64> = counts.keys().map(|&b| f64::from_bits(b)).collect();
            distinct.sort_by(f64::total_cmp);
            let set: FxHashSet<u64> = counts.keys().copied().collect();
            KeyStorage::Float {
                distinct,
                set,
                counts,
            }
        }
        Family::String => {
            let mut counts: FxHashMap<String, usize> = FxHashMap::default();
            match dt {
                DataType::String => {
                    for chunk in series.str().unwrap().downcast_iter() {
                        for s in chunk.iter().flatten() {
                            nonnull_rows += 1;
                            if let Some(c) = counts.get_mut(s) {
                                *c += 1;
                            } else {
                                counts.insert(s.to_string(), 1);
                            }
                        }
                    }
                }
                _ => {
                    let cast = series.cast(&DataType::String).unwrap();
                    for chunk in cast.str().unwrap().downcast_iter() {
                        for s in chunk.iter().flatten() {
                            nonnull_rows += 1;
                            if let Some(c) = counts.get_mut(s) {
                                *c += 1;
                            } else {
                                counts.insert(s.to_string(), 1);
                            }
                        }
                    }
                }
            }
            let mut distinct: Vec<String> = counts.keys().cloned().collect();
            distinct.sort();
            let set: FxHashSet<String> = counts.keys().cloned().collect();
            KeyStorage::Str {
                distinct,
                set,
                counts,
            }
        }
        Family::Temporal => {
            let mut counts: FxHashMap<i64, usize> = FxHashMap::default();
            match dt {
                DataType::Date => {
                    for chunk in series.date().unwrap().physical().downcast_iter() {
                        for &days in chunk.iter().flatten() {
                            let us = (days as i64) * 86_400_000_000;
                            nonnull_rows += 1;
                            *counts.entry(us).or_insert(0) += 1;
                        }
                    }
                }
                DataType::Datetime(unit, _) => {
                    for chunk in series.datetime().unwrap().physical().downcast_iter() {
                        for &raw in chunk.iter().flatten() {
                            let us = match unit {
                                TimeUnit::Nanoseconds => raw.div_euclid(1_000),
                                TimeUnit::Microseconds => raw,
                                TimeUnit::Milliseconds => raw * 1_000,
                            };
                            nonnull_rows += 1;
                            *counts.entry(us).or_insert(0) += 1;
                        }
                    }
                }
                _ => unreachable!(),
            }
            let mut distinct: Vec<i64> = counts.keys().copied().collect();
            distinct.sort_unstable();
            let set: FxHashSet<i64> = distinct.iter().copied().collect();
            KeyStorage::Temporal {
                distinct,
                set,
                counts,
            }
        }
    };

    let distinct_count = match &keys {
        KeyStorage::Int { distinct, .. } => distinct.len(),
        KeyStorage::Float { distinct, .. } => distinct.len(),
        KeyStorage::Str { distinct, .. } => distinct.len(),
        KeyStorage::Temporal { distinct, .. } => distinct.len(),
    };
    ExaminedCol {
        table: table.to_string(),
        name: name.to_string(),
        family,
        rows,
        nonnull_rows,
        distinct_count,
        unique: distinct_count == nonnull_rows && distinct_count > 0,
        text_keys: int_text_keys && family == Family::Integer,
        keys,
    }
}

/// Indices of a strided probe over a sorted distinct array of length `k`,
/// matching `_probe` in `_many.py`. Returns `(indices, is_exact)`.
fn probe_indices(k: usize) -> (Vec<usize>, bool) {
    if k <= PROBE_LIMIT {
        return ((0..k).collect(), true);
    }
    let step = k.div_ceil(PROBE_LIMIT);
    let mut idx: Vec<usize> = (0..k).step_by(step).collect();
    if idx.last().copied() != Some(k - 1) {
        idx.push(k - 1);
    }
    (idx, false)
}

fn fmt_iso_us(us: i64) -> String {
    let days = us.div_euclid(86_400_000_000);
    let rem = us.rem_euclid(86_400_000_000);
    let secs = rem / 1_000_000;
    let frac = rem % 1_000_000;
    let (y, m, d) = dateutil::civil_from_days(days);
    let hh = secs / 3600;
    let mm = (secs % 3600) / 60;
    let ss = secs % 60;
    if frac == 0 {
        format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}")
    } else {
        format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{frac:06}")
    }
}

fn fmt_g3(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    if v.abs() < 1e-4 || v.abs() >= 1e3 {
        let raw = format!("{v:.2e}");
        if let Some((m, e)) = raw.split_once('e') {
            let m_clean = m.trim_end_matches('0').trim_end_matches('.');
            let (sign, digits) = if let Some(rest) = e.strip_prefix('-') {
                ('-', rest)
            } else if let Some(rest) = e.strip_prefix('+') {
                ('+', rest)
            } else {
                ('+', e)
            };
            let padded = if digits.len() < 2 {
                format!("0{digits}")
            } else {
                digits.to_string()
            };
            return format!("{m_clean}e{sign}{padded}");
        }
    }
    let places = (2 - v.abs().log10().floor() as i32).max(0) as usize;
    let raw = format!("{v:.places$}");
    if raw.contains('.') {
        raw.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        raw
    }
}

/// Multi-table profiling report (candidate foreign keys and partial overlaps).
#[derive(Clone, Debug)]
pub struct MultiTableReport {
    pub data: Value,
}

impl MultiTableReport {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.data).expect("multi-table report serialization")
    }

    pub fn to_json_budgeted(&self, max_chars: usize) -> Result<String, String> {
        let text = self.to_json();
        if text.chars().count() <= max_chars {
            return Ok(text);
        }
        let full_chars = text.chars().count();
        let mut body = self.data.clone();
        let candidates = self
            .data
            .get("candidates")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let overlaps = self
            .data
            .get("overlaps")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let kept_c: Vec<Value> = candidates.iter().take(10).cloned().collect();
        let kept_o: Vec<Value> = overlaps.iter().take(10).cloned().collect();
        if let Some(obj) = body.as_object_mut() {
            obj.insert(
                "truncation".into(),
                json!({
                    "full_json_chars": full_chars,
                    "returned_candidate_records": kept_c.len(),
                    "omitted_candidate_records": candidates.len() - kept_c.len(),
                    "returned_overlap_records": kept_o.len(),
                    "omitted_overlap_records": overlaps.len() - kept_o.len(),
                }),
            );
            obj.insert("candidates".into(), Value::Array(kept_c));
            obj.insert("overlaps".into(), Value::Array(kept_o));
        }
        let reduced = serde_json::to_string_pretty(&body).expect("multi-table serialization");
        if reduced.chars().count() > max_chars {
            return Err(format!(
                "max_chars={max_chars} is below the minimal budgeted multi-table report skeleton ({} chars)",
                reduced.chars().count()
            ));
        }
        Ok(reduced)
    }

    pub fn to_digest(&self, budget_chars: usize) -> String {
        let tables = self
            .data
            .get("tables")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        let candidates = self
            .data
            .get("candidates")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let overlaps = self
            .data
            .get("overlaps")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let strong: Vec<&Value> = candidates
            .iter()
            .filter(|c| c.get("tier").and_then(Value::as_str) == Some("strong"))
            .collect();
        let weak: Vec<&Value> = candidates
            .iter()
            .filter(|c| c.get("tier").and_then(Value::as_str) == Some("weak"))
            .collect();
        let noun = if candidates.len() == 1 { "key" } else { "keys" };
        let mut lines = vec![format!(
            "Multi-table report: {tables} tables, {} candidate foreign {noun} ({} strong, {} lower-tier), {} partial overlaps.",
            candidates.len(),
            strong.len(),
            weak.len(),
            overlaps.len()
        )];
        for rec in strong.iter().take(10) {
            lines.push(format!(
                "- strong: {}.{} -> {}.{} (names agree; {} distinct values)",
                rec["child_table"].as_str().unwrap_or(""),
                rec["child_column"].as_str().unwrap_or(""),
                rec["parent_table"].as_str().unwrap_or(""),
                rec["parent_column"].as_str().unwrap_or(""),
                rec["child_distinct"].as_u64().unwrap_or(0),
            ));
        }
        for rec in weak.iter().take(10) {
            lines.push(format!(
                "- lower-tier: {}.{} -> {}.{} (chance {}; {} distinct values)",
                rec["child_table"].as_str().unwrap_or(""),
                rec["child_column"].as_str().unwrap_or(""),
                rec["parent_table"].as_str().unwrap_or(""),
                rec["parent_column"].as_str().unwrap_or(""),
                fmt_g3(rec["chance_score"].as_f64().unwrap_or(0.0)),
                rec["child_distinct"].as_u64().unwrap_or(0),
            ));
        }
        for rec in overlaps.iter().take(10) {
            lines.push(format!(
                "- overlap: {}.{} -> {}.{}: {:.1}% of {} distinct values match; {} orphan rows",
                rec["child_table"].as_str().unwrap_or(""),
                rec["child_column"].as_str().unwrap_or(""),
                rec["parent_table"].as_str().unwrap_or(""),
                rec["parent_column"].as_str().unwrap_or(""),
                rec["containment"].as_f64().unwrap_or(0.0) * 100.0,
                rec["child_distinct"].as_u64().unwrap_or(0),
                rec["orphan_rows"].as_u64().unwrap_or(0),
            ));
        }
        if let Some(stmt) = self
            .data
            .get("coverage")
            .and_then(|c| c.get("statement"))
            .and_then(Value::as_str)
        {
            lines.push(stmt.to_string());
        }
        let full = lines.join("\n");
        if full.chars().count() <= budget_chars {
            return full;
        }
        let mut kept: Vec<String> = Vec::new();
        let mut used = 0usize;
        for line in lines {
            let l = line.chars().count();
            if !kept.is_empty() && used + l + 1 > budget_chars {
                break;
            }
            kept.push(line);
            used += l + 1;
        }
        kept.join("\n")
    }
}

/// Profile a list of `(table_name, &DataFrame)` pairs for candidate foreign
/// keys and partial value overlaps.
pub fn profile_many(
    tables: &[(String, DataFrame)],
    overlap_threshold: f64,
) -> Result<MultiTableReport, String> {
    if tables.is_empty() {
        return Err("profile_many requires at least one table".to_string());
    }
    if !(overlap_threshold > 0.0 && overlap_threshold <= 1.0) || !overlap_threshold.is_finite() {
        return Err("overlap_threshold must be in (0, 1]".to_string());
    }
    for (name, df) in tables {
        let cols = df.get_column_names();
        let uniq: BTreeSet<&str> = cols.iter().map(|s| s.as_str()).collect();
        if uniq.len() != cols.len() {
            return Err(format!("table '{name}' has duplicate column names"));
        }
    }

    // Check whether any UInt64 column exceeds i64::MAX (family-wide text-key
    // / value-pool decision matching _many.py).
    let mut int_text_keys = false;
    for (_, df) in tables {
        for col in df.columns() {
            if matches!(col.dtype(), DataType::UInt64) {
                let s = col.as_materialized_series();
                for chunk in s.u64().unwrap().downcast_iter() {
                    if chunk.iter().flatten().any(|&v| v > i64::MAX as u64) {
                        int_text_keys = true;
                        break;
                    }
                }
            }
            if int_text_keys {
                break;
            }
        }
        if int_text_keys {
            break;
        }
    }

    let mut work: Vec<(usize, String)> = Vec::new();
    let mut unexamined: Vec<Value> = Vec::new();
    for (ti, (tname, df)) in tables.iter().enumerate() {
        for col in df.columns() {
            let cname = col.name().as_str();
            let dt = col.dtype();
            if family_of(dt).is_none() {
                unexamined.push(json!({
                    "table": tname,
                    "column": cname,
                    "dtype": format!("{dt}"),
                    "reason": skip_reason(dt),
                }));
            } else {
                work.push((ti, cname.to_string()));
            }
        }
    }

    let columns: Vec<ExaminedCol> = work
        .par_iter()
        .map(|(ti, cname)| extract_column(&tables[*ti].0, &tables[*ti].1, cname, int_text_keys))
        .collect();

    let mut pools: FxHashMap<Family, usize> = FxHashMap::default();
    let mut pool_for = |fam: Family| -> usize {
        if let Some(&sz) = pools.get(&fam) {
            return sz;
        }
        let sz = match fam {
            Family::Integer => {
                let mut u: FxHashSet<i128> = FxHashSet::default();
                for c in &columns {
                    if let KeyStorage::Int { distinct, .. } = &c.keys {
                        u.extend(distinct.iter().copied());
                    }
                }
                u.len()
            }
            Family::Float => {
                let mut u: FxHashSet<u64> = FxHashSet::default();
                for c in &columns {
                    if let KeyStorage::Float { set, .. } = &c.keys {
                        u.extend(set.iter().copied());
                    }
                }
                u.len()
            }
            Family::String => {
                let mut u: FxHashSet<&str> = FxHashSet::default();
                for c in &columns {
                    if let KeyStorage::Str { distinct, .. } = &c.keys {
                        u.extend(distinct.iter().map(String::as_str));
                    }
                }
                u.len()
            }
            Family::Temporal => {
                let mut u: FxHashSet<i64> = FxHashSet::default();
                for c in &columns {
                    if let KeyStorage::Temporal { distinct, .. } = &c.keys {
                        u.extend(distinct.iter().copied());
                    }
                }
                u.len()
            }
        };
        pools.insert(fam, sz);
        sz
    };

    let mut ordered_cross_table_pairs = 0usize;
    let mut same_family_pairs = 0usize;
    let mut unique_parent_pairs = 0usize;
    let mut size_pruned_pairs = 0usize;
    let mut probe_checked_pairs = 0usize;
    let mut exact_checked_pairs = 0usize;

    let mut candidates: Vec<Value> = Vec::new();
    let mut overlaps: Vec<Value> = Vec::new();

    for (ci, child) in columns.iter().enumerate() {
        if child.distinct_count == 0 {
            continue;
        }
        let mut probe_cache: Option<(Vec<usize>, bool, usize)> = None;
        for (pi, parent) in columns.iter().enumerate() {
            if ci == pi || parent.table == child.table {
                continue;
            }
            ordered_cross_table_pairs += 1;
            if parent.family != child.family {
                continue;
            }
            same_family_pairs += 1;
            if !parent.unique {
                continue;
            }
            unique_parent_pairs += 1;
            if (child.distinct_count as f64) * overlap_threshold > (parent.distinct_count as f64) {
                size_pruned_pairs += 1;
                continue;
            }
            probe_checked_pairs += 1;
            let (probe_idx, probe_exact, unprobed) = probe_cache.get_or_insert_with(|| {
                let (idx, exact) = probe_indices(child.distinct_count);
                let unp = child.distinct_count - idx.len();
                (idx, exact, unp)
            });

            let probe_shared = match (&child.keys, &parent.keys) {
                (KeyStorage::Int { distinct, .. }, KeyStorage::Int { set, .. }) => probe_idx
                    .iter()
                    .filter(|&&i| set.contains(&distinct[i]))
                    .count(),
                (KeyStorage::Float { distinct, .. }, KeyStorage::Float { set, .. }) => probe_idx
                    .iter()
                    .filter(|&&i| set.contains(&distinct[i].to_bits()))
                    .count(),
                (KeyStorage::Str { distinct, .. }, KeyStorage::Str { set, .. }) => probe_idx
                    .iter()
                    .filter(|&&i| set.contains(&distinct[i]))
                    .count(),
                (KeyStorage::Temporal { distinct, .. }, KeyStorage::Temporal { set, .. }) => {
                    probe_idx
                        .iter()
                        .filter(|&&i| set.contains(&distinct[i]))
                        .count()
                }
                _ => unreachable!(),
            };

            let shared = if *probe_exact {
                probe_shared
            } else {
                if ((probe_shared + *unprobed) as f64)
                    < overlap_threshold * (child.distinct_count as f64)
                {
                    continue;
                }
                exact_checked_pairs += 1;
                match (&child.keys, &parent.keys) {
                    (KeyStorage::Int { distinct, .. }, KeyStorage::Int { set, .. }) => {
                        distinct.iter().filter(|v| set.contains(v)).count()
                    }
                    (KeyStorage::Float { distinct, .. }, KeyStorage::Float { set, .. }) => distinct
                        .iter()
                        .filter(|v| set.contains(&v.to_bits()))
                        .count(),
                    (KeyStorage::Str { distinct, .. }, KeyStorage::Str { set, .. }) => {
                        distinct.iter().filter(|v| set.contains(v.as_str())).count()
                    }
                    (KeyStorage::Temporal { distinct, .. }, KeyStorage::Temporal { set, .. }) => {
                        distinct.iter().filter(|v| set.contains(v)).count()
                    }
                    _ => unreachable!(),
                }
            };

            if shared == child.distinct_count {
                let agree = names_agree(&child.name, &parent.name);
                let (chance_score, chance_basis) =
                    if child.family == Family::Integer && !child.text_keys && !parent.text_keys {
                        let (
                            KeyStorage::Int {
                                span_min: Some(lo),
                                span_max: Some(hi),
                                ..
                            },
                            KeyStorage::Int {
                                distinct: p_dist, ..
                            },
                        ) = (&child.keys, &parent.keys)
                        else {
                            unreachable!()
                        };
                        let span = (hi - lo) as u128 + 1;
                        let lo_idx = p_dist.partition_point(|&v| v < *lo);
                        let hi_idx = p_dist.partition_point(|&v| v <= *hi);
                        let inside = (hi_idx - lo_idx) as u128;
                        let k = child.distinct_count as u128;
                        (
                            chance_from_logs(log_choose(inside, k), log_choose(span, k)),
                            "integer_span",
                        )
                    } else {
                        let k = child.distinct_count as u128;
                        let pool = pool_for(child.family) as u128;
                        (
                            chance_from_logs(
                                log_choose(parent.distinct_count as u128, k),
                                log_choose(pool, k),
                            ),
                            "value_pool",
                        )
                    };
                candidates.push(json!({
                    "child_table": child.table,
                    "child_column": child.name,
                    "parent_table": parent.table,
                    "parent_column": parent.name,
                    "child_distinct": child.distinct_count,
                    "parent_distinct": parent.distinct_count,
                    "child_rows": child.rows,
                    "child_nonnull_rows": child.nonnull_rows,
                    "child_null_rows": child.rows - child.nonnull_rows,
                    "tier": if agree { "strong" } else { "weak" },
                    "name_agreement": agree,
                    "containment": 1.0,
                    "shared_distinct": child.distinct_count,
                    "orphan_rows": 0,
                    "chance_score": chance_score,
                    "chance_basis": chance_basis,
                }));
            } else if (shared as f64) / (child.distinct_count as f64) >= overlap_threshold {
                let mut orphan_rows = 0usize;
                let mut orphan_distinct = 0usize;
                let mut examples: Vec<Value> = Vec::new();
                match (&child.keys, &parent.keys) {
                    (
                        KeyStorage::Int {
                            distinct, counts, ..
                        },
                        KeyStorage::Int { set, .. },
                    ) => {
                        for &v in distinct {
                            if !set.contains(&v) {
                                orphan_distinct += 1;
                                orphan_rows += counts[&v];
                                if examples.len() < ORPHAN_EXAMPLES {
                                    if let Ok(iv) = i64::try_from(v) {
                                        examples.push(json!(iv));
                                    } else if let Ok(uv) = u64::try_from(v) {
                                        examples.push(json!(uv));
                                    } else {
                                        examples.push(json!(v.to_string()));
                                    }
                                }
                            }
                        }
                    }
                    (
                        KeyStorage::Float {
                            distinct, counts, ..
                        },
                        KeyStorage::Float { set, .. },
                    ) => {
                        for &v in distinct {
                            let b = v.to_bits();
                            if !set.contains(&b) {
                                orphan_distinct += 1;
                                orphan_rows += counts[&b];
                                if examples.len() < ORPHAN_EXAMPLES {
                                    examples.push(json!(v));
                                }
                            }
                        }
                    }
                    (
                        KeyStorage::Str {
                            distinct, counts, ..
                        },
                        KeyStorage::Str { set, .. },
                    ) => {
                        for v in distinct {
                            if !set.contains(v.as_str()) {
                                orphan_distinct += 1;
                                orphan_rows += counts[v.as_str()];
                                if examples.len() < ORPHAN_EXAMPLES {
                                    examples.push(json!(v));
                                }
                            }
                        }
                    }
                    (
                        KeyStorage::Temporal {
                            distinct, counts, ..
                        },
                        KeyStorage::Temporal { set, .. },
                    ) => {
                        for &v in distinct {
                            if !set.contains(&v) {
                                orphan_distinct += 1;
                                orphan_rows += counts[&v];
                                if examples.len() < ORPHAN_EXAMPLES {
                                    examples.push(json!(fmt_iso_us(v)));
                                }
                            }
                        }
                    }
                    _ => unreachable!(),
                }
                overlaps.push(json!({
                    "child_table": child.table,
                    "child_column": child.name,
                    "parent_table": parent.table,
                    "parent_column": parent.name,
                    "child_distinct": child.distinct_count,
                    "parent_distinct": parent.distinct_count,
                    "child_rows": child.rows,
                    "child_nonnull_rows": child.nonnull_rows,
                    "child_null_rows": child.rows - child.nonnull_rows,
                    "containment": (shared as f64) / (child.distinct_count as f64),
                    "shared_distinct": shared,
                    "orphan_distinct": orphan_distinct,
                    "orphan_rows": orphan_rows,
                    "orphan_examples": examples,
                }));
            }
        }
    }

    candidates.sort_by(|a, b| {
        let ta = usize::from(a["tier"].as_str() != Some("strong"));
        let tb = usize::from(b["tier"].as_str() != Some("strong"));
        let ca = a["chance_score"].as_f64().unwrap_or(1.0);
        let cb = b["chance_score"].as_f64().unwrap_or(1.0);
        let da = a["child_distinct"].as_u64().unwrap_or(0);
        let db = b["child_distinct"].as_u64().unwrap_or(0);
        ta.cmp(&tb)
            .then_with(|| ca.total_cmp(&cb))
            .then_with(|| db.cmp(&da))
            .then_with(|| a["child_table"].as_str().cmp(&b["child_table"].as_str()))
            .then_with(|| a["child_column"].as_str().cmp(&b["child_column"].as_str()))
            .then_with(|| a["parent_table"].as_str().cmp(&b["parent_table"].as_str()))
            .then_with(|| {
                a["parent_column"]
                    .as_str()
                    .cmp(&b["parent_column"].as_str())
            })
    });

    overlaps.sort_by(|a, b| {
        let ca = a["containment"].as_f64().unwrap_or(0.0);
        let cb = b["containment"].as_f64().unwrap_or(0.0);
        cb.total_cmp(&ca)
            .then_with(|| a["child_table"].as_str().cmp(&b["child_table"].as_str()))
            .then_with(|| a["child_column"].as_str().cmp(&b["child_column"].as_str()))
            .then_with(|| a["parent_table"].as_str().cmp(&b["parent_table"].as_str()))
            .then_with(|| {
                a["parent_column"]
                    .as_str()
                    .cmp(&b["parent_column"].as_str())
            })
    });

    let mut statement = format!(
        "Considered {ordered_cross_table_pairs} ordered column pairs across different tables; \
         {same_family_pairs} pairs shared a value family (integer, float, string, or temporal), \
         {unique_parent_pairs} of those had a unique parent candidate, \
         {probe_checked_pairs} passed the size prune and were probe-screened, and \
         {exact_checked_pairs} pairs were large enough to need full verification beyond the probe. \
         Same-table pairs were not examined."
    );
    if !unexamined.is_empty() {
        statement.push_str(&format!(
            " {} columns were not examined (see coverage.unexamined_columns).",
            unexamined.len()
        ));
    }

    let families: Vec<&str> = {
        let set: BTreeSet<&str> = columns.iter().map(|c| c.family.as_str()).collect();
        set.into_iter().collect()
    };
    let mut value_pools = Map::new();
    let mut sorted_pools: Vec<(&str, usize)> =
        pools.iter().map(|(f, &sz)| (f.as_str(), sz)).collect();
    sorted_pools.sort_by_key(|&(k, _)| k);
    for (k, v) in sorted_pools {
        value_pools.insert(k.to_string(), json!(v));
    }

    let tables_meta: Vec<Value> = tables
        .iter()
        .map(|(name, df)| {
            json!({
                "name": name,
                "rows": df.height(),
                "columns": df.width(),
            })
        })
        .collect();

    let data = json!({
        "kind": "onceover.profile_many",
        "version": REPORT_VERSION,
        "tables": tables_meta,
        "candidates": candidates,
        "overlaps": overlaps,
        "coverage": {
            "columns_examined": columns.len(),
            "unexamined_columns": unexamined,
            "pairs": {
                "ordered_cross_table_pairs": ordered_cross_table_pairs,
                "same_family_pairs": same_family_pairs,
                "unique_parent_pairs": unique_parent_pairs,
                "size_pruned_pairs": size_pruned_pairs,
                "probe_checked_pairs": probe_checked_pairs,
                "exact_checked_pairs": exact_checked_pairs,
            },
            "families": families,
            "value_pools": Value::Object(value_pools),
            "statement": statement,
        },
    });
    Ok(MultiTableReport { data })
}
