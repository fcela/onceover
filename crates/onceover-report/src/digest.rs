//! Context-budgeted plain-text briefing digest and token-compact LLM brief.
//!
//! Operates on the versioned JSON representation (`serde_json::Value`) so both
//! native Rust `ProfileReport` values and JSON-roundtripped reports (from Python,
//! R, or CLI callers) share a single implementation.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const SECTION_ORDER: [&str; 7] = [
    "row",
    "wrong",
    "redundant",
    "target",
    "relationships",
    "anomalies",
    "not_examined",
];

const BODY_SECTIONS: [&str; 5] = ["wrong", "redundant", "target", "relationships", "anomalies"];

const EXACT_TYPES: [&str; 4] = [
    "functional",
    "arithmetic_identity",
    "affine",
    "duplicate_encoding",
];

fn section_title(name: &str, designated: Option<&str>) -> String {
    match name {
        "row" => "What a row is".to_string(),
        "wrong" => "What is wrong".to_string(),
        "redundant" => "What is redundant".to_string(),
        "target" => match designated {
            Some(t) => format!("What relates to the target ({t})"),
            None => "What relates to the target".to_string(),
        },
        "relationships" => "Other relationships".to_string(),
        "anomalies" => "Anomalies".to_string(),
        "not_examined" => "Not examined".to_string(),
        other => other.to_string(),
    }
}

