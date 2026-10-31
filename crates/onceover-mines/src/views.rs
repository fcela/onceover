//! `views`: a deterministic greedy view/cluster pass.
//!
//! Columns partition into views (columns in one view share one row
//! clustering; across views they are independent), and rows within each
//! view partition into clusters with per-column conjugate component
//! models. Inference is a deterministic greedy search, not MCMC: no
//! randomness, no seed, fixed tie-breaks everywhere, so the same input
//! gives the same output. Scores are marginal-likelihood values, never
//! posterior probabilities. Design: docs/view-cluster-design.md.
//!
//! Determinism rules observed here: iteration only over Vec/BTreeMap in
//! fixed order; no HashMap iteration; no wall-clock values in findings
//! or metrics (runtime is the caller's to measure, never embedded).

use onceover_core::{sample_rows, TypedCol, TypedTable};
use onceover_report::{Category, Evidence, EvidenceBasis, Finding, Method};
use serde_json::json;
use std::collections::BTreeMap;

/// Configuration for the views pass. All bounds are count-based so the
/// pass itself introduces no nondeterminism.
#[derive(Clone, Debug)]
pub struct ViewsConfig {
    /// Sample rows for final scoring (usually the encoder row cap).
    pub row_cap: usize,
    /// Rows used for search decisions (splits, merges).
    pub search_rows: usize,
    /// Maximum eligible columns considered. When more columns are
    /// eligible, a pairwise-dependence pre-screen keeps the ones
    /// with the strongest normalized dependence to any other column (see
    /// `select_columns`); the rest are recorded as dropped.
    pub max_columns: usize,
    /// Maximum clusters per view.
    pub max_clusters: usize,
    /// Maximum merge attempts in the merge loop.
    pub tried_merges_cap: usize,
    /// Maximum categorical levels kept before pooling the tail.
    pub nominal_max_levels: usize,
    /// Complexity penalty multiplier (nats per cluster/view, times log n).
    pub gamma: f64,
    /// Anomaly threshold: median log predictive density minus this.
    pub anomaly_threshold_nats: f64,
    /// Maximum anomaly cells reported per column.
    pub anomaly_cap: usize,
    /// Minimum cluster size (sample rows) for a degenerate finding.
    pub min_degenerate_rows: usize,
    /// Cell-visit budget for split evaluation; stops search when hit.
    pub work_budget: u64,
}

impl Default for ViewsConfig {
    fn default() -> Self {
        ViewsConfig {
            row_cap: 20_000,
            search_rows: 2000,
            max_columns: 64,
            max_clusters: 8,
            tried_merges_cap: 256,
            nominal_max_levels: 32,
            gamma: 2.0,
            anomaly_threshold_nats: 10.0,
            anomaly_cap: 20,
            min_degenerate_rows: 30,
            work_budget: 300_000_000,
        }
    }
}

/// Outcome summary for coverage (no wall-clock values: determinism).
pub struct ViewsOutcome {
    pub coverage: String,
    pub n_views: usize,
    pub n_findings: usize,
    pub budget_hit: bool,
    pub dropped_columns: Vec<String>,
}

/// Natural log of the gamma function (Lanczos, g = 7).
fn ln_gamma(x: f64) -> f64 {
    // Coefficients from the standard g = 7 Lanczos approximation.
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.5203681218851,
        -1259.1392167224028,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507343278686905,
        -0.13857109526572012,
        9.984_369_578_019_572e-6,
        1.5056327351493116e-7,
    ];
    if x < 0.5 {
        // Reflection: Gamma(1 - x) * Gamma(x) = pi / sin(pi * x).
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let z = x - 1.0;
    let mut a = C[0];
    for (i, c) in C.iter().enumerate().skip(1) {
        a += c / (z + i as f64);
    }
    let t = z + 7.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (z + 0.5) * t.ln() - t + a.ln()
}

/// Log marginal likelihood of standardized values under NIG(0, 1, 2, 1).
/// `mean` and `m2` are the sample mean and sum of squared deviations.
fn nig_log_marginal(n: usize, mean: f64, m2: f64) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let (mu0, k0, a0, b0) = (0.0f64, 1.0f64, 2.0f64, 1.0f64);
    let nf = n as f64;
    let kn = k0 + nf;
    let an = a0 + nf / 2.0;
    let bn = b0 + m2 / 2.0 + k0 * nf * (mean - mu0).powi(2) / (2.0 * kn);
    ln_gamma(an) - ln_gamma(a0) + a0 * b0.ln() - an * bn.ln() + 0.5 * (k0.ln() - kn.ln())
        - (nf / 2.0) * (2.0 * std::f64::consts::PI).ln()
}

/// Log predictive density of a standardized value under a NIG cluster.
fn nig_log_predictive(x: f64, n: usize, mean: f64, m2: f64) -> f64 {
    let (mu0, k0, a0, b0) = (0.0f64, 1.0f64, 2.0f64, 1.0f64);
    let nf = n as f64;
    let kn = k0 + nf;
    let an = a0 + nf / 2.0;
    let bn = b0 + m2 / 2.0 + k0 * nf * (mean - mu0).powi(2) / (2.0 * kn);
    let mu_n = (k0 * mu0 + nf * mean) / kn;
    let nu = 2.0 * an;
    let s2 = bn * (kn + 1.0) / (an * kn);
    ln_gamma((nu + 1.0) / 2.0)
        - ln_gamma(nu / 2.0)
        - 0.5 * (nu * std::f64::consts::PI * s2).ln()
        - ((nu + 1.0) / 2.0) * (1.0 + (x - mu_n).powi(2) / (nu * s2)).ln()
}

/// Log marginal likelihood of level counts under Dirichlet(alpha).
fn dm_log_marginal(counts: &[u64], alpha: f64) -> f64 {
    let k = counts.len() as f64;
    let n: u64 = counts.iter().sum();
    let mut s = ln_gamma(k * alpha) - ln_gamma(n as f64 + k * alpha);
    for &c in counts {
        s += ln_gamma(c as f64 + alpha) - ln_gamma(alpha);
    }
    s
}

