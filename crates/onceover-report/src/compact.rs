//! Token-compact and size-budgeted JSON serialization of a profile report.

use serde_json::{json, Map, Value};

const BULKY_METRIC_ARRAYS: [&str; 9] = [
    "periods",
    "entity_spans",
    "period_labels",
    "entries_per_period",
    "exits_per_period",
    "period_top_shares",
    "mapping",
    "examples",
    "duplicate_groups",
];
const COMPACT_ARRAY_CAP: usize = 8;

/// Produce a token-compact JSON object for LLM consumption: adds short
/// `f1`/`g1` handles, collapses boilerplate `skipped` on-request coverage
/// entries, trims verbose method/validation prose and oversized repeated
/// metric arrays, and omits `tier == "none"` association entries.
pub fn compact_report_value(data: &Value) -> Value {
    let mut out = data.clone();
    let Some(root) = out.as_object_mut() else {
        return out;
    };

    if let Some(cov) = root.get("coverage").and_then(Value::as_object) {
        let mut compact_cov = Map::new();
        let mut skipped_on_request: Vec<String> = Vec::new();
        for (k, entry) in cov {
            let status = entry.get("status").and_then(Value::as_str);
            let detail = entry.get("detail").and_then(Value::as_str).unwrap_or("");
            if status == Some("skipped") && detail.contains("on-request detector") {
                skipped_on_request.push(k.clone());
            } else if detail.chars().count() > 200 {
                let mut e = entry.clone();
                if let Some(eo) = e.as_object_mut() {
                    let truncated: String = detail.chars().take(197).collect();
                    eo.insert("detail".into(), json!(format!("{truncated}...")));
                }
                compact_cov.insert(k.clone(), e);
            } else {
                compact_cov.insert(k.clone(), entry.clone());
            }
        }
        if !skipped_on_request.is_empty() {
            skipped_on_request.sort();
            compact_cov.insert(
                "skipped_on_request".into(),
                json!({
                    "status": "skipped",
                    "detectors": skipped_on_request,
                }),
            );
        }
        root.insert("coverage".into(), Value::Object(compact_cov));
    }

    for (bucket, prefix) in [("findings", "f"), ("grouped_findings", "g")] {
        if let Some(arr) = root.get_mut(bucket).and_then(Value::as_array_mut) {
            for (idx, f) in arr.iter_mut().enumerate() {
                let Some(fo) = f.as_object_mut() else {
                    continue;
                };
                fo.insert("handle".into(), json!(format!("{prefix}{}", idx + 1)));
                fo.remove("interpretation");
                if fo
                    .get("grouped_members")
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty)
                {
                    fo.remove("grouped_members");
                }
                if fo.get("grouped_under").is_some_and(Value::is_null) {
                    fo.remove("grouped_under");
                }
                if let Some(subj) = fo.get_mut("subject").and_then(Value::as_object_mut) {
                    if subj
                        .get("inputs")
                        .and_then(Value::as_array)
                        .is_some_and(Vec::is_empty)
                    {
                        subj.remove("inputs");
                    }
                    if subj.get("target").is_some_and(Value::is_null) {
                        subj.remove("target");
                    }
                }
                if let Some(evs) = fo.get_mut("evidence").and_then(Value::as_array_mut) {
                    for ev in evs {
                        let Some(evo) = ev.as_object_mut() else {
                            continue;
                        };
                        if let Some(m) = evo.get_mut("method").and_then(Value::as_object_mut) {
                            m.remove("provenance");
                        }
                        if let Some(metrics) = evo.get_mut("metrics").and_then(Value::as_object_mut)
                        {
                            for prose_key in ["method", "validation"] {
                                if metrics
                                    .get(prose_key)
                                    .and_then(Value::as_str)
                                    .is_some_and(|s| s.chars().count() > 120)
                                {
                                    metrics.remove(prose_key);
                                }
                            }
                            let mut additions: Vec<(String, Value)> = Vec::new();
                            for arr_key in BULKY_METRIC_ARRAYS {
                                if let Some(val) = metrics.get_mut(arr_key) {
                                    match val {
                                        Value::Array(items) if items.len() > COMPACT_ARRAY_CAP => {
                                            let total = items.len();
                                            items.truncate(COMPACT_ARRAY_CAP);
                                            additions
                                                .push((format!("_{arr_key}_total"), json!(total)));
                                        }
                                        Value::Object(map) if map.len() > COMPACT_ARRAY_CAP => {
                                            let total = map.len();
                                            let kept: Map<String, Value> = map
                                                .iter()
                                                .take(COMPACT_ARRAY_CAP)
                                                .map(|(k, v)| (k.clone(), v.clone()))
                                                .collect();
                                            *map = kept;
                                            additions
                                                .push((format!("_{arr_key}_total"), json!(total)));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            for (k, v) in additions {
                                metrics.insert(k, v);
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(assocs) = root.get("associations").and_then(Value::as_array) {
        if !assocs.is_empty() {
            let kept: Vec<Value> = assocs
                .iter()
                .filter(|a| a.get("tier").and_then(Value::as_str) != Some("none"))
                .cloned()
                .collect();
            if kept.len() < assocs.len() {
                let omitted = assocs.len() - kept.len();
                root.insert("associations".into(), Value::Array(kept));
                root.insert("associations_omitted_none".into(), json!(omitted));
            }
        }
    }
    out
}

fn index_entry(f: &Value) -> Value {
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
        "interestingness": f.get("interestingness").cloned().unwrap_or(Value::Null),
        "claim": f.get("claim").cloned().unwrap_or(Value::Null),
    })
}

/// Size-budgeted JSON view of a report that never exceeds `max_chars`
/// (in Unicode characters). Mirrors the exact shedding ladder: findings
/// and grouped_findings to index entries via binary search, then column
/// cards to `names_only`, then coverage detail truncation.
pub fn to_json_budgeted_value(
    data: &Value,
    max_chars: usize,
    compact: bool,
) -> Result<String, String> {
    let mut working = if compact {
        compact_report_value(data)
    } else {
        data.clone()
    };
    let findings: Vec<Value> = working
        .get("findings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let grouped: Vec<Value> = working
        .get("grouped_findings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total = findings.len();
    let total_grouped = grouped.len();

    let split = |removed: usize| -> (usize, usize) {
        if removed <= total {
            (total - removed, total_grouped)
        } else {
            (0, total_grouped.saturating_sub(removed - total))
        }
    };

    let render = |work: &Value, removed: usize, cols_mode: &str, cov_mode: &str| -> String {
        let (full_findings, full_grouped) = split(removed);
        let mut view = Map::new();
        view.insert(
            "schema_version".into(),
            work.get("schema_version").cloned().unwrap_or(Value::Null),
        );
        view.insert(
            "frame".into(),
            work.get("frame").cloned().unwrap_or(Value::Null),
        );
        view.insert(
            "overview".into(),
            work.get("overview").cloned().unwrap_or(Value::Null),
        );
        if cols_mode == "names_only" {
            let names: Vec<Value> = work
                .get("columns")
                .and_then(Value::as_object)
                .map(|m| m.keys().map(|k| json!(k)).collect())
                .unwrap_or_default();
            view.insert("columns".into(), json!({}));
            view.insert("column_names".into(), Value::Array(names));
        } else {
            view.insert(
                "columns".into(),
                work.get("columns").cloned().unwrap_or_else(|| json!({})),
            );
        }
        view.insert(
            "findings".into(),
            Value::Array(findings[..full_findings].to_vec()),
        );
        view.insert(
            "finding_index".into(),
            Value::Array(findings[full_findings..].iter().map(index_entry).collect()),
        );
        view.insert(
            "grouped_findings".into(),
            Value::Array(grouped[..full_grouped].to_vec()),
        );
        view.insert(
            "grouped_index".into(),
            Value::Array(grouped[full_grouped..].iter().map(index_entry).collect()),
        );
        view.insert(
            "groups".into(),
            work.get("groups").cloned().unwrap_or_else(|| json!([])),
        );
        view.insert(
            "selection".into(),
            work.get("selection").cloned().unwrap_or(Value::Null),
        );
        view.insert(
            "coverage".into(),
            work.get("coverage").cloned().unwrap_or_else(|| json!({})),
        );
        view.insert(
            "associations".into(),
            work.get("associations")
                .cloned()
                .unwrap_or_else(|| json!([])),
        );
        view.insert(
            "budget".into(),
            json!({
                "max_chars": max_chars,
                "full_findings": full_findings,
                "indexed_findings": total - full_findings,
                "full_grouped_findings": full_grouped,
                "indexed_grouped_findings": total_grouped - full_grouped,
                "columns": cols_mode,
                "coverage_detail": cov_mode,
            }),
        );
        serde_json::to_string(&Value::Object(view)).unwrap_or_default()
    };

    let ceiling = total + total_grouped;
    let removed = if render(&working, ceiling, "full", "full").chars().count() > max_chars {
        ceiling
    } else {
        let (mut lo, mut hi) = (0usize, ceiling);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if render(&working, mid, "full", "full").chars().count() > max_chars {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    };

    let mut cols_mode = "full";
    let mut cov_mode = "full";
    let mut out = render(&working, removed, cols_mode, cov_mode);
    if out.chars().count() > max_chars {
        cols_mode = "names_only";
        out = render(&working, removed, cols_mode, cov_mode);
    }
    if out.chars().count() > max_chars {
        if let Some(cov) = working.get_mut("coverage").and_then(Value::as_object_mut) {
            for entry in cov.values_mut() {
                if let Some(detail) = entry.get("detail").and_then(Value::as_str) {
                    if detail.chars().count() > 160 {
                        let trunc: String = detail.chars().take(160).collect();
                        if let Some(eo) = entry.as_object_mut() {
                            eo.insert("detail".into(), json!(format!("{trunc} [truncated]")));
                        }
                    }
                }
            }
        }
        cov_mode = "truncated";
        out = render(&working, removed, cols_mode, cov_mode);
    }
    if out.chars().count() > max_chars {
        if let Some(cov) = working.get_mut("coverage").and_then(Value::as_object_mut) {
            for entry in cov.values_mut() {
                if let Some(eo) = entry.as_object_mut() {
                    eo.insert("detail".into(), json!(""));
                }
            }
        }
        cov_mode = "status_only";
        out = render(&working, removed, cols_mode, cov_mode);
    }
    if out.chars().count() > max_chars {
        return Err(format!(
            "max_chars={max_chars} is below the minimal budgeted report skeleton ({} chars with all {total} findings and {total_grouped} grouped findings indexed)",
            out.chars().count()
        ));
    }
    Ok(out)
}
