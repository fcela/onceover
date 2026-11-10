//! Group contrast: one declared subset of the rows against the
//! rest of the table, column by column.
//!
//! A reader often holds one subset in mind — one region, one
//! cohort, one licence type — and wants to know how it differs
//! from everything else. Profiling the subset alone loses the
//! comparison, and the pairwise association screens relate two
//! columns over all rows, which is a different question. This
//! pass answers the subset question directly: the caller declares
//! the group in the hints (a boolean column's true rows, or the
//! rows where a column equals a value), and each other column is
//! compared between the group and the rest in the terms its kind
//! supports — means and medians with an effect size for numeric
//! columns, level shares for categorical columns, the true share
//! for boolean columns.
//!
//! A difference is reported only when the sizes support it: at
//! least 10 rows on each side and at least 10 known values per
//! side for the column, a numeric effect of at least 0.5 pooled
//! standard deviations (Cohen's d), a share difference of at
//! least 0.10, and in every case the difference at least 3 times
//! its standard error, so a small group must differ by a lot
//! before anything is stated. Findings are ranked by effect size
//! and capped at 10: a summary of how two populations differ,
//! not an exhaustive table.

use std::collections::BTreeMap;

use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, GroupContrast, Method, Pattern};
use serde_json::{json, Value};

/// Smallest number of rows on either side of the contrast.
const MIN_SIDE_ROWS: usize = 10;
/// Smallest number of known values on either side for one column.
const MIN_SIDE_VALUES: usize = 10;
/// Numeric reporting bar: |Cohen's d| on the pooled standard
/// deviation.
const NUMERIC_EFFECT_MIN: f64 = 0.5;
/// Share reporting bar: an absolute share difference.
const SHARE_EFFECT_MIN: f64 = 0.10;
/// Support bar: the difference must be at least this many times
/// its standard error.
const SUPPORT_Z: f64 = 3.0;
/// Categorical columns with more distinct values than this are
/// not contrasted: per-level shares stop being a summary.
const MAX_LEVELS: usize = 64;
/// Level movements recorded in a categorical finding's evidence.
const MAX_LEVELS_REPORTED: usize = 3;
/// Findings emitted by this pass per profile.
const MAX_FINDINGS: usize = 10;

const METHOD_TEXT: &str = "two-population contrast of the declared group against the rest of the table: numeric columns compare means with Cohen's d on the pooled standard deviation and are reported at |d| >= 0.5 with the mean difference at least 3 times its Welch standard error; boolean and categorical columns compare shares and are reported at a difference of at least 0.10 with the difference at least 3 times its pooled standard error; a column needs at least 10 known values on each side and each side at least 10 rows; categorical columns with more than 64 distinct values are not contrasted; at most 10 findings, ranked by effect size";

/// The pass's result: findings plus exactly one of a ran detail or
/// a skip reason for the coverage entry.
pub struct ContrastOutcome {
    pub findings: Vec<Finding>,
    pub detail: String,
    pub skipped: Option<String>,
}

fn skip(reason: String) -> ContrastOutcome {
    ContrastOutcome {
        findings: Vec::new(),
        detail: String::new(),
        skipped: Some(reason),
    }
}

/// The declared value, in a form matched against one column kind.
enum GroupValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

fn parse_value(spec: &GroupContrast) -> Result<Option<GroupValue>, String> {
    match &spec.value {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(GroupValue::Bool(*b))),
        Some(Value::Number(n)) => n
            .as_f64()
            .map(|f| Some(GroupValue::Number(f)))
            .ok_or_else(|| "the contrast value is a number outside the f64 range".to_string()),
        Some(Value::String(s)) => Ok(Some(GroupValue::Text(s.clone()))),
        Some(_) => Err("the contrast value must be a string, number, or boolean".to_string()),
    }
}

fn render_value(v: &GroupValue) -> String {
    match v {
        GroupValue::Bool(b) => b.to_string(),
        GroupValue::Number(f) => crate::fmt_float(*f),
        GroupValue::Text(s) => format!("'{s}'"),
    }
}

