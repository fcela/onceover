use serde::Serialize;

pub const MAX_GROUPS: usize = 4;
const GRID: usize = 12;
const CELLS: usize = GRID * GRID;
const FUNC_BINS: usize = 12;
const MIN_GEOM_VERTICES: usize = 5;
const MIN_STRINGY_VERTICES: usize = 8;
const MIN_CLUMPY_VERTICES: usize = 4;
const MIN_MONOTONIC_DISTINCT: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    Monotonic,
    Functional,
    BinnedStriated,
    BinnedStringy,
    BinnedClumpy,
}

impl Metric {
    #[inline]
    pub fn is_geometric(self) -> bool {
        matches!(
            self,
            Self::BinnedStriated | Self::BinnedStringy | Self::BinnedClumpy
        )
    }
}

impl std::str::FromStr for Metric {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "monotonic" => Ok(Self::Monotonic),
            "functional" => Ok(Self::Functional),
            "binned_striated" => Ok(Self::BinnedStriated),
            "binned_stringy" => Ok(Self::BinnedStringy),
            "binned_clumpy" => Ok(Self::BinnedClumpy),
            _ => Err(format!(
                "unknown metric {value:?}; expected 'monotonic', 'functional', 'binned_striated', 'binned_stringy', or 'binned_clumpy'"
            )),
        }
    }
}

#[derive(Clone, Copy, Default, Serialize)]
pub struct Shape {
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rho: Option<f64>,
}

#[derive(Clone, Copy, Default)]
pub struct MstMetrics {
    pub striated: f64,
    pub stringy: f64,
    pub clumpy: f64,
}

impl MstMetrics {
    #[inline]
    pub fn get(self, metric: Metric) -> f64 {
        match metric {
            Metric::BinnedStriated => self.striated,
            Metric::BinnedStringy => self.stringy,
            Metric::BinnedClumpy => self.clumpy,
            Metric::Monotonic | Metric::Functional => 0.0,
        }
    }
}

/// Use the same robust affine scale, grid, and quantile edges for the pooled
/// cloud and every child. Fits `[q01, q99]` bounds when enough points exist
/// (falling back to `[min, max]`) so a single extreme outlier cannot collapse
/// the grid into edge bins.
pub struct Scale {
    bounds: [[f64; 2]; 2],
    func_edges: [[f64; FUNC_BINS - 1]; 2],
    sorted_ref: [Vec<f64>; 2],
}

impl Scale {
    pub fn fit(points: &[[f64; 2]]) -> Self {
        if points.is_empty() {
            return Self {
                bounds: [[0.0, 0.0]; 2],
                func_edges: [[0.0; FUNC_BINS - 1]; 2],
                sorted_ref: [Vec::new(), Vec::new()],
            };
        }
        let mut sorted_ref: [Vec<f64>; 2] = std::array::from_fn(|axis| {
            let mut vals: Vec<f64> = points.iter().map(|p| p[axis]).collect();
            vals.sort_unstable_by(f64::total_cmp);
            vals
        });
        let bounds = std::array::from_fn(|axis| {
            let vals = &sorted_ref[axis];
            let n = vals.len();
            if n >= 20 {
                let lo_q = vals[(n / 100).min(n - 1)];
                let hi_q = vals[(n - 1).saturating_sub(n / 100)];
                if hi_q > lo_q {
                    return [lo_q, hi_q];
                }
            }
            [vals[0], vals[n - 1]]
        });
        let func_edges = std::array::from_fn(|axis| {
            let vals = &sorted_ref[axis];
            let n = vals.len();
            std::array::from_fn(|k| {
                let idx = (((k + 1) * n) / FUNC_BINS).min(n - 1);
                vals[idx]
            })
        });
        // Keep a compact 256-point quantile lookup for fast O(log 256) CDF mapping.
        for vals in &mut sorted_ref {
            if vals.len() > 256 {
                let n = vals.len();
                *vals = (0..256).map(|i| vals[(i * (n - 1)) / 255]).collect();
            }
        }
        Self {
            bounds,
            func_edges,
            sorted_ref,
        }
    }