/// Log predictive probability of a level under Dirichlet(alpha) counts.
fn dm_log_predictive(level: usize, counts: &[u64], alpha: f64) -> f64 {
    let k = counts.len() as f64;
    let n: u64 = counts.iter().sum();
    ((counts[level] as f64 + alpha) / (n as f64 + k * alpha)).ln()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ColKind {
    Continuous,
    Categorical,
}

/// One extracted column on the sample rows. `vals` holds standardized
/// values (continuous) or level ids as f64 (categorical); `raw` holds
/// raw values (continuous only) for human-readable reporting.
struct ViewCol {
    name: String,
    kind: ColKind,
    vals: Vec<Option<f64>>,
    raw: Vec<Option<f64>>,
    n_levels: usize,
    level_names: Vec<String>,
}

/// Extract eligible columns on the given sample row indices (original
/// table row numbers). Returns (columns, dropped names).
fn extract_columns(
    table: &TypedTable,
    sample: &[usize],
    cfg: &ViewsConfig,
) -> (Vec<ViewCol>, Vec<String>) {
    let mut cols = Vec::new();
    let mut dropped = Vec::new();
    for (ci, col) in table.cols.iter().enumerate() {
        let name = table.names[ci].clone();
        let vals_opt: Option<ViewCol> = match col {
            TypedCol::I64 { vals } => {
                let raw: Vec<Option<f64>> = sample
                    .iter()
                    .map(|&r| vals.at(r).map(|v| v as f64))
                    .collect();
                Some(standardize(name.clone(), raw))
            }
            TypedCol::F64 { vals } => {
                let raw: Vec<Option<f64>> = sample.iter().map(|&r| vals.at(r)).collect();
                Some(standardize(name.clone(), raw))
            }
            TypedCol::Date { vals } => {
                let raw: Vec<Option<f64>> = sample
                    .iter()
                    .map(|&r| vals.get(r).copied().flatten().map(|v| v as f64))
                    .collect();
                Some(standardize(name.clone(), raw))
            }
            TypedCol::Datetime { vals } => {
                let raw: Vec<Option<f64>> = sample
                    .iter()
                    .map(|&r| vals.get(r).copied().flatten().map(|v| v as f64))
                    .collect();
                Some(standardize(name.clone(), raw))
            }
            TypedCol::Str { vals } => {
                let strs: Vec<Option<String>> = sample
                    .iter()
                    .map(|&r| vals.get(r).copied().flatten().map(str::to_owned))
                    .collect();
                Some(encode_levels(name.clone(), strs, cfg.nominal_max_levels))
            }
            TypedCol::StrOwned { vals } => {
                let strs: Vec<Option<String>> = sample
                    .iter()
                    .map(|&r| vals.get(r).and_then(|v| v.as_deref().map(str::to_owned)))
                    .collect();
                Some(encode_levels(name.clone(), strs, cfg.nominal_max_levels))
            }
            TypedCol::Bool { vals } => {
                let strs: Vec<Option<String>> = sample
                    .iter()
                    .map(|&r| {
                        vals.get(r)
                            .copied()
                            .flatten()
                            .map(|b| if b { "true" } else { "false" }.to_string())
                    })
                    .collect();
                Some(encode_levels(name.clone(), strs, cfg.nominal_max_levels))
            }
            TypedCol::Null { .. } | TypedCol::Other { .. } => None,
        };
        match vals_opt {
            Some(vc) => {
                // A column with no non-null values on the sample carries
                // no information; exclude it (recorded as dropped).
                let any = vc.vals.iter().any(|v| v.is_some());
                if !any {
                    dropped.push(name);
                } else {
                    cols.push(vc);
                }
            }
            None => dropped.push(name),
        }
    }
    (cols, dropped)
}

/// Equal-frequency bins used to discretize continuous columns for
/// the cap pre-screen.
const SCREEN_BINS: usize = 8;

/// Rows used by the cap pre-screen (a prefix of the search rows).
const SCREEN_ROW_LIMIT: usize = 256;

/// Pair evaluations the cap pre-screen performs at most; wider
/// tables are screened on a fixed stride over the pair list.
const SCREEN_PAIR_LIMIT: u64 = 250_000;

/// Discretize one column over the search rows for the cap
/// pre-screen: level ids for categorical columns, equal-frequency
/// bins for continuous ones. `None` marks missing values.
fn screen_codes(col: &ViewCol, search_pos: &[usize]) -> Vec<Option<u32>> {
    match col.kind {
        ColKind::Categorical => search_pos
            .iter()
            .map(|&p| col.vals[p].map(|v| v as u32))
            .collect(),
        ColKind::Continuous => {
            let mut present: Vec<(f64, usize)> = search_pos
                .iter()
                .enumerate()
                .filter_map(|(k, &p)| col.vals[p].map(|v| (v, k)))
                .collect();
            present.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            let mut out: Vec<Option<u32>> = vec![None; search_pos.len()];
            let n = present.len().max(1);
            for (rank, (_, k)) in present.iter().enumerate() {
                out[*k] = Some((rank * SCREEN_BINS / n) as u32);
            }
            out
        }
    }
}

/// Normalized mutual information between two coded columns, over
/// the rows where both are present: MI divided by the smaller
/// marginal entropy, in [0, 1]. Raw MI ranks frequent columns
/// above rare ones at equal association strength; the normalized
/// score ranks the association itself, so a rare column in a
/// near-perfect dependent pair scores near 1.
fn mutual_information(a: &[Option<u32>], b: &[Option<u32>]) -> f64 {
    let mut joint: BTreeMap<(u32, u32), u64> = BTreeMap::new();
    let mut ca: BTreeMap<u32, u64> = BTreeMap::new();
    let mut cb: BTreeMap<u32, u64> = BTreeMap::new();
    let mut n = 0u64;
    for (x, y) in a.iter().zip(b.iter()) {
        if let (Some(x), Some(y)) = (x, y) {
            *joint.entry((*x, *y)).or_default() += 1;
            *ca.entry(*x).or_default() += 1;
            *cb.entry(*y).or_default() += 1;
            n += 1;
        }
    }
    if n == 0 {
        return 0.0;
    }
    let nf = n as f64;
    let mut mi = 0.0;
    for ((x, y), c) in &joint {
        let pxy = *c as f64 / nf;
        mi += pxy * (pxy / ((ca[x] as f64 / nf) * (cb[y] as f64 / nf))).ln();
    }
    let entropy = |counts: &BTreeMap<u32, u64>| -> f64 {
        counts
            .values()
            .map(|&k| {
                let p = k as f64 / nf;
                -p * p.ln()
            })
            .sum()
    };
    let denom = entropy(&ca).min(entropy(&cb));
    if denom <= 0.0 {
        0.0
    } else {
        mi / denom
    }
}

/// Apply the column cap. With more eligible columns than the cap,
/// taking the first ones in table order silently removes every
/// later column from the pass — including columns in strong
/// dependent pairs, which the pass then cannot recover by
/// construction. Instead, rank each column by its strongest
/// pairwise normalized mutual information with any other eligible column
/// (over the search rows) and keep the top `max_columns`; ties
/// break by table order, and both the kept set and the dropped
/// list preserve table order. Returns (kept columns, dropped
/// names, whether selection ran). The screen charges the shared
/// work budget; if the budget runs out mid-screen the ranking
/// uses the pairs computed so far (iteration order is fixed, so
/// the result stays deterministic).
fn select_columns(
    cols: Vec<ViewCol>,
    search_pos: &[usize],
    cfg: &ViewsConfig,
    work: &mut WorkBudget,
) -> (Vec<ViewCol>, Vec<String>, bool) {
    if cols.len() <= cfg.max_columns {
        return (cols, Vec::new(), false);
    }
    // The screen's cost is bounded on both axes: at most
    // SCREEN_ROW_LIMIT rows, and at most SCREEN_PAIR_LIMIT pairs
    // (a fixed stride over the flattened pair list when the table
    // is wider than the pair limit, so every column keeps partners
    // spread across the table). Without the pair bound, a
    // several-thousand-column table makes the screen itself the
    // most expensive part of the pass.
    let screen_pos = &search_pos[..search_pos.len().min(SCREEN_ROW_LIMIT)];
    let codes: Vec<Vec<Option<u32>>> = cols.iter().map(|c| screen_codes(c, screen_pos)).collect();
    // A column with more distinct codes than half its present rows
    // (an identifier, pre-pooling) can memorize every other column
    // on the sample: its empirical dependence is maximal against
    // everything and means nothing. Pairs touching such a column
    // contribute no ranking signal to either side.
    let near_unique: Vec<bool> = codes
        .iter()
        .map(|col| {
            let mut distinct = std::collections::BTreeSet::new();
            let mut present = 0usize;
            for v in col.iter().flatten() {
                distinct.insert(*v);
                present += 1;
            }
            distinct.len() * 2 > present
        })
        .collect();
    let c = cols.len();
    let total_pairs = (c * (c - 1) / 2) as u64;
    let stride = (total_pairs / SCREEN_PAIR_LIMIT).max(1);
    let mut best = vec![0.0f64; c];
    let mut pair_idx = 0u64;
    'outer: for i in 0..c {
        for j in (i + 1)..c {
            let take = pair_idx.is_multiple_of(stride);
            pair_idx += 1;
            if !take {
                continue;
            }
            work.charge(screen_pos.len() as u64);
            if work.hit {
                break 'outer;
            }
            if near_unique[i] || near_unique[j] {
                continue;
            }
            let mi = mutual_information(&codes[i], &codes[j]);
            if mi > best[i] {
                best[i] = mi;
            }
            if mi > best[j] {
                best[j] = mi;
            }
        }
    }
    let mut order: Vec<usize> = (0..c).collect();
    order.sort_by(|&a, &b| best[b].total_cmp(&best[a]).then_with(|| a.cmp(&b)));
    let mut keep = vec![false; c];
    for &i in order.iter().take(cfg.max_columns) {
        keep[i] = true;
    }
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    for (i, col) in cols.into_iter().enumerate() {
        if keep[i] {
            kept.push(col);
        } else {
            dropped.push(col.name.clone());
        }
    }
    (kept, dropped, true)
}

/// Standardize a continuous column robustly: z = (x - median) / IQR,
/// with quartiles by linear interpolation (type 7). Robust scaling
/// matters: mean/sd scaling lets a few extreme spikes compress the bulk
/// so a constant block becomes indistinguishable from varied rows.
/// Fallbacks, in order: if IQR == 0, use mean/sd; if that degenerates
/// too, map everything to 0.0.
fn standardize(name: String, raw: Vec<Option<f64>>) -> ViewCol {
    let mut xs: Vec<f64> = raw.iter().filter_map(|v| *v).collect();
    xs.sort_by(|a, b| a.total_cmp(b));
    let quantile = |p: f64| -> Option<f64> {
        if xs.is_empty() {
            return None;
        }
        let h = p * (xs.len() - 1) as f64;
        let lo = h.floor() as usize;
        let hi = h.ceil() as usize;
        Some(xs[lo] + (h - lo as f64) * (xs[hi] - xs[lo]))
    };
    let (center, scale) = match (quantile(0.5), quantile(0.25), quantile(0.75)) {
        (Some(med), Some(q1), Some(q3)) if q3 - q1 > 0.0 => (med, q3 - q1),
        _ => {
            let n = xs.len();
            if n == 0 {
                (0.0, 0.0)
            } else {
                let mean = xs.iter().sum::<f64>() / n as f64;
                let sd = (xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64).sqrt();
                (mean, sd)
            }
        }
    };
    let vals: Vec<Option<f64>> = raw
        .iter()
        .map(|v| {
            v.map(|x| {
                if scale > 0.0 {
                    (x - center) / scale
                } else {
                    0.0
                }
            })
        })
        .collect();
    ViewCol {
        name,
        kind: ColKind::Continuous,
        vals,
        raw,
        n_levels: 0,
        level_names: Vec::new(),
    }
}

/// Level-encode a string/bool column: the most frequent levels (ties by
/// level name ascending) keep their identity up to the cap; the rest
/// pool into one trailing `other` level. The tail name must not
/// collide with a kept level: if the literal string "other" is itself
/// kept, the index map would otherwise overwrite its id with the tail
/// id, remapping real "other" rows into the tail and leaving a
/// zero-count ghost id.
fn encode_levels(name: String, strs: Vec<Option<String>>, cap: usize) -> ViewCol {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for s in strs.iter().flatten() {
        *counts.entry(s.clone()).or_insert(0) += 1;
    }
    let mut levels: Vec<(String, u64)> = counts.into_iter().collect();
    levels.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let kept: Vec<String> = levels.iter().take(cap).map(|(s, _)| s.clone()).collect();
    let kept_n = kept.len();
    let tail_count: u64 = levels.iter().skip(cap).map(|(_, c)| c).sum();
    let has_tail = tail_count > 0;
    let mut level_names = kept.clone();
    if has_tail {
        let mut tail_name = "other".to_string();
        while level_names.contains(&tail_name) {
            tail_name = format!("{tail_name} (pooled)");
        }
        level_names.push(tail_name);
    }
    let index: BTreeMap<String, usize> = level_names
        .iter()
        .enumerate()
        .map(|(i, s)| (s.clone(), i))
        .collect();
    let tail_id = kept_n;
    let vals: Vec<Option<f64>> = strs
        .iter()
        .map(|s| {
            s.as_ref()
                .map(|v| index.get(v).copied().unwrap_or(tail_id) as f64)
        })
        .collect();
    ViewCol {
        name,
        kind: ColKind::Categorical,
        vals,
        raw: Vec::new(),
        n_levels: level_names.len(),
        level_names,
    }
}

/// Cell-visit budget for split evaluation. When exceeded, search stops
/// with its current partition and records the hit (bounded compute).
struct WorkBudget {
    used: u64,
    cap: u64,
    hit: bool,
}

