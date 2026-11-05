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
use std::collections::BTreeMap;

pub struct HotspotConfig {
    /// Per-feature level/bin cap for eligibility.
    pub max_levels: usize,
    /// Cell-count caps for pairs and triples.
    pub max_cells_pair: usize,
    pub max_cells_triple: usize,
    /// Triples are examined only when the budget enables them.
    pub triples: bool,
    /// Cap on triples examined (feature-index order).
    pub max_triples: usize,
    pub min_expected: f64,
    pub min_ratio: f64,
    pub min_observed: usize,
    pub min_share: f64,
    pub max_per_combination: usize,
    pub max_findings: usize,
    /// Cooperative deadline (issue #117, stage 2): the
    /// evaluation loop polls it once per combination — the
    /// pass's work quantum — and stops at the next combination
    /// boundary when it has been reached, disclosing how much
    /// of the combination universe was examined. Unset (the
    /// default) the pass always runs to completion.
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

    // The combination universe is enumerated, never
    // materialized: at 5,001 eligible columns it holds 12.5M
    // pairs, and building the list cost ~1.1 s and one heap
    // allocation per combination (issue #117). Enumeration is a
    // pure function of the bin counts and the config, so the
    // pass enumerates twice — once to total the examined cells
    // (the Bonferroni denominator), once to evaluate — holding
    // only the current combination: O(1) combination state in
    // place of the O(combinations) list. `enumerate_combos`
    // yields pairs first, then (deep) triples, in exactly the
    // order the list used to hold them. The totals enumeration
    // is arithmetic over bin counts and always completes: it
    // defines the planned universe the deadline disclosure is
    // measured against.
    let (n_combos, triples_examined, cells_examined) =
        enumerate_combos(&enc.n_bins, cfg, &eligible, |_| true);
    if n_combos == 0 || cells_examined == 0 {
        return HotspotsOutcome {
            findings: Vec::new(),
            coverage: format!(
                "{} eligible columns, no combinations within the cell caps",
                eligible.len()
            ),
            deadline_stopped: false,
        };
    }
    // The Bonferroni denominator is the planned universe, and
    // a deadline stop does not reduce it: the bar a reported
    // cell cleared is the full-universe bar whether or not the
    // scan reached the cells past the stop. What the stop
    // changes is which cells were examined, and that is what
    // the coverage entry (and, on stopped runs, each finding)
    // discloses — the correction itself is never silently
    // weakened (issue #117).
    let z_bar = inv_normal_cdf(1.0 - 0.01 / (2.0 * cells_examined as f64));

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
    let mut hits: Vec<CellHit> = Vec::new();
    // The deadline is polled once per combination — the work
    // quantum: one combination's evaluation is a single scan
    // of the encoded sample, so the overshoot past the
    // deadline is bounded by that one scan. The examined set
    // on a stopped run is a prefix of the enumeration order.
    let mut evaluated_combos = 0usize;
    let mut evaluated_triples = 0usize;
    let mut evaluated_cells = 0usize;
    let mut stopped = false;
    enumerate_combos(&enc.n_bins, cfg, &eligible, |combo| {
        if stop() {
            stopped = true;
            return false;
        }
        evaluated_combos += 1;
        if combo.len() == 3 {
            evaluated_triples += 1;
        }
        evaluated_cells += combo.iter().map(|&f| enc.n_bins[f]).product::<usize>();
        if combo.iter().any(|&f| present[f] < MIN_COMPLETE_ROWS) {
            return true;
        }
        evaluate_combo(enc, combo, cfg, z_bar, &mut hits);
        true
    });
    hits.sort_by(|a, b| {
        b.z.total_cmp(&a.z)
            .then(a.feats.cmp(&b.feats))
            .then(a.codes.cmp(&b.codes))
    });
    let stopped_cells = stopped.then_some(evaluated_cells);
    let mut per_combo: BTreeMap<Vec<usize>, usize> = BTreeMap::new();
    let mut findings = Vec::new();
    for hit in hits {
        let n_in_combo = per_combo.entry(hit.feats.clone()).or_insert(0);
        if *n_in_combo >= cfg.max_per_combination || findings.len() >= cfg.max_findings {
            continue;
        }
        *n_in_combo += 1;
        findings.push(render(enc, &hit, cells_examined, z_bar, stopped_cells));
    }
    let cov = if stopped {
        format!(
            "{} eligible columns, {} of {} combinations examined ({} of {} triples), {} of {} cells examined before the deadline stopped the scan, z bar {:.2} (Bonferroni over the full {}-cell universe, family alpha 0.01 — the stop does not reduce the correction); counts over the encoded sample of {} rows; combinations past the stop were not examined, so the absence of a hotspot there is unmeasured, not a negative",
            eligible.len(),
            evaluated_combos,
            n_combos,
            evaluated_triples,
            triples_examined,
            evaluated_cells,
            cells_examined,
            z_bar,
            cells_examined,
            enc.n,
        )
    } else {
        format!(
            "{} eligible columns, {} combinations examined ({} triples), {} cells examined, z bar {:.2} (Bonferroni over cells, family alpha 0.01); counts over the encoded sample of {} rows",
            eligible.len(),
            n_combos,
            triples_examined,
            cells_examined,
            z_bar,
            enc.n,
        )
    };
    HotspotsOutcome {
        findings,
        coverage: cov,
        deadline_stopped: stopped,
    }
}

