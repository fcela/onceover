//! Entity fingerprints (issue #448): each entity's distribution over
//! one categorical dimension, compared across entities.
//!
//! For one entity column and one dimension, every entity has a vector
//! of row counts over the dimension's levels. The pass asks four
//! questions of those vectors, in this order:
//!
//! 1. **Do entities differ in mix at all?** A chi-square test of
//!    homogeneity on the entity × level table. The statement needs
//!    the excess over sampling noise to be material: Cramér's V of
//!    the excess, sqrt(max(0, χ² − df) / (N·(L − 1))), at least 0.1
//!    (the association passes' effect floor). Shuffled levels, or
//!    entities drawn from one profile with any support, give χ² ≈ df
//!    and V ≈ 0: nothing is stated, and coverage says so.
//! 2. **Groups or a continuum?** Each entity's shares are shrunk
//!    toward the pooled shares with a Dirichlet prior whose strength
//!    is a method-of-moments estimate (a 3-row entity does not look
//!    extreme), and embedded by square roots (the Hellinger
//!    embedding, where Euclidean distance is the Hellinger distance).
//!    Principal axes whose variance exceeds twice the multinomial
//!    noise variance (about 1/(4n) per entity) form the signal
//!    space. k-means for k = 1..8 runs in that space, and k is
//!    chosen by the gap statistic (Tibshirani, Walther and Hastie
//!    2001): log within-cluster dispersion against the same quantity
//!    on uniform draws over the box spanned by the principal axes,
//!    taking the smallest k within one standard error of the largest
//!    gap. Groups whose union is homogeneous by the test of step 1
//!    (V of the excess below 0.1) are merged. One group with
//!    material differences is a continuum: entities spread along the
//!    first principal axis rather than falling into groups.
//! 3. **Which entities match no group?** With two or more groups, an
//!    entity whose counts reject its group's pooled profile by the
//!    chi-square test at a Bonferroni bar over entities (0.05 / E).
//! 4. **Specialists.** The share of entities with an effective number
//!    of levels (exp of the entropy of their shares) of 1.5 or less,
//!    against the pooled distribution's effective number, stated
//!    when at least 10% of entities are specialists and the share
//!    under multinomial sampling from the pooled shares, at each
//!    entity's own support, is at most a third of it.
//!
//! Row counts also support a scan for changes in mix when an axis is resolved.

use std::collections::BTreeMap;

use onceover_core::TypedTable;
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;

use crate::dep::chi2_sf;
use crate::fingerprints_drift::{self, DriftInput, FingerprintPeriods};
use crate::slices::splitmix64;
use crate::Deadline;

/// Smallest excess Cramér's V stated (the association passes' floor).
pub const MIN_EXCESS_V: f64 = 0.1;
const MAX_K: usize = 8;
const GAP_REFERENCES: usize = 10;
const MAX_LEVELS: usize = 30;
const MAX_ENTITIES: usize = 5000;
pub(crate) const MIN_ENTITIES: usize = 20;

/// The resolved inputs: the entity column and the dimensions.
pub struct FingerprintInput {
    pub entity: usize,
    pub dimensions: Vec<usize>,
    pub periods: Option<FingerprintPeriods>,
    pub deadline: Deadline,
}

