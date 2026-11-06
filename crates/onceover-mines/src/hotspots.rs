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

use crate::deadline::Deadline;
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
    /// Cooperative deadline (issue #117, stage 2): polled
    /// once per combination during pair evaluation and once
    /// per triple during triple evaluation — the pass's work
    /// quantum — and the scan stops at the next combination
    /// boundary when it has been reached, disclosing how much
    /// was examined. Unset (the default) the pass always runs
    /// to completion.
    pub deadline: Deadline,
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
            deadline: Deadline::none(),
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

/// What the hotspots pass reports, including whether the
/// deadline stopped it mid-scan (issue #117, stage 2). The
/// pipeline reads `deadline_stopped` to mark the coverage
/// entry `capped` and latch the profile's deadline state;
/// the coverage string itself carries the examined counts.
pub struct HotspotsOutcome {
    pub findings: Vec<Finding>,
    pub coverage: String,
    pub deadline_stopped: bool,
}

pub fn hotspots(enc: &EncodedMatrix, cfg: &HotspotConfig) -> HotspotsOutcome {
    hotspots_impl(enc, cfg, &|| cfg.deadline.exceeded())
}

fn hotspots_impl(
    enc: &EncodedMatrix,
    cfg: &HotspotConfig,
    stop: &dyn Fn() -> bool,
) -> HotspotsOutcome {
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
    let (n_pairs, pair_cells) = enumerate_pairs(&enc.n_bins, cfg, &eligible, |_| true);

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
    //
    // The deadline is polled before each pair (issue #117,
    // stage 2): one combination is the work quantum, so the
    // overshoot past the deadline is bounded by one
    // combination. A stop during pair evaluation skips the
    // triples phase entirely: triple selection spends the
    // pair pre-scores, and a selection cannot rest on a
    // partial screen. The examined counters cover the visited
    // prefix — a pair counts as examined once the poll before
    // it has passed, including pairs then skipped by the
    // complete-rows floor, whose cells were part of the
    // counting pass's total.
    let mut cand_hits: Vec<CellHit> = Vec::new();
    let mut pair_scores: HashMap<(usize, usize), f64> = HashMap::new();
    let mut pairs_evaluated = 0usize;
    let mut pair_cells_evaluated = 0usize;
    let mut stopped_in_pairs = false;
    if n_pairs > 0 {
        enumerate_pairs(&enc.n_bins, cfg, &eligible, |combo| {
            if stopped_in_pairs {
                return false;
            }
            if stop() {
                stopped_in_pairs = true;
                return false;
            }
            pairs_evaluated += 1;
            pair_cells_evaluated += enc.n_bins[combo[0]] * enc.n_bins[combo[1]];
            if combo.iter().any(|&f| present[f] < MIN_COMPLETE_ROWS) {
                return true;
            }
            let (hits, best_z) = evaluate_combo(enc, combo, cfg);
            cand_hits.extend(hits);
            if in_small[combo[0]] && in_small[combo[1]] {
                if let Some(z) = best_z {
                    pair_scores.insert((combo[0], combo[1]), z);
                }
            }
            true
        });
    }

    // Choose the examined triples by pre-score before the cap
    // binds, so position in the table no longer decides which
    // triples are examined (issue #126). A pair-phase stop
    // skips selection, for the reason stated above.
    let (selected_triples, triple_universe, triple_cells) = if cfg.triples && !stopped_in_pairs {
        select_triples(&enc.n_bins, cfg, &small, |a, b| {
            let key = if a < b { (a, b) } else { (b, a) };
            pair_scores.get(&key).copied().unwrap_or(0.0)
        })
    } else {
        (Vec::new(), 0, 0)
    };
    let triples_selected = selected_triples.len();
    let n_combos = n_pairs + triples_selected;
    if !stopped_in_pairs && (n_combos == 0 || pair_cells + triple_cells == 0) {
        return HotspotsOutcome {
            findings: Vec::new(),
            coverage: format!(
                "{} eligible columns, no combinations within the cell caps",
                eligible.len()
            ),
            deadline_stopped: false,
        };
    }

    // Evaluate the selected triples, polling the deadline
    // before each one. A triple counts as examined once the
    // poll before it has passed, including triples then
    // skipped by the complete-rows floor. A stop keeps the
    // completed evaluations: their hits stand, filtered by
    // the bar over the cells examined at the stop point.
    let mut triples_evaluated = 0usize;
    let mut triple_cells_evaluated = 0usize;
    let mut stopped_in_triples = false;
    for combo in &selected_triples {
        if stop() {
            stopped_in_triples = true;
            break;
        }
        triples_evaluated += 1;
        triple_cells_evaluated += combo.iter().map(|&f| enc.n_bins[f]).product::<usize>();
        if combo.iter().any(|&f| present[f] < MIN_COMPLETE_ROWS) {
            continue;
        }
        let (hits, _) = evaluate_combo(enc, combo, cfg);
        cand_hits.extend(hits);
    }
    let stopped = stopped_in_pairs || stopped_in_triples;
    // The Bonferroni denominator is the examined cells: the
    // counting pass's pair cells plus the selected triples'
    // cells on a complete run, the examined prefix of those
    // on a stopped run.
    let cells_examined = pair_cells_evaluated + triple_cells_evaluated;
    let z_bar = if cells_examined > 0 {
        inv_normal_cdf(1.0 - 0.01 / (2.0 * cells_examined as f64))
    } else {
        // No cell was examined (the deadline fired before the
        // first pair): there are no candidate hits to filter,
        // and the bar is never rendered.
        f64::INFINITY
    };
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
        findings.push(render(
            enc,
            &hit,
            cells_examined,
            z_bar,
            if stopped_in_pairs {
                Some(pair_cells)
            } else if stopped_in_triples {
                Some(pair_cells + triple_cells)
            } else {
                None
            },
        ));
    }
    // Partial coverage is disclosed, never silent: how much
    // of the scan was examined before the deadline stopped it,
    // and (on a pair-phase stop) that triples were never
    // selected, because their selection spends the completed
    // pair screen's pre-scores.
    let coverage = if stopped_in_pairs {
        if pairs_evaluated == 0 {
            format!(
                "{} eligible columns, {} of {} pair combinations examined, {} of {} pair cells examined before the deadline stopped the scan; triples were not selected or examined — selection needs the complete pair screen; counts over the encoded sample of {} rows",
                eligible.len(),
                pairs_evaluated,
                n_pairs,
                pair_cells_evaluated,
                pair_cells,
                enc.n,
            )
        } else {
            format!(
                "{} eligible columns, {} of {} pair combinations examined, {} of {} pair cells examined before the deadline stopped the scan, z bar {:.2} (Bonferroni over the examined cells, family alpha 0.01); triples were not selected or examined — selection needs the complete pair screen; counts over the encoded sample of {} rows",
                eligible.len(),
                pairs_evaluated,
                n_pairs,
                pair_cells_evaluated,
                pair_cells,
                z_bar,
                enc.n,
            )
        }
    } else if stopped_in_triples {
        let mut cov = format!(
            "{} eligible columns, {} combinations examined ({} of {} triples), {} of {} cells examined before the deadline stopped the scan, z bar {:.2} (Bonferroni over the examined cells, family alpha 0.01); counts over the encoded sample of {} rows",
            eligible.len(),
            n_pairs + triples_evaluated,
            triples_evaluated,
            triples_selected,
            cells_examined,
            pair_cells + triple_cells,
            z_bar,
            enc.n,
        );
        if triple_universe > triples_selected {
            cov.push_str(&format!(
                "; triple cap {} bound: {} of {} candidate triples examined, \
                 chosen by pairwise pre-score (strongest constituent pair cell), \
                 not column order",
                cfg.max_triples, triples_selected, triple_universe
            ));
        }
        cov
    } else {
        let mut cov = format!(
            "{} eligible columns, {} combinations examined ({} triples), {} cells examined, z bar {:.2} (Bonferroni over cells, family alpha 0.01); counts over the encoded sample of {} rows",
            eligible.len(),
            n_combos,
            triples_selected,
            cells_examined,
            z_bar,
            enc.n,
        );
        if triple_universe > triples_selected {
            cov.push_str(&format!(
                "; triple cap {} bound: {} of {} candidate triples examined, \
                 chosen by pairwise pre-score (strongest constituent pair cell), \
                 not column order",
                cfg.max_triples, triples_selected, triple_universe
            ));
        }
        cov
    };
    HotspotsOutcome {
        findings,
        coverage,
        deadline_stopped: stopped,
    }
}