fn section_share(name: &str, has_target: bool) -> f64 {
    match name {
        "wrong" => 0.24,
        "redundant" => 0.13,
        "target" => {
            if has_target {
                0.26
            } else {
                0.0
            }
        }
        "relationships" => {
            if has_target {
                0.24
            } else {
                0.50
            }
        }
        "anomalies" => 0.13,
        _ => 0.0,
    }
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn f64_field(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn format_commas_int(n: i64) -> String {
    let neg = n < 0;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + usize::from(neg));
    if neg {
        out.push('-');
    }
    let len = digits.len();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn format_float_commas(v: f64, places: i32) -> String {
    let neg = v < 0.0;
    let abs = v.abs();
    let s = if places > 0 {
        let raw = format!("{abs:.prec$}", prec = places as usize);
        let trimmed = raw.trim_end_matches('0').trim_end_matches('.');
        trimmed.to_string()
    } else {
        format!("{abs:.0}")
    };
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (s.as_str(), None),
    };
    let mut out = String::with_capacity(s.len() + int_part.len() / 3 + usize::from(neg));
    if neg {
        out.push('-');
    }
    let len = int_part.len();
    for (i, ch) in int_part.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    if let Some(f) = frac_part {
        if !f.is_empty() {
            out.push('.');
            out.push_str(f);
        }
    }
    out
}

fn format_sci_2(v: f64) -> String {
    let raw = format!("{v:.2e}");
    if let Some((mant, exp)) = raw.split_once('e') {
        let (sign, digits) = if let Some(rest) = exp.strip_prefix('-') {
            ('-', rest)
        } else if let Some(rest) = exp.strip_prefix('+') {
            ('+', rest)
        } else {
            ('+', exp)
        };
        if digits.len() < 2 {
            return format!("{mant}e{sign}0{digits}");
        }
        return format!("{mant}e{sign}{digits}");
    }
    raw
}

fn round_sig3(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    if value == 0.0 {
        return "0".to_string();
    }
    let places = 2 - value.abs().log10().floor() as i32;
    let factor = 10f64.powi(places);
    let scaled = value * factor;
    let rounded = if scaled.is_finite() {
        scaled.round_ties_even() / factor
    } else {
        value
    };
    if rounded.fract() == 0.0 && rounded.abs() < 1e15 {
        let whole = rounded as i64;
        if whole.abs() >= 10_000 {
            format_commas_int(whole)
        } else {
            whole.to_string()
        }
    } else if rounded.abs() < 0.001 {
        format_sci_2(rounded)
    } else {
        format_float_commas(rounded, places.max(0))
    }
}

/// Round decimal numbers (`\d+\.\d+(?:[eE][+-]?\d+)?`) in a claim to
/// three significant figures, matching the Python digest formatter.
pub fn digest_numbers(text: &str) -> String {
    let bytes = text.as_bytes();
    let n = bytes.len();
    let mut out = String::with_capacity(n);
    let mut i = 0usize;
    while i < n {
        if bytes[i].is_ascii_digit() {
            let start = i;
            let mut j = i;
            while j < n && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j + 1 < n && bytes[j] == b'.' && bytes[j + 1].is_ascii_digit() {
                j += 1;
                while j < n && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                // Optional exponent `[eE][+-]?\d+`.
                let exp_start = j;
                if j < n && (bytes[j] == b'e' || bytes[j] == b'E') {
                    let mut k = j + 1;
                    if k < n && (bytes[k] == b'+' || bytes[k] == b'-') {
                        k += 1;
                    }
                    let d_start = k;
                    while k < n && bytes[k].is_ascii_digit() {
                        k += 1;
                    }
                    if k > d_start {
                        j = k;
                    } else {
                        j = exp_start;
                    }
                }
                let token = &text[start..j];
                if let Ok(val) = token.parse::<f64>() {
                    out.push_str(&round_sig3(val));
                } else {
                    out.push_str(token);
                }
                i = j;
                continue;
            }
        }
        let ch = text[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

pub fn quality_line(data: &Value) -> Option<String> {
    let columns = data.get("columns").and_then(Value::as_object);
    let mut gaps: Vec<(f64, &str)> = Vec::new();
    let mut empty = 0usize;
    let mut constant = 0usize;
    if let Some(cols) = columns {
        for (name, card) in cols {
            let nf = f64_field(card, "null_fraction");
            if nf > 0.0 && nf < 1.0 {
                gaps.push((nf, name.as_str()));
            }
            match str_field(card, "card_role") {
                Some("all_null") => empty += 1,
                Some("constant") => constant += 1,
                _ => {}
            }
        }
    }
    let mut parts: Vec<String> = Vec::new();
    if !gaps.is_empty() {
        let &(rate, name) = gaps
            .iter()
            .max_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(b.1)))
            .unwrap();
        parts.push(format!(
            "missing values in {} column(s), most in {name} ({:.1}%)",
            gaps.len(),
            rate * 100.0
        ));
    }
    if empty > 0 {
        parts.push(format!("{empty} empty column(s)"));
    }
    if constant > 0 {
        parts.push(format!("{constant} constant column(s)"));
    }
    if let Some(findings) = data.get("findings").and_then(Value::as_array) {
        for f in findings {
            if str_field(f, "type") == Some("duplicate_rows") {
                if let Some(claim) = str_field(f, "claim") {
                    if let Some((prefix, _)) = claim.split_once(" rows repeat") {
                        if !prefix.is_empty() && prefix.bytes().all(|b| b.is_ascii_digit()) {
                            parts.push(format!("{prefix} duplicate row(s)"));
                        }
                    }
                }
                break;
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("quality: {}", parts.join("; ")))
    }
}

fn digest_columns(finding: &Value) -> Vec<String> {
    let subj = finding.get("subject");
    let mut cols: Vec<String> = subj
        .and_then(|s| s.get("columns"))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    if let Some(first_ev) = finding
        .get("evidence")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
    {
        if let Some(drivers) = first_ev
            .get("metrics")
            .and_then(|m| m.get("drivers"))
            .and_then(Value::as_array)
        {
            if !drivers.is_empty() && drivers.iter().all(Value::is_string) {
                cols = drivers
                    .iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect();
            }
        }
    }
    if let Some(target) = subj.and_then(|s| str_field(s, "target")) {
        if !target.is_empty() && !cols.iter().any(|c| c == target) {
            cols.push(target.to_string());
        }
    }
    cols
}

fn fact_precedence(finding: &Value) -> u8 {
    match str_field(finding, "type") {
        Some("leakage_signature") => 0,
        Some(t) if EXACT_TYPES.contains(&t) => 1,
        Some("order_constraint" | "order_violation") => 2,
        Some("if_then" | "outcome_distribution" | "outcome_contrast") => 3,
        _ => 4,
    }
}

fn is_weak_association(finding: &Value) -> bool {
    if str_field(finding, "type") != Some("statistical") {
        return false;
    }
    finding
        .get("evidence")
        .and_then(Value::as_array)
        .is_some_and(|evs| {
            evs.iter()
                .any(|ev| ev.get("metrics").and_then(|m| str_field(m, "tier")) == Some("weak"))
        })
}

struct Fact<'a> {
    columns: BTreeSet<String>,
    rep: &'a Value,
}

fn digest_facts<'a>(findings: &'a [Value]) -> (Vec<Fact<'a>>, Vec<&'a Value>) {
    let mut weak: Vec<(&'a Value, Vec<String>)> = Vec::new();
    let mut by_key: BTreeMap<Vec<String>, usize> = BTreeMap::new();
    let mut facts: Vec<Fact<'a>> = Vec::new();

    for f in findings {
        let cols = digest_columns(f);
        if is_weak_association(f) {
            weak.push((f, cols));
            continue;
        }
        let mut key: Vec<String> = if cols.is_empty() {
            vec![
                String::new(),
                str_field(f, "id").unwrap_or_default().to_string(),
            ]
        } else {
            let mut k = cols.clone();
            k.sort();
            k
        };
        if str_field(f, "type") == Some("outcome_contrast") {
            if let Some(first_ev) = f
                .get("evidence")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
            {
                if let Some(grouping) = first_ev
                    .get("metrics")
                    .and_then(|m| str_field(m, "grouping_column"))
                {
                    if !grouping.is_empty() && !cols.iter().any(|c| c == grouping) {
                        key.push(format!("calendar:{grouping}"));
                    }
                }
            }
        }
        if let Some(&slot) = by_key.get(&key) {
            let cur = facts[slot].rep;
            let new_key = (fact_precedence(f), -f64_field(f, "interestingness"));
            let cur_key = (fact_precedence(cur), -f64_field(cur, "interestingness"));
            if new_key
                .0
                .cmp(&cur_key.0)
                .then_with(|| new_key.1.total_cmp(&cur_key.1))
                .is_lt()
            {
                facts[slot].rep = f;
            }
        } else {
            let slot = facts.len();
            by_key.insert(key, slot);
            facts.push(Fact {
                columns: cols.into_iter().collect(),
                rep: f,
            });
        }
    }

    let mut remaining_weak: Vec<&'a Value> = Vec::new();
    for (f, mut cols) in weak {
        let covered = if cols.is_empty() {
            false
        } else {
            cols.sort();
            by_key.contains_key(&cols)
        };
        if !covered {
            remaining_weak.push(f);
        }
    }
    (facts, remaining_weak)
}

fn designated_target(findings: &[Value]) -> Option<&str> {
    for f in findings {
        if matches!(
            str_field(f, "type"),
            Some(
                "leakage_signature" | "model_failure" | "outcome_distribution" | "outcome_contrast"
            )
        ) {
            if let Some(t) = f.get("subject").and_then(|s| str_field(s, "target")) {
                if !t.is_empty() {
                    return Some(t);
                }
            }
        }
    }
    None
}

fn is_dirty_key(rep: &Value) -> bool {
    if str_field(rep, "category") != Some("uniqueness") {
        return false;
    }
    if rep.get("specification").and_then(|s| str_field(s, "form")) == Some("near_unique") {
        return true;
    }
    str_field(rep, "claim")
        .unwrap_or_default()
        .contains("nearly unique")
}

fn fact_section(rep: &Value, columns: &BTreeSet<String>, designated: Option<&str>) -> &'static str {
    let typ = str_field(rep, "type").unwrap_or_default();
    let cat = str_field(rep, "category").unwrap_or_default();
    let target = rep.get("subject").and_then(|s| str_field(s, "target"));
    if typ == "leakage_signature" {
        return "target";
    }
    if matches!(cat, "missingness" | "duplicates" | "formats" | "gaps") {
        return "wrong";
    }
    if is_dirty_key(rep) {
        return "wrong";
    }
    if matches!(typ, "sentinel_values" | "boundary_mass") {
        return "wrong";
    }
    if let Some(d) = designated {
        if target == Some(d) {
            return "target";
        }
    }
    if cat == "concentration" && columns.len() <= 1 {
        return "wrong";
    }
    if EXACT_TYPES.contains(&typ) {
        return "redundant";
    }
    if let Some(d) = designated {
        if target.is_none()
            && columns.contains(d)
            && matches!(
                cat,
                "dependencies" | "associations" | "conditional_rules" | "changes"
            )
        {
            return "target";
        }
    }
    if cat == "anomalies" {
        return "anomalies";
    }
    if cat == "concentration"
        && matches!(typ, "outlier_entity" | "spike_cells" | "entity_specialists")
    {
        return "anomalies";
    }
    "relationships"
}

fn row_lines(frame: Option<&Value>) -> Vec<String> {
    let Some(frame) = frame else {
        return Vec::new();
    };
    let mut lines: Vec<String> = Vec::new();
    if let Some(stmt) = frame.get("grain").and_then(|g| str_field(g, "statement")) {
        if !stmt.is_empty() {
            lines.push(stmt.to_string());
        }
    }
    if let Some(axis) = frame.get("axis").and_then(Value::as_object) {
        if !axis.is_empty() {
            let desc = axis
                .get("description")
                .and_then(Value::as_str)
                .or_else(|| axis.get("name").and_then(Value::as_str))
                .unwrap_or_default();
            let mut line = format!("time axis \u{2014} {desc}");
            if let (Some(min), Some(max)) = (
                axis.get("min").and_then(Value::as_str),
                axis.get("max").and_then(Value::as_str),
            ) {
                if !min.is_empty() && !max.is_empty() {
                    line.push_str(&format!(", {min} to {max}"));
                    if let Some(step) = axis.get("step").and_then(Value::as_str) {
                        if !step.is_empty() {
                            line.push_str(&format!(", in {step} steps"));
                        }
                    }
                    if let Some(dv) = axis.get("distinct_values").and_then(Value::as_u64) {
                        if dv > 0 {
                            line.push_str(&format!(" ({dv} distinct values)"));
                        }
                    }
                }
            }
            lines.push(line);
        }
    }
    if let Some(purpose) = frame
        .get("declarations")
        .and_then(|d| str_field(d, "purpose"))
    {
        if !purpose.is_empty() {
            lines.push(format!("declared purpose \u{2014} {purpose}"));
        }
    }
    lines.truncate(3);
    lines
}

fn not_examined_line(data: &Value, on_request_detectors: &[&str]) -> Option<String> {
    let cov = data.get("coverage").and_then(Value::as_object)?;
    let mut capped: Vec<&str> = Vec::new();
    let mut errored: Vec<&str> = Vec::new();
    for (key, entry) in cov {
        if key == "selection" || key == "deadline" {
            continue;
        }
        if on_request_detectors.contains(&key.as_str()) {
            continue;
        }
        match str_field(entry, "status") {
            Some("capped") => capped.push(key.as_str()),
            Some("error") => errored.push(key.as_str()),
            _ => {}
        }
    }
    capped.sort_unstable();
    errored.sort_unstable();
    let mut bits: Vec<String> = Vec::new();
    if !capped.is_empty() {
        bits.push(format!("capped: {}", capped.join(", ")));
    }
    if !errored.is_empty() {
        bits.push(format!("errored: {}", errored.join(", ")));
    }
    if cov.contains_key("deadline") {
        bits.push("the run was cut short by its deadline".to_string());
    }
    if bits.is_empty() {
        None
    } else {
        Some(bits.join("; "))
    }
}

fn fmt_compact_num(x: Option<f64>) -> String {
    let Some(x) = x else {
        return "?".to_string();
    };
    if !x.is_finite() {
        return "?".to_string();
    }
    if x == 0.0 {
        return "0".to_string();
    }
    if x.abs() >= 1e6 || (x.abs() > 0.0 && x.abs() < 1e-3) {
        return format!("{x:.2e}");
    }
    if x.fract() == 0.0 && x.abs() < 1e6 {
        return format!("{}", x as i64);
    }
    let places = (2 - x.abs().log10().floor() as i32).max(0);
    let factor = 10f64.powi(places);
    let rounded = (x * factor).round_ties_even() / factor;
    if rounded.fract() == 0.0 && rounded.abs() < 1e15 {
        format!("{}", rounded as i64)
    } else {
        let s = format!("{rounded:.prec$}", prec = places as usize);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn schema_summary_lines(data: &Value, max_cols: usize) -> Vec<String> {
    let Some(columns) = data.get("columns").and_then(Value::as_object) else {
        return Vec::new();
    };
    if columns.is_empty() {
        return Vec::new();
    }
    let mut id_g = Vec::new();
    let mut time_g = Vec::new();
    let mut dim_g = Vec::new();
    let mut flag_g = Vec::new();
    let mut num_g = Vec::new();
    let mut text_g = Vec::new();
    let mut other_g = Vec::new();

    for (name, card) in columns.iter().take(max_cols) {
        let role = str_field(card, "semantic_role")
            .or_else(|| str_field(card, "card_role"))
            .unwrap_or("other");
        let null_pct = f64_field(card, "null_fraction");
        let null_tag = if null_pct >= 0.01 {
            format!(",{:.0}%null", null_pct * 100.0)
        } else {
            String::new()
        };
        let distinct = card.get("distinct").and_then(Value::as_u64).unwrap_or(0);
        match role {
            "identifier" | "row_number" | "unique" => {
                id_g.push(format!("{name}({distinct}{null_tag})"));
            }
            "timestamp" | "timestamp_part" | "date" | "datetime" => {
                let ts = card.get("temporal_stats");
                let span = match (
                    ts.and_then(|t| str_field(t, "min")),
                    ts.and_then(|t| str_field(t, "max")),
                ) {
                    (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => format!("{a}..{b}"),
                    _ => distinct.to_string(),
                };
                time_g.push(format!("{name}[{span}{null_tag}]"));
            }
            "flag" | "boolean" => {
                if null_tag.is_empty() {
                    flag_g.push(name.clone());
                } else {
                    flag_g.push(format!("{name}({})", &null_tag[1..]));
                }
            }
            "dimension" | "categorical" => {
                dim_g.push(format!("{name}({distinct}{null_tag})"));
            }
            "measure" | "continuous" => {
                let ns = card.get("numeric_stats");
                let min = fmt_compact_num(ns.and_then(|s| s.get("min")).and_then(Value::as_f64));
                let max = fmt_compact_num(ns.and_then(|s| s.get("max")).and_then(Value::as_f64));
                let med = fmt_compact_num(ns.and_then(|s| s.get("q50")).and_then(Value::as_f64));
                num_g.push(format!("{name}[{min}..{max},med={med}{null_tag}]"));
            }
            "text" | "free_text" => {
                text_g.push(format!("{name}({distinct}{null_tag})"));
            }
            _ => {
                other_g.push(format!("{name}({role}{null_tag})"));
            }
        }
    }

    let mut parts: Vec<String> = Vec::new();
    for (label, items) in [
        ("id", id_g),
        ("time", time_g),
        ("dim", dim_g),
        ("flag", flag_g),
        ("num", num_g),
        ("text", text_g),
        ("other", other_g),
    ] {
        if !items.is_empty() {
            parts.push(format!("{label}: {}", items.join(", ")));
        }
    }
    if columns.len() > max_cols {
        parts.push(format!("+{} more cols", columns.len() - max_cols));
    }
    if parts.is_empty() {
        Vec::new()
    } else {
        vec![format!("Schema: {}", parts.join(" | "))]
    }
}

fn matches_grain_statement(claim: &str, grain_statement: Option<&str>) -> bool {
    let Some(gs) = grain_statement else {
        return false;
    };
    if gs.is_empty() || claim.is_empty() {
        return false;
    }
    claim == gs || claim.starts_with(&format!("{gs} ("))
}

#[derive(Clone)]
struct Entry<'a> {
    id: usize,
    text: String,
    rep: Option<&'a Value>,
}

fn score_cmp(a: &Entry<'_>, b: &Entry<'_>) -> std::cmp::Ordering {
    let sa = a
        .rep
        .map(|r| f64_field(r, "interestingness"))
        .unwrap_or(0.0);
    let sb = b
        .rep
        .map(|r| f64_field(r, "interestingness"))
        .unwrap_or(0.0);
    sb.total_cmp(&sa).then_with(|| a.text.cmp(&b.text))
}

fn bucket_key(rep: &Value) -> (String, String, String) {
    let cat = str_field(rep, "category").unwrap_or_default().to_string();
    let typ = str_field(rep, "type").unwrap_or_default().to_string();
    if typ == "level_contrast" {
        if let Some(evs) = rep.get("evidence").and_then(Value::as_array) {
            for ev in evs {
                if let Some(m) = ev
                    .get("metrics")
                    .and_then(|m| str_field(m, "measure_column"))
                {
                    if !m.is_empty() {
                        return (cat, typ, m.to_string());
                    }
                }
            }
        }
    }
    if typ == "outcome_contrast" {
        if let Some(evs) = rep.get("evidence").and_then(Value::as_array) {
            for ev in evs {
                if let Some(g) = ev
                    .get("metrics")
                    .and_then(|m| str_field(m, "grouping_column"))
                {
                    if !g.is_empty() {
                        return (cat, typ, g.to_string());
                    }
                }
            }
        }
    }
    (cat, typ, String::new())
}

fn contrast_form(rep: &Value) -> Option<&str> {
    rep.get("evidence")
        .and_then(Value::as_array)?
        .iter()
        .find_map(|ev| ev.get("metrics").and_then(|m| str_field(m, "form")))
}

fn relationship_sequence<'a>(
    entries: Vec<Entry<'a>>,
    allowance: usize,
    tag_of: &dyn Fn(&Value) -> String,
) -> Vec<Entry<'a>> {
    let mut leakage: Vec<Entry<'a>> = entries
        .iter()
        .filter(|e| e.rep.and_then(|r| str_field(r, "type")) == Some("leakage_signature"))
        .cloned()
        .collect();
    leakage.sort_by(score_cmp);

    let mut directory: Vec<Entry<'a>> = entries
        .iter()
        .filter(|e| e.rep.and_then(|r| str_field(r, "type")) == Some("predictive"))
        .cloned()
        .collect();
    directory.sort_by(score_cmp);

    let mut outcome_account: Vec<Entry<'a>> = entries
        .iter()
        .filter(|e| e.rep.and_then(|r| str_field(r, "type")) == Some("outcome_distribution"))
        .cloned()
        .collect();
    outcome_account.sort_by(score_cmp);

    let mut bucket_order: Vec<(String, String, String)> = Vec::new();
    let mut buckets: BTreeMap<(String, String, String), Vec<Entry<'a>>> = BTreeMap::new();
    for e in &entries {
        if let Some(r) = e.rep {
            if matches!(
                str_field(r, "type"),
                Some("leakage_signature" | "predictive" | "outcome_distribution")
            ) {
                continue;
            }
            let key = bucket_key(r);
            if !buckets.contains_key(&key) {
                bucket_order.push(key.clone());
            }
            buckets.entry(key).or_default().push(e.clone());
        }
    }
    for b in buckets.values_mut() {
        b.sort_by(score_cmp);
    }

    let lc_keys: BTreeSet<(String, String, String)> = bucket_order
        .iter()
        .filter(|k| {
            buckets
                .get(*k)
                .and_then(|b| b.first())
                .and_then(|e| e.rep)
                .and_then(|r| str_field(r, "type"))
                == Some("level_contrast")
        })
        .cloned()
        .collect();
    let mut firsts: Vec<Entry<'a>> = lc_keys.iter().map(|k| buckets[k][0].clone()).collect();
    firsts.sort_by(|a, b| {
        let fa = usize::from(a.rep.and_then(contrast_form) == Some("scale"));
        let fb = usize::from(b.rep.and_then(contrast_form) == Some("scale"));
        fa.cmp(&fb).then_with(|| score_cmp(a, b))
    });
    let first_ids: BTreeSet<usize> = firsts.iter().map(|e| e.id).collect();

    let oc_keys: BTreeSet<(String, String, String)> = bucket_order
        .iter()
        .filter(|k| {
            buckets
                .get(*k)
                .and_then(|b| b.first())
                .and_then(|e| e.rep)
                .and_then(|r| str_field(r, "type"))
                == Some("outcome_contrast")
        })
        .cloned()
        .collect();
    let mut outcome_firsts: Vec<Entry<'a>> =
        oc_keys.iter().map(|k| buckets[k][0].clone()).collect();
    outcome_firsts.sort_by(score_cmp);
    let outcome_first_ids: BTreeSet<usize> = outcome_firsts.iter().map(|e| e.id).collect();

    let round_lead = |e: &Entry<'a>| -> u8 {
        if let Some(r) = e.rep {
            if str_field(r, "category") == Some("changes") {
                return 0;
            }
            if first_ids.contains(&e.id)
                || outcome_first_ids.contains(&e.id)
                || str_field(r, "type") == Some("model_failure")
            {
                return 1;
            }
        }
        2
    };

    let rounds_over = |bmap: &BTreeMap<(String, String, String), Vec<Entry<'a>>>,
                       keys: &[(String, String, String)],
                       first_depth: &BTreeMap<(String, String, String), usize>|
     -> Vec<Entry<'a>> {
        let mut out = Vec::new();
        let mut depth = 0usize;
        loop {
            let mut round_entries: Vec<Entry<'a>> = Vec::new();
            for k in keys {
                let fd = first_depth.get(k).copied().unwrap_or(0);
                if let Some(b) = bmap.get(k) {
                    if b.len() > fd + depth {
                        round_entries.push(b[fd + depth].clone());
                    }
                }
            }
            if round_entries.is_empty() {
                return out;
            }
            round_entries.sort_by(|a, b| {
                round_lead(a)
                    .cmp(&round_lead(b))
                    .then_with(|| score_cmp(a, b))
            });
            out.extend(round_entries);
            depth += 1;
        }
    };

    let dir_chars: usize = directory
        .iter()
        .map(|e| {
            let tag_len = e.rep.map(tag_of).map(|t| t.len()).unwrap_or(0);
            e.text.len() + tag_len + 4
        })
        .sum();

    if dir_chars <= allowance {
        let changes_keys: Vec<(String, String, String)> = bucket_order
            .iter()
            .filter(|k| {
                buckets
                    .get(*k)
                    .and_then(|b| b.first())
                    .and_then(|e| e.rep)
                    .and_then(|r| str_field(r, "category"))
                    == Some("changes")
            })
            .cloned()
            .collect();
        let zero_depth: BTreeMap<_, _> = changes_keys.iter().map(|k| (k.clone(), 0)).collect();
        let changes_queue = rounds_over(&buckets, &changes_keys, &zero_depth);

        let hotspot_keys: Vec<(String, String, String)> = bucket_order
            .iter()
            .filter(|k| {
                buckets
                    .get(*k)
                    .and_then(|b| b.first())
                    .and_then(|e| e.rep)
                    .and_then(|r| str_field(r, "type"))
                    == Some("hotspot")
            })
            .cloned()
            .collect();
        let hotspot_lead = hotspot_keys.first().map(|k| buckets[k][0].clone());

        let lead_outcome: Vec<Entry<'a>> = outcome_firsts.iter().take(2).cloned().collect();
        let lead_firsts: Vec<Entry<'a>> = if outcome_firsts.is_empty() {
            firsts.iter().take(2).cloned().collect()
        } else {
            firsts.iter().take(1).cloned().collect()
        };
        let lead_ids: BTreeSet<usize> = lead_outcome
            .iter()
            .chain(lead_firsts.iter())
            .map(|e| e.id)
            .collect();

        let mut lead: Vec<Entry<'a>> = Vec::new();
        let mut hotspot_placed = false;
        let max_len = changes_queue
            .len()
            .max(lead_firsts.len())
            .max(lead_outcome.len());
        for i in 0..max_len {
            if i < changes_queue.len() {
                lead.push(changes_queue[i].clone());
            }
            if i < lead_outcome.len() {
                lead.push(lead_outcome[i].clone());
            }
            if i < lead_firsts.len() {
                lead.push(lead_firsts[i].clone());
            }
            if i == 1 {
                if let Some(hl) = &hotspot_lead {
                    lead.push(hl.clone());
                    hotspot_placed = true;
                }
            }
        }
        if let Some(hl) = hotspot_lead {
            if !hotspot_placed {
                lead.push(hl);
            }
        }

        let changes_set: BTreeSet<_> = changes_keys.into_iter().collect();
        let hotspot_set: BTreeSet<_> = hotspot_keys.into_iter().collect();
        let rest_keys: Vec<(String, String, String)> = bucket_order
            .iter()
            .filter(|k| !changes_set.contains(*k))
            .cloned()
            .collect();
        let rest_depth: BTreeMap<_, _> = rest_keys
            .iter()
            .map(|k| {
                let consumed = ((lc_keys.contains(k) || oc_keys.contains(k))
                    && lead_ids.contains(&buckets[k][0].id))
                    || hotspot_set.contains(k);
                (k.clone(), usize::from(consumed))
            })
            .collect();
        let rest = rounds_over(&buckets, &rest_keys, &rest_depth);

        let mut out = leakage;
        out.extend(outcome_account);
        out.extend(directory);
        out.extend(lead);
        out.extend(rest);
        return out;
    }

    let mut contrast_rest: Vec<Entry<'a>> = Vec::new();
    for k in &bucket_order {
        if lc_keys.contains(k) {
            for e in &buckets[k] {
                if !first_ids.contains(&e.id) {
                    contrast_rest.push(e.clone());
                }
            }
        }
    }
    contrast_rest.sort_by(score_cmp);
    let mut contrast_queue = firsts;
    contrast_queue.extend(contrast_rest);

    let mut tail_order: Vec<(String, String, String)> = Vec::new();
    let mut tail: BTreeMap<(String, String, String), Vec<Entry<'a>>> = BTreeMap::new();
    for k in &bucket_order {
        if !lc_keys.contains(k) {
            tail_order.push(k.clone());
            tail.insert(k.clone(), buckets[k].clone());
        }
    }
    if !contrast_queue.is_empty() {
        let cq_key = (
            "associations".to_string(),
            "level_contrast".to_string(),
            String::new(),
        );
        if !tail.contains_key(&cq_key) {
            tail_order.push(cq_key.clone());
        }
        tail.insert(cq_key, contrast_queue);
    }

    let mut first_round: Vec<Entry<'a>> = tail_order
        .iter()
        .filter_map(|k| tail.get(k).and_then(|b| b.first()).cloned())
        .collect();
    if let Some(d0) = directory.first() {
        first_round.push(d0.clone());
    }
    first_round.sort_by(|a, b| {
        round_lead(a)
            .cmp(&round_lead(b))
            .then_with(|| score_cmp(a, b))
    });
    let ones: BTreeMap<_, _> = tail_order.iter().map(|k| (k.clone(), 1)).collect();
    let rest = rounds_over(&tail, &tail_order, &ones);

    let mut out = leakage;
    out.extend(outcome_account);
    out.extend(first_round);
    if directory.len() > 1 {
        out.extend(directory.into_iter().skip(1));
    }
    out.extend(rest);
    out
}

