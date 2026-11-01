//! Ordered partitions: model-based recursive partitioning (MOB,
//! Zeileis/Hothorn/Hornik 2008) of a numeric response along an
//! ordering variable, with categorical dimensions competing
//! against the ordering variable at every node. Each node fits
//! `y ~ level + linear trend in t` by OLS. A node splits at the
//! candidate — a breakpoint along `t`, or a bipartition of a
//! dimension's levels ordered by their within-level parameter
//! contrast — whose two-segment fit reduces SSE the most,
//! provided the split clears two deterministic bars (the house
//! stopping pattern; no p-value gates anything): a relative-gain
//! materiality bar, and an F-statistic floor. The F floor is
//! what keeps the pass silent on noise: the best of ~n candidate
//! breakpoints on a pure-noise series still buys a few percent
//! of SSE by chance, so a gain bar alone recurses into noise.
//! Calibration (2,000 pure-noise series at n = 609, segment
//! floor 24, gain bar 2%): the largest noise F observed was
//! 11.96; an F floor of 10 admitted 3 of 2,000 series, a floor
//! of 15 admitted none. Genuine regime splits on panel-scale
//! data carry F in the tens to hundreds (a 15% SSE gain at
//! n = 609 is F ≈ 52). The accepted splits' F statistics are
//! reported in the evidence. Recursion stops at the depth and
//! leaf caps and at the minimum segment size, so the pass
//! reports regimes: subsets with a level and a slope a reader
//! can refit from the evidence alone.
//!
//! Spikes shorter than the segment floor are out of scope by
//! design — a segment model cannot price a brief excursion,
//! and the breaks and anomaly passes own that territory.

use std::collections::BTreeMap;

use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;

/// Configuration for the ordered-partitions pass. Budgets map to
/// these through `TierConfig` in the pipeline crate.
#[derive(Clone, Debug)]
pub struct PartitionsConfig {
    /// Maximum rows used per response, taken as a systematic
    /// (every k-th in `t` order) sample so break dates stay
    /// interpretable; the evidence states the resolution when a
    /// sample binds. 0 disables the pass.
    pub row_cap: usize,
    /// Minimum finite (t, y) pairs for a response to be screened.
    pub min_rows: usize,
    /// Minimum rows in any leaf segment or split side.
    pub min_segment: usize,
    /// Minimum relative SSE gain (against the node's own SSE)
    /// for a split to be accepted.
    pub min_gain: f64,
    /// Minimum F statistic (2 and n-4 df, split model against
    /// the node's single fit) for a split to be accepted. Guards
    /// against best-of-many-candidates noise fits.
    pub min_f: f64,
    /// Maximum tree depth (root = depth 0).
    pub max_depth: usize,
    /// Maximum leaves per tree.
    pub max_leaves: usize,
    /// Maximum responses screened per run; the rest are counted
    /// in coverage.
    pub max_responses: usize,
    /// Maximum findings emitted per run.
    pub max_findings: usize,
    /// Maximum categorical dimensions offered to the split
    /// search per response.
    pub max_dimensions: usize,
    /// Maximum levels a dimension may hold to be offered.
    pub max_levels: usize,
}

impl Default for PartitionsConfig {
    fn default() -> Self {
        Self {
            row_cap: 20_000,
            min_rows: 96,
            min_segment: 24,
            min_gain: 0.02,
            min_f: 15.0,
            max_depth: 3,
            max_leaves: 8,
            max_responses: 128,
            max_findings: 25,
            max_dimensions: 4,
            max_levels: 32,
        }
    }
}

impl PartitionsConfig {
    pub fn disabled() -> Self {
        Self {
            row_cap: 0,
            ..Self::default()
        }
    }
    pub fn is_enabled(&self) -> bool {
        self.row_cap > 0
    }
}

pub enum PartitionsStatus {
    NotApplicable(String),
    Ran(String),
    Capped(String),
}

pub struct PartitionsOutcome {
    pub findings: Vec<Finding>,
    pub status: PartitionsStatus,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    Date,
    Datetime,
    Integer,
}

fn display_time(axis: Axis, t: i64) -> String {
    match axis {
        Axis::Datetime => onceover_core::dateutil::iso_datetime(t),
        Axis::Date => onceover_core::dateutil::iso_date(t),
        Axis::Integer => t.to_string(),
    }
}

