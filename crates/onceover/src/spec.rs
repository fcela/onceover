//! Frame spec inference, hierarchy discovery, and spec-to-hints application
//! (`infer_spec` / `apply_spec`).

use crate::frame::ROLE_VOCABULARY;
use crate::{profile, Budget, ProfileConfig};
use onceover_core::{extract, TypedCol};
use onceover_report::ProfileHints;
use polars::prelude::{DataFrame, DataType};
use rustc_hash::FxHashMap;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const SPEC_VERSION: u64 = 1;
const MAX_HIERARCHY_COLUMNS: usize = 16;
const MAX_HIERARCHY_CHAINS: usize = 32;

/// Dense integer codes `0..distinct` for a discrete column with zero nulls,
/// or `None` if the column has any null or an unsupported dtype.
fn dense_codes_if_complete(col: &TypedCol<'_>, n: usize) -> Option<(Vec<u32>, usize)> {
    if col.null_count() > 0 || n == 0 {
        return None;
    }
    match col {
        TypedCol::Str { vals } => {
            let mut map: FxHashMap<&str, u32> = FxHashMap::default();
            let mut codes = Vec::with_capacity(n);
            for v in vals {
                let s = (*v)?;
                let next = map.len() as u32;
                let code = *map.entry(s).or_insert(next);
                codes.push(code);
            }
            Some((codes, map.len()))
        }
        TypedCol::StrOwned { vals } => {
            let mut map: FxHashMap<&str, u32> = FxHashMap::default();
            let mut codes = Vec::with_capacity(n);
            for v in vals {
                let s = v.as_deref()?;
                let next = map.len() as u32;
                let code = *map.entry(s).or_insert(next);
                codes.push(code);
            }
            Some((codes, map.len()))
        }
        TypedCol::Bool { vals } => {
            let mut map: FxHashMap<bool, u32> = FxHashMap::default();
            let mut codes = Vec::with_capacity(n);
            for v in vals {
                let b = (*v)?;
                let next = map.len() as u32;
                let code = *map.entry(b).or_insert(next);
                codes.push(code);
            }
            Some((codes, map.len()))
        }
        TypedCol::I64 { vals } => {
            let mut map: FxHashMap<i64, u32> = FxHashMap::default();
            let mut codes = Vec::with_capacity(n);
            for r in 0..n {
                let x = vals.at(r)?;
                let next = map.len() as u32;
                let code = *map.entry(x).or_insert(next);
                codes.push(code);
            }
            Some((codes, map.len()))
        }
        TypedCol::U64 { vals } => {
            let mut map: FxHashMap<u64, u32> = FxHashMap::default();
            let mut codes = Vec::with_capacity(n);
            for r in 0..n {
                let x = vals.at(r)?;
                let next = map.len() as u32;
                let code = *map.entry(x).or_insert(next);
                codes.push(code);
            }
            Some((codes, map.len()))
        }
        TypedCol::Date { vals } => {
            let mut map: FxHashMap<i32, u32> = FxHashMap::default();
            let mut codes = Vec::with_capacity(n);
            for v in vals {
                let x = (*v)?;
                let next = map.len() as u32;
                let code = *map.entry(x).or_insert(next);
                codes.push(code);
            }
            Some((codes, map.len()))
        }
        TypedCol::Datetime { vals, .. } => {
            let mut map: FxHashMap<i64, u32> = FxHashMap::default();
            let mut codes = Vec::with_capacity(n);
            for v in vals {
                let x = (*v)?;
                let next = map.len() as u32;
                let code = *map.entry(x).or_insert(next);
                codes.push(code);
            }
            Some((codes, map.len()))
        }
        _ => None,
    }
}

fn dtype_supported_for_hierarchy(dt: &DataType) -> bool {
    matches!(
        dt,
        DataType::String
            | DataType::Boolean
            | DataType::Categorical(_, _)
            | DataType::Enum(_, _)
            | DataType::Date
            | DataType::Datetime(_, _)
            | DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
    )
}

