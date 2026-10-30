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

pub fn hotspots(enc: &EncodedMatrix, cfg: &HotspotConfig) -> (Vec<Finding>, String) {
    let eligible: Vec<usize> = (0..enc.feat_names.len())
        .filter(|&f| enc.n_bins[f] >= 2 && enc.n_bins[f] <= cfg.max_levels)
        .collect();

    // Combinations to examine, pairs first, then (deep) triples.
    let mut combos: Vec<Vec<usize>> = Vec::new();
    for a in 0..eligible.len() {
        for b in (a + 1)..eligible.len() {
            let (fa, fb) = (eligible[a], eligible[b]);
            if enc.n_bins[fa] * enc.n_bins[fb] <= cfg.max_cells_pair {
                combos.push(vec![fa, fb]);
            }
        }
    }
    let mut triples_examined = 0usize;
    if cfg.triples {
        let small: Vec<usize> = eligible
            .iter()
            .copied()
            .filter(|&f| enc.n_bins[f] <= 8)
            .collect();
        'outer: for a in 0..small.len() {
            for b in (a + 1)..small.len() {
                for c in (b + 1)..small.len() {
                    if triples_examined >= cfg.max_triples {
                        break 'outer;
                    }
                    let (fa, fb, fc) = (small[a], small[b], small[c]);
                    if enc.n_bins[fa] * enc.n_bins[fb] * enc.n_bins[fc] <= cfg.max_cells_triple {
                        combos.push(vec![fa, fb, fc]);
                        triples_examined += 1;
                    }
                }
            }
        }
    }
    let cells_examined: usize = combos
        .iter()
        .map(|c| c.iter().map(|&f| enc.n_bins[f]).product::<usize>())
        .sum();
    if combos.is_empty() || cells_examined == 0 {
        return (
            Vec::new(),
            format!(
                "{} eligible columns, no combinations within the cell caps",
                eligible.len()
            ),
        );
    }
    let z_bar = inv_normal_cdf(1.0 - 0.01 / (2.0 * cells_examined as f64));

    let mut hits: Vec<CellHit> = Vec::new();
    for combo in &combos {
        evaluate_combo(enc, combo, cfg, z_bar, &mut hits);
    }
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
    let cov = format!(
        "{} eligible columns, {} combinations examined ({} triples), {} cells examined, z bar {:.2} (Bonferroni over cells, family alpha 0.01); counts over the encoded sample of {} rows",
        eligible.len(),
        combos.len(),
        triples_examined,
        cells_examined,
        z_bar,
        enc.n,
    );
    (findings, cov)
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
    if n_complete < 200 {
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
) -> Finding {
    let names: Vec<String> = hit.feats.iter().map(|&f| enc.feat_names[f].clone()).collect();
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
        1.383577518672690e+02,
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
}
