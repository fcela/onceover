//! Pairwise dependence screen: Pearson, Spearman, and Chatterjee's ξ
//! (both directions), redundancy groups, and arithmetic identity candidates.

use onceover_report::{Method, Pattern};
use std::collections::BTreeMap;

use onceover_core::TypedCol;
use onceover_report::{EvidenceBasis, Finding};
use serde_json::json;

/// Pairwise dependence results for one numeric pair.
#[derive(Clone, Debug, Default)]
pub struct PairDep {
    pub a: usize,
    pub b: usize,
    pub pearson: Option<f64>,
    pub spearman: Option<f64>,
    pub xi_ab: Option<f64>, // ξ(a → b): "b is a function of a" signal
    pub xi_ba: Option<f64>,
    pub n: usize,
    pub population_rows: usize,
    pub sampled: bool,
}

impl PairDep {
    pub fn max_signal(&self) -> f64 {
        [
            self.pearson.map(|v| v.abs()),
            self.spearman.map(|v| v.abs()),
            self.xi_ab,
            self.xi_ba,
        ]
        .into_iter()
        .flatten()
        .fold(0.0f64, f64::max)
    }
}

/// Extract f64 pairs (pairwise-complete) from two numeric columns.
pub fn pairs_of(a: &TypedCol, b: &TypedCol) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let get = |c: &TypedCol, i: usize| -> Option<f64> {
        match c {
            TypedCol::I64 { vals } => vals.at(i).map(|v| v as f64),
            TypedCol::F64 { vals } => vals.at(i),
            _ => None,
        }
    };
    let n = a.len().min(b.len());
    for i in 0..n {
        if let (Some(x), Some(y)) = (get(a, i), get(b, i)) {
            out.push((x, y));
        }
    }
    out
}

/// Extract bounded pair-complete observations over shared sampled row ids.
pub fn sampled_pairs(a: &TypedCol, b: &TypedCol, rows: &[u32]) -> Vec<(f64, f64)> {
    rows.iter()
        .filter_map(|&r| {
            Some((
                crate::encode::numeric_at(a, r as usize)?,
                crate::encode::numeric_at(b, r as usize)?,
            ))
        })
        .collect()
}

pub fn pearson(pairs: &[(f64, f64)]) -> Option<f64> {
    let n = pairs.len();
    if n < 30 {
        return None;
    }
    let mx = pairs.iter().map(|p| p.0).sum::<f64>() / n as f64;
    let my = pairs.iter().map(|p| p.1).sum::<f64>() / n as f64;
    let mut cov = 0.0;
    let mut vx = 0.0;
    let mut vy = 0.0;
    for (x, y) in pairs {
        let (dx, dy) = (x - mx, y - my);
        cov += dx * dy;
        vx += dx * dx;
        vy += dy * dy;
    }
    if vx <= 0.0 || vy <= 0.0 {
        return None;
    }
    Some(cov / (vx * vy).sqrt())
}

/// Average (mid)ranks of values, ties averaged.
pub fn midranks(vals: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..vals.len()).collect();
    idx.sort_by(|&a, &b| vals[a].total_cmp(&vals[b]));
    let mut ranks = vec![0.0f64; vals.len()];
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && vals[idx[j + 1]] == vals[idx[i]] {
            j += 1;
        }
        let avg = (i + j) as f64 / 2.0 + 1.0; // 1-based average rank
        for &k in &idx[i..=j] {
            ranks[k] = avg;
        }
        i = j + 1;
    }
    ranks
}

pub fn spearman(pairs: &[(f64, f64)]) -> Option<f64> {
    if pairs.len() < 30 {
        return None;
    }
    let xs: Vec<f64> = pairs.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = pairs.iter().map(|p| p.1).collect();
    let rx = midranks(&xs);
    let ry = midranks(&ys);
    let rank_pairs: Vec<(f64, f64)> = rx.into_iter().zip(ry).collect();
    pearson(&rank_pairs)
}

