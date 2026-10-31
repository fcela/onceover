//! Shared predicate evaluation makes rule evidence comparable across detectors.
use onceover_core::{TypedCol, TypedTable};
use onceover_report::{
    Category, Evidence, EvidenceBasis, EvidenceScope, Finding, Method, Operator, Predicate,
    Sampling, Validation,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn value(col: &TypedCol, row: usize, transform: Option<&str>) -> Option<Value> {
    if transform.is_some_and(|t| t != "day_index") {
        return None;
    }
    if transform == Some("day_index")
        && !matches!(col, TypedCol::Date { .. } | TypedCol::Datetime { .. })
    {
        return None;
    }
    match col {
        TypedCol::I64 { vals } => vals.at(row).map(|v| json!(v)),
        TypedCol::U64 { vals } => vals.at(row).map(|v| json!(v)),
        TypedCol::F64 { vals } => vals.at(row).map(|v| json!(v)),
        TypedCol::Str { .. } | TypedCol::StrOwned { .. } => col.str_at(row).map(|s| json!(s)),
        TypedCol::Bool { vals } => vals[row].map(|b| json!(b)),
        TypedCol::Date { vals } => vals[row].map(|v| {
            if transform == Some("day_index") {
                json!(v)
            } else {
                json!(onceover_core::dateutil::iso_date(v as i64))
            }
        }),
        TypedCol::Datetime { vals, unit } => vals[row].map(|v| {
            if transform == Some("day_index") {
                json!(onceover_core::dateutil::timestamp_days(v, *unit))
            } else {
                json!(onceover_core::dateutil::iso_timestamp(v, *unit))
            }
        }),
        _ => None,
    }
}
fn compare(a: &Value, b: &Value, op: Operator) -> bool {
    let order = if let (Some(x), Some(y)) = (a.as_i64(), b.as_i64()) {
        Some(x.cmp(&y))
    } else if let (Some(x), Some(y)) = (a.as_u64(), b.as_u64()) {
        Some(x.cmp(&y))
    } else if let (Some(x), Some(y)) = (a.as_f64(), b.as_f64()) {
        x.partial_cmp(&y)
    } else if let (Some(x), Some(y)) = (a.as_str(), b.as_str()) {
        Some(x.cmp(y))
    } else if let (Some(x), Some(y)) = (a.as_bool(), b.as_bool()) {
        Some(x.cmp(&y))
    } else {
        None
    };
    use std::cmp::Ordering::*;
    match op {
        Operator::Eq => order == Some(Equal),
        Operator::Ne => order.is_some_and(|o| o != Equal),
        Operator::Lt => order == Some(Less),
        Operator::Le => matches!(order, Some(Less | Equal)),
        Operator::Gt => order == Some(Greater),
        Operator::Ge => matches!(order, Some(Greater | Equal)),
    }
}
fn partition(row: u32) -> bool {
    let mut x = row as u64 + 0x9e3779b97f4a7c15;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    (x ^ (x >> 31)).is_multiple_of(5)
}
fn describe(p: &Predicate) -> String {
    let op = match p.operator {
        Operator::Eq => "=",
        Operator::Ne => "!=",
        Operator::Lt => "<",
        Operator::Le => "<=",
        Operator::Gt => ">",
        Operator::Ge => ">=",
    };
    let column = if let Some(t) = &p.transform {
        format!("{t}({})", p.column)
    } else {
        p.column.clone()
    };
    format!("{column} {op} {}", p.value)
}

pub fn evaluate_rules(
    table: &TypedTable,
    findings: &mut Vec<Finding>,
    row_cap: usize,
    policy: &onceover_report::SelectionPolicy,
) {
    let rows = onceover_core::sample_rows(table.n, row_cap);
    let columns: BTreeMap<&str, usize> = table
        .names
        .iter()
        .enumerate()
        .map(|(i, name)| (name.as_str(), i))
        .collect();
    findings.retain_mut(|finding| {
        // Wide-form rules were evaluated against their own source table
        // before being merged into the long-form report. Its column names
        // (and row population) are not those of this table.
        if finding.evidence.iter().any(|e| {
            e.metrics.get("source_form").and_then(Value::as_str) == Some("pivoted wide form")
                && e.method.detector == "rule_evaluator"
                && e.validation.method == "diagnostic_partition"
        }) {
            return true;
        }
        if finding.category != Category::ConditionalRules {
            return true;
        }
        let Some(outcome) = finding.specification.outcome.as_ref() else {
            return false;
        };
        let conditions = &finding.specification.conditions;
        if conditions.is_empty() || conditions.iter().any(|p| p.column == outcome.column) {
            return false;
        }
        let Some(&target) = columns.get(outcome.column.as_str()) else {
            return false;
        };
        let Some(indices) = conditions.iter().map(|p| columns.get(p.column.as_str()).copied())
            .collect::<Option<Vec<_>>>() else {
            return false;
        };
        let (mut train, mut n, mut antecedent, mut consequent, mut joint) = (0, 0, 0, 0, 0);
        for &row in &rows {
            let index = row as usize;
            let Some(y) = value(&table.cols[target], index, outcome.transform.as_deref()) else {
                continue;
            };
            let Some(xs) = conditions.iter().zip(&indices)
                .map(|(p, &c)| value(&table.cols[c], index, p.transform.as_deref()))
                .collect::<Option<Vec<_>>>() else {
                continue;
            };
            if !partition(row) {
                train += 1;
                continue;
            }
            n += 1;
            let a = conditions.iter().zip(xs).all(|(p, x)| compare(&x, &p.value, p.operator));
            let b = compare(&y, &outcome.value, outcome.operator);
            antecedent += usize::from(a);
            consequent += usize::from(b);
            joint += usize::from(a && b);
        }
        if n < 30 || antecedent < policy.min_rule_antecedent || joint < policy.min_rule_support {
            return false;
        }
        let rate = joint as f64 / antecedent as f64;
        let baseline = consequent as f64 / n as f64;
        let lift = rate / baseline;
        if rate <= baseline || (rate - baseline < policy.min_rule_gain && lift < policy.min_rule_lift) {
            return false;
        }
        let metrics = BTreeMap::from([
            ("eligible_rows".into(), json!(n)),
            ("antecedent_count".into(), json!(antecedent)),
            ("outcome_count".into(), json!(consequent)),
            ("joint_count".into(), json!(joint)),
            ("joint_support".into(), json!(joint as f64 / n as f64)),
            ("conditional_rate".into(), json!(rate)),
            ("baseline_rate".into(), json!(baseline)),
            ("lift".into(), json!(lift)),
        ]);
        finding.claim = format!(
            "When {}, {} holds in {joint} of {antecedent} matching diagnostic rows ({:.2}%; baseline {:.2}%)",
            conditions.iter().map(describe).collect::<Vec<_>>().join(" and "),
            describe(outcome), 100.0 * rate, 100.0 * baseline,
        );
        finding.evidence.push(Evidence {
            method: Method::new("rule_evaluator", "predicate_counts",
                "Common deterministic diagnostic partition; discovery may have seen these rows; reused for selection"),
            basis: EvidenceBasis::Measured,
            scope: EvidenceScope {
                sampling: Sampling::Sample,
                population_rows: Some(table.n),
                rows_evaluated: Some(n),
                missing_values: "complete conditions and outcome".into(),
            },
            validation: Validation {
                method: "diagnostic_partition".into(),
                training_rows: Some(train),
                evaluation_rows: Some(n),
                folds: None,
                used_for_selection: true,
            },
            metrics,
        });
        true
    });
}