/// Render a context-budgeted plain-text digest from a report JSON value.
///
/// `on_request_detectors` lists detector IDs whose `skipped` coverage
/// entries should not appear in "Not examined".
pub fn render_digest_value(
    data: &Value,
    budget_chars: usize,
    compact: bool,
    include_schema: bool,
    on_request_detectors: &[&str],
) -> String {
    let findings = data
        .get("findings")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let overview = data.get("overview");
    let handle_of: BTreeMap<&str, String> = findings
        .iter()
        .enumerate()
        .filter_map(|(i, f)| str_field(f, "id").map(|id| (id, format!("f{}", i + 1))))
        .collect();

    let tag_of = |rep: &Value| -> String {
        let fid = str_field(rep, "id").unwrap_or_default();
        if compact {
            handle_of
                .get(fid)
                .cloned()
                .unwrap_or_else(|| fid.to_string())
        } else {
            fid.to_string()
        }
    };

    let n_rows = overview
        .and_then(|o| o.get("n_rows"))
        .map(|v| match v {
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            _ => "?".to_string(),
        })
        .unwrap_or_else(|| "?".to_string());
    let n_cols = overview
        .and_then(|o| o.get("n_columns"))
        .map(|v| match v {
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            _ => "?".to_string(),
        })
        .unwrap_or_else(|| "?".to_string());
    let schema_ver = str_field(data, "schema_version").unwrap_or("?");
    let profile_line = format!(
        "profile digest: {n_rows} rows x {n_cols} columns, {} findings, schema {schema_ver}",
        findings.len()
    );

    let mut fixed: Vec<String> = vec![profile_line.clone()];
    if let Some(source) = data.get("source").and_then(Value::as_object) {
        if source.get("basis").and_then(Value::as_str) == Some("sampled") {
            let kind = source.get("kind").and_then(Value::as_str).unwrap_or("None");
            let rows = source
                .get("rows")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "None".to_string());
            let rows_prof = source
                .get("rows_profiled")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "None".to_string());
            fixed.push(format!(
                "source: {kind}, {rows} rows; profiled on a stratified sample of {rows_prof} rows \u{2014} findings describe the sample"
            ));
        }
    }
    if include_schema {
        fixed.extend(schema_summary_lines(data, 32));
    }

    let mut r_lines = row_lines(data.get("frame"));
    let not_ex = not_examined_line(data, on_request_detectors);
    let designated = designated_target(findings);
    let (facts, weak) = digest_facts(findings);
    let grain_stmt = data
        .get("frame")
        .and_then(|f| f.get("grain"))
        .and_then(|g| str_field(g, "statement"));

    let mut next_entry_id = 0usize;
    let mut sections: BTreeMap<&'static str, Vec<Entry<'_>>> = BTreeMap::new();
    let mut grain_tag: Option<String> = None;
    for fact in &facts {
        let claim = str_field(fact.rep, "claim").unwrap_or_default();
        if matches_grain_statement(claim, grain_stmt) {
            if grain_tag.is_none() {
                grain_tag = Some(tag_of(fact.rep));
            }
            continue;
        }
        let sec = fact_section(fact.rep, &fact.columns, designated);
        sections.entry(sec).or_default().push(Entry {
            id: next_entry_id,
            text: digest_numbers(claim),
            rep: Some(fact.rep),
        });
        next_entry_id += 1;
    }
    if let (Some(tag), Some(gs)) = (grain_tag, grain_stmt) {
        if let Some(first) = r_lines.first_mut() {
            if first == gs {
                first.push_str(&format!(" [{tag}]"));
            }
        }
    }

    for entries in sections.values_mut() {
        entries.sort_by(score_cmp);
    }

    if !weak.is_empty() {
        let mut weak_sorted = weak;
        weak_sorted.sort_by(|a, b| {
            f64_field(b, "interestingness").total_cmp(&f64_field(a, "interestingness"))
        });
        let examples: Vec<String> = weak_sorted
            .iter()
            .take(3)
            .map(|f| digest_numbers(str_field(f, "claim").unwrap_or_default()))
            .collect();
        let line = format!(
            "{} tested association(s) below the strong effect bar \u{2014} strongest: {} [full table: report.associations()]",
            weak_sorted.len(),
            examples.join("; ")
        );
        sections.entry("relationships").or_default().push(Entry {
            id: next_entry_id,
            text: line,
            rep: Some(weak_sorted[0]),
        });
        next_entry_id += 1;
    }
    let _ = next_entry_id;

    if let Some(q) = quality_line(data) {
        let text = q.strip_prefix("quality: ").unwrap_or(&q).to_string();
        sections.entry("wrong").or_default().insert(
            0,
            Entry {
                id: usize::MAX,
                text,
                rep: None,
            },
        );
    }

    let omission = |omitted: &[&Value]| -> String {
        let mut by_cat: BTreeMap<&str, usize> = BTreeMap::new();
        for rep in omitted {
            let cat = str_field(rep, "category").unwrap_or("?");
            *by_cat.entry(cat).or_insert(0) += 1;
        }
        let mut ordered: Vec<(&str, usize)> = by_cat.into_iter().collect();
        ordered.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        let names: Vec<String> = ordered
            .iter()
            .take(4)
            .map(|(c, n)| format!("{c} {n}"))
            .collect();
        let retrieve = ordered
            .first()
            .map(|(c, _)| format!("; report.select(category=\"{c}\") retrieves them"))
            .unwrap_or_default();
        format!(
            "... [{} fact(s) omitted to fit {budget_chars} chars; {} findings total: {}{retrieve}]",
            omitted.len(),
            findings.len(),
            names.join(", ")
        )
    };

    let titles: BTreeMap<&str, String> = SECTION_ORDER
        .iter()
        .map(|&k| (k, section_title(k, designated)))
        .collect();

    let mut blocks: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    if !r_lines.is_empty() {
        let mut b = vec![format!("{}:", titles["row"])];
        b.extend(r_lines);
        blocks.insert("row", b);
    }
    if let Some(ne) = not_ex {
        blocks.insert(
            "not_examined",
            vec![format!("{}:", titles["not_examined"]), ne],
        );
    }

    let mut fixed_for_len: Vec<&str> = fixed.iter().map(String::as_str).collect();
    for &sec in &SECTION_ORDER {
        if let Some(b) = blocks.get(sec) {
            fixed_for_len.extend(b.iter().map(String::as_str));
        }
    }
    let fixed_chars = if fixed_for_len.is_empty() {
        0
    } else {
        fixed_for_len
            .iter()
            .map(|s| s.chars().count())
            .sum::<usize>()
            + fixed_for_len.len()
            - 1
    };

    let reserve = if facts.is_empty() {
        0
    } else {
        let all_reps: Vec<&Value> = facts.iter().map(|f| f.rep).collect();
        omission(&all_reps).chars().count() + 1
    };

    let mut room = budget_chars as isize - fixed_chars as isize - reserve as isize;
    for &name in &BODY_SECTIONS {
        room -= titles[name].chars().count() as isize + 2;
    }
    let room_pos = room.max(0) as usize;
    let has_target = designated.is_some();

    let mut kept: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut omitted_reps: Vec<&Value> = Vec::new();
    let mut waiting: BTreeMap<&str, Vec<(String, &Value)>> = BTreeMap::new();
    let mut used_total = 0usize;

    for &name in &BODY_SECTIONS {
        let Some(entries) = sections.get(name) else {
            continue;
        };
        if entries.is_empty() {
            continue;
        }
        let allowance = (section_share(name, has_target) * room_pos as f64) as usize;
        let ordered = if name == "target" || name == "relationships" {
            relationship_sequence(entries.clone(), allowance, &tag_of)
        } else {
            entries.clone()
        };
        let mut used = 0usize;
        let mut chosen: Vec<String> = Vec::new();
        let mut rest: Vec<(String, &Value)> = Vec::new();
        for e in ordered {
            let line = match e.rep {
                Some(r) => format!("{} [{}]", e.text, tag_of(r)),
                None => e.text.clone(),
            };
            let line_len = line.chars().count();
            if let Some(r) = e.rep {
                if used + line_len + 1 > allowance {
                    rest.push((line, r));
                    continue;
                }
            }
            chosen.push(line);
            used += line_len + 1;
        }
        if !chosen.is_empty() {
            kept.insert(name, chosen);
        }
        if !rest.is_empty() {
            waiting.insert(name, rest);
        }
        used_total += used;
    }

    let mut leftover = room_pos.saturating_sub(used_total);
    for &name in &BODY_SECTIONS {
        if let Some(rest) = waiting.remove(name) {
            for (line, rep) in rest {
                let line_len = line.chars().count();
                if line_len + 1 > leftover {
                    omitted_reps.push(rep);
                    continue;
                }
                kept.entry(name).or_default().push(line);
                leftover = leftover.saturating_sub(line_len + 1);
            }
        }
    }

    for (&name, lines) in &kept {
        let mut b = vec![format!("{}:", titles[name])];
        b.extend(lines.iter().cloned());
        blocks.insert(name, b);
    }

    let assemble = |bmap: &BTreeMap<&str, Vec<String>>, omitted: &[&Value]| -> String {
        let mut parts: Vec<String> = fixed.clone();
        for &name in &SECTION_ORDER {
            if let Some(block) = bmap.get(name) {
                if name != "row" && name != "not_examined" && block.len() <= 1 {
                    continue;
                }
                parts.extend(block.iter().cloned());
            }
        }
        if !omitted.is_empty() {
            parts.push(omission(omitted));
        }
        let mut s = parts.join("\n");
        s.push('\n');
        s
    };

    let mut result = assemble(&blocks, &omitted_reps);
    while result.chars().count() > budget_chars {
        let mut shrunk = false;
        for &name in SECTION_ORDER.iter().rev() {
            let can_pop = blocks.get(name).is_some_and(|l| l.len() > 1);
            if can_pop {
                let dropped = blocks.get_mut(name).unwrap().pop().unwrap();
                if kept.contains_key(name) {
                    if let Some(entries) = sections.get(name) {
                        for e in entries {
                            if let Some(r) = e.rep {
                                if format!("{} [{}]", e.text, tag_of(r)) == dropped {
                                    omitted_reps.push(r);
                                }
                            }
                        }
                    }
                }
                shrunk = true;
                break;
            }
        }
        if !shrunk {
            break;
        }
        result = assemble(&blocks, &omitted_reps);
    }

    if result.chars().count() > budget_chars {
        for drop_set in [&["not_examined"][..], &["row", "not_examined"][..]] {
            let smaller: BTreeMap<&str, Vec<String>> = blocks
                .iter()
                .filter(|(k, _)| !drop_set.contains(*k))
                .map(|(&k, v)| (k, v.clone()))
                .collect();
            result = assemble(&smaller, &omitted_reps);
            if result.chars().count() <= budget_chars {
                return result;
            }
        }
        let mut fallback = profile_line;
        fallback.push('\n');
        return fallback.chars().take(budget_chars).collect();
    }
    result
}
