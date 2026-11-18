//! Leakage signatures against a designated target (issue #282).
//!
//! The profiler is response-free by default; this pass runs only
//! when the caller designates a target (`profile(df, target="y")`).
//! It measures every other column's univariate strength against
//! the target on its own — no joint model — and states a finding
//! only for the shape a practitioner checks first and a joint
//! screen cannot name: one column that nearly determines the
//! target by itself, out of line with the rest of the field. That
//! shape is what a column derived from the outcome, or recorded
//! after it, looks like; the statement names the signature and
//! the mundane explanations, and never asserts leakage.
//!
//! Strength scales, all on 0..1 and chance-corrected:
//! - categorical target: the reliable fraction of information
//!   (RFI) of the fixed-margin FD machinery — the share of the
//!   target's entropy the column accounts for beyond what the
//!   permutation model expects at the same margins.
//! - numeric target, numeric predictor: Chatterjee's xi in the
//!   target direction (the share of the target's variation that
//!   is a function of the predictor, up to noise).
//! - numeric target, categorical predictor: the between-group
//!   share of the target's variance, degrees-of-freedom adjusted
//!   (adjusted eta-squared).
//!
//! A column is stated when it determines the target exactly on
//! the evaluated rows, or when its strength clears the bar
//! (0.80) and dwarfs the runner-up by the stated ratio (1.5x):
//! leakage is a column out of line with the field, not a strong
//! field. The bars are frozen in
//! `docs/benchmarks/2026-11-13-leakage-signatures.md`.

use std::collections::BTreeMap;

use onceover_core::typed::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;

use crate::dep::chatterjee_xi;

/// Minimum complete cases for a predictor to be evaluated.
pub const MIN_ROWS: usize = 100;
/// Strength bar for the dwarfing path (all three scales).
pub const STRENGTH_BAR: f64 = 0.80;
/// The strongest column must exceed the runner-up by this ratio.
pub const DWARF_RATIO: f64 = 1.5;
/// A numeric predictor with at most this many distinct values is
/// treated as categorical for a numeric target (and vice versa a
/// numeric target with at most this many distinct values is a
/// categorical target).
pub const CATEGORICAL_MAX_DISTINCT: usize = 20;
/// Guard on FD grouping: a predictor with more distinct values
/// than this is not grouped against a categorical target.
const MAX_FD_DISTINCT: usize = 50_000;
/// At most this many exactly-determining columns are stated.
const MAX_DETERMINATION_FINDINGS: usize = 3;

#[derive(Clone, Copy, PartialEq)]
enum Measure {
    EntropyFraction,
    Xi,
    Eta2Adjusted,
}

impl Measure {
    fn label(self) -> &'static str {
        match self {
            Measure::EntropyFraction => "entropy_fraction_corrected",
            Measure::Xi => "chatterjee_xi",
            Measure::Eta2Adjusted => "eta2_adjusted",
        }
    }
}

struct Strength {
    ci: usize,
    value: f64,
    measure: Measure,
    n_used: usize,
    violations: Option<usize>,
    determines: bool,
}

fn numeric_values(table: &TypedTable, ci: usize) -> Option<Vec<Option<f64>>> {
    match &table.cols[ci] {
        TypedCol::I64 { vals } => Some(vals.iter().map(|v| v.map(|x| *x as f64)).collect()),
        TypedCol::U64 { vals } => Some(vals.iter().map(|v| v.map(|x| *x as f64)).collect()),
        TypedCol::F64 { vals } => Some(vals.iter().map(|v| v.copied()).collect()),
        TypedCol::Date { vals } => Some(vals.iter().map(|v| v.map(|x| x as f64)).collect()),
        TypedCol::Datetime { vals, .. } => Some(vals.iter().map(|v| v.map(|x| x as f64)).collect()),
        _ => None,
    }
}

/// A row's level key for categorical treatment, or None when the
/// value is missing.
fn level_key(table: &TypedTable, ci: usize, r: usize) -> Option<String> {
    match &table.cols[ci] {
        TypedCol::Str { vals } => vals[r].map(|s| s.to_string()),
        TypedCol::StrOwned { vals } => vals[r].clone(),
        TypedCol::Bool { vals } => vals[r].map(|b| b.to_string()),
        TypedCol::I64 { vals } => vals.at(r).map(|v| v.to_string()),
        TypedCol::U64 { vals } => vals.at(r).map(|v| v.to_string()),
        TypedCol::F64 { vals } => vals.at(r).map(|v| v.to_string()),
        TypedCol::Date { vals } => vals[r].map(|v| v.to_string()),
        TypedCol::Datetime { vals, .. } => vals[r].map(|v| v.to_string()),
        _ => None,
    }
}

