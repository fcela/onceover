//! Hotspots: cells of a 2- or 3-column combination holding far
//! more rows than the columns' marginals predict under
//! independence. Single-column concentration findings cover
//! marginal shares; the combination case — a city x category
//! spike, a price band x channel cluster — is invisible from
//! the marginals alone. The scan runs over the shared encoded
//! matrix (nominal features level-coded, numeric features in
//! the encoder's quantile bins), so cell counts are counts over
//! the encoded row sample, and the evidence says so.
//!
//! The false-positive bar is stated, not implied: every cell of
//! every eligible combination is counted as examined, and a
//! reported cell's standardized residual must clear a
//! Bonferroni threshold over all examined cells at family
//! alpha 0.01, on top of absolute bars (expected count, ratio,
//! observed count, share of scanned rows).

use crate::encode::{EncodedMatrix, MISSING};
use crate::fmt_float;
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BinaryHeap, HashMap};

pub struct HotspotConfig {
    /// Per-feature level/bin cap for eligibility.
    pub max_levels: usize,
    /// Cell-count caps for pairs and triples.
    pub max_cells_pair: usize,
    pub max_cells_triple: usize,
    /// Triples are examined only when the budget enables them.
    pub triples: bool,
    /// Cap on triples examined. When the candidate universe
    /// exceeds the cap, the examined triples are chosen by a
    /// pairwise pre-score, not by column order (issue #126).
    pub max_triples: usize,
    pub min_expected: f64,
    pub min_ratio: f64,
    pub min_observed: usize,
    pub min_share: f64,
    pub max_per_combination: usize,
    pub max_findings: usize,
}

impl Default for HotspotConfig {
    fn default() -> Self {
        Self {
            max_levels: 20,
            max_cells_pair: 1024,
            max_cells_triple: 4096,
            triples: false,
            max_triples: 400,
            min_expected: 10.0,
            min_ratio: 2.0,
            min_observed: 20,
            min_share: 0.005,
            max_per_combination: 3,
            max_findings: 15,
        }
    }
}

struct CellHit {
    feats: Vec<usize>,
    codes: Vec<u8>,
    observed: usize,
    expected: f64,
    z: f64,
}

/// A combination reports only when it has at least this many
/// complete rows; `evaluate_combo` returns without a hit below
/// it, so combinations whose complete-row count is bounded
/// below it by any single feature are skipped before counting.
const MIN_COMPLETE_ROWS: usize = 200;