/// Membership of every row: `Some(true)` in the group, `Some(false)`
/// in the rest, `None` when the group column is null on the row.
fn membership(
    table: &TypedTable,
    gi: usize,
    value: &Option<GroupValue>,
) -> Result<Vec<Option<bool>>, String> {
    let col = &table.cols[gi];
    let name = &table.names[gi];
    let dtype = &table.dtypes[gi];
    let out: Vec<Option<bool>> = match col {
        TypedCol::Bool { vals } => {
            let want = match value {
                None => true,
                Some(GroupValue::Bool(b)) => *b,
                Some(other) => {
                    return Err(format!(
                        "the contrast value {} cannot mark membership in boolean column '{name}'; give a boolean or omit the value",
                        render_value(other)
                    ))
                }
            };
            vals.iter().map(|v| v.map(|b| b == want)).collect()
        }
        TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. } => {
            let Some(GroupValue::Number(x)) = value else {
                return Err(match value {
                    None => format!(
                        "a contrast on column '{name}' needs a value: only a boolean column defines a group without one"
                    ),
                    Some(other) => format!(
                        "the contrast value {} cannot match numeric column '{name}' of type {dtype}",
                        render_value(other)
                    ),
                });
            };
            (0..table.n)
                .map(|r| col.numeric_at(r).map(|v| v == *x))
                .collect()
        }
        TypedCol::Str { .. } | TypedCol::StrOwned { .. } => {
            let Some(GroupValue::Text(s)) = value else {
                return Err(match value {
                    None => format!(
                        "a contrast on column '{name}' needs a value: only a boolean column defines a group without one"
                    ),
                    Some(other) => format!(
                        "the contrast value {} cannot match string column '{name}'",
                        render_value(other)
                    ),
                });
            };
            (0..table.n)
                .map(|r| col.str_at(r).map(|v| v == s.as_str()))
                .collect()
        }
        _ => {
            return Err(format!(
                "group column '{name}' has type {dtype}; a contrast group is defined on a boolean, numeric, or string column"
            ))
        }
    };
    Ok(out)
}

struct Side {
    n: usize,
    mean: f64,
    sd: f64,
    median: f64,
}

fn summarize(mut vals: Vec<f64>) -> Side {
    let n = vals.len();
    let mean = vals.iter().sum::<f64>() / n as f64;
    let var = if n > 1 {
        vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / (n - 1) as f64
    } else {
        0.0
    };
    vals.sort_by(|a, b| a.total_cmp(b));
    let median = if n % 2 == 1 {
        vals[n / 2]
    } else {
        (vals[n / 2 - 1] + vals[n / 2]) / 2.0
    };
    Side {
        n,
        mean,
        sd: var.sqrt(),
        median,
    }
}

/// A share difference's pooled standard error:
/// sqrt(p(1-p)(1/n_g + 1/n_r)) with p the two sides' pooled share.
fn share_se(x_g: usize, n_g: usize, x_r: usize, n_r: usize) -> f64 {
    let p = (x_g + x_r) as f64 / (n_g + n_r) as f64;
    (p * (1.0 - p) * (1.0 / n_g as f64 + 1.0 / n_r as f64)).sqrt()
}

struct Ctx<'a, 't> {
    table: &'a TypedTable<'t>,
    member: &'a [Option<bool>],
    group_col: &'a str,
    group_value: Option<Value>,
    group_def: String,
    group_rows: usize,
    rest_rows: usize,
    unassigned: usize,
}

impl Ctx<'_, '_> {
    fn base_metrics(&self, col: &str, kind: &str) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("column".into(), json!(col)),
            ("group_column".into(), json!(self.group_col)),
            (
                "group_value".into(),
                self.group_value.clone().unwrap_or(Value::Null),
            ),
            ("kind".into(), json!(kind)),
            ("group_rows".into(), json!(self.group_rows)),
            ("rest_rows".into(), json!(self.rest_rows)),
            ("unassigned_rows".into(), json!(self.unassigned)),
            ("n".into(), json!(self.group_rows + self.rest_rows)),
            ("population_rows".into(), json!(self.table.n)),
            ("method".into(), json!(METHOD_TEXT)),
        ])
    }

    fn finding(
        &self,
        col: &str,
        claim: String,
        score: f64,
        metrics: BTreeMap<String, Value>,
    ) -> Finding {
        Finding::new(
            Pattern::GroupContrast,
            BTreeMap::from([(
                "columns".into(),
                json!([self.group_col, col]),
            )]),
            claim,
            EvidenceBasis::Measured,
            score,
            metrics,
            Some(format!(
                "the contrast is descriptive: it compares the declared group ({}) with the rest of the table on this column, over the rows where both the group and the column are known; it does not adjust for other columns, and the group was declared by the caller, not found in the data",
                self.group_def
            )),
            Method::new(
                "group_contrast",
                "two_population_contrast",
                "declared group contrast (onceover v0.1)",
            ),
        )
    }
}