fn distinct_count(table: &TypedTable, ci: usize) -> usize {
    let mut seen = std::collections::HashSet::new();
    for r in 0..table.n {
        if let Some(k) = level_key(table, ci, r) {
            seen.insert(k);
            if seen.len() > MAX_FD_DISTINCT {
                return seen.len();
            }
        }
    }
    seen.len()
}

fn is_categorical_col(table: &TypedTable, ci: usize) -> bool {
    match &table.cols[ci] {
        TypedCol::Str { .. } | TypedCol::StrOwned { .. } | TypedCol::Bool { .. } => true,
        TypedCol::Null { .. } | TypedCol::Other { .. } => false,
        _ => distinct_count(table, ci) <= CATEGORICAL_MAX_DISTINCT,
    }
}

/// Adjusted eta-squared of a numeric target across a categorical
/// predictor's levels, on complete cases. Returns (value, n,
/// determines) where determines means the target is constant
/// within every level.
fn adjusted_eta2(
    table: &TypedTable,
    pred: usize,
    target_vals: &[Option<f64>],
) -> Option<(f64, usize, bool)> {
    let mut groups: BTreeMap<String, (f64, f64, usize)> = BTreeMap::new(); // (sum, sumsq, n)
    let mut n = 0usize;
    let mut total_sum = 0.0f64;
    for (r, target_val) in target_vals.iter().enumerate() {
        let Some(y) = target_val else { continue };
        if !y.is_finite() {
            continue;
        }
        let Some(key) = level_key(table, pred, r) else {
            continue;
        };
        let e = groups.entry(key).or_default();
        e.0 += y;
        e.1 += y * y;
        e.2 += 1;
        n += 1;
        total_sum += y;
    }
    let k = groups.len();
    if n < MIN_ROWS || k < 2 || groups.values().any(|g| g.2 < 2) {
        return None;
    }
    let grand = total_sum / n as f64;
    let mut ss_total = 0.0f64;
    let mut ss_within = 0.0f64;
    for (sum, sumsq, cnt) in groups.values() {
        let mean = sum / *cnt as f64;
        ss_total += sumsq - 2.0 * grand * sum + *cnt as f64 * grand * grand;
        ss_within += sumsq - 2.0 * mean * sum + *cnt as f64 * mean * mean;
    }
    if ss_total <= 0.0 {
        return None;
    }
    let determines = ss_within <= 0.0;
    let adjusted = crate::dep::adjusted_eta_squared(ss_within, ss_total, n, k);
    Some((adjusted, n, determines))
}

/// The per-column strength evaluation against one target: every
/// other column's univariate strength and its exact-determination
/// flag, on complete cases. Shared by the leakage screen (which
/// states the strongest signatures) and by
/// `determining_columns` (which the model-failure screen uses to
/// exclude determiners before fitting), so the determination
/// test exists in exactly one place.
struct Evaluation {
    strengths: Vec<Strength>,
    evaluated: usize,
    target_categorical: bool,
}

