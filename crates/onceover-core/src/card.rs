//! Column cards: the "what is each column, really?" answer — role inference,
//! stats, concentration, top values, format signatures — plus card-level
//! findings.

use onceover_report::{Method, Pattern};
use std::collections::BTreeMap;

use onceover_report::{
    BoolStats, ColumnCard, Concentration, EvidenceBasis, Finding, NumericStats, PatternCount,
    StringStats, TemporalStats, ValueCount,
};
use rustc_hash::FxHashMap;
use serde_json::json;

use crate::counts::{build_counts, ColCounts, VKey};
use crate::dateutil::{iso_date, iso_datetime};
use crate::typed::TypedCol;

/// Everything the pipeline needs from one column's card computation.
#[derive(Debug, Clone)]
pub struct CardOut {
    pub card: ColumnCard,
    pub findings: Vec<Finding>,
    pub counts: ColCounts,
}

/// Format a VKey for display, dtype-aware (dates/datetimes stored as I).
fn fmt_value(key: &VKey, dtype: &str, col: &TypedCol) -> String {
    match (key, dtype) {
        (VKey::I(v), "date") => iso_date(*v),
        (VKey::I(v), "datetime") => match col {
            TypedCol::Datetime { unit, .. } => crate::dateutil::iso_timestamp(*v, *unit),
            _ => iso_datetime(*v),
        },
        _ => key.display(),
    }
}

fn finding(
    pattern: Pattern,
    subject_cols: &[&str],
    claim: String,
    interest: f64,
    evidence: BTreeMap<String, serde_json::Value>,
    hint: Option<&str>,
) -> Finding {
    let mut subject = BTreeMap::new();
    if let Some(c) = subject_cols.first() {
        subject.insert("column".to_string(), json!(c));
    }
    Finding::new(
        pattern,
        subject,
        claim,
        EvidenceBasis::Measured,
        interest,
        evidence,
        hint.map(|s| s.to_string()),
        Method::new(
            "column_profile",
            "column_statistics",
            "onceover column pass",
        ),
    )
}

fn pct(x: f64) -> String {
    format!("{:.1}%", 100.0 * x)
}

/// Compute `(q25, q50, q75)` in O(n) via partial selection, matching
/// `stats::quantile_sorted` (Hyndman-Fan type 7) exactly.
fn quartiles_unsorted(vals: &mut [f64]) -> (Option<f64>, Option<f64>, Option<f64>) {
    let n = vals.len();
    if n == 0 {
        return (None, None, None);
    }
    if n == 1 {
        let v = vals[0];
        return (Some(v), Some(v), Some(v));
    }
    let interp = |slice: &mut [f64], pos: f64| -> (usize, f64) {
        let lo_idx = pos.floor() as usize;
        let hi_idx = (lo_idx + 1).min(slice.len() - 1);
        let frac = pos - lo_idx as f64;
        let (_, &mut lo_val, right) = slice.select_nth_unstable_by(lo_idx, f64::total_cmp);
        let hi_val = if hi_idx == lo_idx {
            lo_val
        } else {
            right.iter().copied().fold(f64::INFINITY, f64::min)
        };
        (lo_idx, lo_val * (1.0 - frac) + hi_val * frac)
    };

    // Partition at q50 first so q25 lives strictly in the left partition
    // (or touches lo50) and q75 lives in the right partition.
    let pos50 = 0.50 * (n - 1) as f64;
    let lo50 = pos50.floor() as usize;
    let hi50 = (lo50 + 1).min(n - 1);
    let frac50 = pos50 - lo50 as f64;
    let (left, &mut v_lo50, right) = vals.select_nth_unstable_by(lo50, f64::total_cmp);
    let v_hi50 = if hi50 == lo50 {
        v_lo50
    } else {
        right.iter().copied().fold(f64::INFINITY, f64::min)
    };
    let q50 = v_lo50 * (1.0 - frac50) + v_hi50 * frac50;

    let pos25 = 0.25 * (n - 1) as f64;
    let lo25 = pos25.floor() as usize;
    let hi25 = (lo25 + 1).min(n - 1);
    let frac25 = pos25 - lo25 as f64;
    let q25 = if lo25 < lo50 {
        let (_, &mut v_lo25, lright) = left.select_nth_unstable_by(lo25, f64::total_cmp);
        let v_hi25 = if hi25 == lo25 {
            v_lo25
        } else if hi25 == lo50 {
            v_lo50
        } else {
            lright.iter().copied().fold(f64::INFINITY, f64::min)
        };
        v_lo25 * (1.0 - frac25) + v_hi25 * frac25
    } else {
        // n == 2 or 3 where lo25 == lo50
        v_lo50 * (1.0 - frac25) + v_hi50 * frac25
    };

    let pos75 = 0.75 * (n - 1) as f64;
    let lo75 = pos75.floor() as usize;
    let q75 = if lo75 > lo50 {
        let (_, q) = interp(right, pos75 - (lo50 + 1) as f64);
        q
    } else {
        // n == 2 where lo75 == lo50 == 0
        let frac75 = pos75 - lo75 as f64;
        v_lo50 * (1.0 - frac75) + v_hi50 * frac75
    };

    (Some(q25), Some(q50), Some(q75))
}