/// The ordering axis of the table: the hinted column when it is a
/// date, datetime, or integer column; else the first date/datetime
/// column; else a year-like integer column (every present value in
/// 1900..=2100, at least 8 distinct). Mirrors the breaks pass's
/// axis selection so the two passes read a table the same way.
fn select_axis(
    table: &TypedTable,
    time_hint: Option<&str>,
) -> Option<(usize, Axis, Vec<Option<i64>>)> {
    fn axis_of(col: &TypedCol) -> Option<Axis> {
        match col {
            TypedCol::Date { .. } => Some(Axis::Date),
            TypedCol::Datetime { .. } => Some(Axis::Datetime),
            TypedCol::I64 { .. } | TypedCol::U64 { .. } => Some(Axis::Integer),
            _ => None,
        }
    }
    // Per-row axis ticks, mirroring the breaks pass's `time_at`:
    // datetime ticks are normalized to microseconds because the
    // stored ticks retain the source unit.
    fn ticks(col: &TypedCol, n: usize) -> Vec<Option<i64>> {
        (0..n)
            .map(|r| match col {
                TypedCol::Date { vals } => vals[r].map(i64::from),
                TypedCol::Datetime { .. } => col.datetime_micros(r),
                TypedCol::I64 { vals } => vals.get(r).flatten(),
                TypedCol::U64 { vals } => vals.at(r).and_then(|v| i64::try_from(v).ok()),
                _ => None,
            })
            .collect()
    }
    let hinted = time_hint
        .and_then(|name| table.names.iter().position(|n| n == name))
        .and_then(|i| axis_of(&table.cols[i]).map(|a| (i, a)));
    let auto_date = table.cols.iter().enumerate().find_map(|(i, c)| match c {
        TypedCol::Date { .. } => Some((i, Axis::Date)),
        TypedCol::Datetime { .. } => Some((i, Axis::Datetime)),
        _ => None,
    });
    let year_like = table.cols.iter().enumerate().find_map(|(i, c)| match c {
        TypedCol::I64 { vals } => {
            let present: Vec<i64> = vals.iter().flatten().copied().collect();
            let mut distinct = present.clone();
            distinct.sort_unstable();
            distinct.dedup();
            let year_like = !present.is_empty()
                && distinct.len() >= 8
                && present.iter().all(|&v| (1900..=2100).contains(&v));
            year_like.then_some((i, Axis::Integer))
        }
        TypedCol::U64 { vals } => {
            let present: Vec<u64> = vals.iter().flatten().copied().collect();
            let mut distinct = present.clone();
            distinct.sort_unstable();
            distinct.dedup();
            let year_like = !present.is_empty()
                && distinct.len() >= 8
                && present.iter().all(|&v| (1900..=2100).contains(&v));
            year_like.then_some((i, Axis::Integer))
        }
        _ => None,
    });
    let (idx, axis) = hinted.or(auto_date).or(year_like)?;
    Some((idx, axis, ticks(&table.cols[idx], table.n)))
}

/// A categorical dimension of the table, as level ids per table
/// row. Rows with a missing value carry `u32::MAX`; a node that
/// contains such a row is not split on this dimension.
struct Dimension {
    name: String,
    levels: Vec<String>,
    level_of_row: Vec<u32>,
}

fn collect_dimensions(
    table: &TypedTable,
    axis_idx: usize,
    excluded: &[usize],
    cfg: &PartitionsConfig,
) -> Vec<Dimension> {
    let mut dims = Vec::new();
    for (i, col) in table.cols.iter().enumerate() {
        if dims.len() >= cfg.max_dimensions {
            break;
        }
        if i == axis_idx || excluded.contains(&i) {
            continue;
        }
        let cell: Vec<Option<String>> = match col {
            TypedCol::Str { vals } => vals.iter().map(|v| v.map(str::to_string)).collect(),
            TypedCol::StrOwned { vals } => vals.clone(),
            TypedCol::Bool { vals } => vals.iter().map(|v| v.map(|b| b.to_string())).collect(),
            _ => continue,
        };
        let mut levels: Vec<String> = Vec::new();
        let mut level_of_row: Vec<u32> = Vec::with_capacity(table.n);
        let mut ok = true;
        for v in &cell {
            match v {
                Some(s) => {
                    let id = match levels.iter().position(|l| l == s) {
                        Some(p) => p as u32,
                        None => {
                            if levels.len() >= cfg.max_levels {
                                ok = false;
                                break;
                            }
                            levels.push(s.clone());
                            (levels.len() - 1) as u32
                        }
                    };
                    level_of_row.push(id);
                }
                None => level_of_row.push(u32::MAX),
            }
        }
        if ok && levels.len() >= 2 {
            dims.push(Dimension {
                name: table.names[i].clone(),
                levels,
                level_of_row,
            });
        }
    }
    dims
}

