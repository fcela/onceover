//! Bounded search for marginal shifts. Ordered targets use empirical CDF
//! gaps. Categorical targets use total variation and relative changes in
//! rare levels. Holdout tests the contrast frozen during discovery.

use crate::dep::quantile_sorted;
use crate::partition_scagnostics::sample_rows_capped;
use crate::slices::{bits_and, bits_count, bits_for, bits_hash, level_json, level_labels};
use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, PassInfo, Pattern, Sampling};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use crate::deadline::Deadline;

const ID: &str = "distribution_slices";
const MAX_LEVELS: usize = 25;
const MAX_CONDITION_COLUMNS: usize = 12;
const MAX_NUMERIC_TARGETS: usize = 24;
const MAX_CATEGORICAL_TARGETS: usize = 8;
const PROPOSALS_PER_TARGET: usize = 8;
const RARE_PROPOSALS_PER_TARGET: usize = 4;
const MAX_RARE_SHARE: f64 = 0.05;
const MIN_RARE_OCCURRENCES: usize = 8;
const MIN_RARE_RATIO: f64 = 2.0;
const MAX_FINDINGS: usize = 20;
const HALF_SUPPORT: usize = 25;
const GRID: usize = 33;
const GAP_FLOOR: f64 = 0.12;
const ALPHA: f64 = 0.05;

fn is_categorical(col: &TypedCol) -> bool {
    col.is_string() || matches!(col, TypedCol::Bool { .. })
}

fn category_at<'a>(col: &'a TypedCol, row: usize) -> Option<&'a str> {
    match col {
        TypedCol::Bool { vals } => vals[row].map(|v| if v { "true" } else { "false" }),
        _ => col.str_at(row),
    }
}

struct Condition {
    col: usize,
    op: &'static str,
    value: Value,
    text: String,
    bits: Vec<u64>,
}

impl Condition {
    fn matches(&self, table: &TypedTable, row: usize) -> bool {
        let col = &table.cols[self.col];
        match self.op {
            "le" => finite(col, row).is_some_and(|v| v <= self.value.as_f64().unwrap()),
            "gt" => finite(col, row).is_some_and(|v| v > self.value.as_f64().unwrap()),
            _ => match col {
                TypedCol::I64 { vals } => vals
                    .at(row)
                    .zip(self.value.as_i64())
                    .is_some_and(|(a, b)| a == b),
                TypedCol::U64 { vals } => vals
                    .at(row)
                    .zip(self.value.as_u64())
                    .is_some_and(|(a, b)| a == b),
                TypedCol::Bool { vals } => vals[row]
                    .zip(self.value.as_bool())
                    .is_some_and(|(a, b)| a == b),
                _ => col
                    .str_at(row)
                    .zip(self.value.as_str())
                    .is_some_and(|(a, b)| a == b),
            },
        }
    }
}

fn finite(col: &TypedCol, row: usize) -> Option<f64> {
    col.numeric_at(row).filter(|v| v.is_finite())
}

fn sorted_train(col: &TypedCol, train: &[u32]) -> Vec<f64> {
    let mut values: Vec<_> = train
        .iter()
        .filter_map(|&r| finite(col, r as usize))
        .collect();
    values.sort_unstable_by(f64::total_cmp);
    values
}

fn observed_quantile(sorted: &[f64], q: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * q).floor() as usize]
}

