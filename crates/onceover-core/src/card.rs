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

fn quantile(sorted: &[f64], q: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        Some(sorted[lo])
    } else {
        let w = pos - lo as f64;
        Some(sorted[lo] * (1.0 - w) + sorted[hi] * w)
    }
}

fn numeric_stats_f64(vals: Vec<f64>, integral_forced: bool) -> NumericStats {
    let mut sorted = vals;
    sorted.sort_unstable_by(|a, b| a.total_cmp(b));
    let vals = &sorted;
    let n = vals.len();
    let mean = if n > 0 {
        Some(vals.iter().sum::<f64>() / n as f64)
    } else {
        None
    };
    let std = match (n, mean) {
        (0, _) | (_, None) => None,
        (n, Some(m)) => {
            let var = vals.iter().map(|v| (v - m) * (v - m)).sum::<f64>() / n as f64;
            Some(var.sqrt())
        }
    };
    NumericStats {
        min: sorted.first().copied(),
        max: sorted.last().copied(),
        mean,
        std,
        q25: quantile(&sorted, 0.25),
        q50: quantile(&sorted, 0.50),
        q75: quantile(&sorted, 0.75),
        zeros: vals.iter().filter(|v| **v == 0.0).count(),
        negatives: vals.iter().filter(|v| **v < 0.0).count(),
        integral: integral_forced
            || vals
                .iter()
                .all(|v| v.abs() <= 9.007_199_254_740_992e15 && *v == v.trunc()),
    }
}

/// Format-signature mapping for a string: digits -> '#', lowercase -> 'a',
/// uppercase -> 'A', other chars kept. Truncated at 24 chars (marked '~').
fn format_pattern(s: &str) -> Box<str> {
    let mut out = String::with_capacity(s.len().min(24));
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
    Box::from(out.as_str())
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
            TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. } => {
                let likely_cat = (distinct <= 12 && n_nonnull >= 30)
                    || (distinct_ratio <= 0.02 && distinct <= 100 && n_nonnull >= 100);
                if likely_cat {
                    "categorical"
                } else if distinct_ratio >= 0.98 {
                    "identifier"
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
        TypedCol::Str { .. } | TypedCol::StrOwned { .. } => {
            let mut lens: Vec<usize> = Vec::new();
            for i in 0..col.len() {
                if let Some(v) = col.str_at(i) {
                    lens.push(v.chars().count());
                }
            }
            lens.sort_unstable();
            let n_str = lens.len();
            string_stats = Some(StringStats {
                min_len: lens.first().copied().unwrap_or(0),
                max_len: lens.last().copied().unwrap_or(0),
                mean_len: if n_str > 0 {
                    lens.iter().sum::<usize>() as f64 / n_str as f64
                } else {
                    0.0
                },
                n_empty: (0..col.len())
                    .filter(|i| col.str_at(*i) == Some(""))
                    .count(),
            });
        }
        TypedCol::Bool { vals } => {
            bool_stats = Some(BoolStats {
                trues: vals.iter().flatten().filter(|b| **b).count(),
                falses: vals.iter().flatten().filter(|b| !**b).count(),
            });
        }
        TypedCol::Date { vals } => {
            let ds: Vec<i32> = vals.iter().flatten().copied().collect();
            let bounds = [ds.iter().min().copied(), ds.iter().max().copied()];
            let ds: Vec<i32> = bounds.into_iter().flatten().collect();
            temporal_stats = Some(TemporalStats {
                min: ds.first().map(|d| iso_date(*d as i64)),
                max: ds.last().map(|d| iso_date(*d as i64)),
                span_days: ds
                    .first()
                    .zip(ds.last())
                    .map(|(a, b)| (*b as i64) - (*a as i64)),
                modal_interval_days: None, // filled by the date pass
            });
        }
        TypedCol::Datetime { vals, unit } => {
            let ds: Vec<i64> = vals.iter().flatten().copied().collect();
            let bounds = [ds.iter().min().copied(), ds.iter().max().copied()];
            let ds: Vec<i64> = bounds.into_iter().flatten().collect();
            temporal_stats = Some(TemporalStats {
                min: ds
                    .first()
                    .map(|v| crate::dateutil::iso_timestamp(*v, *unit)),
                max: ds.last().map(|v| crate::dateutil::iso_timestamp(*v, *unit)),
                span_days: ds.first().zip(ds.last()).map(|(a, b)| {
                    (((*b as i128) - (*a as i128))
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
        for i in 0..col.len() {
            if let Some(v) = col.str_at(i) {
                n_str += 1;
                let p = format_pattern(v);
                let entry = pats.entry(p).or_insert((0, None));
                entry.0 += 1;
                if entry.1.is_none() && v.len() <= 40 {
                    entry.1 = Some(v.to_string());
                }
                if pats.len() > 32 {
                    // Too many patterns: bucket the rest as "other".
                    let entry = pats.entry(Box::from("*")).or_insert((0, None));
                    entry.0 += 1;
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
            if top_share < 0.9 && second_share >= 0.05 {
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
            TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. }
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

    // high concentration
    if distinct >= 2 && n_nonnull >= 50 {
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

    // identifier-like
    if distinct_ratio >= 0.98 && n_nonnull >= 100 && !counts.capped {
        let mut role_metrics = BTreeMap::from([
            ("distinct".into(), json!(distinct)),
            ("n".into(), json!(n_nonnull)),
        ]);
        if matches!(dtype, "float32" | "float64") {
            // A float column at ~100% distinct is a measurement, not
            // an identifier; declare the demotion the ledger applies.
            role_metrics.insert(
                "triviality".into(),
                json!("continuous_measure_key_candidate"),
            );
            role_metrics.insert("triviality_factor".into(), json!(0.6));
        }
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
        role,
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
    fn constant_and_identifier_roles() {
        let col = TypedCol::I64 {
            vals: vec![Some(1); 10].into(),
        };
        let out = column_card("c", "int64", &col);
        assert_eq!(out.card.role, "constant");
        assert!(out
            .findings
            .iter()
            .any(|f| f.category == onceover_report::Category::Concentration));

        let ids: Vec<Option<i64>> = (0..500).map(Some).collect();
        let col = TypedCol::I64 { vals: ids.into() };
        let out = column_card("id", "int64", &col);
        assert_eq!(out.card.role, "identifier");
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
        assert_eq!(out.card.role, "categorical");
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
        assert_eq!(out.card.role, "categorical");
        assert!(out
            .findings
            .iter()
            .any(|f| f.category == onceover_report::Category::Formats));
    }
}
