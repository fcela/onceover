use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::deadline::Deadline;

use super::metrics::{is_range_restriction, Cloud, Metric, Scale, Scratch, Shape, MAX_GROUPS};

pub const DEFAULT_ROW_CAP: usize = 4_096;
pub const DEFAULT_PAIR_CAP: usize = 16;
pub const DEFAULT_DESCRIPTOR_CAP: usize = 96;
pub const DEFAULT_RULE_CAP: usize = 128;

const MIN_GROUP: usize = 24;
const MIN_SHARE: usize = 10;
const STANDOUT_MIN_SHARE: usize = 20; // >= 5% of rows in each half (and >= MIN_GROUP)
const MIN_GAIN: f64 = 0.08;
const MIN_STANDOUT_SCORE: f64 = 0.45;
const MIN_STANDOUT_LIFT: f64 = 0.35;
const MIN_GEOM_STANDOUT_SCORE: f64 = 0.55;
const MIN_GEOM_STANDOUT_LIFT: f64 = 0.40;
const MIN_GEOM_STANDOUT_GROUP: usize = 32;
const EXTRA_LEAF_COST: f64 = 0.02;
const ALPHA: f64 = 0.05;
const HIGH_CARD_MAX_LEVELS: usize = 32;
const HIGH_CARD_TOP_K: usize = 12;

#[derive(Clone, Debug)]
pub struct SearchConfig {
    pub row_cap: usize,
    pub pair_cap: usize,
    pub descriptor_cap: usize,
    pub rule_cap: usize,
    pub deadline: Deadline,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            row_cap: DEFAULT_ROW_CAP,
            pair_cap: DEFAULT_PAIR_CAP,
            descriptor_cap: DEFAULT_DESCRIPTOR_CAP,
            rule_cap: DEFAULT_RULE_CAP,
            deadline: Deadline::none(),
        }
    }
}

#[derive(Default, Deserialize)]
pub struct Input {
    pub measures: BTreeMap<String, Vec<Option<f64>>>,
    #[serde(default)]
    pub categorical: BTreeMap<String, Vec<Option<String>>>,
    #[serde(default)]
    pub ordered: BTreeMap<String, Vec<Option<f64>>>,
    #[serde(default)]
    pub pairs: Option<Vec<[String; 2]>>,
    #[serde(default = "default_metrics")]
    pub metrics: Vec<Metric>,
    #[serde(default)]
    pub seed: u64,
}

pub fn default_metrics() -> Vec<Metric> {
    vec![
        Metric::Monotonic,
        Metric::Functional,
        Metric::BinnedStriated,
        Metric::BinnedStringy,
        Metric::BinnedClumpy,
    ]
}

/// Plan a bounded scan before extracting values from the source table.
pub struct ScanPlan {
    pairs: Vec<[String; 2]>,
    categorical: Vec<String>,
    ordered: Vec<String>,
    pair_total: usize,
    descriptors_total: usize,
}

impl ScanPlan {
    pub fn new(
        measures: &BTreeSet<String>,
        categorical: &BTreeSet<String>,
        ordered: &BTreeSet<String>,
        pairs: Option<&[[String; 2]]>,
    ) -> Result<Self, String> {
        Self::with_config(
            measures,
            categorical,
            ordered,
            pairs,
            None,
            &SearchConfig::default(),
        )
    }

