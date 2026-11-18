//! Shared encoded matrix for the model-based passes (predictability screen,
//! decision tables, PRISM rules): every usable column quantile-binned or
//! level-coded over a deterministic row subsample.

use onceover_core::{RowKeys, TypedCol, TypedTable};
use rustc_hash::FxHashMap;

pub const MISSING: u8 = 255;
pub const MAX_BINS: usize = 32;

#[derive(Debug, Clone)]
pub struct EncodedMatrix {
    /// Subsampled original row ids (deterministic content-keyed sample).
    pub rows: Vec<u32>,
    /// Content keys aligned with `rows` and with every feature's bins.
    pub row_keys: RowKeys,
    pub n: usize,
    /// Original column index per feature.
    pub feat_cols: Vec<usize>,
    pub feat_names: Vec<String>,
    pub feat_dtypes: Vec<String>,
    /// True for level-coded (nominal) features.
    pub feat_nominal: Vec<bool>,
    /// feature-major bins: bins[f][pos]; MISSING for null.
    pub bins: Vec<Vec<u8>>,
    /// Real (non-missing) bin count per feature (<= MAX_BINS).
    pub n_bins: Vec<usize>,
    /// Numeric: bin upper bounds (len = n_bins − 1). Nominal: empty.
    pub edges: Vec<Vec<f64>>,
    /// Nominal: code -> value label. Numeric: empty.
    pub levels: Vec<Vec<String>>,
    /// For a feature derived from the time axis (hour of day, day of
    /// week, month of year), the part it is; None for a column's own
    /// feature. A calendar feature's `feat_cols` entry is the axis
    /// column it is read from.
    pub calendar_part: Vec<Option<&'static str>>,
}

pub struct EncodeConfig {
    pub row_cap: usize,
    pub feature_cap: usize,
    /// Max distinct string levels coded as a nominal feature.
    pub nominal_max_levels: usize,
    /// Table columns never encoded: the redundant side of a
    /// one-to-one recoding, whose information the kept side carries.
    pub skip_columns: Vec<usize>,
    /// The time axis column, when the table has one held as a date or
    /// datetime column: its calendar parts are encoded as features of
    /// their own, so models can name an hour of day or a day of week
    /// as a driver.
    pub calendar_axis: Option<usize>,
}

impl Default for EncodeConfig {
    fn default() -> Self {
        EncodeConfig {
            row_cap: 20_000,
            feature_cap: 48,
            nominal_max_levels: 32,
            skip_columns: Vec::new(),
            calendar_axis: None,
        }
    }
}

