//! Bounded pairwise association rules for nominal encoded features.

use crate::dep::{assoc_tier, bh_qvalues, chi2_sf, f_sf, fmt_q, Q_BAR};
use crate::encode::{EncodedMatrix, MISSING};
use onceover_report::{AssociationPair, EvidenceBasis, Finding};
use onceover_report::{Method, Pattern};
use serde_json::json;
use std::collections::BTreeMap;

pub struct AssociationConfig {
    pub max_levels: usize,
    pub min_support: usize,
    pub min_lift: f64,
    pub min_confidence: f64,
    pub max_findings: usize,
}

impl Default for AssociationConfig {
    fn default() -> Self {
        Self {
            max_levels: 16,
            min_support: 30,
            min_lift: 1.5,
            min_confidence: 0.75,
            max_findings: 100,
        }
    }
}

#[derive(Clone)]
struct Mask(Vec<u64>);
impl Mask {
    fn new(n: usize) -> Self {
        Self(vec![0; n.div_ceil(64)])
    }
    fn set(&mut self, i: usize) {
        self.0[i / 64] |= 1u64 << (i % 64);
    }
    fn count(&self) -> usize {
        self.0.iter().map(|w| w.count_ones() as usize).sum()
    }
    fn and_count(&self, rhs: &Self) -> usize {
        self.0
            .iter()
            .zip(&rhs.0)
            .map(|(a, b)| (a & b).count_ones() as usize)
            .sum()
    }
    fn intersection(&self, rhs: &Self) -> Self {
        Self(self.0.iter().zip(&rhs.0).map(|(a, b)| a & b).collect())
    }
}

fn holdout(row: u32) -> bool {
    // Stable integer mix of the original row id. The split is independent of
    // the deterministic stratified sample used to construct EncodedMatrix.
    let mut x = row as u64 + 0x9e3779b97f4a7c15;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^= x >> 31;
    x.is_multiple_of(5)
}