fn numeric_contrast(ctx: &Ctx, ci: usize) -> Option<(f64, Finding)> {
    let col = &ctx.table.cols[ci];
    let name = &ctx.table.names[ci];
    let mut g: Vec<f64> = Vec::new();
    let mut r: Vec<f64> = Vec::new();
    for row in 0..ctx.table.n {
        if let (Some(m), Some(v)) = (ctx.member[row], col.numeric_at(row)) {
            if m {
                g.push(v);
            } else {
                r.push(v);
            }
        }
    }
    if g.len() < MIN_SIDE_VALUES || r.len() < MIN_SIDE_VALUES {
        return None;
    }
    let gs = summarize(g);
    let rs = summarize(r);
    let diff = gs.mean - rs.mean;
    let mut metrics = ctx.base_metrics(name, "numeric");
    metrics.insert("group_n".into(), json!(gs.n));
    metrics.insert("rest_n".into(), json!(rs.n));
    metrics.insert("group_mean".into(), json!(gs.mean));
    metrics.insert("rest_mean".into(), json!(rs.mean));
    metrics.insert("difference".into(), json!(diff));
    metrics.insert("group_median".into(), json!(gs.median));
    metrics.insert("rest_median".into(), json!(rs.median));
    metrics.insert("group_sd".into(), json!(gs.sd));
    metrics.insert("rest_sd".into(), json!(rs.sd));
    if gs.sd == 0.0 && rs.sd == 0.0 {
        if diff == 0.0 {
            return None;
        }
        // Constant within both populations, different constants:
        // a perfect separation, with no pooled spread to scale by.
        metrics.insert("pooled_sd".into(), json!(0.0));
        metrics.insert("effect_size".into(), Value::Null);
        let claim = format!(
            "{name} is {} on every group row with a value ({}; {} rows) and {} on every other row with a value ({} rows)",
            crate::fmt_float(gs.mean),
            ctx.group_def,
            gs.n,
            crate::fmt_float(rs.mean),
            rs.n,
        );
        return Some((1.0, ctx.finding(name, claim, 0.95, metrics)));
    }
    let pooled_sd = (((gs.n - 1) as f64 * gs.sd * gs.sd + (rs.n - 1) as f64 * rs.sd * rs.sd)
        / (gs.n + rs.n - 2) as f64)
        .sqrt();
    let d = diff / pooled_sd;
    let welch_se = (gs.sd * gs.sd / gs.n as f64 + rs.sd * rs.sd / rs.n as f64).sqrt();
    if d.abs() < NUMERIC_EFFECT_MIN || diff.abs() < SUPPORT_Z * welch_se {
        return None;
    }
    metrics.insert("pooled_sd".into(), json!(pooled_sd));
    metrics.insert("effect_size".into(), json!(d));
    let claim = format!(
        "{name} averages {} in the group ({}; {} rows) against {} in the other {} rows — a difference of {}, {} pooled standard deviations (medians {} against {})",
        crate::fmt_float(gs.mean),
        ctx.group_def,
        gs.n,
        crate::fmt_float(rs.mean),
        rs.n,
        crate::fmt_float(diff),
        crate::fmt_float(d.abs()),
        crate::fmt_float(gs.median),
        crate::fmt_float(rs.median),
    );
    let rank = (d.abs() / 2.0).min(1.0);
    let score = 0.5 + 0.225 * d.abs().min(2.0);
    Some((rank, ctx.finding(name, claim, score, metrics)))
}

