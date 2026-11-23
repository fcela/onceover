//! Finding lookup (by stable ID or short `f1`/`g1` handle), filtering, and
//! report-to-report diffing.

use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const CARD_QUANTITIES: [&str; 4] = ["n", "null_count", "null_fraction", "distinct"];
const CARD_NUMERIC_QUANTITIES: [&str; 3] = ["mean", "min", "max"];

/// Resolve a stable ID or short report-local handle (`"f1"`, `"g1"`) to
/// `(is_grouped, index)` into `findings` or `grouped_findings`.
pub fn finding_slot_in_value(data: &Value, finding_id: &str) -> Option<(bool, usize)> {
    let findings = data
        .get("findings")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let grouped = data
        .get("grouped_findings")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    if let Some(rest) = finding_id.strip_prefix('f') {
        if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(n) = rest.parse::<usize>() {
                if n >= 1 && n <= findings.len() {
                    return Some((false, n - 1));
                }
            }
        }
    } else if let Some(rest) = finding_id.strip_prefix('g') {
        if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(n) = rest.parse::<usize>() {
                if n >= 1 && n <= grouped.len() {
                    return Some((true, n - 1));
                }
            }
        }
    }

    if let Some(idx) = findings
        .iter()
        .position(|f| f.get("id").and_then(Value::as_str) == Some(finding_id))
    {
        return Some((false, idx));
    }
    if let Some(idx) = grouped
        .iter()
        .position(|f| f.get("id").and_then(Value::as_str) == Some(finding_id))
    {
        return Some((true, idx));
    }
    None
}

/// Look up a finding by stable SHA-256 ID or short report-local handle
/// (`"f1"`..`"fN"` for `findings`, `"g1"`..`"gM"` for `grouped_findings`).
pub fn finding_by_id_or_handle<'a>(data: &'a Value, finding_id: &str) -> Option<&'a Value> {
    let (is_grouped, idx) = finding_slot_in_value(data, finding_id)?;
    let key = if is_grouped {
        "grouped_findings"
    } else {
        "findings"
    };
    data.get(key).and_then(Value::as_array)?.get(idx)
}

/// Filter parameters for [`select_findings_value`].
#[derive(Default, Clone, Debug)]
pub struct SelectFilter<'a> {
    pub category: Option<&'a str>,
    pub finding_type: Option<&'a str>,
    pub column: Option<&'a str>,
    pub target: Option<&'a str>,
    pub scope: Option<&'a str>,
    pub object: Option<&'a str>,
    pub support: Option<&'a str>,
    pub conditionality: Option<&'a str>,
    pub temporality: Option<&'a str>,
    pub quality_dimension: Option<&'a str>,
    pub status: Option<&'a str>,
    pub include_grouped: bool,
}

/// Return every finding in `data` matching all supplied filter criteria.
pub fn select_findings_value(data: &Value, filter: &SelectFilter<'_>) -> Vec<Value> {
    let findings = data
        .get("findings")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let grouped = if filter.include_grouped {
        data.get("grouped_findings")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    } else {
        &[]
    };
    let facet_checks = [
        ("scope", filter.scope),
        ("object", filter.object),
        ("support", filter.support),
        ("conditionality", filter.conditionality),
        ("temporality", filter.temporality),
        ("quality_dimension", filter.quality_dimension),
        ("status", filter.status),
    ];

    let mut out = Vec::new();
    for f in findings.iter().chain(grouped.iter()) {
        if let Some(cat) = filter.category {
            if f.get("category").and_then(Value::as_str) != Some(cat) {
                continue;
            }
        }
        if let Some(ty) = filter.finding_type {
            if f.get("type").and_then(Value::as_str) != Some(ty) {
                continue;
            }
        }
        let facets = f.get("facets");
        if facet_checks.iter().any(|&(name, want)| {
            want.is_some_and(|w| {
                facets.and_then(|fc| fc.get(name)).and_then(Value::as_str) != Some(w)
            })
        }) {
            continue;
        }
        let subj = f.get("subject");
        if let Some(col) = filter.column {
            let has_col = subj
                .and_then(|s| s.get("columns"))
                .and_then(Value::as_array)
                .is_some_and(|cols| cols.iter().any(|c| c.as_str() == Some(col)));
            if !has_col {
                continue;
            }
        }
        if let Some(tgt) = filter.target {
            if subj.and_then(|s| s.get("target")).and_then(Value::as_str) != Some(tgt) {
                continue;
            }
        }
        out.push(f.clone());
    }
    out
}

