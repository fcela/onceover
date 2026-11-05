//! Pairwise dependence screen: Pearson, Spearman, and Chatterjee's ξ
//! (both directions), redundancy groups, and arithmetic identity candidates.

use crate::deadline::Deadline;
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
    let n = a.len().min(b.len());
    for i in 0..n {
        if let (Some(x), Some(y)) = (a.numeric_at(i), b.numeric_at(i)) {
            out.push((x, y));
        }
    }
    out
}

/// Extract bounded pair-complete observations over shared sampled row ids.
pub fn sampled_pairs(a: &TypedCol, b: &TypedCol, rows: &[u32]) -> Vec<(f64, f64)> {
    rows.iter()
        .filter_map(|&r| Some((a.numeric_at(r as usize)?, b.numeric_at(r as usize)?)))
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

/// Pairs are consumed in chunks of this many ordinals wherever
/// the universe is streamed (issue #117): large enough to keep
/// rayon's per-chunk overhead negligible, small enough that a
/// chunk's scored pairs stay a few MB.
const PAIR_CHUNK: usize = 1 << 18;

/// The pair at ordinal `k` (0-based) in the numeric-pair
/// universe: pairs are ordered lexicographically by position in
/// `numeric` — (0,1), (0,2), …, (0,m−1), (1,2), … — the order
/// the materialized pair list used to hold. The number of pairs
/// before first-position `i` is i·m − i·(i+1)/2; `i` is
/// recovered from the quadratic root and corrected by at most
/// a step either way, so the result is exact.
fn pair_at(numeric: &[usize], k: usize) -> (usize, usize) {
    let m = numeric.len();
    debug_assert!(m >= 2 && k < m * (m - 1) / 2);
    let before = |i: usize| i * m - i * (i + 1) / 2;
    let span = (2 * m - 1) as f64;
    let mut i = ((span - (span * span - 8.0 * k as f64).sqrt()) / 2.0) as usize;
    while i + 1 < m && before(i + 1) <= k {
        i += 1;
    }
    while before(i) > k {
        i -= 1;
    }
    (numeric[i], numeric[i + 1 + (k - before(i))])
}

/// Total-order wrapper for f64 scores, for the streaming
/// selection's bounded heap: `total_cmp` keeps the heap order
/// identical to the in-place partition's comparator, including
/// for signed zeros.
#[derive(PartialEq)]
struct TotalF64(f64);

impl Eq for TotalF64 {}

impl PartialOrd for TotalF64 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TotalF64 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// A stage-1 boundary-tie candidate in the streaming survivor
/// selection, ordered by (first column name, second column
/// name, pair ordinal) — the fill order of the in-place
/// selection, with the ordinal making full ties deterministic.
struct TieCand {
    name_a: String,
    name_b: String,
    ord: usize,
    pair: (usize, usize),
}

impl PartialEq for TieCand {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for TieCand {}

impl PartialOrd for TieCand {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TieCand {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name_a
            .cmp(&other.name_a)
            .then_with(|| self.name_b.cmp(&other.name_b))
            .then_with(|| self.ord.cmp(&other.ord))
    }
}

/// What the dependence screen reports, including whether the
/// deadline stopped it mid-search (issue #117, stage 2). The
/// pipeline reads `deadline_stopped` to mark the coverage
/// entry `capped` and latch the profile's deadline state; the
/// coverage string itself carries the examined counts.
pub struct DepOutcome {
    pub deps: Vec<PairDep>,
    pub findings: Vec<Finding>,
    pub coverage: String,
    pub deadline_stopped: bool,
}

/// Screen all numeric pairs (rayon-parallel, deterministic order).
pub fn screen_sampled(
    table: &onceover_core::TypedTable,
    pair_cap: usize,
    row_cap: usize,
    excluded: &[usize],
    deadline: Deadline,
) -> DepOutcome {
    screen_sampled_impl(table, pair_cap, row_cap, excluded, &|| deadline.exceeded())
}

fn screen_sampled_impl(
    table: &onceover_core::TypedTable,
    pair_cap: usize,
    row_cap: usize,
    excluded: &[usize],
    stop: &dyn Fn() -> bool,
) -> DepOutcome {
    use rayon::prelude::*;

    // Frame-role scaffolding stays in the pair universe (the pairs
    // feed the identities and period passes, whose own targeting
    // rules apply), but a dependence *finding* about a pair that
    // involves a scaffolding column is a statement about the
    // frame's construction, not about the data: findings are
    // filtered at emission below, and the filter is recorded in
    // coverage.
    let numeric: Vec<usize> = (0..table.cols.len())
        .filter(|&i| {
            matches!(
                table.cols[i],
                TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. }
            )
        })
        .collect();
    // Width gate: past 25M pairs even the cheap stage's constant
    // is too large (7,071+ numeric columns); skip pair screening
    // and say so in coverage rather than truncating silently.
    // Checked arithmetically, before any pair is enumerated.
    const PAIR_GATE: usize = 25_000_000;
    let total_pairs = numeric.len() * numeric.len().saturating_sub(1) / 2;
    if total_pairs > PAIR_GATE {
        return DepOutcome {
            deps: Vec::new(),
            findings: Vec::new(),
            coverage: format!(
                "0 of {} numeric pairs (width-gated); pair screening skipped: {} pairs exceeds the {}-pair gate for the all-pairs stage",
                total_pairs, total_pairs, PAIR_GATE
            ),
            deadline_stopped: false,
        };
    }
    // The frame-role withholding note is part of every
    // coverage string this function returns, including the
    // deadline early return below, so build it once here.
    let excluded_suffix = {
        let excluded_numeric: Vec<&str> = excluded
            .iter()
            .filter(|&&i| {
                matches!(
                    table.cols[i],
                    TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. }
                )
            })
            .map(|&i| table.names[i].as_str())
            .collect();
        if excluded_numeric.is_empty() {
            String::new()
        } else {
            format!(
                "; dependence findings involving {} column(s) withheld by frame role: {}",
                excluded_numeric.len(),
                excluded_numeric.join(", ")
            )
        }
    };
    let capped = total_pairs > pair_cap;
    // The pair universe is enumerated by ordinal, never
    // materialized: at 5,000 numeric columns it holds ~12.5M
    // pairs. `pair_at` maps an ordinal to its pair in the same
    // lexicographic order the list used to hold, and every stage
    // consumes the universe in bounded chunks (issue #117), so
    // peak pair storage is O(chunk + survivors), not O(pairs).
    // `pair_list` survives only as the capped path's final
    // survivor list, bounded by `pair_cap` by construction.
    let mut pair_list: Vec<(usize, usize)> = Vec::new();
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
        let score_one = |a: usize, b: usize, rows: &[u32], fast: bool| -> f64 {
            let (x, y) = (inv[a], inv[b]);
            if x >= 0 && y >= 0 {
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
            }
        };
        let score_pairs =
            |rows: &[u32], pairs: Vec<(usize, usize)>, fast: bool| -> Vec<((usize, usize), f64)> {
                pairs
                    .into_par_iter()
                    .map(|(a, b)| ((a, b), score_one(a, b, rows, fast)))
                    .collect()
            };
        // Score pairs by ordinal range, in pair order, without
        // materializing the pairs first (issue #117).
        let score_range = |rows: &[u32],
                           start: usize,
                           end: usize,
                           fast: bool|
         -> Vec<(usize, (usize, usize), f64)> {
            (start..end)
                .into_par_iter()
                .map(|k| {
                    let (a, b) = pair_at(&numeric, k);
                    (k, (a, b), score_one(a, b, rows, fast))
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
        // Survivor selection keeps exactly the full sort's top
        // `want` entries: every pair above the boundary score,
        // then boundary ties filled by column name. Two routes
        // produce that set in that order. While the universe
        // fits the survivor bound it is scored into one list
        // and partitioned in place. Above it, the universe is
        // streamed in chunks (issue #117): a first pass retains
        // only the `want` largest scores, whose minimum is the
        // boundary; a second pass — scoring is a pure function
        // of the encoded matrix, so it reproduces every score
        // bit-for-bit — collects the pairs above the boundary
        // in pair order and the name-smallest boundary ties.
        // Selection state is O(want); nothing is O(pairs).
        let stream_over = (pair_cap * 4).max(1024);
        // Set by the streaming stage-1 loops when the deadline
        // stops one of them: (which pass, pairs scored in it).
        let mut stage1_stop: Option<(&'static str, usize)> = None;
        let mut ranked = if total_pairs <= stream_over {
            let all_pairs: Vec<(usize, usize)> =
                (0..total_pairs).map(|k| pair_at(&numeric, k)).collect();
            let mut ranked = score_pairs(&stage1_rows, all_pairs, true);
            let want = stream_over.min(ranked.len());
            if ranked.len() > want {
                let (_, pivot, _) =
                    ranked.select_nth_unstable_by(want - 1, |a, b| b.1.total_cmp(&a.1));
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
            ranked
        } else {
            let want = stream_over;
            // Pass 1: the boundary is the `want`-th largest
            // score; retain only the `want` largest scores.
            let mut best: std::collections::BinaryHeap<std::cmp::Reverse<TotalF64>> =
                std::collections::BinaryHeap::new();
            let mut start = 0usize;
            while start < total_pairs {
                // Deadline checkpoint (issue #117, stage 2):
                // the work quantum is one PAIR_CHUNK of the
                // universe, so the overshoot past the deadline
                // is bounded by one chunk's scoring.
                if stop() {
                    stage1_stop = Some(("first", start));
                    break;
                }
                let end = (start + PAIR_CHUNK).min(total_pairs);
                for (_, _, score) in score_range(&stage1_rows, start, end, true) {
                    if best.len() < want {
                        best.push(std::cmp::Reverse(TotalF64(score)));
                    } else if let Some(&std::cmp::Reverse(TotalF64(min))) = best.peek() {
                        if score.total_cmp(&min) == std::cmp::Ordering::Greater {
                            best.pop();
                            best.push(std::cmp::Reverse(TotalF64(score)));
                        }
                    }
                }
                start = end;
            }
            let boundary = best.peek().map(|r| r.0 .0).unwrap_or(f64::NEG_INFINITY);
            // Pass 2: pairs above the boundary in pair order,
            // and the name-smallest boundary ties.
            let mut above: Vec<((usize, usize), f64)> = Vec::new();
            let mut ties: std::collections::BinaryHeap<TieCand> =
                std::collections::BinaryHeap::new();
            let mut start = 0usize;
            while start < total_pairs && stage1_stop.is_none() {
                if stop() {
                    stage1_stop = Some(("second", start));
                    break;
                }
                let end = (start + PAIR_CHUNK).min(total_pairs);
                for (k, (a, b), score) in score_range(&stage1_rows, start, end, true) {
                    if score > boundary {
                        above.push(((a, b), score));
                    } else if score == boundary {
                        ties.push(TieCand {
                            name_a: table.names[a].clone(),
                            name_b: table.names[b].clone(),
                            ord: k,
                            pair: (a, b),
                        });
                        if ties.len() > want {
                            ties.pop();
                        }
                    }
                }
                start = end;
            }
            let need = want - above.len();
            let mut top = above;
            top.extend(
                ties.into_sorted_vec()
                    .into_iter()
                    .take(need)
                    .map(|t| (t.pair, boundary)),
            );
            top
        };
        if let Some((pass, scored)) = stage1_stop {
            // Survivor selection is a global ranking over the
            // complete stage-1 screen; a partial screen cannot
            // produce it without silently changing the
            // selection rule. The screen therefore reports no
            // pairs rather than a truncated ranking, and says
            // exactly how far it got (issue #117, stage 2).
            // The routes not covered by checkpoints are
            // bounded by construction: the in-place route
            // scores at most `stream_over` pairs, and stage 2
            // rescores at most `stream_over` survivors.
            return DepOutcome {
                deps: Vec::new(),
                findings: Vec::new(),
                coverage: format!(
                    "0 of {} numeric pairs (deadline); stage-1 screening stopped at the deadline in its {} pass after {} of {} pairs; survivor selection needs the complete screen, so no pairs were selected and no detailed statistics were computed; 0 strong pairwise, 0 redundancy links{}",
                    total_pairs, pass, scored, total_pairs, excluded_suffix
                ),
                deadline_stopped: true,
            };
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

    let dep_for_pair = |a: usize, b: usize| -> PairDep {
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
    };
    // Capped: detailed statistics cover the survivor list only
    // (bounded by the pair cap). Uncapped: the whole universe,
    // enumerated in ordinal chunks so the pair list is never
    // materialized; chunks concatenate in pair order, so the
    // result is identical to screening a materialized list.
    // Both routes compute in bounded blocks and poll the
    // deadline before each block (issue #117, stage 2): the
    // work quantum is one block — DETAILED_CHUNK survivor
    // pairs on the capped path, one PAIR_CHUNK of the universe
    // on the uncapped path. A stop keeps the completed prefix:
    // those pairs keep their detailed statistics and findings,
    // and the coverage entry states how many of the selected
    // pairs were completed.
    const DETAILED_CHUNK: usize = 64;
    let mut deps: Vec<PairDep> = if capped {
        Vec::new()
    } else {
        Vec::with_capacity(total_pairs)
    };
    let mut detailed_stopped = false;
    if capped {
        let mut start = 0usize;
        while start < pair_list.len() {
            if stop() {
                detailed_stopped = true;
                break;
            }
            let end = (start + DETAILED_CHUNK).min(pair_list.len());
            let chunk: Vec<PairDep> = pair_list[start..end]
                .par_iter()
                .map(|&(a, b)| dep_for_pair(a, b))
                .collect();
            deps.extend(chunk);
            start = end;
        }
    } else {
        let mut start = 0usize;
        while start < total_pairs {
            if stop() {
                detailed_stopped = true;
                break;
            }
            let end = (start + PAIR_CHUNK).min(total_pairs);
            let chunk: Vec<PairDep> = (start..end)
                .into_par_iter()
                .map(|k| {
                    let (a, b) = pair_at(&numeric, k);
                    dep_for_pair(a, b)
                })
                .collect();
            deps.extend(chunk);
            start = end;
        }
    }

    // Findings: only strong pairs, and never a pair involving a
    // frame-role scaffolding column.
    let mut dep_findings: Vec<(usize, Finding)> = deps
        .iter()
        .filter(|d| !excluded.contains(&d.a) && !excluded.contains(&d.b))
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
    // On the capped path "all pairs screened" is the stage-1
    // statement and stays true when detailed statistics stop
    // early; on the uncapped path screening IS the detailed
    // pass, so a stop must not claim it completed.
    let screened_clause = if detailed_stopped && !capped {
        format!(
            "{} of {} pairs screened before the deadline stopped the pass",
            deps.len(),
            total_pairs
        )
    } else {
        format!("all {} pairs screened", total_pairs)
    };
    let mut coverage = format!(
        "{} of {} numeric pairs ({}); {}; detailed statistics over {} of {} rows; {} strong pairwise, {} redundancy links",
        deps.len(),
        total_pairs,
        if capped { "capped" } else { "full" },
        screened_clause, rows.len(), table.n,
        findings.len(),
        n_links,
    );
    if detailed_stopped {
        coverage.push_str(&format!(
            "; deadline reached during detailed statistics: {} of {} {} completed",
            deps.len(),
            if capped { pair_list.len() } else { total_pairs },
            if capped { "selected pairs" } else { "pairs" },
        ));
    }
    coverage.push_str(&excluded_suffix);

    DepOutcome {
        deps,
        findings,
        coverage,
        deadline_stopped: detailed_stopped,
    }
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

    #[test]
    fn pair_at_matches_materialized_order() {
        // Exhaustive over small widths: the ordinal mapping
        // must reproduce the nested-loop pair order exactly —
        // the streaming stages depend on it (issue #117).
        for m in 2..40usize {
            let numeric: Vec<usize> = (0..m).map(|i| i * 3 + 1).collect();
            let mut expect: Vec<(usize, usize)> = Vec::new();
            for (i, &a) in numeric.iter().enumerate() {
                for &b in numeric.iter().skip(i + 1) {
                    expect.push((a, b));
                }
            }
            for (k, &pair) in expect.iter().enumerate() {
                assert_eq!(pair_at(&numeric, k), pair, "m={m} k={k}");
            }
        }
        // Spot checks at screening scale: first pair, first
        // pair of the second row, last pair.
        let m = 5001usize;
        let numeric: Vec<usize> = (0..m).collect();
        let total = m * (m - 1) / 2;
        assert_eq!(pair_at(&numeric, 0), (0, 1));
        assert_eq!(pair_at(&numeric, m - 2), (0, m - 1));
        assert_eq!(pair_at(&numeric, m - 1), (1, 2));
        assert_eq!(pair_at(&numeric, total - 1), (m - 2, m - 1));
    }

    /// A table of `m` fully-observed F64 columns over `n` rows,
    /// deterministic pseudo-varied values.
    fn numeric_table(m: usize, n: usize) -> onceover_core::TypedTable<'static> {
        onceover_core::TypedTable {
            n,
            names: (0..m).map(|j| format!("c{j}")).collect(),
            dtypes: vec!["float64".to_string(); m],
            cols: (0..m)
                .map(|j| onceover_core::TypedCol::F64 {
                    vals: (0..n)
                        .map(|i| Some((((i * 31 + j * 17) % 89) as f64) * 0.5 + (j % 5) as f64))
                        .collect::<Vec<_>>()
                        .into(),
                })
                .collect(),
        }
    }

    #[test]
    fn deadline_stops_stage1_streaming_after_one_chunk() {
        // 730 columns -> 265,935 pairs: over the streaming
        // threshold at pair_cap 496 and over one PAIR_CHUNK, so
        // stage 1 takes the two-pass streaming route. The stop
        // predicate is consulted once per chunk; firing on the
        // second consultation must leave exactly one chunk
        // scored and no selection at all (issue #117, stage 2).
        let table = numeric_table(730, 120);
        let calls = std::cell::Cell::new(0usize);
        let out = screen_sampled_impl(&table, 496, 20_000, &[], &|| {
            calls.set(calls.get() + 1);
            calls.get() > 1
        });
        assert!(out.deadline_stopped);
        assert!(out.deps.is_empty());
        assert!(out.findings.is_empty());
        assert!(
            out.coverage.contains(
                "stage-1 screening stopped at the deadline in its first pass after 262144 of 266085 pairs"
            ),
            "coverage: {}",
            out.coverage
        );
    }

    #[test]
    fn deadline_stops_detailed_statistics_at_block_boundary() {
        // 30 columns -> 435 pairs; pair_cap 200 takes the
        // capped route with an in-place stage 1 (no checkpoints
        // there), so the stop predicate's consultations are the
        // detailed-statistics blocks of 64: firing on the third
        // leaves exactly 128 completed pairs.
        let table = numeric_table(30, 200);
        let calls = std::cell::Cell::new(0usize);
        let out = screen_sampled_impl(&table, 200, 20_000, &[], &|| {
            calls.set(calls.get() + 1);
            calls.get() > 2
        });
        assert!(out.deadline_stopped);
        assert_eq!(out.deps.len(), 128);
        assert!(
            out.coverage.contains(
                "deadline reached during detailed statistics: 128 of 200 selected pairs completed"
            ),
            "coverage: {}",
            out.coverage
        );
    }

    #[test]
    fn deadline_stops_uncapped_screen_at_first_chunk() {
        // pair_cap above the universe: uncapped route, one
        // chunk covers all 435 pairs; a stop at the first
        // consultation leaves zero pairs and must not claim
        // the screen completed.
        let table = numeric_table(30, 200);
        let out = screen_sampled_impl(&table, 10_000, 20_000, &[], &|| true);
        assert!(out.deadline_stopped);
        assert!(out.deps.is_empty());
        assert!(
            out.coverage
                .contains("0 of 435 pairs screened before the deadline stopped the pass"),
            "coverage: {}",
            out.coverage
        );
    }

    #[test]
    fn no_stop_and_future_deadline_match_unbounded_run() {
        let table = numeric_table(30, 200);
        let base = screen_sampled_impl(&table, 200, 20_000, &[], &|| false);
        assert!(!base.deadline_stopped);
        let via_deadline = screen_sampled(
            &table,
            200,
            20_000,
            &[],
            crate::deadline::Deadline::at(
                std::time::Instant::now() + std::time::Duration::from_secs(600),
            ),
        );
        assert!(!via_deadline.deadline_stopped);
        assert_eq!(base.coverage, via_deadline.coverage);
        assert_eq!(base.deps.len(), via_deadline.deps.len());
        assert_eq!(base.findings.len(), via_deadline.findings.len());
    }
}