    fn func_point(&self, point: [f64; 2]) -> ([u8; 2], [f64; 2]) {
        let bins = std::array::from_fn(|axis| {
            let v = point[axis];
            let edges = &self.func_edges[axis];
            let mut b = 0u8;
            while (b as usize) < FUNC_BINS - 1 && v > edges[b as usize] {
                b += 1;
            }
            b
        });
        let cdf = std::array::from_fn(|axis| {
            let ref_vals = &self.sorted_ref[axis];
            if ref_vals.len() <= 1 {
                return 0.5;
            }
            let v = point[axis];
            let lo = ref_vals.partition_point(|&s| s < v);
            let hi = ref_vals.partition_point(|&s| s <= v);
            ((lo + hi) as f64 * 0.5 / (ref_vals.len() - 1) as f64).clamp(0.0, 1.0)
        });
        (bins, cdf)
    }

    fn grid_point(&self, point: [f64; 2]) -> (usize, [f64; 2]) {
        let normalized = std::array::from_fn(|i| {
            let [lo, hi] = self.bounds[i];
            let span = hi - lo;
            if span == f64::INFINITY {
                // Halving keeps the span finite even across both float extremes.
                let half_span = hi * 0.5 - lo * 0.5;
                ((point[i] * 0.5 - lo * 0.5) / half_span).clamp(0.0, 1.0)
            } else if span > 0.0 {
                ((point[i] - lo) / span).clamp(0.0, 1.0)
            } else {
                0.0
            }
        });
        let bin = normalized.map(|v| ((v * GRID as f64) as usize).min(GRID - 1));
        (bin[0] * GRID + bin[1], normalized)
    }
}

pub struct Cloud {
    ranks: Option<[RankOrder; 2]>,
    func: Vec<([u8; 2], [f64; 2])>,
    grid: Vec<(usize, [f64; 2])>,
}

impl Cloud {
    pub fn new(points: &[[f64; 2]], scale: &Scale, metrics: &[Metric]) -> Self {
        let ranks = metrics.contains(&Metric::Monotonic).then(|| {
            std::array::from_fn(|i| {
                RankOrder::new(&points.iter().map(|point| point[i]).collect::<Vec<_>>())
            })
        });
        let mut func = Vec::new();
        if metrics.contains(&Metric::Functional) {
            func.extend(points.iter().map(|&point| scale.func_point(point)));
        }
        let mut grid = Vec::new();
        if metrics.iter().any(|m| m.is_geometric()) {
            grid.extend(points.iter().map(|&point| scale.grid_point(point)));
        }
        Self { ranks, func, grid }
    }

    pub fn score(
        &self,
        metric: Metric,
        groups: &[u8],
        scratch: &mut Scratch,
    ) -> [Shape; MAX_GROUPS] {
        match metric {
            Metric::Monotonic => monotonic(self.ranks.as_ref().unwrap(), groups, scratch),
            Metric::Functional => functional_scores(&self.func, groups),
            m if m.is_geometric() => {
                let geom = self.score_geometry(groups);
                geom.map(|g| Shape {
                    score: g.get(m),
                    rho: None,
                })
            }
            _ => [Shape::default(); MAX_GROUPS],
        }
    }

    /// Compute all MST-based geometric scagnostics in a single pass per group.
    pub fn score_geometry(&self, groups: &[u8]) -> [MstMetrics; MAX_GROUPS] {
        geometry_scores(&self.grid, groups)
    }
}

pub struct Scratch {
    x_ranks: Vec<f64>,
}

impl Scratch {
    pub fn new(n: usize) -> Self {
        Self {
            x_ranks: vec![0.0; n],
        }
    }
}

struct RankOrder {
    order: Vec<usize>,
    tie_ends: Vec<usize>,
}

impl RankOrder {
    fn new(values: &[f64]) -> Self {
        let mut order: Vec<_> = (0..values.len()).collect();
        order.sort_unstable_by(|&a, &b| values[a].total_cmp(&values[b]));
        let mut tie_ends = Vec::new();
        for (i, adjacent) in order.windows(2).enumerate() {
            if values[adjacent[0]] != values[adjacent[1]] {
                tie_ends.push(i + 1);
            }
        }
        tie_ends.push(order.len());
        Self { order, tie_ends }
    }