fn boolean_contrast(ctx: &Ctx, ci: usize) -> Option<(f64, Finding)> {
    let TypedCol::Bool { vals } = &ctx.table.cols[ci] else {
        return None;
    };
    let name = &ctx.table.names[ci];
    let (mut x_g, mut n_g, mut x_r, mut n_r) = (0usize, 0usize, 0usize, 0usize);
    for (row, v) in vals.iter().enumerate() {
        if let (Some(m), Some(b)) = (ctx.member[row], v) {
            if m {
                n_g += 1;
                x_g += usize::from(*b);
            } else {
                n_r += 1;
                x_r += usize::from(*b);
            }
        }
    }
    if n_g < MIN_SIDE_VALUES || n_r < MIN_SIDE_VALUES {
        return None;
    }
    let p_g = x_g as f64 / n_g as f64;
    let p_r = x_r as f64 / n_r as f64;
    let diff = p_g - p_r;
    let se = share_se(x_g, n_g, x_r, n_r);
    if diff.abs() < SHARE_EFFECT_MIN || diff.abs() < SUPPORT_Z * se {
        return None;
    }
    let mut metrics = ctx.base_metrics(name, "boolean");
    metrics.insert("group_n".into(), json!(n_g));
    metrics.insert("rest_n".into(), json!(n_r));
    metrics.insert("group_share".into(), json!(p_g));
    metrics.insert("rest_share".into(), json!(p_r));
    metrics.insert("difference".into(), json!(diff));
    let claim = format!(
        "{name} is true for {:.1}% of the group ({}; {} rows) against {:.1}% of the other {} rows ({:+.1} points)",
        100.0 * p_g,
        ctx.group_def,
        n_g,
        100.0 * p_r,
        n_r,
        100.0 * diff,
    );
    let rank = (diff.abs() / 0.5).min(1.0);
    let score = 0.5 + 1.8 * diff.abs().min(0.25);
    Some((rank, ctx.finding(name, claim, score, metrics)))
}

fn categorical_contrast(ctx: &Ctx, ci: usize) -> Option<(f64, Finding)> {
    let col = &ctx.table.cols[ci];
    let name = &ctx.table.names[ci];
    let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let (mut n_g, mut n_r) = (0usize, 0usize);
    for row in 0..ctx.table.n {
        if let (Some(m), Some(v)) = (ctx.member[row], col.str_at(row)) {
            let e = counts.entry(v).or_default();
            if m {
                n_g += 1;
                e.0 += 1;
            } else {
                n_r += 1;
                e.1 += 1;
            }
        }
    }
    if n_g < MIN_SIDE_VALUES || n_r < MIN_SIDE_VALUES || counts.len() > MAX_LEVELS {
        return None;
    }
    let mut levels: Vec<(&str, f64, f64, f64)> = Vec::new();
    for (level, (cg, cr)) in &counts {
        let p_g = *cg as f64 / n_g as f64;
        let p_r = *cr as f64 / n_r as f64;
        let diff = p_g - p_r;
        let se = share_se(*cg, n_g, *cr, n_r);
        if diff.abs() >= SHARE_EFFECT_MIN && diff.abs() >= SUPPORT_Z * se {
            levels.push((level, p_g, p_r, diff));
        }
    }
    if levels.is_empty() {
        return None;
    }
    // Rank by the size of the movement. Movements that tie at any
    // precision the counts can express prefer the level the group
    // over-represents, then the level name: a two-level column's
    // levels move by exactly opposite amounts, and the headline
    // should name the level the group has more of.
    levels.sort_by(|a, b| {
        (b.3.abs() * 1e12)
            .round()
            .total_cmp(&(a.3.abs() * 1e12).round())
            .then(b.3.total_cmp(&a.3))
            .then(a.0.cmp(b.0))
    });
    let (level, p_g, p_r, diff) = levels[0];
    let mut metrics = ctx.base_metrics(name, "categorical");
    metrics.insert("group_n".into(), json!(n_g));
    metrics.insert("rest_n".into(), json!(n_r));
    metrics.insert("distinct".into(), json!(counts.len()));
    metrics.insert("level".into(), json!(level));
    metrics.insert("group_share".into(), json!(p_g));
    metrics.insert("rest_share".into(), json!(p_r));
    metrics.insert("difference".into(), json!(diff));
    metrics.insert(
        "levels".into(),
        json!(levels
            .iter()
            .take(MAX_LEVELS_REPORTED)
            .map(|(l, pg, pr, d)| {
                json!({"level": l, "group_share": pg, "rest_share": pr, "difference": d})
            })
            .collect::<Vec<_>>()),
    );
    let mut claim = format!(
        "{name} is '{level}' for {:.1}% of the group ({}; {} rows) against {:.1}% of the other {} rows ({:+.1} points)",
        100.0 * p_g,
        ctx.group_def,
        n_g,
        100.0 * p_r,
        n_r,
        100.0 * diff,
    );
    if levels.len() > 1 {
        claim.push_str(&format!(
            "; {} more level{} differ{} by at least 10 points — the largest are in the evidence",
            levels.len() - 1,
            if levels.len() == 2 { "" } else { "s" },
            if levels.len() == 2 { "s" } else { "" },
        ));
    }
    let rank = (diff.abs() / 0.5).min(1.0);
    let score = 0.5 + 1.8 * diff.abs().min(0.25);
    Some((rank, ctx.finding(name, claim, score, metrics)))
}