/// Sufficient statistics for one OLS fit of y on (1, t).
#[derive(Clone, Copy, Default)]
struct Sums {
    n: f64,
    st: f64,
    stt: f64,
    sy: f64,
    sty: f64,
    syy: f64,
}

impl Sums {
    fn add(&mut self, t: f64, y: f64) {
        self.n += 1.0;
        self.st += t;
        self.stt += t * t;
        self.sy += y;
        self.sty += t * y;
        self.syy += y * y;
    }
    fn plus(&self, o: &Sums) -> Sums {
        Sums {
            n: self.n + o.n,
            st: self.st + o.st,
            stt: self.stt + o.stt,
            sy: self.sy + o.sy,
            sty: self.sty + o.sty,
            syy: self.syy + o.syy,
        }
    }
    fn minus(&self, o: &Sums) -> Sums {
        Sums {
            n: self.n - o.n,
            st: self.st - o.st,
            stt: self.stt - o.stt,
            sy: self.sy - o.sy,
            sty: self.sty - o.sty,
            syy: self.syy - o.syy,
        }
    }
    /// OLS fit: (SSE, intercept, slope). A segment whose `t` is
    /// constant falls back to the level-only fit.
    fn fit(&self) -> (f64, f64, f64) {
        if self.n <= 0.0 {
            return (0.0, 0.0, 0.0);
        }
        let det = self.n * self.stt - self.st * self.st;
        if det <= 1e-12 * self.n * self.stt.max(1.0) {
            let mean = self.sy / self.n;
            return ((self.syy - self.sy * mean).max(0.0), mean, 0.0);
        }
        let slope = (self.n * self.sty - self.st * self.sy) / det;
        let intercept = (self.sy - slope * self.st) / self.n;
        let sse = (self.syy - intercept * self.sy - slope * self.sty).max(0.0);
        (sse, intercept, slope)
    }
}

/// One response's data in `t` order, with each dimension's level
/// id per position.
struct SeriesData {
    t: Vec<f64>,
    ticks: Vec<i64>,
    y: Vec<f64>,
    /// (dimension index, level id) per position, one vector per
    /// dimension offered to this response.
    dim_levels: Vec<Vec<u32>>,
}

impl SeriesData {
    fn sums(&self, positions: &[usize]) -> Sums {
        let mut s = Sums::default();
        for &p in positions {
            s.add(self.t[p], self.y[p]);
        }
        s
    }
}

/// One fitted leaf of a tree.
#[derive(Clone, Debug)]
struct Leaf {
    /// Period labels of the leaf's first and last rows in `t`.
    period: [String; 2],
    n: usize,
    level_at_start: f64,
    slope: f64,
    sse: f64,
    /// Dimension conditions on the path from the root, e.g.
    /// `state in {nd, ok, tx, wy}`. Empty for a pure time tree.
    subset: Vec<String>,
}

/// What a split divides on.
#[derive(Clone, Debug)]
enum SplitKind {
    Time {
        at_label: String,
    },
    Dimension {
        dim: String,
        left_levels: Vec<String>,
        right_levels: Vec<String>,
    },
}

/// One accepted split, with its F statistic (2 and n-4 df
/// against the node's single-fit model) and the SSE reduction
/// it bought, as a share of the root SSE.
#[derive(Clone, Debug)]
struct Split {
    kind: SplitKind,
    f_stat: f64,
    gain_share: f64,
}

/// Keep the lower-SSE of the incumbent and a new candidate.
fn offer(best: &mut Option<Candidate>, cand: Option<Candidate>) {
    if let Some(c) = cand {
        if best.as_ref().is_none_or(|b| c.sse < b.sse) {
            *best = Some(c);
        }
    }
}

struct TreeBuilder<'a> {
    data: &'a SeriesData,
    dims: &'a [Dimension],
    cfg: &'a PartitionsConfig,
    axis: Axis,
    root_sse: f64,
    splits: Vec<Split>,
    leaves: Vec<Leaf>,
}