pub fn hotspots(enc: &EncodedMatrix, cfg: &HotspotConfig) -> (Vec<Finding>, String) {
    let eligible: Vec<usize> = (0..enc.feat_names.len())
        .filter(|&f| enc.n_bins[f] >= 2 && enc.n_bins[f] <= cfg.max_levels)
        .collect();
    // Features a triple may use: at most 8 bins, so a triple's
    // cell count stays inside the triple cell cap's reach.
    let small: Vec<usize> = eligible
        .iter()
        .copied()
        .filter(|&f| enc.n_bins[f] <= 8)
        .collect();
    let mut in_small = vec![false; enc.feat_names.len()];
    for &f in &small {
        in_small[f] = true;
    }

    // The combination universe is enumerated, never
    // materialized: at 5,001 eligible columns it holds 12.5M
    // pairs, and building the list cost ~1.1 s and one heap
    // allocation per combination (issue #117). Enumeration is a
    // pure function of the bin counts and the config, so pairs
    // are enumerated once to total their cells and once to
    // evaluate, holding only the current combination: O(1)
    // combination state in place of the O(combinations) list.
    let (n_pairs, pair_cells) = enumerate_pairs(&enc.n_bins, cfg, &eligible, |_| {});

    // Complete rows of a combination are a subset of any one
    // feature's non-missing rows, so a feature with fewer than
    // MIN_COMPLETE_ROWS present bounds the whole combination
    // below the reporting floor. Wide frames with few rows put
    // every pair in that case; counting them anyway is the
    // pass's quadratic blow-up on such frames. The counts are
    // computed once here.
    let present: Vec<usize> = enc
        .bins
        .iter()
        .map(|b| b.iter().filter(|&&c| c != MISSING).count())
        .collect();

    // Evaluate the pairs first. The candidate hits are kept
    // unfiltered: the Bonferroni z bar depends on the total
    // examined cells, which is final only once the triples are
    // chosen. Each pair of triple-eligible features also yields
    // a pre-score — its strongest cell z over the count floors —
    // the currency the triple cap spends (issue #126).
    let mut cand_hits: Vec<CellHit> = Vec::new();
    let mut pair_scores: HashMap<(usize, usize), f64> = HashMap::new();
    if n_pairs > 0 {
        enumerate_pairs(&enc.n_bins, cfg, &eligible, |combo| {
            if combo.iter().any(|&f| present[f] < MIN_COMPLETE_ROWS) {
                return;
            }
            let (hits, best_z) = evaluate_combo(enc, combo, cfg);
            cand_hits.extend(hits);
            if in_small[combo[0]] && in_small[combo[1]] {
                if let Some(z) = best_z {
                    pair_scores.insert((combo[0], combo[1]), z);
                }
            }
        });
    }

    // Choose the examined triples by pre-score before the cap
    // binds, so position in the table no longer decides which
    // triples are examined (issue #126).
    let (selected_triples, triple_universe, triple_cells) = if cfg.triples {
        select_triples(&enc.n_bins, cfg, &small, |a, b| {
            let key = if a < b { (a, b) } else { (b, a) };
            pair_scores.get(&key).copied().unwrap_or(0.0)
        })
    } else {
        (Vec::new(), 0, 0)
    };
    let triples_examined = selected_triples.len();
    let n_combos = n_pairs + triples_examined;
    let cells_examined = pair_cells + triple_cells;
    if n_combos == 0 || cells_examined == 0 {
        return (
            Vec::new(),
            format!(
                "{} eligible columns, no combinations within the cell caps",
                eligible.len()
            ),
        );
    }
    let z_bar = inv_normal_cdf(1.0 - 0.01 / (2.0 * cells_examined as f64));

    for combo in &selected_triples {
        if combo.iter().any(|&f| present[f] < MIN_COMPLETE_ROWS) {
            continue;
        }
        let (hits, _) = evaluate_combo(enc, combo, cfg);
        cand_hits.extend(hits);
    }
    let mut hits: Vec<CellHit> = cand_hits.into_iter().filter(|h| h.z >= z_bar).collect();
    hits.sort_by(|a, b| {
        b.z.total_cmp(&a.z)
            .then(a.feats.cmp(&b.feats))
            .then(a.codes.cmp(&b.codes))
    });
    let mut per_combo: BTreeMap<Vec<usize>, usize> = BTreeMap::new();
    let mut findings = Vec::new();
    for hit in hits {
        let n_in_combo = per_combo.entry(hit.feats.clone()).or_insert(0);
        if *n_in_combo >= cfg.max_per_combination || findings.len() >= cfg.max_findings {
            continue;
        }
        *n_in_combo += 1;
        findings.push(render(enc, &hit, cells_examined, z_bar));
    }
    let mut cov = format!(
        "{} eligible columns, {} combinations examined ({} triples), {} cells examined, z bar {:.2} (Bonferroni over cells, family alpha 0.01); counts over the encoded sample of {} rows",
        eligible.len(),
        n_combos,
        triples_examined,
        cells_examined,
        z_bar,
        enc.n,
    );
    if triple_universe > triples_examined {
        cov.push_str(&format!(
            "; triple cap {} bound: {} of {} candidate triples examined, \
             chosen by pairwise pre-score (strongest constituent pair cell), \
             not column order",
            cfg.max_triples, triples_examined, triple_universe
        ));
    }
    (findings, cov)
}