/// The group-contrast pass. See the module documentation for the
/// reporting bars.
pub fn group_contrast(table: &TypedTable, spec: &GroupContrast) -> ContrastOutcome {
    let Some(gi) = table.names.iter().position(|n| n == &spec.column) else {
        return skip(format!(
            "group column '{}' is not a column of the frame",
            spec.column
        ));
    };
    let value = match parse_value(spec) {
        Ok(v) => v,
        Err(e) => return skip(e),
    };
    let member = match membership(table, gi, &value) {
        Ok(m) => m,
        Err(e) => return skip(e),
    };
    let group_rows = member.iter().filter(|m| **m == Some(true)).count();
    let rest_rows = member.iter().filter(|m| **m == Some(false)).count();
    let unassigned = table.n - group_rows - rest_rows;
    if group_rows == 0 {
        return skip(format!(
            "the declared group ({}) names no rows",
            group_def(&spec.column, &value)
        ));
    }
    if rest_rows == 0 {
        return skip(format!(
            "the declared group ({}) covers every row with a known group value; there is no rest to contrast against",
            group_def(&spec.column, &value)
        ));
    }
    if group_rows < MIN_SIDE_ROWS || rest_rows < MIN_SIDE_ROWS {
        return skip(format!(
            "the declared group has {group_rows} rows against {rest_rows} rest rows; a contrast needs at least {MIN_SIDE_ROWS} rows on each side"
        ));
    }
    let ctx = Ctx {
        table,
        member: &member,
        group_col: &spec.column,
        group_value: spec.value.clone(),
        group_def: group_def(&spec.column, &value),
        group_rows,
        rest_rows,
        unassigned,
    };
    let mut scored: Vec<(f64, Finding)> = Vec::new();
    let (mut evaluated, mut not_evaluated) = (0usize, 0usize);
    for ci in 0..table.cols.len() {
        if ci == gi {
            continue;
        }
        let hit = match &table.cols[ci] {
            TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. } => {
                evaluated += 1;
                numeric_contrast(&ctx, ci)
            }
            TypedCol::Bool { .. } => {
                evaluated += 1;
                boolean_contrast(&ctx, ci)
            }
            TypedCol::Str { .. } | TypedCol::StrOwned { .. }
                if distinct_within_cap(&table.cols[ci]) =>
            {
                evaluated += 1;
                categorical_contrast(&ctx, ci)
            }
            TypedCol::Str { .. } | TypedCol::StrOwned { .. } => {
                not_evaluated += 1;
                None
            }
            _ => {
                not_evaluated += 1;
                None
            }
        };
        if let Some(s) = hit {
            scored.push(s);
        }
    }
    let cleared = scored.len();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.claim.cmp(&b.1.claim)));
    scored.truncate(MAX_FINDINGS);
    let findings: Vec<Finding> = scored.into_iter().map(|(_, f)| f).collect();
    let mut detail = format!(
        "group {}: {group_rows} rows against {rest_rows} rest rows{}; {evaluated} columns evaluated, {cleared} contrasts cleared the reporting bars (numeric |d| >= 0.5, share differences >= 0.10, each at least 3 standard errors), {} reported (cap {MAX_FINDINGS})",
        ctx.group_def,
        if unassigned > 0 {
            format!(" ({unassigned} rows with no group value excluded from both sides)")
        } else {
            String::new()
        },
        findings.len(),
    );
    if not_evaluated > 0 {
        detail.push_str(&format!(
            "; {not_evaluated} columns not evaluated (date, datetime, unsupported, or over {MAX_LEVELS} distinct values)"
        ));
    }
    ContrastOutcome {
        findings,
        detail,
        skipped: None,
    }
}