/// Evaluate every predictor column against the target at index
/// `tci`. Returns None when the target's type is not evaluated
/// (neither categorical nor numeric).
fn evaluate_strengths(table: &TypedTable, tci: usize) -> Option<Evaluation> {
    let target_categorical = is_categorical_col(table, tci);
    let target_vals = numeric_values(table, tci);
    if !target_categorical && target_vals.is_none() {
        return None;
    }
    let rows: Vec<u32> = (0..table.n as u32).collect();
    let mut strengths: Vec<Strength> = Vec::new();
    let mut evaluated = 0usize;
    for ci in 0..table.cols.len() {
        if ci == tci {
            continue;
        }
        if matches!(
            table.cols[ci],
            TypedCol::Null { .. } | TypedCol::Other { .. }
        ) {
            continue;
        }
        if target_categorical {
            let distinct = distinct_count(table, ci);
            if distinct > MAX_FD_DISTINCT {
                continue;
            }
            let stats = onceover_deps::fd::fd_stats(table, &rows, &[ci], tci);
            if stats.n_used < MIN_ROWS {
                continue;
            }
            evaluated += 1;
            // Exact determination is only a statement when the
            // predictor's values repeat: a near-unique column
            // pairs every one of its singleton groups with a
            // single target value vacuously, on any target. The
            // average group must hold at least four rows.
            let determines = stats.violations == 0 && distinct * 4 <= stats.n_used;
            strengths.push(Strength {
                ci,
                value: stats.rfi.clamp(0.0, 1.0),
                measure: Measure::EntropyFraction,
                n_used: stats.n_used,
                violations: Some(stats.violations),
                determines,
            });
        } else {
            let tv = target_vals.as_ref().expect("numeric target values");
            if is_categorical_col(table, ci) {
                let Some((value, n, determines)) = adjusted_eta2(table, ci, tv) else {
                    continue;
                };
                evaluated += 1;
                strengths.push(Strength {
                    ci,
                    value,
                    measure: Measure::Eta2Adjusted,
                    n_used: n,
                    violations: None,
                    determines,
                });
            } else {
                let Some(pred_vals) = numeric_values(table, ci) else {
                    continue;
                };
                let pairs: Vec<(f64, f64)> = (0..table.n)
                    .filter_map(|r| match (pred_vals[r], tv[r]) {
                        (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Some((x, y)),
                        _ => None,
                    })
                    .collect();
                if pairs.len() < MIN_ROWS {
                    continue;
                }
                let Some(xi) = chatterjee_xi(&pairs) else {
                    continue;
                };
                evaluated += 1;
                strengths.push(Strength {
                    ci,
                    value: xi.clamp(0.0, 1.0),
                    measure: Measure::Xi,
                    n_used: pairs.len(),
                    violations: None,
                    determines: xi >= 0.999,
                });
            }
        }
    }
    Some(Evaluation {
        strengths,
        evaluated,
        target_categorical,
    })
}

/// Columns that determine the target exactly, by the same tests
/// the leakage screen states: zero functional-dependence
/// violations for a categorical target (with the guard that the
/// predictor's groups average at least four rows, so a near-unique
/// column does not determine vacuously), a target constant within
/// every level of a categorical predictor for a numeric target,
/// or Chatterjee's xi at 0.999 or above for a numeric predictor.
/// Returned in column order. The model-failure screen excludes
/// these columns from its model inputs and its slice variables
/// before fitting (issue #306): a column that restates the target
/// would otherwise hand the cross-fit a perfect score and spend
/// the statement budget on slices of itself.
pub fn determining_columns(table: &TypedTable, target_name: &str) -> Vec<usize> {
    let Some(tci) = table.names.iter().position(|n| n == target_name) else {
        return Vec::new();
    };
    let Some(evaluation) = evaluate_strengths(table, tci) else {
        return Vec::new();
    };
    let mut cols: Vec<usize> = evaluation
        .strengths
        .iter()
        .filter(|s| s.determines)
        .map(|s| s.ci)
        .collect();
    cols.sort_unstable();
    cols
}

pub fn screen_leakage(
    table: &TypedTable,
    target_name: &str,
    declared_after: &[String],
) -> (Vec<Finding>, String) {
    let Some(tci) = table.names.iter().position(|n| n == target_name) else {
        return (
            Vec::new(),
            format!("designated target '{target_name}' is not a column of the frame"),
        );
    };
    let Some(evaluation) = evaluate_strengths(table, tci) else {
        return (
            Vec::new(),
            format!(
                "designated target '{target_name}' has a type the leakage screen does not evaluate"
            ),
        );
    };
    let Evaluation {
        mut strengths,
        evaluated,
        target_categorical,
    } = evaluation;
    // Rank: strength descending, column name as the tiebreak.
    strengths.sort_by(|a, b| {
        b.value
            .total_cmp(&a.value)
            .then(table.names[a.ci].cmp(&table.names[b.ci]))
    });
    let kind = if target_categorical {
        "categorical"
    } else {
        "numeric"
    };
    let head = format!(
        "target '{target_name}' designated ({kind}); {evaluated} predictors evaluated on complete cases (minimum {MIN_ROWS})"
    );
    if strengths.is_empty() {
        return (
            Vec::new(),
            format!("{head}; no predictor could be evaluated"),
        );
    }
    let top = &strengths[0];
    let runner = strengths.get(1);
    let runner_txt = runner.map_or_else(
        || "no runner-up".to_string(),
        |r| format!("next strongest '{}' at {:.3}", table.names[r.ci], r.value),
    );

    // Declared suspects first (issue #310): the columns the
    // caller declared recorded after the outcome are checked
    // before the field, and one that determines the target or
    // clears the strength bar is stated as a declared suspect,
    // ahead of determiners found on strength alone. Every
    // declared column's measured strength is stated in coverage,
    // whether or not it is stated as a finding.
    let determiners: Vec<&Strength> = strengths.iter().filter(|s| s.determines).collect();
    let mut declared_clause = String::new();
    if !declared_after.is_empty() {
        let mut checks: Vec<String> = Vec::new();
        let mut declared_findings: Vec<Finding> = Vec::new();
        for name in declared_after {
            match strengths.iter().find(|s| table.names[s.ci] == *name) {
                Some(s) if s.determines => {
                    checks.push(format!("'{name}' determines the target exactly"));
                    declared_findings.push(make_finding(
                        table,
                        s,
                        runner,
                        target_name,
                        target_categorical,
                        true,
                        determiners.len(),
                        true,
                    ));
                }
                Some(s) if s.value >= STRENGTH_BAR => {
                    checks.push(format!(
                        "'{name}' at {:.3}, over the {STRENGTH_BAR:.2} bar",
                        s.value
                    ));
                    declared_findings.push(make_finding(
                        table,
                        s,
                        runner,
                        target_name,
                        target_categorical,
                        false,
                        0,
                        true,
                    ));
                }
                Some(s) => {
                    checks.push(format!(
                        "'{name}' at {:.3}, below the {STRENGTH_BAR:.2} bar",
                        s.value
                    ));
                }
                None => {
                    checks.push(format!("'{name}' could not be evaluated"));
                }
            }
        }
        declared_clause = format!("; declared suspects checked first: {}", checks.join(", "));
        if !declared_findings.is_empty() {
            let n_declared = declared_findings.len();
            let declared_cols: Vec<usize> = declared_after
                .iter()
                .filter_map(|n| table.names.iter().position(|t| t == n))
                .collect();
            let mut findings = declared_findings;
            let rest: Vec<Finding> = determiners
                .iter()
                .filter(|s| !declared_cols.contains(&s.ci))
                .take(MAX_DETERMINATION_FINDINGS)
                .map(|s| {
                    make_finding(
                        table,
                        s,
                        runner,
                        target_name,
                        target_categorical,
                        true,
                        determiners.len(),
                        false,
                    )
                })
                .collect();
            let n_rest = rest.len();
            findings.extend(rest);
            return (
                findings,
                format!(
                    "{head}{declared_clause}; {n_declared} stated as declared suspect(s){}",
                    if n_rest > 0 {
                        format!("; {n_rest} further column(s) determine the target exactly and are stated")
                    } else {
                        String::new()
                    }
                ),
            );
        }
    }

    // Determination path: every exactly-determining column is
    // stated (capped), whatever the rest of the field looks like.
    if !determiners.is_empty() {
        let findings: Vec<Finding> = determiners
            .iter()
            .take(MAX_DETERMINATION_FINDINGS)
            .map(|s| {
                make_finding(
                    table,
                    s,
                    runner,
                    target_name,
                    target_categorical,
                    true,
                    determiners.len(),
                    false,
                )
            })
            .collect();
        let n_stated = findings.len();
        return (
            findings,
            format!(
                "{head}{declared_clause}; {} column(s) determine the target exactly on the evaluated rows ({}); {} stated",
                determiners.len(),
                runner_txt,
                n_stated
            ),
        );
    }

    // Dwarfing path.
    let dwarfs = runner.is_none_or(|r| top.value >= DWARF_RATIO * r.value);
    if top.value >= STRENGTH_BAR && dwarfs {
        let f = make_finding(
            table,
            top,
            runner,
            target_name,
            target_categorical,
            false,
            0,
            false,
        );
        return (
            vec![f],
            format!(
                "{head}{declared_clause}; strongest '{}' at {:.3} clears the {STRENGTH_BAR:.2} bar and dwarfs the field ({runner_txt}); 1 stated",
                table.names[top.ci], top.value
            ),
        );
    }
    let reason = if top.value < STRENGTH_BAR {
        format!(
            "strongest '{}' at {:.3} is below the {STRENGTH_BAR:.2} bar ({runner_txt})",
            table.names[top.ci], top.value
        )
    } else {
        format!(
            "strongest '{}' at {:.3} does not dwarf the field ({runner_txt}; ratio required {DWARF_RATIO}x)",
            table.names[top.ci], top.value
        )
    };
    (
        Vec::new(),
        format!("{head}{declared_clause}; no leakage signature: {reason}"),
    )
}

#[allow(clippy::too_many_arguments)]
fn make_finding(
    table: &TypedTable,
    s: &Strength,
    runner: Option<&Strength>,
    target_name: &str,
    target_categorical: bool,
    determination: bool,
    n_determiners: usize,
    declared: bool,
) -> Finding {
    let pred = &table.names[s.ci];
    let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    params.insert("columns".into(), json!([pred.clone(), target_name]));
    params.insert("inputs".into(), json!([pred.clone()]));
    params.insert("target".into(), json!(target_name));
    params.insert("column".into(), json!(target_name));

    let mut metrics: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    metrics.insert("strength".into(), json!(round3(s.value)));
    metrics.insert("strength_measure".into(), json!(s.measure.label()));
    metrics.insert("n_used".into(), json!(s.n_used));
    metrics.insert("determination".into(), json!(determination));
    if declared {
        metrics.insert("declared_suspect".into(), json!(true));
    }
    if let Some(v) = s.violations {
        metrics.insert("violations".into(), json!(v));
    }
    if let Some(r) = runner {
        metrics.insert("runner_up_column".into(), json!(table.names[r.ci]));
        metrics.insert("runner_up_strength".into(), json!(round3(r.value)));
        if r.value > 0.0 {
            metrics.insert("strength_ratio".into(), json!(round3(s.value / r.value)));
        }
    }

    let strength_clause = match s.measure {
        Measure::EntropyFraction => format!(
            "it accounts for {:.0}% of the target's entropy on its own, after the chance correction",
            100.0 * s.value
        ),
        Measure::Xi => format!(
            "its directed dependence on the target is {:.2} on its own (Chatterjee's xi)",
            s.value
        ),
        Measure::Eta2Adjusted => format!(
            "it accounts for {:.0}% of the target's variance between its groups, after the degrees-of-freedom adjustment",
            100.0 * s.value
        ),
    };
    let runner_clause = runner.map_or_else(
        || "no other column was evaluated".to_string(),
        |r| {
            format!(
                "next strongest column: '{}' at {:.3}",
                table.names[r.ci], r.value
            )
        },
    );
    let (claim, score) = if determination {
        let exact_clause = if target_categorical {
            format!(
                "every value of '{pred}' pairs with a single value of '{target_name}' on the {} evaluated rows",
                s.n_used
            )
        } else {
            format!(
                "the target is an exact function of '{pred}' on the {} evaluated rows",
                s.n_used
            )
        };
        let co = if n_determiners > 1 {
            format!(" {n_determiners} columns determine the target this way; each is stated.")
        } else {
            String::new()
        };
        if declared {
            (
                format!(
                    "'{pred}' — declared by the caller as recorded after the outcome — determines the target '{target_name}' exactly: {exact_clause}. The declaration names the mechanism and the measurement confirms it: check it before modelling.{co}"
                ),
                0.97,
            )
        } else {
            (
                format!(
                    "'{pred}' determines the target '{target_name}' exactly: {exact_clause}. A column that restates the target this way is usually derived from the outcome or recorded after it — check it before modelling.{co}"
                ),
                0.95,
            )
        }
    } else if declared {
        (
            format!(
                "'{pred}' — declared by the caller as recorded after the outcome — nearly determines the target '{target_name}': {strength_clause} ({runner_clause}). The declaration names the mechanism and the measurement confirms the strength: check it before modelling."
            ),
            0.9,
        )
    } else {
        (
            format!(
                "'{pred}' nearly determines the target '{target_name}': {strength_clause} ({runner_clause}). A column this strong, out of line with the rest of the field, is usually derived from the outcome or recorded after it — check it before modelling."
            ),
            0.88,
        )
    };
    Finding::new(
        Pattern::LeakageSignature,
        params,
        claim,
        EvidenceBasis::Measured,
        score,
        metrics,
        Some(format!(
            "before modelling '{target_name}', establish when '{pred}' becomes known: a column recorded after the outcome, or computed from it, must not be used as a predictor"
        )),
        Method::new(
            "leakage",
            "univariate strength screen against the designated target (chance-corrected entropy fraction / Chatterjee xi / adjusted eta-squared)",
            "onceover leakage screen v1 (issue #282)",
        ),
    )
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}