/// Enumerate the eligible pairs, in the exact order the
/// materialized list used to hold them (eligible order, gated
/// by the pair cell cap). Returns (pairs, cells) totals. Only
/// the current combination is materialized, so enumeration
/// state is O(1) in the number of combinations, beyond the
/// O(width) eligible list.
fn enumerate_pairs(
    n_bins: &[usize],
    cfg: &HotspotConfig,
    eligible: &[usize],
    mut visit: impl FnMut(&[usize]),
) -> (usize, usize) {
    let mut n_pairs = 0usize;
    let mut cells = 0usize;
    for a in 0..eligible.len() {
        for b in (a + 1)..eligible.len() {
            let (fa, fb) = (eligible[a], eligible[b]);
            let pair_cells = n_bins[fa] * n_bins[fb];
            if pair_cells <= cfg.max_cells_pair {
                visit(&[fa, fb]);
                n_pairs += 1;
                cells += pair_cells;
            }
        }
    }
    (n_pairs, cells)
}

/// Enumerate the whole cell-capped triple universe over the
/// <= 8-bin features, in eligible order, with no count cap:
/// which triples the pass examines is `select_triples`'
/// decision, made with the pair pre-scores in hand (issue
/// #126). Before that change this enumeration stopped at
/// `cfg.max_triples`, so the cap was spent in column order.
fn enumerate_triples(
    n_bins: &[usize],
    cfg: &HotspotConfig,
    small: &[usize],
    mut visit: impl FnMut(&[usize]),
) {
    for a in 0..small.len() {
        for b in (a + 1)..small.len() {
            for c in (b + 1)..small.len() {
                let (fa, fb, fc) = (small[a], small[b], small[c]);
                let triple_cells = n_bins[fa] * n_bins[fb] * n_bins[fc];
                if triple_cells <= cfg.max_cells_triple {
                    visit(&[fa, fb, fc]);
                }
            }
        }
    }
}

/// One candidate triple in the selection heap: its pre-score
/// and its position in the enumeration order.
#[derive(Clone, Copy, PartialEq)]
struct ScoredTriple {
    score: f64,
    ordinal: usize,
    combo: [usize; 3],
}

impl Eq for ScoredTriple {}

impl PartialOrd for ScoredTriple {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScoredTriple {
    /// Kept-worthiness: a higher score wins; ties go to the
    /// lower enumeration ordinal.
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .total_cmp(&other.score)
            .then(other.ordinal.cmp(&self.ordinal))
    }
}

/// The triples the pass examines: the whole cell-capped
/// universe when it fits the cap, else the top
/// `cfg.max_triples` by pre-score — the strongest constituent
/// pair's cell z — held in a bounded heap over the streamed
/// universe (issue #117's pattern: the universe itself is
/// never materialized). Ties, and the all-zero-score case,
/// resolve by enumeration ordinal, so selection is a
/// deterministic function of the encoded matrix and the old
/// column-order behavior survives as the tie-break. Returns
/// (selected triples in enumeration order, universe size,
/// selected cells).
fn select_triples(
    n_bins: &[usize],
    cfg: &HotspotConfig,
    small: &[usize],
    pair_score: impl Fn(usize, usize) -> f64,
) -> (Vec<[usize; 3]>, usize, usize) {
    let mut heap: BinaryHeap<Reverse<ScoredTriple>> = BinaryHeap::new();
    let mut universe = 0usize;
    enumerate_triples(n_bins, cfg, small, |combo| {
        let score = pair_score(combo[0], combo[1])
            .max(pair_score(combo[1], combo[2]))
            .max(pair_score(combo[0], combo[2]));
        let cand = ScoredTriple {
            score,
            ordinal: universe,
            combo: [combo[0], combo[1], combo[2]],
        };
        universe += 1;
        if cfg.max_triples == 0 {
            return;
        }
        if heap.len() < cfg.max_triples {
            heap.push(Reverse(cand));
        } else if let Some(&Reverse(worst)) = heap.peek() {
            if cand > worst {
                heap.pop();
                heap.push(Reverse(cand));
            }
        }
    });
    let mut selected: Vec<ScoredTriple> = heap.into_iter().map(|r| r.0).collect();
    selected.sort_by_key(|c| c.ordinal);
    let cells: usize = selected
        .iter()
        .map(|c| c.combo.iter().map(|&f| n_bins[f]).product::<usize>())
        .sum();
    (selected.iter().map(|c| c.combo).collect(), universe, cells)
}

