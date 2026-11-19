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
//! 3. **Which entities match no group?** Test each entity's counts
//!    against every supported group, omitting the entity itself from
//!    the reference. It is a misfit only if every test rejects at the
//!    Bonferroni bar over entities (0.05 / E).
//! 4. **Specialists.** The share of entities with an effective number
//!    of levels (exp of the entropy of their shares) of 1.5 or less,
//!    against the pooled distribution's effective number, stated
//!    when at least 10% of entities are specialists and the share
//!    under multinomial sampling from the pooled shares, at each
//!    entity's own support, is at most a third of it.
//!
//! The same scan accepts positive measure weights, using effective sample
//! sizes for inference, and screens bounded pairs for additional joint signal.

use std::collections::BTreeMap;

use onceover_core::TypedTable;
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;

use crate::dep::chi2_sf;
use crate::fingerprints_drift::{self, DriftInput, FingerprintPeriods};
use crate::fingerprints_peers::{self, HierarchyPeers, PeerInput, MIN_PEERS};
use crate::slices::splitmix64;
use crate::{Deadline, WeightSeries};

/// Smallest excess Cramér's V stated (the association passes' floor).
pub const MIN_EXCESS_V: f64 = 0.1;
const MAX_K: usize = 8;
const MIN_GROUP_ENTITIES: usize = 5;
const MIN_MISFIT_ROWS: f64 = 10.0;
const MIN_MISFIT_DISTANCE: f64 = 0.10;
const GAP_REFERENCES: usize = 10;
pub(crate) const MAX_LEVELS: usize = 30;
const MAX_PAIRS: usize = 12;
const MIN_JOINT_EXCESS: f64 = 0.04;
const MAX_ENTITIES: usize = 5000;
pub(crate) const MIN_ENTITIES: usize = 20;

/// The resolved inputs: the entity column and the dimensions.
pub struct FingerprintInput<'a> {
    pub entity: usize,
    pub dimensions: Vec<usize>,
    pub periods: Option<FingerprintPeriods>,
    pub deadline: Deadline,
    pub weights: &'a [WeightSeries<'a>],
    pub hierarchies: &'a [Vec<String>],
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

/// Report shares in the weight's units, while evaluating precision
/// on a row-equivalent scale. Normalize within each entity to avoid
/// overflow and keep an entity's support independent of its size.
pub(crate) fn effective_rows(mass: f64, squares: f64) -> f64 {
    if squares > 0.0 {
        mass * (mass / squares)
    } else {
        0.0
    }
}

struct FingerprintWeight<'a> {
    name: &'a str,
    /// Weight relative to the largest positive weight for this entity.
    normalized: Vec<f64>,
    entity_maxima: Vec<f64>,
}

impl<'a> FingerprintWeight<'a> {
    fn new(series: &'a WeightSeries<'a>, codes: &[Option<u32>], n: usize) -> Self {
        let positive = |value: &Option<f64>| value.filter(|w| w.is_finite() && *w > 0.0);
        let mut maxima = vec![0.0f64; n];
        for (&code, value) in codes.iter().zip(series.values) {
            if let (Some(code), Some(value)) = (code, positive(value)) {
                maxima[code as usize] = maxima[code as usize].max(value);
            }
        }
        let normalized = codes
            .iter()
            .zip(series.values)
            .map(|(&code, value)| match (code, positive(value)) {
                (Some(code), Some(value)) => value / maxima[code as usize],
                _ => 0.0,
            })
            .collect();
        Self {
            name: series.name,
            normalized,
            entity_maxima: maxima,
        }
    }
}

struct MixDimension {
    columns: Vec<String>,
    codes: Vec<Option<u32>>,
    labels: Vec<String>,
    /// Each observed joint level's two marginal codes.
    joint: Vec<(usize, usize)>,
}

impl MixDimension {
    fn label(&self) -> String {
        self.columns.join(" × ")
    }