fn numeric_stats_f64(mut vals: Vec<f64>, integral_forced: bool) -> NumericStats {
    let n = vals.len();
    if n == 0 {
        return NumericStats {
            min: None,
            max: None,
            mean: None,
            std: None,
            q25: None,
            q50: None,
            q75: None,
            zeros: 0,
            negatives: 0,
            integral: true,
        };
    }

    let mut min_v = vals[0];
    let mut max_v = vals[0];
    let mut sum = 0.0f64;
    let mut sum_c = 0.0f64;
    let mut zeros = 0usize;
    let mut negatives = 0usize;
    let mut integral = true;

    for &v in &vals {
        if v < min_v {
            min_v = v;
        }
        if v > max_v {
            max_v = v;
        }
        // Kahan summation for numerical stability without sorting.
        let y = v - sum_c;
        let t = sum + y;
        sum_c = (t - sum) - y;
        sum = t;
        if v == 0.0 {
            zeros += 1;
        }
        if v < 0.0 {
            negatives += 1;
        }
        if !integral_forced && integral && (v.abs() > 9.007_199_254_740_992e15 || v != v.trunc()) {
            integral = false;
        }
    }

    let mean = sum / n as f64;
    let mut var_sum = 0.0f64;
    let mut var_c = 0.0f64;
    for &v in &vals {
        let d = v - mean;
        let y = d * d - var_c;
        let t = var_sum + y;
        var_c = (t - var_sum) - y;
        var_sum = t;
    }
    let std = (var_sum / n as f64).max(0.0).sqrt();
    let (q25, q50, q75) = quartiles_unsorted(&mut vals);

    NumericStats {
        min: Some(min_v),
        max: Some(max_v),
        mean: Some(mean),
        std: Some(std),
        q25,
        q50,
        q75,
        zeros,
        negatives,
        integral,
    }
}

/// Format-signature mapping for a string: digits -> '#', lowercase -> 'a',
/// uppercase -> 'A', other chars kept. Truncated at 24 chars (marked '~').
fn write_format_pattern(s: &str, out: &mut String) {
    out.clear();
    for (i, ch) in s.chars().enumerate() {
        if i >= 24 {
            out.push('~');
            break;
        }
        if ch.is_ascii_digit() {
            out.push('#');
        } else if ch.is_ascii_lowercase() {
            out.push('a');
        } else if ch.is_ascii_uppercase() {
            out.push('A');
        } else {
            out.push(ch);
        }
    }
}

/// Whether two format signatures are a real format difference. Run length
/// alone is not one: `A#` and `A##` are labels with a different number of
/// digits. Words alone are not one either: `Aaaa` and `Aaa Aaaa` are names,
/// though a capitalization difference between them still counts (the caller
/// marks that case cosmetic).
fn patterns_differ(a: &str, b: &str) -> bool {
    let collapse = |p: &str, fold: bool| -> String {
        let mut out = String::with_capacity(p.len());
        for c in p.chars() {
            let c = if fold && c == 'A' { 'a' } else { c };
            if !out.ends_with(c) {
                out.push(c);
            }
        }
        out
    };
    if collapse(a, false) == collapse(b, false) {
        return false;
    }
    let words = |p: &str| p.chars().all(|c| matches!(c, 'A' | 'a' | ' '));
    if words(a) && words(b) {
        return collapse(a, true) == collapse(b, true);
    }
    true
}