/// A candidate split: the two sides' row positions, in `t`
/// order, and their combined SSE.
struct Candidate {
    sse: f64,
    kind: SplitKind,
    left: Vec<usize>,
    right: Vec<usize>,
}

impl TreeBuilder<'_> {
    /// Best time split of a node whose positions are in `t`
    /// order, by two-segment SSE.
    fn best_time_split(&self, positions: &[usize]) -> Option<Candidate> {
        if positions.len() < 2 * self.cfg.min_segment {
            return None;
        }
        // Prefix sums over the node's positions.
        let m = positions.len();
        let mut prefix: Vec<Sums> = Vec::with_capacity(m + 1);
        prefix.push(Sums::default());
        for &p in positions {
            let mut s = *prefix.last().unwrap();
            s.add(self.data.t[p], self.data.y[p]);
            prefix.push(s);
        }
        let total = *prefix.last().unwrap();
        let mut best: Option<(usize, f64)> = None;
        for (i, pair) in prefix.windows(2).enumerate() {
            let k = i + 1; // left segment length
            if k < self.cfg.min_segment || k > m - self.cfg.min_segment {
                continue;
            }
            let sse = pair[1].fit().0 + total.minus(&pair[1]).fit().0;
            if best.is_none_or(|(_, b)| sse < b) {
                best = Some((k, sse));
            }
        }
        let (k, sse) = best?;
        Some(Candidate {
            sse,
            kind: SplitKind::Time {
                at_label: display_time(self.axis, self.data.ticks[positions[k]]),
            },
            left: positions[..k].to_vec(),
            right: positions[k..].to_vec(),
        })
    }

    /// Best bipartition of one dimension's levels within a
    /// node. Levels are ordered by their within-level fitted
    /// level at the node's `t` midpoint (then slope, then
    /// name); prefix bipartitions of that ordering are scanned.
    fn best_dim_split(&self, positions: &[usize], dim_idx: usize) -> Option<Candidate> {
        let dim = &self.dims[dim_idx];
        let level_of = &self.data.dim_levels[dim_idx];
        let n_levels = dim.levels.len();
        if n_levels < 2 {
            return None;
        }
        let mut per_level: Vec<Sums> = vec![Sums::default(); n_levels];
        let mut per_level_pos: Vec<Vec<usize>> = vec![Vec::new(); n_levels];
        for &p in positions {
            let lv = level_of[p];
            if lv == u32::MAX {
                // A row with no level on this dimension belongs
                // to neither side; rather than dropping it below
                // the split (which would break the SSE accounting
                // the bars are computed on), this dimension does
                // not compete at this node.
                return None;
            }
            per_level[lv as usize].add(self.data.t[p], self.data.y[p]);
            per_level_pos[lv as usize].push(p);
        }
        let node = self.data.sums(positions);
        let t_mid = if node.n > 0.0 { node.st / node.n } else { 0.0 };
        // Order levels by fitted level at t_mid, then slope,
        // then name. Levels with no rows in this node drop out.
        let mut order: Vec<usize> = (0..n_levels).filter(|&l| per_level[l].n >= 1.0).collect();
        if order.len() < 2 {
            return None;
        }
        order.sort_by(|&a, &b| {
            let (_, ia, sa) = per_level[a].fit();
            let (_, ib, sb) = per_level[b].fit();
            (ia + sa * t_mid)
                .total_cmp(&(ib + sb * t_mid))
                .then(sa.total_cmp(&sb))
                .then(dim.levels[a].cmp(&dim.levels[b]))
        });
        // Scan prefix bipartitions of the ordering; both sides
        // must hold at least min_segment rows.
        let mut left_sums = Sums::default();
        let mut best: Option<(usize, f64)> = None;
        for j in 0..(order.len() - 1) {
            left_sums = left_sums.plus(&per_level[order[j]]);
            let right_sums = node.minus(&left_sums);
            if (left_sums.n as usize) < self.cfg.min_segment
                || (right_sums.n as usize) < self.cfg.min_segment
            {
                continue;
            }
            let sse = left_sums.fit().0 + right_sums.fit().0;
            if best.is_none_or(|(_, b)| sse < b) {
                best = Some((j, sse));
            }
        }
        let (j, sse) = best?;
        let (left_levels, right_levels): (Vec<String>, Vec<String>) = (
            order[..=j].iter().map(|&l| dim.levels[l].clone()).collect(),
            order[j + 1..]
                .iter()
                .map(|&l| dim.levels[l].clone())
                .collect(),
        );
        let mut left: Vec<usize> = Vec::new();
        let mut right: Vec<usize> = Vec::new();
        for (idx, &l) in order.iter().enumerate() {
            if idx <= j {
                left.extend(&per_level_pos[l]);
            } else {
                right.extend(&per_level_pos[l]);
            }
        }
        left.sort_unstable();
        right.sort_unstable();
        Some(Candidate {
            sse,
            kind: SplitKind::Dimension {
                dim: dim.name.clone(),
                left_levels,
                right_levels,
            },
            left,
            right,
        })
    }

    /// Recurse over a node's positions (kept in `t` order).
    /// `subset` accumulates the dimension conditions on the
    /// path from the root.
    fn build(&mut self, positions: Vec<usize>, depth: usize, subset: Vec<String>) {
        let node = self.data.sums(&positions);
        let (sse_node, intercept, slope) = node.fit();
        let n = positions.len();
        let can_split = depth < self.cfg.max_depth
            && n >= 2 * self.cfg.min_segment
            && sse_node > 0.0
            && self.leaves.len() + 1 < self.cfg.max_leaves;
        if can_split {
            let mut best: Option<Candidate> = None;
            offer(&mut best, self.best_time_split(&positions));
            for d in 0..self.dims.len() {
                offer(&mut best, self.best_dim_split(&positions, d));
            }
            if let Some(cand) = best {
                let gain = (sse_node - cand.sse) / sse_node;
                let df2 = (n as f64 - 4.0).max(1.0);
                let f_stat = ((sse_node - cand.sse) / 2.0) / (cand.sse / df2);
                if gain >= self.cfg.min_gain && f_stat >= self.cfg.min_f {
                    self.splits.push(Split {
                        kind: cand.kind.clone(),
                        f_stat,
                        gain_share: if self.root_sse > 0.0 {
                            (sse_node - cand.sse) / self.root_sse
                        } else {
                            0.0
                        },
                    });
                    let (mut left_subset, mut right_subset) = (subset.clone(), subset.clone());
                    if let SplitKind::Dimension {
                        dim,
                        left_levels,
                        right_levels,
                    } = &cand.kind
                    {
                        left_subset.push(format!("{dim} in {{{}}}", left_levels.join(", ")));
                        right_subset.push(format!("{dim} in {{{}}}", right_levels.join(", ")));
                    }
                    self.build(cand.left, depth + 1, left_subset);
                    self.build(cand.right, depth + 1, right_subset);
                    return;
                }
            }
        }
        self.leaves.push(Leaf {
            period: [
                display_time(self.axis, self.data.ticks[positions[0]]),
                display_time(self.axis, self.data.ticks[positions[n - 1]]),
            ],
            n,
            level_at_start: intercept + slope * self.data.t[positions[0]],
            slope,
            sse: sse_node,
            subset,
        });
    }
}