/// Score every cell of one combination against independence of
/// its margins over the combination's complete rows. Returns
/// the cells clearing the absolute bars (expected, ratio,
/// observed, share) with their z — the caller applies the
/// Bonferroni z bar once the examined-cell total is final —
/// and the strongest cell z over cells clearing the count
/// floors (observed and expected), the pre-score currency for
/// combinations containing this one. The pre-score ignores
/// the ratio and share bars on purpose: a strong triple's
/// pairs dilute toward their margins, so a pair can sit below
/// the ratio bar while its z still marks the triple as worth
/// examining.
fn evaluate_combo(
    enc: &EncodedMatrix,
    combo: &[usize],
    cfg: &HotspotConfig,
) -> (Vec<CellHit>, Option<f64>) {
    let dims: Vec<usize> = combo.iter().map(|&f| enc.n_bins[f]).collect();
    let n_cells: usize = dims.iter().product();
    let mut counts = vec![0usize; n_cells];
    let mut margins: Vec<Vec<usize>> = dims.iter().map(|&d| vec![0usize; d]).collect();
    let mut n_complete = 0usize;
    for pos in 0..enc.n {
        let mut idx = 0usize;
        let mut stride = 1usize;
        let mut complete = true;
        let mut codes = [0u8; 3];
        for (k, &f) in combo.iter().enumerate() {
            let code = enc.bins[f][pos];
            if code == MISSING {
                complete = false;
                break;
            }
            codes[k] = code;
            idx += code as usize * stride;
            stride *= dims[k];
        }
        if !complete {
            continue;
        }
        n_complete += 1;
        counts[idx] += 1;
        for (k, &code) in codes.iter().enumerate().take(combo.len()) {
            margins[k][code as usize] += 1;
        }
    }
    if n_complete < MIN_COMPLETE_ROWS {
        return (Vec::new(), None);
    }
    let nf = n_complete as f64;
    let mut hits = Vec::new();
    let mut best_z: Option<f64> = None;
    for (idx, &obs) in counts.iter().enumerate() {
        if obs < cfg.min_observed {
            continue;
        }
        // Decode the cell's codes from its flat index.
        let mut rem = idx;
        let mut codes = Vec::with_capacity(combo.len());
        let mut expected = nf;
        for (k, &d) in dims.iter().enumerate() {
            let code = rem % d;
            rem /= d;
            codes.push(code as u8);
            expected *= margins[k][code] as f64 / nf;
        }
        if expected < cfg.min_expected {
            continue;
        }
        let z = (obs as f64 - expected) / expected.sqrt();
        best_z = Some(best_z.map_or(z, |b: f64| b.max(z)));
        let ratio = obs as f64 / expected;
        if ratio < cfg.min_ratio || obs as f64 / enc.n as f64 <= cfg.min_share {
            continue;
        }
        hits.push(CellHit {
            feats: combo.to_vec(),
            codes,
            observed: obs,
            expected,
            z,
        });
    }
    (hits, best_z)
}

fn cell_label(enc: &EncodedMatrix, f: usize, code: u8) -> String {
    if !enc.levels[f].is_empty() {
        return enc.levels[f]
            .get(code as usize)
            .cloned()
            .unwrap_or_else(|| format!("level {}", code));
    }
    let edges = &enc.edges[f];
    let c = code as usize;
    if edges.is_empty() {
        return format!("bin {}", c);
    }
    if c == 0 {
        format!("<= {}", fmt_float(edges[0]))
    } else if c >= edges.len() {
        format!("> {}", fmt_float(edges[edges.len() - 1]))
    } else {
        format!("({}, {}]", fmt_float(edges[c - 1]), fmt_float(edges[c]))
    }
}