pub fn encode(table: &TypedTable, cfg: &EncodeConfig) -> (EncodedMatrix, String) {
    let all_keys = onceover_core::row_keys(table);
    let rows = all_keys.sample(cfg.row_cap);
    let m = rows.len();
    let mut out = EncodedMatrix {
        row_keys: all_keys.subset(&rows),
        rows,
        n: m,
        feat_cols: Vec::new(),
        feat_names: Vec::new(),
        feat_dtypes: Vec::new(),
        feat_nominal: Vec::new(),
        bins: Vec::new(),
        n_bins: Vec::new(),
        edges: Vec::new(),
        levels: Vec::new(),
        calendar_part: Vec::new(),
    };
    let mut skipped = 0usize;

    for (ci, col) in table.cols.iter().enumerate() {
        if cfg.skip_columns.contains(&ci) {
            continue;
        }
        match col {
            TypedCol::I64 { .. }
            | TypedCol::U64 { .. }
            | TypedCol::F64 { .. }
            | TypedCol::Decimal { .. } => {
                let vals: Vec<Option<f64>> = (0..m)
                    .map(|j| col.numeric_at(out.rows[j] as usize))
                    .collect();
                if let Some((bins, edges)) = bin_numeric(&vals) {
                    out.feat_cols.push(ci);
                    out.feat_names.push(table.names[ci].clone());
                    out.feat_nominal.push(false);
                    out.bins.push(bins);
                    out.n_bins.push(edges.len() + 1);
                    out.edges.push(edges);
                    out.levels.push(Vec::new());
                    out.calendar_part.push(None);
                } else {
                    skipped += 1;
                }
            }
            TypedCol::Date { .. } | TypedCol::Datetime { .. } => {
                let vals: Vec<Option<f64>> =
                    (0..m).map(|j| days_at(col, out.rows[j] as usize)).collect();
                if let Some((bins, edges)) = bin_numeric(&vals) {
                    out.feat_cols.push(ci);
                    out.feat_names.push(table.names[ci].clone());
                    out.feat_nominal.push(false);
                    out.bins.push(bins);
                    out.n_bins.push(edges.len() + 1);
                    out.edges.push(edges);
                    out.levels.push(Vec::new());
                    out.calendar_part.push(None);
                } else {
                    skipped += 1;
                }
                if cfg.calendar_axis == Some(ci) {
                    push_calendar_parts(&mut out, table, ci);
                }
            }
            TypedCol::Bool { vals } => {
                let mut bins = vec![MISSING; m];
                for (j, &r) in out.rows.iter().enumerate() {
                    match vals[r as usize] {
                        Some(true) => bins[j] = 1,
                        Some(false) => bins[j] = 0,
                        None => {}
                    }
                }
                out.feat_cols.push(ci);
                out.feat_names.push(table.names[ci].clone());
                out.feat_nominal.push(true);
                out.bins.push(bins);
                out.n_bins.push(2);
                out.edges.push(Vec::new());
                out.levels.push(vec!["false".into(), "true".into()]);
                out.calendar_part.push(None);
            }
            TypedCol::Str { .. } | TypedCol::StrOwned { .. } => {
                // Count levels over the subsample.
                let mut counts: FxHashMap<String, u32> = FxHashMap::default();
                for &r in &out.rows {
                    if let Some(v) = col.str_at(r as usize) {
                        *counts.entry(v.to_string()).or_insert(0) += 1;
                    }
                }
                if counts.is_empty() || counts.len() > cfg.nominal_max_levels {
                    skipped += 1;
                    continue;
                }
                let mut levels: Vec<(String, u32)> = counts.into_iter().collect();
                levels.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                let labels: Vec<String> = levels.iter().map(|(v, _)| v.clone()).collect();
                let code_of: FxHashMap<&str, u8> = labels
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (v.as_str(), i as u8))
                    .collect();
                let mut bins = vec![MISSING; m];
                for (j, &r) in out.rows.iter().enumerate() {
                    if let Some(v) = col.str_at(r as usize) {
                        if let Some(&c) = code_of.get(v) {
                            bins[j] = c;
                        }
                    }
                }
                out.feat_cols.push(ci);
                out.feat_names.push(table.names[ci].clone());
                out.feat_nominal.push(true);
                out.bins.push(bins);
                out.n_bins.push(labels.len());
                out.edges.push(Vec::new());
                out.levels.push(labels);
                out.calendar_part.push(None);
            }
            TypedCol::Null { .. } | TypedCol::Other { .. } => {
                skipped += 1;
            }
        }
    }

    drop_duplicate_calendar_parts(&mut out);
    out.feat_dtypes = out
        .feat_cols
        .iter()
        .map(|&c| table.dtypes[c].clone())
        .collect();
    let uncapped = out.feat_cols.len();
    let coverage = format!(
        "{} rows ({} of {}), {} features (all usable columns; per-model predictor budget {}, {} skipped/unusable)",
        out.n,
        out.n,
        table.n,
        out.feat_cols.len(),
        cfg.feature_cap,
        skipped + uncapped.saturating_sub(out.feat_cols.len()),
    );
    (out, coverage)
}

/// Ticks of a date or datetime column in microseconds.
fn micros_at(col: &TypedCol, r: usize) -> Option<i64> {
    match col {
        TypedCol::Date { vals } => vals[r].map(|d| d as i64 * 86_400_000_000),
        TypedCol::Datetime { vals, unit } => vals[r].map(|v| {
            let per_s = onceover_core::dateutil::ticks_per_second(*unit);
            (v as i128 * 1_000_000 / per_s as i128) as i64
        }),
        _ => None,
    }
}

