//! Consolidate equivalent statements, rank their evidence, and select a report.
//!
//! Ranking contract (frame-model step 2): selection consumes
//! candidates in demotion-adjusted score order, exactly as before
//! — no budget changes, and no finding a dataset earns is lost to
//! the re-ranking. The selected findings are then presented in
//! report-band order (see `report_band`: account, completeness,
//! relations, column structure, deviations), score within a band.
//! Demotions are the one ranking force that also reaches
//! selection, through the lowered score.
//!
//! Scoring contract: a finding arrives with the producing detector's
//! own interestingness. Selection replaces `interestingness` with the
//! selection score — the mean of the detector's score and the ledger's
//! evidence score below — and records the detector's score in
//! `detector_interestingness`, so the replacement is never silent.
//! Triviality demotions apply on top. A producer that knows a
//! finding restates the file's own structure may declare it in the
//! evidence metrics (`triviality` = reason, `triviality_factor`);
//! the ledger adds its own structural rule — a functional dependency
//! whose inputs contain an exact unique key is implied by that key
//! (`implied_by_unique_key`, 0.6). All applicable factors multiply,
//! and the consolidated reasons and factor are written back to every
//! evidence record. Nothing is dropped for triviality; demoted
//! findings stay in the report, ranked lower, with the reasons on
//! record.
//!
//! Families (issue #116, schema 0.6): statements that restate one
//! exact verified relation are consolidated before the caps run —
//! one lead is selected, the rest are returned as grouped findings
//! that spend no selection slot (see `form_families` for the seed
//! cases and the rotation test). Grouping never removes a
//! statement from the report; it moves it to `grouped_findings`.
use crate::{Category, Evidence, EvidenceBasis, Finding, FindingGroup, Sampling, Scope};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SelectionSummary {
    pub candidates: usize,
    pub verified_candidates: usize,
    pub distinct_statements: usize,
    pub selected: usize,
    /// Verified relationship families whose lead was selected
    /// (schema 0.6); their member statements are in the report's
    /// `grouped_findings`, counted by `grouped_statements`.
    #[serde(default)]
    pub families: usize,
    #[serde(default)]
    pub grouped_statements: usize,
}

