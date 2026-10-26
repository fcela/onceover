//! Numerical model-tree and single-break primitives, independent of table/report types.

use serde::Serialize;

#[derive(Clone, Debug)]
pub struct ModelTreeConfig {
    pub max_depth: usize,
    pub min_leaf: usize,
    pub max_features: usize,
    /// Coarse quantile candidates per feature, followed by local refinement.
    pub max_thresholds: usize,
    /// Split SDR as a fraction of the node's target standard deviation.
    pub min_gain_fraction: f64,
    /// Dimensionless ridge penalty on slopes (intercept is unpenalized).
    pub ridge: f64,
    pub prune: bool,
    /// M5 ancestor smoothing pseudo-count; zero disables smoothing.
    pub smoothing: f64,
}

impl Default for ModelTreeConfig {
    fn default() -> Self {
        Self {
            max_depth: 3,
            min_leaf: 30,
            max_features: 8,
            max_thresholds: 32,
            min_gain_fraction: 0.005,
            ridge: 1e-8,
            prune: true,
            smoothing: 15.0,
        }
    }
}

#[derive(Clone, Debug)]
struct Linear {
    intercept: f64,
    coefficients: Vec<f64>,
}

impl Linear {
    fn predict(&self, x: &[f64]) -> f64 {
        self.intercept
            + self
                .coefficients
                .iter()
                .zip(x)
                .map(|(a, b)| a * b)
                .sum::<f64>()
    }

    fn blend(&mut self, parent: &Self, n: usize, k: f64) {
        let w = n as f64 / (n as f64 + k);
        self.intercept = w * self.intercept + (1.0 - w) * parent.intercept;
        for (c, p) in self.coefficients.iter_mut().zip(&parent.coefficients) {
            *c = w * *c + (1.0 - w) * p;
        }
    }
}

#[derive(Clone, Debug)]
struct Node {
    model: Linear,
    n: usize,
    split: Option<(usize, f64, Box<Node>, Box<Node>)>,
}

impl Node {
    fn predict(&self, x: &[f64], smoothing: f64) -> f64 {
        if let Some((f, t, left, right)) = &self.split {
            let child = if x[*f] <= *t { left } else { right };
            let w = child.n as f64 / (child.n as f64 + smoothing);
            w * child.predict(x, smoothing) + (1.0 - w) * self.model.predict(x)
        } else {
            self.model.predict(x)
        }
    }

    fn leaves(&self) -> usize {
        self.split
            .as_ref()
            .map(|(_, _, l, r)| l.leaves() + r.leaves())
            .unwrap_or(1)
    }

    fn depth(&self) -> usize {
        self.split
            .as_ref()
            .map(|(_, _, l, r)| 1 + l.depth().max(r.depth()))
            .unwrap_or(0)
    }