fn train_conditions(
    table: &TypedTable,
    train: &[u32],
    excluded: &[usize],
    min_rows: usize,
    deadline: Deadline,
) -> (Vec<Condition>, usize) {
    let mut categories = Vec::new();
    let mut thresholds = Vec::new();
    let mut skipped = 0;
    for (col, name) in table.names.iter().enumerate() {
        if deadline.exceeded() {
            break;
        }
        if excluded.contains(&col) {
            continue;
        }
        if let Some((labels, distinct)) = level_labels(table, col, train, MAX_LEVELS) {
            let mut conditions = Vec::new();
            for level in distinct {
                let bits = bits_for(train.len(), |i| labels[i].as_ref() == Some(&level));
                let n = bits_count(&bits);
                if n >= min_rows && train.len() - n >= min_rows {
                    conditions.push(Condition {
                        col,
                        op: "eq",
                        value: level_json(table, col, &level),
                        text: format!("{name} = {level:?}"),
                        bits,
                    });
                }
            }
            if conditions.is_empty() {
                continue;
            }
            if categories.len() < MAX_CONDITION_COLUMNS {
                categories.push(conditions);
            } else {
                skipped += 1;
            }
        } else if table.cols[col].is_numeric() {
            let values = sorted_train(&table.cols[col], train);
            if values.len() < 2 * min_rows || values.first() == values.last() {
                continue;
            }
            if thresholds.len() >= MAX_CONDITION_COLUMNS {
                skipped += 1;
                continue;
            }
            let mut conditions = Vec::new();
            let cuts = [0.125, 0.25, 0.5, 0.75, 0.875].map(|q| observed_quantile(&values, q));
            for (i, &cut) in cuts.iter().enumerate() {
                if i > 0 && cuts[i - 1] == cut {
                    continue;
                }
                for (op, symbol) in [("le", "<="), ("gt", ">")] {
                    let bits = bits_for(train.len(), |i| {
                        finite(&table.cols[col], train[i] as usize)
                            .is_some_and(|v| (v <= cut) == (op == "le"))
                    });
                    let n = bits_count(&bits);
                    if n >= min_rows && train.len() - n >= min_rows {
                        conditions.push(Condition {
                            col,
                            op,
                            value: json!(cut),
                            text: format!("{name} {symbol} {}", crate::fmt_float(cut)),
                            bits,
                        });
                    }
                }
            }
            if !conditions.is_empty() {
                thresholds.push(conditions);
            }
        }
    }
    (
        categories.into_iter().chain(thresholds).flatten().collect(),
        skipped,
    )
}

#[derive(Clone, Copy)]
enum Contrast {
    Threshold(f64),
    Levels(u32),
    RareLevel(usize),
}

#[derive(Clone, Copy)]
struct Rates {
    inside: f64,
    outside: f64,
}

impl Rates {
    fn new(inside: usize, outside: usize, inside_n: usize, outside_n: usize) -> Self {
        Self {
            inside: inside as f64 / inside_n as f64,
            outside: outside as f64 / outside_n as f64,
        }
    }

    fn gap(self) -> f64 {
        self.inside - self.outside
    }
}

/// Effect floor for a rare level, applied independently to both halves.
fn rare_change(rates: Rates, counts: [usize; 2], sizes: [usize; 2]) -> bool {
    let high_side = usize::from(rates.outside > rates.inside);
    let pooled = (counts[0] + counts[1]) as f64 / (sizes[0] + sizes[1]) as f64;
    counts[high_side] >= MIN_RARE_OCCURRENCES
        && pooled <= MAX_RARE_SHARE
        && rates.inside.max(rates.outside) >= MIN_RARE_RATIO * rates.inside.min(rates.outside)
}

/// Squared phi coefficient for the two-by-two table of region and level membership.
fn rare_gain(rates: Rates, share: f64) -> f64 {
    let pooled = share * rates.inside + (1.0 - share) * rates.outside;
    share * (1.0 - share) * rates.gap().powi(2) / (pooled * (1.0 - pooled))
}

#[derive(Clone, Copy)]
struct Proposal {
    // A singleton repeats its index. A conjunction uses increasing indices.
    rule: [usize; 2],
    partition: u64,
    gain: f64,
    contrast: Contrast,
    rates: Rates,
    inside_n: usize,
    outside_n: usize,
}

enum TargetKind {
    Numeric(Vec<f64>),
    Categorical(Vec<String>),
}

impl TargetKind {
    fn width(&self) -> usize {
        match self {
            Self::Numeric(cuts) => cuts.len(),
            Self::Categorical(levels) => levels.len(),
        }
    }
}

struct Target {
    col: usize,
    n: usize,
    kind: TargetKind,
    bins: Vec<u8>,
    histogram: [usize; GRID + 1],
    proposals: Vec<Proposal>,
    rare_proposals: Vec<Proposal>,
}