    fn new(table: &TypedTable, a: usize, b: Option<usize>) -> Result<Self, String> {
        let mut columns = vec![table.names[a].clone()];
        if let Some(b) = b {
            columns.push(table.names[b].clone());
        }
        let (left_codes, left_labels) = dimension_codes(table, a)?;
        let Some(b) = b else {
            return Ok(Self {
                columns,
                codes: left_codes,
                labels: left_labels,
                joint: vec![],
            });
        };
        let (right_codes, right_labels) = dimension_codes(table, b)?;
        let mut index = BTreeMap::new();
        let mut labels = Vec::new();
        let mut codes = Vec::with_capacity(table.n);
        let mut joint = Vec::new();
        for (a, b) in left_codes.into_iter().zip(right_codes) {
            let code = match a.zip(b) {
                Some((a, b)) => {
                    let pair = (a as usize, b as usize);
                    let code = *index.entry(pair).or_insert_with(|| {
                        let code = joint.len() as u32;
                        joint.push(pair);
                        labels.push(format!(
                            "{:?} × {:?}",
                            left_labels[pair.0], right_labels[pair.1]
                        ));
                        code
                    });
                    if labels.len() > MAX_LEVELS {
                        return Err(format!("over {MAX_LEVELS} observed joint levels"));
                    }
                    Some(code)
                }
                None => None,
            };
            codes.push(code);
        }
        Ok(Self {
            columns,
            codes,
            labels,
            joint,
        })
    }
}

fn dimension_codes(
    table: &TypedTable,
    column: usize,
) -> Result<(Vec<Option<u32>>, Vec<String>), String> {
    let (codes, n, labels) = crate::panels::entity_index(table, column)
        .map_err(|_| "fewer than two levels or unsupported categorical values".to_string())?;
    if !(2..=MAX_LEVELS).contains(&n) {
        return Err(format!("{n} levels, outside 2..={MAX_LEVELS}"));
    }
    Ok((codes, labels))
}

/// Excess joint deviance after subtracting both marginal deviances.
/// Per effective row this is twice the mean within-entity mutual
/// information minus the pooled mutual information, with the degrees
/// of freedom subtracted as a noise correction. It is a conservative
/// screen, not a test of homogeneous log odds.
fn joint_excess(rows: &[&Vec<f64>], joint: &[(usize, usize)]) -> f64 {
    let rows: Vec<_> = rows
        .iter()
        .copied()
        .filter(|r| r.iter().sum::<f64>() >= 10.0)
        .collect();
    if rows.len() < MIN_ENTITIES {
        return 0.0;
    }
    let deviance = |rows: &[&Vec<f64>]| {
        let mut pooled = vec![0.0; rows[0].len()];
        for r in rows {
            for (p, v) in pooled.iter_mut().zip(*r) {
                *p += v;
            }
        }
        let total = pooled.iter().sum::<f64>();
        let statistic: f64 = rows
            .iter()
            .map(|r| {
                let n = r.iter().sum::<f64>();
                r.iter()
                    .zip(&pooled)
                    .filter(|(&x, _)| x > 0.0)
                    .map(|(&x, &p)| 2.0 * x * (x * total / (n * p)).ln())
                    .sum::<f64>()
            })
            .sum();
        let df = (rows.len() - 1) * (pooled.iter().filter(|&&x| x > 0.0).count() - 1);
        (statistic, df, total)
    };
    let (g, df, total) = deviance(&rows);
    let marginal = |code: fn(&(usize, usize)) -> usize| {
        let width = joint.iter().map(code).max().unwrap_or(0) + 1;
        let projected: Vec<_> = rows
            .iter()
            .map(|r| {
                let mut m = vec![0.0; width];
                for (cell, &x) in joint.iter().zip(*r) {
                    m[code(cell)] += x;
                }
                m
            })
            .collect();
        deviance(&projected.iter().collect::<Vec<_>>())
    };
    let (a, da, _) = marginal(|p| p.0);
    let (b, db, _) = marginal(|p| p.1);
    (g - a - b - df.saturating_sub(da + db) as f64) / total
}