pub fn association_rules(
    enc: &EncodedMatrix,
    cfg: &AssociationConfig,
    excluded_targets: &[usize],
) -> (Vec<Finding>, String) {
    if cfg.min_support == 0
        || !cfg.min_lift.is_finite()
        || cfg.min_lift <= 0.0
        || !cfg.min_confidence.is_finite()
        || !(0.0..=1.0).contains(&cfg.min_confidence)
        || cfg.max_levels > 254
        || cfg.max_findings > 1000
    {
        return (Vec::new(), "invalid association config: require min_support >= 1, finite positive min_lift, min_confidence in [0,1], max_levels <= 254, max_findings <= 1000".into());
    }
    let feats: Vec<usize> = (0..enc.bins.len())
        .filter(|&f| {
            enc.feat_nominal.get(f) == Some(&true)
                && enc.n_bins.get(f).copied().unwrap_or(usize::MAX) <= cfg.max_levels
                && enc.levels.get(f).map(|x| !x.is_empty()).unwrap_or(false)
        })
        .collect();
    let nominal = enc.feat_nominal.iter().filter(|&&v| v).count();
    let mut masks: Vec<Vec<(Mask, Mask)>> = Vec::new(); // per feature, level: train, holdout
    let mut valid: Vec<(Mask, Mask)> = Vec::new();
    let (mut ntr, mut nho) = (0usize, 0usize);
    for j in 0..enc.n {
        if holdout(*enc.rows.get(j).unwrap_or(&(j as u32))) {
            nho += 1
        } else {
            ntr += 1
        }
    }
    for &f in &feats {
        let mut by = vec![(Mask::new(enc.n), Mask::new(enc.n)); enc.n_bins[f]];
        let (mut tr_valid, mut ho_valid) = (Mask::new(enc.n), Mask::new(enc.n));
        for j in 0..enc.n {
            let c = enc.bins[f][j];
            if c == MISSING || c as usize >= by.len() {
                continue;
            }
            if holdout(*enc.rows.get(j).unwrap_or(&(j as u32))) {
                by[c as usize].1.set(j);
                ho_valid.set(j);
            } else {
                by[c as usize].0.set(j);
                tr_valid.set(j);
            }
        }
        masks.push(by);
        valid.push((tr_valid, ho_valid));
    }
    type RuleKey = (String, String, String, String);
    let mut found: Vec<(f64, RuleKey, Finding)> = Vec::new();
    for ai in 0..feats.len() {
        for bi in ai + 1..feats.len() {
            let (a, b) = (feats[ai], feats[bi]);
            let observed = (
                valid[ai].0.intersection(&valid[bi].0),
                valid[ai].1.intersection(&valid[bi].1),
            );
            let (tr_n, ho_n) = (observed.0.count(), observed.1.count());
            let train_a: Vec<_> = masks[ai]
                .iter()
                .map(|m| m.0.and_count(&observed.0))
                .collect();
            let hold_a: Vec<_> = masks[ai]
                .iter()
                .map(|m| m.1.and_count(&observed.1))
                .collect();
            let train_b: Vec<_> = masks[bi]
                .iter()
                .map(|m| m.0.and_count(&observed.0))
                .collect();
            let hold_b: Vec<_> = masks[bi]
                .iter()
                .map(|m| m.1.and_count(&observed.1))
                .collect();
            for ac in 0..masks[ai].len() {
                for bc in 0..masks[bi].len() {
                    let (jl, jh) = (
                        masks[ai][ac].0.and_count(&masks[bi][bc].0),
                        masks[ai][ac].1.and_count(&masks[bi][bc].1),
                    );
                    if jl < cfg.min_support || jh < cfg.min_support || tr_n == 0 || ho_n == 0 {
                        continue;
                    }
                    for rev in [false, true] {
                        let (lhs, rhs, lcode, rcode, lc, lh, rc, rh) = if rev {
                            (
                                b,
                                a,
                                bc,
                                ac,
                                train_b[bc],
                                hold_b[bc],
                                train_a[ac],
                                hold_a[ac],
                            )
                        } else {
                            (
                                a,
                                b,
                                ac,
                                bc,
                                train_a[ac],
                                hold_a[ac],
                                train_b[bc],
                                hold_b[bc],
                            )
                        };
                        // Frame-role scaffolding is excluded as a
                        // rule consequent; it may still antecede.
                        if excluded_targets.contains(&enc.feat_cols[rhs]) {
                            continue;
                        }
                        let (tc, tl) = (
                            jl as f64 / lc as f64,
                            jl as f64 * tr_n as f64 / (lc as f64 * rc as f64),
                        );
                        let (hc, hl) = (
                            jh as f64 / lh as f64,
                            jh as f64 * ho_n as f64 / (lh as f64 * rh as f64),
                        );
                        if tc < cfg.min_confidence
                            || hc < cfg.min_confidence
                            || tl < cfg.min_lift
                            || hl < cfg.min_lift
                        {
                            continue;
                        }
                        if rc as f64 / tr_n as f64 >= 0.9 || rh as f64 / ho_n as f64 >= 0.9 {
                            continue;
                        }
                        let lv = enc.levels[lhs][lcode].clone();
                        let rv = enc.levels[rhs][rcode].clone();
                        let mut ev = BTreeMap::new();
                        for (k, v) in [
                            ("train_n", json!(ntr)),
                            ("holdout_n", json!(nho)),
                            ("train_pair_n", json!(tr_n)),
                            ("holdout_pair_n", json!(ho_n)),
                            ("train_joint_count", json!(jl)),
                            ("holdout_joint_count", json!(jh)),
                            ("train_support", json!(jl as f64 / tr_n as f64)),
                            ("holdout_support", json!(jh as f64 / ho_n as f64)),
                            ("train_confidence", json!(tc)),
                            ("holdout_confidence", json!(hc)),
                            ("train_lift", json!(tl)),
                            ("holdout_lift", json!(hl)),
                            ("antecedent", json!(lv)),
                            ("consequent", json!(rv)),
                            ("validation", json!("Diagnostic holdout across many candidate rules; no inferential p-value or multiple-testing guarantee.")),
                        ] {
                            ev.insert(k.into(), v);
                        }
                        let score = hc * hl.min(5.0);
                        let f = Finding::new(
Pattern::ConditionalRule,
BTreeMap::from([
                                ("antecedent_feature".into(), json!(enc.feat_names[lhs])),
                                ("antecedent_value".into(), json!(lv)),
                                ("consequent_feature".into(), json!(enc.feat_names[rhs])),
                                ("consequent_value".into(), json!(rv)),
                            ]),
format!("{} = {} is associated with {} = {} (holdout confidence {:.2}, lift {:.2})", enc.feat_names[lhs], lv, enc.feat_names[rhs], rv, hc, hl),
EvidenceBasis::Estimated,
(hc * 0.5 + ((hl - 1.0) / 4.0).min(1.0) * 0.5).min(1.0),
ev,
None,
Method::new("association_rules", "nominal_pair_counts", "Pairwise nominal association with deterministic row holdout")
).with_rule(
    vec![onceover_report::Predicate::equal(enc.feat_names[lhs].clone(),enc.literal(lhs,&lv))],
    onceover_report::Predicate::equal(enc.feat_names[rhs].clone(),enc.literal(rhs,&rv))
);
                        found.push((
                            score,
                            (
                                enc.feat_names[lhs].clone(),
                                lv.clone(),
                                enc.feat_names[rhs].clone(),
                                rv.clone(),
                            ),
                            f,
                        ));
                    }
                }
            }
        }
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    found.dedup_by(|a, b| a.1 == b.1);
    let count = found.len();
    let findings: Vec<Finding> = found
        .into_iter()
        .take(cfg.max_findings)
        .map(|x| x.2)
        .collect();
    let emitted = findings.len();
    (
        findings,
        format!(
            "{} of {nominal} nominal features (all eligible columns, level cap {}); {ntr} train / {nho} diagnostic holdout rows; {count} rules passing criteria, {emitted} emitted (cap {})",
            feats.len(), cfg.max_levels, cfg.max_findings
        ),
    )
}

/// One tested pair from the pair screen: effect size, raw
/// p-value, and the evidence extras the finding carries.
struct PairTest {
    /// Table column indices of the pair, for the scaffold filter.
    cols: (usize, usize),
    /// Encoded feature indices of the pair (nominal side first
    /// for mixed pairs), for the Simpson reversal check.
    feats: (usize, usize),
    a: String,
    b: String,
    kind: &'static str,
    form: &'static str,
    n: usize,
    /// Cramér's V (nominal) or correlation ratio η (mixed); None
    /// when the pair is degenerate (a constant side, no variance).
    effect: Option<f64>,
    p_value: Option<f64>,
    extra: Vec<(&'static str, serde_json::Value)>,
}

/// The distinct non-missing bin codes a nominal feature takes
/// over the encoded rows, ascending. The Simpson check reads a
/// feature's signed direction only when exactly two codes are
/// active.
fn active_codes(enc: &EncodedMatrix, f: usize) -> Vec<u8> {
    let mut seen: Vec<u8> = Vec::new();
    for &c in &enc.bins[f] {
        if c != MISSING && !seen.contains(&c) {
            seen.push(c);
        }
    }
    seen.sort_unstable();
    seen
}

/// Tested pairwise associations over the encoded features
/// (issue #134). Every nominal pair gets a G-test with Cramér's V;
/// every nominal × numeric pair gets an F-test with the
/// correlation ratio η, so mixed pairs no longer depend on the
/// GBM screen to be seen at all. Benjamini–Hochberg over the
/// pairs tested in this pass assigns the reporting tiers,
/// mirroring the numeric dependence screen: strong at or above
/// the 0.5 effect bar, weak below it, both at q <= 0.05. Pairs
/// that cannot be tested (too few rows, sparse expected counts,
/// a degenerate side) are carried in the associations table
/// untested rather than dropped.
pub fn pair_associations(
    enc: &EncodedMatrix,
    table: &onceover_core::TypedTable,
    cfg: &AssociationConfig,
    excluded: &[usize],
) -> (Vec<Finding>, Vec<AssociationPair>, String) {
    let nominal_feats: Vec<usize> = (0..enc.bins.len())
        .filter(|&f| {
            enc.feat_nominal.get(f) == Some(&true)
                && enc.n_bins.get(f).copied().unwrap_or(usize::MAX) <= cfg.max_levels
                && enc.levels.get(f).map(|x| !x.is_empty()).unwrap_or(false)
        })
        .collect();
    let numeric_feats: Vec<usize> = (0..enc.bins.len())
        .filter(|&f| {
            enc.feat_nominal.get(f) == Some(&false)
                && matches!(
                    table.cols[enc.feat_cols[f]],
                    onceover_core::TypedCol::I64 { .. }
                        | onceover_core::TypedCol::U64 { .. }
                        | onceover_core::TypedCol::F64 { .. }
                )
        })
        .collect();

    let mut tests: Vec<PairTest> = Vec::new();

    // Nominal pairs: G-test on the pair-complete contingency table.
    for (ii, &fa) in nominal_feats.iter().enumerate() {
        for &fb in &nominal_feats[ii + 1..] {
            let (la, lb) = (enc.n_bins[fa], enc.n_bins[fb]);
            let mut counts = vec![vec![0usize; lb]; la];
            let mut n = 0usize;
            for j in 0..enc.n {
                let (x, y) = (enc.bins[fa][j], enc.bins[fb][j]);
                if x == MISSING || y == MISSING {
                    continue;
                }
                let (x, y) = (x as usize, y as usize);
                if x < la && y < lb {
                    counts[x][y] += 1;
                    n += 1;
                }
            }
            let mut test = PairTest {
                cols: (enc.feat_cols[fa], enc.feat_cols[fb]),
                feats: (fa, fb),
                a: enc.feat_names[fa].clone(),
                b: enc.feat_names[fb].clone(),
                kind: "nominal",
                form: "nominal_pair",
                n,
                effect: None,
                p_value: None,
                extra: Vec::new(),
            };
            if n >= 30 {
                let rows: Vec<usize> = counts.iter().map(|r| r.iter().sum()).collect();
                let cols: Vec<usize> = (0..lb).map(|y| counts.iter().map(|r| r[y]).sum()).collect();
                let r_act = rows.iter().filter(|&&s| s > 0).count();
                let c_act = cols.iter().filter(|&&s| s > 0).count();
                if r_act >= 2 && c_act >= 2 {
                    let nf = n as f64;
                    let (mut chi2, mut g, mut min_e) = (0.0f64, 0.0f64, f64::INFINITY);
                    for x in 0..la {
                        if rows[x] == 0 {
                            continue;
                        }
                        for y in 0..lb {
                            if cols[y] == 0 {
                                continue;
                            }
                            let e = rows[x] as f64 * cols[y] as f64 / nf;
                            let o = counts[x][y] as f64;
                            chi2 += (o - e) * (o - e) / e;
                            if o > 0.0 {
                                g += 2.0 * o * (o / e).ln();
                            }
                            min_e = min_e.min(e);
                        }
                    }
                    let df = (r_act - 1) * (c_act - 1);
                    let v = (chi2 / nf / ((r_act - 1).min(c_act - 1)) as f64).sqrt();
                    test.effect = Some(v);
                    test.p_value = (min_e >= 1.0).then(|| chi2_sf(g, df as f64));
                    test.extra = vec![
                        ("chi_square", json!(chi2)),
                        ("g_statistic", json!(g)),
                        ("df", json!(df)),
                        ("levels_a", json!(r_act)),
                        ("levels_b", json!(c_act)),
                        ("min_expected", json!(min_e)),
                    ];
                }
            }
            tests.push(test);
        }
    }

    // Mixed pairs: one-way F-test of the numeric column across
    // the nominal column's levels; effect is the correlation
    // ratio η = sqrt(SS_between / SS_total).
    for &fa in &nominal_feats {
        for &fb in &numeric_feats {
            let col = &table.cols[enc.feat_cols[fb]];
            let la = enc.n_bins[fa];
            let mut sums = vec![0.0f64; la];
            let mut cnts = vec![0usize; la];
            let (mut total, mut n) = (0.0f64, 0usize);
            for j in 0..enc.n {
                let x = enc.bins[fa][j];
                if x == MISSING || x as usize >= la {
                    continue;
                }
                if let Some(v) = col.numeric_at(enc.rows[j] as usize) {
                    sums[x as usize] += v;
                    cnts[x as usize] += 1;
                    total += v;
                    n += 1;
                }
            }
            let mut test = PairTest {
                cols: (enc.feat_cols[fa], enc.feat_cols[fb]),
                feats: (fa, fb),
                a: enc.feat_names[fa].clone(),
                b: enc.feat_names[fb].clone(),
                kind: "mixed",
                form: "mixed_pair",
                n,
                effect: None,
                p_value: None,
                extra: Vec::new(),
            };
            if n >= 30 {
                let groups: Vec<usize> = (0..la).filter(|&g| cnts[g] > 0).collect();
                let k = groups.len();
                if k >= 2 && n > k {
                    let mean = total / n as f64;
                    let (mut ssb, mut sst) = (0.0f64, 0.0f64);
                    for j in 0..enc.n {
                        let x = enc.bins[fa][j];
                        if x == MISSING || x as usize >= la {
                            continue;
                        }
                        if let Some(v) = col.numeric_at(enc.rows[j] as usize) {
                            sst += (v - mean) * (v - mean);
                        }
                    }
                    for &g in &groups {
                        let gm = sums[g] / cnts[g] as f64;
                        ssb += cnts[g] as f64 * (gm - mean) * (gm - mean);
                    }
                    if sst > 0.0 {
                        let eta2 = ssb / sst;
                        let eta = eta2.sqrt();
                        let ssw = sst - ssb;
                        let f = if ssw > 0.0 {
                            (ssb / (k - 1) as f64) / (ssw / (n - k) as f64)
                        } else {
                            f64::INFINITY
                        };
                        test.effect = Some(eta);
                        test.p_value = Some(f_sf(f, (k - 1) as f64, (n - k) as f64));
                        test.extra = vec![
                            ("eta_squared", json!(eta2)),
                            ("f_statistic", json!(if f.is_finite() { f } else { 1e308 })),
                            ("df_between", json!(k - 1)),
                            ("df_within", json!(n - k)),
                            ("groups", json!(k)),
                        ];
                    }
                }
            }
            tests.push(test);
        }
    }

    // Benjamini–Hochberg over the pairs tested in this pass.
    let qvalues = bh_qvalues(&tests.iter().map(|t| t.p_value).collect::<Vec<_>>());
    let records: Vec<AssociationPair> = tests
        .iter()
        .zip(&qvalues)
        .map(|(t, q)| AssociationPair {
            a: t.a.clone(),
            b: t.b.clone(),
            kind: t.kind.to_string(),
            n: t.n,
            pearson: None,
            spearman: None,
            xi_ab: None,
            xi_ba: None,
            cramers_v: if t.kind == "nominal" { t.effect } else { None },
            eta: if t.kind == "mixed" { t.effect } else { None },
            p_value: t.p_value,
            q_value: *q,
            tier: assoc_tier(t.effect.unwrap_or(0.0), 0.5, *q).to_string(),
        })
        .collect();

    let mut findings: Vec<(usize, &'static str, Finding)> = tests
        .iter()
        .zip(&qvalues)
        .enumerate()
        .filter(|(_, (t, _))| !excluded.contains(&t.cols.0) && !excluded.contains(&t.cols.1))
        .filter_map(|(idx, (t, q))| {
            let effect = t.effect?;
            let tier = assoc_tier(effect, 0.5, *q);
            if tier == "none" {
                return None;
            }
            let mut ev = BTreeMap::from([
                ("kind".into(), json!(t.form)),
                ("n".into(), json!(t.n)),
                ("population_rows".into(), json!(enc.n)),
                ("row_sampled".into(), json!(enc.n < table.n)),
                ("p_value".into(), json!(t.p_value)),
                ("q_value".into(), json!(q)),
                ("tier".into(), json!(tier)),
            ]);
            for (k, v) in &t.extra {
                ev.insert((*k).into(), v.clone());
            }
            ev.insert("variables".into(), json!([t.a, t.b]));
            let (claim, method) = if t.kind == "nominal" {
                ev.insert("cramers_v".into(), json!(effect));
                ev.insert("method".into(), json!("g_test+cramers_v"));
                let claim = if tier == "weak" {
                    format!(
                        "{} and {} show weak nominal association (Cramer's V {:.3}; significant after Benjamini-Hochberg correction, q = {})",
                        t.a,
                        t.b,
                        effect,
                        fmt_q(q.unwrap_or(1.0))
                    )
                } else {
                    format!(
                        "{} and {} show nominal association (Cramer's V {:.3})",
                        t.a, t.b, effect
                    )
                };
                (
                    claim,
                    Method::new(
                        "association_rules",
                        "nominal_pair_test",
                        "G-test on the pair-complete contingency table with Cramer's V effect; Benjamini-Hochberg correction over the pairs tested in this pass",
                    ),
                )
            } else {
                ev.insert("eta".into(), json!(effect));
                ev.insert("method".into(), json!("f_test+correlation_ratio"));
                let claim = if tier == "weak" {
                    format!(
                        "{} differs weakly across the levels of {} (correlation ratio {:.3}; significant after Benjamini-Hochberg correction, q = {})",
                        t.b,
                        t.a,
                        effect,
                        fmt_q(q.unwrap_or(1.0))
                    )
                } else {
                    format!(
                        "{} differs across the levels of {} (correlation ratio {:.3})",
                        t.b, t.a, effect
                    )
                };
                (
                    claim,
                    Method::new(
                        "association_rules",
                        "mixed_pair_test",
                        "One-way F-test of the numeric column across the nominal column's levels with correlation-ratio effect; Benjamini-Hochberg correction over the pairs tested in this pass",
                    ),
                )
            };
            let interestingness = if tier == "weak" {
                0.10 + 0.25 * effect
            } else {
                0.35 + 0.4 * effect
            };
            let mut finding = Finding::new(
                Pattern::StatisticalAssociation,
                BTreeMap::from([("x".into(), json!(t.a)), ("y".into(), json!(t.b))]),
                claim,
                EvidenceBasis::Estimated,
                interestingness,
                ev,
                None,
                method,
            );
            finding.specification.form = Some(t.form.to_string());
            Some((idx, tier, finding))
        })
        .collect();
    findings.sort_by(|a, b| {
        (a.1 != "strong").cmp(&(b.1 != "strong")).then(
            b.2.interestingness
                .total_cmp(&a.2.interestingness)
                .then(a.0.cmp(&b.0)),
        )
    });
    findings.truncate(20);

    // Simpson reversal check (issue #134, item 5): a strong
    // statement is unconditional, so check whether its pooled
    // direction reverses inside every usable stratum of another
    // nominal dimension. Only pairs with a signed reading are
    // eligible: nominal pairs that are 2x2 on the observed
    // levels (log odds ratio) and mixed pairs whose nominal
    // side has two observed levels (point-biserial). Cramer's V
    // and the correlation ratio over more levels are unsigned
    // and carry no direction to reverse. A reversal appends a
    // qualification to the claim and a payload to the evidence;
    // the finding itself is never removed or re-tiered.
    let (mut simpson_checked, mut simpson_qualified) = (0usize, 0usize);
    let mut simpson_budget = crate::simpson::SimpsonBudget::new();
    let has_strong = findings.iter().any(|(_, t, _)| *t == "strong");
    let stratifiers = if has_strong {
        crate::simpson::stratifiers_from_enc(enc, excluded)
    } else {
        Vec::new()
    };
    if has_strong {
        for (idx, tier, finding) in findings.iter_mut() {
            if *tier != "strong" {
                continue;
            }
            let t = &tests[*idx];
            let (fa, fb) = t.feats;
            let mut coding: Vec<(&'static str, String)> = Vec::new();
            let (stat, pooled, universe) = if t.kind == "nominal" {
                let act_a = active_codes(enc, fa);
                let act_b = active_codes(enc, fb);
                if act_a.len() != 2 || act_b.len() != 2 {
                    continue;
                }
                coding.push(("coded_level_a", enc.levels[fa][act_a[1] as usize].clone()));
                coding.push(("coded_level_b", enc.levels[fb][act_b[1] as usize].clone()));
                let pos = |c: u8, act: &[u8]| -> Option<f64> {
                    if c == MISSING {
                        None
                    } else if c == act[0] {
                        Some(0.0)
                    } else if c == act[1] {
                        Some(1.0)
                    } else {
                        None
                    }
                };
                let universe: Vec<(Option<f64>, Option<f64>)> = (0..enc.n)
                    .map(|j| (pos(enc.bins[fa][j], &act_a), pos(enc.bins[fb][j], &act_b)))
                    .collect();
                let pairs: Vec<(f64, f64)> =
                    universe.iter().filter_map(|(x, y)| (*x).zip(*y)).collect();
                match crate::simpson::log_odds_ratio(&pairs) {
                    Some(v) => (crate::simpson::SignedStat::LogOddsRatio, v, universe),
                    None => continue,
                }
            } else {
                let act = active_codes(enc, fa);
                if act.len() != 2 {
                    continue;
                }
                coding.push(("coded_level", enc.levels[fa][act[1] as usize].clone()));
                let col = &table.cols[enc.feat_cols[fb]];
                let universe: Vec<(Option<f64>, Option<f64>)> = (0..enc.n)
                    .map(|j| {
                        let c = enc.bins[fa][j];
                        let x = if c == MISSING {
                            None
                        } else if c == act[0] {
                            Some(0.0)
                        } else if c == act[1] {
                            Some(1.0)
                        } else {
                            None
                        };
                        (x, col.numeric_at(enc.rows[j] as usize))
                    })
                    .collect();
                let pairs: Vec<(f64, f64)> =
                    universe.iter().filter_map(|(x, y)| (*x).zip(*y)).collect();
                match crate::dep::pearson(&pairs) {
                    Some(v) => (crate::simpson::SignedStat::PointBiserial, v, universe),
                    None => continue,
                }
            };
            simpson_checked += 1;
            if let Some(hit) = crate::simpson::reversal_for_pair(
                stat,
                pooled,
                &universe,
                &stratifiers,
                &[t.cols.0, t.cols.1],
                &mut simpson_budget,
            ) {
                simpson_qualified += 1;
                finding
                    .claim
                    .push_str(&crate::simpson::claim_clause(stat, pooled, &hit));
                if let Some(ev) = finding.evidence.first_mut() {
                    let mut payload = crate::simpson::simpson_payload(stat, pooled, &hit);
                    for (k, v) in &coding {
                        payload.insert((*k).to_string(), json!(v));
                    }
                    ev.metrics
                        .insert("simpson".to_string(), serde_json::Value::Object(payload));
                }
            }
        }
    }

    let n_pair_findings = findings.len();
    let n_weak_pairs = findings.iter().filter(|(_, t, _)| *t == "weak").count();
    let findings: Vec<Finding> = findings.into_iter().map(|(_, _, f)| f).collect();

    let n_nominal = tests.iter().filter(|t| t.kind == "nominal").count();
    let n_tested = tests.iter().filter(|t| t.p_value.is_some()).count();
    let mut coverage = format!(
        "{} nominal pairs, {} mixed pairs screened over {} encoded rows; {} tested with p-values, Benjamini-Hochberg over the {} tested pairs: {} strong and {} weak pair findings (q <= {}, effect bar 0.5); untested pairs are carried in the associations table",
        n_nominal,
        tests.len() - n_nominal,
        enc.n,
        n_tested,
        n_tested,
        n_pair_findings - n_weak_pairs,
        n_weak_pairs,
        Q_BAR,
    );
    if simpson_qualified > 0 || simpson_budget.capped {
        coverage.push_str(&format!(
            "; Simpson reversal check over {} strong pairs: {} qualified{}",
            simpson_checked,
            simpson_qualified,
            if simpson_budget.capped {
                " (evaluation bound reached; some pairs unchecked)"
            } else {
                ""
            }
        ));
    }
    (findings, records, coverage)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn matrix(n: usize, nominal: Vec<bool>, bins: Vec<Vec<u8>>, names: Vec<&str>) -> EncodedMatrix {
        let levels = bins
            .iter()
            .enumerate()
            .map(|(f, _)| {
                if f == 2 {
                    vec![]
                } else {
                    vec!["no".into(), "yes".into()]
                }
            })
            .collect();
        EncodedMatrix {
            rows: (0..n as u32).collect(),
            n,
            feat_cols: (0..bins.len()).collect(),
            feat_dtypes: vec!["string".into(); bins.len()],
            feat_names: names.into_iter().map(str::to_string).collect(),
            feat_nominal: nominal,
            n_bins: bins
                .iter()
                .map(|x| if x.contains(&1) { 2 } else { 1 })
                .collect(),
            bins,
            edges: vec![Vec::new(); 3],
            levels,
        }
    }
    fn cfg() -> AssociationConfig {
        AssociationConfig {
            min_support: 30,
            min_lift: 1.5,
            min_confidence: 0.75,
            ..Default::default()
        }
    }
    #[test]
    fn finds_planted_directional_rule_and_ignores_numeric_bins() {
        let n = 1500;
        let a = (0..n)
            .map(|i| if i % 2 == 0 { 0 } else { 1 })
            .collect::<Vec<_>>();
        let b = (0..n)
            .map(|i| if i % 2 == 0 { 1 } else { 0 })
            .collect::<Vec<_>>();
        let numeric = vec![0; n];
        let enc = matrix(
            n,
            vec![true, true, false],
            vec![a, b, numeric],
            vec!["a", "b", "numeric"],
        );
        let (f, _) = association_rules(&enc, &cfg(), &[]);
        assert!(!f.is_empty());
        assert!(f
            .iter()
            .any(|x| x.subject.inputs == ["a"] && x.subject.target.as_deref() == Some("b")));
    }
    #[test]
    fn excluded_column_is_never_a_rule_consequent() {
        let n = 1500;
        let a = (0..n)
            .map(|i| if i % 2 == 0 { 0 } else { 1 })
            .collect::<Vec<_>>();
        let b = (0..n)
            .map(|i| if i % 2 == 0 { 1 } else { 0 })
            .collect::<Vec<_>>();
        let enc = matrix(
            n,
            vec![true, true, false],
            vec![a, b, vec![0; n]],
            vec!["a", "b", "numeric"],
        );
        // Column 1 ("b") excluded as a target: the a -> b direction
        // disappears; b remains usable as an antecedent for a.
        let (f, _) = association_rules(&enc, &cfg(), &[1]);
        assert!(f.iter().all(|x| x.subject.target.as_deref() != Some("b")));
        assert!(f
            .iter()
            .any(|x| x.subject.inputs == ["b"] && x.subject.target.as_deref() == Some("a")));
    }
    #[test]
    fn missing_values_are_excluded_from_pair_denominator_and_order_is_deterministic() {
        let n = 1800;
        let a = (0..n)
            .map(|i| {
                if i % 11 == 0 {
                    MISSING
                } else if i % 2 == 0 {
                    0
                } else {
                    1
                }
            })
            .collect::<Vec<_>>();
        let b = (0..n)
            .map(|i| {
                if i % 13 == 0 {
                    MISSING
                } else if i % 2 == 0 {
                    1
                } else {
                    0
                }
            })
            .collect::<Vec<_>>();
        let enc = matrix(
            n,
            vec![true, true, false],
            vec![a, b, vec![0; n]],
            vec!["a", "b", "numeric"],
        );
        let (x, _) = association_rules(&enc, &cfg(), &[]);
        let (y, _) = association_rules(&enc, &cfg(), &[]);
        assert_eq!(x, y);
        assert!(!x.is_empty());
        assert!(
            x[0].metrics()["train_pair_n"].as_u64().unwrap()
                < x[0].metrics()["train_n"].as_u64().unwrap()
        );
    }
    #[test]
    fn no_nominal_features_returns_empty() {
        let enc = matrix(
            100,
            vec![false, false, false],
            vec![vec![0; 100]; 3],
            vec!["x", "y", "z"],
        );
        assert!(association_rules(&enc, &cfg(), &[]).0.is_empty());
    }

    #[test]
    fn holdout_reversal_suppresses_training_only_association() {
        let n = 3000;
        let a: Vec<u8> = (0..n).map(|i| (i % 2) as u8).collect();
        let b: Vec<u8> = (0..n)
            .map(|i| if holdout(i as u32) { 1 - a[i] } else { a[i] })
            .collect();
        let enc = matrix(
            n,
            vec![true, true, false],
            vec![a, b, vec![0; n]],
            vec!["a", "b", "numeric"],
        );
        assert!(association_rules(&enc, &cfg(), &[]).0.is_empty());
    }

    #[test]
    fn invalid_config_is_rejected_without_findings() {
        let enc = matrix(
            100,
            vec![true, true, false],
            vec![vec![0; 100]; 3],
            vec!["a", "b", "n"],
        );
        let mut bad = cfg();
        bad.min_support = 0;
        let (findings, coverage) = association_rules(&enc, &bad, &[]);
        assert!(findings.is_empty());
        assert!(coverage.contains("invalid association config"));

        let mut bad = cfg();
        bad.min_lift = f64::NAN;
        assert!(association_rules(&enc, &bad, &[]).0.is_empty());

        let mut bad = cfg();
        bad.min_confidence = f64::NAN;
        assert!(association_rules(&enc, &bad, &[]).0.is_empty());
    }

    // ---- pair_associations (issue #134) ----

    use crate::encode::{encode, EncodeConfig};
    use onceover_core::extract;
    use polars::df;

    struct Lcg(u64);
    impl Lcg {
        fn next_f64(&mut self) -> f64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            (self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    #[test]
    fn pair_screen_states_weak_nominal_and_strong_mixed() {
        // 5,000 rows: a binary nominal pair with Cramer's V near
        // 0.3 (joint [[0.325, 0.175], [0.175, 0.325]]), and a
        // numeric column whose mean shifts by 3 across the first
        // column's levels (correlation ratio well above 0.5).
        let n = 5000usize;
        let mut rng = Lcg(4242);
        let (mut a, mut b, mut v) = (Vec::new(), Vec::new(), Vec::new());
        for _ in 0..n {
            let x = rng.next_f64() < 0.5;
            let y = if rng.next_f64() < 0.65 { x } else { !x };
            a.push(if x { "p" } else { "q" });
            b.push(if y { "r" } else { "s" });
            v.push(if x { 3.0 } else { 0.0 } + rng.next_f64());
        }
        let df = df! { "a" => a, "b" => b, "v" => v }.unwrap();
        let table = extract(&df);
        let (enc, _) = encode(&table, &EncodeConfig::default());
        let (findings, records, coverage) =
            pair_associations(&enc, &table, &AssociationConfig::default(), &[]);
        assert!(
            coverage.contains("1 nominal pairs, 2 mixed pairs"),
            "{coverage}"
        );
        let nominal = records.iter().find(|r| r.kind == "nominal").unwrap();
        let v_meas = nominal.cramers_v.unwrap();
        assert!((0.2..0.4).contains(&v_meas), "V = {v_meas}");
        assert_eq!(nominal.tier, "weak");
        assert!(nominal.q_value.unwrap() <= 0.05);
        let mixed_ab = records
            .iter()
            .find(|r| r.kind == "mixed" && r.a == "a" && r.b == "v")
            .unwrap();
        assert!(mixed_ab.eta.unwrap() > 0.5, "eta = {:?}", mixed_ab.eta);
        assert_eq!(mixed_ab.tier, "strong");
        // Both pairs produce findings, with the tier in evidence.
        let weak_f = findings
            .iter()
            .find(|f| f.specification.form.as_deref() == Some("nominal_pair"))
            .expect("weak nominal finding");
        assert_eq!(weak_f.evidence[0].metrics["tier"].as_str(), Some("weak"));
        assert!(
            weak_f.claim.contains("weak nominal association"),
            "{}",
            weak_f.claim
        );
        assert!(findings
            .iter()
            .any(|f| f.specification.form.as_deref() == Some("mixed_pair")));
    }

    #[test]
    fn pair_screen_independent_columns_earn_no_statement() {
        let n = 4000usize;
        let mut rng = Lcg(777);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for _ in 0..n {
            a.push(if rng.next_f64() < 0.5 { "p" } else { "q" });
            b.push(if rng.next_f64() < 0.5 { "r" } else { "s" });
        }
        let df = df! { "a" => a, "b" => b }.unwrap();
        let table = extract(&df);
        let (enc, _) = encode(&table, &EncodeConfig::default());
        let (findings, records, _) =
            pair_associations(&enc, &table, &AssociationConfig::default(), &[]);
        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].tier, "none");
        assert!(records[0].p_value.is_some());
    }
}