impl Target {
    fn train_numeric(
        col: usize,
        table: &TypedTable,
        train: &[u32],
        min_rows: usize,
    ) -> Option<Self> {
        let sorted = sorted_train(&table.cols[col], train);
        if sorted.len() < 2 * min_rows || sorted.first() == sorted.last() {
            return None;
        }
        let max = *sorted.last().unwrap();
        let mut cuts: Vec<f64> = [0.01]
            .into_iter()
            .chain((1..32).map(|i| i as f64 / 32.0))
            .chain([0.99])
            .map(|q| observed_quantile(&sorted, q))
            .filter(|&cut| cut < max)
            .collect();
        cuts.dedup();
        let mut histogram = [0; GRID + 1];
        let bins = train
            .iter()
            .map(|&row| {
                finite(&table.cols[col], row as usize).map_or(u8::MAX, |y| {
                    let bin = cuts.partition_point(|&cut| cut < y);
                    histogram[bin] += 1;
                    bin as u8
                })
            })
            .collect();
        Some(Self {
            col,
            n: sorted.len(),
            kind: TargetKind::Numeric(cuts),
            bins,
            histogram,
            proposals: Vec::new(),
            rare_proposals: Vec::new(),
        })
    }

    /// Err signals a target above the cardinality bound.
    fn train_categorical(
        col: usize,
        table: &TypedTable,
        train: &[u32],
        min_rows: usize,
    ) -> Result<Option<Self>, ()> {
        let column = &table.cols[col];
        let mut distinct = BTreeSet::new();
        let mut n = 0;
        for level in train
            .iter()
            .filter_map(|&r| category_at(column, r as usize))
        {
            distinct.insert(level);
            if distinct.len() > MAX_LEVELS {
                return Err(());
            }
            n += 1;
        }
        if n < 2 * min_rows || distinct.len() < 2 {
            return Ok(None);
        }
        let levels: Vec<_> = distinct.into_iter().map(str::to_owned).collect();
        let mut histogram = [0; GRID + 1];
        let bins = train
            .iter()
            .map(|&r| {
                category_code(column, r as usize, &levels).map_or(u8::MAX, |code| {
                    histogram[code] += 1;
                    code as u8
                })
            })
            .collect();
        Ok(Some(Self {
            col,
            n,
            kind: TargetKind::Categorical(levels),
            bins,
            histogram,
            proposals: Vec::new(),
            rare_proposals: Vec::new(),
        }))
    }

    fn score(&mut self, rule: [usize; 2], partition: u64, bits: &[u64], min_rows: usize) {
        let mut counts = [0; GRID + 1];
        for (i, &word) in bits.iter().enumerate() {
            let mut word = word;
            while word != 0 {
                let pos = i * 64 + word.trailing_zeros() as usize;
                word &= word - 1;
                let bin = self.bins[pos];
                if bin != u8::MAX {
                    counts[bin as usize] += 1;
                }
            }
        }
        let inside_n: usize = counts.iter().sum();
        let outside_n = self.n - inside_n;
        if inside_n < min_rows || outside_n < min_rows {
            return;
        }
        let share = inside_n as f64 / self.n as f64;
        let proposal = |contrast, rates, gain| Proposal {
            rule,
            partition,
            gain,
            contrast,
            rates,
            inside_n,
            outside_n,
        };
        let (contrast, rates, distance_squared) = match &self.kind {
            TargetKind::Numeric(cuts) => {
                let mut inside_cum = 0;
                let mut outside_cum = 0;
                let mut squared = 0.0;
                let mut best = Rates {
                    inside: 0.0,
                    outside: 0.0,
                };
                let mut threshold = cuts[0];
                for (idx, &cut) in cuts.iter().enumerate() {
                    inside_cum += counts[idx];
                    outside_cum += self.histogram[idx] - counts[idx];
                    let rates = Rates::new(inside_cum, outside_cum, inside_n, outside_n);
                    let gap = rates.gap();
                    squared += gap * gap;
                    if gap.abs() > best.gap().abs() {
                        best = rates;
                        threshold = cut;
                    }
                }
                (
                    Contrast::Threshold(threshold),
                    best,
                    squared / cuts.len() as f64,
                )
            }
            TargetKind::Categorical(levels) => {
                // The set of positive mass differences attains total variation.
                let (mut mask, mut inside, mut outside) = (0u32, 0, 0);
                for (i, &slice_count) in counts[..levels.len()].iter().enumerate() {
                    let rest_count = self.histogram[i] - slice_count;
                    let rates = Rates::new(slice_count, rest_count, inside_n, outside_n);
                    if rates.gap().abs() < GAP_FLOOR
                        && rare_change(rates, [slice_count, rest_count], [inside_n, outside_n])
                    {
                        offer_proposal(
                            &mut self.rare_proposals,
                            proposal(Contrast::RareLevel(i), rates, rare_gain(rates, share)),
                            RARE_PROPOSALS_PER_TARGET,
                        );
                    }
                    if slice_count * outside_n > rest_count * inside_n {
                        mask |= 1 << i;
                        inside += slice_count;
                        outside += rest_count;
                    }
                }
                let rates = Rates::new(inside, outside, inside_n, outside_n);
                (Contrast::Levels(mask), rates, rates.gap().powi(2))
            }
        };
        if rates.gap().abs() >= GAP_FLOOR {
            offer_proposal(
                &mut self.proposals,
                proposal(contrast, rates, share * (1.0 - share) * distance_squared),
                PROPOSALS_PER_TARGET,
            );
        }
    }
}