/// The result of selection: the selected findings in presentation
/// order, the statements grouped under family leads (also in
/// presentation order), how many statements each per-detector cap
/// turned away, and how many families were selected.
#[derive(Clone, Debug)]
pub struct SelectionOutcome {
    pub selected: Vec<Finding>,
    pub grouped: Vec<Finding>,
    pub capped: BTreeMap<String, usize>,
    pub families: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelectionPolicy {
    pub max_findings: usize,
    pub min_rule_support: usize,
    pub min_rule_antecedent: usize,
    pub min_rule_gain: f64,
    pub min_rule_lift: f64,
    pub max_rules: usize,
    pub rules_per_outcome: usize,
    pub predictive_findings_per_target: usize,
    /// Per-detector caps on selected statements, attributed by the
    /// detector of a statement's first evidence record. The first two
    /// defaults bound the report's context cost where the 2026-11-02
    /// battery measured it: functional_dependencies and gbm together
    /// produced ~41% of all report characters (550,443 and 467,098 of
    /// 2,479,798). The discontinuities cap (25) follows the 2026-11-04
    /// battery (docs/benchmarks/2026-11-04-detector-battery-step2.md):
    /// its statements score in a narrow high band (0.83–0.93), so the
    /// score ranking cannot ration them against other detectors, and
    /// on the state-unemployment panel they took 81 of the 100 slots,
    /// crowding out every gbm and numeric_dependence statement. A
    /// statement past its detector's cap is not selected; the counts
    /// of capped statements are returned by
    /// `select_findings_counted` and reported in coverage, so a cap
    /// is never silent. Detectors absent from the map are uncapped.
    pub max_findings_per_detector: BTreeMap<String, usize>,
}
impl Default for SelectionPolicy {
    fn default() -> Self {
        Self {
            max_findings: 100,
            min_rule_support: 20,
            min_rule_antecedent: 30,
            min_rule_gain: 0.02,
            min_rule_lift: 1.5,
            max_rules: 20,
            rules_per_outcome: 2,
            predictive_findings_per_target: 3,
            max_findings_per_detector: BTreeMap::from([
                ("functional_dependencies".to_string(), 25),
                ("gbm".to_string(), 20),
                ("discontinuities".to_string(), 25),
                ("missingness".to_string(), 20),
            ]),
        }
    }
}
fn metric(e: &Evidence, key: &str) -> Option<f64> {
    e.metrics.get(key).and_then(Value::as_f64)
}
fn quality(e: &Evidence) -> f64 {
    match e.validation.method.as_str() {
        "direct_comparison" => {
            if e.scope.sampling == Sampling::Full {
                1.0
            } else {
                0.85
            }
        }
        "cross_validation" | "diagnostic_holdout" | "diagnostic_partition" => 0.8,
        _ => match e.basis {
            EvidenceBasis::Measured => 0.85,
            EvidenceBasis::Estimated => 0.6,
            EvidenceBasis::Inferred => 0.4,
        },
    }
}
/// The ledger's evidence score: how well-supported and strong the
/// statement is, from its evidence alone (quality of validation,
/// effect strength, specificity, simplicity). It contributes half of
/// the selection score; the detector's own score contributes the rest.
fn score(f: &Finding) -> f64 {
    let mut best: f64 = 0.0;
    for e in &f.evidence {
        if f.category == Category::ConditionalRules && !e.metrics.contains_key("conditional_rate") {
            continue;
        }
        let strength = match f.category {
            Category::ConditionalRules => {
                let p = metric(e, "conditional_rate")
                    .or_else(|| metric(e, "holdout_confidence"))
                    .or_else(|| metric(e, "precision"))
                    .unwrap_or(0.0);
                let base = metric(e, "baseline_rate").unwrap_or(0.5);
                let gain = ((p - base) / (1.0 - base).max(0.01)).clamp(0.0, 1.0);
                let support = metric(e, "joint_count")
                    .or_else(|| metric(e, "holdout_joint_count"))
                    .or_else(|| metric(e, "support"))
                    .unwrap_or(0.0);
                (0.5 * gain
                    + 0.5 * ((metric(e, "lift").unwrap_or(1.0) - 1.0) / 4.0).clamp(0.0, 1.0))
                    * (support / 30.0).min(1.0)
            }
            Category::Dependencies if f.finding_type.as_deref() == Some("predictive") => {
                let r2 = metric(e, "holdout_r2").or_else(|| {
                    (e.metrics.get("task").and_then(Value::as_str) == Some("regression"))
                        .then(|| metric(e, "metric"))
                        .flatten()
                });
                r2.unwrap_or_else(|| {
                    let p = metric(e, "cv_accuracy")
                        .or_else(|| metric(e, "metric"))
                        .unwrap_or(0.0);
                    let base = metric(e, "baseline").unwrap_or(0.0);
                    (p - base) / (1.0 - base).max(0.01)
                })
                .clamp(0.0, 1.0)
            }
            Category::Dependencies => {
                let n = e.scope.rows_evaluated.unwrap_or(1).max(1) as f64;
                1.0 - metric(e, "violations").unwrap_or(0.0) / n
            }
            Category::Uniqueness => 1.0,
            Category::Associations => ["pearson", "spearman", "xi_ab", "xi_ba"]
                .into_iter()
                .filter_map(|k| metric(e, k))
                .map(f64::abs)
                .fold(0.0, f64::max),
            Category::Concentration => metric(e, "top_share").unwrap_or(0.7),
            _ => 0.75,
        };
        let specificity = if f.specification.form.is_some() {
            1.0
        } else {
            0.5
        };
        let simplicity =
            1.0 / (1.0 + (f.subject.inputs.len() + f.specification.conditions.len()) as f64 / 8.0);
        best = best.max(
            0.45 * quality(e)
                + 0.35 * strength.clamp(0.0, 1.0)
                + 0.1 * specificity
                + 0.1 * simplicity,
        );
    }
    crate::round4(best)
}

/// The report band of a finding (frame-model step 2): the
/// presentation order of the selected findings — account first
/// (grain and key statements), then completeness and frame
/// integrity, then relations, then column structure, then
/// deviations; the selection score orders findings within a band.
/// The bands are the digest order stated as a ranking of the
/// report body: a deviation is presented after the account it
/// deviates from, however strong its detector score, and a
/// relation after the key and completeness statements that frame
/// it. Selection is by score and does not consult the band.
/// Frame-scope findings (the account's own statements: the
/// factorized grain, the near key) are account content whatever
/// their category.
pub fn report_band(f: &Finding) -> u8 {
    if f.facets.scope == Scope::Frame {
        return 0;
    }
    match f.category {
        Category::Uniqueness => 0,
        Category::Missingness | Category::Gaps | Category::Duplicates => 1,
        Category::Dependencies | Category::Associations | Category::ConditionalRules => 2,
        Category::ColumnRoles
        | Category::Formats
        | Category::Ordering
        | Category::Concentration => 3,
        Category::Changes | Category::Intervals => 4,
    }
}

/// The exact-relation kind of a statement, when the library's own
/// checks verify it exactly (issue #116). Families form only over
/// these statements. Functional dependencies, arithmetic identities
/// and affine dependencies share the `functional` finding type;
/// the producing detector and algorithm separate them. A statement
/// carrying conditions is scoped — it holds only within a slice —
/// and is never an exact-relation candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExactKind {
    /// An exact functional dependency (0 violations recorded by
    /// the functional_dependencies pass).
    Fd,
    /// An exact multi-term identity (identities pass,
    /// multiterm_scan, 0 violations).
    Identity,
    /// An exact affine pair dependency (identities pass,
    /// ols_residual_scan, 0 violations).
    Affine,
    /// An exact duplicate encoding: two columns paired one-to-one
    /// (duplicate_encodings pass, `exact` true).
    Bijection,
    /// An exact rollup: a finer column mapping one-to-one onto a
    /// coarser one (duplicate_encodings pass, `exceptions` 0).
    Rollup,
}