fn render(enc: &EncodedMatrix, hit: &CellHit, cells_examined: usize, z_bar: f64) -> Finding {
    let names: Vec<String> = hit
        .feats
        .iter()
        .map(|&f| enc.feat_names[f].clone())
        .collect();
    let cell_desc: Vec<String> = hit
        .feats
        .iter()
        .zip(&hit.codes)
        .map(|(&f, &c)| format!("{} = '{}'", enc.feat_names[f], cell_label(enc, f, c)))
        .collect();
    let ratio = hit.observed as f64 / hit.expected;
    let share = hit.observed as f64 / enc.n as f64;
    Finding::new(
        Pattern::Hotspot,
        BTreeMap::from([("columns".into(), json!(names))]),
        format!(
            "cell ({}) holds {} of {} scanned rows ({:.1}%) against {:.1} expected under independence of {} — {:.1}x the baseline (z = {:.1}, bar {:.1} over {} cells examined)",
            cell_desc.join(", "),
            hit.observed,
            enc.n,
            100.0 * share,
            hit.expected,
            names.join(", "),
            ratio,
            hit.z,
            z_bar,
            cells_examined,
        ),
        EvidenceBasis::Measured,
        0.5,
        BTreeMap::from([
            ("columns".into(), json!(names)),
            (
                "cell".into(),
                json!(hit
                    .feats
                    .iter()
                    .zip(&hit.codes)
                    .map(|(&f, &c)| (enc.feat_names[f].clone(), json!(cell_label(enc, f, c))))
                    .collect::<BTreeMap<String, serde_json::Value>>()),
            ),
            ("observed_rows".into(), json!(hit.observed)),
            ("expected_rows".into(), json!(hit.expected)),
            ("ratio".into(), json!(ratio)),
            ("share".into(), json!(share)),
            ("z".into(), json!(hit.z)),
            ("z_bar".into(), json!(z_bar)),
            ("cells_examined".into(), json!(cells_examined)),
            ("rows_scanned".into(), json!(enc.n)),
            (
                "method".into(),
                json!("observed cell count vs the independence baseline from the combination's margins over complete rows of the encoded sample; a cell reports only past absolute bars (expected >= 10, ratio >= 2, observed >= 20, share > 0.5%) and a Bonferroni z bar over all examined cells at family alpha 0.01"),
            ),
        ]),
        Some(
            "the combination is over-represented relative to its marginals; check for a process, segment, or recording rule concentrating rows here before treating any one column's marginal as representative"
                .into(),
        ),
        Method::new(
            "hotspots",
            "independence_excess_scan",
            "exact cell counts over the encoded sample vs independence margins (onceover v0.1)",
        ),
    )
}