fn quantity_change(before: &Value, after: &Value) -> Value {
    let mut entry = Map::new();
    entry.insert("before".into(), before.clone());
    entry.insert("after".into(), after.clone());
    if let (Some(b), Some(a)) = (before.as_f64(), after.as_f64()) {
        // Keep exact integer delta when both inputs are integers.
        if let (Some(bi), Some(ai)) = (before.as_i64(), after.as_i64()) {
            entry.insert("delta".into(), json!(ai - bi));
        } else {
            entry.insert("delta".into(), json!(a - b));
        }
        if b != 0.0 {
            entry.insert("relative".into(), json!((a - b) / b));
        }
    }
    Value::Object(entry)
}

fn card_changes(base_card: &Value, other_card: &Value) -> Map<String, Value> {
    let mut changes = Map::new();
    for key in CARD_QUANTITIES {
        let before = base_card.get(key).unwrap_or(&Value::Null);
        let after = other_card.get(key).unwrap_or(&Value::Null);
        if before != after {
            changes.insert(key.into(), quantity_change(before, after));
        }
    }
    let base_stats = base_card.get("numeric_stats");
    let other_stats = other_card.get("numeric_stats");
    for key in CARD_NUMERIC_QUANTITIES {
        let before = base_stats.and_then(|s| s.get(key)).unwrap_or(&Value::Null);
        let after = other_stats.and_then(|s| s.get(key)).unwrap_or(&Value::Null);
        if before != after {
            changes.insert(key.into(), quantity_change(before, after));
        }
    }
    changes
}

fn column_diff(
    base_columns: Option<&Map<String, Value>>,
    other_columns: Option<&Map<String, Value>>,
) -> Value {
    let empty = Map::new();
    let base = base_columns.unwrap_or(&empty);
    let other = other_columns.unwrap_or(&empty);
    let names: BTreeSet<&str> = base
        .keys()
        .chain(other.keys())
        .map(String::as_str)
        .collect();

    let brief = |name: &str, card: &Value| -> Value {
        json!({
            "name": name,
            "dtype": card.get("dtype").cloned().unwrap_or(Value::Null),
            "card_role": card.get("card_role").cloned().unwrap_or(Value::Null),
        })
    };

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut retyped = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged = 0usize;

    for name in names {
        match (base.get(name), other.get(name)) {
            (None, Some(oc)) => added.push(brief(name, oc)),
            (Some(bc), None) => removed.push(brief(name, bc)),
            (Some(bc), Some(oc)) => {
                let bd = bc.get("dtype").unwrap_or(&Value::Null);
                let od = oc.get("dtype").unwrap_or(&Value::Null);
                if bd != od {
                    retyped.push(json!({
                        "name": name,
                        "dtype": [bd, od],
                    }));
                } else {
                    let ch = card_changes(bc, oc);
                    if !ch.is_empty() {
                        changed.push(json!({
                            "name": name,
                            "changes": Value::Object(ch),
                        }));
                    } else {
                        unchanged += 1;
                    }
                }
            }
            (None, None) => {}
        }
    }
    json!({
        "added": added,
        "removed": removed,
        "retyped": retyped,
        "changed": changed,
        "unchanged": unchanged,
    })
}