fn distinct_within_cap(col: &TypedCol) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    for row in 0..col.len() {
        if let Some(v) = col.str_at(row) {
            seen.insert(v);
            if seen.len() > MAX_LEVELS {
                return false;
            }
        }
    }
    true
}

/// How the group reads in claims and coverage: `region = 'north'`
/// for a value group, `flag is true` for a bare boolean column.
fn group_def(column: &str, value: &Option<GroupValue>) -> String {
    match value {
        None => format!("{column} is true"),
        Some(v) => format!("{column} = {}", render_value(v)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::extract;
    use polars::prelude::*;

    fn spec(column: &str, value: Option<Value>) -> GroupContrast {
        GroupContrast {
            column: column.into(),
            value,
        }
    }

    fn planted() -> DataFrame {
        let mut region: Vec<&str> = Vec::new();
        let mut tokens: Vec<f64> = Vec::new();
        let mut plan: Vec<&str> = Vec::new();
        for i in 0..2000 {
            if i < 36 {
                region.push("north");
                tokens.push(400.0 + (i % 7) as f64);
                plan.push(if i % 5 < 3 { "pro" } else { "free" });
            } else {
                region.push(["south", "east", "west"][i % 3]);
                tokens.push(200.0 + (i % 7) as f64);
                plan.push(if i % 5 == 0 { "pro" } else { "free" });
            }
        }
        df! { "region" => region, "tokens" => tokens, "plan" => plan }.unwrap()
    }

    #[test]
    fn planted_difference_fires_with_direction_and_size() {
        let df = planted();
        let table = extract(&df);
        let out = group_contrast(&table, &spec("region", Some(json!("north"))));
        assert!(out.skipped.is_none(), "{:?}", out.skipped);
        let types: Vec<_> = out
            .findings
            .iter()
            .map(|f| f.finding_type.as_deref())
            .collect();
        assert_eq!(types, vec![Some("group_contrast"), Some("group_contrast")]);
        let tokens = out
            .findings
            .iter()
            .find(|f| f.claim.contains("tokens averages"))
            .expect("numeric contrast on tokens");
        let m = tokens.metrics();
        assert_eq!(m["group_rows"], json!(36));
        assert_eq!(m["rest_rows"], json!(1964));
        assert!((m["group_mean"].as_f64().unwrap() - 403.0).abs() < 1.0);
        assert!((m["rest_mean"].as_f64().unwrap() - 203.0).abs() < 1.0);
        assert!(m["effect_size"].as_f64().unwrap() > 50.0);
        tokens.validate().unwrap();
        let plan = out
            .findings
            .iter()
            .find(|f| f.claim.contains("plan is 'pro'"))
            .expect("categorical contrast on plan");
        let m = plan.metrics();
        assert!((m["group_share"].as_f64().unwrap() - 0.6).abs() < 0.02);
        assert!((m["rest_share"].as_f64().unwrap() - 0.2).abs() < 0.02);
        plan.validate().unwrap();
    }

    #[test]
    fn missing_and_degenerate_groups_skip_with_reasons() {
        let df = planted();
        let table = extract(&df);
        let out = group_contrast(&table, &spec("nope", Some(json!("x"))));
        assert!(out.skipped.unwrap().contains("not a column"));
        let out = group_contrast(&table, &spec("region", Some(json!("arctic"))));
        assert!(out.skipped.unwrap().contains("names no rows"));
        let out = group_contrast(&table, &spec("region", None));
        assert!(out.skipped.unwrap().contains("needs a value"));
    }

    #[test]
    fn boolean_column_defines_a_group_without_a_value() {
        let flag: Vec<bool> = (0..500).map(|i| i % 10 == 0).collect();
        let score: Vec<f64> = (0..500)
            .map(|i| if i % 10 == 0 { 10.0 } else { 5.0 } + (i % 3) as f64 * 0.01)
            .collect();
        let df = df! { "flag" => flag, "score" => score }.unwrap();
        let table = extract(&df);
        let out = group_contrast(&table, &spec("flag", None));
        assert!(out.skipped.is_none(), "{:?}", out.skipped);
        assert_eq!(out.findings.len(), 1);
        assert!(out.findings[0].claim.contains("score averages"));
        out.findings[0].validate().unwrap();
    }
}