/// Screen every numeric response against the table's ordering
/// axis (and its categorical dimensions) and return one finding
/// per response whose tree has at least one accepted split.
pub fn ordered_partitions(
    table: &TypedTable,
    excluded: &[usize],
    cfg: &PartitionsConfig,
    time_hint: Option<&str>,
) -> PartitionsOutcome {
    if !cfg.is_enabled() {
        return PartitionsOutcome {
            findings: Vec::new(),
            status: PartitionsStatus::NotApplicable("disabled at this budget".into()),
        };
    }
    let Some((axis_idx, axis, ticks)) = select_axis(table, time_hint) else {
        return PartitionsOutcome {
            findings: Vec::new(),
            status: PartitionsStatus::NotApplicable(
                "no date, datetime, or year-like integer column to order by".into(),
            ),
        };
    };
    let axis_name = table.names[axis_idx].clone();
    let dims = collect_dimensions(table, axis_idx, excluded, cfg);
    // t in f64: days for dates, days (from microsecond ticks)
    // for datetimes, the raw value for integers.
    let t_of = |tick: i64| -> f64 {
        match axis {
            Axis::Date | Axis::Integer => tick as f64,
            Axis::Datetime => tick as f64 / 86_400_000_000.0,
        }
    };
    let slope_unit_days = matches!(axis, Axis::Date | Axis::Datetime);

    let responses: Vec<usize> = (0..table.cols.len())
        .filter(|&i| i != axis_idx && !excluded.contains(&i) && table.cols[i].is_numeric())
        .collect();
    let n_responses = responses.len();
    let mut findings: Vec<Finding> = Vec::new();
    let mut screened = 0usize;
    let mut with_splits = 0usize;
    let mut capped = false;
    let mut sampled_any = false;

    for &rc in &responses {
        if screened >= cfg.max_responses {
            capped = true;
            break;
        }
        let col = &table.cols[rc];
        // (tick, t, y, table row) for rows where both the axis
        // and the response are present and finite.
        let mut rows: Vec<(i64, f64, f64, u32)> = Vec::new();
        for (row, tick) in ticks.iter().enumerate() {
            if let (Some(tick), Some(y)) = (*tick, crate::encode::numeric_at(col, row)) {
                if y.is_finite() {
                    rows.push((tick, t_of(tick), y, row as u32));
                }
            }
        }
        if rows.len() < cfg.min_rows {
            continue;
        }
        screened += 1;
        rows.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)).then(a.3.cmp(&b.3)));
        if rows.len() > cfg.row_cap {
            sampled_any = true;
            let pos = crate::stride_rows(rows.len(), cfg.row_cap);
            rows = pos.iter().map(|&p| rows[p as usize]).collect();
        }
        let data = SeriesData {
            t: rows.iter().map(|r| r.1).collect(),
            ticks: rows.iter().map(|r| r.0).collect(),
            y: rows.iter().map(|r| r.2).collect(),
            dim_levels: dims
                .iter()
                .map(|d| rows.iter().map(|r| d.level_of_row[r.3 as usize]).collect())
                .collect(),
        };
        let positions: Vec<usize> = (0..data.t.len()).collect();
        let (root_sse, _, _) = data.sums(&positions).fit();
        if root_sse <= 0.0 {
            continue;
        }
        let mut builder = TreeBuilder {
            data: &data,
            dims: &dims,
            cfg,
            axis,
            root_sse,
            splits: Vec::new(),
            leaves: Vec::new(),
        };
        builder.build(positions, 0, Vec::new());
        if builder.splits.is_empty() {
            continue;
        }
        with_splits += 1;
        let tree_sse: f64 = builder.leaves.iter().map(|l| l.sse).sum();
        let relative_gain = (root_sse - tree_sse) / root_sse;
        let y_name = table.names[rc].clone();
        let describe = |s: &Split| -> String {
            match &s.kind {
                SplitKind::Time { at_label } => format!("break at {at_label}"),
                SplitKind::Dimension {
                    dim,
                    left_levels,
                    right_levels,
                } => format!(
                    "{dim} split {{{}}} against {{{}}}",
                    left_levels.join(", "),
                    right_levels.join(", ")
                ),
            }
        };
        let first = &builder.splits[0];
        let claim = format!(
            "{y_name} follows {} level+trend regimes along {axis_name}: first split {}, \
             {} split{} in all, level+trend SSE reduced {:.1}%",
            builder.leaves.len(),
            describe(first),
            builder.splits.len(),
            if builder.splits.len() == 1 { "" } else { "s" },
            100.0 * relative_gain,
        );
        let splits_json: Vec<serde_json::Value> = builder
            .splits
            .iter()
            .map(|s| match &s.kind {
                SplitKind::Time { at_label } => json!({
                    "on": axis_name,
                    "at": at_label,
                    "f_statistic": s.f_stat,
                    "gain_share": s.gain_share,
                }),
                SplitKind::Dimension {
                    dim,
                    left_levels,
                    right_levels,
                } => json!({
                    "on": dim,
                    "left_levels": left_levels,
                    "right_levels": right_levels,
                    "f_statistic": s.f_stat,
                    "gain_share": s.gain_share,
                }),
            })
            .collect();
        let leaves_json: Vec<serde_json::Value> = builder
            .leaves
            .iter()
            .map(|l| {
                json!({
                    "period": l.period,
                    "subset": l.subset,
                    "n": l.n,
                    "level_at_period_start": l.level_at_start,
                    "slope_per_year": if slope_unit_days {
                        l.slope * 365.25
                    } else {
                        l.slope
                    },
                })
            })
            .collect();
        let metrics = BTreeMap::from([
            ("response".into(), json!(y_name)),
            ("ordering_column".into(), json!(axis_name)),
            (
                "model".into(),
                json!("level + linear trend in the ordering variable"),
            ),
            ("pooled_sse".into(), json!(root_sse)),
            ("tree_sse".into(), json!(tree_sse)),
            ("relative_gain".into(), json!(relative_gain)),
            ("splits".into(), json!(splits_json)),
            ("leaves".into(), json!(leaves_json)),
            ("n".into(), json!(data.t.len())),
            ("population_rows".into(), json!(table.n)),
        ]);
        let raw = BTreeMap::from([
            ("y".into(), json!(y_name)),
            ("time_column".into(), json!(axis_name)),
            ("model".into(), json!("level + linear trend")),
        ]);
        findings.push(Finding::new(
            Pattern::OrderedPartition,
            raw,
            claim,
            EvidenceBasis::Measured,
            0.30 + 0.55 * relative_gain.min(1.0),
            metrics,
            Some(format!(
                "a single level+trend line does not describe {y_name} over the whole span of \
                 {axis_name}; quote per-regime levels and slopes, or include the break dates \
                 as controls, instead of one pooled trend"
            )),
            Method::new(
                "ordered_partitions",
                "mob_level_trend",
                "Model-based recursive partitioning (Zeileis/Hothorn/Hornik 2008): per-node \
                 OLS of the response on a level and a linear trend in the ordering variable, \
                 best-SSE breakpoint or level-ordered dimension bipartition per node, \
                 deterministic relative-gain and F stopping bars; split F statistics are \
                 reported, no p-values",
            ),
        ));
    }

    findings.sort_by(|a, b| {
        b.interestingness
            .total_cmp(&a.interestingness)
            .then(a.claim.cmp(&b.claim))
    });
    let total_found = findings.len();
    findings.truncate(cfg.max_findings);
    let mut detail = format!(
        "{screened} numeric responses partitioned along {axis_name} ({} with accepted splits, \
         {total_found} findings{}); {} dimension(s) offered; minimum segment {} rows, \
         gain bar {:.0}%, F floor {:.0}",
        with_splits,
        if total_found > findings.len() {
            format!(", capped at {}", cfg.max_findings)
        } else {
            String::new()
        },
        dims.len(),
        cfg.min_segment,
        100.0 * cfg.min_gain,
        cfg.min_f,
    );
    if capped {
        detail.push_str(&format!(
            "; {} of {n_responses} responses screened (response cap {})",
            screened, cfg.max_responses
        ));
    }
    if sampled_any {
        detail.push_str(
            "; at least one response was systematically sampled to the row cap, \
             so its break dates carry that resolution",
        );
    }
    let status = if capped {
        PartitionsStatus::Capped(detail)
    } else {
        PartitionsStatus::Ran(detail)
    };
    PartitionsOutcome { findings, status }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> PartitionsConfig {
        PartitionsConfig {
            min_rows: 48,
            ..PartitionsConfig::default()
        }
    }

    fn series_data(ys: &[f64]) -> SeriesData {
        SeriesData {
            t: (0..ys.len()).map(|i| i as f64).collect(),
            ticks: (0..ys.len()).map(|i| i as i64).collect(),
            y: ys.to_vec(),
            dim_levels: Vec::new(),
        }
    }

    fn build_tree<'a>(
        data: &'a SeriesData,
        dims: &'a [Dimension],
        cfg: &'a PartitionsConfig,
    ) -> TreeBuilder<'a> {
        let positions: Vec<usize> = (0..data.t.len()).collect();
        let (root_sse, _, _) = data.sums(&positions).fit();
        let mut b = TreeBuilder {
            data,
            dims,
            cfg,
            axis: Axis::Integer,
            root_sse,
            splits: Vec::new(),
            leaves: Vec::new(),
        };
        b.build(positions, 0, Vec::new());
        b
    }

    #[test]
    fn segment_fit_recovers_level_and_slope() {
        // y = 2 + 0.5 t exactly.
        let ys: Vec<f64> = (0..100).map(|i| 2.0 + 0.5 * i as f64).collect();
        let s = series_data(&ys);
        let (sse, intercept, slope) = s.sums(&(0..100).collect::<Vec<_>>()).fit();
        assert!(sse < 1e-6, "sse {sse}");
        assert!((intercept - 2.0).abs() < 1e-9);
        assert!((slope - 0.5).abs() < 1e-9);
    }

    #[test]
    fn planted_level_break_is_found_at_the_right_index() {
        let mut seed = 11u64;
        let mut rnd = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 11) as f64) / ((1u64 << 53) as f64) - 0.5
        };
        let ys: Vec<f64> = (0..240)
            .map(|i| (if i >= 120 { 5.0 } else { 0.0 }) + 0.1 * rnd())
            .collect();
        let data = series_data(&ys);
        let c = cfg();
        let b = build_tree(&data, &[], &c);
        assert_eq!(b.splits.len(), 1, "splits: {}", b.splits.len());
        match &b.splits[0].kind {
            SplitKind::Time { at_label } => {
                let at: i64 = at_label.parse().unwrap();
                assert!((at - 120).abs() <= 2, "at {at}");
            }
            other => panic!("expected a time split, got {other:?}"),
        }
        assert_eq!(b.leaves.len(), 2);
        assert!((b.leaves[0].level_at_start - 0.0).abs() < 0.1);
        assert!((b.leaves[1].level_at_start - 5.0).abs() < 0.1);
    }

    #[test]
    fn planted_dimension_split_separates_the_groups() {
        // Two groups over the same t span: group b runs 4 units
        // above group a throughout. No time break exists; the
        // dimension split must win at the root.
        let mut seed = 7u64;
        let mut rnd = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 11) as f64) / ((1u64 << 53) as f64) - 0.5
        };
        let n_t = 200;
        let mut ys: Vec<f64> = Vec::new();
        let mut grp: Vec<u32> = Vec::new();
        for _i in 0..n_t {
            for g in 0..2u32 {
                ys.push(if g == 1 { 4.0 } else { 0.0 } + 0.1 * rnd());
                grp.push(g);
            }
        }
        let mut data = series_data(&ys);
        // series_data built t as row index; here t must repeat
        // per group: rebuild t/ticks so both groups share t.
        data.t = (0..ys.len()).map(|i| (i / 2) as f64).collect();
        data.ticks = (0..ys.len()).map(|i| (i / 2) as i64).collect();
        data.dim_levels = vec![grp];
        let dims = vec![Dimension {
            name: "grp".into(),
            levels: vec!["a".into(), "b".into()],
            level_of_row: Vec::new(),
        }];
        let c = cfg();
        let b = build_tree(&data, &dims, &c);
        assert_eq!(b.splits.len(), 1, "splits: {}", b.splits.len());
        match &b.splits[0].kind {
            SplitKind::Dimension {
                dim,
                left_levels,
                right_levels,
            } => {
                assert_eq!(dim, "grp");
                assert_eq!(left_levels.as_slice(), ["a".to_string()].as_slice());
                assert_eq!(right_levels.as_slice(), ["b".to_string()].as_slice());
            }
            other => panic!("expected a dimension split, got {other:?}"),
        }
        assert_eq!(b.leaves.len(), 2);
        assert_eq!(b.leaves[0].subset, vec!["grp in {a}".to_string()]);
        assert!((b.leaves[1].level_at_start - 4.0).abs() < 0.1);
    }

    #[test]
    fn noise_series_produces_no_splits() {
        let mut seed = 3u64;
        let mut rnd = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 11) as f64) / ((1u64 << 53) as f64) - 0.5
        };
        let ys: Vec<f64> = (0..240).map(|_| rnd()).collect();
        let data = series_data(&ys);
        let c = cfg();
        let b = build_tree(&data, &[], &c);
        assert!(b.splits.is_empty(), "noise split: {:?}", b.splits);
    }

    #[test]
    fn panel_scale_noise_produces_no_splits() {
        // The negative control at the panel's own scale: 40
        // pure-noise series of 609 points each must all stay
        // silent at the shipped bars. (Calibration over 2,000
        // such series put the largest noise F at 11.96, under
        // the 15.0 floor; see the module documentation.)
        for s in 0..40u64 {
            let mut seed = 1_000_003u64.wrapping_mul(s + 1);
            let mut rnd = move || {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                ((seed >> 11) as f64) / ((1u64 << 53) as f64) - 0.5
            };
            let ys: Vec<f64> = (0..609).map(|_| rnd()).collect();
            let data = series_data(&ys);
            let c = cfg();
            let b = build_tree(&data, &[], &c);
            assert!(b.splits.is_empty(), "seed {s}: {:?}", b.splits);
        }
    }
}