/// Chatterjee's ξ (Ann. Statist. 2021), tie-corrected per Eq. 1.2 — the
/// exact algorithm SciPy implements: stable sort by x (ties keep original
/// order), max-ranks r_i = #{j: y_j ≤ y_i}, l_i = #{j: y_j ≥ y_i}, and
/// ξ = 1 − n·Σ|r_{i+1} − r_i| / (2·Σ l_i(n − l_i)).
/// Returns None when x or y is constant / n too small.
pub fn chatterjee_xi(pairs: &[(f64, f64)]) -> Option<f64> {
    let n = pairs.len();
    if n < 30 {
        return None;
    }
    // x constant -> undefined
    let x0 = pairs[0].0;
    if pairs.iter().all(|p| p.0 == x0) {
        return None;
    }
    // Stable sort by x...
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| pairs[a].0.total_cmp(&pairs[b].0));

    // ...then break x-ties uniformly at random (Chatterjee's
    // recommendation; seeded => deterministic). Fixed orderings correlate
    // with data order and bias ξ badly when y encodes row position.
    let mut rng = crate::gbm::Lcg(0x9E37_79B9_7F4A_7C15 ^ (n as u64));
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && pairs[idx[j + 1]].0 == pairs[idx[i]].0 {
            j += 1;
        }
        if j > i {
            let len = j - i + 1;
            for k in (1..len).rev() {
                let r = (rng.next_u64() % (k as u64 + 1)) as usize;
                idx.swap(i + k, i + r);
            }
        }
        i = j + 1;
    }

    // y in (randomly tie-broken) x-order.
    let ys: Vec<f64> = idx.iter().map(|&i| pairs[i].1).collect();

    // y constant -> undefined.
    let y0 = ys[0];
    if ys.iter().all(|&v| v == y0) {
        return None;
    }

    // Sorted y for rank lookups: r_i = #{j: y_j <= y_i} (max-rank),
    // l_i = #{j: y_j >= y_i}.
    let mut sorted_y = ys.clone();
    sorted_y.sort_by(|a, b| a.total_cmp(b));

    let mut num: f64 = 0.0;
    let mut den: f64 = 0.0;
    let mut prev_r: f64 = 0.0;
    for (i, &y) in ys.iter().enumerate() {
        let r = sorted_y.partition_point(|&v| v <= y) as f64; // #{<= y}
        let less = sorted_y.partition_point(|&v| v < y); // #{< y}
        let l = (n - less) as f64; // #{>= y}
        den += 2.0 * l * (n as f64 - l);
        if i > 0 {
            num += (r - prev_r).abs();
        }
        prev_r = r;
    }
    if den <= 0.0 {
        return None;
    }
    Some(1.0 - (n as f64) * num / den)
}

/// Screen all numeric pairs (rayon-parallel, deterministic order).
pub fn screen(
    table: &onceover_core::TypedTable,
    pair_cap: usize,
) -> (Vec<PairDep>, Vec<Finding>, String) {
    screen_sampled(table, pair_cap, 20_000)
}