impl WorkBudget {
    fn charge(&mut self, n: u64) {
        self.used += n;
        if self.used > self.cap {
            self.hit = true;
        }
    }
}

/// Sufficient-statistic accumulator for one column within one cluster.
#[derive(Clone)]
enum Acc {
    Cont { n: usize, mean: f64, m2: f64 },
    Cat { counts: Vec<u64> },
}

impl Acc {
    fn new(kind: ColKind, n_levels: usize) -> Acc {
        match kind {
            ColKind::Continuous => Acc::Cont {
                n: 0,
                mean: 0.0,
                m2: 0.0,
            },
            ColKind::Categorical => Acc::Cat {
                counts: vec![0; n_levels],
            },
        }
    }

    fn add(&mut self, v: Option<f64>) {
        let Some(x) = v else { return };
        match self {
            Acc::Cont { n, mean, m2 } => {
                *n += 1;
                let delta = x - *mean;
                *mean += delta / *n as f64;
                *m2 += delta * (x - *mean);
            }
            Acc::Cat { counts } => {
                let i = x as usize;
                if i < counts.len() {
                    counts[i] += 1;
                }
            }
        }
    }

    /// Inverse of `add`: remove one observation (inverse Welford for
    /// continuous, decrement for categorical). Nulls are a no-op, as in
    /// `add`. Used for leave-one-out scoring: a row must never be
    /// scored under statistics that contain itself.
    fn remove(&mut self, v: Option<f64>) {
        let Some(x) = v else { return };
        match self {
            Acc::Cont { n, mean, m2 } => {
                if *n == 0 {
                    return;
                }
                if *n == 1 {
                    *n = 0;
                    *mean = 0.0;
                    *m2 = 0.0;
                    return;
                }
                let nf = *n as f64;
                let mean_new = (nf * *mean - x) / (nf - 1.0);
                *m2 -= (x - *mean) * (x - mean_new);
                *mean = mean_new;
                *n -= 1;
            }
            Acc::Cat { counts } => {
                let i = x as usize;
                if i < counts.len() {
                    counts[i] = counts[i].saturating_sub(1);
                }
            }
        }
    }

    fn marginal(&self) -> f64 {
        match self {
            Acc::Cont { n, mean, m2 } => nig_log_marginal(*n, *mean, *m2),
            Acc::Cat { counts } => dm_log_marginal(counts, 1.0),
        }
    }

    fn log_predictive(&self, x: f64) -> f64 {
        match self {
            Acc::Cont { n, mean, m2 } => nig_log_predictive(x, *n, *mean, *m2),
            Acc::Cat { counts } => {
                let i = x as usize;
                if i < counts.len() {
                    dm_log_predictive(i, counts, 1.0)
                } else {
                    // Unreachable through encode, but an unknown level
                    // is impossible, not certain: never score it 1.0.
                    f64::NEG_INFINITY
                }
            }
        }
    }
}

/// Marginal sum of a clustering: sum over groups and columns of the
/// component log marginal, minus gamma * log(n) per group.
fn cluster_score(
    cols: &[ViewCol],
    col_idx: &[usize],
    groups: &[Vec<usize>],
    gamma: f64,
    n_ref: usize,
) -> f64 {
    let penalty = gamma * (n_ref.max(1) as f64).ln();
    let mut total = -penalty * groups.len() as f64;
    for g in groups {
        for &ci in col_idx {
            let mut acc = Acc::new(cols[ci].kind, cols[ci].n_levels);
            for &row in g {
                acc.add(cols[ci].vals[row]);
            }
            total += acc.marginal();
        }
    }
    total
}

/// Split threshold for a column within a row set: the distinct-value
/// boundary nearest the median position. For distinct values this is
/// exactly the lower median value. For tied values it is load-bearing:
/// splitting at the median VALUE degenerates whenever the median equals
/// the cluster maximum (left side = every row, right side empty), which
/// happens routinely for discrete-valued columns — a singleton column
/// then cannot split at all while a pair view can split around it,
/// manufacturing a spurious merge. Splitting at a genuine value
/// boundary always separates two non-empty value groups. Returns None
/// when the column is constant (or empty) within the row set.
fn split_threshold(cols: &[ViewCol], ci: usize, rows: &[usize]) -> Option<f64> {
    let mut vs: Vec<f64> = rows.iter().filter_map(|&r| cols[ci].vals[r]).collect();
    if vs.is_empty() {
        return None;
    }
    vs.sort_by(|a, b| a.total_cmp(b));
    let m = vs.len();
    let target = (m - 1) / 2;
    let mut best: Option<(usize, f64)> = None;
    for k in 0..m.saturating_sub(1) {
        if vs[k] < vs[k + 1] {
            let dist = k.abs_diff(target);
            let better = match best {
                None => true,
                Some((bk, _)) => {
                    let bd = bk.abs_diff(target);
                    dist < bd || (dist == bd && k < bk)
                }
            };
            if better {
                best = Some((k, vs[k]));
            }
        }
    }
    best.map(|(_, t)| t)
}

/// Divisive median-split clustering of a column set over search rows
/// (sample positions). Returns groups (sample positions) and score.
fn phase_b(
    cols: &[ViewCol],
    col_idx: &[usize],
    search_rows: &[usize],
    cfg: &ViewsConfig,
    work: &mut WorkBudget,
) -> (Vec<Vec<usize>>, f64) {
    let n_search = search_rows.len();
    if n_search == 0 || col_idx.is_empty() {
        return (Vec::new(), 0.0);
    }
    let penalty = cfg.gamma * (n_search.max(1) as f64).ln();
    let mut groups: Vec<Vec<usize>> = vec![search_rows.to_vec()];
    loop {
        if groups.len() >= cfg.max_clusters || work.hit {
            break;
        }
        // Whole-cluster marginals and split thresholds, once per round.
        let mut whole: Vec<Vec<f64>> = Vec::with_capacity(groups.len());
        let mut thresholds: Vec<Vec<Option<f64>>> = Vec::with_capacity(groups.len());
        for g in &groups {
            let mut wrow = Vec::with_capacity(col_idx.len());
            let mut mrow = Vec::with_capacity(col_idx.len());
            for &ci in col_idx {
                let mut acc = Acc::new(cols[ci].kind, cols[ci].n_levels);
                for &row in g {
                    acc.add(cols[ci].vals[row]);
                }
                wrow.push(acc.marginal());
                mrow.push(split_threshold(cols, ci, g));
            }
            whole.push(wrow);
            thresholds.push(mrow);
        }
        // Best split: highest gain, ties by lowest cluster then column.
        let mut best: Option<(f64, usize, usize)> = None;
        for (gi, g) in groups.iter().enumerate() {
            if g.len() < 2 {
                continue;
            }
            for (jj, &ci) in col_idx.iter().enumerate() {
                let Some(med) = thresholds[gi][jj] else {
                    continue;
                };
                // Side counts for null placement (non-null rows only).
                let mut left_nn = 0usize;
                let mut right_nn = 0usize;
                for &row in g {
                    if let Some(x) = cols[ci].vals[row] {
                        if x <= med {
                            left_nn += 1;
                        } else {
                            right_nn += 1;
                        }
                    }
                }
                if left_nn == 0 || right_nn == 0 {
                    continue;
                }
                let null_left = left_nn >= right_nn;
                let mut left_acc: Vec<Acc> = col_idx
                    .iter()
                    .map(|&cj| Acc::new(cols[cj].kind, cols[cj].n_levels))
                    .collect();
                let mut right_acc: Vec<Acc> = col_idx
                    .iter()
                    .map(|&cj| Acc::new(cols[cj].kind, cols[cj].n_levels))
                    .collect();
                let mut left_rows = 0usize;
                let mut right_rows = 0usize;
                for &row in g {
                    let left = match cols[ci].vals[row] {
                        Some(x) => x <= med,
                        None => null_left,
                    };
                    if left {
                        left_rows += 1;
                    } else {
                        right_rows += 1;
                    }
                    for (k, &cj) in col_idx.iter().enumerate() {
                        if left {
                            left_acc[k].add(cols[cj].vals[row]);
                        } else {
                            right_acc[k].add(cols[cj].vals[row]);
                        }
                    }
                }
                work.charge((g.len() * col_idx.len()) as u64);
                if work.hit {
                    break;
                }
                if left_rows == 0 || right_rows == 0 {
                    continue;
                }
                let mut gain = -penalty;
                for k in 0..col_idx.len() {
                    gain += left_acc[k].marginal() + right_acc[k].marginal() - whole[gi][k];
                }
                let better = match best {
                    None => true,
                    Some((bg, bgi, bj)) => gain > bg || (gain == bg && (gi, jj) < (bgi, bj)),
                };
                if better {
                    best = Some((gain, gi, jj));
                }
            }
            if work.hit {
                break;
            }
        }
        match best {
            Some((gain, gi, jj)) if gain > 0.0 && !work.hit => {
                let ci = col_idx[jj];
                let med = thresholds[gi][jj].expect("best split has a threshold");
                let g = groups.remove(gi);
                let mut left_nn = 0usize;
                let mut right_nn = 0usize;
                for &row in &g {
                    if let Some(x) = cols[ci].vals[row] {
                        if x <= med {
                            left_nn += 1;
                        } else {
                            right_nn += 1;
                        }
                    }
                }
                let null_left = left_nn >= right_nn;
                let mut left = Vec::new();
                let mut right = Vec::new();
                for row in g {
                    let is_left = match cols[ci].vals[row] {
                        Some(x) => x <= med,
                        None => null_left,
                    };
                    if is_left {
                        left.push(row);
                    } else {
                        right.push(row);
                    }
                }
                groups.push(left);
                groups.push(right);
            }
            _ => break,
        }
    }
    let score = cluster_score(cols, col_idx, &groups, cfg.gamma, n_search);
    (groups, score)
}

/// One view during search: its columns, search-sample clustering, score.
struct ViewState {
    cols: Vec<usize>,
    groups: Vec<Vec<usize>>,
    score: f64,
    merge_gain: f64,
}