fn offer_proposal(shortlist: &mut Vec<Proposal>, p: Proposal, cap: usize) {
    // Equivalent and complementary rules name the same comparison.
    // For rare-level proposals, distinct levels remain distinct contrasts.
    if shortlist.iter().any(|other| {
        other.partition == p.partition
            && match (other.contrast, p.contrast) {
                (Contrast::RareLevel(a), Contrast::RareLevel(b)) => a == b,
                _ => true,
            }
    }) {
        return;
    }
    let index = shortlist.partition_point(|other| other.gain >= p.gain);
    if index < cap {
        shortlist.insert(index, p);
        shortlist.truncate(cap);
    }
}

fn log_factorials(n: usize) -> Vec<f64> {
    let mut log_fact = vec![0.0; n + 1];
    for k in 1..=n {
        log_fact[k] = log_fact[k - 1] + (k as f64).ln();
    }
    log_fact
}

/// Exact one-sided conditional test of the frozen contrast.
fn upper_tail(inside: usize, outside: usize, n: usize, m: usize, fact: &[f64]) -> f64 {
    let total = inside + outside;
    let choose = |n: usize, k: usize| fact[n] - fact[k] - fact[n - k];
    let denom = choose(n + m, total);
    (inside..=n.min(total))
        .map(|k| (choose(n, k) + choose(m, total - k) - denom).exp())
        .sum::<f64>()
        .min(1.0)
}

/// Codes from the discovery vocabulary. New holdout levels share one
/// bucket outside the selected set. Missing values have no code.
fn category_code(col: &TypedCol, row: usize, levels: &[String]) -> Option<usize> {
    let value = category_at(col, row)?;
    Some(
        levels
            .binary_search_by(|label| label.as_str().cmp(value))
            .unwrap_or(levels.len()),
    )
}

enum HoldoutDetails {
    Numeric([Vec<f64>; 2]),
    Categorical(Box<[[usize; MAX_LEVELS + 1]; 2]>),
}

struct Holdout {
    inside_n: usize,
    outside_n: usize,
    rates: Rates,
    p: f64,
    details: HoldoutDetails,
}

impl Holdout {
    fn effect(&self, contrast: Contrast) -> f64 {
        if matches!(contrast, Contrast::RareLevel(_)) {
            let share = self.inside_n as f64 / (self.inside_n + self.outside_n) as f64;
            rare_gain(self.rates, share).sqrt()
        } else {
            self.rates.gap().abs()
        }
    }
}