/// Enumerate the combinations the pass examines, in the exact
/// order the materialized list used to hold them: pairs first
/// (eligible order, gated by the pair cell cap), then triples
/// when the config enables them — over the features with at
/// most 8 bins, in the same order, gated by the triple cell
/// cap, stopping once `cfg.max_triples` triples have been
/// examined (the cap check runs before each candidate, as it
/// did in the list-building loop). `visit` returns false to
/// halt the enumeration early (the deadline path); totals
/// then cover only the visited prefix. Returns (combinations,
/// triples, cells) totals: the coverage counts and the
/// Bonferroni denominator. Only the current combination is
/// materialized, so enumeration state is O(1) in the number of
/// combinations, beyond the O(width) eligible/small lists.
fn enumerate_combos(
    n_bins: &[usize],
    cfg: &HotspotConfig,
    eligible: &[usize],
    mut visit: impl FnMut(&[usize]) -> bool,
) -> (usize, usize, usize) {
    let mut n_combos = 0usize;
    let mut cells_examined = 0usize;
    let mut halted = false;
    'pairs: for a in 0..eligible.len() {
        for b in (a + 1)..eligible.len() {
            let (fa, fb) = (eligible[a], eligible[b]);
            let pair_cells = n_bins[fa] * n_bins[fb];
            if pair_cells <= cfg.max_cells_pair {
                if !visit(&[fa, fb]) {
                    halted = true;
                    break 'pairs;
                }
                n_combos += 1;
                cells_examined += pair_cells;
            }
        }
    }
    let mut triples_examined = 0usize;
    if cfg.triples && !halted {
        let small: Vec<usize> = eligible
            .iter()
            .copied()
            .filter(|&f| n_bins[f] <= 8)
            .collect();
        'outer: for a in 0..small.len() {
            for b in (a + 1)..small.len() {
                for c in (b + 1)..small.len() {
                    if triples_examined >= cfg.max_triples {
                        break 'outer;
                    }
                    let (fa, fb, fc) = (small[a], small[b], small[c]);
                    let triple_cells = n_bins[fa] * n_bins[fb] * n_bins[fc];
                    if triple_cells <= cfg.max_cells_triple {
                        if !visit(&[fa, fb, fc]) {
                            break 'outer;
                        }
                        n_combos += 1;
                        cells_examined += triple_cells;
                        triples_examined += 1;
                    }
                }
            }
        }
    }
    (n_combos, triples_examined, cells_examined)
}