fn exact_kind(f: &Finding) -> Option<ExactKind> {
    if !f.specification.conditions.is_empty()
        || f.facets.conditionality != crate::Conditionality::Unconditional
    {
        return None;
    }
    let zero_violations = |detector: &str, algorithm: Option<&str>| {
        f.evidence.iter().any(|e| {
            e.method.detector == detector
                && algorithm.is_none_or(|a| e.method.algorithm == a)
                && metric(e, "violations").is_some_and(|v| v == 0.0)
        })
    };
    match f.finding_type.as_deref() {
        Some("duplicate_encoding") => match f.specification.form.as_deref() {
            Some("duplicate_encoding") => f
                .evidence
                .iter()
                .any(|e| e.metrics.get("exact").and_then(Value::as_bool) == Some(true))
                .then_some(ExactKind::Bijection),
            Some("rollup") => f
                .evidence
                .iter()
                .any(|e| metric(e, "exceptions").is_some_and(|v| v == 0.0))
                .then_some(ExactKind::Rollup),
            _ => None,
        },
        Some("functional") => {
            if zero_violations("functional_dependencies", None) {
                Some(ExactKind::Fd)
            } else if zero_violations("identities", Some("multiterm_scan")) {
                Some(ExactKind::Identity)
            } else if zero_violations("identities", Some("ols_residual_scan")) {
                Some(ExactKind::Affine)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The linear relation a statement asserts, as (weights,
/// intercept): the equation `sum_c weights[c] * c + intercept = 0`
/// over its columns, recovered from the producing pass's own
/// recorded coefficients. Multi-term identities record `terms`
/// (the predictors) and `coefficients` aligned with them — for
/// `linear_combination` the vector is [intercept, c1, ..., ck],
/// for sum/difference the unit/sign multipliers. An affine pair
/// records slope `a` and intercept `b` for target = a * input + b.
/// Product and ratio identities are not linear and return None,
/// as does a statement whose coefficients were not recorded.
fn linear_weights(f: &Finding) -> Option<(BTreeMap<String, f64>, f64)> {
    let target = f.subject.target.as_ref()?;
    for e in &f.evidence {
        if e.method.detector != "identities" {
            continue;
        }
        if e.method.algorithm == "multiterm_scan" {
            let form = e
                .metrics
                .get("form")
                .and_then(Value::as_str)
                .or(f.specification.form.as_deref())?;
            if form == "product" || form == "ratio" {
                return None;
            }
            let terms: Vec<String> = e
                .metrics
                .get("terms")?
                .as_array()?
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
            let coeffs: Vec<f64> = e
                .metrics
                .get("coefficients")?
                .as_array()?
                .iter()
                .filter_map(Value::as_f64)
                .collect();
            let (intercept, aligned): (f64, &[f64]) = if form == "linear_combination" {
                if coeffs.len() != terms.len() + 1 {
                    return None;
                }
                (coeffs[0], &coeffs[1..])
            } else {
                if coeffs.len() != terms.len() {
                    return None;
                }
                (0.0, &coeffs[..])
            };
            let mut weights: BTreeMap<String, f64> =
                terms.into_iter().zip(aligned.iter().copied()).collect();
            weights.insert(target.clone(), -1.0);
            return Some((weights, intercept));
        }
        if e.method.algorithm == "ols_residual_scan" {
            let a = metric(e, "a")?;
            let b = metric(e, "b")?;
            let x = f.subject.inputs.first()?;
            let mut weights = BTreeMap::new();
            weights.insert(x.clone(), a);
            weights.insert(target.clone(), -1.0);
            return Some((weights, b));
        }
    }
    None
}

/// Whether two linear relations are the same relation rearranged:
/// their (weights, intercept) vectors are proportional. The
/// tolerance is relative and far above the fit noise the passes
/// record (coefficients land within ~1e-12 of the exact values),
/// so genuinely different relations — different coefficients, a
/// different intercept — never test proportional.
fn proportional(w1: &(BTreeMap<String, f64>, f64), w2: &(BTreeMap<String, f64>, f64)) -> bool {
    let (m1, b1) = w1;
    let (m2, b2) = w2;
    let mut pivot: Option<(&String, f64)> = None;
    for (c, v) in m1 {
        if pivot.is_none_or(|(_, pv)| v.abs() > pv.abs()) {
            pivot = Some((c, *v));
        }
    }
    let Some((p, pv)) = pivot else { return false };
    if pv == 0.0 {
        return false;
    }
    let r = m2.get(p).copied().unwrap_or(0.0) / pv;
    if r == 0.0 {
        return false;
    }
    let close = |x: f64, y: f64| (x - y).abs() <= 1e-6 * x.abs().max(y.abs()).max(1.0);
    let columns: BTreeSet<&String> = m1.keys().chain(m2.keys()).collect();
    columns.iter().all(|c| {
        close(
            m2.get(*c).copied().unwrap_or(0.0),
            r * m1.get(*c).copied().unwrap_or(0.0),
        )
    }) && close(*b2, r * b1)
}

/// A verified relationship family: one lead statement and the
/// statements that restate the same exact relation, keyed by
/// canonical statement key.
struct Family {
    lead: String,
    members: Vec<String>,
}

/// Build the verified relationship families over the ranked
/// statements (issue #116). A family is seeded only by exact
/// verified relations:
///
/// - an exact duplicate encoding (a bijection) gathers the exact
///   FDs over its column pair, in both directions;
/// - an exact rollup gathers the exact FD in its own direction
///   over its pair;
/// - an exact identity gathers the statements that assert the
///   same relation rearranged — rotations, tested on the recorded
///   coefficient vectors, so a sum, its difference rearrangements
///   and a fitted linear combination of the same equation form
///   one class, while a different relation over the same columns
///   (different coefficients, a different intercept, a sum and a
///   product that both hold) never merges with it — and the exact
///   FDs whose columns are exactly the identity's columns attach
///   to the first class family over the set: an FD over an
///   identity's columns restates the identity, since each column
///   is then a function of the others by the identity itself;
/// - an exact affine pair gathers the exact FDs over its pair
///   (affine statements join the same rotation classes: a pair's
///   reverse affine is proportional to it);
/// - a set of exact FDs over one column set whose directions
///   cover every column (each column is the target of an FD on
///   the rest) is the FD signature of one underlying exact
///   relation and forms a family on its own.
///
/// Predictions are never members: a predictive statement is not
/// equivalent to an exact mapping. Scoped statements (conditions,
/// value-conditioned facets) are never members. A statement joins
/// at most one family; seeds are considered in the order above,
/// and within a seed kind in ranked order, so the construction is
/// deterministic. A family with no members is not a family.
fn form_families(ranked: &[Finding]) -> Vec<Family> {
    let kind_of: BTreeMap<String, ExactKind> = ranked
        .iter()
        .filter_map(|f| exact_kind(f).map(|k| (f.canonical_key(), k)))
        .collect();
    let by_key: BTreeMap<String, &Finding> =
        ranked.iter().map(|f| (f.canonical_key(), f)).collect();
    let columns_of =
        |key: &str| -> BTreeSet<String> { by_key[key].subject.columns.iter().cloned().collect() };
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    let mut families: Vec<Family> = Vec::new();
    let push_family = |families: &mut Vec<Family>,
                       claimed: &mut BTreeSet<String>,
                       lead: String,
                       members: Vec<String>| {
        if members.is_empty() {
            return;
        }
        claimed.insert(lead.clone());
        for m in &members {
            claimed.insert(m.clone());
        }
        families.push(Family { lead, members });
    };
    // Exact FDs over a column set, in ranked order.
    let fds_over = |set: &BTreeSet<String>, claimed: &BTreeSet<String>| -> Vec<String> {
        ranked
            .iter()
            .map(|f| f.canonical_key())
            .filter(|k| {
                kind_of.get(k) == Some(&ExactKind::Fd)
                    && !claimed.contains(k)
                    && &columns_of(k) == set
            })
            .collect()
    };

    // (a) Bijections: the duplicate-encoding statement leads; the
    // FDs in both directions over the pair are its restatements.
    for f in ranked {
        let key = f.canonical_key();
        if kind_of.get(&key) != Some(&ExactKind::Bijection) || claimed.contains(&key) {
            continue;
        }
        let members = fds_over(&columns_of(&key), &claimed);
        push_family(&mut families, &mut claimed, key, members);
    }
    // (b) Rollups: the rollup statement leads; the exact FD in the
    // rollup's own direction (finer determines coarser) restates it.
    for f in ranked {
        let key = f.canonical_key();
        if kind_of.get(&key) != Some(&ExactKind::Rollup) || claimed.contains(&key) {
            continue;
        }
        let (finer, coarser) = f
            .evidence
            .iter()
            .find_map(|e| {
                let finer = e.metrics.get("finer").and_then(Value::as_str)?;
                let coarser = e.metrics.get("coarser").and_then(Value::as_str)?;
                Some((finer.to_string(), coarser.to_string()))
            })
            .unwrap_or_default();
        let set = columns_of(&key);
        let members: Vec<String> = fds_over(&set, &claimed)
            .into_iter()
            .filter(|k| {
                let mf = by_key[k];
                mf.subject.target.as_deref() == Some(coarser.as_str())
                    && mf.subject.inputs.len() == 1
                    && mf.subject.inputs[0] == finer
            })
            .collect();
        push_family(&mut families, &mut claimed, key, members);
    }
    // (c) Exact identities and affine dependencies over one column
    // set, partitioned into rotation classes by the relations they
    // assert: two linear statements are rotations of one relation
    // exactly when their recorded coefficient vectors (weights and
    // intercept) are proportional, whatever surface form the scan
    // reported (a sum, a difference, or a fitted linear combination
    // of the rearranged terms). Distinct relations over the same
    // columns — different coefficients, a different intercept, a
    // sum and a product that both happen to hold — land in
    // different classes and are never merged. Product/ratio
    // identities share one multiplicative class per column set.
    // The first class family over a set also gathers the exact
    // FDs over the set: an FD over an identity's columns restates
    // the identity, since each column is then a function of the
    // others by the identity itself.
    let rank_of: BTreeMap<String, usize> = ranked
        .iter()
        .enumerate()
        .map(|(i, f)| (f.canonical_key(), i))
        .collect();
    enum ClassKind {
        Multiplicative,
        Linear(BTreeMap<String, f64>, f64),
        Unrecoverable,
    }
    let mut relation_groups: BTreeMap<BTreeSet<String>, Vec<String>> = BTreeMap::new();
    for f in ranked {
        let key = f.canonical_key();
        if matches!(
            kind_of.get(&key),
            Some(ExactKind::Identity) | Some(ExactKind::Affine)
        ) && !claimed.contains(&key)
        {
            relation_groups
                .entry(columns_of(&key))
                .or_default()
                .push(key);
        }
    }
    for (set, group) in &relation_groups {
        let mut classes: Vec<(ClassKind, Vec<String>)> = Vec::new();
        for key in group {
            let f = by_key[key];
            let multiplicative = matches!(
                f.specification.form.as_deref(),
                Some("product") | Some("ratio")
            );
            if multiplicative {
                match classes
                    .iter_mut()
                    .find(|(kind, _)| matches!(kind, ClassKind::Multiplicative))
                {
                    Some((_, members)) => members.push(key.clone()),
                    None => classes.push((ClassKind::Multiplicative, vec![key.clone()])),
                }
                continue;
            }
            match linear_weights(f) {
                Some(weights) => {
                    let mut placed = false;
                    for (kind, members) in classes.iter_mut() {
                        if let ClassKind::Linear(w, b) = kind {
                            if proportional(&(w.clone(), *b), &weights) {
                                members.push(key.clone());
                                placed = true;
                                break;
                            }
                        }
                    }
                    if !placed {
                        classes.push((ClassKind::Linear(weights.0, weights.1), vec![key.clone()]));
                    }
                }
                None => classes.push((ClassKind::Unrecoverable, vec![key.clone()])),
            }
        }
        // Classes compete in the order of their best-ranked member.
        classes.sort_by_key(|(_, members)| {
            members
                .iter()
                .map(|k| rank_of[k])
                .min()
                .unwrap_or(usize::MAX)
        });
        let mut fds_attached = false;
        for (_, members) in &classes {
            let lead = members
                .iter()
                .min_by_key(|k| {
                    let form_rank = match by_key[*k].specification.form.as_deref() {
                        Some("sum") | Some("product") => 0,
                        Some("difference") | Some("ratio") => 1,
                        Some("linear_combination") => 2,
                        _ => 3,
                    };
                    (form_rank, rank_of[*k])
                })
                .expect("class is non-empty")
                .clone();
            if claimed.contains(&lead) {
                continue;
            }
            let mut rest: Vec<String> = members
                .iter()
                .filter(|k| **k != lead && !claimed.contains(*k))
                .cloned()
                .collect();
            if !fds_attached {
                rest.extend(fds_over(set, &claimed));
                fds_attached = true;
            }
            push_family(&mut families, &mut claimed, lead, rest);
        }
    }
    // (e) FD rotation clusters: exact FDs over one column set
    // whose directions cover every column.
    let mut fd_groups: BTreeMap<BTreeSet<String>, Vec<String>> = BTreeMap::new();
    for f in ranked {
        let key = f.canonical_key();
        if kind_of.get(&key) == Some(&ExactKind::Fd) && !claimed.contains(&key) {
            fd_groups.entry(columns_of(&key)).or_default().push(key);
        }
    }
    for (set, group) in &fd_groups {
        if set.len() < 2 || group.len() < 2 {
            continue;
        }
        let covered = set.iter().all(|c| {
            group.iter().any(|k| {
                let mf = by_key[k];
                mf.subject.target.as_deref() == Some(c.as_str())
                    && mf.subject.inputs.iter().cloned().collect::<BTreeSet<_>>()
                        == set.iter().filter(|x| *x != c).cloned().collect()
            })
        });
        if !covered {
            continue;
        }
        let lead = group[0].clone();
        let members: Vec<String> = group[1..].to_vec();
        push_family(&mut families, &mut claimed, lead, members);
    }
    families
}

fn assign_id(f: &mut Finding) {
    // IDs depend on the statement, never its rank or detector enumeration.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in f.canonical_key().bytes() {
        hash = (hash ^ byte as u64).wrapping_mul(0x100000001b3);
    }
    f.id = format!("{}-{hash:016x}", f.category.as_str());
}

/// The standing admission checks of the selection loop, shared by
/// ordinary statements and by family leads admitted early (when a
/// member is reached first): the conditional-rule caps, the
/// per-target predictive cap, and the per-detector report caps.
/// Counters advance exactly as the standing loop advanced them —
/// a rule's outcome is counted before the per-detector check, so
/// a detector-capped rule has still spent its outcome slot. On
/// success the finding is assigned its id and pushed; the id is
/// returned.
struct Admission<'a> {
    policy: &'a SelectionPolicy,
    rules: usize,
    outcomes: BTreeMap<String, usize>,
    targets: BTreeMap<String, usize>,
    per_detector: BTreeMap<String, usize>,
    capped: BTreeMap<String, usize>,
}

impl Admission<'_> {
    fn admit(&mut self, mut f: Finding, selected: &mut Vec<Finding>) -> Option<String> {
        if f.category == Category::ConditionalRules {
            if self.rules >= self.policy.max_rules {
                return None;
            }
            let outcome = serde_json::to_string(&f.specification.outcome).unwrap();
            let count = self.outcomes.entry(outcome).or_default();
            if *count >= self.policy.rules_per_outcome {
                return None;
            }
            *count += 1;
            self.rules += 1;
        }
        if f.is(Category::Dependencies, Some("predictive")) {
            let target = f.subject.target.clone().unwrap_or_default();
            let count = self.targets.entry(target).or_default();
            if *count >= self.policy.predictive_findings_per_target {
                return None;
            }
            *count += 1;
        }
        // Per-detector report caps: a statement is attributed to the
        // detector of its first evidence record (evidence was sorted
        // above, so the attribution is deterministic). Statements
        // past the cap are counted, not silently dropped: the caller
        // reports the counts in coverage.
        if let Some(detector) = f.evidence.first().map(|e| e.method.detector.clone()) {
            if let Some(&cap) = self.policy.max_findings_per_detector.get(&detector) {
                let count = self.per_detector.entry(detector.clone()).or_default();
                if *count >= cap {
                    *self.capped.entry(detector).or_default() += 1;
                    return None;
                }
                *count += 1;
            }
        }
        assign_id(&mut f);
        let id = f.id.clone();
        selected.push(f);
        Some(id)
    }
}

pub fn finalize_ledger(findings: Vec<Finding>) -> Vec<Finding> {
    select_findings(findings, &SelectionPolicy::default())
}

pub fn select_findings(findings: Vec<Finding>, policy: &SelectionPolicy) -> Vec<Finding> {
    select_findings_counted(findings, policy).selected
}

/// Selection, additionally returning the statements grouped under
/// verified relationship families (issue #116) and how many
/// statements each per-detector cap in the policy turned away
/// (detector -> count). Callers that assemble a report surface the
/// cap counts in coverage so the caps are disclosed, never silent.
pub fn select_findings_counted(
    findings: Vec<Finding>,
    policy: &SelectionPolicy,
) -> SelectionOutcome {
    let mut statements = BTreeMap::<String, Finding>::new();
    let mut detector_scores = BTreeMap::<String, f64>::new();
    for mut finding in findings {
        finding.subject.canonicalize();
        if finding.validate().is_err() {
            continue;
        }
        let key = finding.canonical_key();
        // The detector's score for a statement is the strongest claim
        // any merged candidate arrived with.
        let entry = detector_scores.entry(key.clone()).or_insert(0.0);
        *entry = entry.max(finding.interestingness);
        if let Some(existing) = statements.get_mut(&key) {
            // Keep a statement description from the best evaluated candidate;
            // the metrics remain separate and are never averaged or pooled.
            let existing_rows = existing
                .evidence
                .iter()
                .filter_map(|e| e.scope.rows_evaluated)
                .max()
                .unwrap_or(0);
            let incoming_rows = finding
                .evidence
                .iter()
                .filter_map(|e| e.scope.rows_evaluated)
                .max()
                .unwrap_or(0);
            if incoming_rows > existing_rows {
                existing.claim = finding.claim;
            }
            existing.evidence.extend(finding.evidence);
            if existing.interpretation.is_none() {
                existing.interpretation = finding.interpretation;
            }
        } else {
            statements.insert(key, finding);
        }
    }
    let mut ranked: Vec<_> = statements.into_values().collect();
    // Exact unique keys among the candidates (uniqueness statements
    // whose evidence records no duplicated values; near-unique "dirty"
    // keys do not imply anything). A functional dependency whose inputs
    // contain such a key holds by construction.
    let key_sets: Vec<BTreeSet<String>> = ranked
        .iter()
        .filter(|f| f.category == Category::Uniqueness)
        .filter(|f| {
            f.evidence
                .iter()
                .all(|e| metric(e, "duplicates").is_none_or(|d| d == 0.0))
        })
        .map(|f| f.subject.columns.iter().cloned().collect())
        .collect();
    for f in &mut ranked {
        f.evidence
            .sort_by_key(|e| serde_json::to_string(e).unwrap());
        f.evidence.dedup();
        let detector = detector_scores
            .get(&f.canonical_key())
            .copied()
            .unwrap_or(f.interestingness);
        f.detector_interestingness = Some(detector);
        let mut combined = 0.5 * detector + 0.5 * score(f);
        let key_implied = f.category == Category::Dependencies
            && f.finding_type.as_deref() == Some("functional")
            && !f.subject.inputs.is_empty()
            && f.subject.target.as_deref().is_some_and(|t| {
                key_sets
                    .iter()
                    .any(|ks| !ks.contains(t) && ks.iter().all(|c| f.subject.inputs.contains(c)))
            });
        // Triviality demotions: producer-declared (reason -> factor,
        // first declaration of a reason wins) plus the ledger's own
        // key-implied rule. Factors multiply; the consolidated record
        // is written to every evidence record of the finding.
        let mut demotions: BTreeMap<String, f64> = BTreeMap::new();
        for e in &f.evidence {
            if let (Some(r), Some(x)) = (
                e.metrics.get("triviality").and_then(Value::as_str),
                e.metrics.get("triviality_factor").and_then(Value::as_f64),
            ) {
                demotions.entry(r.to_string()).or_insert(x);
            }
        }
        if key_implied {
            demotions.insert("implied_by_unique_key".into(), 0.6);
        }
        if !demotions.is_empty() {
            let factor: f64 = demotions.values().product();
            combined *= factor;
            let reasons = demotions.keys().cloned().collect::<Vec<_>>().join("+");
            let rounded = crate::round4(factor);
            for e in &mut f.evidence {
                e.metrics.insert("triviality".into(), json!(reasons));
                e.metrics.insert("triviality_factor".into(), json!(rounded));
            }
        }
        f.interestingness = crate::round4(combined);
    }
    // Selection order is the standing one: demotion-adjusted
    // interestingness, canonical key as the tiebreak. The caps
    // below consume candidates in this order, so the report keeps
    // the findings it earns under the standing budgets — frame-
    // model step 2 changes no budget and no selection outcome
    // beyond the demotions themselves (a demoted finding's lower
    // score can cost it a capped slot; that is the demotion
    // mechanism working as designed).
    ranked.sort_by(|a, b| {
        b.interestingness
            .total_cmp(&a.interestingness)
            .then(a.canonical_key().cmp(&b.canonical_key()))
    });
    // Verified relationship families (issue #116) form over the
    // ranked statements before the caps run. A family competes at
    // its best-ranked member's position: the first member reached
    // triggers the lead's admission attempt through the standing
    // checks. If the lead is admitted, the remaining members are
    // grouped under it — full records, no selection slot, no
    // digest slot. If the lead is turned away, the family does not
    // form and every member competes on its own, exactly as before
    // families existed. Non-family statements are untouched by the
    // mechanism: the admission checks, their order, and the cap
    // counters are the standing ones (see `Admission`).
    let families = form_families(&ranked);
    let member_of: BTreeMap<String, usize> = families
        .iter()
        .enumerate()
        .flat_map(|(i, fam)| fam.members.iter().map(move |m| (m.clone(), i)))
        .collect();
    let lead_of: BTreeMap<String, usize> = families
        .iter()
        .enumerate()
        .map(|(i, fam)| (fam.lead.clone(), i))
        .collect();
    #[derive(Clone, Copy, PartialEq)]
    enum FamState {
        LeadSelected,
        LeadFailed,
    }
    let mut fam_state: BTreeMap<usize, FamState> = BTreeMap::new();
    let mut lead_ids: BTreeMap<usize, String> = BTreeMap::new();
    let mut grouped_member_ids: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let order: Vec<String> = ranked.iter().map(|f| f.canonical_key()).collect();
    let mut by_key: BTreeMap<String, Finding> =
        ranked.into_iter().map(|f| (f.canonical_key(), f)).collect();
    let mut admission = Admission {
        policy,
        rules: 0,
        outcomes: BTreeMap::new(),
        targets: BTreeMap::new(),
        per_detector: BTreeMap::new(),
        capped: BTreeMap::new(),
    };
    let mut selected: Vec<Finding> = Vec::new();
    let mut grouped: Vec<Finding> = Vec::new();
    for key in &order {
        if selected.len() >= policy.max_findings {
            break;
        }
        if let Some(&fi) = member_of.get(key) {
            match fam_state.get(&fi) {
                Some(FamState::LeadSelected) => {
                    let mut f = by_key.remove(key).expect("grouped member present");
                    assign_id(&mut f);
                    f.grouped_under = Some(lead_ids[&fi].clone());
                    grouped_member_ids.entry(fi).or_default().push(f.id.clone());
                    grouped.push(f);
                    continue;
                }
                Some(FamState::LeadFailed) => {}
                None => {
                    // First member reached: the family competes
                    // here, through its lead.
                    let lead_key = families[fi].lead.clone();
                    let lead = by_key.remove(&lead_key).expect("family lead present");
                    match admission.admit(lead, &mut selected) {
                        Some(id) => {
                            fam_state.insert(fi, FamState::LeadSelected);
                            lead_ids.insert(fi, id);
                            let mut f = by_key.remove(key).expect("grouped member present");
                            assign_id(&mut f);
                            f.grouped_under = Some(lead_ids[&fi].clone());
                            grouped_member_ids.entry(fi).or_default().push(f.id.clone());
                            grouped.push(f);
                            continue;
                        }
                        None => {
                            fam_state.insert(fi, FamState::LeadFailed);
                        }
                    }
                }
            }
        }
        let Some(f) = by_key.remove(key) else {
            // A lead admitted early, at its first member's
            // position; its own position is spent.
            continue;
        };
        let is_lead = lead_of.get(key).copied();
        match admission.admit(f, &mut selected) {
            Some(id) => {
                if let Some(fi) = is_lead {
                    fam_state.insert(fi, FamState::LeadSelected);
                    lead_ids.insert(fi, id);
                }
            }
            None => {
                if let Some(fi) = is_lead {
                    fam_state.insert(fi, FamState::LeadFailed);
                }
            }
        }
    }
    // Attach the grouped member ids to their leads, and count the
    // families that actually grouped at least one statement.
    let mut family_count = 0;
    if !grouped.is_empty() {
        let selected_index: BTreeMap<String, usize> = selected
            .iter()
            .enumerate()
            .map(|(i, f)| (f.id.clone(), i))
            .collect();
        for (fi, member_ids) in &grouped_member_ids {
            if member_ids.is_empty() {
                continue;
            }
            family_count += 1;
            let lead_id = &lead_ids[fi];
            if let Some(&pos) = selected_index.get(lead_id) {
                selected[pos].grouped_members = member_ids.clone();
            }
        }
    }
    // Presentation order (frame-model step 2): the selected
    // findings are returned in report-band order — account,
    // completeness, relations, column structure, deviations — with
    // interestingness inside the band, so a reader meets what the
    // dataset is before what deviates in it. Selection above is
    // unaffected by the band. A family lead presents at the
    // earliest band among itself and its grouped members (issue
    // #116): the lead stands for the family's strongest content,
    // so a bijection lead grouped with its FDs presents with the
    // relations, not with the column-structure statements.
    let mut eff_band: BTreeMap<String, u8> = BTreeMap::new();
    if !grouped.is_empty() {
        let member_band: BTreeMap<String, u8> = grouped
            .iter()
            .map(|f| (f.id.clone(), report_band(f)))
            .collect();
        for f in &selected {
            if !f.grouped_members.is_empty() {
                let band = f
                    .grouped_members
                    .iter()
                    .filter_map(|id| member_band.get(id))
                    .fold(report_band(f), |b, m| b.min(*m));
                eff_band.insert(f.id.clone(), band);
            }
        }
    }
    let band_of = |f: &Finding| {
        eff_band
            .get(&f.id)
            .copied()
            .unwrap_or_else(|| report_band(f))
    };
    selected.sort_by(|a, b| {
        band_of(a)
            .cmp(&band_of(b))
            .then(b.interestingness.total_cmp(&a.interestingness))
            .then(a.canonical_key().cmp(&b.canonical_key()))
    });
    grouped.sort_by(|a, b| {
        report_band(a)
            .cmp(&report_band(b))
            .then(b.interestingness.total_cmp(&a.interestingness))
            .then(a.canonical_key().cmp(&b.canonical_key()))
    });
    SelectionOutcome {
        selected,
        grouped,
        capped: admission.capped,
        families: family_count,
    }
}

/// A connected view over selected findings; this does not assert redundancy.
pub fn group_findings(findings: &[Finding]) -> Vec<FindingGroup> {
    let mut adjacency = BTreeMap::<String, BTreeSet<String>>::new();
    for f in findings.iter().filter(|f| {
        matches!(
            f.category,
            Category::Dependencies | Category::ConditionalRules | Category::Associations
        )
    }) {
        for a in &f.subject.columns {
            for b in &f.subject.columns {
                if a != b {
                    adjacency.entry(a.clone()).or_default().insert(b.clone());
                }
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut groups = Vec::new();
    for start in adjacency.keys() {
        if seen.contains(start) {
            continue;
        }
        let mut stack = vec![start.clone()];
        let mut columns = BTreeSet::new();
        while let Some(c) = stack.pop() {
            if !seen.insert(c.clone()) {
                continue;
            }
            columns.insert(c.clone());
            if let Some(next) = adjacency.get(&c) {
                stack.extend(next.iter().cloned());
            }
        }
        if columns.len() < 2 {
            continue;
        }
        let ids = findings
            .iter()
            .filter(|f| {
                f.subject.columns.len() > 1 && f.subject.columns.iter().all(|c| columns.contains(c))
            })
            .map(|f| f.id.clone())
            .collect();
        groups.push(FindingGroup{id:format!("group-{}",groups.len()+1),columns:columns.into_iter().collect(),finding_ids:ids,
            description:"Connected findings; inspect individual statements and directions before inferring redundancy.".into()});
    }
    groups
}
