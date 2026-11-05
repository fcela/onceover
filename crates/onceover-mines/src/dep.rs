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
    /// Per-pair p-value for the association test: the smallest
    /// component p-value (Pearson, Spearman, ξ in both directions),
    /// Bonferroni-multiplied by the number of components computed,
    /// so it tests "any component departs from independence" at the
    /// stated level (issue #134). None when no component was testable.
    pub p_value: Option<f64>,
    /// Benjamini–Hochberg adjusted p-value over every pair tested
    /// in the same screen run; filled in by the screen, not by the
    /// per-pair computation.
    pub q_value: Option<f64>,
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

// ---------------------------------------------------------------------
// Screening statistics (issue #134): distribution functions and
// multiple-testing control for the tested association screens. The
// implementations are the standard Numerical Recipes series /
// continued-fraction evaluations, accurate to ~1e-14 in the ranges
// a screen produces; the unit tests pin them against known values
// and the Python acceptance harness spot-checks the p-values the
// library emits against SciPy.

/// The Benjamini–Hochberg false-discovery bar for the reporting
/// tiers: a pair is stated (strong or weak) when its q-value is at
/// most this.
pub(crate) const Q_BAR: f64 = 0.05;

/// The reporting tier of a tested pair: "strong" when it is
/// significant after correction and its effect reaches the strong
/// bar, "weak" when significant but below the bar, "none" otherwise
/// (including untested pairs).
pub(crate) fn assoc_tier(effect: f64, strong_bar: f64, q: Option<f64>) -> &'static str {
    match q {
        Some(q) if q <= Q_BAR => {
            if effect >= strong_bar {
                "strong"
            } else {
                "weak"
            }
        }
        _ => "none",
    }
}

/// Benjamini–Hochberg adjusted p-values (q-values) for one tested
/// family, returned in input order. Untested entries (None) stay
/// None and do not count toward the family size.
pub(crate) fn bh_qvalues(pvals: &[Option<f64>]) -> Vec<Option<f64>> {
    let mut idx: Vec<usize> = (0..pvals.len()).filter(|&i| pvals[i].is_some()).collect();
    idx.sort_by(|&a, &b| pvals[a].unwrap().total_cmp(&pvals[b].unwrap()));
    let m = idx.len() as f64;
    let mut out: Vec<Option<f64>> = vec![None; pvals.len()];
    let mut running = 1.0f64;
    for (rank, &i) in idx.iter().enumerate().rev() {
        let r = (rank + 1) as f64;
        running = running.min(pvals[i].unwrap() * m / r);
        out[i] = Some(running.min(1.0));
    }
    out
}

/// Lanczos approximation of ln Γ(x) for x > 0.
fn ln_gamma(x: f64) -> f64 {
    const COEF: [f64; 6] = [
        76.18009172947146,
        -86.50532032941677,
        24.01409824083091,
        -1.231739572450155,
        0.001208650973866179,
        -0.000005395239384953,
    ];
    let mut y = x;
    let mut tmp = x + 5.5;
    tmp -= (x + 0.5) * tmp.ln();
    let mut ser = 1.000000000190015;
    for c in COEF {
        y += 1.0;
        ser += c / y;
    }
    -tmp + (2.5066282746310005 * ser / x).ln()
}

/// Regularized lower incomplete gamma P(a, x): series below
/// x = a + 1, continued fraction above (via Q).
pub(crate) fn gamma_p(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= a + 1.0 {
        return 1.0 - gamma_q(a, x);
    }
    let mut ap = a;
    let mut sum = 1.0 / a;
    let mut del = sum;
    for _ in 0..1000 {
        ap += 1.0;
        del *= x / ap;
        sum += del;
        if del.abs() < sum.abs() * 1e-15 {
            break;
        }
    }
    (sum * (-x + a * x.ln() - ln_gamma(a)).exp()).clamp(0.0, 1.0)
}

/// Regularized upper incomplete gamma Q(a, x) = 1 − P(a, x).
pub(crate) fn gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    if x < a + 1.0 {
        return 1.0 - gamma_p(a, x);
    }
    let tiny = 1e-300;
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / tiny;
    let mut d = 1.0 / b.max(tiny);
    let mut h = d;
    for i in 1..1000 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < tiny {
            d = tiny;
        }
        c = b + an / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-15 {
            break;
        }
    }
    ((-x + a * x.ln() - ln_gamma(a)).exp() * h).clamp(0.0, 1.0)
}