fn confirm(
    target: &Target,
    rule: &[&Condition],
    table: &TypedTable,
    holdout: &[usize],
    p: &Proposal,
    fact: &[f64],
) -> Option<Holdout> {
    let column = &table.cols[target.col];
    let side = |r| usize::from(!rule.iter().all(|c| c.matches(table, r)));
    let (n, counts, details) = match (&target.kind, p.contrast) {
        (TargetKind::Numeric(_), Contrast::Threshold(cut)) => {
            let mut values = [Vec::new(), Vec::new()];
            for &r in holdout {
                if let Some(y) = finite(column, r) {
                    values[side(r)].push(y);
                }
            }
            let n = [values[0].len(), values[1].len()];
            let count = |side: &[f64]| side.iter().filter(|&&y| y <= cut).count();
            let counts = [count(&values[0]), count(&values[1])];
            (n, counts, HoldoutDetails::Numeric(values))
        }
        (
            TargetKind::Categorical(levels),
            contrast @ (Contrast::Levels(_) | Contrast::RareLevel(_)),
        ) => {
            let mut histograms = [[0; MAX_LEVELS + 1]; 2];
            for &r in holdout {
                if let Some(code) = category_code(column, r, levels) {
                    histograms[side(r)][code] += 1;
                }
            }
            let n = histograms.each_ref().map(|hist| hist.iter().sum());
            let counts = histograms.each_ref().map(|hist| match contrast {
                Contrast::RareLevel(i) => hist[i],
                Contrast::Levels(mask) => hist[..levels.len()]
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| mask & (1 << i) != 0)
                    .map(|(_, count)| count)
                    .sum(),
                _ => unreachable!(),
            });
            (n, counts, HoldoutDetails::Categorical(Box::new(histograms)))
        }
        _ => unreachable!("the proposal is built by its target"),
    };
    let [inside_n, outside_n] = n;
    if inside_n.min(outside_n) < HALF_SUPPORT {
        return None;
    }
    let [a, b] = counts;
    let rates = Rates::new(a, b, inside_n, outside_n);
    let directional_gap = rates.gap() * p.rates.gap().signum();
    let replicates = if matches!(p.contrast, Contrast::RareLevel(_)) {
        directional_gap > 0.0 && rare_change(rates, counts, n)
    } else {
        directional_gap >= GAP_FLOOR
    };
    if !replicates {
        return None;
    }
    let (a, b) = if p.rates.gap() < 0.0 {
        (inside_n - a, outside_n - b)
    } else {
        (a, b)
    };
    Some(Holdout {
        inside_n,
        outside_n,
        rates,
        p: upper_tail(a, b, inside_n, outside_n, fact),
        details,
    })
}