struct Rng(u64);
impl Rng {
    fn next_f64(&mut self) -> f64 {
        self.0 = splitmix64(self.0);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn below(&mut self, n: usize) -> usize {
        ((self.next_f64() * n as f64) as usize).min(n - 1)
    }
}

fn sq(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}

/// k-means++ with restarts; returns (within dispersion, labels).
fn kmeans(x: &[Vec<f64>], k: usize, rng: &mut Rng, restarts: usize) -> (f64, Vec<usize>) {
    let n = x.len();
    let mut best = (f64::INFINITY, vec![0; n]);
    for _ in 0..restarts {
        let mut c: Vec<Vec<f64>> = vec![x[rng.below(n)].clone()];
        while c.len() < k {
            let d: Vec<f64> = x
                .iter()
                .map(|p| c.iter().map(|q| sq(p, q)).fold(f64::INFINITY, f64::min))
                .collect();
            let total: f64 = d.iter().sum();
            if total <= 0.0 {
                break;
            }
            let mut r = rng.next_f64() * total;
            let mut pick = n - 1;
            for (i, di) in d.iter().enumerate() {
                r -= di;
                if r <= 0.0 {
                    pick = i;
                    break;
                }
            }
            c.push(x[pick].clone());
        }
        let mut lab = vec![0usize; n];
        for _ in 0..50 {
            let mut changed = false;
            for (i, p) in x.iter().enumerate() {
                let j = (0..c.len())
                    .min_by(|&a, &b| sq(p, &c[a]).total_cmp(&sq(p, &c[b])))
                    .unwrap_or(0);
                if j != lab[i] {
                    lab[i] = j;
                    changed = true;
                }
            }
            for (j, cj) in c.iter_mut().enumerate() {
                let members: Vec<&Vec<f64>> = x
                    .iter()
                    .zip(&lab)
                    .filter(|(_, &l)| l == j)
                    .map(|(p, _)| p)
                    .collect();
                if !members.is_empty() {
                    for (d, v) in cj.iter_mut().enumerate() {
                        *v = members.iter().map(|p| p[d]).sum::<f64>() / members.len() as f64;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let w: f64 = x.iter().zip(&lab).map(|(p, &l)| sq(p, &c[l])).sum();
        if w < best.0 {
            best = (w, lab);
        }
    }
    best
}

/// Eigen-decomposition of a symmetric matrix (cyclic Jacobi);
/// eigenvalues descending with their unit eigenvectors.
#[allow(clippy::needless_range_loop)]
fn jacobi(mut a: Vec<Vec<f64>>) -> Vec<(f64, Vec<f64>)> {
    let n = a.len();
    let mut v: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| f64::from(i == j)).collect())
        .collect();
    for _ in 0..100 {
        let off: f64 = (0..n)
            .flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| a[i][j] * a[i][j])
            .sum();
        if off < 1e-22 {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let (c, s) = (1.0 / (t * t + 1.0).sqrt(), t / (t * t + 1.0).sqrt());
                for k in 0..n {
                    let (akp, akq) = (a[k][p], a[k][q]);
                    a[k][p] = c * akp - s * akq;
                    a[k][q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let (apk, aqk) = (a[p][k], a[q][k]);
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
                for row in v.iter_mut() {
                    let (vkp, vkq) = (row[p], row[q]);
                    row[p] = c * vkp - s * vkq;
                    row[q] = s * vkp + c * vkq;
                }
            }
        }
    }
    let mut out: Vec<(f64, Vec<f64>)> = (0..n)
        .map(|i| (a[i][i], v.iter().map(|r| r[i]).collect()))
        .collect();
    out.sort_by(|x, y| y.0.total_cmp(&x.0));
    out
}

/// Excess Cramér's V of an entity × level count table, with the
/// chi-square statistic and its degrees of freedom.
fn excess_v(rows: &[&Vec<f64>]) -> (f64, f64, f64) {
    let l = rows.first().map_or(0, |r| r.len());
    let col: Vec<f64> = (0..l).map(|j| rows.iter().map(|r| r[j]).sum()).collect();
    let used: Vec<usize> = (0..l).filter(|&j| col[j] > 0.0).collect();
    let total: f64 = col.iter().sum();
    if rows.len() < 2 || used.len() < 2 || total <= 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let mut x2 = 0.0;
    for r in rows {
        let n: f64 = r.iter().sum();
        for &j in &used {
            let e = n * col[j] / total;
            if e > 0.0 {
                x2 += (r[j] - e) * (r[j] - e) / e;
            }
        }
    }
    let df = ((rows.len() - 1) * (used.len() - 1)) as f64;
    let v = ((x2 - df).max(0.0) / (total * (used.len() - 1) as f64)).sqrt();
    (v, x2, df)
}

fn eff_levels(shares: &[f64]) -> f64 {
    (-shares
        .iter()
        .filter(|&&p| p > 0.0)
        .map(|p| p * p.ln())
        .sum::<f64>())
    .exp()
}

pub(crate) fn pct(x: f64) -> String {
    format!("{:.0}%", 100.0 * x)
}

/// The levels that most separate a group from the pooled shares,
/// as "share of level (against pooled)" phrases, at most three.
fn distinguishing(
    group: &[f64],
    pooled: &[f64],
    labels: &[String],
) -> (String, Vec<serde_json::Value>) {
    let mut idx: Vec<usize> = (0..pooled.len()).collect();
    idx.sort_by(|&a, &b| (group[b] - pooled[b]).total_cmp(&(group[a] - pooled[a])));
    let top: Vec<usize> = idx
        .into_iter()
        .filter(|&j| group[j] - pooled[j] > 0.05)
        .take(3)
        .collect();
    let text = top
        .iter()
        .map(|&j| {
            format!(
                "{} on '{}' (against {} overall)",
                pct(group[j]),
                labels[j],
                pct(pooled[j])
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let json = top
        .iter()
        .map(|&j| json!({"level": labels[j], "share": group[j], "overall": pooled[j]}))
        .collect();
    (text, json)
}

/// Run the pass. Returns findings and the coverage detail.
pub fn entity_fingerprints(
    table: &TypedTable,
    input: &FingerprintInput,
) -> (Vec<Finding>, String, bool) {
    let ename = table.names[input.entity].clone();
    let Ok((ecodes, n_ent, elabels)) = crate::panels::entity_index(table, input.entity) else {
        return (
            vec![],
            format!("entity column {ename} cannot key entities"),
            false,
        );
    };
    if n_ent < MIN_ENTITIES {
        return (
            vec![],
            format!("{n_ent} entities in {ename}; the pass needs at least {MIN_ENTITIES}"),
            false,
        );
    }
    let mut findings = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut limited = false;
    for (dimension_index, &d) in input.dimensions.iter().enumerate() {
        if input.deadline.exceeded() {
            notes.push(format!(
                "deadline after {dimension_index} of {} dimensions",
                input.dimensions.len()
            ));
            limited = true;
            break;
        }
        if d == input.entity {
            continue;
        }
        let dname = table.names[d].clone();
        let Ok((dcodes, n_lev, dlabels)) = crate::panels::entity_index(table, d) else {
            notes.push(format!("{dname}: not a categorical column"));
            continue;
        };
        if !(2..=MAX_LEVELS).contains(&n_lev) {
            notes.push(format!("{dname}: {n_lev} levels, outside 2..={MAX_LEVELS}"));
            continue;
        }
        // Entity × level counts.
        let mut counts = vec![vec![0.0f64; n_lev]; n_ent];
        for (e, l) in ecodes.iter().zip(&dcodes) {
            if let (Some(e), Some(l)) = (e, l) {
                counts[*e as usize][*l as usize] += 1.0;
            }
        }
        // Entities with rows on this dimension; the largest by support
        // when there are more than the cap.
        let mut ents: Vec<usize> = (0..n_ent)
            .filter(|&e| counts[e].iter().sum::<f64>() > 0.0)
            .collect();
        ents.sort_by(|&a, &b| {
            counts[b]
                .iter()
                .sum::<f64>()
                .total_cmp(&counts[a].iter().sum::<f64>())
                .then_with(|| elabels[a].cmp(&elabels[b]))
        });
        let capped = ents.len() > MAX_ENTITIES;
        if capped {
            limited = true;
            notes.push(format!(
                "{dname}: {MAX_ENTITIES} of {} entities with values examined",
                ents.len()
            ));
        }
        ents.truncate(MAX_ENTITIES);
        ents.sort_unstable();
        let e_n = ents.len();
        if e_n < MIN_ENTITIES {
            notes.push(format!("{dname}: {e_n} entities with values"));
            continue;
        }
        if let Some(periods) = &input.periods {
            let (mut changes, detail, stopped) = fingerprints_drift::scan(DriftInput {
                periods,
                entity: &ename,
                dimension: &dname,
                entity_labels: &elabels,
                level_labels: &dlabels,
                entities: &ents,
                entity_codes: &ecodes,
                level_codes: &dcodes,
                dimension_count: input.dimensions.len(),
                deadline: input.deadline,
            });
            notes.push(format!("{dname}: {detail}"));
            findings.append(&mut changes);
            if stopped {
                limited = true;
                break;
            }
        }
        let rows: Vec<&Vec<f64>> = ents.iter().map(|&e| &counts[e]).collect();
        let (v, x2, df) = excess_v(&rows);
        let p = chi2_sf(x2, df);
        if v < MIN_EXCESS_V || p > 0.05 / input.dimensions.len().max(1) as f64 {
            notes.push(format!(
                "{dname}: entities do not differ in mix beyond sampling noise (excess V {v:.3}, chi-square {x2:.0} on {df:.0} df)"
            ));
            continue;
        }
        let n: Vec<f64> = rows.iter().map(|r| r.iter().sum()).collect();
        let total: f64 = n.iter().sum();
        let pooled: Vec<f64> = (0..n_lev)
            .map(|j| rows.iter().map(|r| r[j]).sum::<f64>() / total)
            .collect();
        // Dirichlet strength by moments: E[Σ n(s−p)²/p]/(L−1) = 1 + (n−1)/(α+1).
        let used = pooled.iter().filter(|&&q| q > 0.0).count().max(2);
        let disp: f64 = rows
            .iter()
            .zip(&n)
            .map(|(r, &ni)| {
                (0..n_lev)
                    .filter(|&j| pooled[j] > 0.0)
                    .map(|j| (r[j] / ni - pooled[j]).powi(2) / pooled[j])
                    .sum::<f64>()
                    * ni
            })
            .sum::<f64>()
            / (e_n * (used - 1)) as f64;
        let nbar = total / e_n as f64;
        let alpha = if disp > 1.0 {
            ((nbar - 1.0) / (disp - 1.0) - 1.0).clamp(0.5, 1e6)
        } else {
            1e6
        };
        let x: Vec<Vec<f64>> = rows
            .iter()
            .zip(&n)
            .map(|(r, &ni)| {
                (0..n_lev)
                    .map(|j| ((r[j] + alpha * pooled[j]) / (ni + alpha)).sqrt())
                    .collect()
            })
            .collect();
        // Principal axes and the signal space.
        let mean: Vec<f64> = (0..n_lev)
            .map(|j| x.iter().map(|r| r[j]).sum::<f64>() / e_n as f64)
            .collect();
        let xc: Vec<Vec<f64>> = x
            .iter()
            .map(|r| r.iter().zip(&mean).map(|(a, b)| a - b).collect())
            .collect();
        let cov: Vec<Vec<f64>> = (0..n_lev)
            .map(|i| {
                (0..n_lev)
                    .map(|j| xc.iter().map(|r| r[i] * r[j]).sum::<f64>() / e_n as f64)
                    .collect()
            })
            .collect();
        let eig = jacobi(cov);
        let noise: f64 = n
            .iter()
            .map(|&ni| (ni / (ni + alpha)).powi(2) / (4.0 * ni))
            .sum::<f64>()
            / e_n as f64;
        let m = eig
            .iter()
            .filter(|(ev, _)| *ev > 2.0 * noise)
            .count()
            .clamp(1, n_lev - 1);
        let z: Vec<Vec<f64>> = xc
            .iter()
            .map(|r| {
                eig[..m]
                    .iter()
                    .map(|(_, u)| r.iter().zip(u).map(|(a, b)| a * b).sum())
                    .collect()
            })
            .collect();
        let mut rng = Rng(0x05ee_d448 ^ (d as u64) << 20);
        let kmax = MAX_K.min(e_n / 5).max(1);
        let log_w: Vec<f64> = (1..=kmax)
            .map(|k| kmeans(&z, k, &mut rng, 5).0.max(1e-300).ln())
            .collect();
        let lo: Vec<f64> = (0..m)
            .map(|j| z.iter().map(|r| r[j]).fold(f64::INFINITY, f64::min))
            .collect();
        let hi: Vec<f64> = (0..m)
            .map(|j| z.iter().map(|r| r[j]).fold(f64::NEG_INFINITY, f64::max))
            .collect();
        let mut refs = vec![vec![0.0; kmax]; GAP_REFERENCES];
        for rf in refs.iter_mut() {
            let sample: Vec<Vec<f64>> = (0..e_n)
                .map(|_| {
                    (0..m)
                        .map(|j| lo[j] + rng.next_f64() * (hi[j] - lo[j]))
                        .collect()
                })
                .collect();
            for (k, slot) in rf.iter_mut().enumerate() {
                *slot = kmeans(&sample, k + 1, &mut rng, 2).0.max(1e-300).ln();
            }
        }
        let gap: Vec<f64> = (0..kmax)
            .map(|k| refs.iter().map(|r| r[k]).sum::<f64>() / GAP_REFERENCES as f64 - log_w[k])
            .collect();
        let se: Vec<f64> = (0..kmax)
            .map(|k| {
                let mu = refs.iter().map(|r| r[k]).sum::<f64>() / GAP_REFERENCES as f64;
                (refs.iter().map(|r| (r[k] - mu).powi(2)).sum::<f64>() / GAP_REFERENCES as f64)
                    .sqrt()
                    * (1.0 + 1.0 / GAP_REFERENCES as f64).sqrt()
            })
            .collect();
        let kbest = (0..kmax)
            .max_by(|&a, &b| gap[a].total_cmp(&gap[b]))
            .unwrap_or(0);
        let k_gap = (0..kmax)
            .find(|&k| gap[k] >= gap[kbest] - se[kbest])
            .unwrap_or(0)
            + 1;
        let (_, mut lab) = kmeans(&z, k_gap, &mut rng, 5);
        // Merge groups whose union is homogeneous.
        loop {
            let mut groups: Vec<usize> = lab.clone();
            groups.sort_unstable();
            groups.dedup();
            if groups.len() < 2 {
                break;
            }
            let mut best: Option<(f64, usize, usize)> = None;
            for (a, &gi) in groups.iter().enumerate() {
                for &gj in &groups[a + 1..] {
                    let union: Vec<&Vec<f64>> = rows
                        .iter()
                        .zip(&lab)
                        .filter(|(_, &l)| l == gi || l == gj)
                        .map(|(r, _)| *r)
                        .collect();
                    let (vij, _, _) = excess_v(&union);
                    if best.is_none_or(|b| vij < b.0) {
                        best = Some((vij, gi, gj));
                    }
                }
            }
            match best {
                Some((vij, gi, gj)) if vij < MIN_EXCESS_V => {
                    lab.iter_mut().filter(|l| **l == gj).for_each(|l| *l = gi)
                }
                _ => break,
            }
        }
        let mut gids: Vec<usize> = lab.clone();
        gids.sort_unstable();
        gids.dedup();
        let lab: Vec<usize> = lab
            .iter()
            .map(|l| gids.iter().position(|g| g == l).unwrap_or(0))
            .collect();
        let k = gids.len();
        let share_of = |members: &[usize]| -> Vec<f64> {
            let t: f64 = members.iter().map(|&i| n[i]).sum();
            (0..n_lev)
                .map(|j| members.iter().map(|&i| rows[i][j]).sum::<f64>() / t.max(1.0))
                .collect()
        };
        let scope = format!(
            "{e_n} {ename} values{}",
            if capped {
                format!(" (the {MAX_ENTITIES} with most rows)")
            } else {
                String::new()
            }
        );
        let base_metrics = |extra: &[(&str, serde_json::Value)]| {
            let mut m = BTreeMap::from([
                ("entity_column".to_string(), json!(ename)),
                ("dimension".to_string(), json!(dname)),
                ("entities".to_string(), json!(e_n)),
                ("excess_cramers_v".to_string(), json!(v)),
                ("chi_square".to_string(), json!(x2)),
                ("df".to_string(), json!(df)),
                ("p_value".to_string(), json!(p)),
                ("shrinkage_strength".to_string(), json!(alpha)),
                ("signal_axes".to_string(), json!(m)),
                ("gap".to_string(), json!(gap)),
                ("n_rows".to_string(), json!(total as u64)),
            ]);
            for (key, val) in extra {
                m.insert((*key).to_string(), val.clone());
            }
            m
        };
        let method = || {
            Method::new("entity_fingerprints", "entity_mix_groups", "entity × level counts; Hellinger embedding, gap-statistic k-means, homogeneity merge (onceover v0.1)")
        };
        if k >= 2 {
            let mut groups: Vec<(Vec<usize>, Vec<f64>)> = (0..k)
                .map(|g| {
                    let mem: Vec<usize> = (0..e_n).filter(|&i| lab[i] == g).collect();
                    let sh = share_of(&mem);
                    (mem, sh)
                })
                .collect();
            groups.sort_by_key(|g| std::cmp::Reverse(g.0.len()));
            let mut parts = Vec::new();
            let mut payload = Vec::new();
            for (mem, sh) in &groups {
                let (text, lv) = distinguishing(sh, &pooled, &dlabels);
                let size = format!(
                    "{} {ename} ({})",
                    mem.len(),
                    pct(mem.len() as f64 / e_n as f64)
                );
                parts.push(if text.is_empty() {
                    format!("{size} near the overall mix")
                } else {
                    format!("{size} with {text}")
                });
                payload.push(json!({"entities": mem.len(), "levels": lv, "examples": mem.iter().take(5).map(|&i| elabels[ents[i]].clone()).collect::<Vec<_>>()}));
            }
            let claim = format!(
                "{ename} fall into {k} groups by their mix of {dname}: {}",
                parts.join("; ")
            );
            findings.push(Finding::new(
                Pattern::EntityGroups,
                BTreeMap::from([("columns".into(), json!([ename, dname]))]),
                claim,
                EvidenceBasis::Estimated,
                (0.5 + v).min(1.0),
                base_metrics(&[("groups", json!(payload)), ("scope", json!(scope))]),
                Some(format!("each {ename}'s share of rows on each {dname} level was compared; groups differ materially (excess Cramér's V of 0.1 or more between any two), members of one group do not")),
                method(),
            ));
            // Misfits: reject their own group's pooled profile.
            let bar = 0.05 / e_n as f64;
            let mut misfits: Vec<(usize, f64)> = Vec::new();
            for (mem, sh) in &groups {
                for &i in mem {
                    let q: Vec<f64> = sh
                        .iter()
                        .zip(&pooled)
                        .map(|(a, b)| 0.98 * a + 0.02 * b)
                        .collect();
                    let xe: f64 = (0..n_lev)
                        .filter(|&j| q[j] > 0.0)
                        .map(|j| (rows[i][j] - n[i] * q[j]).powi(2) / (n[i] * q[j]))
                        .sum();
                    let pe = chi2_sf(xe, (used - 1) as f64);
                    if pe < bar {
                        misfits.push((i, pe));
                    }
                }
            }
            if !misfits.is_empty() {
                misfits.sort_by(|a, b| a.1.total_cmp(&b.1));
                let rows_share = misfits.iter().map(|(i, _)| n[*i]).sum::<f64>() / total;
                let names: Vec<String> = misfits
                    .iter()
                    .take(5)
                    .map(|(i, _)| elabels[ents[*i]].clone())
                    .collect();
                let claim = format!(
                    "{} {ename} ({}) match none of the {k} {dname}-mix groups: each one's counts reject its nearest group's profile (chi-square, Bonferroni over {e_n} {ename}); together they hold {} of rows; for example {}",
                    misfits.len(), pct(misfits.len() as f64 / e_n as f64), pct(rows_share), names.join(", ")
                );
                findings.push(Finding::new(
                    Pattern::EntityMisfits,
                    BTreeMap::from([("columns".into(), json!([ename, dname]))]),
                    claim,
                    EvidenceBasis::Measured,
                    0.4 + 0.3 * (misfits.len() as f64 / e_n as f64).min(1.0),
                    base_metrics(&[
                        ("misfits", json!(misfits.iter().map(|(i, pe)| json!({"entity": elabels[ents[*i]], "rows": n[*i], "p_value": pe})).collect::<Vec<_>>())),
                        ("bar", json!(bar)),
                    ]),
                    None,
                    Method::new("entity_fingerprints", "entity_misfit_test", "chi-square of an entity's counts against its group's pooled profile (onceover v0.1)"),
                ));
            }
        } else {
            let (ev1, u1) = &eig[0];
            let mut idx: Vec<usize> = (0..n_lev).collect();
            idx.sort_by(|&a, &b| u1[b].total_cmp(&u1[a]));
            let (pos, neg) = (idx[0], idx[n_lev - 1]);
            let claim = format!(
                "{ename} differ in their mix of {dname} along one continuous axis, not in groups: the share of '{}' against '{}' varies from {ename} to {ename} without clusters (excess Cramér's V {v:.2})",
                dlabels[pos], dlabels[neg]
            );
            findings.push(Finding::new(
                Pattern::EntityContinuum,
                BTreeMap::from([("columns".into(), json!([ename, dname]))]),
                claim,
                EvidenceBasis::Estimated,
                (0.4 + v).min(0.9),
                base_metrics(&[("axis_variance", json!(ev1)), ("axis_levels", json!([dlabels[pos], dlabels[neg]]))]),
                Some("the gap statistic finds no grouping beyond a uniform spread along the principal axes".into()),
                method(),
            ));
        }
        // Specialists.
        let spec_obs = (0..e_n)
            .filter(|&i| {
                n[i] >= 10.0
                    && eff_levels(&rows[i].iter().map(|c| c / n[i]).collect::<Vec<_>>()) <= 1.5
            })
            .count();
        let eligible = (0..e_n).filter(|&i| n[i] >= 10.0).count();
        if eligible >= MIN_ENTITIES && spec_obs as f64 >= 0.1 * eligible as f64 {
            // Multinomial draws from the pooled shares at each entity's support.
            let cum: Vec<f64> = pooled
                .iter()
                .scan(0.0, |s, p| {
                    *s += p;
                    Some(*s)
                })
                .collect();
            let mut sim = 0usize;
            for _ in 0..3 {
                for i in (0..e_n).filter(|&i| n[i] >= 10.0) {
                    let mut c = vec![0.0; n_lev];
                    for _ in 0..n[i] as usize {
                        let r = rng.next_f64();
                        c[cum.iter().position(|&q| r <= q).unwrap_or(n_lev - 1)] += 1.0;
                    }
                    if eff_levels(&c.iter().map(|x| x / n[i]).collect::<Vec<_>>()) <= 1.5 {
                        sim += 1;
                    }
                }
            }
            let sim_share = sim as f64 / (3 * eligible) as f64;
            let obs_share = spec_obs as f64 / eligible as f64;
            if sim_share <= obs_share / 3.0 {
                let claim = format!(
                    "{} of {ename} ({spec_obs} of {eligible} with 10 or more rows) concentrate on essentially one {dname} level (effective number of levels 1.5 or less), against {:.1} effective levels in the overall mix",
                    pct(obs_share), eff_levels(&pooled)
                );
                findings.push(Finding::new(
                    Pattern::EntitySpecialists,
                    BTreeMap::from([("columns".into(), json!([ename, dname]))]),
                    claim,
                    EvidenceBasis::Measured,
                    0.4 + 0.4 * obs_share,
                    base_metrics(&[("specialist_share", json!(obs_share)), ("null_share", json!(sim_share)), ("overall_effective_levels", json!(eff_levels(&pooled)))]),
                    None,
                    Method::new("entity_fingerprints", "entity_specialists", "effective number of levels per entity against multinomial sampling (onceover v0.1)"),
                ));
            }
        }
        notes.push(format!("{dname}: {k} group(s) over {scope}, excess V {v:.3}, {m} signal axes, gap-statistic k {k_gap}"));
    }
    (
        findings,
        format!(
            "entity {ename}; weight: row count; {}; dimensions: {}",
            input.periods.as_ref().map_or_else(
                || "drift skipped: no resolved axis with per-row keys".into(),
                FingerprintPeriods::detail,
            ),
            notes.join("; "),
        ),
        limited,
    )
}