/// Score every cell of one combination against independence of
/// its margins over the combination's complete rows.
fn evaluate_combo(
    enc: &EncodedMatrix,
    combo: &[usize],
    cfg: &HotspotConfig,
    z_bar: f64,
    hits: &mut Vec<CellHit>,
) {
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
        return;
    }
    let nf = n_complete as f64;
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
        let ratio = obs as f64 / expected;
        if ratio < cfg.min_ratio || obs as f64 / enc.n as f64 <= cfg.min_share {
            continue;
        }
        let z = (obs as f64 - expected) / expected.sqrt();
        if z < z_bar {
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
    // On a deadline-stopped run the claim says so: the bar is
    // still the full-universe bar (`cells_examined` is its
    // denominator), and `stopped_cells` states how far the
    // scan actually got. Unstopped claims are unchanged.
    let bar_clause = match stopped_cells {
        Some(evaluated) => format!(
            "bar {:.1} over {} cells planned; the scan stopped at the deadline after {} cells",
            z_bar, cells_examined, evaluated
        ),
        None => format!("bar {:.1} over {} cells examined", z_bar, cells_examined),
    };
    let mut metrics = BTreeMap::from([
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
    if let Some(evaluated) = stopped_cells {
        metrics.insert("cells_evaluated".into(), json!(evaluated));
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
        metrics,
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
    fn enumeration_matches_materialized_universe() {
        // Bin counts chosen to exercise the eligibility filter,
        // the pair cell cap, the triple cell cap over the
        // <= 8-bin subset, and the max-triples cutoff.
        let n_bins = vec![2usize, 3, 8, 9, 20, 21, 4, 8];
        let cfg = HotspotConfig {
            max_cells_pair: 64,
            max_cells_triple: 200,
            triples: true,
            max_triples: 5,
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
        let mut triples = 0usize;
        let small: Vec<usize> = eligible
            .iter()
            .copied()
            .filter(|&f| n_bins[f] <= 8)
            .collect();
        'outer: for a in 0..small.len() {
            for b in (a + 1)..small.len() {
                for c in (b + 1)..small.len() {
                    if triples >= cfg.max_triples {
                        break 'outer;
                    }
                    let (fa, fb, fc) = (small[a], small[b], small[c]);
                    if n_bins[fa] * n_bins[fb] * n_bins[fc] <= cfg.max_cells_triple {
                        expect.push(vec![fa, fb, fc]);
                        triples += 1;
                    }
                }
            }
        }
        let expect_cells: usize = expect
            .iter()
            .map(|c| c.iter().map(|&f| n_bins[f]).product::<usize>())
            .sum();
        let mut seen: Vec<Vec<usize>> = Vec::new();
        let (n, t, cells) = enumerate_combos(&n_bins, &cfg, &eligible, |c| {
            seen.push(c.to_vec());
            true
        });
        assert_eq!(seen, expect);
        assert_eq!(n, expect.len());
        assert_eq!(t, triples);
        assert_eq!(cells, expect_cells);
        assert!(triples == cfg.max_triples, "cutoff exercised");
    }

    /// A small encoded matrix with one planted hotspot: 6
    /// features of 4 bins over 400 rows; features 0 and 1 share
    /// a concentrated cell (100 rows at code (0, 0) against
    /// ~25 expected under independence).
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
    fn deadline_already_reached_stops_before_first_combination() {
        let enc = planted_matrix();
        let cfg = HotspotConfig {
            deadline: Deadline::at(std::time::Instant::now() - std::time::Duration::from_secs(1)),
            ..HotspotConfig::default()
        };
        let out = hotspots(&enc, &cfg);
        assert!(out.deadline_stopped);
        assert!(out.findings.is_empty());
        // 6 eligible features -> 15 pairs, 16 cells each.
        assert!(
            out.coverage.contains("0 of 15 combinations examined"),
            "coverage: {}",
            out.coverage
        );
        assert!(
            out.coverage.contains("0 of 240 cells examined"),
            "coverage: {}",
            out.coverage
        );
    }

    #[test]
    fn counting_stop_examines_exact_prefix() {
        // The stop predicate is consulted once per combination;
        // allowing exactly 5 consultations that return false and
        // stopping on the 6th must examine exactly the first 5
        // combinations of the enumeration order — the examined
        // set is a prefix, reproducible run to run (issue #117).
        let enc = planted_matrix();
        let cfg = HotspotConfig::default();
        let calls = std::cell::Cell::new(0usize);
        let out = hotspots_impl(&enc, &cfg, &|| {
            calls.set(calls.get() + 1);
            calls.get() > 5
        });
        assert!(out.deadline_stopped);
        assert!(
            out.coverage.contains("5 of 15 combinations examined"),
            "coverage: {}",
            out.coverage
        );
        assert!(
            out.coverage.contains("80 of 240 cells examined"),
            "coverage: {}",
            out.coverage
        );
        // A never-stopping run over the same matrix is the
        // exhaustive counterpart: full counts, no stop clause.
        let full = hotspots(&enc, &cfg);
        assert!(!full.deadline_stopped);
        assert!(
            full.coverage
                .contains("15 combinations examined (0 triples), 240 cells examined"),
            "coverage: {}",
            full.coverage
        );
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
        assert!(!timed.deadline_stopped);
        assert_eq!(plain.coverage, timed.coverage);
        assert_eq!(plain.findings.len(), timed.findings.len());
        for (a, b) in plain.findings.iter().zip(timed.findings.iter()) {
            assert_eq!(a.claim, b.claim);
        }
    }
}