/// Encode the axis column's calendar parts as nominal features:
/// hour of day (datetime axes with times of day), day of week, and
/// month of year, each kept only when at least two of its levels
/// occur.
fn push_calendar_parts(out: &mut EncodedMatrix, table: &TypedTable, ci: usize) {
    const DAY: i64 = 86_400_000_000;
    const WEEKDAYS: [&str; 7] = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let col = &table.cols[ci];
    let ticks: Vec<Option<i64>> = out
        .rows
        .iter()
        .map(|&r| micros_at(col, r as usize))
        .collect();
    if ticks.iter().flatten().count() < 30 {
        return;
    }
    let sub_day = ticks.iter().flatten().any(|t| t.rem_euclid(DAY) != 0);
    let mut parts: Vec<(&'static str, Vec<String>, Vec<u8>)> = Vec::new();
    if sub_day {
        let codes = ticks
            .iter()
            .map(|t| t.map_or(MISSING, |t| (t.rem_euclid(DAY) / 3_600_000_000) as u8))
            .collect();
        parts.push((
            "hour of day",
            (0..24).map(|h| h.to_string()).collect(),
            codes,
        ));
    }
    let weekday = ticks
        .iter()
        .map(|t| {
            t.map_or(MISSING, |t| {
                onceover_core::dateutil::weekday(t.div_euclid(DAY)) as u8
            })
        })
        .collect();
    parts.push((
        "day of week",
        WEEKDAYS.iter().map(|s| s.to_string()).collect(),
        weekday,
    ));
    let month = ticks
        .iter()
        .map(|t| {
            t.map_or(MISSING, |t| {
                (onceover_core::dateutil::civil_from_days(t.div_euclid(DAY)).1 - 1) as u8
            })
        })
        .collect();
    parts.push((
        "month of year",
        MONTHS.iter().map(|s| s.to_string()).collect(),
        month,
    ));
    for (part, labels, codes) in parts {
        let mut present = vec![false; labels.len()];
        for &c in &codes {
            if c != MISSING {
                present[c as usize] = true;
            }
        }
        if present.iter().filter(|&&p| p).count() < 2 {
            continue;
        }
        out.feat_cols.push(ci);
        out.feat_names
            .push(format!("{part} of {}", table.names[ci]));
        out.feat_nominal.push(true);
        out.bins.push(codes);
        out.n_bins.push(labels.len());
        out.edges.push(Vec::new());
        out.levels.push(labels);
        out.calendar_part.push(Some(match part {
            "hour of day" => "hour_of_day",
            "day of week" => "day_of_week",
            _ => "month_of_year",
        }));
    }
}

/// Drop a calendar feature whose bins partition the rows exactly as a
/// column's own feature does (an hour column beside the timestamp
/// already carries the hour of day).
fn drop_duplicate_calendar_parts(out: &mut EncodedMatrix) {
    let same = |a: &[u8], b: &[u8]| -> bool {
        let mut ab = [u8::MAX; 256];
        let mut ba = [u8::MAX; 256];
        for (&x, &y) in a.iter().zip(b) {
            if (x == MISSING) != (y == MISSING) {
                return false;
            }
            if x == MISSING {
                continue;
            }
            if ab[x as usize] == u8::MAX {
                ab[x as usize] = y;
            } else if ab[x as usize] != y {
                return false;
            }
            if ba[y as usize] == u8::MAX {
                ba[y as usize] = x;
            } else if ba[y as usize] != x {
                return false;
            }
        }
        true
    };
    let drop: Vec<usize> = (0..out.bins.len())
        .filter(|&f| out.calendar_part[f].is_some())
        .filter(|&f| {
            (0..out.bins.len())
                .any(|g| out.calendar_part[g].is_none() && same(&out.bins[f], &out.bins[g]))
        })
        .collect();
    for &f in drop.iter().rev() {
        out.feat_cols.remove(f);
        out.feat_names.remove(f);
        out.feat_nominal.remove(f);
        out.bins.remove(f);
        out.n_bins.remove(f);
        out.edges.remove(f);
        out.levels.remove(f);
        out.calendar_part.remove(f);
    }
}

fn days_at(col: &TypedCol, r: usize) -> Option<f64> {
    match col {
        TypedCol::Date { vals } => vals[r].map(|v| v as f64),
        TypedCol::Datetime { vals, unit } => {
            vals[r].map(|v| onceover_core::dateutil::timestamp_days(v, *unit) as f64)
        }
        _ => None,
    }
}

/// Quantile binning: bins + upper-edge list. None when degenerate.
fn bin_numeric(vals: &[Option<f64>]) -> Option<(Vec<u8>, Vec<f64>)> {
    let mut sorted: Vec<f64> = vals.iter().flatten().copied().collect();
    if sorted.len() < 30 {
        return None;
    }
    sorted.sort_by(|a, b| a.total_cmp(b));
    let lo = sorted[0];
    let hi = sorted[sorted.len() - 1];
    if hi <= lo {
        return None;
    }
    let n_bins = MAX_BINS.min((sorted.len() / 20).max(2));
    // Upper bounds of bins 0..n_bins-2 (last bin goes to +inf).
    let mut edges: Vec<f64> = Vec::with_capacity(n_bins - 1);
    for k in 1..n_bins {
        let idx = (k * sorted.len()) / n_bins;
        let v = sorted[idx.min(sorted.len() - 1)];
        if edges.last().map(|&e: &f64| v > e).unwrap_or(true) {
            edges.push(v);
        }
    }
    if edges.is_empty() {
        return None;
    }
    let bins: Vec<u8> = vals
        .iter()
        .map(|v| match v {
            None => MISSING,
            Some(x) => {
                let x = *x;
                let mut b = edges.partition_point(|&e| x > e) as u8;
                if b > edges.len() as u8 {
                    b = edges.len() as u8; // == last bin index
                }
                b
            }
        })
        .collect();
    Some((bins, edges))
}

impl EncodedMatrix {
    /// Preserve data types; numeric-looking strings are still strings.
    pub fn literal(&self, f: usize, label: &str) -> serde_json::Value {
        match self.feat_dtypes.get(f).map(String::as_str) {
            Some("boolean") => serde_json::json!(label == "true"),
            Some(t) if t.starts_with("int") || t.starts_with("uint") || t.starts_with("float") => {
                serde_json::from_str(label).unwrap_or_else(|_| serde_json::json!(label))
            }
            _ => serde_json::json!(label),
        }
    }

    pub fn predicates(&self, f: usize, code: u8) -> Vec<onceover_report::Predicate> {
        use onceover_report::{Operator, Predicate};
        if self.feat_nominal[f] {
            return vec![Predicate::equal(
                self.feat_names[f].clone(),
                self.literal(f, &self.levels[f][code as usize]),
            )];
        }
        let edges = &self.edges[f];
        let transform = matches!(
            self.feat_dtypes.get(f).map(String::as_str),
            Some("date" | "datetime")
        )
        .then(|| "day_index".to_string());
        let mut result = Vec::new();
        if code > 0 {
            result.push(Predicate {
                column: self.feat_names[f].clone(),
                operator: Operator::Gt,
                value: serde_json::json!(edges[code as usize - 1]),
                transform: transform.clone(),
            });
        }
        if (code as usize) < edges.len() {
            result.push(Predicate {
                column: self.feat_names[f].clone(),
                operator: Operator::Le,
                value: serde_json::json!(edges[code as usize]),
                transform,
            });
        }
        result
    }

    /// Coarse label for a numeric bin (for human-readable rules):
    /// "lo..hi" using bin edges.
    pub fn numeric_bin_label(&self, f: usize, code: u8) -> Option<String> {
        let edges = &self.edges[f];
        if self.feat_nominal[f] || edges.is_empty() {
            return None;
        }
        let lo = if code == 0 {
            f64::NEG_INFINITY
        } else {
            edges[(code - 1) as usize]
        };
        let hi = if (code as usize) < edges.len() {
            edges[code as usize]
        } else {
            f64::INFINITY
        };
        Some(format!("in ({lo:.4}, {hi:.4}]"))
    }

    pub fn bin_label(&self, f: usize, code: u8) -> String {
        if self.feat_nominal[f] {
            self.levels[f]
                .get(code as usize)
                .cloned()
                .unwrap_or_else(|| "?".into())
        } else {
            self.numeric_bin_label(f, code)
                .unwrap_or_else(|| "?".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bins_are_monotone_and_missing_aware() {
        let vals: Vec<Option<f64>> = (0..200).map(|i| Some(i as f64)).collect();
        let (bins, edges) = bin_numeric(&vals).unwrap();
        assert_eq!(bins.len(), 200);
        assert!(bins.iter().all(|b| *b != MISSING));
        for w in bins.windows(2) {
            assert!(w[0] <= w[1]);
        }
        assert!(!edges.is_empty());
        let with_null = vec![Some(1.0), None, Some(2.0), Some(1.5)];
        assert!(bin_numeric(&with_null).is_none()); // too few values
    }
}