/// Run the pass. Returns findings and the coverage detail.
pub fn entity_fingerprints(
    table: &TypedTable,
    input: &FingerprintInput<'_>,
) -> (Vec<Finding>, String, bool) {
    let ename = table.names[input.entity].clone();
    let Ok((ecodes, n_ent, elabels)) = crate::panels::entity_index(table, input.entity) else {
        return (
            vec![],
            format!("entity column {ename} cannot key entities"),
            false,
        );
    };
    let min_entities = if input.hierarchies.is_empty() {
        MIN_ENTITIES
    } else {
        MIN_PEERS + 1
    };
    if n_ent < min_entities {
        return (
            vec![],
            format!("{n_ent} entities in {ename}; the pass needs at least {min_entities}"),
            false,
        );
    }
    let mut findings = Vec::new();
    let mut notes = Vec::new();
    let peers = HierarchyPeers::resolve(
        table,
        input.entity,
        &ecodes,
        n_ent,
        input.hierarchies,
        &mut notes,
    );
    let mut dimensions: Vec<_> = input
        .dimensions
        .iter()
        .copied()
        .filter(|&d| d != input.entity && peers.as_ref().is_none_or(|p| !p.columns.contains(&d)))
        .collect();
    dimensions.sort_by_key(|&d| &table.names[d]);
    let all_pairs = dimensions.len() * dimensions.len().saturating_sub(1) / 2;
    let pairs: Vec<_> = dimensions
        .iter()
        .enumerate()
        .flat_map(|(i, &a)| dimensions[i + 1..].iter().map(move |&b| (a, Some(b))))
        .take(MAX_PAIRS)
        .collect();
    let pairs_capped = pairs.len() < all_pairs;
    let mut limited = pairs_capped;
    let marginal_count = dimensions.len();
    let pair_count = pairs.len();
    let universe = marginal_count + pair_count;
    let weights = input
        .weights
        .iter()
        .map(|w| FingerprintWeight::new(w, &ecodes, n_ent))
        .collect::<Vec<_>>();
    let bases: Vec<_> = if weights.is_empty() {
        vec![None]
    } else {
        weights.iter().map(Some).collect()
    };
    let basis_count = bases.len();
    if pairs_capped {
        notes.push(format!(
            "joint dimensions: first {} of {all_pairs} pairs in column-name order",
            pairs.len()
        ));
    }
    let mut considered = 0;
    'dimensions: for (d, partner) in dimensions.into_iter().map(|d| (d, None)).chain(pairs) {
        if input.deadline.exceeded() {
            limited = true;
            break;
        }
        considered += 1;
        let dim = match MixDimension::new(table, d, partner) {
            Ok(dim) => dim,
            Err(reason) => {
                let a = &table.names[d];
                let name =
                    partner.map_or_else(|| a.clone(), |b| format!("{a} × {}", table.names[b]));
                notes.push(format!("{name}: {reason}"));
                continue;
            }
        };
        let dname = dim.label();
        let dlabels = &dim.labels;
        let n_lev = dlabels.len();
        if n_lev < 2 {
            notes.push(format!("{dname}: fewer than two complete levels"));
            continue;
        }
        for weight in &bases {
            let weight_name = weight.map(|w| w.name);
            if weight_name.is_some_and(|w| dim.columns.iter().any(|d| d == w)) {
                notes.push(format!("{dname}: weight column is a dimension in this mix"));
                continue;
            }
            if input.deadline.exceeded() {
                limited = true;
                break 'dimensions;
            }
            let (capped, stopped) = scan_mix(
                MixInput {
                    entity: &ename,
                    entity_codes: &ecodes,
                    entity_labels: &elabels,
                    dimension: &dim,
                    dimension_name: &dname,
                    weight: *weight,
                    periods: input.periods.as_ref(),
                    peers: peers.as_ref(),
                    deadline: input.deadline,
                    test_count: partner.map_or(marginal_count, |_| pair_count) * basis_count,
                    seed: d as u64,
                },
                &mut findings,
                &mut notes,
            );
            limited |= capped || stopped;
            if stopped {
                break 'dimensions;
            }
        }
    }
    notes.push(format!("dimension scans: {considered} of {universe} started ({all_pairs} pair candidates before caps)"));
    (
        findings,
        format!(
            "entity {ename}; weight: {}; {}; dimensions: {}",
            if weights.is_empty() {
                "row count".to_string()
            } else {
                weights
                    .iter()
                    .map(|w| w.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            input.periods.as_ref().map_or_else(
                || "drift skipped: no resolved axis with per-row keys".into(),
                FingerprintPeriods::detail,
            ),
            notes.join("; "),
        ),
        limited,
    )
}

struct Misfit {
    entity: usize,
    group: usize,
    p: f64,
    level: usize,
    reference_share: f64,
}

/// Middle values of one share coordinate. These are enough to
/// evaluate the median with or without one group member in O(1).
struct ShareMedian {
    count: usize,
    lower: f64,
    middle: f64,
    upper: f64,
}

impl ShareMedian {
    fn from_values(mut values: Vec<f64>) -> Self {
        values.sort_unstable_by(f64::total_cmp);
        let count = values.len();
        let mid = count / 2;
        Self {
            count,
            lower: values[mid - 1],
            middle: values[mid],
            upper: values[mid + 1],
        }
    }

    fn estimate(&self, omit: Option<f64>) -> f64 {
        let even = self.count.is_multiple_of(2);
        match omit {
            None if even => 0.5 * (self.lower + self.middle),
            None => self.middle,
            Some(x) if even => {
                if x <= self.lower {
                    self.middle
                } else {
                    self.lower
                }
            }
            Some(x) if x < self.middle => 0.5 * (self.middle + self.upper),
            Some(x) if x > self.middle => 0.5 * (self.lower + self.middle),
            Some(_) => 0.5 * (self.lower + self.upper),
        }
    }
}

/// The null is compatibility with at least one supported profile,
/// so use the highest p-value across profiles. An individual
/// contributes neither to its own group reference nor to the
/// pooled smoothing distribution used for that test.
fn profile_misfits(
    rows: &[&Vec<f64>],
    n: &[f64],
    pooled: &[f64],
    groups: &[Vec<usize>],
    used: usize,
    bar: f64,
) -> Vec<Misfit> {
    let n_lev = pooled.len();
    let total: f64 = n.iter().sum();
    let mut references = Vec::with_capacity(groups.len());
    let mut membership = vec![None; n.len()];
    for (g, members) in groups.iter().enumerate() {
        let supported: Vec<_> = members
            .iter()
            .copied()
            .filter(|&i| n[i] >= MIN_MISFIT_ROWS)
            .collect();
        let mut sums = vec![0.0; n_lev];
        for &i in &supported {
            membership[i] = Some(g);
            for (sum, &value) in sums.iter_mut().zip(rows[i]) {
                *sum += value;
            }
        }
        let medians: Vec<_> = (0..n_lev)
            .map(|j| {
                ShareMedian::from_values(supported.iter().map(|&i| rows[i][j] / n[i]).collect())
            })
            .collect();
        let sum = sums.iter().sum::<f64>();
        references.push((medians, sums, sum));
    }
    let mut out = Vec::new();
    for (i, &ni) in n.iter().enumerate() {
        if ni < MIN_MISFIT_ROWS {
            continue;
        }
        let mut best = None::<Misfit>;
        let mut material = true;
        for (g, (medians, sums, reference_total)) in references.iter().enumerate() {
            let own = membership[i] == Some(g);
            let ng = reference_total - if own { ni } else { 0.0 };
            let center: Vec<_> = medians
                .iter()
                .enumerate()
                .map(|(j, median)| median.estimate(own.then(|| rows[i][j] / ni)))
                .collect();
            let center_total = center.iter().sum::<f64>();
            let mut x2 = 0.0;
            let mut level = 0;
            let mut reference_share = 0.0;
            let mut difference: f64 = -1.0;
            let mut distance = 0.0;
            for (j, (&group_count, &base)) in sums.iter().zip(pooled).enumerate() {
                let observed = rows[i][j];
                let group_share = if center_total > 0.0 {
                    center[j] / center_total
                } else {
                    (group_count - if own { observed } else { 0.0 }).max(0.0) / ng
                };
                let baseline = ((total * base - observed) / (total - ni)).max(0.0);
                let q = 0.98 * group_share + 0.02 * baseline;
                if q > 0.0 {
                    x2 += (observed - ni * q).powi(2) / (ni * q);
                } else if observed > 0.0 {
                    x2 = f64::INFINITY;
                }
                let delta = (observed / ni - q).abs();
                distance += delta / 2.0;
                if delta > difference {
                    difference = delta;
                    level = j;
                    reference_share = q;
                }
            }
            if distance < MIN_MISFIT_DISTANCE {
                // Rare cells can make the test significant even when
                // the difference in the overall mix is immaterial.
                material = false;
                break;
            }
            let p = if x2.is_infinite() {
                0.0
            } else {
                chi2_sf(x2, (used - 1) as f64)
            };
            if best.as_ref().is_none_or(|b| p > b.p) {
                best = Some(Misfit {
                    entity: i,
                    group: g,
                    p,
                    level,
                    reference_share,
                });
            }
            if p >= bar {
                break;
            }
        }
        if let Some(best) = best.filter(|best| material && best.p < bar) {
            out.push(best);
        }
    }
    out
}

struct MixInput<'a> {
    entity: &'a str,
    entity_codes: &'a [Option<u32>],
    entity_labels: &'a [String],
    dimension: &'a MixDimension,
    dimension_name: &'a str,
    weight: Option<&'a FingerprintWeight<'a>>,
    periods: Option<&'a FingerprintPeriods>,
    peers: Option<&'a HierarchyPeers>,
    deadline: Deadline,
    test_count: usize,
    seed: u64,
}