    pub fn with_config(
        measures: &BTreeSet<String>,
        categorical: &BTreeSet<String>,
        ordered: &BTreeSet<String>,
        pairs: Option<&[[String; 2]]>,
        preferred_pairs: Option<&[[String; 2]]>,
        cfg: &SearchConfig,
    ) -> Result<Self, String> {
        if measures.len() < 2 {
            return Err("provide at least two numeric measures".into());
        }
        let pair_cap = cfg.pair_cap.max(1);
        let (pairs, pair_total) = if let Some(pairs) = pairs {
            let mut unique = BTreeSet::new();
            for [a, b] in pairs {
                if a == b || !measures.contains(a) || !measures.contains(b) {
                    return Err(format!(
                        "pair ({a:?}, {b:?}) must name two different numeric measures"
                    ));
                }
                let [a, b] = if a < b { [a, b] } else { [b, a] };
                if !unique.insert([a, b]) {
                    return Err("pairs must be distinct".into());
                }
            }
            (pairs.iter().take(pair_cap).cloned().collect(), pairs.len())
        } else if let Some(preferred) = preferred_pairs {
            let total = measures.len().saturating_mul(measures.len() - 1) / 2;
            let mut chosen = Vec::with_capacity(pair_cap.min(total));
            let mut seen = BTreeSet::new();
            for [a, b] in preferred {
                if chosen.len() >= pair_cap {
                    break;
                }
                if a == b || !measures.contains(a) || !measures.contains(b) {
                    continue;
                }
                let key = if a < b {
                    [a.clone(), b.clone()]
                } else {
                    [b.clone(), a.clone()]
                };
                if seen.insert(key) {
                    chosen.push([a.clone(), b.clone()]);
                }
            }
            if chosen.len() < pair_cap {
                let names: Vec<_> = measures.iter().collect();
                'outer: for (i, a) in names.iter().enumerate() {
                    for b in &names[i + 1..] {
                        if chosen.len() >= pair_cap {
                            break 'outer;
                        }
                        let pair = [(*a).clone(), (*b).clone()];
                        if seen.insert(pair.clone()) {
                            chosen.push(pair);
                        }
                    }
                }
            }
            (chosen, total)
        } else {
            let names: Vec<_> = measures.iter().collect();
            let total = names.len().saturating_mul(names.len() - 1) / 2;
            let pairs = names
                .iter()
                .enumerate()
                .flat_map(|(i, a)| {
                    names[i + 1..]
                        .iter()
                        .map(move |b| [(*a).clone(), (*b).clone()])
                })
                .take(pair_cap)
                .collect();
            (pairs, total)
        };
        let desc_cap = cfg.descriptor_cap;
        let categorical_cap = categorical
            .len()
            .min((desc_cap / 2).max(desc_cap.saturating_sub(ordered.len())));
        let ordered_cap = ordered.len().min(desc_cap.saturating_sub(categorical_cap));
        Ok(Self {
            pairs,
            categorical: categorical.iter().take(categorical_cap).cloned().collect(),
            ordered: ordered.iter().take(ordered_cap).cloned().collect(),
            pair_total,
            descriptors_total: categorical.len() + ordered.len(),
        })
    }

    pub fn measure_names(&self) -> impl Iterator<Item = &str> {
        self.pairs.iter().flatten().map(String::as_str)
    }

    pub fn categorical_names(&self) -> impl Iterator<Item = &str> {
        self.categorical.iter().map(String::as_str)
    }

    pub fn ordered_names(&self) -> impl Iterator<Item = &str> {
        self.ordered.iter().map(String::as_str)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Objective {
    /// Weighted average score gain across all leaves of the partition.
    PartitionGain,
    /// Exceptional shape lift concentrated in one binary subgroup.
    StandoutSubgroup,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Rule {
    /// Group i matches levels[i]. Everything else enters the final group.
    Levels { column: String, levels: Vec<String> },
    /// Finite x <= value enters group 0. Everything else enters group 1.
    Threshold { column: String, value: f64 },
    /// Group 2a + b, with 0 for equality and 1 otherwise for each component.
    Cross {
        columns: [String; 2],
        levels: [String; 2],
    },
}

struct Candidate {
    rule: Rule,
    labels: Vec<u8>,
    groups: usize,
    group_labels: Vec<String>,
}

impl Candidate {
    fn new(rule: Rule, group_labels: Vec<String>, input: &Input, rows: &[usize]) -> Self {
        let (labels, groups) = match &rule {
            Rule::Levels { column, levels } => {
                let values = &input.categorical[column];
                let labels = rows
                    .iter()
                    .map(|&row| {
                        levels
                            .iter()
                            .position(|level| Some(level) == values[row].as_ref())
                            .unwrap_or(levels.len()) as u8
                    })
                    .collect();
                (labels, levels.len() + 1)
            }
            Rule::Threshold { column, value } => {
                let values = &input.ordered[column];
                let labels = rows
                    .iter()
                    .map(|&row| {
                        u8::from(!values[row].is_some_and(|v| v.is_finite() && v <= *value))
                    })
                    .collect();
                (labels, 2)
            }
            Rule::Cross { columns, levels } => {
                let [a, b] = columns.each_ref().map(|name| &input.categorical[name]);
                let labels = rows
                    .iter()
                    .map(|&row| {
                        2 * u8::from(a[row].as_ref() != Some(&levels[0]))
                            + u8::from(b[row].as_ref() != Some(&levels[1]))
                    })
                    .collect();
                (labels, 4)
            }
        };
        Self {
            rule,
            labels,
            groups,
            group_labels,
        }
    }

    fn describes(&self, axis: &str) -> bool {
        match &self.rule {
            Rule::Levels { column, .. } | Rule::Threshold { column, .. } => column == axis,
            Rule::Cross { columns, .. } => columns.iter().any(|column| column == axis),
        }
    }
}

fn candidates(
    input: &Input,
    plan: &ScanPlan,
    rows: &[usize],
    split: usize,
    rule_cap: usize,
) -> (Vec<Candidate>, Coverage) {
    let mut rules: Vec<(Rule, Vec<String>)> = Vec::new();
    let mut alternatives: Vec<(Rule, Vec<String>)> = Vec::new();
    let mut binary: Vec<(&String, String, String)> = Vec::new();
    let mut skipped_high_cardinality = Vec::new();
    let min_one_vs_rest = MIN_GROUP.max(split.div_ceil(STANDOUT_MIN_SHARE));

    for name in &plan.categorical {
        let col = &input.categorical[name];
        let mut freq: FxHashMap<&str, usize> = FxHashMap::default();
        let mut has_null = false;
        let mut too_many = false;
        for &row in &rows[..split] {
            match col[row].as_deref() {
                Some(v) => {
                    if !freq.contains_key(v) && freq.len() >= HIGH_CARD_MAX_LEVELS {
                        too_many = true;
                        break;
                    }
                    *freq.entry(v).or_insert(0) += 1;
                }
                None => has_null = true,
            }
        }
        if too_many {
            skipped_high_cardinality.push(name.clone());
            continue;
        }
        let mut observed: Vec<String> = freq.keys().map(|&s| s.to_owned()).collect();
        observed.sort();
        match observed.len() {
            0 => (),
            1 => rules.push((
                Rule::Levels {
                    column: name.clone(),
                    levels: observed.clone(),
                },
                vec![observed[0].clone(), "other".into()],
            )),
            2 => {
                let other_label = if has_null {
                    format!("{}/null", observed[1])
                } else {
                    observed[1].clone()
                };
                rules.push((
                    Rule::Levels {
                        column: name.clone(),
                        levels: vec![observed[0].clone()],
                    },
                    vec![observed[0].clone(), other_label.clone()],
                ));
                binary.push((name, observed[0].clone(), other_label));
            }
            3..=MAX_GROUPS => {
                for level in &observed {
                    alternatives.push((
                        Rule::Levels {
                            column: name.clone(),
                            levels: vec![level.clone()],
                        },
                        vec![level.clone(), "other".into()],
                    ));
                }
                let mut full_labels = observed.clone();
                if has_null {
                    if let Some(last) = full_labels.last_mut() {
                        *last = format!("{last}/null");
                    }
                }
                rules.push((
                    Rule::Levels {
                        column: name.clone(),
                        levels: observed[..observed.len() - 1].to_vec(),
                    },
                    full_labels,
                ));
            }
            _ => {
                // Higher-cardinality (5..=32 levels): test top-K one-vs-rest splits
                // for levels with enough discovery support.
                let mut ranked: Vec<(usize, String)> = freq
                    .into_iter()
                    .filter(|&(_, cnt)| {
                        cnt >= min_one_vs_rest && split.saturating_sub(cnt) >= MIN_GROUP
                    })
                    .map(|(lvl, cnt)| (cnt, lvl.to_owned()))
                    .collect();
                ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                ranked.truncate(HIGH_CARD_TOP_K);
                if ranked.is_empty() {
                    skipped_high_cardinality.push(name.clone());
                } else {
                    let mut iter = ranked.into_iter().map(|(_, lvl)| {
                        let lbls = vec![lvl.clone(), "other".into()];
                        (
                            Rule::Levels {
                                column: name.clone(),
                                levels: vec![lvl],
                            },
                            lbls,
                        )
                    });
                    if let Some(first) = iter.next() {
                        rules.push(first);
                    }
                    alternatives.extend(iter);
                }
            }
        }
    }

    for name in &plan.ordered {
        let col = &input.ordered[name];
        let mut finite: Vec<_> = rows[..split]
            .iter()
            .filter_map(|&row| col[row])
            .filter(|x| x.is_finite())
            .collect();
        if finite.is_empty() {
            continue;
        }
        let mut cuts = Vec::new();
        for q in [0.5, 0.2, 0.35, 0.65, 0.8] {
            let index = (q * (finite.len() - 1) as f64) as usize;
            let (_, value, _) = finite.select_nth_unstable_by(index, f64::total_cmp);
            let value = *value;
            if !cuts.contains(&value) {
                cuts.push(value);
            }
        }
        let mut thresholds = cuts.into_iter().map(|value| {
            let fv = crate::fmt_float(value);
            (
                Rule::Threshold {
                    column: name.clone(),
                    value,
                },
                vec![format!("≤ {fv}"), format!("> {fv}")],
            )
        });
        rules.extend(thresholds.by_ref().take(1));
        alternatives.extend(thresholds);
    }

    let binary_crosses = binary.len().saturating_mul(binary.len().saturating_sub(1)) / 2;
    // Visit one rule per descriptor before considering variants of the same descriptor.
    rules.extend(alternatives);
    let main_total = rules.len();
    let rule_total = main_total + binary_crosses;
    // Keep some capacity for interactions even in frames with many main effects.
    let cross_cap = binary_crosses.min((rule_cap / 4).max(rule_cap.saturating_sub(main_total)));
    rules.truncate(rule_cap.saturating_sub(cross_cap));
    rules.extend(
        binary
            .iter()
            .enumerate()
            .flat_map(|(i, (a, la, oa))| {
                binary[i + 1..].iter().map(move |(b, lb, ob)| {
                    (
                        Rule::Cross {
                            columns: [(*a).clone(), (*b).clone()],
                            levels: [la.clone(), lb.clone()],
                        },
                        vec![
                            format!("{la} & {lb}"),
                            format!("{la} & {ob}"),
                            format!("{oa} & {lb}"),
                            format!("{oa} & {ob}"),
                        ],
                    )
                })
            })
            .take(cross_cap),
    );

    let coverage = Coverage {
        examined_rules: rules.len(),
        descriptors_total: plan.descriptors_total,
        examined_descriptors: plan.categorical.len() + plan.ordered.len(),
        rule_total,
        skipped_high_cardinality,
        ..Coverage::default()
    };
    (
        rules
            .into_iter()
            .map(|(rule, group_labels)| Candidate::new(rule, group_labels, input, rows))
            .collect(),
        coverage,
    )
}

#[derive(Default, Serialize)]
pub struct Coverage {
    pub input_rows: usize,
    pub sampled_rows: usize,
    pub pair_total: usize,
    pub examined_pairs: usize,
    pub descriptors_total: usize,
    pub examined_descriptors: usize,
    pub rule_total: usize,
    pub examined_rules: usize,
    skipped_high_cardinality: Vec<String>,
    pub scored_partitions: usize,
    pairs_without_supported_partition: usize,
    pub validation_family: usize,
    validation: ValidationCoverage,
    pub permutations_run: usize,
    pub deadline_stopped: bool,
    pub capped: bool,
}

#[derive(Default, Serialize)]
struct ValidationCoverage {
    insufficient_support: usize,
    undefined_metric: usize,
    below_gain: usize,
    range_restriction_filtered: usize,
    tested: usize,
}

#[derive(Serialize)]
pub struct ResultSet {
    pub coverage: Coverage,
    pub findings: Vec<Finding>,
}

#[derive(Clone, Serialize)]
pub struct ScoredPartition {
    pub pooled: Shape,
    pub groups: Vec<Group>,
    pub gain: f64,
}

#[derive(Clone, Serialize)]
pub struct Group {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub rows: usize,
    pub shape: Shape,
}

#[derive(Serialize)]
pub struct Finding {
    pub columns: [String; 2],
    pub metric: Metric,
    pub objective: Objective,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standout_group: Option<usize>,
    pub rule: Rule,
    pub discovery: ScoredPartition,
    pub validation: ScoredPartition,
    p_value: f64,
    pub adjusted_p_value: f64,
}

struct Proposal {
    candidate: usize,
    objective: Objective,
    standout_group: usize,
    sign_reversal: bool,
    selection_score: f64,
    discovery: ScoredPartition,
    train_labels: Vec<u8>,
}

struct SearchHalf {
    points: Vec<[f64; 2]>,
    cloud: Cloud,
    sample_positions: Vec<usize>,
    labels: Vec<u8>,
    scratch: Scratch,
}

impl SearchHalf {
    fn new(
        points: Vec<[f64; 2]>,
        sample_positions: Vec<usize>,
        scale: &Scale,
        metrics: &[Metric],
    ) -> Self {
        let n = points.len();
        let cloud = Cloud::new(&points, scale, metrics);
        Self {
            points,
            cloud,
            sample_positions,
            labels: vec![0; n],
            scratch: Scratch::new(n),
        }
    }

    fn pooled(&mut self, metric: Metric) -> Shape {
        self.labels.fill(0);
        self.cloud.score(metric, &self.labels, &mut self.scratch)[0]
    }

    fn assign_counts(&mut self, candidate: &Candidate) -> [usize; MAX_GROUPS] {
        let mut counts = [0; MAX_GROUPS];
        for (group, &pos) in self.labels.iter_mut().zip(&self.sample_positions) {
            *group = candidate.labels[pos];
            counts[*group as usize] += 1;
        }
        counts
    }

    fn partition_supported(&self, counts: [usize; MAX_GROUPS], groups: usize) -> bool {
        let min = MIN_GROUP.max(self.labels.len().div_ceil(MIN_SHARE));
        counts[..groups].iter().all(|&n| n >= min)
    }

    fn standout_supported(&self, counts: [usize; MAX_GROUPS], groups: usize) -> bool {
        if groups != 2 {
            return false;
        }
        let min_small = MIN_GROUP.max(self.labels.len().div_ceil(STANDOUT_MIN_SHARE));
        counts[0] >= min_small && counts[1] >= min_small
    }

    fn scores(&mut self, metric: Metric) -> [Shape; MAX_GROUPS] {
        self.cloud.score(metric, &self.labels, &mut self.scratch)
    }
}

fn weighted(scores: [Shape; MAX_GROUPS], counts: [usize; MAX_GROUPS], n: usize) -> f64 {
    scores
        .iter()
        .zip(counts)
        .map(|(s, n)| s.score * n as f64)
        .sum::<f64>()
        / n as f64
}

fn defined(metric: Metric, pooled: Shape, groups: &[Shape]) -> bool {
    metric != Metric::Monotonic || (pooled.rho.is_some() && groups.iter().all(|s| s.rho.is_some()))
}

fn evidence(
    pooled: Shape,
    scores: [Shape; MAX_GROUPS],
    counts: [usize; MAX_GROUPS],
    group_labels: &[String],
    gain: f64,
) -> ScoredPartition {
    let groups = scores
        .into_iter()
        .zip(counts)
        .enumerate()
        .take_while(|(_, (_, n))| *n > 0)
        .map(|(idx, (shape, rows))| Group {
            label: group_labels.get(idx).cloned(),
            rows,
            shape,
        })
        .collect();
    ScoredPartition {
        pooled,
        groups,
        gain,
    }
}

struct DiscoveredPair {
    columns: [String; 2],
    scale: Scale,
    train_points: Vec<[f64; 2]>,
    proposals: Vec<(Metric, Proposal)>,
    scored: usize,
    deadline_stopped: bool,
}

fn collect_points(
    input: &Input,
    pair: &[String; 2],
    rows: &[usize],
    offset: usize,
) -> (Vec<[f64; 2]>, Vec<usize>) {
    let [x, y] = pair.each_ref().map(|column| &input.measures[column]);
    rows.iter()
        .enumerate()
        .filter_map(|(pos, &row)| {
            let [x, y] = [x[row]?, y[row]?];
            (x.is_finite() && y.is_finite()).then_some(([x, y], offset + pos))
        })
        .unzip()
}

fn discover(
    input: &Input,
    columns: [String; 2],
    rows: &[usize],
    rules: &[Candidate],
    deadline: Deadline,
) -> DiscoveredPair {
    let (points, positions) = collect_points(input, &columns, rows, 0);
    let scale = Scale::fit(&points);
    let mut half = SearchHalf::new(points, positions, &scale, &input.metrics);
    let has_geom = input.metrics.iter().any(|m| m.is_geometric());
    let mut best: Vec<_> = input
        .metrics
        .iter()
        .map(|&m| (m, half.pooled(m), None::<Proposal>))
        .collect();
    let mut scored = 0;
    let mut deadline_stopped = false;

    for (i, candidate) in rules.iter().enumerate() {
        if i % 16 == 0 && deadline.exceeded() {
            deadline_stopped = true;
            break;
        }
        if columns.iter().any(|axis| candidate.describes(axis)) {
            continue;
        }
        let counts = half.assign_counts(candidate);
        let part_ok = half.partition_supported(counts, candidate.groups);
        let standout_ok = !part_ok && half.standout_supported(counts, candidate.groups);
        if !part_ok && !standout_ok {
            continue;
        }
        scored += 1;

        // Compute MST geometry at most once per candidate across all geometric metrics.
        let geom_cache = has_geom.then(|| half.cloud.score_geometry(&half.labels));

        for (metric, pooled, best_slot) in &mut best {
            let scores = if metric.is_geometric() {
                geom_cache.unwrap().map(|g| Shape {
                    score: g.get(*metric),
                    rho: None,
                })
            } else {
                half.scores(*metric)
            };
            if !defined(*metric, *pooled, &scores[..candidate.groups]) {
                continue;
            }
            let mut chosen: Option<(Objective, usize, bool, f64, f64)> = None;
            let (min_so_score, min_so_lift, min_so_rows) = if metric.is_geometric() {
                (
                    MIN_GEOM_STANDOUT_SCORE,
                    MIN_GEOM_STANDOUT_LIFT,
                    MIN_GEOM_STANDOUT_GROUP,
                )
            } else {
                (MIN_STANDOUT_SCORE, MIN_STANDOUT_LIFT, MIN_GROUP)
            };
            // Check both branches of a binary rule for an exceptional subgroup
            // (so alphabetical ordering like "False" < "True" cannot hide a minority),
            // including opposite-sign monotonic reversals (e.g. a 7% +slope minority
            // inside a 93% -slope majority).
            let standout_cand: Option<(usize, bool, f64)> =
                if candidate.groups == 2 && (standout_ok || part_ok) {
                    let mut best_so: Option<(usize, bool, f64)> = None;
                    for g_idx in 0..2 {
                        if counts[g_idx] < min_so_rows {
                            continue;
                        }
                        let s_g = scores[g_idx];
                        let lift = s_g.score - pooled.score;
                        let cand_hit = if s_g.score >= min_so_score && lift >= min_so_lift {
                            Some((g_idx, false, lift))
                        } else if *metric == Metric::Monotonic {
                            match (s_g.rho, pooled.rho) {
                                (Some(rg), Some(rp))
                                    if rg * rp < 0.0 && rg.abs() >= 0.60 && rp.abs() >= 0.20 =>
                                {
                                    Some((g_idx, true, rg.abs()))
                                }
                                _ => None,
                            }
                        } else {
                            None
                        };
                        if let Some(hit) = cand_hit {
                            if best_so.is_none_or(|prev| hit.2 > prev.2) {
                                best_so = Some(hit);
                            }
                        }
                    }
                    best_so
                } else {
                    None
                };
            if part_ok {
                let gain = weighted(scores, counts, half.labels.len()) - pooled.score;
                if gain >= MIN_GAIN {
                    // If a binary rule has strong concentrated lift in one subgroup
                    // but only marginal whole-partition weighted gain, score it as
                    // StandoutSubgroup so holdout tests that subgroup directly.
                    if let Some((g_idx, rev, lift)) = standout_cand {
                        if gain < 0.15 {
                            chosen =
                                Some((Objective::StandoutSubgroup, g_idx, rev, lift, lift * 0.5));
                        }
                    }
                    if chosen.is_none() {
                        let sel = gain - EXTRA_LEAF_COST * (candidate.groups - 2) as f64;
                        chosen = Some((Objective::PartitionGain, 0, false, gain, sel));
                    }
                }
            }
            if chosen.is_none() {
                if let Some((g_idx, rev, lift)) = standout_cand {
                    chosen = Some((Objective::StandoutSubgroup, g_idx, rev, lift, lift * 0.5));
                }
            }
            if let Some((objective, standout_group, sign_reversal, gain, selection_score)) = chosen
            {
                let improves = best_slot.as_ref().is_none_or(|previous| {
                    selection_score > previous.selection_score + 1e-12
                        || ((selection_score - previous.selection_score).abs() <= 1e-12
                            && candidate.groups < previous.discovery.groups.len())
                });
                if improves {
                    *best_slot = Some(Proposal {
                        candidate: i,
                        objective,
                        standout_group,
                        sign_reversal,
                        selection_score,
                        discovery: evidence(*pooled, scores, counts, &candidate.group_labels, gain),
                        train_labels: half.labels.clone(),
                    });
                }
            }
        }
    }

    let proposals: Vec<_> = best
        .into_iter()
        .filter_map(|(metric, _, winner)| Some((metric, winner?)))
        .collect();
    let keep_train_pts = !proposals.is_empty();
    DiscoveredPair {
        columns,
        scale,
        train_points: if keep_train_pts {
            half.points
        } else {
            Vec::new()
        },
        proposals,
        scored,
        deadline_stopped,
    }
}

#[allow(clippy::too_many_arguments)]
fn validate(
    input: &Input,
    pair: DiscoveredPair,
    rows: &[usize],
    split: usize,
    rules: &[Candidate],
    family: usize,
    seed: u64,
    deadline: Deadline,
) -> (Vec<Finding>, ValidationCoverage, usize, bool) {
    let mut coverage = ValidationCoverage::default();
    if pair.proposals.is_empty() {
        return (Vec::new(), coverage, 0, false);
    }
    let (points, positions) = collect_points(input, &pair.columns, &rows[split..], split);
    let metrics: Vec<_> = pair.proposals.iter().map(|(metric, _)| *metric).collect();
    let mut half = SearchHalf::new(points, positions, &pair.scale, &metrics);
    let mut rng = Rng(seed);
    let mut permutations_run = 0;
    let mut findings = Vec::new();
    let mut deadline_stopped = false;
    let permutations = (40 * family).max(199) - 1;

    for (metric, proposal) in pair.proposals {
        if deadline.exceeded() {
            deadline_stopped = true;
            break;
        }
        let pooled = half.pooled(metric);
        let candidate = &rules[proposal.candidate];
        let counts = half.assign_counts(candidate);
        let supported = match proposal.objective {
            Objective::PartitionGain => half.partition_supported(counts, candidate.groups),
            Objective::StandoutSubgroup => half.standout_supported(counts, candidate.groups),
        };
        if !supported {
            coverage.insufficient_support += 1;
            continue;
        }
        let scores = half.scores(metric);
        if !defined(metric, pooled, &scores[..candidate.groups]) {
            coverage.undefined_metric += 1;
            continue;
        }
        let (observed, gain) = match proposal.objective {
            Objective::PartitionGain => {
                let obs = weighted(scores, counts, half.labels.len());
                let g = obs - pooled.score;
                if g < MIN_GAIN {
                    coverage.below_gain += 1;
                    continue;
                }
                (obs, g)
            }
            Objective::StandoutSubgroup => {
                let g_idx = proposal.standout_group;
                let (min_so_score, min_so_lift, min_so_rows) = if metric.is_geometric() {
                    (
                        MIN_GEOM_STANDOUT_SCORE,
                        MIN_GEOM_STANDOUT_LIFT,
                        MIN_GEOM_STANDOUT_GROUP,
                    )
                } else {
                    (MIN_STANDOUT_SCORE, MIN_STANDOUT_LIFT, MIN_GROUP)
                };
                if counts[g_idx] < min_so_rows {
                    coverage.insufficient_support += 1;
                    continue;
                }
                if proposal.sign_reversal {
                    let (Some(rg), Some(rp)) = (scores[g_idx].rho, pooled.rho) else {
                        coverage.undefined_metric += 1;
                        continue;
                    };
                    let opp = -rp.signum() * rg;
                    if opp < 0.60 || rp.abs() < 0.20 {
                        coverage.below_gain += 1;
                        continue;
                    }
                    (opp, opp)
                } else {
                    let obs = scores[g_idx].score;
                    let lift = obs - pooled.score;
                    if obs < min_so_score || lift < min_so_lift {
                        coverage.below_gain += 1;
                        continue;
                    }
                    (obs, lift)
                }
            }
        };

        if !pair.train_points.is_empty()
            && !proposal.train_labels.is_empty()
            && is_range_restriction(
                &pair.train_points,
                &proposal.train_labels,
                &half.points,
                &half.labels,
                candidate.groups,
            )
        {
            coverage.range_restriction_filtered += 1;
            continue;
        }

        coverage.tested += 1;
        let mut exceed = 0;
        let mut completed_perms = 0usize;
        for step in 0..permutations {
            if step % 32 == 0 && deadline.exceeded() {
                deadline_stopped = true;
                break;
            }
            rng.shuffle(&mut half.labels);
            permutations_run += 1;
            completed_perms += 1;
            let perm_scores = half.scores(metric);
            let perm_stat = match proposal.objective {
                Objective::PartitionGain => weighted(perm_scores, counts, half.labels.len()),
                Objective::StandoutSubgroup if proposal.sign_reversal => {
                    let rp_sign = pooled.rho.unwrap_or(0.0).signum();
                    perm_scores[proposal.standout_group]
                        .rho
                        .map_or(0.0, |rg| (-rp_sign * rg).max(0.0))
                }
                Objective::StandoutSubgroup => perm_scores[proposal.standout_group].score,
            };
            if perm_stat >= observed - 1e-12 {
                exceed += 1;
                if (1 + exceed) as f64 * family as f64 / (1 + permutations) as f64 > ALPHA {
                    break;
                }
            }
        }
        if deadline_stopped && completed_perms < permutations {
            continue;
        }
        let p_value = (1 + exceed) as f64 / (1 + permutations) as f64;
        let adjusted = p_value * family as f64;
        if adjusted <= ALPHA {
            findings.push(Finding {
                columns: pair.columns.clone(),
                metric,
                objective: proposal.objective,
                standout_group: (proposal.objective == Objective::StandoutSubgroup)
                    .then_some(proposal.standout_group),
                rule: candidate.rule.clone(),
                discovery: proposal.discovery,
                validation: evidence(pooled, scores, counts, &candidate.group_labels, gain),
                p_value,
                adjusted_p_value: adjusted,
            });
        }
    }
    (findings, coverage, permutations_run, deadline_stopped)
}

pub fn run(input: &Input) -> Result<ResultSet, String> {
    let Some(n) = input.measures.values().next().map(Vec::len) else {
        return Err("provide at least two measures".into());
    };
    if input
        .measures
        .values()
        .chain(input.ordered.values())
        .any(|col| col.len() != n)
        || input.categorical.values().any(|col| col.len() != n)
    {
        return Err("all columns must have the same number of rows".into());
    }
    let cfg = SearchConfig::default();
    let plan = ScanPlan::with_config(
        &input.measures.keys().cloned().collect(),
        &input.categorical.keys().cloned().collect(),
        &input.ordered.keys().cloned().collect(),
        input.pairs.as_deref(),
        None,
        &cfg,
    )?;
    let rows = sample_rows_capped(n, input.seed, cfg.row_cap);
    run_on_rows(input, plan, n, &rows, &cfg)
}

/// Scan columns extracted in sample order from a larger source.
pub fn run_sampled(input: &Input, plan: ScanPlan, source_rows: usize) -> Result<ResultSet, String> {
    run_sampled_with_config(input, plan, source_rows, &SearchConfig::default())
}

/// Scan columns extracted in sample order using explicit budget and deadline controls.
pub fn run_sampled_with_config(
    input: &Input,
    plan: ScanPlan,
    source_rows: usize,
    cfg: &SearchConfig,
) -> Result<ResultSet, String> {
    let sampled = source_rows.min(cfg.row_cap);
    let has_rows = |values: Option<usize>| values == Some(sampled);
    if !plan
        .measure_names()
        .all(|name| has_rows(input.measures.get(name).map(Vec::len)))
        || !plan
            .categorical_names()
            .all(|name| has_rows(input.categorical.get(name).map(Vec::len)))
        || !plan
            .ordered_names()
            .all(|name| has_rows(input.ordered.get(name).map(Vec::len)))
    {
        return Err("sampled columns must match the scan plan and row count".into());
    }
    let rows: Vec<_> = (0..sampled).collect();
    run_on_rows(input, plan, source_rows, &rows, cfg)
}

fn run_on_rows(
    input: &Input,
    plan: ScanPlan,
    n: usize,
    rows: &[usize],
    cfg: &SearchConfig,
) -> Result<ResultSet, String> {
    let unique_metrics: FxHashSet<Metric> = input.metrics.iter().copied().collect();
    if input.metrics.is_empty() || unique_metrics.len() != input.metrics.len() {
        return Err("provide one to five distinct metrics".into());
    }
    let sampled = rows.len();
    let split = sampled / 2;
    let (rules, mut coverage) = candidates(input, &plan, rows, split, cfg.rule_cap);
    coverage.input_rows = n;
    coverage.sampled_rows = sampled;
    coverage.pair_total = plan.pair_total;
    let pairs = plan.pairs;
    coverage.examined_pairs = pairs.len();
    let discover_pair = |pair| discover(input, pair, &rows[..split], &rules, cfg.deadline);
    let discovered: Vec<_> = if pairs.len() < 2 {
        pairs.into_iter().map(discover_pair).collect()
    } else {
        pairs.into_par_iter().map(discover_pair).collect()
    };
    let mut deadline_stopped = discovered.iter().any(|p| p.deadline_stopped);
    coverage.scored_partitions = discovered.iter().map(|p| p.scored).sum();
    coverage.pairs_without_supported_partition =
        discovered.iter().filter(|p| p.scored == 0).count();
    let family = discovered.iter().map(|p| p.proposals.len()).sum();
    coverage.validation_family = family;
    let validate_pair = |pair: DiscoveredPair| {
        let seed = pair_seed(input.seed, &pair.columns);
        validate(input, pair, rows, split, &rules, family, seed, cfg.deadline)
    };
    let validated: Vec<_> = if discovered.len() < 2 {
        discovered.into_iter().map(validate_pair).collect()
    } else {
        discovered.into_par_iter().map(validate_pair).collect()
    };
    let mut findings = Vec::new();
    for (matches, validation, permutations, stopped) in validated {
        findings.extend(matches);
        coverage.validation.insufficient_support += validation.insufficient_support;
        coverage.validation.undefined_metric += validation.undefined_metric;
        coverage.validation.below_gain += validation.below_gain;
        coverage.validation.range_restriction_filtered += validation.range_restriction_filtered;
        coverage.validation.tested += validation.tested;
        coverage.permutations_run += permutations;
        deadline_stopped |= stopped;
    }

    // Suppress redundant `functional` when `monotonic` already survived on the
    // same pair (monotonicity is a more specific functional statement with signed rho),
    // suppress `binned_stringy` when `monotonic` or strong `binned_clumpy` already
    // survived on the same pair, and suppress `binned_striated` when `monotonic`
    // already explained the same pair with a tight monotone line (|rho| >= 0.85).
    let monotonic_pairs: FxHashSet<[String; 2]> = findings
        .iter()
        .filter(|f| f.metric == Metric::Monotonic)
        .map(|f| f.columns.clone())
        .collect();
    let suppressed_stringy_pairs: FxHashSet<[String; 2]> = findings
        .iter()
        .filter(|f| {
            f.metric == Metric::Monotonic
                || (f.metric == Metric::BinnedClumpy && f.validation.gain >= 0.25)
        })
        .map(|f| f.columns.clone())
        .collect();
    let tight_monotonic_pairs: FxHashSet<[String; 2]> = findings
        .iter()
        .filter(|f| {
            f.metric == Metric::Monotonic
                && f.validation
                    .groups
                    .iter()
                    .any(|g| g.shape.rho.is_some_and(|r| r.abs() >= 0.85))
        })
        .map(|f| f.columns.clone())
        .collect();
    if !monotonic_pairs.is_empty()
        || !suppressed_stringy_pairs.is_empty()
        || !tight_monotonic_pairs.is_empty()
    {
        findings.retain(|f| {
            !(f.metric == Metric::Functional && monotonic_pairs.contains(&f.columns))
                && !(f.metric == Metric::BinnedStringy
                    && suppressed_stringy_pairs.contains(&f.columns))
                && !(f.metric == Metric::BinnedStriated
                    && tight_monotonic_pairs.contains(&f.columns))
        });
    }

    coverage.deadline_stopped = deadline_stopped;
    coverage.capped = deadline_stopped
        || sampled < n
        || coverage.examined_pairs < coverage.pair_total
        || coverage.examined_descriptors < coverage.descriptors_total
        || rules.len() < coverage.rule_total;

    findings.sort_by(|a, b| {
        b.validation
            .gain
            .total_cmp(&a.validation.gain)
            .then_with(|| a.columns.cmp(&b.columns))
    });
    Ok(ResultSet { coverage, findings })
}

pub fn sample_rows(n: usize, seed: u64) -> Vec<usize> {
    sample_rows_capped(n, seed, DEFAULT_ROW_CAP)
}

pub fn sample_rows_capped(n: usize, seed: u64, row_cap: usize) -> Vec<usize> {
    let mut rng = Rng(seed);
    if n <= row_cap {
        let mut rows = (0..n).collect::<Vec<_>>();
        for i in 0..n {
            rows.swap(i, i + rng.below(n - i));
        }
        return rows;
    }
    // Floyd's sample uses O(cap) memory and expected O(cap) work.
    let mut selected = FxHashSet::default();
    selected.reserve(row_cap);
    let mut rows = Vec::with_capacity(row_cap);
    for end in n - row_cap..n {
        let row = rng.below(end + 1);
        if selected.insert(row) {
            rows.push(row);
        } else {
            selected.insert(end);
            rows.push(end);
        }
    }
    rng.shuffle(&mut rows);
    rows
}

fn pair_seed(seed: u64, columns: &[String; 2]) -> u64 {
    let [a, b] = if columns[0] <= columns[1] {
        [&columns[0], &columns[1]]
    } else {
        [&columns[1], &columns[0]]
    };
    let mut h: u64 = 0xcbf29ce484222325 ^ seed.wrapping_add(0x2a840488362d7709);
    for byte in a.as_bytes().iter().chain([&0xffu8]).chain(b.as_bytes()) {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub struct Rng(pub u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let x = self.0;
        let x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        let x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        x ^ (x >> 31)
    }

    pub fn below(&mut self, bound: usize) -> usize {
        let bound = bound as u64;
        loop {
            let product = self.next() as u128 * bound as u128;
            if product as u64 >= bound.wrapping_neg() % bound {
                return (product >> 64) as usize;
            }
        }
    }

    pub fn uniform(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn shuffle<T>(&mut self, values: &mut [T]) {
        for i in (1..values.len()).rev() {
            values.swap(i, self.below(i + 1));
        }
    }
}