/// Enumerate the eligible pairs, in the exact order the
/// materialized list used to hold them (eligible order, gated
/// by the pair cell cap). The visitor returns `false` to halt
/// the enumeration (the deadline stop); the totals then cover
/// the visited prefix only. Returns (pairs, cells) totals.
/// Only the current combination is materialized, so
/// enumeration state is O(1) in the number of combinations,
/// beyond the O(width) eligible list.
fn enumerate_pairs(
    n_bins: &[usize],
    cfg: &HotspotConfig,
    eligible: &[usize],
    mut visit: impl FnMut(&[usize]) -> bool,
) -> (usize, usize) {
    let mut n_pairs = 0usize;
    let mut cells = 0usize;
    for a in 0..eligible.len() {
        for b in (a + 1)..eligible.len() {
            let (fa, fb) = (eligible[a], eligible[b]);
            let pair_cells = n_bins[fa] * n_bins[fb];
            if pair_cells <= cfg.max_cells_pair {
                if !visit(&[fa, fb]) {
                    return (n_pairs, cells);
                }
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

fn render(
    enc: &EncodedMatrix,
    hit: &CellHit,
    cells_examined: usize,
    z_bar: f64,
    stopped_cells: Option<usize>,
) -> Finding {
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
    // On a deadline-stopped scan the claim carries the stop:
    // the bar this cell cleared is the examined-cells bar,
    // and the unexamined remainder of the scan is stated.
    let bar_clause = match stopped_cells {
        Some(total) => format!(
            "bar {:.1} over {} cells examined; the scan stopped at the deadline with {} of {} cells unexamined",
            z_bar,
            cells_examined,
            total - cells_examined,
            total,
        ),
        None => format!("bar {:.1} over {} cells examined", z_bar, cells_examined),
    };
    let mut evidence = BTreeMap::from([
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
    ]);
    if stopped_cells.is_some() {
        evidence.insert("cells_evaluated".into(), json!(cells_examined));
    }
    Finding::new(
        Pattern::Hotspot,
        BTreeMap::from([("columns".into(), json!(names))]),
        format!(
            "cell ({}) holds {} of {} scanned rows ({:.1}%) against {:.1} expected under independence of {} — {:.1}x the baseline (z = {:.1}, {})",
            cell_desc.join(", "),
            hit.observed,
            enc.n,
            100.0 * share,
            hit.expected,
            names.join(", "),
            ratio,
            hit.z,
            bar_clause,
        ),
        EvidenceBasis::Measured,
        0.5,
        evidence,
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
        let (n, cells) = enumerate_pairs(&n_bins, &cfg, &eligible, |c| {
            seen.push(c.to_vec());
            true
        });
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

    /// Six features of four bins over 400 rows, with pair
    /// (0, 1) concentrated at cell (0, 0) for the first 100
    /// rows: 15 pairs, 240 pair cells.
    fn planted_matrix() -> EncodedMatrix {
        let n = 400usize;
        let mut bins: Vec<Vec<u8>> = (0..6).map(|_| vec![0u8; n]).collect();
        for r in 0..n {
            for (f, col) in bins.iter_mut().enumerate() {
                col[r] = ((r / (f + 2)) % 4) as u8;
            }
        }
        // Concentrate (0, 0): the first 100 rows sit at code 0
        // on both features 0 and 1.
        for col in bins.iter_mut().take(2) {
            col[..100].fill(0);
        }
        EncodedMatrix {
            rows: (0..n as u32).collect(),
            n,
            bins,
            n_bins: vec![4; 6],
            feat_names: (0..6).map(|i| format!("f{i}")).collect(),
            feat_cols: (0..6).collect(),
            feat_dtypes: vec!["int64".into(); 6],
            feat_nominal: vec![false; 6],
            edges: vec![Vec::new(); 6],
            levels: vec![Vec::new(); 6],
        }
    }

    #[test]
    fn deadline_already_reached_stops_before_first_pair() {
        let enc = planted_matrix();
        let cfg = HotspotConfig {
            deadline: Deadline::at(std::time::Instant::now() - std::time::Duration::from_secs(1)),
            ..HotspotConfig::default()
        };
        let out = hotspots(&enc, &cfg);
        assert!(out.deadline_stopped);
        assert!(out.findings.is_empty());
        assert!(
            out.coverage.contains("0 of 15 pair combinations examined"),
            "coverage: {}",
            out.coverage
        );
        assert!(
            out.coverage.contains("0 of 240 pair cells examined"),
            "coverage: {}",
            out.coverage
        );
        assert!(
            out.coverage.contains("deadline stopped the scan"),
            "coverage: {}",
            out.coverage
        );
    }

    #[test]
    fn counting_stop_examines_exact_pair_prefix_and_skips_triples() {
        // The stop predicate is consulted once before each
        // pair; firing on the sixth consultation leaves
        // exactly five pairs (80 cells) examined. The examined
        // prefix's hits over the bar computed from those 80
        // cells stand — here three cells of pair (0, 2), the
        // per-combination cap — and each carries the stop and
        // the examined-cell denominator. Triples are enabled
        // to pin the skip rule: selection spends the pair
        // pre-scores, so a partial pair screen selects no
        // triples at all (issue #117, stage 2).
        let enc = planted_matrix();
        let cfg = HotspotConfig {
            triples: true,
            ..HotspotConfig::default()
        };
        let calls = std::cell::Cell::new(0usize);
        let out = hotspots_impl(&enc, &cfg, &|| {
            calls.set(calls.get() + 1);
            calls.get() > 5
        });
        assert!(out.deadline_stopped);
        assert!(
            out.coverage.contains("5 of 15 pair combinations examined"),
            "coverage: {}",
            out.coverage
        );
        assert!(
            out.coverage.contains("80 of 240 pair cells examined"),
            "coverage: {}",
            out.coverage
        );
        assert!(
            out.coverage
                .contains("triples were not selected or examined"),
            "coverage: {}",
            out.coverage
        );
        assert_eq!(out.findings.len(), 3);
        for f in &out.findings {
            assert!(
                f.claim.contains("the scan stopped at the deadline"),
                "claim: {}",
                f.claim
            );
            assert_eq!(f.evidence[0].metrics["cells_evaluated"], json!(80));
            assert_eq!(
                f.evidence[0].metrics["columns"],
                json!(["f0", "f2"]),
                "no triple finding may appear"
            );
        }
    }

    #[test]
    fn counting_stop_during_triples_keeps_completed_evaluations() {
        // Triples enabled: the universe holds C(6, 3) = 20
        // triples of 64 cells, all selected under the cap.
        // The predicate passes 15 pair consultations and 2
        // triple consultations, then fires: exactly 2 triples
        // are evaluated, the denominator is 240 + 128 = 368
        // examined cells of the 240 + 1280 planned for the
        // examined set, and the completed evaluations' hits
        // over that bar stand — one triple cell and six pair
        // cells on this fixture — each carrying the stop and
        // the examined-cell denominator (issue #117, stage 2).
        let enc = planted_matrix();
        let cfg = HotspotConfig {
            triples: true,
            ..HotspotConfig::default()
        };
        let calls = std::cell::Cell::new(0usize);
        let out = hotspots_impl(&enc, &cfg, &|| {
            calls.set(calls.get() + 1);
            calls.get() > 17
        });
        assert!(out.deadline_stopped);
        assert!(
            out.coverage
                .contains("17 combinations examined (2 of 20 triples)"),
            "coverage: {}",
            out.coverage
        );
        assert!(
            out.coverage.contains("368 of 1520 cells examined"),
            "coverage: {}",
            out.coverage
        );
        assert_eq!(out.findings.len(), 7);
        let mut triple_findings = 0usize;
        for f in &out.findings {
            assert!(
                f.claim.contains("the scan stopped at the deadline"),
                "claim: {}",
                f.claim
            );
            assert_eq!(f.evidence[0].metrics["cells_evaluated"], json!(368));
            if f.evidence[0].metrics["columns"]
                .as_array()
                .is_some_and(|cols| cols.len() == 3)
            {
                triple_findings += 1;
                assert_eq!(f.evidence[0].metrics["columns"], json!(["f0", "f1", "f2"]));
            }
        }
        assert_eq!(triple_findings, 1);
    }

    #[test]
    fn future_deadline_matches_no_deadline() {
        let enc = planted_matrix();
        let plain = hotspots(&enc, &HotspotConfig::default());
        let cfg = HotspotConfig {
            deadline: Deadline::at(std::time::Instant::now() + std::time::Duration::from_secs(600)),
            ..HotspotConfig::default()
        };
        let timed = hotspots(&enc, &cfg);
        assert!(!plain.deadline_stopped);
        assert!(!timed.deadline_stopped);
        assert_eq!(plain.coverage, timed.coverage);
        assert_eq!(plain.findings.len(), timed.findings.len());
        assert!(
            !plain.findings.is_empty(),
            "the planted concentration should produce a finding"
        );
    }
}