    // Restrict the presorted order to each group, recompute its own midranks,
    // and count how many distinct non-tied values each group contains.
    fn rank_by_group(
        &self,
        groups: &[u8],
        mut visit: impl FnMut(usize, usize, f64),
    ) -> [usize; MAX_GROUPS] {
        let mut ranked = [0usize; MAX_GROUPS];
        let mut distinct = [0usize; MAX_GROUPS];
        let mut start = 0;
        for &end in &self.tie_ends {
            let run = &self.order[start..end];
            if let [row] = *run {
                let group = groups[row] as usize;
                ranked[group] += 1;
                distinct[group] += 1;
                visit(row, group, ranked[group] as f64);
            } else {
                let mut counts = [0usize; MAX_GROUPS];
                for &row in run {
                    counts[groups[row] as usize] += 1;
                }
                for (d, &c) in distinct.iter_mut().zip(&counts) {
                    if c > 0 {
                        *d += 1;
                    }
                }
                for &row in run {
                    let group = groups[row] as usize;
                    let midrank = ranked[group] as f64 + (counts[group] + 1) as f64 * 0.5;
                    visit(row, group, midrank);
                }
                for (n, add) in ranked.iter_mut().zip(counts) {
                    *n += add;
                }
            }
            start = end;
        }
        distinct
    }
}

fn monotonic(ranks: &[RankOrder; 2], groups: &[u8], scratch: &mut Scratch) -> [Shape; MAX_GROUPS] {
    let mut means = [0.5; MAX_GROUPS];
    let distinct_x = ranks[0].rank_by_group(groups, |row, group, rank| {
        scratch.x_ranks[row] = rank;
        means[group] += 0.5;
    });
    let mut sums = [[0.0; 3]; MAX_GROUPS];
    let distinct_y = ranks[1].rank_by_group(groups, |row, group, rank| {
        let dx = scratch.x_ranks[row] - means[group];
        let dy = rank - means[group];
        let [xx, yy, xy] = &mut sums[group];
        *xx += dx * dx;
        *yy += dy * dy;
        *xy += dx * dy;
    });
    std::array::from_fn(|g| {
        let [xx, yy, xy] = sums[g];
        if xx > 0.0
            && yy > 0.0
            && distinct_x[g] >= MIN_MONOTONIC_DISTINCT
            && distinct_y[g] >= MIN_MONOTONIC_DISTINCT
        {
            let rho = (xy / (xx * yy).sqrt()).clamp(-1.0, 1.0);
            Shape {
                score: rho * rho,
                rho: Some(rho),
            }
        } else {
            Shape::default()
        }
    })
}

/// Bias-adjusted nonparametric functional dependence `max(adj_eta²(y | bin(x)), adj_eta²(x | bin(y)))`
/// over discovery-fitted 12-quantile bins and CDF-scaled coordinates.
/// Captures non-monotonic / multi-peak curves (e.g., commute twin peaks vs weekend unimodal)
/// in O(n) with stack arrays, invariant to monotone rescaling, and requires >= 4 occupied bins.
fn functional_scores(func: &[([u8; 2], [f64; 2])], groups: &[u8]) -> [Shape; MAX_GROUPS] {
    let mut cnt_x = [[0usize; FUNC_BINS]; MAX_GROUPS];
    let mut sum_yx = [[0.0f64; FUNC_BINS]; MAX_GROUPS];
    let mut cnt_y = [[0usize; FUNC_BINS]; MAX_GROUPS];
    let mut sum_xy = [[0.0f64; FUNC_BINS]; MAX_GROUPS];

    let mut n_g = [0usize; MAX_GROUPS];
    let mut sum_x = [0.0f64; MAX_GROUPS];
    let mut sum_x2 = [0.0f64; MAX_GROUPS];
    let mut sum_y = [0.0f64; MAX_GROUPS];
    let mut sum_y2 = [0.0f64; MAX_GROUPS];

    for (&([bx, by], [ux, uy]), &group) in func.iter().zip(groups) {
        let g = group as usize;
        n_g[g] += 1;
        sum_x[g] += ux;
        sum_x2[g] += ux * ux;
        sum_y[g] += uy;
        sum_y2[g] += uy * uy;
        cnt_x[g][bx as usize] += 1;
        sum_yx[g][bx as usize] += uy;
        cnt_y[g][by as usize] += 1;
        sum_xy[g][by as usize] += ux;
    }

    let adj_eta2 = |n: usize,
                    sum_v: f64,
                    sum_v2: f64,
                    b_cnt: &[usize; FUNC_BINS],
                    b_sum: &[f64; FUNC_BINS]|
     -> f64 {
        if n < 20 {
            return 0.0;
        }
        let nf = n as f64;
        let mean = sum_v / nf;
        let tss = (sum_v2 - nf * mean * mean).max(0.0);
        if tss <= 1e-10 {
            return 0.0;
        }
        let mut occ = 0usize;
        let mut well_occ = 0usize;
        let mut bss = 0.0f64;
        for b in 0..FUNC_BINS {
            let c = b_cnt[b];
            if c > 0 {
                occ += 1;
                if c >= 2 {
                    well_occ += 1;
                }
                let bm = b_sum[b] / c as f64;
                let d = bm - mean;
                bss += c as f64 * d * d;
            }
        }
        if well_occ < 4 || n <= occ + 8 {
            return 0.0;
        }
        let raw = (bss / tss).clamp(0.0, 1.0);
        let adj = 1.0 - (1.0 - raw) * ((n - 1) as f64 / (n - occ) as f64);
        adj.clamp(0.0, 1.0)
    };

    std::array::from_fn(|g| {
        let n = n_g[g];
        let r2_yx = adj_eta2(n, sum_y[g], sum_y2[g], &cnt_x[g], &sum_yx[g]);
        let r2_xy = adj_eta2(n, sum_x[g], sum_x2[g], &cnt_y[g], &sum_xy[g]);
        Shape {
            score: r2_yx.max(r2_xy),
            rho: None,
        }
    })
}