/// Inverse standard normal CDF (Acklam's rational
/// approximation, |error| < 1.15e-9 in the central range).
fn inv_normal_cdf(p: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969683028665376e+01,
        2.209460984245205e+02,
        -2.759285104469687e+02,
        1.38357751867269e+02,
        -3.066479806614716e+01,
        2.506628277459239e+00,
    ];
    const B: [f64; 5] = [
        -5.447609879822406e+01,
        1.615858368580409e+02,
        -1.556989798598866e+02,
        6.680131188771972e+01,
        -1.328068155288572e+01,
    ];
    const C: [f64; 6] = [
        -7.784894002430293e-03,
        -3.223964580411365e-01,
        -2.400758277161838e+00,
        -2.549732539343734e+00,
        4.374664141464968e+00,
        2.938163982698783e+00,
    ];
    const D: [f64; 4] = [
        7.784695709041462e-03,
        3.224671290700398e-01,
        2.445134137142996e+00,
        3.754408661907416e+00,
    ];
    let p_low = 0.02425;
    if p < p_low {
        let q = (-2.0 * p.ln()).sqrt();
        return (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0);
    }
    if p > 1.0 - p_low {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        return -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0);
    }
    let q = p - 0.5;
    let r = q * q;
    (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
        / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inv_normal_known_points() {
        assert!((inv_normal_cdf(0.975) - 1.959964).abs() < 1e-6);
        assert!((inv_normal_cdf(0.5)).abs() < 1e-9);
        assert!((inv_normal_cdf(0.999999) - 4.753424).abs() < 1e-5);
    }

    #[test]
    fn pair_enumeration_matches_materialized_universe() {
        // Bin counts chosen to exercise the eligibility filter
        // and the pair cell cap.
        let n_bins = vec![2usize, 3, 8, 9, 20, 21, 4, 8];
        let cfg = HotspotConfig {
            max_cells_pair: 64,
            ..HotspotConfig::default()
        };
        let eligible: Vec<usize> = (0..n_bins.len())
            .filter(|&f| n_bins[f] >= 2 && n_bins[f] <= cfg.max_levels)
            .collect();
        // The naive materialization, exactly as the pass used
        // to build it before streaming (issue #117).
        let mut expect: Vec<Vec<usize>> = Vec::new();
        for a in 0..eligible.len() {
            for b in (a + 1)..eligible.len() {
                let (fa, fb) = (eligible[a], eligible[b]);
                if n_bins[fa] * n_bins[fb] <= cfg.max_cells_pair {
                    expect.push(vec![fa, fb]);
                }
            }
        }
        let expect_cells: usize = expect
            .iter()
            .map(|c| c.iter().map(|&f| n_bins[f]).product::<usize>())
            .sum();
        let mut seen: Vec<Vec<usize>> = Vec::new();
        let (n, cells) = enumerate_pairs(&n_bins, &cfg, &eligible, |c| seen.push(c.to_vec()));
        assert_eq!(seen, expect);
        assert_eq!(n, expect.len());
        assert_eq!(cells, expect_cells);
    }

    #[test]
    fn triple_selection_spends_the_cap_by_score() {
        // Same bin-count fixture: the <= 8-bin subset and the
        // triple cell cap bound the candidate universe.
        let n_bins = vec![2usize, 3, 8, 9, 20, 21, 4, 8];
        let cfg = HotspotConfig {
            max_cells_triple: 200,
            triples: true,
            max_triples: 3,
            ..HotspotConfig::default()
        };
        let eligible: Vec<usize> = (0..n_bins.len())
            .filter(|&f| n_bins[f] >= 2 && n_bins[f] <= cfg.max_levels)
            .collect();
        let small: Vec<usize> = eligible
            .iter()
            .copied()
            .filter(|&f| n_bins[f] <= 8)
            .collect();
        // The materialized universe, in enumeration order.
        let mut universe_list: Vec<[usize; 3]> = Vec::new();
        enumerate_triples(&n_bins, &cfg, &small, |c| {
            universe_list.push([c[0], c[1], c[2]]);
        });
        assert!(universe_list.len() > cfg.max_triples);
        // Fabricated pair scores grow with the pair's feature
        // indices, so the strongest triples sit at the END of
        // the enumeration order: selection must follow the
        // score, not the position (issue #126). A triple's
        // score is its strongest pair's, so the winners are
        // the triples holding pairs (6, 7) — both of them —
        // and, among the two holding pair (2, 7), the earlier.
        let (selected, universe, cells) =
            select_triples(&n_bins, &cfg, &small, |a, b| (a * 100 + b) as f64);
        assert_eq!(universe, universe_list.len());
        assert_eq!(selected, vec![[0, 2, 7], [0, 6, 7], [1, 6, 7]]);
        let expect_cells: usize = selected
            .iter()
            .map(|c| c.iter().map(|&f| n_bins[f]).product::<usize>())
            .sum();
        assert_eq!(cells, expect_cells);
        // With no signal anywhere, selection falls back to
        // enumeration order — the old behavior as tie-break.
        let (selected, _, _) = select_triples(&n_bins, &cfg, &small, |_, _| 0.0);
        assert_eq!(selected, universe_list[..cfg.max_triples]);
    }
}