pub fn screen_sampled(
    table: &onceover_core::TypedTable,
    pair_cap: usize,
    row_cap: usize,
) -> (Vec<PairDep>, Vec<Finding>, String) {
    use rayon::prelude::*;

    let numeric: Vec<usize> = (0..table.cols.len())
        .filter(|&i| matches!(table.cols[i], TypedCol::I64 { .. } | TypedCol::F64 { .. }))
        .collect();
    // Width gate: past 25M pairs even the cheap stage's constant
    // is too large (7,071+ numeric columns); skip pair screening
    // and say so in coverage rather than truncating silently.
    // Checked arithmetically, before the pair list is built.
    const PAIR_GATE: usize = 25_000_000;
    let total_pairs = numeric.len() * numeric.len().saturating_sub(1) / 2;
    if total_pairs > PAIR_GATE {
        return (
            Vec::new(),
            Vec::new(),
            format!(
                "0 of {} numeric pairs (width-gated); pair screening skipped: {} pairs exceeds the {}-pair gate for the all-pairs stage",
                total_pairs, total_pairs, PAIR_GATE
            ),
        );
    }
    let mut pair_list: Vec<(usize, usize)> = Vec::new();
    for (i, &a) in numeric.iter().enumerate() {
        for &b in numeric.iter().skip(i + 1) {
            pair_list.push((a, b));
        }
    }
    let capped = total_pairs > pair_cap;
    // Cheap screening covers the entire pair universe in two
    // stages: stage 1 scores every pair on a bounded row
    // subsample (256 encoded rows; exact when the encoded sample
    // is smaller), stage 2 rescores the survivors on all encoded
    // rows and takes the cap. Detailed rank-based statistics use
    // a bounded row sample for the strongest candidates.
    if capped {
        let (encoded, _) = crate::encode::encode(
            table,
            &crate::EncodeConfig {
                row_cap: row_cap.min(2048),
                feature_cap: usize::MAX,
                nominal_max_levels: 32,
            },
        );
        // Direct-index slot lookup (column index -> encoded slot).
        let mut inv = vec![-1i32; table.cols.len()];
        for (f, &c) in encoded.feat_cols.iter().enumerate() {
            inv[c] = f as i32;
        }
        let positions: Vec<u32> = (0..encoded.n as u32).collect();
        // Stage 1: all pairs on a bounded row subsample.
        let stage1_rows = crate::stride_rows(encoded.n, 256);
        // Marginal entropies over the stage-1 rows, per encoded
        // slot; Some only where the column has no missing value
        // there, which makes the entropy pair-independent.
        let ents: Vec<Option<f64>> = encoded
            .bins
            .iter()
            .map(|bins| crate::screen::marginal_entropy(bins, &stage1_rows))
            .collect();
        let score_pairs =
            |rows: &[u32], pairs: Vec<(usize, usize)>, fast: bool| -> Vec<((usize, usize), f64)> {
                pairs
                    .into_par_iter()
                    .map(|(a, b)| {
                        let (x, y) = (inv[a], inv[b]);
                        let score = if x >= 0 && y >= 0 {
                            let (x, y) = (x as usize, y as usize);
                            if fast {
                                crate::screen::information_fast(
                                    &encoded.bins[x],
                                    &encoded.bins[y],
                                    rows,
                                    (ents[x], ents[y]),
                                )
                            } else {
                                crate::screen::information(&encoded.bins[x], &encoded.bins[y], rows)
                            }
                        } else {
                            0.0
                        };
                        ((a, b), score)
                    })
                    .collect()
            };
        let by_score = |ranked: &mut Vec<((usize, usize), f64)>| {
            ranked.sort_by(|a, b| {
                b.1.total_cmp(&a.1)
                    .then(table.names[a.0 .0].cmp(&table.names[b.0 .0]))
                    .then(table.names[a.0 .1].cmp(&table.names[b.0 .1]))
            });
        };
        let mut ranked = score_pairs(&stage1_rows, pair_list, true);
        // Keep the survivor set without sorting the whole pair
        // universe: partition at the survivor boundary score,
        // then fill boundary ties by column name — the result is
        // exactly the full sort's top `want` entries.
        let want = (pair_cap * 4).max(1024).min(ranked.len());
        if ranked.len() > want {
            let (_, pivot, _) = ranked.select_nth_unstable_by(want - 1, |a, b| b.1.total_cmp(&a.1));
            let boundary = pivot.1;
            let mut top: Vec<((usize, usize), f64)> =
                ranked.iter().filter(|r| r.1 > boundary).cloned().collect();
            let need = want - top.len();
            let mut tied: Vec<((usize, usize), f64)> =
                ranked.iter().filter(|r| r.1 == boundary).cloned().collect();
            tied.sort_by(|a, b| {
                table.names[a.0 .0]
                    .cmp(&table.names[b.0 .0])
                    .then(table.names[a.0 .1].cmp(&table.names[b.0 .1]))
            });
            top.extend(tied.into_iter().take(need));
            ranked = top;
        }
        by_score(&mut ranked);
        let survivors: Vec<(usize, usize)> = ranked.into_iter().map(|(pair, _)| pair).collect();
        // Stage 2: survivors rescored on all encoded rows.
        let mut ranked = score_pairs(&positions, survivors, false);
        by_score(&mut ranked);
        pair_list = ranked
            .into_iter()
            .take(pair_cap)
            .map(|(pair, _)| pair)
            .collect();
    }
    let rows = crate::stride_rows(table.n, row_cap);

    let deps: Vec<PairDep> = pair_list
        .par_iter()
        .map(|&(a, b)| {
            let pairs = sampled_pairs(&table.cols[a], &table.cols[b], &rows);
            PairDep {
                a,
                b,
                pearson: pearson(&pairs),
                spearman: spearman(&pairs),
                xi_ab: chatterjee_xi(&pairs),
                xi_ba: {
                    let flipped: Vec<(f64, f64)> = pairs.iter().map(|(x, y)| (*y, *x)).collect();
                    chatterjee_xi(&flipped)
                },
                n: pairs.len(),
                population_rows: table.n,
                sampled: rows.len() < table.n,
            }
        })
        .collect();

    // Findings: only strong pairs.
    let mut dep_findings: Vec<(usize, Finding)> = deps
        .iter()
        .filter(|d| d.n >= 30)
        .filter(|d| {
            let s = d.max_signal();
            s >= 0.5 || d.pearson.map(|v| v.abs()) == Some(1.0)
        })
        .map(|d| {
            let s = d.max_signal();
            (
                deps.iter().position(|x| std::ptr::eq(x, d)).unwrap_or(0),
                dep_finding(table, d, s),
            )
        })
        .collect();
    dep_findings.sort_by(|a, b| {
        b.1.interestingness
            .total_cmp(&a.1.interestingness)
            .then(a.0.cmp(&b.0))
    });
    dep_findings.truncate(20);
    let findings: Vec<Finding> = dep_findings.into_iter().map(|(_, f)| f).collect();

    let n_links = deps.iter().filter(|d| d.max_signal() >= 0.98).count();
    let coverage = format!(
        "{} of {} numeric pairs ({}); all {} pairs screened; detailed statistics over {} of {} rows; {} strong, {} redundancy links",
        pair_list.len(),
        total_pairs,
        if capped { "capped" } else { "full" },
        total_pairs, rows.len(), table.n,
        findings.len(),
        n_links,
    );

    (deps, findings, coverage)
}