    fn leaf_index(&self, x: &[f64]) -> usize {
        match &self.split {
            Some((f, t, l, r)) if x[*f] > *t => l.leaves() + r.leaf_index(x),
            Some((_, _, l, _)) => l.leaf_index(x),
            None => 0,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SplitCondition {
    pub feature: usize,
    pub threshold: f64,
    pub less_equal: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RegimeLeaf {
    pub conditions: Vec<SplitCondition>,
    pub intercept: f64,
    pub coefficients: Vec<f64>,
    pub n_train: usize,
}

#[derive(Clone, Debug)]
pub struct ModelTree {
    root: Node,
    means: Vec<f64>,
    scales: Vec<f64>,
    y_mean: f64,
    pub(super) y_scale: f64,
    smoothing: f64,
}

// Scale using centered ratios to avoid squaring large or tiny raw units.
fn location_scale(values: impl Iterator<Item = f64> + Clone, n: usize) -> Option<(f64, f64)> {
    let mut mean = 0.0;
    for (i, v) in values.clone().enumerate() {
        mean += (v - mean) / (i + 1) as f64;
    }
    let largest = values.clone().map(|v| (v - mean).abs()).fold(0.0, f64::max);
    let scale = if largest > 0.0 {
        largest * (values.map(|v| ((v - mean) / largest).powi(2)).sum::<f64>() / n as f64).sqrt()
    } else {
        1.0
    };
    (mean.is_finite() && scale.is_finite() && scale > 0.0).then_some((mean, scale))
}

impl ModelTree {
    pub fn fit(x: &[Vec<f64>], y: &[f64], cfg: &ModelTreeConfig) -> Result<Self, String> {
        if x.len() != y.len() || y.len() < 2 {
            return Err("model tree requires at least two aligned rows".into());
        }
        let p = x[0].len();
        if p > cfg.max_features
            || cfg.max_features > 16
            || cfg.max_depth > 8
            || cfg.min_leaf < 2
            || cfg.max_thresholds == 0
            || cfg.max_thresholds > 256
            || !cfg.min_gain_fraction.is_finite()
            || !(0.0..1.0).contains(&cfg.min_gain_fraction)
            || !cfg.ridge.is_finite()
            || cfg.ridge < 0.0
            || cfg.ridge > 1.0
            || !cfg.smoothing.is_finite()
            || cfg.smoothing < 0.0
        {
            return Err("invalid or unbounded model-tree configuration".into());
        }
        if x.iter()
            .any(|r| r.len() != p || r.iter().any(|v| !v.is_finite()))
            || y.iter().any(|v| !v.is_finite())
        {
            return Err("model tree requires rectangular, finite, complete numeric rows".into());
        }
        let (mut means, mut scales) = (Vec::new(), Vec::new());
        for j in 0..p {
            let (mean, scale) = location_scale(x.iter().map(|r| r[j]), x.len())
                .ok_or("feature range cannot be represented safely")?;
            means.push(mean);
            scales.push(scale);
        }
        let (y_mean, y_scale) = location_scale(y.iter().copied(), y.len())
            .ok_or("target range cannot be represented safely")?;
        let z: Vec<Vec<f64>> = x
            .iter()
            .map(|r| {
                r.iter()
                    .enumerate()
                    .map(|(j, v)| (v - means[j]) / scales[j])
                    .collect()
            })
            .collect();
        let target: Vec<f64> = y.iter().map(|v| (v - y_mean) / y_scale).collect();
        let root = grow(&z, &target, &(0..y.len()).collect::<Vec<_>>(), 0, cfg);
        Ok(Self {
            root,
            means,
            scales,
            y_mean,
            y_scale,
            smoothing: cfg.smoothing,
        })
    }

    fn normalized(&self, x: &[f64]) -> Option<Vec<f64>> {
        if x.len() != self.means.len() {
            return None;
        }
        let z: Vec<f64> = x
            .iter()
            .enumerate()
            .map(|(j, v)| (v - self.means[j]) / self.scales[j])
            .collect();
        z.iter().all(|v| v.is_finite()).then_some(z)
    }

    pub fn predict(&self, x: &[f64]) -> Option<f64> {
        let v =
            self.y_mean + self.y_scale * self.root.predict(&self.normalized(x)?, self.smoothing);
        v.is_finite().then_some(v)
    }

    /// Global baseline in original units, so a residual finding identifies
    /// the actual model used to define its residuals.
    pub fn global_linear_model(&self) -> (f64, Vec<f64>) {
        let coefficients: Vec<_> = self
            .root
            .model
            .coefficients
            .iter()
            .enumerate()
            .map(|(j, b)| self.y_scale * b / self.scales[j])
            .collect();
        let intercept = self.y_mean + self.y_scale * self.root.model.intercept
            - coefficients
                .iter()
                .zip(&self.means)
                .map(|(b, m)| b * m)
                .sum::<f64>();
        (intercept, coefficients)
    }

    pub fn predict_linear(&self, x: &[f64]) -> Option<f64> {
        let v = self.y_mean + self.y_scale * self.root.model.predict(&self.normalized(x)?);
        v.is_finite().then_some(v)
    }

    pub fn leaf_count(&self) -> usize {
        self.root.leaves()
    }
    pub fn depth(&self) -> usize {
        self.root.depth()
    }
    pub fn leaf_index(&self, x: &[f64]) -> Option<usize> {
        Some(self.root.leaf_index(&self.normalized(x)?))
    }

    /// Effective equations include exactly the same smoothing as predict().
    pub fn leaves(&self) -> Vec<RegimeLeaf> {
        fn visit(
            tree: &ModelTree,
            node: &Node,
            path: &mut Vec<SplitCondition>,
            ancestors: &mut Vec<(Linear, usize)>,
            out: &mut Vec<RegimeLeaf>,
        ) {
            if let Some((f, t, l, r)) = &node.split {
                for (child, less_equal) in [(l.as_ref(), true), (r.as_ref(), false)] {
                    path.push(SplitCondition {
                        feature: *f,
                        threshold: tree.means[*f] + *t * tree.scales[*f],
                        less_equal,
                    });
                    ancestors.push((node.model.clone(), child.n));
                    visit(tree, child, path, ancestors, out);
                    ancestors.pop();
                    path.pop();
                }
            } else {
                let mut effective = node.model.clone();
                for (parent, n) in ancestors.iter().rev() {
                    effective.blend(parent, *n, tree.smoothing);
                }
                let coefficients: Vec<f64> = effective
                    .coefficients
                    .iter()
                    .enumerate()
                    .map(|(j, b)| tree.y_scale * b / tree.scales[j])
                    .collect();
                let intercept = tree.y_mean + tree.y_scale * effective.intercept
                    - coefficients
                        .iter()
                        .zip(&tree.means)
                        .map(|(b, m)| b * m)
                        .sum::<f64>();
                out.push(RegimeLeaf {
                    conditions: path.clone(),
                    intercept,
                    coefficients,
                    n_train: node.n,
                });
            }
        }
        let mut out = Vec::new();
        visit(self, &self.root, &mut Vec::new(), &mut Vec::new(), &mut out);
        out
    }
}

// Householder QR on an augmented ridge system. No normal-equation squaring
// of the condition number; constant and collinear predictors are supported.
fn fit_linear(x: &[Vec<f64>], y: &[f64], ids: &[usize], ridge: f64) -> Linear {
    let p = x[0].len();
    let cols = p + 1;
    let rows = ids.len() + p;
    let mut a = vec![0.0; rows * cols];
    let mut b = vec![0.0; rows];
    for (r, &i) in ids.iter().enumerate() {
        a[r * cols] = 1.0;
        a[r * cols + 1..(r + 1) * cols].copy_from_slice(&x[i]);
        b[r] = y[i];
    }
    for j in 0..p {
        a[(ids.len() + j) * cols + j + 1] = (ridge * ids.len() as f64).sqrt();
    }
    for k in 0..cols.min(rows) {
        let norm = (k..rows)
            .map(|r| a[r * cols + k].powi(2))
            .sum::<f64>()
            .sqrt();
        if norm < 1e-14 {
            continue;
        }
        let mut v: Vec<f64> = (k..rows).map(|r| a[r * cols + k]).collect();
        v[0] += norm.copysign(v[0]);
        let vn = v.iter().map(|v| v * v).sum::<f64>().sqrt();
        for z in &mut v {
            *z /= vn;
        }
        for j in k..cols {
            let dot = 2.0
                * v.iter()
                    .enumerate()
                    .map(|(r, z)| z * a[(r + k) * cols + j])
                    .sum::<f64>();
            for (r, z) in v.iter().enumerate() {
                a[(r + k) * cols + j] -= dot * z;
            }
        }
        let dot = 2.0 * v.iter().enumerate().map(|(r, z)| z * b[r + k]).sum::<f64>();
        for (r, z) in v.iter().enumerate() {
            b[r + k] -= dot * z;
        }
    }
    let mut beta = vec![0.0; cols];
    for k in (0..cols.min(rows)).rev() {
        let rhs = b[k]
            - (k + 1..cols)
                .map(|j| a[k * cols + j] * beta[j])
                .sum::<f64>();
        if a[k * cols + k].abs() > 1e-12 {
            beta[k] = rhs / a[k * cols + k];
        }
    }
    Linear {
        intercept: beta[0],
        coefficients: beta[1..].to_vec(),
    }
}

fn grow(x: &[Vec<f64>], y: &[f64], ids: &[usize], depth: usize, cfg: &ModelTreeConfig) -> Node {
    let mut node = Node {
        model: fit_linear(x, y, ids, cfg.ridge),
        n: ids.len(),
        split: None,
    };
    if depth >= cfg.max_depth || ids.len() / 2 < cfg.min_leaf {
        return node;
    }
    if let Some((f, t)) = best_split(x, y, ids, cfg) {
        let (left, right): (Vec<_>, Vec<_>) = ids.iter().copied().partition(|&i| x[i][f] <= t);
        let l = grow(x, y, &left, depth + 1, cfg);
        let r = grow(x, y, &right, depth + 1, cfg);
        node.split = Some((f, t, Box::new(l), Box::new(r)));
        if cfg.prune {
            let parent_mae = ids
                .iter()
                .map(|&i| (node.model.predict(&x[i]) - y[i]).abs())
                .sum::<f64>()
                / ids.len() as f64;
            let tree_mae = ids
                .iter()
                .map(|&i| (node.predict(&x[i], 0.0) - y[i]).abs())
                .sum::<f64>()
                / ids.len() as f64;
            let adjusted = |e: f64, parameters: usize| {
                if ids.len() <= parameters {
                    f64::INFINITY
                } else {
                    e * (ids.len() + parameters) as f64 / (ids.len() - parameters) as f64
                }
            };
            let p = x[0].len() + 1;
            if adjusted(parent_mae, p) <= adjusted(tree_mae, p * node.leaves()) + 1e-6 {
                node.split = None;
            }
        }
    }
    node
}

fn best_split(
    x: &[Vec<f64>],
    y: &[f64],
    ids: &[usize],
    cfg: &ModelTreeConfig,
) -> Option<(usize, f64)> {
    let n = ids.len();
    let total = ids.iter().map(|&i| y[i]).sum::<f64>();
    let total_sq = ids.iter().map(|&i| y[i] * y[i]).sum::<f64>();
    let sd = (total_sq / n as f64 - (total / n as f64).powi(2))
        .max(0.0)
        .sqrt();
    if sd < 1e-12 {
        return None;
    }
    let mut best = (cfg.min_gain_fraction, 0usize, 0.0);
    let mut selected = false;
    for (f, _) in x[0].iter().enumerate() {
        let mut sorted = ids.to_vec();
        sorted.sort_unstable_by(|&a, &b| x[a][f].total_cmp(&x[b][f]).then(a.cmp(&b)));
        let (mut sum, mut sq) = (vec![0.0; n + 1], vec![0.0; n + 1]);
        for (k, &i) in sorted.iter().enumerate() {
            sum[k + 1] = sum[k] + y[i];
            sq[k + 1] = sq[k] + y[i] * y[i];
        }
        let gain = |k: usize| -> f64 {
            if x[sorted[k - 1]][f] >= x[sorted[k]][f] {
                return -1.0;
            }
            let lsd = (sq[k] / k as f64 - (sum[k] / k as f64).powi(2))
                .max(0.0)
                .sqrt();
            let rn = (n - k) as f64;
            let rsd = ((total_sq - sq[k]) / rn - ((total - sum[k]) / rn).powi(2))
                .max(0.0)
                .sqrt();
            1.0 - (k as f64 * lsd + rn * rsd) / (n as f64 * sd)
        };
        let mut feature_best = (-1.0, cfg.min_leaf);
        for c in 0..=cfg.max_thresholds + 1 {
            let mut k = cfg.min_leaf + (n - 2 * cfg.min_leaf) * c / (cfg.max_thresholds + 1);
            // Snap quantile ranks to the end of a tie group. Otherwise a
            // low-cardinality numeric gate may have no valid coarse candidate.
            if x[sorted[k - 1]][f] == x[sorted[k]][f] {
                let value = x[sorted[k - 1]][f];
                k = sorted.partition_point(|&i| x[i][f] <= value);
            }
            if k < cfg.min_leaf || k > n - cfg.min_leaf {
                continue;
            }
            let g = gain(k);
            if g > feature_best.0 {
                feature_best = (g, k);
            }
        }
        // Prefix statistics make this refinement linear, not a row scan per threshold.
        let radius = n.div_ceil(cfg.max_thresholds + 1);
        for k in feature_best.1.saturating_sub(radius).max(cfg.min_leaf)
            ..=feature_best.1.saturating_add(radius).min(n - cfg.min_leaf)
        {
            let g = gain(k);
            if g > feature_best.0 {
                feature_best = (g, k);
            }
        }
        if feature_best.0 > best.0 + 1e-12 {
            let k = feature_best.1;
            best = (feature_best.0, f, x[sorted[k - 1]][f]);
            selected = true;
        }
    }
    selected.then_some((best.1, best.2))
}

#[derive(Clone, Debug)]
pub struct BreakResult {
    /// First index in the right-hand segment.
    pub index: usize,
    pub gain_fraction: f64,
    pub left_mean: f64,
    pub right_mean: f64,
}

/// Exact best one-mean-break scan in O(n), with centered cumulative statistics.
/// This returns an exploratory effect size, not a p-value for a selected split.
pub fn single_break_l2(values: &[f64], min_segment: usize) -> Option<BreakResult> {
    let n = values.len();
    if min_segment < 2 || n / 2 < min_segment || values.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let (center, scale) = location_scale(values.iter().copied(), n)?;
    let z: Vec<f64> = values.iter().map(|v| (v - center) / scale).collect();
    let total = z.iter().sum::<f64>();
    let total_sq = z.iter().map(|v| v * v).sum::<f64>();
    let sst = total_sq - total * total / n as f64;
    if sst <= 1e-12 {
        return None;
    }
    let (mut left, mut left_sq) = (0.0, 0.0);
    let mut best = (sst, 0usize, 0.0, 0.0);
    for k in 1..n {
        left += z[k - 1];
        left_sq += z[k - 1] * z[k - 1];
        if k < min_segment || n - k < min_segment {
            continue;
        }
        let right = total - left;
        let cost = (left_sq - left * left / k as f64).max(0.0)
            + (total_sq - left_sq - right * right / (n - k) as f64).max(0.0);
        if cost < best.0 {
            best = (cost, k, left / k as f64, right / (n - k) as f64);
        }
    }
    (best.1 > 0).then_some(BreakResult {
        index: best.1,
        gain_fraction: ((sst - best.0) / sst).clamp(0.0, 1.0),
        left_mean: center + scale * best.2,
        right_mean: center + scale * best.3,
    })
}