/// Build the column card and its findings.
pub fn column_card(name: &str, dtype: &str, col: &TypedCol) -> CardOut {
    let n = col.len();
    let null_count = col.null_count();
    let counts = build_counts(col, 4096);
    let distinct = counts.distinct;
    let n_nonnull = counts.n_nonnull;
    let distinct_ratio = distinct as f64 / n_nonnull.max(1) as f64;
    let null_fraction = if n > 0 {
        null_count as f64 / n as f64
    } else {
        0.0
    };

    let mut findings: Vec<Finding> = Vec::new();
    let mut action_hints: Vec<String> = Vec::new();

    // ---- role inference ----
    let role = if matches!(col, TypedCol::Other { .. }) {
        "other"
    } else if n_nonnull == 0 {
        "all_null"
    } else if distinct <= 1 {
        "constant"
    } else {
        match col {
            TypedCol::Bool { .. } => "boolean",
            TypedCol::Date { .. } => "date",
            TypedCol::Datetime { .. } => "datetime",
            TypedCol::Str { .. } | TypedCol::StrOwned { .. } => {
                if distinct <= 500 {
                    "categorical"
                } else {
                    "free_text"
                }
            }
            TypedCol::I64 { .. }
            | TypedCol::U64 { .. }
            | TypedCol::F64 { .. }
            | TypedCol::Decimal { .. } => {
                let likely_cat = (distinct <= 12 && n_nonnull >= 30)
                    || (distinct_ratio <= 0.02 && distinct <= 100 && n_nonnull >= 100);
                if likely_cat {
                    "categorical"
                } else if distinct_ratio >= 0.98 {
                    "unique"
                } else {
                    "continuous"
                }
            }
            TypedCol::Null { .. } | TypedCol::Other { .. } => "other",
        }
    }
    .to_string();

    // ---- stats ----
    let mut numeric_stats = None;
    let mut string_stats = None;
    let mut temporal_stats = None;
    let mut bool_stats = None;

    match col {
        TypedCol::I64 { vals } => {
            let f: Vec<f64> = vals.iter().flatten().map(|v| *v as f64).collect();
            numeric_stats = Some(numeric_stats_f64(f, true));
        }
        TypedCol::U64 { vals } => {
            let f: Vec<f64> = vals.iter().flatten().map(|v| *v as f64).collect();
            numeric_stats = Some(numeric_stats_f64(f, true));
        }
        TypedCol::F64 { vals } => {
            let f: Vec<f64> = vals.iter().flatten().copied().collect();
            numeric_stats = Some(numeric_stats_f64(f, false));
        }
        TypedCol::Decimal { .. } => {
            let f = (0..col.len()).filter_map(|r| col.numeric_at(r)).collect();
            numeric_stats = Some(numeric_stats_f64(f, false));
        }
        TypedCol::Str { .. } | TypedCol::StrOwned { .. } => {
            let mut min_len = usize::MAX;
            let mut max_len = 0usize;
            let mut sum_len = 0usize;
            let mut n_str = 0usize;
            let mut n_empty = 0usize;
            for i in 0..col.len() {
                if let Some(v) = col.str_at(i) {
                    n_str += 1;
                    if v.is_empty() {
                        n_empty += 1;
                        min_len = 0;
                    } else {
                        let len = if v.is_ascii() {
                            v.len()
                        } else {
                            v.chars().count()
                        };
                        if len < min_len {
                            min_len = len;
                        }
                        if len > max_len {
                            max_len = len;
                        }
                        sum_len += len;
                    }
                }
            }
            string_stats = Some(StringStats {
                min_len: if n_str > 0 { min_len } else { 0 },
                max_len,
                mean_len: if n_str > 0 {
                    sum_len as f64 / n_str as f64
                } else {
                    0.0
                },
                n_empty,
            });
        }
        TypedCol::Bool { vals } => {
            let mut trues = 0usize;
            let mut falses = 0usize;
            for b in vals.iter().flatten() {
                if *b {
                    trues += 1;
                } else {
                    falses += 1;
                }
            }
            bool_stats = Some(BoolStats { trues, falses });
        }
        TypedCol::Date { vals } => {
            let mut min_d: Option<i32> = None;
            let mut max_d: Option<i32> = None;
            for &v in vals.iter().flatten() {
                min_d = Some(min_d.map_or(v, |m| m.min(v)));
                max_d = Some(max_d.map_or(v, |m| m.max(v)));
            }
            temporal_stats = Some(TemporalStats {
                min: min_d.map(|d| iso_date(d as i64)),
                max: max_d.map(|d| iso_date(d as i64)),
                span_days: min_d.zip(max_d).map(|(a, b)| (b as i64) - (a as i64)),
                modal_interval_days: None, // filled by the date pass
            });
        }
        TypedCol::Datetime { vals, unit } => {
            let mut min_t: Option<i64> = None;
            let mut max_t: Option<i64> = None;
            for &v in vals.iter().flatten() {
                min_t = Some(min_t.map_or(v, |m| m.min(v)));
                max_t = Some(max_t.map_or(v, |m| m.max(v)));
            }
            temporal_stats = Some(TemporalStats {
                min: min_t.map(|v| crate::dateutil::iso_timestamp(v, *unit)),
                max: max_t.map(|v| crate::dateutil::iso_timestamp(v, *unit)),
                span_days: min_t.zip(max_t).map(|(a, b)| {
                    (((b as i128) - (a as i128))
                        .div_euclid(86_400 * crate::dateutil::ticks_per_second(*unit) as i128))
                        as i64
                }),
                modal_interval_days: None,
            });
        }
        TypedCol::Null { .. } | TypedCol::Other { .. } => {}
    }

    // ---- top values ----
    let top_values: Vec<ValueCount> = counts
        .top
        .iter()
        .take(5)
        .map(|(k, c)| ValueCount {
            value: fmt_value(k, dtype, col),
            count: *c,
            share: *c as f64 / n_nonnull.max(1) as f64,
        })
        .collect();

    // ---- format patterns (string columns) ----
    let mut format_patterns: Vec<PatternCount> = Vec::new();
    if col.is_string() {
        let mut pats: FxHashMap<Box<str>, (u64, Option<String>)> = FxHashMap::default();
        let mut n_str: usize = 0;
        let mut pat_buf = String::with_capacity(28);
        for i in 0..col.len() {
            if let Some(v) = col.str_at(i) {
                n_str += 1;
                write_format_pattern(v, &mut pat_buf);
                if let Some(entry) = pats.get_mut(pat_buf.as_str()) {
                    entry.0 += 1;
                    if entry.1.is_none() && v.len() <= 40 {
                        entry.1 = Some(v.to_string());
                    }
                } else {
                    let example = (v.len() <= 40).then(|| v.to_string());
                    pats.insert(Box::from(pat_buf.as_str()), (1, example));
                }
                if pats.len() > 32 {
                    // Too many patterns: bucket the rest as "other".
                    if let Some(entry) = pats.get_mut("*") {
                        entry.0 += 1;
                    } else {
                        pats.insert(Box::from("*"), (1, None));
                    }
                }
            }
        }
        let mut list: Vec<(Box<str>, u64, Option<String>)> =
            pats.into_iter().map(|(k, (c, e))| (k, c, e)).collect();
        list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        format_patterns = list
            .iter()
            .take(5)
            .map(|(p, c, e)| PatternCount {
                pattern: p.to_string(),
                count: *c as usize,
                share: *c as f64 / n_str.max(1) as f64,
                example: e.clone(),
            })
            .collect();

        if n_str >= 50 && list.len() >= 2 {
            let top = &list[0];
            let second = &list[1];
            let top_share = top.1 as f64 / n_str as f64;
            let second_share = second.1 as f64 / n_str as f64;
            if top_share < 0.9 && second_share >= 0.05 && patterns_differ(&top.0, &second.0) {
                let mut format_metrics = BTreeMap::from([
                    ("n_values".into(), json!(n_str)),
                    ("n_patterns".into(), json!(list.len())),
                ]);
                // Capitalization-only variation: the two dominant
                // patterns are the same shape at different letter
                // case. Cosmetic — it does not block parsing the way
                // a shape or separator change does — so it declares
                // the demotion the ledger applies.
                if top.0 != second.0 && top.0.replace('A', "a") == second.0.replace('A', "a") {
                    format_metrics
                        .insert("triviality".into(), json!("capitalization_only_format_mix"));
                    format_metrics.insert("triviality_factor".into(), json!(0.6));
                }
                findings.push(finding(
                    Pattern::FormatVariation,
                    &[name],
                    format!(
                        "column {} mixes value formats ({}: {:.0}% '{}' vs {:.0}% '{}')",
                        name,
                        list.len(),
                        100.0 * top_share,
                        top.0,
                        100.0 * second_share,
                        second.0
                    ),
                    0.42,
                    format_metrics,
                    Some("normalize before parsing/casting"),
                ));
            }
        }
    }

    // ---- card-level findings ----
    let conc: Option<Concentration> = counts.concentration.clone();

    if role == "all_null" {
        findings.push(finding(
            Pattern::MissingValues,
            &[name],
            format!("column {} is entirely null", name),
            0.15,
            BTreeMap::from([("n".into(), json!(n))]),
            Some("consider dropping"),
        ));
        action_hints.push("drop_candidate".into());
    } else if role == "constant" {
        let val = counts.top.first().map(|(k, _)| fmt_value(k, dtype, col));
        findings.push(finding(
            Pattern::ValueConcentration,
            &[name],
            format!(
                "column {} is constant{}",
                name,
                val.as_deref()
                    .map(|v| format!(" ('{}')", v))
                    .unwrap_or_default()
            ),
            0.40,
            BTreeMap::from([("n".into(), json!(n)), ("value".into(), json!(val))]),
            Some("drop from modeling; carries no information"),
        ));
        action_hints.push("drop_candidate".into());
    }

    // near-constant
    if distinct == 2 && n_nonnull >= 100 {
        if let Some(c) = &conc {
            if c.top1_share.unwrap_or(0.0) >= 0.99 {
                let top = &counts.top[0];
                let second = &counts.top[1];
                findings.push(finding(
                    Pattern::ValueConcentration,
                    &[name],
                    format!(
                        "column {} is nearly constant: {} '{}' vs {} '{}'",
                        name,
                        pct(c.top1_share.unwrap()),
                        fmt_value(&top.0, dtype, col),
                        counts.top[1].1,
                        fmt_value(&second.0, dtype, col)
                    ),
                    0.45,
                    BTreeMap::from([
                        ("top_share".into(), json!(c.top1_share)),
                        ("n".into(), json!(n_nonnull)),
                    ]),
                    Some("treat the rare value as the signal"),
                ));
            }
        }
    }

    // likely categorical (numeric)
    if role == "categorical"
        && matches!(
            col,
            TypedCol::I64 { .. }
                | TypedCol::U64 { .. }
                | TypedCol::F64 { .. }
                | TypedCol::Decimal { .. }
        )
    {
        action_hints.push("treat_as_categorical".into());
        let mut ev = BTreeMap::from([
            ("distinct".into(), json!(distinct)),
            ("distinct_ratio".into(), json!(distinct_ratio)),
        ]);
        if let Some((k, c)) = counts.top.first() {
            ev.insert("top_value".into(), json!(fmt_value(k, dtype, col)));
            ev.insert("top_count".into(), json!(c));
        }
        findings.push(finding(
            Pattern::CategoricalRole,
            &[name],
            format!(
                "numeric column {} behaves like a category ({} distinct values)",
                name, distinct
            ),
            0.50,
            ev,
            Some("treat as categorical in models; do not scale"),
        ));
    }

    // high concentration. A two-valued column's share of its common
    // value is its rate (a rare condition is rare by definition), so
    // only the near-constant case above is stated for it.
    if distinct >= 3 && n_nonnull >= 50 {
        if let Some(c) = &conc {
            if let Some(t1) = c.top1_share {
                if t1 >= 0.9 {
                    let top = &counts.top[0];
                    findings.push(finding(Pattern::ValueConcentration,
                        &[name],
                        format!(
                            "column {} is highly concentrated: {} '{}' (effective cardinality {:.2})",
                            name,
                            pct(t1),
                            fmt_value(&top.0, dtype, col),
                            c.effective_cardinality
                        ),
                        0.52 + 0.3 * t1,
                        BTreeMap::from([
                            ("hhi".into(), json!(c.hhi)),
                            ("effective_cardinality".into(), json!(c.effective_cardinality)),
                        ]),
                        Some("beware in group-bys and one-hot encoding"),
                    ));
                }
            }
        }
    }

    // identifier-like. Unique floats holding fractional values are
    // measurements; the card's distinct count already records them.
    let fractional = matches!(dtype, "float32" | "float64")
        && !numeric_stats.as_ref().is_some_and(|s| s.integral);
    if distinct_ratio >= 0.98 && n_nonnull >= 100 && !counts.capped && !fractional {
        let role_metrics = BTreeMap::from([
            ("distinct".into(), json!(distinct)),
            ("n".into(), json!(n_nonnull)),
        ]);
        findings.push(finding(
            Pattern::IdentifierRole,
            &[name],
            format!(
                "column {} looks like a unique identifier ({} distinct of {} values)",
                name, distinct, n_nonnull
            ),
            0.60,
            role_metrics,
            Some("candidate join key; exclude from numeric features"),
        ));
    }

    let card = ColumnCard {
        name: name.to_string(),
        dtype: dtype.to_string(),
        card_role: role,
        // Semantic roles are assigned by the frame model in the
        // pipeline, which has the cross-column view; the card pass
        // itself leaves them unset.
        semantic_role: None,
        semantic_role_evidence: None,
        n,
        null_count,
        null_fraction,
        distinct,
        distinct_ratio,
        numeric_stats,
        string_stats,
        temporal_stats,
        bool_stats,
        concentration: conc,
        top_values,
        format_patterns,
        action_hints,
    };

    CardOut {
        card,
        findings,
        counts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_and_unique_roles() {
        let col = TypedCol::I64 {
            vals: vec![Some(1); 10].into(),
        };
        let out = column_card("c", "int64", &col);
        assert_eq!(out.card.card_role, "constant");
        assert!(out
            .findings
            .iter()
            .any(|f| f.category == onceover_report::Category::Concentration));

        let ids: Vec<Option<i64>> = (0..500).map(Some).collect();
        let col = TypedCol::I64 { vals: ids.into() };
        let out = column_card("id", "int64", &col);
        assert_eq!(out.card.card_role, "unique");
        assert!(out
            .findings
            .iter()
            .any(|f| f.category == onceover_report::Category::ColumnRoles
                && f.specification.definition.get("role") == Some(&json!("identifier"))));
    }

    #[test]
    fn likely_categorical_numeric() {
        let vals: Vec<Option<i64>> = (0..1000).map(|i| Some(i % 5)).collect();
        let out = column_card("code", "int64", &TypedCol::I64 { vals: vals.into() });
        assert_eq!(out.card.card_role, "categorical");
        assert!(out
            .findings
            .iter()
            .any(|f| f.category == onceover_report::Category::ColumnRoles
                && f.specification.definition.get("role") == Some(&json!("categorical"))));
    }

    #[test]
    fn string_format_patterns() {
        let vals: Vec<Option<&str>> = (0..100)
            .map(|i| {
                if i % 2 == 0 {
                    Some("2024-01-15")
                } else {
                    Some("15/01/2024")
                }
            })
            .collect();
        let out = column_card("d", "string", &TypedCol::Str { vals });
        assert_eq!(out.card.card_role, "categorical");
        assert!(out
            .findings
            .iter()
            .any(|f| f.category == onceover_report::Category::Formats));
    }

    #[test]
    fn quartiles_unsorted_matches_sorted_exactly() {
        for n in [1, 2, 3, 4, 5, 6, 7, 8, 9, 100, 1001] {
            let orig: Vec<f64> = (0..n)
                .map(|i| ((i * 37 + 13) % (n + 5)) as f64 * 0.37 - 4.0)
                .collect();
            let mut sorted = orig.clone();
            sorted.sort_unstable_by(f64::total_cmp);
            let mut copy = orig.clone();
            let (q25, q50, q75) = quartiles_unsorted(&mut copy);
            assert_eq!(
                q25,
                Some(crate::stats::quantile_sorted(&sorted, 0.25)),
                "n={n} q25"
            );
            assert_eq!(
                q50,
                Some(crate::stats::quantile_sorted(&sorted, 0.50)),
                "n={n} q50"
            );
            assert_eq!(
                q75,
                Some(crate::stats::quantile_sorted(&sorted, 0.75)),
                "n={n} q75"
            );
        }
    }
}