fn geometry_scores(grid: &[(usize, [f64; 2])], groups: &[u8]) -> [MstMetrics; MAX_GROUPS] {
    let mut bins = [[[0.0; 2]; CELLS]; MAX_GROUPS];
    let mut counts = [[0usize; CELLS]; MAX_GROUPS];
    for (&(cell, [x, y]), &group) in grid.iter().zip(groups) {
        let g = group as usize;
        bins[g][cell][0] += x;
        bins[g][cell][1] += y;
        counts[g][cell] += 1;
    }
    std::array::from_fn(|g| {
        let mut vertices = [[0.0; 2]; CELLS];
        let mut weights = [0usize; CELLS];
        let mut x_mask = 0u32;
        let mut y_mask = 0u32;
        let mut len = 0;
        for c in 0..CELLS {
            let n = counts[g][c];
            if n > 0 {
                let nf = n as f64;
                vertices[len] = [bins[g][c][0] / nf, bins[g][c][1] / nf];
                weights[len] = n;
                x_mask |= 1u32 << (c / GRID);
                y_mask |= 1u32 << (c % GRID);
                len += 1;
            }
        }
        let x_bins = x_mask.count_ones() as usize;
        let y_bins = y_mask.count_ones() as usize;
        if x_bins < 2 || y_bins < 2 {
            // Collapsed onto a single horizontal or vertical strip — not a 2D cloud shape.
            return MstMetrics::default();
        }
        let mut m = mst_features(&vertices[..len], &weights[..len], MIN_GEOM_VERTICES);
        if x_bins < 3 || y_bins < 3 {
            // Striation and 1D curved stringiness require genuine 2D spread (>= 3 bins per axis).
            m.striated = 0.0;
            m.stringy = 0.0;
        }
        m
    })
}