/// Greedy view discovery: singleton clusterings, pair pre-screen by
/// shared-clustering gain, then merges in pre-screen order.
fn views_search(
    cols: &[ViewCol],
    search_rows: &[usize],
    cfg: &ViewsConfig,
    work: &mut WorkBudget,
) -> Vec<ViewState> {
    let n_search = search_rows.len();
    let c = cols.len();
    let penalty = cfg.gamma * (n_search.max(1) as f64).ln();
    // Singleton scores.
    let mut single_scores = Vec::with_capacity(c);
    let mut single_groups = Vec::with_capacity(c);
    for i in 0..c {
        let (g, s) = phase_b(cols, &[i], search_rows, cfg, work);
        single_groups.push(g);
        single_scores.push(s);
        if work.hit {
            break;
        }
    }
    let mut views: Vec<ViewState> = (0..single_groups.len())
        .map(|i| ViewState {
            cols: vec![i],
            groups: std::mem::take(&mut single_groups[i]),
            score: single_scores[i],
            merge_gain: 0.0,
        })
        .collect();
    if work.hit || c < 2 {
        return views;
    }
    // Pair pre-screen.
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for i in 0..c {
        for j in (i + 1)..c {
            let (_, s) = phase_b(cols, &[i, j], search_rows, cfg, work);
            pairs.push((s - single_scores[i] - single_scores[j], i, j));
            if work.hit {
                break;
            }
        }
        if work.hit {
            break;
        }
    }
    pairs.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    // Merge loop.
    let mut col_to_view: Vec<usize> = (0..c).collect();
    let mut attempts = 0usize;
    for (_, i, j) in pairs {
        if work.hit || attempts >= cfg.tried_merges_cap {
            break;
        }
        let vi = col_to_view[i];
        let vj = col_to_view[j];
        if vi == vj {
            continue;
        }
        attempts += 1;
        let mut combined = views[vi].cols.clone();
        combined.extend(views[vj].cols.iter().copied());
        combined.sort_unstable();
        let (mgroups, mscore) = phase_b(cols, &combined, search_rows, cfg, work);
        if work.hit {
            break;
        }
        // A merged view with a single cluster carries no partition
        // evidence: two single-cluster columns would otherwise merge on
        // pure penalty savings (2 * gamma * ln n) and report a vacuous
        // dependence block. Merges must earn a real clustering.
        if mgroups.len() < 2 {
            continue;
        }
        let delta = mscore - views[vi].score - views[vj].score + penalty;
        if delta > 0.0 {
            let gain = views[vi].merge_gain + views[vj].merge_gain + delta;
            let (lo, hi) = if vi < vj { (vi, vj) } else { (vj, vi) };
            views[lo] = ViewState {
                cols: combined,
                groups: mgroups,
                score: mscore,
                merge_gain: gain,
            };
            views.remove(hi);
            col_to_view = vec![0; c];
            for (vidx, v) in views.iter().enumerate() {
                for &col in &v.cols {
                    col_to_view[col] = vidx;
                }
            }
        }
    }
    views
}

/// Per-cluster, per-column statistics from a row assignment.
fn stats_for(cols: &[ViewCol], col_idx: &[usize], groups: &[Vec<usize>]) -> Vec<Vec<Acc>> {
    groups
        .iter()
        .map(|g| {
            col_idx
                .iter()
                .map(|&ci| {
                    let mut acc = Acc::new(cols[ci].kind, cols[ci].n_levels);
                    for &row in g {
                        acc.add(cols[ci].vals[row]);
                    }
                    acc
                })
                .collect()
        })
        .collect()
}

/// Assign every sample position to the cluster maximizing total log
/// predictive density across the view's columns (nulls contribute 0).
/// Ties go to the lowest cluster index. Scoring is leave-one-out: a
/// row is scored under its own cluster's statistics with itself
/// removed, so outliers cannot partly explain themselves; rows that
/// did not contribute to `stats` (non-search rows on the first pass)
/// score under the full statistics. `stat_groups` are the groups whose
/// rows built `stats`.
fn assign_rows(
    cols: &[ViewCol],
    col_idx: &[usize],
    stats: &[Vec<Acc>],
    n_sample: usize,
    stat_groups: &[Vec<usize>],
) -> Vec<usize> {
    let mut home = vec![None; n_sample];
    for (k, g) in stat_groups.iter().enumerate() {
        for &p in g {
            if p < n_sample {
                home[p] = Some(k);
            }
        }
    }
    let mut assignment = vec![0usize; n_sample];
    for (p, slot) in assignment.iter_mut().enumerate() {
        let mut best_k = 0usize;
        let mut best_s = f64::NEG_INFINITY;
        for (k, kstats) in stats.iter().enumerate() {
            let mut s = 0.0;
            for (jj, &ci) in col_idx.iter().enumerate() {
                if let Some(x) = cols[ci].vals[p] {
                    if home[p] == Some(k) {
                        let mut loo = kstats[jj].clone();
                        loo.remove(Some(x));
                        s += loo.log_predictive(x);
                    } else {
                        s += kstats[jj].log_predictive(x);
                    }
                }
            }
            if s > best_s {
                best_s = s;
                best_k = k;
            }
        }
        *slot = best_k;
    }
    assignment
}

/// Invert an assignment into groups (sample positions per cluster),
/// dropping empty clusters.
fn groups_from_assignment(assignment: &[usize], n_clusters: usize) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); n_clusters];
    for (p, &k) in assignment.iter().enumerate() {
        if k < n_clusters {
            groups[k].push(p);
        }
    }
    groups.into_iter().filter(|g| !g.is_empty()).collect()
}

const HONESTY: &str =
    "Greedy deterministic partition; scores are marginal-likelihood values, not posterior probabilities.";

fn make_evidence(metrics: BTreeMap<String, serde_json::Value>) -> Evidence {
    let mut e = Evidence::new(
        Method::new(
            "views",
            "greedy_view_cluster",
            "deterministic greedy view/cluster partition; marginal-likelihood scores, not posterior probabilities",
        ),
        EvidenceBasis::Estimated,
        metrics,
    );
    e.scope.missing_values =
        "nulls excluded from component scores; null counts recorded in metrics".to_string();
    e
}

/// Run the views pass on an extracted table. Returns findings and an
/// outcome summary (no wall-clock values: determinism).
pub fn views_pass(table: &TypedTable, cfg: &ViewsConfig) -> (Vec<Finding>, ViewsOutcome) {
    let empty_outcome = |detail: &str| ViewsOutcome {
        coverage: detail.to_string(),
        n_views: 0,
        n_findings: 0,
        budget_hit: false,
        dropped_columns: Vec::new(),
    };
    if table.n == 0 {
        return (Vec::new(), empty_outcome("no rows"));
    }
    let sample: Vec<usize> = sample_rows(table.n, cfg.row_cap)
        .iter()
        .map(|&r| r as usize)
        .collect();
    let n_sample = sample.len();
    let (all_cols, mut dropped) = extract_columns(table, &sample, cfg);
    if all_cols.is_empty() {
        return (
            Vec::new(),
            ViewsOutcome {
                coverage: "no eligible columns".to_string(),
                n_views: 0,
                n_findings: 0,
                budget_hit: false,
                dropped_columns: dropped,
            },
        );
    }
    let search_pos: Vec<usize> = sample_rows(n_sample, cfg.search_rows)
        .iter()
        .map(|&r| r as usize)
        .collect();
    let mut work = WorkBudget {
        used: 0,
        cap: cfg.work_budget,
        hit: false,
    };
    let n_eligible = all_cols.len();
    let (cols, cap_dropped, prescreened) = select_columns(all_cols, &search_pos, cfg, &mut work);
    dropped.extend(cap_dropped);
    let views = views_search(&cols, &search_pos, cfg, &mut work);
    // A budget hit mid-singleton drops tail columns from the search
    // entirely: they are in no view. They are not extract drops, so
    // list them separately rather than losing them silently. (A hit
    // later in search drops nothing: every column is already a view.)
    let mut budget_dropped: Vec<String> = Vec::new();
    if work.hit {
        let covered: std::collections::BTreeSet<usize> =
            views.iter().flat_map(|v| v.cols.iter().copied()).collect();
        for (i, col) in cols.iter().enumerate() {
            if !covered.contains(&i) {
                budget_dropped.push(col.name.clone());
            }
        }
    }
    let mut findings: Vec<Finding> = Vec::new();
    let mut total_score = 0.0;
    let penalty_full = cfg.gamma * (n_sample.max(1) as f64).ln();
    for view in &views {
        if view.groups.is_empty() {
            continue;
        }
        // Assignment: search stats -> assign -> full stats -> reassign
        // once -> final full stats. Deterministic and bounded.
        let search_stats = stats_for(&cols, &view.cols, &view.groups);
        let a1 = assign_rows(&cols, &view.cols, &search_stats, n_sample, &view.groups);
        let g1 = groups_from_assignment(&a1, view.groups.len());
        if g1.is_empty() {
            continue;
        }
        let stats1 = stats_for(&cols, &view.cols, &g1);
        let a2 = assign_rows(&cols, &view.cols, &stats1, n_sample, &g1);
        let groups = groups_from_assignment(&a2, g1.len());
        if groups.is_empty() {
            continue;
        }
        let stats = stats_for(&cols, &view.cols, &groups);
        // Full-sample view score.
        let mut view_score = -penalty_full * groups.len() as f64;
        for kstats in &stats {
            for acc in kstats {
                view_score += acc.marginal();
            }
        }
        total_score += view_score;
        findings.extend(emit_view_findings(
            table, &cols, view, &groups, &stats, &sample, n_sample, view_score, cfg, &dropped,
        ));
    }
    total_score -= penalty_full * views.len() as f64;
    let n_findings = findings.len();
    let budget_suffix = if work.hit {
        if budget_dropped.is_empty() {
            "; search work budget reached".to_string()
        } else {
            format!(
                "; search work budget reached; columns omitted by budget: {}",
                budget_dropped.join(", ")
            )
        }
    } else {
        String::new()
    };
    let cap_suffix = if prescreened {
        format!(
            "; {} eligible columns over the {}-column cap: kept the {} with \
             the strongest pairwise dependence (mutual-information pre-screen)",
            n_eligible,
            cfg.max_columns,
            cfg.max_columns.min(n_eligible)
        )
    } else {
        String::new()
    };
    let coverage = format!(
        "{} views ({} multi-column), {} findings, sample {} of {} rows, total score {:.1} nats{}{}",
        views.len(),
        views.iter().filter(|v| v.cols.len() > 1).count(),
        n_findings,
        n_sample,
        table.n,
        total_score,
        budget_suffix,
        cap_suffix
    );
    (
        findings,
        ViewsOutcome {
            coverage,
            n_views: views.len(),
            n_findings,
            budget_hit: work.hit,
            dropped_columns: dropped,
        },
    )
}