fn finding(
    table: &TypedTable,
    target: &Target,
    rule: &[&Condition],
    p: Proposal,
    mut h: Holdout,
    adjusted_p: f64,
    family: usize,
) -> Finding {
    let pct = |x| format!("{:.1}%", 100.0 * x);
    let gap = h.rates.gap().abs();
    let name = &table.names[target.col];
    let text = rule
        .iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join(" and ");
    let conditions = rule
        .iter()
        .map(|c| {
            json!({
                "column": table.names[c.col], "op": c.op, "value": c.value,
            })
        })
        .collect::<Vec<_>>();
    let quantiles = |values: &mut Vec<f64>| {
        values.sort_unstable_by(f64::total_cmp);
        [0.1, 0.25, 0.5, 0.75, 0.9].map(|q| quantile_sorted(values, q))
    };
    let (form, event, search, validation, algorithm, method) =
        match (&target.kind, p.contrast, &mut h.details) {
            (TargetKind::Numeric(_), Contrast::Threshold(cut), HoldoutDetails::Numeric(values)) => (
                "marginal_distribution_shift",
                format!("at or below {}", crate::fmt_float(cut)),
                json!({
                    "slice_cdf": p.rates.inside, "rest_cdf": p.rates.outside,
                }),
                json!({
                    "slice_cdf": h.rates.inside, "rest_cdf": h.rates.outside, "cdf_gap": gap,
                    "quantile_levels": [0.1, 0.25, 0.5, 0.75, 0.9],
                    "slice_quantiles": quantiles(&mut values[0]), "rest_quantiles": quantiles(&mut values[1]),
                }),
                "quantile_grid_cdf_slices",
                "empirical CDF discovery with a frozen-threshold Fisher exact test on held-out rows and Holm correction",
            ),
            (TargetKind::Categorical(levels), Contrast::Levels(mask), HoldoutDetails::Categorical(hist)) => {
                let selected = |i: usize| mask & (1u32 << i) != 0;
                let selected_levels: Vec<_> = levels.iter().enumerate().filter(|(i, _)| selected(*i))
                    .map(|(_, level)| level_json(table, target.col, level)).collect();
                let first_levels = selected_levels.iter().take(3).map(Value::to_string)
                    .collect::<Vec<_>>().join(", ");
                let event = if selected_levels.len() > 3 {
                    format!("in the {} selected levels (including {first_levels})", selected_levels.len())
                } else {
                    format!("in {{{first_levels}}}")
                };
                let level_counts = levels.iter().enumerate().map(|(i, level)| json!({
                    "level": level_json(table, target.col, level), "in_selected_set": selected(i),
                    "slice_count": hist[0][i], "rest_count": hist[1][i],
                })).collect::<Vec<_>>();
                (
                    "categorical_distribution_shift",
                    event,
                    json!({
                        "slice_share": p.rates.inside, "rest_share": p.rates.outside,
                        "total_variation": p.rates.gap(),
                    }),
                    json!({
                        "slice_share": h.rates.inside, "rest_share": h.rates.outside, "share_gap": gap,
                        "levels": level_counts, "selected_levels": selected_levels,
                        "slice_unseen_count": hist[0][levels.len()],
                        "rest_unseen_count": hist[1][levels.len()],
                    }),
                    "total_variation_slices",
                    "total variation discovery with a frozen-level-set Fisher exact test on held-out rows and Holm correction",
                )
            }
            (TargetKind::Categorical(levels), Contrast::RareLevel(i), HoldoutDetails::Categorical(hist)) => {
                let level = level_json(table, target.col, &levels[i]);
                let stats = |rates: Rates, n_inside, n_outside| {
                    let pooled = (rates.inside * n_inside as f64 + rates.outside * n_outside as f64)
                        / (n_inside + n_outside) as f64;
                    let low = rates.inside.min(rates.outside);
                    let ratio = (low > 0.0).then(|| rates.inside.max(rates.outside) / low);
                    json!({
                        "slice_share": rates.inside, "rest_share": rates.outside,
                        "pooled_share": pooled, "share_ratio": ratio,
                        "share_gap": rates.gap().abs(),
                    })
                };
                let mut holdout_stats = stats(h.rates, h.inside_n, h.outside_n);
                holdout_stats["slice_count"] = json!(hist[0][i]);
                holdout_stats["rest_count"] = json!(hist[1][i]);
                (
                    "rare_level_shift",
                    format!("equal to {level}"),
                    stats(p.rates, p.inside_n, p.outside_n),
                    holdout_stats,
                    "rare_level_slices",
                    "relative change in a rare categorical level with a frozen-level Fisher exact test on held-out rows and Holm correction",
                )
            }
            _ => unreachable!("the validation follows its proposal"),
        };
    let claim = format!(
        "within {text}, {} of held-out {name} values are {event}, versus {} in the rest ({} vs {} values)",
        pct(h.rates.inside), pct(h.rates.outside), h.inside_n, h.outside_n,
    );
    let with_counts = |mut values: Value, slice_n, rest_n| {
        values["slice_n"] = json!(slice_n);
        values["rest_n"] = json!(rest_n);
        values
    };
    let mut columns = vec![name.clone()];
    columns.extend(rule.iter().map(|c| table.names[c.col].clone()));
    let mut metrics = BTreeMap::from([
        ("form".into(), json!(form)),
        ("conditions".into(), json!(conditions)),
        ("search_gain".into(), json!(p.gain)),
        ("n_train".into(), json!(p.inside_n + p.outside_n)),
        ("n_holdout".into(), json!(h.inside_n + h.outside_n)),
        ("slice_rows".into(), json!(p.inside_n + h.inside_n)),
        (
            "search".into(),
            with_counts(search, p.inside_n, p.outside_n),
        ),
        (
            "holdout".into(),
            with_counts(validation, h.inside_n, h.outside_n),
        ),
        ("p_value".into(), json!(h.p)),
        ("p_adjusted".into(), json!(adjusted_p)),
        ("confirmation_family".into(), json!(family)),
    ]);
    match (&target.kind, p.contrast) {
        (_, Contrast::Threshold(cut)) => {
            metrics.insert("threshold".into(), json!(cut));
        }
        (TargetKind::Categorical(levels), Contrast::RareLevel(i)) => {
            metrics.insert(
                "target_level".into(),
                level_json(table, target.col, &levels[i]),
            );
        }
        _ => {}
    }
    Finding::new(
        Pattern::SliceShift,
        BTreeMap::from([
            ("columns".into(), json!(columns)),
            ("column".into(), json!(name)),
            ("conditions".into(), json!(conditions)),
            ("form".into(), json!(form)),
        ]),
        claim,
        EvidenceBasis::Estimated,
        0.5 + 0.45 * h.effect(p.contrast),
        metrics,
        Some("Confirmation assumes exchangeable rows. Repeated entities or serial dependence can make the p-value too small.".into()),
        Method::new(ID, algorithm, method),
    )
}

