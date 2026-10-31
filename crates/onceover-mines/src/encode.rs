//! Shared encoded matrix for the model-based passes (predictability screen,
//! decision tables, PRISM rules): every usable column quantile-binned or
//! level-coded over a deterministic row subsample.

use onceover_core::TypedCol;
use onceover_core::TypedTable;
use rustc_hash::FxHashMap;

pub const MISSING: u8 = 255;
pub const MAX_BINS: usize = 32;

#[derive(Debug, Clone)]
pub struct EncodedMatrix {
    /// Subsampled original row ids (deterministic stratified sample).
    pub rows: Vec<u32>,
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
}

pub struct EncodeConfig {
    pub row_cap: usize,
    pub feature_cap: usize,
    /// Max distinct string levels coded as a nominal feature.
    pub nominal_max_levels: usize,
}

impl Default for EncodeConfig {
    fn default() -> Self {
        EncodeConfig {
            row_cap: 20_000,
            feature_cap: 48,
            nominal_max_levels: 32,
        }
    }
}

pub fn encode(table: &TypedTable, cfg: &EncodeConfig) -> (EncodedMatrix, String) {
    let rows: Vec<u32> = crate::stride_rows(table.n, cfg.row_cap);
    let m = rows.len();
    let mut out = EncodedMatrix {
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
    };
    let mut skipped = 0usize;

    for (ci, col) in table.cols.iter().enumerate() {
        match col {
            TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. } => {
                let vals: Vec<Option<f64>> = (0..m)
                    .map(|j| numeric_at(col, out.rows[j] as usize))
                    .collect();
                if let Some((bins, edges)) = bin_numeric(&vals) {
                    out.feat_cols.push(ci);
                    out.feat_names.push(table.names[ci].clone());
                    out.feat_nominal.push(false);
                    out.bins.push(bins);
                    out.n_bins.push(edges.len() + 1);
                    out.edges.push(edges);
                    out.levels.push(Vec::new());
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
                } else {
                    skipped += 1;
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
            }
            TypedCol::Null { .. } | TypedCol::Other { .. } => {
                skipped += 1;
            }
        }
    }

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

pub(crate) fn numeric_at(col: &TypedCol, r: usize) -> Option<f64> {
    match col {
        TypedCol::I64 { vals } => vals.at(r).map(|v| v as f64),
        TypedCol::U64 { vals } => vals.at(r).map(|v| v as f64),
        TypedCol::F64 { vals } => vals.at(r),
        _ => None,
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