/// Per-cluster summaries for reporting: original row range, size,
/// per-column mean (continuous, raw scale) or top level (categorical).
struct ClusterSummary {
    rows_min: usize,
    rows_max: usize,
    n: usize,
    means: Vec<Option<f64>>,
    top_levels: Vec<Option<(String, f64)>>,
}

fn summarize_clusters(
    cols: &[ViewCol],
    col_idx: &[usize],
    groups: &[Vec<usize>],
    stats: &[Vec<Acc>],
    sample: &[usize],
) -> Vec<ClusterSummary> {
    groups
        .iter()
        .enumerate()
        .map(|(k, g)| {
            let orig: Vec<usize> = g.iter().map(|&p| sample[p]).collect();
            let rows_min = orig.iter().copied().min().unwrap_or(0);
            let rows_max = orig.iter().copied().max().unwrap_or(0);
            let mut means = Vec::new();
            let mut top_levels = Vec::new();
            for (jj, &ci) in col_idx.iter().enumerate() {
                match &stats[k][jj] {
                    Acc::Cont { .. } => {
                        // Raw mean over non-null raw values in the cluster.
                        let mut s = 0.0;
                        let mut n = 0usize;
                        for &p in g {
                            if let Some(x) = cols[ci].raw[p] {
                                s += x;
                                n += 1;
                            }
                        }
                        means.push(if n > 0 { Some(s / n as f64) } else { None });
                        top_levels.push(None);
                    }
                    Acc::Cat { counts } => {
                        means.push(None);
                        let total: u64 = counts.iter().sum();
                        let (bi, bc) = counts
                            .iter()
                            .enumerate()
                            .max_by_key(|(_, &c)| c)
                            .map(|(i, &c)| (i, c))
                            .unwrap_or((0, 0));
                        top_levels.push(if total > 0 {
                            Some((
                                cols[ci].level_names.get(bi).cloned().unwrap_or_default(),
                                bc as f64 / total as f64,
                            ))
                        } else {
                            None
                        });
                    }
                }
            }
            ClusterSummary {
                rows_min,
                rows_max,
                n: g.len(),
                means,
                top_levels,
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn emit_view_findings(
    table: &TypedTable,
    cols: &[ViewCol],
    view: &ViewState,
    groups: &[Vec<usize>],
    stats: &[Vec<Acc>],
    sample: &[usize],
    n_sample: usize,
    view_score: f64,
    cfg: &ViewsConfig,
    dropped: &[String],
) -> Vec<Finding> {
    use onceover_report::Subject;
    let mut out = Vec::new();
    let names: Vec<String> = view.cols.iter().map(|&ci| cols[ci].name.clone()).collect();
    let summaries = summarize_clusters(cols, &view.cols, groups, stats, sample);
    let mut base_metrics = BTreeMap::new();
    base_metrics.insert("gamma".to_string(), json!(cfg.gamma));
    base_metrics.insert("n".to_string(), json!(n_sample));
    base_metrics.insert("population_rows".to_string(), json!(table.n));
    base_metrics.insert("sampled".to_string(), json!(n_sample < table.n));
    base_metrics.insert("search_rows".to_string(), json!(cfg.search_rows));
    base_metrics.insert("max_columns".to_string(), json!(cfg.max_columns));
    base_metrics.insert("max_clusters".to_string(), json!(cfg.max_clusters));
    base_metrics.insert(
        "anomaly_threshold_nats".to_string(),
        json!(cfg.anomaly_threshold_nats),
    );
    base_metrics.insert("anomaly_cap".to_string(), json!(cfg.anomaly_cap));
    base_metrics.insert("view_columns".to_string(), json!(names));
    base_metrics.insert("n_clusters".to_string(), json!(groups.len()));
    base_metrics.insert("view_score_nats".to_string(), json!(view_score));
    base_metrics.insert("merge_gain_nats".to_string(), json!(view.merge_gain));
    base_metrics.insert("dropped_columns".to_string(), json!(dropped));
    // Null counts per subject column over the view's rows.
    let mut null_counts = BTreeMap::new();
    for &ci in &view.cols {
        let nc = cols[ci].vals.iter().filter(|v| v.is_none()).count();
        null_counts.insert(cols[ci].name.clone(), json!(nc));
    }
    base_metrics.insert("null_counts".to_string(), json!(null_counts));

    // ---- View finding (multi-column views only).
    if view.cols.len() > 1 {
        let mut m = base_metrics.clone();
        m.insert(
            "cluster_row_ranges".to_string(),
            json!(summaries
                .iter()
                .map(|s| json!({"rows_min": s.rows_min, "rows_max": s.rows_max, "n": s.n}))
                .collect::<Vec<_>>()),
        );
        out.push(Finding::from_statement(
            Category::Associations,
            Some("statistical".to_string()),
            Subject::columns(names.clone()),
            Default::default(),
            format!(
                "Columns {} form a dependence block under the greedy view partition (merge gain {:.1} nats over separate views on the search sample; {}).",
                names.join(", "),
                view.merge_gain,
                HONESTY
            ),
            make_evidence(m),
            0.60,
            Some(HONESTY.to_string()),
        ));
    }

    // ---- Regime finding (views with >= 2 clusters): largest shift.
    if groups.len() >= 2 {
        // Shift per column: continuous = max pairwise |z-mean diff|;
        // categorical = max pairwise total-variation distance.
        let mut best: Option<(f64, usize)> = None;
        for (jj, &ci) in view.cols.iter().enumerate() {
            let shift = match cols[ci].kind {
                ColKind::Continuous => {
                    let mut m = f64::NEG_INFINITY;
                    for a in 0..groups.len() {
                        for b in (a + 1)..groups.len() {
                            let (ma, na) = cont_mean(&cols[ci], &groups[a]);
                            let (mb, nb) = cont_mean(&cols[ci], &groups[b]);
                            if na > 0 && nb > 0 {
                                m = m.max((ma - mb).abs());
                            }
                        }
                    }
                    if m == f64::NEG_INFINITY {
                        0.0
                    } else {
                        m
                    }
                }
                ColKind::Categorical => {
                    let mut m = 0.0f64;
                    for a in 0..groups.len() {
                        for b in (a + 1)..groups.len() {
                            // An all-null cluster has no distribution;
                            // its zero-vector distance (TV 0.5 against
                            // anything) is arbitrary, not a shift.
                            let (ta, tb) = match (&stats[a][jj], &stats[b][jj]) {
                                (Acc::Cat { counts: ca }, Acc::Cat { counts: cb }) => {
                                    (ca.iter().sum::<u64>(), cb.iter().sum::<u64>())
                                }
                                _ => (0, 0),
                            };
                            if ta == 0 || tb == 0 {
                                continue;
                            }
                            let pa = cat_dist(&stats[a][jj]);
                            let pb = cat_dist(&stats[b][jj]);
                            let tv: f64 = pa
                                .iter()
                                .zip(pb.iter())
                                .map(|(x, y)| (x - y).abs())
                                .sum::<f64>()
                                / 2.0;
                            m = m.max(tv);
                        }
                    }
                    m
                }
            };
            let better = match best {
                None => true,
                Some((bs, bi)) => shift > bs || (shift == bs && jj < bi),
            };
            if better {
                best = Some((shift, jj));
            }
        }
        if let Some((shift, jj)) = best {
            if shift > 0.0 {
                let ci = view.cols[jj];
                let mut spec = onceover_report::Specification::default();
                spec.definition.insert(
                    "reference_model".to_string(),
                    json!("single-component per column"),
                );
                spec.definition.insert(
                    "clusters".to_string(),
                    json!(summaries
                        .iter()
                        .map(|s| {
                            let mut o =
                                json!({"rows_min": s.rows_min, "rows_max": s.rows_max, "n": s.n});
                            if let Some(m) = s.means[jj] {
                                o["mean"] = json!(m);
                            }
                            if let Some((lv, sh)) = &s.top_levels[jj] {
                                o["top_level"] = json!(lv);
                                o["top_share"] = json!(sh);
                            }
                            o
                        })
                        .collect::<Vec<_>>()),
                );
                spec.definition
                    .insert("headline_column".to_string(), json!(cols[ci].name));
                let mut m = base_metrics.clone();
                m.insert("headline_shift".to_string(), json!(shift));
                out.push(Finding::from_statement(
                    Category::Changes,
                    Some("residual_level".to_string()),
                    Subject::columns(names.clone()),
                    spec,
                    format!(
                        "Rows split into {} clusters; headline column {} shifts by {:.2} between clusters ({}).",
                        groups.len(),
                        cols[ci].name,
                        shift,
                        HONESTY
                    ),
                    make_evidence(m),
                    0.70,
                    Some(HONESTY.to_string()),
                ));
            }
        }
    }

    // ---- Degenerate-cluster findings, per column. A cluster is
    // degenerate when one exact value (continuous) or level
    // (categorical) holds at least 90% of its non-null rows. Purity is
    // deliberately NOT required: greedy search rarely isolates a
    // constant block exactly (it usually lands as the dominant share
    // of a slightly mixed cluster), and a 90%-dominant value is the
    // honest statement of that structure.
    for (jj, &ci) in view.cols.iter().enumerate() {
        let col = &cols[ci];
        // Modal findings are one statement per column: collecting
        // entries here keeps multiple modal clusters (and the spike
        // finding below) from colliding on one selector key.
        let mut modal_entries: Vec<(usize, usize, usize, usize, serde_json::Value, f64, usize)> =
            Vec::new();
        for (k, g) in groups.iter().enumerate() {
            // The gate counts non-null observations, not rows: a
            // cluster of 40 rows with 39 nulls holds one observation,
            // not a degenerate finding.
            let non_null = g.iter().filter(|&&p| col.vals[p].is_some()).count();
            if non_null < cfg.min_degenerate_rows {
                continue;
            }
            let (modal_value, modal_share, modal_n): (serde_json::Value, f64, usize) =
                match &stats[k][jj] {
                    Acc::Cont { .. } => {
                        let mut counts: BTreeMap<u64, (usize, f64)> = BTreeMap::new();
                        for &p in g {
                            if let (Some(z), Some(r)) = (col.vals[p], col.raw[p]) {
                                // Canonicalize signed zero: +0.0 and
                                // -0.0 are the same value and must count
                                // together, or a mixed-sign zero column
                                // escapes the modal test on bit keys.
                                let key = if z == 0.0 { 0.0f64 } else { z }.to_bits();
                                let e = counts.entry(key).or_insert((0, r));
                                e.0 += 1;
                            }
                        }
                        let total: usize = counts.values().map(|(c, _)| c).sum();
                        if total == 0 {
                            continue;
                        }
                        let (n, raw) = counts
                            .values()
                            .max_by_key(|(c, _)| *c)
                            .copied()
                            .unwrap_or((0, 0.0));
                        (json!(raw), n as f64 / total as f64, n)
                    }
                    Acc::Cat { counts } => {
                        let total: u64 = counts.iter().sum();
                        if total == 0 {
                            continue;
                        }
                        let (bi, bc) = counts
                            .iter()
                            .enumerate()
                            .max_by_key(|(_, &c)| c)
                            .unwrap_or((0, &0));
                        (
                            json!(col.level_names.get(bi).cloned().unwrap_or_default()),
                            *bc as f64 / total as f64,
                            *bc as usize,
                        )
                    }
                };
            if modal_share >= 0.9 {
                let s = &summaries[k];
                modal_entries.push((
                    k,
                    s.rows_min,
                    s.rows_max,
                    g.len(),
                    modal_value,
                    modal_share,
                    modal_n,
                ));
            }
        }
        if !modal_entries.is_empty() {
            let mut m = base_metrics.clone();
            let claim = if modal_entries.len() == 1 {
                let (k, rmin, rmax, cn, mv, ms, mn) = &modal_entries[0];
                m.insert("cluster".to_string(), json!(k));
                m.insert("rows_min".to_string(), json!(rmin));
                m.insert("rows_max".to_string(), json!(rmax));
                m.insert("cluster_n".to_string(), json!(cn));
                m.insert("modal_value".to_string(), mv.clone());
                m.insert("modal_share".to_string(), json!(ms));
                format!(
                    "{} of {} rows in cluster {} (rows {}-{}) hold {} at {} ({:.0}% of the cluster; {}).",
                    mn, cn, k, rmin, rmax, col.name, mv, ms * 100.0, HONESTY
                )
            } else {
                let frags: Vec<String> = modal_entries
                    .iter()
                    .map(|(k, rmin, rmax, cn, mv, ms, mn)| {
                        format!(
                            "{} of {} rows in cluster {} (rows {}-{}) hold {} at {} ({:.0}% of the cluster)",
                            mn, cn, k, rmin, rmax, col.name, mv, ms * 100.0
                        )
                    })
                    .collect();
                m.insert(
                    "modal_clusters".to_string(),
                    json!(modal_entries
                        .iter()
                        .map(|(k, rmin, rmax, cn, mv, ms, _)| {
                            json!({
                                "cluster": k,
                                "rows_min": rmin,
                                "rows_max": rmax,
                                "cluster_n": cn,
                                "modal_value": mv,
                                "modal_share": ms,
                            })
                        })
                        .collect::<Vec<_>>()),
                );
                format!("{} ({}).", frags.join("; "), HONESTY)
            };
            out.push(Finding::from_statement(
                Category::Concentration,
                Some("modal_share".to_string()),
                Subject::columns([col.name.clone()]),
                Default::default(),
                claim,
                make_evidence(m),
                0.60,
                Some(HONESTY.to_string()),
            ));
        }
        // Spike cells: log predictive density below median - threshold.
        // Cells in small clusters (fewer than min_degenerate_rows rows)
        // are scored under the LARGEST cluster instead of their own:
        // a handful of identical spikes would otherwise form their own
        // cluster and look perfectly typical under it, making them
        // invisible to exactly the score meant to catch them.
        // First-wins on ties, matching the module's tie-break
        // convention (max_by_key would take the last tied cluster).
        let background = groups.iter().enumerate().fold(0, |best, (k, g)| {
            if g.len() > groups[best].len() {
                k
            } else {
                best
            }
        });
        let mut cells: Vec<(f64, usize, serde_json::Value)> = Vec::new();
        for (k, g) in groups.iter().enumerate() {
            let score_k = if g.len() < cfg.min_degenerate_rows && k != background {
                background
            } else {
                k
            };
            for &p in g {
                if let Some(x) = col.vals[p] {
                    // Leave-one-out when scoring under the cell's own
                    // cluster, so a cell cannot explain itself; cells
                    // scored under the background cluster are not in
                    // its statistics and need no adjustment.
                    let lp = if score_k == k {
                        let mut loo = stats[k][jj].clone();
                        loo.remove(Some(x));
                        loo.log_predictive(x)
                    } else {
                        stats[score_k][jj].log_predictive(x)
                    };
                    let value = match col.kind {
                        ColKind::Continuous => {
                            json!(col.raw[p])
                        }
                        ColKind::Categorical => {
                            let i = x as usize;
                            json!(col.level_names.get(i).cloned().unwrap_or_default())
                        }
                    };
                    cells.push((lp, sample[p], value));
                }
            }
        }
        if !cells.is_empty() {
            let mut lps: Vec<f64> = cells.iter().map(|c| c.0).collect();
            lps.sort_by(|a, b| a.total_cmp(b));
            let median = lps[(lps.len() - 1) / 2];
            let threshold = median - cfg.anomaly_threshold_nats;
            cells.sort_by(|a, b| a.0.total_cmp(&b.0));
            let flagged: Vec<(f64, usize, serde_json::Value)> = cells
                .into_iter()
                .filter(|c| c.0 < threshold)
                .take(cfg.anomaly_cap)
                .collect();
            if !flagged.is_empty() {
                let mut m = base_metrics.clone();
                m.insert("threshold".to_string(), json!(threshold));
                m.insert("median_logp".to_string(), json!(median));
                m.insert(
                    "cells".to_string(),
                    json!(flagged
                        .iter()
                        .map(|(lp, row, v)| json!({"row": row, "value": v, "logp": lp}))
                        .collect::<Vec<_>>()),
                );
                let lowest = &flagged[0];
                out.push(Finding::from_statement(
                    Category::Concentration,
                    Some("spike_cells".to_string()),
                    Subject::columns([col.name.clone()]),
                    Default::default(),
                    format!(
                        "{} cells in {} have log predictive density below {:.1} nats under their cluster (lowest {} at row {}; {}).",
                        flagged.len(),
                        col.name,
                        threshold,
                        lowest.2,
                        lowest.1,
                        HONESTY
                    ),
                    make_evidence(m),
                    0.50,
                    Some(HONESTY.to_string()),
                ));
            }
        }
    }
    out
}

/// Mean and count of a continuous column's standardized values in a row set.
fn cont_mean(col: &ViewCol, rows: &[usize]) -> (f64, usize) {
    let mut s = 0.0;
    let mut n = 0usize;
    for &r in rows {
        if let Some(x) = col.vals[r] {
            s += x;
            n += 1;
        }
    }
    if n > 0 {
        (s / n as f64, n)
    } else {
        (0.0, 0)
    }
}

/// Level distribution of a categorical accumulator.
fn cat_dist(acc: &Acc) -> Vec<f64> {
    match acc {
        Acc::Cat { counts } => {
            let total: u64 = counts.iter().sum();
            if total == 0 {
                return vec![0.0; counts.len()];
            }
            counts.iter().map(|&c| c as f64 / total as f64).collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::{NumericValues, TypedCol, TypedTable};

    #[test]
    fn ln_gamma_known_values() {
        assert!((ln_gamma(2.0) - 0.0).abs() < 1e-9);
        assert!((ln_gamma(3.5) - 1.200973602).abs() < 1e-9);
        assert!((ln_gamma(6.0) - 4.787491743).abs() < 1e-9);
    }

    #[test]
    fn nig_worked_example() {
        // Design note section 5: raw [19, 20, 21] under median/IQR
        // scaling gives standardized [-1, 0, 1]: mean 0, S = 2.
        let v = nig_log_marginal(3, 0.0, 2.0);
        assert!((v - (-4.67500)).abs() < 1e-4, "nig = {v}");
    }

    #[test]
    fn nig_predictive_is_marginal_ratio() {
        // Regression test for the halved-beta-exponent bug: the
        // posterior predictive must equal the marginal ratio, exactly.
        // Cases: a typical value, a far value (where the old formula
        // was off by tens of nats), and the empty-cluster prior.
        for (vals, x) in [
            (vec![-1.0, 0.0, 1.0], 0.5),
            (vec![0.0; 50], 5.0),
            (vec![], 0.7),
        ] {
            let (n, mean, m2) = {
                let mut n = 0usize;
                let mut mean = 0.0f64;
                let mut m2 = 0.0f64;
                for &v in &vals {
                    n += 1;
                    let d = v - mean;
                    mean += d / n as f64;
                    m2 += d * (v - mean);
                }
                (n, mean, m2)
            };
            let mut vals2 = vals.clone();
            vals2.push(x);
            let (n2, mean2, m2_2) = {
                let mut n = 0usize;
                let mut mean = 0.0f64;
                let mut m2 = 0.0f64;
                for &v in &vals2 {
                    n += 1;
                    let d = v - mean;
                    mean += d / n as f64;
                    m2 += d * (v - mean);
                }
                (n, mean, m2)
            };
            let ratio = nig_log_marginal(n2, mean2, m2_2) - nig_log_marginal(n, mean, m2);
            let pred = nig_log_predictive(x, n, mean, m2);
            assert!(
                (ratio - pred).abs() < 1e-9,
                "n = {n}, x = {x}: ratio = {ratio}, pred = {pred}"
            );
        }
    }

    #[test]
    fn dm_worked_example() {
        let v = dm_log_marginal(&[3, 1], 1.0);
        assert!((v - (-2.99573)).abs() < 1e-4, "dm = {v}");
    }

    fn cont_stats(vals: &[f64]) -> (usize, f64, f64) {
        let mut n = 0usize;
        let mut mean = 0.0f64;
        let mut m2 = 0.0f64;
        for &v in vals {
            n += 1;
            let d = v - mean;
            mean += d / n as f64;
            m2 += d * (v - mean);
        }
        (n, mean, m2)
    }

    #[test]
    fn acc_remove_matches_from_scratch() {
        let vals = vec![-1.0, 0.5, 2.0, -0.25];
        let mut full = Acc::Cont {
            n: 0,
            mean: 0.0,
            m2: 0.0,
        };
        for &v in &vals {
            full.add(Some(v));
        }
        for i in 0..vals.len() {
            let mut loo = full.clone();
            loo.remove(Some(vals[i]));
            let rest: Vec<f64> = vals
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, &v)| v)
                .collect();
            let (n, mean, m2) = cont_stats(&rest);
            match loo {
                Acc::Cont {
                    n: ln,
                    mean: lm,
                    m2: lm2,
                } => {
                    assert_eq!(ln, n);
                    assert!((lm - mean).abs() < 1e-12, "mean {lm} vs {mean}");
                    assert!((lm2 - m2).abs() < 1e-12, "m2 {lm2} vs {m2}");
                }
                _ => panic!("expected Cont"),
            }
        }
        // Removing the only value empties the accumulator.
        let mut single = Acc::Cont {
            n: 0,
            mean: 0.0,
            m2: 0.0,
        };
        single.add(Some(3.0));
        single.remove(Some(3.0));
        match single {
            Acc::Cont { n, mean, m2 } => {
                assert_eq!(n, 0);
                assert_eq!(mean, 0.0);
                assert_eq!(m2, 0.0);
            }
            _ => panic!("expected Cont"),
        }
        // Categorical: decrement, saturating at zero.
        let mut cat = Acc::Cat {
            counts: vec![3, 1, 2],
        };
        cat.remove(Some(1.0));
        match cat {
            Acc::Cat { counts } => assert_eq!(counts, vec![3, 0, 2]),
            _ => panic!("expected Cat"),
        }
    }

    #[test]
    fn assign_rows_is_leave_one_out() {
        // Cluster A {0, 0, 0, 1000} vs wide cluster B. Under
        // self-inclusive scoring the 1000 row sticks to A (-9.1 vs
        // about -22.8 under B); under leave-one-out it moves to B
        // (-52.0 vs -22.8). Only the flip is asserted: the 0 rows may
        // also move, which is correct LOO behavior for this fixture.
        let vals = vec![0.0, 0.0, 0.0, 1000.0, -100.0, -50.0, 0.0, 50.0, 100.0];
        let col = ViewCol {
            name: "x".to_string(),
            kind: ColKind::Continuous,
            vals: vals.iter().map(|&v| Some(v)).collect(),
            raw: Vec::new(),
            n_levels: 0,
            level_names: Vec::new(),
        };
        let cols = vec![col];
        let groups = vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7, 8]];
        let stats = stats_for(&cols, &[0], &groups);
        let a = assign_rows(&cols, &[0], &stats, 9, &groups);
        assert_eq!(a[3], 1, "outlier row must leave its own cluster");
    }

    #[test]
    fn encode_levels_other_collision() {
        // "other" is itself a kept level; the pooled tail must not
        // steal its id (kept id would become a zero-count ghost and
        // real "other" rows would remap to the tail).
        let mut strs: Vec<Option<String>> = Vec::new();
        for _ in 0..100 {
            strs.push(Some("other".to_string()));
        }
        for _ in 0..100 {
            strs.push(Some("a".to_string()));
        }
        for i in 0..40 {
            strs.push(Some(format!("rare{i:02}")));
        }
        let col = encode_levels("x".to_string(), strs, 32);
        // Kept: "a", "other", rare00..rare29; tail: rare30..rare39.
        assert_eq!(col.n_levels, 33);
        let other_id = col
            .level_names
            .iter()
            .position(|s| s == "other")
            .expect("real other level kept");
        let tail_id = col.n_levels - 1;
        assert_ne!(col.level_names[tail_id], "other");
        assert!(col.vals[0..100].iter().all(|v| *v == Some(other_id as f64)));
        assert!(col.vals[230..240]
            .iter()
            .all(|v| *v == Some(tail_id as f64)));
    }

    #[test]
    fn modal_counts_signed_zero_together() {
        // Mixed-sign zero values are one value for the modal test:
        // without canonicalization the 50/50 bit split gives share
        // 0.5 and no finding; with it, share 1.0.
        let vals: Vec<Option<f64>> = (0..100)
            .map(|i| Some(if i % 2 == 0 { 0.0 } else { -0.0 }))
            .collect();
        let col = ViewCol {
            name: "z".to_string(),
            kind: ColKind::Continuous,
            vals: vals.clone(),
            raw: vals.to_vec(),
            n_levels: 0,
            level_names: Vec::new(),
        };
        let cols = vec![col];
        let groups = vec![(0..100).collect::<Vec<usize>>()];
        let stats = stats_for(&cols, &[0], &groups);
        let table = TypedTable {
            n: 100,
            names: vec!["z".to_string()],
            dtypes: vec!["float64".to_string()],
            cols: vec![],
        };
        let view = ViewState {
            cols: vec![0],
            groups: groups.clone(),
            score: 0.0,
            merge_gain: 0.0,
        };
        let sample: Vec<usize> = (0..100).collect();
        let findings = emit_view_findings(
            &table,
            &cols,
            &view,
            &groups,
            &stats,
            &sample,
            100,
            0.0,
            &ViewsConfig::default(),
            &[],
        );
        assert!(
            findings
                .iter()
                .any(|f| f.category == Category::Concentration
                    && f.finding_type.as_deref() == Some("modal_share")),
            "expected a modal-share finding, got: {}",
            serde_json::to_string(&findings).unwrap()
        );
    }

    #[test]
    fn predictive_worked_values() {
        // NIG cluster from section 5: n = 3, mean 0, m2 = 2.
        let typical = nig_log_predictive(0.0, 3, 0.0, 2.0);
        assert!((typical - (-0.7863)).abs() < 1e-3, "typical = {typical}");
        let spike = nig_log_predictive(1200.0, 3, 0.0, 2.0);
        assert!((spike - (-51.07)).abs() < 0.05, "spike = {spike}");
    }

    fn two_regime_col() -> ViewCol {
        // 100 rows at -1, 100 rows at +1 (already standardized scale).
        let mut vals = Vec::new();
        for _ in 0..100 {
            vals.push(Some(-1.0));
        }
        for _ in 0..100 {
            vals.push(Some(1.0));
        }
        ViewCol {
            name: "x".to_string(),
            kind: ColKind::Continuous,
            vals,
            raw: Vec::new(),
            n_levels: 0,
            level_names: Vec::new(),
        }
    }

    #[test]
    fn phase_b_splits_two_regimes_exactly() {
        let col = two_regime_col();
        let cols = vec![col];
        let search: Vec<usize> = (0..200).collect();
        let mut work = WorkBudget {
            used: 0,
            cap: 10_000_000,
            hit: false,
        };
        let cfg = ViewsConfig::default();
        let (groups, _) = phase_b(&cols, &[0], &search, &cfg, &mut work);
        assert_eq!(groups.len(), 2);
        let mut g0 = groups[0].clone();
        let mut g1 = groups[1].clone();
        g0.sort_unstable();
        g1.sort_unstable();
        let low: Vec<usize> = (0..100).collect();
        let high: Vec<usize> = (100..200).collect();
        assert!(
            (g0 == low && g1 == high) || (g0 == high && g1 == low),
            "groups = {groups:?}"
        );
    }

    fn synthetic_table() -> TypedTable<'static> {
        // temp: 100 rows near 20, 100 rows near 32 (regime shift, no spikes);
        // noise: deterministic pseudo-random independent values.
        let mut temp = Vec::new();
        let mut noise = Vec::new();
        let mut s = 0x9E3779B97F4A7C15u64;
        let mut next = || {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
        };
        for i in 0..200 {
            if i < 100 {
                temp.push(Some(20.0 + (i % 5) as f64 * 0.1));
            } else {
                temp.push(Some(32.0 + (i % 5) as f64 * 0.1));
            }
            noise.push(Some(next() * 10.0));
        }
        TypedTable {
            n: 200,
            names: vec!["temp".to_string(), "noise".to_string()],
            dtypes: vec!["float64".to_string(), "float64".to_string()],
            cols: vec![
                TypedCol::F64 {
                    vals: NumericValues::Owned(temp),
                },
                TypedCol::F64 {
                    vals: NumericValues::Owned(noise),
                },
            ],
        }
    }

    #[test]
    fn cap_prescreen_keeps_dependent_columns_past_the_old_cutoff() {
        // 70 eligible columns, cap 64. The two columns past the old
        // table-order cutoff are identical regime columns (maximally
        // dependent); the other 68 are independent noise. Table-order
        // capping dropped the pair by construction; the
        // dependence pre-screen must keep them and drop noise
        // columns instead.
        let mut s = 0x9E3779B97F4A7C15u64;
        let mut next = move || {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
        };
        let n = 400usize;
        let mut names: Vec<String> = Vec::new();
        let mut cols: Vec<TypedCol> = Vec::new();
        for i in 0..68 {
            names.push(format!("noise_{i}"));
            let vals: Vec<Option<f64>> = (0..n).map(|_| Some(next() * 10.0)).collect();
            cols.push(TypedCol::F64 {
                vals: NumericValues::Owned(vals),
            });
        }
        let regime: Vec<Option<f64>> = (0..n)
            .map(|i| {
                let base = if i < n / 2 { 0.0 } else { 10.0 };
                Some(base + next() * 0.01)
            })
            .collect();
        for name in ["signal_a", "signal_b"] {
            names.push(name.to_string());
            cols.push(TypedCol::F64 {
                vals: NumericValues::Owned(regime.clone()),
            });
        }
        let table = TypedTable {
            n,
            names,
            dtypes: vec!["float64".to_string(); 70],
            cols,
        };
        let cfg = ViewsConfig {
            row_cap: 1000,
            search_rows: 400,
            ..Default::default()
        };
        let (findings, outcome) = views_pass(&table, &cfg);
        assert_eq!(
            outcome.dropped_columns.len(),
            6,
            "dropped = {:?}",
            outcome.dropped_columns
        );
        assert!(!outcome.dropped_columns.contains(&"signal_a".to_string()));
        assert!(!outcome.dropped_columns.contains(&"signal_b".to_string()));
        assert!(
            findings.iter().any(|f| f.category == Category::Associations
                && f.subject.columns.contains(&"signal_a".to_string())
                && f.subject.columns.contains(&"signal_b".to_string())),
            "the kept pair must merge into one view"
        );
    }

    #[test]
    fn cap_prescreen_drops_identifier_before_dependent_columns() {
        // 66 eligible columns at Animals scale (60 rows), cap 64.
        // A unique string identifier can memorize every other
        // column on a sample this size, so raw dependence ranks it
        // first; the screen must give it no ranking signal and drop
        // it ahead of the genuinely dependent pair at the end.
        let mut s = 0x2545F4914F6CDD1Du64;
        let mut next = move || {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            (s.wrapping_mul(0x9E3779B97F4A7C15) >> 11) as f64 / (1u64 << 53) as f64
        };
        let n = 60usize;
        let mut names: Vec<String> = Vec::new();
        let mut dtypes: Vec<String> = Vec::new();
        let mut cols: Vec<TypedCol> = Vec::new();
        for i in 0..63 {
            names.push(format!("noise_{i}"));
            dtypes.push("float64".to_string());
            let vals: Vec<Option<f64>> = (0..n).map(|_| Some(next() * 10.0)).collect();
            cols.push(TypedCol::F64 {
                vals: NumericValues::Owned(vals),
            });
        }
        names.push("row_id".to_string());
        dtypes.push("string".to_string());
        cols.push(TypedCol::StrOwned {
            vals: (0..n).map(|i| Some(format!("row_{i:03}"))).collect(),
        });
        let regime: Vec<Option<f64>> = (0..n)
            .map(|i| {
                let base = if i < n / 2 { 0.0 } else { 10.0 };
                Some(base + next() * 0.01)
            })
            .collect();
        for name in ["signal_a", "signal_b"] {
            names.push(name.to_string());
            dtypes.push("float64".to_string());
            cols.push(TypedCol::F64 {
                vals: NumericValues::Owned(regime.clone()),
            });
        }
        let table = TypedTable {
            n,
            names,
            dtypes,
            cols,
        };
        let cfg = ViewsConfig {
            row_cap: 1000,
            search_rows: 200,
            ..Default::default()
        };
        let (_, outcome) = views_pass(&table, &cfg);
        assert_eq!(
            outcome.dropped_columns.len(),
            2,
            "dropped = {:?}",
            outcome.dropped_columns
        );
        assert!(
            outcome.dropped_columns.contains(&"row_id".to_string()),
            "identifier must be dropped: {:?}",
            outcome.dropped_columns
        );
        assert!(!outcome.dropped_columns.contains(&"signal_a".to_string()));
        assert!(!outcome.dropped_columns.contains(&"signal_b".to_string()));
    }

    #[test]
    fn views_pass_deterministic_and_finds_regime() {
        let table = synthetic_table();
        let cfg = ViewsConfig {
            row_cap: 1000,
            search_rows: 200,
            ..Default::default()
        };
        let (f1, _) = views_pass(&table, &cfg);
        let (f2, _) = views_pass(&table, &cfg);
        let j1 = serde_json::to_string(&f1).unwrap();
        let j2 = serde_json::to_string(&f2).unwrap();
        assert_eq!(j1, j2, "views pass must be deterministic");
        // A regime (changes) finding on temp must exist.
        assert!(
            f1.iter().any(|f| f.category == Category::Changes
                && f.subject.columns.contains(&"temp".to_string())),
            "findings = {}",
            j1
        );
        // temp and noise are independent: no multi-column view finding.
        assert!(
            !f1.iter().any(|f| f.category == Category::Associations),
            "unexpected view finding: {}",
            j1
        );
    }

    #[test]
    fn phase_b_splits_binary_column_with_majority_upper_value() {
        // Regression: the old median-value split degenerated when the
        // median equaled the cluster maximum (left side = every row,
        // right side empty), so a binary column with a majority upper
        // value never split as a singleton while a pair view could
        // split around it — manufacturing a spurious merge. The
        // distinct-value-boundary rule must split this into 2 groups.
        let mut vals = Vec::new();
        for _ in 0..190 {
            vals.push(Some(0.0));
        }
        for _ in 0..210 {
            vals.push(Some(1.0));
        }
        let cols = vec![ViewCol {
            name: "b".to_string(),
            kind: ColKind::Continuous,
            vals,
            raw: Vec::new(),
            n_levels: 0,
            level_names: Vec::new(),
        }];
        let search: Vec<usize> = (0..400).collect();
        let mut work = WorkBudget {
            used: 0,
            cap: 10_000_000,
            hit: false,
        };
        let cfg = ViewsConfig::default();
        let (groups, _) = phase_b(&cols, &[0], &search, &cfg, &mut work);
        assert_eq!(groups.len(), 2);
        let mut sizes: Vec<usize> = groups.iter().map(|g| g.len()).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, vec![190, 210]);
    }

    #[test]
    fn views_pass_does_not_merge_independent_binary_columns() {
        // Two uncorrelated binary columns (each of the four value
        // combinations appears exactly 100 times) must not form a view.
        let mut a = Vec::new();
        let mut b = Vec::new();
        for i in 0..400 {
            a.push(Some((i % 2) as i64));
            b.push(Some(((i / 2) % 2) as i64));
        }
        let table = TypedTable {
            n: 400,
            names: vec!["a".to_string(), "b".to_string()],
            dtypes: vec!["int64".to_string(), "int64".to_string()],
            cols: vec![
                TypedCol::I64 {
                    vals: NumericValues::Owned(a),
                },
                TypedCol::I64 {
                    vals: NumericValues::Owned(b),
                },
            ],
        };
        let cfg = ViewsConfig {
            row_cap: 1000,
            search_rows: 400,
            ..Default::default()
        };
        let (findings, _) = views_pass(&table, &cfg);
        assert!(
            !findings
                .iter()
                .any(|f| f.category == Category::Associations),
            "spurious view: {}",
            serde_json::to_string(&findings).unwrap()
        );
    }

    #[test]
    fn views_pass_does_not_merge_constant_columns() {
        // Two constant columns have single-cluster singletons and a
        // single-cluster joint view: merging them would be pure
        // penalty savings (2 * gamma * ln n) with no partition
        // evidence, so no dependence block may be reported.
        let table = TypedTable {
            n: 500,
            names: vec!["c1".to_string(), "c2".to_string()],
            dtypes: vec!["float64".to_string(), "float64".to_string()],
            cols: vec![
                TypedCol::F64 {
                    vals: NumericValues::Owned(vec![Some(7.0); 500]),
                },
                TypedCol::F64 {
                    vals: NumericValues::Owned(vec![Some(3.0); 500]),
                },
            ],
        };
        let cfg = ViewsConfig {
            row_cap: 1000,
            search_rows: 500,
            ..Default::default()
        };
        let (findings, _) = views_pass(&table, &cfg);
        assert!(
            !findings
                .iter()
                .any(|f| f.category == Category::Associations),
            "constant columns merged: {}",
            serde_json::to_string(&findings).unwrap()
        );
    }

    #[test]
    fn views_pass_flags_spikes_and_constant_block() {
        // temp: 60 rows constant 21.5, 37 rows varied, 3 spikes at 999.9.
        let mut temp = Vec::new();
        for _ in 0..60 {
            temp.push(Some(21.5));
        }
        for i in 0..37 {
            temp.push(Some(20.0 + (i % 7) as f64 * 0.3));
        }
        for _ in 0..3 {
            temp.push(Some(999.9));
        }
        let table = TypedTable {
            n: 100,
            names: vec!["temp".to_string()],
            dtypes: vec!["float64".to_string()],
            cols: vec![TypedCol::F64 {
                vals: NumericValues::Owned(temp),
            }],
        };
        let cfg = ViewsConfig {
            row_cap: 1000,
            search_rows: 100,
            ..Default::default()
        };
        let (findings, _) = views_pass(&table, &cfg);
        let j = serde_json::to_string(&findings).unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f.category == Category::Concentration
                    && f.metrics().get("modal_value").and_then(|v| v.as_f64()) == Some(21.5)
                    && f.metrics()
                        .get("modal_share")
                        .and_then(|v| v.as_f64())
                        .is_some_and(|s| s >= 0.9)),
            "no degenerate finding: {}",
            j
        );
        assert!(
            findings
                .iter()
                .any(|f| f.category == Category::Concentration && f.claim.contains("999.9")),
            "no spike finding: {}",
            j
        );
    }
}