/// Discover exact, nontrivial many-to-one hierarchy paths (finest to coarsest)
/// on the inspected frame.
pub fn hierarchy_candidates(
    df: &DataFrame,
    roles: &BTreeMap<String, Option<String>>,
    entity: Option<&str>,
    excluded: &BTreeSet<String>,
) -> Vec<Value> {
    let n = df.height();
    if n == 0 {
        return Vec::new();
    }
    let mut eligible: Vec<String> = df
        .columns()
        .iter()
        .filter_map(|c| {
            let name = c.name().as_str();
            if excluded.contains(name) || !dtype_supported_for_hierarchy(c.dtype()) {
                return None;
            }
            let role = roles.get(name).and_then(|r| r.as_deref());
            let is_entity = entity == Some(name);
            if role == Some("dimension") || is_entity || role == Some("identifier") {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect();
    if eligible.is_empty() {
        return Vec::new();
    }
    eligible.sort_by(|a, b| {
        let a_not_ent = entity != Some(a.as_str());
        let b_not_ent = entity != Some(b.as_str());
        let a_not_dim = roles.get(a).and_then(|r| r.as_deref()) != Some("dimension");
        let b_not_dim = roles.get(b).and_then(|r| r.as_deref()) != Some("dimension");
        a_not_ent
            .cmp(&b_not_ent)
            .then_with(|| a_not_dim.cmp(&b_not_dim))
            .then_with(|| a.cmp(b))
    });

    let table = extract(df);
    let mut col_names: Vec<String> = Vec::new();
    let mut col_codes: Vec<Vec<u32>> = Vec::new();
    let mut col_card: Vec<usize> = Vec::new();

    for name in &eligible {
        if col_names.len() >= MAX_HIERARCHY_COLUMNS {
            break;
        }
        let Some(ci) = table.names.iter().position(|t| t == name) else {
            continue;
        };
        if let Some((codes, distinct)) = dense_codes_if_complete(&table.cols[ci], n) {
            if distinct >= 2 && distinct <= n / 2 {
                col_names.push(name.clone());
                col_codes.push(codes);
                col_card.push(distinct);
            }
        }
    }

    let k = col_names.len();
    if k < 2 {
        return Vec::new();
    }

    // Exact many-to-one edges: fine -> coarse iff every distinct value of fine
    // maps to a single value of coarse, and at least two values of coarse
    // hold multiple values of fine.
    let mut edges: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); k];
    for fine in 0..k {
        let f_card = col_card[fine];
        let f_codes = &col_codes[fine];
        for coarse in 0..k {
            if col_card[coarse] >= f_card {
                continue;
            }
            let c_codes = &col_codes[coarse];
            let mut mapping = vec![u32::MAX; f_card];
            let mut functional = true;
            for r in 0..n {
                let fc = f_codes[r] as usize;
                let cc = c_codes[r];
                let slot = mapping[fc];
                if slot == u32::MAX {
                    mapping[fc] = cc;
                } else if slot != cc {
                    functional = false;
                    break;
                }
            }
            if !functional {
                continue;
            }
            let mut parent_sizes = vec![0usize; col_card[coarse]];
            for &cc in &mapping {
                if cc != u32::MAX {
                    parent_sizes[cc as usize] += 1;
                }
            }
            if parent_sizes.iter().filter(|&&s| s > 1).count() >= 2 {
                edges[fine].insert(coarse);
            }
        }
    }

    // Transitive reduction: keep only immediate parents.
    let immediate: Vec<Vec<usize>> = (0..k)
        .map(|c| {
            let mut ancestors = BTreeSet::new();
            for &p in &edges[c] {
                for &anc in &edges[p] {
                    ancestors.insert(anc);
                }
            }
            let mut imm: Vec<usize> = edges[c]
                .iter()
                .copied()
                .filter(|p| !ancestors.contains(p))
                .collect();
            imm.sort_by(|&a, &b| col_names[a].cmp(&col_names[b]));
            imm
        })
        .collect();

    let mut has_child = vec![false; k];
    for parents in &immediate {
        for &p in parents {
            has_child[p] = true;
        }
    }

    let mut stack: Vec<Vec<usize>> = (0..k)
        .rev()
        .filter(|&c| !immediate[c].is_empty() && !has_child[c])
        .map(|c| vec![c])
        .collect();
    let mut out: Vec<Value> = Vec::new();
    while let Some(chain) = stack.pop() {
        if out.len() >= MAX_HIERARCHY_CHAINS {
            break;
        }
        let tip = *chain.last().unwrap();
        if !immediate[tip].is_empty() {
            for &parent in immediate[tip].iter().rev() {
                let mut next = chain.clone();
                next.push(parent);
                stack.push(next);
            }
            continue;
        }
        let levels: Vec<&str> = chain.iter().map(|&i| col_names[i].as_str()).collect();
        let distinct_levels: Vec<usize> = chain.iter().map(|&i| col_card[i]).collect();
        out.push(json!({
            "levels": levels,
            "source": "inferred",
            "evidence": {
                "rows": n,
                "distinct_levels": distinct_levels,
            },
        }));
    }
    out
}

fn resolve_spec_inner(
    df: &DataFrame,
    hints: Option<&ProfileHints>,
    target: Option<&str>,
    discover_hierarchies: bool,
) -> Value {
    let cfg = ProfileConfig {
        budget: Budget::Quick,
        detectors: Some(
            ["keys", "duplicate_encodings"]
                .into_iter()
                .map(String::from)
                .collect(),
        ),
        hints: hints.cloned(),
        target: target.map(String::from),
        ..Default::default()
    };
    let report = profile(df, &cfg);
    let declared_roles = hints.map(|h| &h.roles);

    let mut columns_map = Map::new();
    let mut role_lookup: BTreeMap<String, Option<String>> = BTreeMap::new();
    if let Some(frame) = &report.frame {
        for r in &frame.roles {
            let source = if declared_roles.is_some_and(|m| m.contains_key(&r.column)) {
                "declared"
            } else {
                "inferred"
            };
            role_lookup.insert(r.column.clone(), r.role.clone());
            columns_map.insert(
                r.column.clone(),
                json!({
                    "role": r.role,
                    "source": source,
                    "evidence": r.evidence,
                }),
            );
        }
    }

    let (time_axis_val, axis_col_name) = match report.frame.as_ref().and_then(|f| f.axis.as_ref()) {
        Some(axis) => {
            let col = axis
                .source_column
                .as_deref()
                .unwrap_or(axis.name.as_str())
                .to_string();
            let source = if axis.source == "hint" {
                "declared"
            } else {
                "inferred"
            };
            let mut obj = Map::new();
            obj.insert("column".into(), json!(col));
            obj.insert("source".into(), json!(source));
            obj.insert("evidence".into(), json!(axis.description));
            if !axis.parts.is_empty() {
                obj.insert("parts".into(), json!(axis.parts));
            }
            (Value::Object(obj), Some(col))
        }
        None => (
            json!({
                "column": Value::Null,
                "source": "none",
                "evidence": "no time axis found",
            }),
            None,
        ),
    };

    let mut entity_col: Option<String> = None;
    let entity_val = if let Some(ent) = hints.and_then(|h| h.entity.as_deref()) {
        entity_col = Some(ent.to_string());
        json!({
            "column": ent,
            "source": "declared",
            "evidence": "declared by hints.entity",
        })
    } else {
        let mut inferred = json!({
            "column": Value::Null,
            "source": "none",
            "evidence": "no entity found",
        });
        let n = report.overview.n_rows;
        if let Some(frame) = &report.frame {
            if let Some(grain) = &frame.grain {
                let axis_name = frame.axis.as_ref().map(|a| a.name.as_str());
                let factors: Vec<_> = grain
                    .factorization
                    .iter()
                    .filter(|f| Some(f.column.as_str()) != axis_name)
                    .collect();
                if factors.len() == 1 && factors[0].distinct >= 10 {
                    let col = &factors[0].column;
                    let null_count = report.columns.get(col).map(|c| c.null_count).unwrap_or(0);
                    let nonnull = n.saturating_sub(null_count);
                    let role = role_lookup.get(col).and_then(|r| r.as_deref());
                    if matches!(role, Some("identifier" | "dimension"))
                        && 2 * factors[0].distinct <= nonnull
                    {
                        entity_col = Some(col.clone());
                        inferred = json!({
                            "column": col,
                            "source": "inferred",
                            "evidence": format!("the grain's non-axis factor: {}", grain.statement),
                        });
                    }
                }
            }
        }
        inferred
    };

    let weights_list: Vec<String> = hints.map(|h| h.weights.clone()).unwrap_or_default();
    let weights_source = if weights_list.is_empty() {
        "none"
    } else {
        "declared"
    };

    let mut spec = Map::new();
    spec.insert("spec_version".into(), json!(SPEC_VERSION));
    spec.insert("columns".into(), Value::Object(columns_map));
    spec.insert("time_axis".into(), time_axis_val);
    spec.insert("entity".into(), entity_val);
    spec.insert(
        "target".into(),
        json!({
            "column": target,
            "source": if target.is_some() { "declared" } else { "none" },
        }),
    );
    spec.insert(
        "weights".into(),
        json!({
            "columns": weights_list,
            "source": weights_source,
        }),
    );
    if discover_hierarchies {
        let mut excluded: BTreeSet<String> = hints
            .map(|h| h.exclude_columns.iter().cloned().collect())
            .unwrap_or_default();
        if let Some(t) = target {
            excluded.insert(t.to_string());
        }
        if let Some(ax) = &axis_col_name {
            excluded.insert(ax.clone());
        }
        if let Some(oa) = hints.and_then(|h| h.ordered_axis.as_deref()) {
            excluded.insert(oa.to_string());
        }
        for w in &weights_list {
            excluded.insert(w.clone());
        }
        let hc = hierarchy_candidates(df, &role_lookup, entity_col.as_deref(), &excluded);
        spec.insert("hierarchy_candidates".into(), Value::Array(hc));
    }
    Value::Object(spec)
}

/// Infer the frame's resolved interpretation without profiling it.
pub fn infer_spec(df: &DataFrame, hints: Option<&ProfileHints>, target: Option<&str>) -> Value {
    resolve_spec_inner(df, hints, target, true)
}

/// Validate a user-edited spec against `df` and translate it into minimal
/// `ProfileHints` + `target` declarations so an unedited spec reproduces
/// the plain run exactly while every edited entry is held.
pub fn apply_spec(
    df: &DataFrame,
    spec: &Value,
    hints: Option<&ProfileHints>,
    target: Option<&str>,
) -> Result<(ProfileHints, Option<String>), String> {
    let Some(spec_obj) = spec.as_object() else {
        return Err(format!(
            "spec must be a dict with spec_version {SPEC_VERSION} (from infer_spec)"
        ));
    };
    if spec_obj.get("spec_version").and_then(Value::as_u64) != Some(SPEC_VERSION) {
        return Err(format!(
            "spec must be a dict with spec_version {SPEC_VERSION} (from infer_spec)"
        ));
    }
    let names: Vec<String> = df
        .get_column_names()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let Some(cols) = spec_obj.get("columns").and_then(Value::as_object) else {
        return Err("spec['columns'] must map each column name to its entry".to_string());
    };
    let missing: Vec<&String> = names.iter().filter(|c| !cols.contains_key(*c)).collect();
    let extra: Vec<&String> = cols.keys().filter(|c| !names.contains(*c)).collect();
    if !missing.is_empty() || !extra.is_empty() {
        return Err(format!(
            "spec does not describe this frame: columns missing from the spec {missing:?}, spec columns not in the frame {extra:?}"
        ));
    }

    let mut wanted: BTreeMap<String, Option<String>> = BTreeMap::new();
    for (name, entry) in cols {
        let role = entry
            .get("role")
            .and_then(|r| if r.is_null() { None } else { r.as_str() });
        if let Some(r) = role {
            if !ROLE_VOCABULARY.contains(&r) {
                return Err(format!(
                    "spec role '{r}' for column '{name}' is not one of {:?}",
                    ROLE_VOCABULARY
                ));
            }
        }
        wanted.insert(name.clone(), role.map(String::from));
    }

    let mut out_hints = hints.cloned().unwrap_or_default();
    let mut clash: Vec<&str> = Vec::new();
    if !out_hints.roles.is_empty() {
        clash.push("roles");
    }
    if out_hints.time.is_some() {
        clash.push("time");
    }
    if out_hints.entity.is_some() {
        clash.push("entity");
    }
    if !out_hints.weights.is_empty() {
        clash.push("weights");
    }
    if !clash.is_empty() {
        return Err(format!(
            "hints {clash:?} are carried by the spec; edit the spec instead"
        ));
    }

    let spec_target = spec_obj
        .get("target")
        .and_then(|t| t.get("column"))
        .and_then(Value::as_str);
    if let (Some(st), Some(t)) = (spec_target, target) {
        if st != t {
            return Err(format!(
                "target '{t}' differs from the spec's target '{st}'"
            ));
        }
    }
    let final_target = spec_target.or(target).map(String::from);

    let axis = spec_obj
        .get("time_axis")
        .and_then(|t| t.get("column"))
        .and_then(Value::as_str);
    let has_parts = spec_obj
        .get("time_axis")
        .and_then(|t| t.get("parts"))
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty());
    let entity = spec_obj
        .get("entity")
        .and_then(|e| e.get("column"))
        .and_then(Value::as_str);

    if let Some(col) = axis {
        if !names.iter().any(|n| n == col) && !has_parts {
            return Err(format!(
                "spec time_axis column '{col}' is not a column of the frame"
            ));
        }
    }
    if let Some(col) = entity {
        if !names.iter().any(|n| n == col) {
            return Err(format!(
                "spec entity column '{col}' is not a column of the frame"
            ));
        }
    }

    if let Some(weights) = spec_obj
        .get("weights")
        .and_then(|w| w.get("columns"))
        .and_then(Value::as_array)
    {
        let ws: Vec<String> = weights
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect();
        if !ws.is_empty() {
            out_hints.weights = ws;
        }
    }
    if let Some(ent) = entity {
        out_hints.entity = Some(ent.to_string());
    }

    let mut declared_roles: BTreeMap<String, String> = BTreeMap::new();
    let mut last_differ: Vec<String> = Vec::new();
    for _ in 0..4 {
        let mut trial = out_hints.clone();
        if !declared_roles.is_empty() {
            trial.roles = declared_roles.clone();
        }
        let resolved = resolve_spec_inner(df, Some(&trial), final_target.as_deref(), false);
        let resolved_axis = resolved
            .get("time_axis")
            .and_then(|t| t.get("column"))
            .and_then(Value::as_str);
        if axis != resolved_axis && out_hints.time.is_none() {
            match axis {
                None => {
                    return Err(
                        "the spec states no time axis, but the frame has one the run would use; \
                         declare the axis column's role instead of removing the axis"
                            .to_string(),
                    );
                }
                Some(ax) => {
                    out_hints.time = Some(ax.to_string());
                    continue;
                }
            }
        }
        let mut differ: BTreeMap<String, String> = BTreeMap::new();
        for (c, wanted_role) in &wanted {
            if let Some(wr) = wanted_role {
                let got = resolved
                    .get("columns")
                    .and_then(|m| m.get(c))
                    .and_then(|e| e.get("role"))
                    .and_then(Value::as_str);
                if got != Some(wr.as_str()) {
                    differ.insert(c.clone(), wr.clone());
                }
            }
        }
        if differ.is_empty() {
            if !declared_roles.is_empty() {
                out_hints.roles = declared_roles;
            }
            return Ok((out_hints, final_target));
        }
        last_differ = differ.keys().cloned().collect();
        declared_roles.extend(differ);
    }
    Err(format!(
        "the spec's roles cannot all be held together: {last_differ:?} still resolve differently"
    ))
}