/// Diff two report JSON values: findings added, removed, or changed
/// (matched by stable ID), coverage status changes, and column-card diffs.
pub fn diff_report_values(base: &Value, other: &Value) -> Value {
    let brief = |f: &Value| -> Value {
        let cols = f
            .get("subject")
            .and_then(|s| s.get("columns"))
            .cloned()
            .unwrap_or_else(|| json!([]));
        json!({
            "id": f.get("id").cloned().unwrap_or(Value::Null),
            "category": f.get("category").cloned().unwrap_or(Value::Null),
            "type": f.get("type").cloned().unwrap_or(Value::Null),
            "columns": cols,
            "claim": f.get("claim").cloned().unwrap_or(Value::Null),
            "interestingness": f.get("interestingness").cloned().unwrap_or(Value::Null),
        })
    };

    let mut base_map: BTreeMap<&str, &Value> = BTreeMap::new();
    if let Some(arr) = base.get("findings").and_then(Value::as_array) {
        for f in arr {
            if let Some(id) = f.get("id").and_then(Value::as_str) {
                base_map.insert(id, f);
            }
        }
    }
    let mut new_map: BTreeMap<&str, &Value> = BTreeMap::new();
    if let Some(arr) = other.get("findings").and_then(Value::as_array) {
        for f in arr {
            if let Some(id) = f.get("id").and_then(Value::as_str) {
                new_map.insert(id, f);
            }
        }
    }

    let mut added: Vec<Value> = Vec::new();
    let mut removed: Vec<Value> = Vec::new();
    let mut changed: Vec<(f64, Value)> = Vec::new();
    let mut common = 0usize;

    for (&id, &f) in &new_map {
        if !base_map.contains_key(id) {
            added.push(brief(f));
        }
    }
    for (&id, &f) in &base_map {
        if !new_map.contains_key(id) {
            removed.push(brief(f));
        }
    }
    for (&id, &a) in &base_map {
        if let Some(&b) = new_map.get(id) {
            common += 1;
            let sa = a
                .get("interestingness")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let sb = b
                .get("interestingness")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            if a.get("claim") != b.get("claim") || (sa - sb).abs() > 1e-9 {
                let cols = b
                    .get("subject")
                    .and_then(|s| s.get("columns"))
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                changed.push((
                    (sb - sa).abs(),
                    json!({
                        "id": id,
                        "category": b.get("category").cloned().unwrap_or(Value::Null),
                        "type": b.get("type").cloned().unwrap_or(Value::Null),
                        "columns": cols,
                        "claim": [
                            a.get("claim").cloned().unwrap_or(Value::Null),
                            b.get("claim").cloned().unwrap_or(Value::Null),
                        ],
                        "interestingness": [sa, sb],
                    }),
                ));
            }
        }
    }
    changed.sort_by(|a, b| b.0.total_cmp(&a.0));
    let unchanged = common - changed.len();

    let empty = Map::new();
    let cov_a = base
        .get("coverage")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let cov_b = other
        .get("coverage")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let cov_names: BTreeSet<&str> = cov_a
        .keys()
        .chain(cov_b.keys())
        .map(String::as_str)
        .collect();
    let mut coverage_changes = Map::new();
    for name in cov_names {
        let sa = cov_a
            .get(name)
            .and_then(|e| e.get("status"))
            .unwrap_or(&Value::Null);
        let sb = cov_b
            .get(name)
            .and_then(|e| e.get("status"))
            .unwrap_or(&Value::Null);
        if sa != sb {
            coverage_changes.insert(name.into(), json!([sa, sb]));
        }
    }

    json!({
        "added": added,
        "removed": removed,
        "changed": changed.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
        "unchanged": unchanged,
        "coverage_changes": Value::Object(coverage_changes),
        "columns": column_diff(
            base.get("columns").and_then(Value::as_object),
            other.get("columns").and_then(Value::as_object),
        ),
    })
}