pub fn profile_distribution_slices(
    table: &TypedTable,
    excluded: &[usize],
    designated_target: Option<usize>,
    row_cap: usize,
    deep: bool,
    deadline: Deadline,
) -> (Vec<Finding>, PassInfo) {
    let sample = sample_rows_capped(table.n, 0x5b43_24e4_2747_185e, row_cap);
    if sample.len() < 100 {
        return (
            Vec::new(),
            PassInfo::na("fewer than 100 rows on the sample"),
        );
    }
    let (train, holdout) = sample.split_at(sample.len() / 2);
    let train: Vec<u32> = train.iter().map(|&r| r as u32).collect();
    let min_rows = HALF_SUPPORT.max(train.len().div_ceil(100));
    let (conditions, condition_columns_capped) =
        train_conditions(table, &train, excluded, min_rows, deadline);
    let mut targets = Vec::new();
    let mut target_cols = (0..table.cols.len())
        .filter(|&col| {
            (table.cols[col].is_numeric() || is_categorical(&table.cols[col]))
                && (Some(col) == designated_target || !excluded.contains(&col))
        })
        .collect::<Vec<_>>();
    target_cols.sort_by_key(|&col| Some(col) != designated_target);
    let (mut numeric, mut categorical) = (0, 0);
    let (mut numeric_capped, mut categorical_capped, mut cardinality_capped) = (0, 0, 0);
    for col in target_cols {
        if deadline.exceeded() {
            break;
        }
        if table.cols[col].is_numeric() {
            if numeric == MAX_NUMERIC_TARGETS {
                numeric_capped += 1;
            } else if let Some(target) = Target::train_numeric(col, table, &train, min_rows) {
                numeric += 1;
                targets.push(target);
            }
        } else if categorical == MAX_CATEGORICAL_TARGETS {
            categorical_capped += 1;
        } else {
            match Target::train_categorical(col, table, &train, min_rows) {
                Ok(Some(target)) => {
                    categorical += 1;
                    targets.push(target);
                }
                Ok(None) => {}
                Err(()) => cardinality_capped += 1,
            }
        }
    }
    if deadline.exceeded() {
        return (
            Vec::new(),
            PassInfo::capped("deadline reached during preparation; no rules examined"),
        );
    }
    if conditions.is_empty() || targets.is_empty() {
        return (
            Vec::new(),
            if cardinality_capped > 0 {
                PassInfo::capped(format!(
                    "no eligible comparisons; {cardinality_capped} categorical targets exceed {MAX_LEVELS} discovery levels"
                ))
            } else {
                PassInfo::na("no supported context rules or nonconstant targets")
            },
        );
    }

    let rules = (0..conditions.len())
        .map(|i| [i, i])
        .chain((0..conditions.len()).flat_map(|i| {
            let conditions = &conditions;
            (i + 1..conditions.len())
                .filter(move |&j| conditions[i].col != conditions[j].col)
                .map(move |j| [i, j])
        }));
    let total_rules = rules.clone().count();
    let all_train = bits_for(train.len(), |_| true);
    let work_cap = if deep { 100_000_000 } else { 30_000_000 };
    let (mut work, mut examined, mut scored) = (0, 0, 0);
    'search: for [i, j] in rules {
        let a = &conditions[i];
        let b = &conditions[j];
        if deadline.exceeded() || work + all_train.len() > work_cap {
            break;
        }
        work += all_train.len();
        let paired;
        let bits = if i == j {
            &a.bits
        } else {
            paired = bits_and(&a.bits, &b.bits);
            &paired
        };
        let n = bits_count(bits);
        if n >= min_rows && train.len() - n >= min_rows {
            let partition = if bits[0] & 1 == 0 {
                bits_hash(bits)
            } else {
                let opposite: Vec<_> = bits.iter().zip(&all_train).map(|(b, t)| t ^ b).collect();
                bits_hash(&opposite)
            };
            for target in &mut targets {
                if target.col == a.col || target.col == b.col {
                    continue;
                }
                let cost = n + bits.len() + target.kind.width();
                if work + cost > work_cap {
                    break 'search;
                }
                work += cost;
                target.score([i, j], partition, bits, min_rows);
                scored += 1;
            }
        }
        examined += 1;
    }
    let rare_family: usize = targets.iter().map(|t| t.rare_proposals.len()).sum();
    let family = rare_family + targets.iter().map(|t| t.proposals.len()).sum::<usize>();
    let fact = log_factorials(holdout.len());
    let mut confirmed = Vec::new();
    let mut tested = 0;
    'validation: for target in &targets {
        for &p in target.proposals.iter().chain(&target.rare_proposals) {
            if deadline.exceeded() {
                break 'validation;
            }
            let rule = p.rule[..if p.rule[0] == p.rule[1] { 1 } else { 2 }]
                .iter()
                .map(|&i| &conditions[i])
                .collect::<Vec<_>>();
            if let Some(h) = confirm(target, &rule, table, holdout, &p, &fact) {
                confirmed.push((target, rule, p, h));
            }
            tested += 1;
        }
    }
    confirmed.sort_by(|a, b| a.3.p.total_cmp(&b.3.p));
    let mut adjusted = 0.0_f64;
    let mut passed = Vec::new();
    for (rank, (target, rule, p, h)) in confirmed.into_iter().enumerate() {
        // Unsupported, unreplicated and untested proposals count as p=1.
        adjusted = adjusted.max(h.p * (family - rank) as f64);
        if adjusted > ALPHA {
            break;
        }
        passed.push((target, rule, p, h, adjusted));
    }
    let accepted = passed.len();
    passed.sort_by(|a, b| {
        (Some(b.0.col) == designated_target)
            .cmp(&(Some(a.0.col) == designated_target))
            .then_with(|| {
                b.3.effect(b.2.contrast)
                    .total_cmp(&a.3.effect(a.2.contrast))
            })
            .then_with(|| a.1.len().cmp(&b.1.len()))
    });
    let mut findings = passed
        .into_iter()
        .take(MAX_FINDINGS)
        .map(|(target, rule, p, h, adjusted)| finding(table, target, &rule, p, h, adjusted, family))
        .collect::<Vec<_>>();
    for item in &mut findings {
        item.evidence[0].scope.sampling = if sample.len() < table.n {
            Sampling::Sample
        } else {
            Sampling::Full
        };
    }
    let detail = format!(
        "{examined} of {total_rules} slice rules fully examined, {scored} rule × target comparisons; \
        {numeric} numeric and {categorical} categorical targets, {} context predicates; \
        {condition_columns_capped} condition columns capped, {numeric_capped} numeric target columns capped, \
        {categorical_capped} categorical target columns capped; {cardinality_capped} categorical targets above {MAX_LEVELS} levels; \
        {} discovery and {} holdout rows of {} input rows; {tested} of {family} frozen proposals checked \
        ({rare_family} rare-level proposals; caps {PROPOSALS_PER_TARGET} distribution and \
        {RARE_PROPOSALS_PER_TARGET} rare-level proposals per target); {accepted} passed Holm at {ALPHA}, {} stated (cap {MAX_FINDINGS}); \
        {work} of {work_cap} search work units",
        conditions.len(),
        train.len(), holdout.len(), table.n, findings.len(),
    );
    let coverage = if examined < total_rules
        || tested < family
        || condition_columns_capped > 0
        || numeric_capped + categorical_capped + cardinality_capped > 0
        || accepted > MAX_FINDINGS
    {
        let reason = if deadline.exceeded() {
            "; deadline reached"
        } else {
            ""
        };
        PassInfo::capped(format!("{detail}{reason}"))
    } else if sample.len() < table.n {
        PassInfo::sampled(detail)
    } else {
        PassInfo::ran(detail)
    };
    (findings, coverage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_upper_tail_counts_rare_tied_values() {
        let fact = log_factorials(8);
        // Eight rows, four on each side and four successes.
        // There are 70 placements. For >=3 inside, 16 have 3 and 1 has 4.
        let p = upper_tail(3, 1, 4, 4, &fact);
        assert!((p - 17.0 / 70.0).abs() < 1e-12);
        assert!((upper_tail(4, 0, 4, 4, &fact) - 1.0 / 70.0).abs() < 1e-12);
    }
}