/// Scan one view and record whether its cohort was capped or the
/// deadline interrupted the scan.
fn scan_mix(
    cfg: MixInput<'_>,
    findings: &mut Vec<Finding>,
    notes: &mut Vec<String>,
) -> (bool, bool) {
    let MixInput {
        entity: ename,
        entity_codes: ecodes,
        entity_labels: elabels,
        dimension: dim,
        dimension_name: dname,
        weight,
        periods,
        peers,
        deadline,
        test_count,
        seed,
    } = cfg;
    let n_ent = elabels.len();
    let dlabels = &dim.labels;
    let n_lev = dlabels.len();
    let partner = dim.columns.get(1);
    let weight_name = weight.map(|w| w.name);
    let mix_unit = weight_name.map_or_else(|| "rows".to_string(), |w| format!("'{w}'"));
    let mix_label = weight_name.map_or_else(
        || dname.to_string(),
        |w| format!("{dname} weighted by '{w}'"),
    );
    let mut subject_columns = vec![ename.to_string()];
    subject_columns.extend(dim.columns.iter().cloned());
    subject_columns.extend(weight_name.map(str::to_string));
    let subject = || BTreeMap::from([("columns".into(), json!(subject_columns))]);
    let mut counts = vec![vec![0.0f64; n_lev]; n_ent];
    let mut squares = vec![0.0; n_ent];
    let mut row_counts = vec![0usize; n_ent];
    for (i, (e, l)) in ecodes.iter().zip(&dim.codes).enumerate() {
        if let (Some(e), Some(l)) = (e, l) {
            let w = weight.map_or(1.0, |w| w.normalized[i]);
            if w > 0.0 {
                counts[*e as usize][*l as usize] += w;
                squares[*e as usize] += w * w;
                row_counts[*e as usize] += 1;
            }
        }
    }
    let mass: Vec<f64> = counts.iter().map(|r| r.iter().sum()).collect();
    let support: Vec<f64> = mass
        .iter()
        .zip(squares)
        .map(|(&w, w2)| effective_rows(w, w2))
        .collect();
    let require_support = weight.is_some() || partner.is_some();
    let mut ents: Vec<usize> = (0..n_ent)
        .filter(|&e| mass[e] > 0.0 && (!require_support || support[e] >= 10.0 - 1e-9))
        .collect();
    if require_support {
        let positive = mass.iter().filter(|&&m| m > 0.0).count();
        if ents.len() < positive {
            notes.push(format!(
                "{mix_label}: {} of {positive} entities have fewer than 10 effective rows",
                positive - ents.len()
            ));
        }
    }
    ents.sort_by(|&a, &b| {
        support[b]
            .total_cmp(&support[a])
            .then_with(|| elabels[a].cmp(&elabels[b]))
    });
    let capped = ents.len() > MAX_ENTITIES;
    if capped {
        notes.push(format!(
            "{mix_label}: {MAX_ENTITIES} of {} entities with values examined",
            ents.len()
        ));
    }
    ents.truncate(MAX_ENTITIES);
    if weight.is_some() || partner.is_some() {
        ents.sort_by_key(|&e| &elabels[e]);
    } else {
        ents.sort_unstable();
    }
    let e_n = ents.len();
    let min_entities = if peers.is_some() {
        MIN_PEERS + 1
    } else {
        MIN_ENTITIES
    };
    if e_n < min_entities {
        notes.push(format!(
            "{mix_label}: {e_n} entities with values; at least {min_entities} needed"
        ));
        return (capped, false);
    }
    let scaled;
    let rows: Vec<&Vec<f64>> = if weight.is_some() {
        scaled = ents
            .iter()
            .map(|&e| counts[e].iter().map(|c| c * support[e] / mass[e]).collect())
            .collect::<Vec<_>>();
        scaled.iter().collect()
    } else {
        ents.iter().map(|&e| &counts[e]).collect()
    };
    if let Some(periods) = periods.filter(|_| e_n >= MIN_ENTITIES) {
        let (mut changes, detail, stopped) = fingerprints_drift::scan(DriftInput {
            periods,
            entity: ename,
            dimension: dname,
            dimension_columns: &dim.columns,
            joint_levels: &dim.joint,
            weight: weight.map(|w| (w.name, w.normalized.as_slice())),
            entity_labels: elabels,
            level_labels: dlabels,
            entities: &ents,
            entity_codes: ecodes,
            level_codes: &dim.codes,
            dimension_count: test_count,
            peers,
            deadline,
        });
        notes.push(format!("{mix_label}: {detail}"));
        findings.append(&mut changes);
        if stopped {
            return (capped, true);
        }
    }
    if periods.is_some() && e_n < MIN_ENTITIES {
        notes.push(format!(
            "{mix_label}: fewer than {MIN_ENTITIES} entities for drift"
        ));
    }
    let joint_gain = if partner.is_some() {
        let gain = joint_excess(&rows, &dim.joint);
        if gain < MIN_JOINT_EXCESS {
            notes.push(format!("{mix_label}: no additional joint signal (excess deviance per effective row {gain:.3})"));
            return (capped, false);
        }
        Some(gain)
    } else {
        None
    };
    if let Some(hierarchy) = peers {
        let (finding, detail) = fingerprints_peers::outliers(PeerInput {
            hierarchy,
            entity: ename,
            entities: &ents,
            entity_labels: elabels,
            dimension: dname,
            dimension_columns: &dim.columns,
            level_labels: dlabels,
            rows: &rows,
            weight: weight_name,
            test_count,
        });
        findings.extend(finding);
        notes.push(format!("{mix_label}: {detail}"));
    }
    if e_n < MIN_ENTITIES {
        notes.push(format!(
            "{mix_label}: fewer than {MIN_ENTITIES} entities for group discovery"
        ));
        return (capped, false);
    }
    let (v, x2, df) = excess_v(&rows);
    let p = chi2_sf(x2, df);
    if v < MIN_EXCESS_V || p > 0.05 / test_count.max(1) as f64 {
        notes.push(format!(
        "{mix_label}: entities do not differ in mix beyond sampling noise (excess V {v:.3}, chi-square {x2:.0} on {df:.0} df)"
    ));
        return (capped, false);
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
    let mut rng = Rng(0x05ee_d448 ^ seed << 20);
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
            (refs.iter().map(|r| (r[k] - mu).powi(2)).sum::<f64>() / GAP_REFERENCES as f64).sqrt()
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
    let largest = weight.map_or(1.0, |w| {
        ents.iter().map(|&e| w.entity_maxima[e]).fold(0.0, f64::max)
    });
    let relative_maximum = |e: usize| weight.map_or(1.0, |w| w.entity_maxima[e] / largest);
    let share_of = |members: &[usize]| -> Vec<f64> {
        let group_max = weight.map_or(1.0, |w| {
            members
                .iter()
                .map(|&i| w.entity_maxima[ents[i]])
                .fold(0.0, f64::max)
        });
        let totals: Vec<f64> = (0..n_lev)
            .map(|j| {
                members
                    .iter()
                    .map(|&i| {
                        let e = ents[i];
                        counts[e][j] * weight.map_or(1.0, |w| w.entity_maxima[e] / group_max)
                    })
                    .sum::<f64>()
            })
            .collect();
        let mass = totals.iter().sum::<f64>();
        totals
            .into_iter()
            .map(|c| if mass > 0.0 { c / mass } else { 0.0 })
            .collect()
    };
    let overall_mix = share_of(&(0..e_n).collect::<Vec<_>>());
    let mass_by_entity: Vec<f64> = ents
        .iter()
        .map(|&e| mass[e] * relative_maximum(e))
        .collect();
    let whole_mass = mass_by_entity.iter().sum::<f64>();
    let scope = format!(
        "{e_n} {ename} values{}",
        if capped {
            format!(" (the {MAX_ENTITIES} with greatest support)")
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
            (
                "n_rows".to_string(),
                json!(ents.iter().map(|&e| row_counts[e]).sum::<usize>()),
            ),
        ]);
        if let Some(weight) = weight_name {
            m.insert("weight".into(), json!(weight));
            m.insert("effective_rows".into(), json!(total));
        }
        if let Some(gain) = joint_gain {
            m.insert("dimensions".into(), json!(dim.columns));
            m.insert("joint_excess_deviance_per_row".into(), json!(gain));
        }
        for (key, val) in extra {
            m.insert((*key).to_string(), val.clone());
        }
        m
    };
    let model = if weight.is_some() {
        "effective counts"
    } else {
        "row counts"
    };
    let method = || {
        Method::new("entity_fingerprints", "entity_mix_groups", &format!("entity × level {model}; Hellinger embedding, gap-statistic k-means, homogeneity merge"))
    };
    if k >= 2 {
        let mut groups: Vec<Vec<usize>> = (0..k)
            .map(|g| (0..e_n).filter(|&i| lab[i] == g).collect::<Vec<_>>())
            .filter(|members| {
                members.iter().filter(|&&i| n[i] >= MIN_MISFIT_ROWS).count() >= MIN_GROUP_ENTITIES
            })
            .collect();
        groups.sort_by_key(|g| std::cmp::Reverse(g.len()));
        let supported = groups.len();
        if supported < k {
            notes.push(format!(
                "{mix_label}: {supported} of {k} candidate groups have at least {MIN_GROUP_ENTITIES} entities with {MIN_MISFIT_ROWS:.0} effective rows each"
            ));
        }
        if supported >= 2 {
            let mut parts = Vec::new();
            let mut payload = Vec::new();
            for mem in &groups {
                let (text, lv) = distinguishing(&share_of(mem), &overall_mix, dlabels);
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
            let claim = if supported == k {
                format!(
                    "{ename} fall into {supported} groups by their mix of {mix_label}: {}",
                    parts.join("; ")
                )
            } else {
                let assigned: usize = groups.iter().map(Vec::len).sum();
                format!("{assigned} of {e_n} {ename} fall into {supported} supported groups by their mix of {mix_label}: {}", parts.join("; "))
            };
            findings.push(Finding::new(
                Pattern::EntityGroups,
                subject(),
                claim,
                EvidenceBasis::Estimated,
                (0.5 + v).min(1.0),
                base_metrics(&[("groups", json!(payload)), ("scope", json!(scope))]),
                Some(format!("The model compares each {ename}'s share of {mix_unit} by {dname} level using {model}. Group and overall shares use pooled {mix_unit}.")),
                method(),
            ));
        }
        let bar = 0.05 / e_n as f64;
        if peers.is_none() && !groups.is_empty() {
            let mut misfits = profile_misfits(&rows, &n, &pooled, &groups, used, bar);
            if !misfits.is_empty() {
                misfits.sort_by(|a, b| {
                    a.p.total_cmp(&b.p)
                        .then_with(|| elabels[ents[a.entity]].cmp(&elabels[ents[b.entity]]))
                });
                let rows_share = misfits
                    .iter()
                    .map(|m| mass_by_entity[m.entity])
                    .sum::<f64>()
                    / whole_mass;
                let names = misfits
                    .iter()
                    .take(5)
                    .map(|m| elabels[ents[m.entity]].as_str())
                    .collect::<Vec<_>>();
                let claim = format!(
                    "{} {ename} ({}) match none of the {supported} supported groups by {mix_label}. Together they hold {} of {mix_unit}. Examples include {}",
                    misfits.len(), pct(misfits.len() as f64 / e_n as f64), pct(rows_share), names.join(", ")
                );
                let details = misfits.iter().map(|m| json!({
                    "entity": elabels[ents[m.entity]], "rows": row_counts[ents[m.entity]],
                    "effective_rows": n[m.entity], "p_value": m.p, "closest_group": m.group + 1,
                    "level": dlabels[m.level], "share": rows[m.entity][m.level] / n[m.entity],
                    "reference_share": m.reference_share,
                })).collect::<Vec<_>>();
                findings.push(Finding::new(
                    Pattern::EntityMisfits,
                    subject(),
                    claim,
                    EvidenceBasis::Estimated,
                    0.4 + 0.3 * (misfits.len() as f64 / e_n as f64).min(1.0),
                    base_metrics(&[("misfits", json!(details)), ("bar", json!(bar)), ("groups_tested", json!(supported))]),
                    Some("Each entity rejects every supported profile at the Bonferroni threshold. The evidence shows the level with the largest share difference from its best-fitting reference.".into()),
                    Method::new("entity_fingerprints", "entity_misfit_test", &format!("chi-square of an entity's {model} against each group’s median-share profile with the entity left out")),
                ));
            }
        }
    } else {
        let (ev1, u1) = &eig[0];
        let mut idx: Vec<usize> = (0..n_lev).collect();
        idx.sort_by(|&a, &b| u1[b].total_cmp(&u1[a]));
        let (pos, neg) = (idx[0], idx[n_lev - 1]);
        let claim = format!(
        "{ename} differ in their mix of {mix_label} along one continuous axis, not in groups: the share of '{}' against '{}' varies from {ename} to {ename} without clusters (excess Cramér's V {v:.2})",
        dlabels[pos], dlabels[neg]
    );
        findings.push(Finding::new(
        Pattern::EntityContinuum,
        subject(),
        claim,
        EvidenceBasis::Estimated,
        (0.4 + v).min(0.9),
        base_metrics(&[("axis_variance", json!(ev1)), ("axis_levels", json!([dlabels[pos], dlabels[neg]]))]),
        Some("the gap statistic finds no grouping beyond a uniform spread along the principal axes".into()),
        method(),
    ));
    }
    // Specialists.
    let row_support = if weight.is_some() {
        "effective rows"
    } else {
        "rows"
    };
    let spec_obs = (0..e_n)
        .filter(|&i| {
            n[i] >= 10.0 && eff_levels(&rows[i].iter().map(|c| c / n[i]).collect::<Vec<_>>()) <= 1.5
        })
        .count();
    let eligible = (0..e_n).filter(|&i| n[i] >= 10.0).count();
    if eligible >= MIN_ENTITIES && spec_obs as f64 >= 0.1 * eligible as f64 {
        // Under a common mix each row's level is drawn independently
        // at its own weight. Preserve the weight distribution in the
        // null, so one large row cannot manufacture specialists.
        let mut row_weights = vec![Vec::new(); e_n];
        if let Some(weight) = weight {
            let mut positions = vec![None; n_ent];
            for (i, &e) in ents.iter().enumerate() {
                positions[e] = Some(i);
            }
            for ((e, l), &w) in ecodes.iter().zip(&dim.codes).zip(&weight.normalized) {
                if let (Some(i), Some(_)) = (e.and_then(|e| positions[e as usize]), l) {
                    if w > 0.0 {
                        row_weights[i].push(w);
                    }
                }
            }
            for weights in &mut row_weights {
                weights.sort_unstable_by(f64::total_cmp);
            }
        }
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
                let mut draw = |w| {
                    let r = rng.next_f64();
                    c[cum.iter().position(|&q| r <= q).unwrap_or(n_lev - 1)] += w;
                };
                let draw_mass = if weight.is_some() {
                    row_weights[i].iter().copied().for_each(&mut draw);
                    mass[ents[i]]
                } else {
                    (0..row_counts[ents[i]]).for_each(|_| draw(1.0));
                    n[i]
                };
                if eff_levels(&c.iter().map(|x| x / draw_mass).collect::<Vec<_>>()) <= 1.5 {
                    sim += 1;
                }
            }
        }
        let sim_share = sim as f64 / (3 * eligible) as f64;
        let obs_share = spec_obs as f64 / eligible as f64;
        if sim_share <= obs_share / 3.0 {
            let claim = format!(
            "{} of {ename} ({spec_obs} of {eligible} with 10 or more {row_support}) concentrate on essentially one level of {mix_label} (effective number of levels 1.5 or less), against {:.1} effective levels in the overall mix",
            pct(obs_share), eff_levels(&overall_mix)
        );
            findings.push(Finding::new(
            Pattern::EntitySpecialists,
            subject(),
            claim,
            EvidenceBasis::Measured,
            0.4 + 0.4 * obs_share,
            base_metrics(&[("specialist_share", json!(obs_share)), ("null_share", json!(sim_share)), ("overall_effective_levels", json!(eff_levels(&overall_mix)))]),
            None,
            Method::new("entity_fingerprints", "entity_specialists", "effective number of levels per entity against multinomial sampling (onceover v0.1)"),
        ));
        }
    }
    notes.push(format!("{mix_label}: {k} group(s) over {scope}, excess V {v:.3}, {m} signal axes, gap-statistic k {k_gap}"));
    (capped, false)
}

#[cfg(test)]
mod tests {
    use super::profile_misfits;

    #[test]
    fn a_misfit_must_reject_every_supported_profile() {
        let mut counts = vec![vec![90.0, 10.0, 0.0]; 5];
        counts.extend(vec![vec![10.0, 90.0, 0.0]; 5]);
        counts.extend([
            vec![10.0, 90.0, 0.0], // Assigned to the first group, fits the second.
            vec![89.0, 10.0, 1.0], // Rare extra level, still close to the first group.
            vec![50.0, 50.0, 0.0], // Fits neither group.
        ]);
        let rows: Vec<_> = counts.iter().collect();
        let n = vec![100.0; rows.len()];
        let pooled: Vec<_> = (0..3)
            .map(|j| rows.iter().map(|r| r[j]).sum::<f64>() / n.iter().sum::<f64>())
            .collect();
        let groups = [vec![0, 1, 2, 3, 4, 10], vec![5, 6, 7, 8, 9]];
        let misfits = profile_misfits(&rows, &n, &pooled, &groups, 3, 0.05 / n.len() as f64);
        assert_eq!(misfits.iter().map(|m| m.entity).collect::<Vec<_>>(), [12]);
    }
}