/// Regularized incomplete beta I_x(a, b).
pub(crate) fn beta_inc(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let bt = (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        (bt * beta_cf(x, a, b) / a).clamp(0.0, 1.0)
    } else {
        (1.0 - bt * beta_cf(1.0 - x, b, a) / b).clamp(0.0, 1.0)
    }
}

fn beta_cf(x: f64, a: f64, b: f64) -> f64 {
    let tiny = 1e-300;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < tiny {
        d = tiny;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=200 {
        let m = m as f64;
        let m2 = 2.0 * m;
        let mut aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        h *= d * c;
        aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 3e-16 {
            break;
        }
    }
    h
}

/// Upper tail of the standard normal, P(Z > z), via
/// erfc(z/√2)/2 = Q(1/2, z²/2)/2.
pub(crate) fn norm_sf(z: f64) -> f64 {
    let q = 0.5 * gamma_q(0.5, z * z / 2.0);
    if z >= 0.0 {
        q
    } else {
        1.0 - q
    }
}

/// Two-sided p-value for a correlation coefficient at n pairs,
/// from t = r·√((n−2)/(1−r²)) on n−2 df:
/// P(|T| ≥ |t|) = I_{df/(df+t²)}(df/2, 1/2). Used for Pearson
/// directly and for Spearman through the same t approximation on
/// ranks.
pub(crate) fn corr_p_value(r: f64, n: usize) -> Option<f64> {
    if n < 4 || !r.is_finite() {
        return None;
    }
    let r = r.clamp(-1.0, 1.0);
    if r.abs() >= 1.0 {
        return Some(0.0);
    }
    let df = (n - 2) as f64;
    let t2 = r * r * df / (1.0 - r * r);
    Some(beta_inc(df / (df + t2), df / 2.0, 0.5))
}

/// Upper tail of the chi-square distribution.
pub(crate) fn chi2_sf(x: f64, df: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    gamma_q(df / 2.0, x / 2.0)
}

/// Upper tail of the F distribution.
pub(crate) fn f_sf(f: f64, d1: f64, d2: f64) -> f64 {
    if f <= 0.0 {
        return 1.0;
    }
    beta_inc(d2 / (d2 + d1 * f), d2 / 2.0, d1 / 2.0)
}

/// Format a q-value for a claim: exact to three decimals at or
/// above 0.001, a true bound below it (the payload always carries
/// the precise value).
pub(crate) fn fmt_q(q: f64) -> String {
    if q < 0.001 {
        "<0.001".to_string()
    } else {
        format!("{q:.3}")
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
///
/// The parts form also returns the max-rank multiset and the
/// denominator, which the permutation calibration of the test
/// consumes: both are invariant under permutation of the y order.
fn chatterjee_parts(pairs: &[(f64, f64)]) -> Option<(f64, Vec<f64>, f64)> {
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
    let mut rs: Vec<f64> = Vec::with_capacity(n);
    for (i, &y) in ys.iter().enumerate() {
        let r = sorted_y.partition_point(|&v| v <= y) as f64; // #{<= y}
        let less = sorted_y.partition_point(|&v| v < y); // #{< y}
        let l = (n - less) as f64; // #{>= y}
        den += 2.0 * l * (n as f64 - l);
        if i > 0 {
            num += (r - prev_r).abs();
        }
        prev_r = r;
        rs.push(r);
    }
    if den <= 0.0 {
        return None;
    }
    Some((1.0 - (n as f64) * num / den, rs, den))
}

pub fn chatterjee_xi(pairs: &[(f64, f64)]) -> Option<f64> {
    chatterjee_parts(pairs).map(|(xi, _, _)| xi)
}

/// Mean and variance of S = Σ|r_{i+1} − r_i| when the rank sequence
/// is a uniform random permutation of the fixed multiset `rs` —
/// the permutation calibration of Chatterjee's ξ under
/// independence, in closed form (issue #134). With
/// P = Σ_{j≠k}|r_j − r_k|, s2 = Σ_{j≠k}(r_j − r_k)², and
/// s3 = Σ_k (A_k² − B_k) for A_k = Σ_{j≠k}|r_k − r_j| and
/// B_k = Σ_{j≠k}(r_k − r_j)²: E[S] = P/n, and Var(S) combines the
/// per-term, adjacent-term (sharing one index), and disjoint-term
/// second moments of the permutation distribution.
fn perm_moments_s(rs: &[f64]) -> (f64, f64) {
    let n = rs.len() as f64;
    let sum_r: f64 = rs.iter().sum();
    let sum_r2: f64 = rs.iter().map(|v| v * v).sum();
    // Group the sorted ranks: A(v) and B(v) are shared by every
    // position whose rank equals v.
    let mut sorted = rs.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mut p = 0.0f64;
    let mut s3 = 0.0f64;
    let mut i = 0usize;
    let mut cnt_less = 0.0f64;
    let mut sum_less = 0.0f64;
    while i < sorted.len() {
        let mut j = i;
        while j + 1 < sorted.len() && sorted[j + 1] == sorted[i] {
            j += 1;
        }
        let v = sorted[i];
        let mult = (j - i + 1) as f64;
        let cnt_greater = n - cnt_less - mult;
        let sum_greater = sum_r - sum_less - mult * v;
        let a_v = v * cnt_less - sum_less + sum_greater - v * cnt_greater;
        let b_v = (n - 1.0) * v * v - 2.0 * v * (sum_r - v) + (sum_r2 - v * v);
        p += mult * a_v;
        s3 += mult * (a_v * a_v - b_v);
        cnt_less += mult;
        sum_less += mult * v;
        i = j + 1;
    }
    let s2 = 2.0 * n * sum_r2 - 2.0 * sum_r * sum_r;
    let mu = p / (n * (n - 1.0));
    let e_s = (n - 1.0) * mu;
    let e_d2 = s2 / (n * (n - 1.0));
    let mut var = (n - 1.0) * (e_d2 - mu * mu);
    if n >= 3.0 {
        let e_adj = s3 / (n * (n - 1.0) * (n - 2.0));
        var += 2.0 * (n - 2.0) * (e_adj - mu * mu);
    }
    if n >= 4.0 {
        let e_dis =
            ((p * p - 4.0 * s3 - 2.0 * s2) / (n * (n - 1.0) * (n - 2.0) * (n - 3.0))).max(0.0);
        var += (n - 2.0) * (n - 3.0) * (e_dis - mu * mu);
    }
    (e_s, var.max(0.0))
}

/// Chatterjee's ξ with a permutation-calibrated p-value: under
/// independence the y order is a uniform permutation of the fixed
/// rank multiset, whose first two moments `perm_moments_s` gives
/// exactly; the p-value is the normal upper tail at the observed
/// ξ under that permutation mean and standard deviation
/// (one-sided: large ξ departs from independence). The p-value is
/// None when the permutation variance degenerates.
pub fn chatterjee_xi_tested(pairs: &[(f64, f64)]) -> Option<(f64, Option<f64>)> {
    let (xi, rs, den) = chatterjee_parts(pairs)?;
    let n = pairs.len() as f64;
    let (e_s, var_s) = perm_moments_s(&rs);
    let sd = n * var_s.sqrt() / den;
    let p = if sd > 0.0 && sd.is_finite() {
        let e_xi = 1.0 - n * e_s / den;
        Some(norm_sf((xi - e_xi) / sd).clamp(0.0, 1.0))
    } else {
        None
    };
    Some((xi, p))
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

/// Screen all numeric pairs (rayon-parallel, deterministic order).
pub fn screen_sampled(
    table: &onceover_core::TypedTable,
    pair_cap: usize,
    row_cap: usize,
    excluded: &[usize],
) -> (
    Vec<PairDep>,
    Vec<Finding>,
    String,
    Vec<onceover_report::AssociationPair>,
) {
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
        return (
            Vec::new(),
            Vec::new(),
            format!(
                "0 of {} numeric pairs (width-gated); pair screening skipped: {} pairs exceeds the {}-pair gate for the all-pairs stage",
                total_pairs, total_pairs, PAIR_GATE
            ),
            Vec::new(),
        );
    }
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
            while start < total_pairs {
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
        let pearson_r = pearson(&pairs);
        let spearman_r = spearman(&pairs);
        let (xi_ab, p_xi_ab) = chatterjee_xi_tested(&pairs)
            .map(|(x, p)| (Some(x), p))
            .unwrap_or((None, None));
        let (xi_ba, p_xi_ba) = {
            let flipped: Vec<(f64, f64)> = pairs.iter().map(|(x, y)| (*y, *x)).collect();
            chatterjee_xi_tested(&flipped)
                .map(|(x, p)| (Some(x), p))
                .unwrap_or((None, None))
        };
        // One p-value per pair: the smallest component p-value,
        // Bonferroni-multiplied by the number of components
        // computed, so the pair test controls "any component
        // departs from independence" at its stated level.
        let mut components: Vec<f64> = Vec::new();
        if let Some(p) = pearson_r.and_then(|r| corr_p_value(r, pairs.len())) {
            components.push(p);
        }
        if let Some(p) = spearman_r.and_then(|r| corr_p_value(r, pairs.len())) {
            components.push(p);
        }
        if let Some(p) = p_xi_ab {
            components.push(p);
        }
        if let Some(p) = p_xi_ba {
            components.push(p);
        }
        let p_value = if components.is_empty() {
            None
        } else {
            let min_p = components.iter().fold(1.0f64, |m, &p| m.min(p));
            Some((min_p * components.len() as f64).min(1.0))
        };
        PairDep {
            a,
            b,
            pearson: pearson_r,
            spearman: spearman_r,
            xi_ab,
            xi_ba,
            n: pairs.len(),
            population_rows: table.n,
            sampled: rows.len() < table.n,
            p_value,
            q_value: None,
        }
    };
    // Capped: detailed statistics cover the survivor list only
    // (bounded by the pair cap). Uncapped: the whole universe,
    // enumerated in ordinal chunks so the pair list is never
    // materialized; chunks concatenate in pair order, so the
    // result is identical to screening a materialized list.
    let mut deps: Vec<PairDep> = if capped {
        pair_list
            .par_iter()
            .map(|&(a, b)| dep_for_pair(a, b))
            .collect()
    } else {
        let mut deps: Vec<PairDep> = Vec::with_capacity(total_pairs);
        let mut start = 0usize;
        while start < total_pairs {
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
        deps
    };

    // Benjamini–Hochberg over every pair tested in this run
    // (issue #134): in the capped path the tested family is the
    // survivor list, not the screened universe — coverage states
    // the family size, and the associations table carries the
    // q-values so a reader can re-derive every tier.
    let qvalues = bh_qvalues(&deps.iter().map(|d| d.p_value).collect::<Vec<_>>());
    let tested = qvalues.iter().filter(|q| q.is_some()).count();
    for (d, q) in deps.iter_mut().zip(qvalues) {
        d.q_value = q;
    }

    // Findings: tested pairs only (issue #134) — strong tier at
    // or above the 0.5 effect bar, weak tier significant after
    // correction but below it — and never a pair involving a
    // frame-role scaffolding column.
    let mut dep_findings: Vec<(usize, &'static str, Finding)> = deps
        .iter()
        .enumerate()
        .filter(|(_, d)| !excluded.contains(&d.a) && !excluded.contains(&d.b))
        .filter(|(_, d)| d.n >= 30)
        .filter_map(|(idx, d)| {
            let s = d.max_signal();
            let tier = assoc_tier(s, 0.5, d.q_value);
            (tier != "none").then(|| (idx, tier, dep_finding(table, d, s, tier)))
        })
        .collect();
    dep_findings.sort_by(|a, b| {
        b.2.interestingness
            .total_cmp(&a.2.interestingness)
            .then(a.0.cmp(&b.0))
    });
    dep_findings.truncate(20);
    let n_weak = dep_findings.iter().filter(|(_, t, _)| *t == "weak").count();
    let findings: Vec<Finding> = dep_findings.into_iter().map(|(_, _, f)| f).collect();

    // The associations table: every pair that received detailed
    // statistics, with its test results and tier (issue #134).
    let records: Vec<onceover_report::AssociationPair> = deps
        .iter()
        .map(|d| onceover_report::AssociationPair {
            a: table.names[d.a].clone(),
            b: table.names[d.b].clone(),
            kind: "numeric".to_string(),
            n: d.n,
            pearson: d.pearson,
            spearman: d.spearman,
            xi_ab: d.xi_ab,
            xi_ba: d.xi_ba,
            cramers_v: None,
            eta: None,
            p_value: d.p_value,
            q_value: d.q_value,
            tier: assoc_tier(d.max_signal(), 0.5, d.q_value).to_string(),
        })
        .collect();

    let n_links = deps.iter().filter(|d| d.max_signal() >= 0.98).count();
    let mut coverage = format!(
        "{} of {} numeric pairs ({}); all {} pairs screened; detailed statistics over {} of {} rows; {} strong pairwise, {} weak pairwise (Benjamini-Hochberg q <= {} over {} tested pairs), {} redundancy links",
        deps.len(),
        total_pairs,
        if capped { "capped" } else { "full" },
        total_pairs, rows.len(), table.n,
        findings.len() - n_weak,
        n_weak,
        Q_BAR,
        tested,
        n_links,
    );
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
    if !excluded_numeric.is_empty() {
        coverage.push_str(&format!(
            "; dependence findings involving {} column(s) withheld by frame role: {}",
            excluded_numeric.len(),
            excluded_numeric.join(", ")
        ));
    }

    (deps, findings, coverage, records)
}

fn dep_finding(table: &onceover_core::TypedTable, d: &PairDep, signal: f64, tier: &str) -> Finding {
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
        ("p_value".into(), json!(d.p_value)),
        ("q_value".into(), json!(d.q_value)),
        ("tier".into(), json!(tier)),
    ]);
    let (claim, interestingness) = if tier == "weak" {
        (
            format!(
                "{a} and {b} show weak numeric association (maximum measured effect {signal:.3}; significant after Benjamini-Hochberg correction, q = {})",
                fmt_q(d.q_value.unwrap_or(1.0))
            ),
            0.10 + 0.25 * signal,
        )
    } else {
        (
            format!("{a} and {b} show numeric association (maximum measured effect {signal:.3})"),
            0.35 + 0.4 * signal,
        )
    };
    evidence.insert("variables".into(), json!([a, b]));
    evidence.insert(
        "method".into(),
        json!("pearson+spearman+chatterjee_xi_tested"),
    );
    Finding::new(
Pattern::StatisticalAssociation,
BTreeMap::from([("x".into(), json!(a)), ("y".into(), json!(b))]),
claim,
EvidenceBasis::Estimated,
interestingness,
evidence,
None,
Method::new("numeric_dependence", "pearson_spearman_chatterjee", "Pearson/Spearman/Chatterjee ξ on disclosed row sample; per-pair tests (t approximation for the correlations, permutation-calibrated for ξ) with Benjamini–Hochberg correction over the run's tested pairs; pair selection by binned mutual information")
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
    fn distribution_functions_match_known_values() {
        // Incomplete beta: I_0.5(0.5, 0.5) = 0.5; I_x(1, 1) = x.
        assert!((beta_inc(0.5, 0.5, 0.5) - 0.5).abs() < 1e-12);
        assert!((beta_inc(0.3, 1.0, 1.0) - 0.3).abs() < 1e-12);
        // Chi-square upper tails at the textbook 5% points.
        assert!((chi2_sf(3.8414588, 1.0) - 0.05).abs() < 1e-6);
        assert!((chi2_sf(5.9914645, 2.0) - 0.05).abs() < 1e-6);
        assert!((chi2_sf(18.307038, 10.0) - 0.05).abs() < 1e-6);
        // Normal upper tail.
        assert!((norm_sf(0.0) - 0.5).abs() < 1e-12);
        assert!((norm_sf(1.959964) - 0.025).abs() < 1e-6);
        assert!((norm_sf(-1.959964) - 0.975).abs() < 1e-6);
        // Correlation p-values: r = 0.5 at n = 30 is the classic
        // t = 3.056 on 28 df, two-sided p = 0.0049.
        let p = corr_p_value(0.5, 30).unwrap();
        assert!((p - 0.00486).abs() < 0.0005, "p = {p}");
        // r = 0.2 at n = 5000: t = 14.43, p far below 1e-40.
        let p = corr_p_value(0.2, 5000).unwrap();
        assert!(p < 1e-40 && p > 0.0, "p = {p}");
        assert_eq!(corr_p_value(1.0, 100), Some(0.0));
        assert_eq!(corr_p_value(0.5, 3), None);
        // F upper tail: F(1, 10) at 4.9646 is the 5% point.
        assert!((f_sf(4.964603, 1.0, 10.0) - 0.05).abs() < 1e-5);
    }

    #[test]
    fn bh_qvalues_are_monotone_step_up() {
        let p = vec![Some(0.001), Some(0.01), Some(0.04), Some(0.20), None];
        let q = bh_qvalues(&p);
        // m = 4 (the None does not count): q = p * 4 / rank,
        // enforced non-decreasing from the largest p down.
        assert!((q[0].unwrap() - 0.004).abs() < 1e-12);
        assert!((q[1].unwrap() - 0.02).abs() < 1e-12);
        assert!((q[2].unwrap() - 0.04 * 4.0 / 3.0).abs() < 1e-12);
        assert!((q[3].unwrap() - 0.20).abs() < 1e-12);
        assert_eq!(q[4], None);
        // Step-up monotonicity: a small p after a larger one
        // inherits the larger adjusted value.
        let q2 = bh_qvalues(&[Some(0.04), Some(0.001)]);
        assert!(q2[0].unwrap() >= q2[1].unwrap());
        assert!((q2[1].unwrap() - 0.002).abs() < 1e-12);
    }

    #[test]
    fn xi_permutation_moments_match_brute_force() {
        // The closed-form permutation mean/variance of S must
        // match a direct simulation over random permutations of
        // the same rank multiset, with and without ties.
        let mut s = 0x2545F4914F6CDD1Du64;
        let mut next_u = move || {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
        };
        for tied in [false, true] {
            let n = 60usize;
            let rs: Vec<f64> = (0..n)
                .map(|i| {
                    if tied {
                        ((i % 7) + 1) as f64 * 8.0 // heavy ties, max-rank style
                    } else {
                        (i + 1) as f64
                    }
                })
                .collect();
            let (e_s, var_s) = perm_moments_s(&rs);
            let reps = 40_000usize;
            let mut sum = 0.0f64;
            let mut sum2 = 0.0f64;
            let mut perm: Vec<f64> = rs.clone();
            for _ in 0..reps {
                // Fisher–Yates with the local generator.
                for i in (1..n).rev() {
                    let j = (next_u() * (i + 1) as f64) as usize;
                    perm.swap(i, j.min(i));
                }
                let sv: f64 = perm.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
                sum += sv;
                sum2 += sv * sv;
            }
            let mean = sum / reps as f64;
            let var = sum2 / reps as f64 - mean * mean;
            assert!(
                (mean - e_s).abs() < 0.02 * e_s + 0.05,
                "tied={tied}: simulated mean {mean:.3} vs closed form {e_s:.3}"
            );
            assert!(
                (var - var_s).abs() < 0.05 * var_s + 0.05,
                "tied={tied}: simulated var {var:.3} vs closed form {var_s:.3}"
            );
        }
    }

    #[test]
    fn xi_test_flags_dependence_and_clears_independence() {
        let mut s = 0x9E3779B97F4A7C15u64;
        let mut next = || {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
        };
        // y = x^2: the tested ξ must be decisively significant.
        let pairs: Vec<(f64, f64)> = (0..2000)
            .map(|i| {
                let x = i as f64 / 1999.0;
                (x, x * x)
            })
            .collect();
        let (_, p) = chatterjee_xi_tested(&pairs).unwrap();
        assert!(p.unwrap() < 1e-10, "p = {:?}", p);
        // Independent draws at n = 4000: not significant.
        let indep: Vec<(f64, f64)> = (0..4000).map(|_| (next(), next())).collect();
        let (_, p2) = chatterjee_xi_tested(&indep).unwrap();
        assert!(p2.unwrap() > 0.001, "p = {:?}", p2);
        // y = sin(6πx): ξ sees a dependence Pearson cannot. A circle
        // would not do: y there is not a function of x, and ξ measures
        // how far y is a function of x.
        let wave: Vec<(f64, f64)> = (0..2000)
            .map(|i| {
                let x = i as f64 / 1999.0;
                (x, (6.0 * std::f64::consts::PI * x).sin())
            })
            .collect();
        assert!(pearson(&wave).unwrap().abs() < 0.3);
        let (xi_w, p3) = chatterjee_xi_tested(&wave).unwrap();
        assert!(
            xi_w > 0.5 && p3.unwrap() < 1e-6,
            "xi = {xi_w}, p = {:?}",
            p3
        );
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
}