fn mst_features(vertices: &[[f64; 2]], weights: &[usize], min_striated_v: usize) -> MstMetrics {
    let v = vertices.len();
    if v < 3 {
        return MstMetrics::default();
    }
    // Dense Prim over at most CELLS (144) occupied cell centroids.
    let mut nearest = [f64::INFINITY; CELLS];
    let mut parent = [0usize; CELLS];
    let mut edge_len = [0.0f64; CELLS];
    let mut degree = [0usize; CELLS];
    let mut deg2_nbrs = [[0usize; 2]; CELLS];
    // Tree adjacency stored flat: at most 2*(v-1) directed edges.
    let mut head = [usize::MAX; CELLS];
    let mut to = [0usize; CELLS * 2];
    let mut w_adj = [0.0f64; CELLS * 2];
    let mut next_adj = [usize::MAX; CELLS * 2];
    let mut adj_len = 0usize;
    let mut total_len = 0.0f64;

    nearest[0] = 0.0;
    for step in 0..v {
        let mut cur = 0;
        let mut dist2 = f64::INFINITY;
        for (i, &cand) in nearest[..v].iter().enumerate() {
            if cand >= 0.0 && cand < dist2 {
                cur = i;
                dist2 = cand;
            }
        }
        nearest[cur] = -1.0;
        if step > 0 {
            let p = parent[cur];
            let len = dist2.sqrt();
            edge_len[cur] = len;
            total_len += len;
            for (a, b) in [(cur, p), (p, cur)] {
                if degree[a] < 2 {
                    deg2_nbrs[a][degree[a]] = b;
                }
                degree[a] += 1;
                to[adj_len] = b;
                w_adj[adj_len] = len;
                next_adj[adj_len] = head[a];
                head[a] = adj_len;
                adj_len += 1;
            }
        }
        let [x, y] = vertices[cur];
        for (i, &[a, b]) in vertices.iter().enumerate() {
            let dx = a - x;
            let dy = b - y;
            let d2 = dx * dx + dy * dy;
            if d2 < nearest[i] {
                nearest[i] = d2;
                parent[i] = cur;
            }
        }
    }

    // 1. Striated: fraction of degree-2 vertices with incident-edge cosine < -0.75.
    let striated = if v >= min_striated_v {
        let mut aligned = 0usize;
        for (i, &[x, y]) in vertices.iter().enumerate() {
            if degree[i] == 2 {
                let [a, b] = deg2_nbrs[i];
                let [ax, ay] = [vertices[a][0] - x, vertices[a][1] - y];
                let [bx, by] = [vertices[b][0] - x, vertices[b][1] - y];
                let dot = ax * bx + ay * by;
                if dot < 0.0 && dot * dot > 0.5625 * (ax * ax + ay * ay) * (bx * bx + by * by) {
                    aligned += 1;
                }
            }
        }
        aligned as f64 / v as f64
    } else {
        0.0
    };

    // 2. Stringy: MST weighted diameter / total edge length (Wilkinson & Wills).
    let stringy = if v >= MIN_STRINGY_VERTICES && total_len > 1e-12 {
        let farthest = |start: usize| -> (usize, f64) {
            let mut stack = [(0usize, usize::MAX, 0.0f64); CELLS];
            let mut sp = 1usize;
            stack[0] = (start, usize::MAX, 0.0);
            let mut best_u = start;
            let mut best_d = 0.0f64;
            while sp > 0 {
                sp -= 1;
                let (u, par, d) = stack[sp];
                if d > best_d {
                    best_d = d;
                    best_u = u;
                }
                let mut e = head[u];
                while e != usize::MAX {
                    let nb = to[e];
                    if nb != par {
                        stack[sp] = (nb, u, d + w_adj[e]);
                        sp += 1;
                    }
                    e = next_adj[e];
                }
            }
            (best_u, best_d)
        };
        let (end_a, _) = farthest(0);
        let (_, diameter) = farthest(end_a);
        (diameter / total_len).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // 3. Clumpy: max_e (1 - max_{e' in R(e)} w(e') / w(e)) via O(V) rerooting DP,
    // requiring the smaller component R(e) to contain >= max(10, 10% of points)
    // so stray outlier points cannot trigger clumping, while compact multi-point
    // clusters occupying 1-2 grid cells still register cleanly (using grid cell
    // width as the internal diameter floor for single-cell clusters).
    let total_weight: usize = weights.iter().sum();
    let clumpy = if v >= MIN_CLUMPY_VERTICES && total_weight >= 20 {
        let min_side_weight = (total_weight / 10).max(10);
        let cell_width = 1.0 / GRID as f64;
        // Build rooted BFS order from node 0.
        let mut order = [0usize; CELLS];
        let mut tparent = [usize::MAX; CELLS];
        let mut tp_len = [0.0f64; CELLS];
        let mut qh = 0usize;
        let mut qt = 1usize;
        order[0] = 0;
        while qh < qt {
            let u = order[qh];
            qh += 1;
            let mut e = head[u];
            while e != usize::MAX {
                let nb = to[e];
                if nb != tparent[u] {
                    tparent[nb] = u;
                    tp_len[nb] = w_adj[e];
                    order[qt] = nb;
                    qt += 1;
                }
                e = next_adj[e];
            }
        }
        // Bottom-up subtree aggregates.
        let mut sub_nodes = [1usize; CELLS];
        let mut sub_weight = [0usize; CELLS];
        let mut sub_max = [0.0f64; CELLS];
        sub_weight[..v].copy_from_slice(&weights[..v]);
        for &u in order[1..qt].iter().rev() {
            let p = tparent[u];
            sub_nodes[p] += sub_nodes[u];
            sub_weight[p] += sub_weight[u];
            let contrib = tp_len[u].max(sub_max[u]);
            if contrib > sub_max[p] {
                sub_max[p] = contrib;
            }
        }
        // Top-down complement max edge strictly outside each child's subtree.
        let mut up_max = [0.0f64; CELLS];
        for &u in &order[..qt] {
            let up_all = if u == 0 {
                0.0
            } else {
                tp_len[u].max(up_max[u])
            };
            let mut best1 = 0.0f64;
            let mut best1_c = usize::MAX;
            let mut best2 = 0.0f64;
            let mut e = head[u];
            while e != usize::MAX {
                let c = to[e];
                if tparent[c] == u {
                    let contrib = tp_len[c].max(sub_max[c]);
                    if contrib > best1 {
                        best2 = best1;
                        best1 = contrib;
                        best1_c = c;
                    } else if contrib > best2 {
                        best2 = contrib;
                    }
                }
                e = next_adj[e];
            }
            let mut e = head[u];
            while e != usize::MAX {
                let c = to[e];
                if tparent[c] == u {
                    let sib_best = if c == best1_c { best2 } else { best1 };
                    up_max[c] = up_all.max(sib_best);
                }
                e = next_adj[e];
            }
        }
        let mut best_clumpy = 0.0f64;
        for &u in &order[1..qt] {
            let w_e = tp_len[u];
            if w_e <= 1e-12 {
                continue;
            }
            let a_nodes = sub_nodes[u];
            let b_nodes = v - a_nodes;
            let a_weight = sub_weight[u];
            let b_weight = total_weight - a_weight;
            let (sm_nodes, sm_weight, sm_max) =
                if a_nodes < b_nodes || (a_nodes == b_nodes && a_weight <= b_weight) {
                    (a_nodes, a_weight, sub_max[u])
                } else {
                    (b_nodes, b_weight, up_max[u])
                };
            if sm_weight >= min_side_weight {
                let inner = if sm_nodes <= 1 { cell_width } else { sm_max };
                let score = (1.0 - inner / w_e).clamp(0.0, 1.0);
                if score > best_clumpy {
                    best_clumpy = score;
                }
            }
        }
        best_clumpy
    } else {
        0.0
    };

    MstMetrics {
        striated,
        stringy,
        clumpy,
    }
}

/// Detect when a `monotonic` gain is a range-restriction artifact caused by
/// partitioning a single smooth global curve `y = f(x)` (or `x = g(y)`) along
/// a descriptor strongly correlated with one axis.
///
/// Fits simple binned smooth curves on discovery rank-scaled coordinates and
/// checks on holdout whether the pooled curve already explains >= 60% of the
/// rank variance while per-group curves add < 6% extra R².
pub fn is_range_restriction(
    train_pts: &[[f64; 2]],
    train_groups: &[u8],
    holdout_pts: &[[f64; 2]],
    holdout_groups: &[u8],
    n_groups: usize,
) -> bool {
    if train_pts.len() < 48 || holdout_pts.len() < 48 || n_groups < 2 {
        return false;
    }
    let cdf_axis = |axis: usize| -> (Vec<f64>, Vec<f64>) {
        let mut sorted: Vec<f64> = train_pts.iter().map(|p| p[axis]).collect();
        sorted.sort_unstable_by(f64::total_cmp);
        let denom = (sorted.len().saturating_sub(1)).max(1) as f64;
        let map_val = |v: f64| -> f64 {
            let lo = sorted.partition_point(|&s| s < v);
            let hi = sorted.partition_point(|&s| s <= v);
            let mid = (lo + hi) as f64 * 0.5;
            (mid / denom).clamp(0.0, 1.0)
        };
        let tr = train_pts.iter().map(|p| map_val(p[axis])).collect();
        let ho = holdout_pts.iter().map(|p| map_val(p[axis])).collect();
        (tr, ho)
    };
    let (tr_x, ho_x) = cdf_axis(0);
    let (tr_y, ho_y) = cdf_axis(1);

    let check_dir = |tx: &[f64], ty: &[f64], hx: &[f64], hy: &[f64]| -> bool {
        if eta_squared(tx, train_groups, n_groups) <= 0.35 {
            return false;
        }
        let (r2_pool, r2_grp) =
            binned_curve_r2(tx, ty, train_groups, hx, hy, holdout_groups, n_groups);
        r2_pool >= 0.60 && (r2_grp - r2_pool) < 0.06
    };

    check_dir(&tr_x, &tr_y, &ho_x, &ho_y) || check_dir(&tr_y, &tr_x, &ho_y, &ho_x)
}

fn eta_squared(values: &[f64], groups: &[u8], n_groups: usize) -> f64 {
    let n = values.len() as f64;
    if n < 2.0 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / n;
    let total_ss: f64 = values.iter().map(|&v| (v - mean).powi(2)).sum();
    if total_ss <= 1e-12 {
        return 0.0;
    }
    let mut sum = [0.0f64; MAX_GROUPS];
    let mut cnt = [0usize; MAX_GROUPS];
    for (&v, &g) in values.iter().zip(groups) {
        let idx = g as usize;
        sum[idx] += v;
        cnt[idx] += 1;
    }
    let between_ss: f64 = (0..n_groups)
        .filter(|&g| cnt[g] > 0)
        .map(|g| {
            let gm = sum[g] / cnt[g] as f64;
            cnt[g] as f64 * (gm - mean).powi(2)
        })
        .sum();
    (between_ss / total_ss).clamp(0.0, 1.0)
}

fn binned_curve_r2(
    tx: &[f64],
    ty: &[f64],
    tg: &[u8],
    hx: &[f64],
    hy: &[f64],
    hg: &[u8],
    n_groups: usize,
) -> (f64, f64) {
    const POOLED_BINS: usize = 10;
    const GROUP_BINS: usize = 6;

    let tr_mean = ty.iter().sum::<f64>() / ty.len() as f64;
    let mut p_sum = [0.0f64; POOLED_BINS];
    let mut p_cnt = [0usize; POOLED_BINS];
    for (&x, &y) in tx.iter().zip(ty) {
        let b = ((x * POOLED_BINS as f64) as usize).min(POOLED_BINS - 1);
        p_sum[b] += y;
        p_cnt[b] += 1;
    }
    let p_pred: [f64; POOLED_BINS] = std::array::from_fn(|b| {
        if p_cnt[b] > 0 {
            p_sum[b] / p_cnt[b] as f64
        } else {
            tr_mean
        }
    });

    let mut g_mean_sum = [0.0f64; MAX_GROUPS];
    let mut g_mean_cnt = [0usize; MAX_GROUPS];
    let mut g_sum = [[0.0f64; GROUP_BINS]; MAX_GROUPS];
    let mut g_cnt = [[0usize; GROUP_BINS]; MAX_GROUPS];
    for ((&x, &y), &g) in tx.iter().zip(ty).zip(tg) {
        let gi = g as usize;
        g_mean_sum[gi] += y;
        g_mean_cnt[gi] += 1;
        let b = ((x * GROUP_BINS as f64) as usize).min(GROUP_BINS - 1);
        g_sum[gi][b] += y;
        g_cnt[gi][b] += 1;
    }
    let g_fallback: [f64; MAX_GROUPS] = std::array::from_fn(|g| {
        if g_mean_cnt[g] > 0 {
            g_mean_sum[g] / g_mean_cnt[g] as f64
        } else {
            tr_mean
        }
    });
    let g_pred: [[f64; GROUP_BINS]; MAX_GROUPS] = std::array::from_fn(|g| {
        std::array::from_fn(|b| {
            if g_cnt[g][b] > 0 {
                g_sum[g][b] / g_cnt[g][b] as f64
            } else {
                g_fallback[g]
            }
        })
    });

    let ho_mean = hy.iter().sum::<f64>() / hy.len() as f64;
    let ho_tss: f64 = hy.iter().map(|&y| (y - ho_mean).powi(2)).sum();
    if ho_tss <= 1e-12 {
        return (0.0, 0.0);
    }
    let mut rss_pool = 0.0f64;
    let mut rss_grp = 0.0f64;
    for ((&x, &y), &g) in hx.iter().zip(hy).zip(hg) {
        let bp = ((x * POOLED_BINS as f64) as usize).min(POOLED_BINS - 1);
        rss_pool += (y - p_pred[bp]).powi(2);
        let gi = (g as usize).min(n_groups - 1);
        let bg = ((x * GROUP_BINS as f64) as usize).min(GROUP_BINS - 1);
        rss_grp += (y - g_pred[gi][bg]).powi(2);
    }
    (1.0 - rss_pool / ho_tss, 1.0 - rss_grp / ho_tss)
}

#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::stats::spearman;

    #[test]
    fn restricted_ranks_match_independent_ranking_with_ties() {
        let points: Vec<_> = (0..100)
            .map(|i| [(i % 7) as f64, ((i * i) % 21) as f64])
            .collect();
        let groups: Vec<_> = (0..100).map(|i| (i % 4) as u8).collect();
        let cloud = Cloud::new(&points, &Scale::fit(&points), &[Metric::Monotonic]);
        let scores = cloud.score(Metric::Monotonic, &groups, &mut Scratch::new(points.len()));
        for (group, shape) in scores.iter().enumerate() {
            let (x, y): (Vec<_>, Vec<_>) = points
                .iter()
                .zip(&groups)
                .filter(|(_, &g)| g as usize == group)
                .map(|(&[x, y], _)| (x, y))
                .unzip();
            assert!((shape.rho.unwrap() - spearman(&x, &y).unwrap()).abs() < 1e-12);
        }
    }

    #[test]
    fn striation_counts_internal_vertices_of_a_line() {
        assert_eq!(
            mst_features(&[[0.0, 0.0], [0.5, 0.0], [1.0, 0.0]], &[1, 1, 1], 3).striated,
            1.0 / 3.0
        );
        assert_eq!(
            mst_features(&[[0.0, 0.0], [0.5, 0.0], [0.5, 0.5]], &[1, 1, 1], 3).striated,
            0.0
        );
    }

    #[test]
    fn stringy_separates_curved_arc_from_filled_grid() {
        // 10 points along a parabola (non-monotonic 1D curve).
        let arc: Vec<[f64; 2]> = (0..10)
            .map(|i| {
                let t = i as f64 / 9.0;
                [t, 4.0 * (t - 0.5).powi(2)]
            })
            .collect();
        let w_arc = vec![5; arc.len()];
        let arc_m = mst_features(&arc, &w_arc, MIN_GEOM_VERTICES);
        assert!(arc_m.stringy > 0.9, "arc stringy = {}", arc_m.stringy);

        // 10x10 uniform grid (2D filled cloud).
        let mut grid = Vec::new();
        for i in 0..10 {
            for j in 0..10 {
                grid.push([i as f64 / 9.0, j as f64 / 9.0]);
            }
        }
        let w_grid = vec![5; grid.len()];
        let grid_m = mst_features(&grid, &w_grid, MIN_GEOM_VERTICES);
        assert!(grid_m.stringy < 0.4, "grid stringy = {}", grid_m.stringy);
    }

    #[test]
    fn clumpy_separates_two_clusters_and_ignores_single_outlier() {
        let mut clusters = Vec::new();
        for &(cx, cy) in &[(0.1, 0.1), (0.85, 0.85)] {
            for dx in [-0.03, 0.0, 0.03] {
                for dy in [-0.03, 0.03] {
                    clusters.push([cx + dx, cy + dy]);
                }
            }
        }
        let weights = vec![10; clusters.len()];
        let m = mst_features(&clusters, &weights, MIN_GEOM_VERTICES);
        assert!(m.clumpy > 0.85, "two-cluster clumpy = {}", m.clumpy);

        // Single outlier far from a uniform cluster must NOT produce high clumpy.
        let mut with_outlier = Vec::new();
        for i in 0..4 {
            for j in 0..4 {
                with_outlier.push([i as f64 * 0.05, j as f64 * 0.05]);
            }
        }
        with_outlier.push([1.0, 1.0]);
        let mut w_out = vec![10; 16];
        w_out.push(1);
        let m_out = mst_features(&with_outlier, &w_out, MIN_GEOM_VERTICES);
        assert!(
            m_out.clumpy < 0.25,
            "single-outlier clumpy = {}",
            m_out.clumpy
        );
    }

    #[test]
    fn range_restriction_filters_sliced_parabola_but_keeps_opposite_slopes() {
        let mut tr_pts = Vec::new();
        let mut tr_grp = Vec::new();
        let mut ho_pts = Vec::new();
        let mut ho_grp = Vec::new();
        for i in 0..400 {
            let x = -1.0 + 2.0 * (i as f64 / 399.0);
            let y = x * x;
            let g = u8::from(x >= 0.0);
            if i % 2 == 0 {
                tr_pts.push([x, y]);
                tr_grp.push(g);
            } else {
                ho_pts.push([x, y]);
                ho_grp.push(g);
            }
        }
        assert!(is_range_restriction(&tr_pts, &tr_grp, &ho_pts, &ho_grp, 2));

        // Opposite slopes y = +x vs y = -x must NOT be filtered.
        let mut opp_tr = Vec::new();
        let mut opp_tg = Vec::new();
        let mut opp_ho = Vec::new();
        let mut opp_hg = Vec::new();
        for i in 0..400 {
            let g = (i % 2) as u8;
            let x = -1.0 + 2.0 * ((i / 2) as f64 / 199.0);
            let y = if g == 0 { x } else { -x };
            if (i / 2) % 2 == 0 {
                opp_tr.push([x, y]);
                opp_tg.push(g);
            } else {
                opp_ho.push([x, y]);
                opp_hg.push(g);
            }
        }
        assert!(!is_range_restriction(&opp_tr, &opp_tg, &opp_ho, &opp_hg, 2));
    }
}