fn dep_finding(table: &onceover_core::TypedTable, d: &PairDep, signal: f64) -> Finding {
    let a = &table.names[d.a];
    let b = &table.names[d.b];
    let mut evidence = BTreeMap::from([
        ("n".into(), json!(d.n)),
        ("population_rows".into(), json!(d.population_rows)),
        ("row_sampled".into(), json!(d.sampled)),
        ("pearson".into(), json!(d.pearson)),
        ("spearman".into(), json!(d.spearman)),
        ("xi_ab".into(), json!(d.xi_ab)),
        ("xi_ba".into(), json!(d.xi_ba)),
    ]);
    let claim =
        format!("{a} and {b} show numeric association (maximum measured effect {signal:.3})");
    evidence.insert("variables".into(), json!([a, b]));
    evidence.insert("method".into(), json!("pearson+spearman+chatterjee_xi"));
    Finding::new(
Pattern::StatisticalAssociation,
BTreeMap::from([("x".into(), json!(a)), ("y".into(), json!(b))]),
claim,
EvidenceBasis::Estimated,
0.35 + 0.4 * signal,
evidence,
None,
Method::new("numeric_dependence", "pearson_spearman_chatterjee", "Pearson/Spearman/Chatterjee ξ on disclosed row sample; pair selection by binned mutual information")
)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xi_detects_function_and_independence() {
        // y = x^2 on [0, 1]: xi(x->y) should be high.
        let pairs: Vec<(f64, f64)> = (0..1000)
            .map(|i| {
                let x = i as f64 / 999.0;
                (x, x * x)
            })
            .collect();
        let xi = chatterjee_xi(&pairs).unwrap();
        assert!(xi > 0.9, "xi = {xi}");

        // Deterministic pseudo-random independent data: xi should be low.
        let mut s = 0x9E3779B97F4A7C15u64;
        let mut next = || {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
        };
        let indep: Vec<(f64, f64)> = (0..5000).map(|_| (next(), next())).collect();
        let xi2 = chatterjee_xi(&indep).unwrap();
        assert!(xi2 < 0.05, "xi indep = {xi2}");

        // REGRESSION (false-positive bug): tied x (10 blocks) vs independent
        // continuous y must NOT inflate xi. The old ascending-rearrangement
        // implementation reported ~0.99 here.
        let tied_x: Vec<(f64, f64)> = (0..3000).map(|i| ((i % 10) as f64, next())).collect();
        let xi3 = chatterjee_xi(&tied_x).unwrap();
        assert!(xi3 < 0.1, "xi tied-x indep = {xi3}");

        // Tied y against a deterministic function: still high.
        let func: Vec<(f64, f64)> = (0..1000)
            .map(|i| {
                let x = i as f64 / 999.0;
                (x, if x > 0.5 { 1.0 } else { 0.0 })
            })
            .collect();
        let xi4 = chatterjee_xi(&func).unwrap();
        assert!(xi4 > 0.8, "xi step function = {xi4}");

        // ADVERSARIAL regression: x = block id, y = row index. Stable-order
        // tie breaking reported ξ ≈ 0.99 on this (row position leaks into
        // y); random tie breaking must report ≈ 0.
        let adversarial: Vec<(f64, f64)> = (0..3000).map(|i| ((i % 3) as f64, i as f64)).collect();
        let xi5 = chatterjee_xi(&adversarial).unwrap();
        assert!(xi5 < 0.1, "xi adversarial = {xi5}");
    }

    #[test]
    fn pearson_spearman_basics() {
        let pairs: Vec<(f64, f64)> = (0..100).map(|i| (i as f64, 2.0 * i as f64 + 1.0)).collect();
        assert!((pearson(&pairs).unwrap() - 1.0).abs() < 1e-12);
        assert!((spearman(&pairs).unwrap() - 1.0).abs() < 1e-12);
        // Monotone non-linear: spearman 1, pearson < 1.
        let pairs: Vec<(f64, f64)> = (1..=100).map(|i| (i as f64, (i * i) as f64)).collect();
        assert!((spearman(&pairs).unwrap() - 1.0).abs() < 1e-12);
        assert!(pearson(&pairs).unwrap() < 0.999);
    }

    #[test]
    fn midranks_handles_ties() {
        let r = midranks(&[1.0, 2.0, 2.0, 3.0]);
        assert_eq!(r, vec![1.0, 2.5, 2.5, 4.0]);
    }
}
