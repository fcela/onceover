//! Changes in an entity's categorical mix along a resolved axis.

use std::collections::BTreeMap;

use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;

use crate::fingerprints::{pct, MIN_ENTITIES};
use crate::Deadline;

const MAX_PERIODS: usize = 24;
const WINDOW: usize = 6;
const MIN_ROWS: u32 = 12;
const MIN_CHANGE: f64 = 0.20;
const MAX_EXAMPLES: usize = 100;

pub struct FingerprintPeriods {
    pub name: String,
    labels: Vec<String>,
    period: Vec<Option<usize>>,
    coarsened: bool,
}

impl FingerprintPeriods {
    /// Retain an already discrete axis. For finer axes use the existing
    /// equal-support period builder, which never splits tied keys.
    pub fn new(name: String, keys: Vec<f64>, label: impl Fn(f64) -> String) -> Self {
        let mut distinct = Vec::new();
        for &key in keys.iter().filter(|v| v.is_finite()) {
            if !distinct.contains(&key) {
                distinct.push(key);
                if distinct.len() > MAX_PERIODS {
                    break;
                }
            }
        }
        let coarsened = distinct.len() > MAX_PERIODS;
        let mut period = vec![None; keys.len()];
        let labels = if coarsened {
            let (ranges, order) = crate::drift::build_periods(&keys, MAX_PERIODS, 1);
            ranges
                .iter()
                .enumerate()
                .map(|(p, &(start, end))| {
                    for &row in &order[start..end] {
                        period[row as usize] = Some(p);
                    }
                    label(keys[order[start] as usize])
                })
                .collect()
        } else {
            distinct.sort_by(f64::total_cmp);
            for (&key, slot) in keys.iter().zip(&mut period) {
                if key.is_finite() {
                    *slot = distinct.iter().position(|&v| v == key);
                }
            }
            distinct.into_iter().map(label).collect()
        };
        Self {
            name,
            labels,
            period,
            coarsened,
        }
    }

    pub(crate) fn detail(&self) -> String {
        format!(
            "row-count drift along {}: {} {} [{}]",
            self.name,
            self.labels.len(),
            if self.coarsened {
                "equal-support periods"
            } else {
                "distinct periods"
            },
            self.labels.join(", ")
        )
    }
}

// Conditional on the two window sizes and the level total, the
// before-count is hypergeometric under exchangeable period labels.
// Double the tail in the observed direction for a conservative
// two-sided exact p-value. Stop summing once it cannot pass the bar.
fn permutation_p(before: u32, after: u32, n: u32, m: u32, log_fact: &[f64], bar: f64) -> f64 {
    let total = before + after;
    let choose =
        |n: u32, k: u32| log_fact[n as usize] - log_fact[k as usize] - log_fact[(n - k) as usize];
    let denom = choose(n + m, total);
    let high = u64::from(before) * u64::from(m) > u64::from(after) * u64::from(n);
    let end = if high {
        n.min(total)
    } else {
        total.saturating_sub(m)
    };
    let mut tail = 0.0;
    let mut k = before;
    loop {
        tail += (choose(n, k) + choose(m, total - k) - denom).exp();
        if 2.0 * tail > bar || k == end {
            return (2.0 * tail).min(1.0);
        }
        if high {
            k += 1;
        } else {
            k -= 1;
        }
    }
}

#[derive(Clone, Copy)]
struct Shift {
    cut: usize,
    level: usize,
    before: f64,
    after: f64,
    peer_before: f64,
    peer_after: f64,
    n_before: u32,
    n_after: u32,
    p: f64,
}

pub(crate) struct DriftInput<'a> {
    pub periods: &'a FingerprintPeriods,
    pub entity: &'a str,
    pub dimension: &'a str,
    pub entity_labels: &'a [String],
    pub level_labels: &'a [String],
    pub entities: &'a [usize],
    pub entity_codes: &'a [Option<u32>],
    pub level_codes: &'a [Option<u32>],
    pub dimension_count: usize,
    pub deadline: Deadline,
}

pub(crate) fn scan(input: DriftInput<'_>) -> (Vec<Finding>, String, bool) {
    let DriftInput {
        periods,
        entity,
        dimension,
        entity_labels,
        level_labels,
        entities,
        entity_codes,
        level_codes,
        dimension_count,
        deadline,
    } = input;
    let n_periods = periods.labels.len();
    if n_periods < 2 {
        return (
            vec![],
            "drift skipped: fewer than two usable periods".into(),
            false,
        );
    }
    let l = level_labels.len();
    let stride = l * (n_periods + 1);
    let mut entities = entities.to_vec();
    entities.sort_by_key(|&e| &entity_labels[e]);
    let mut selected = vec![None; entity_labels.len()];
    for (i, &e) in entities.iter().enumerate() {
        selected[e] = Some(i);
    }
    let mut prefix = vec![0u32; stride * entities.len()];
    for ((e, j), p) in entity_codes.iter().zip(level_codes).zip(&periods.period) {
        if let (Some(i), Some(j), Some(p)) = (e.and_then(|e| selected[e as usize]), j, p) {
            prefix[i * stride + (p + 1) * l + *j as usize] += 1;
        }
    }
    let mut max_rows = 0;
    for row in prefix.chunks_exact_mut(stride) {
        for i in l..stride {
            row[i] += row[i - l];
        }
        max_rows = max_rows.max(row[stride - l..].iter().sum::<u32>());
    }
    let mut log_fact = vec![0.0; max_rows as usize + 1];
    for k in 1..log_fact.len() {
        log_fact[k] = log_fact[k - 1] + (k as f64).ln();
    }
    let bar = 0.05 / ((n_periods - 1) * l * entities.len() * dimension_count) as f64;
    let mut picks: Vec<Vec<Shift>> = vec![Vec::new(); entities.len()];
    let mut global: Option<(Shift, usize, usize)> = None;
    let mut completed = 0;
    let mut tested = vec![0u32; entities.len()];
    let mut shared_cuts = Vec::new();
    for cut in 1..n_periods {
        if deadline.exceeded() {
            break;
        }
        let left = cut.saturating_sub(WINDOW) * l;
        let mid = cut * l;
        let right = (cut + WINDOW).min(n_periods) * l;
        let mut matched = Vec::new();
        let mut shares = vec![(0.0, 0.0, 0, 0); l];
        for (i, row) in prefix.chunks_exact(stride).enumerate() {
            let n: u32 = (0..l).map(|j| row[mid + j] - row[left + j]).sum();
            let m: u32 = (0..l).map(|j| row[right + j] - row[mid + j]).sum();
            if n < MIN_ROWS || m < MIN_ROWS {
                continue;
            }
            matched.push((i, row, n, m));
            for (j, (b, a, up, down)) in shares.iter_mut().enumerate() {
                let before = f64::from(row[mid + j] - row[left + j]) / f64::from(n);
                let after = f64::from(row[right + j] - row[mid + j]) / f64::from(m);
                *b += before;
                *a += after;
                *up += usize::from(after - before >= 0.10);
                *down += usize::from(before - after >= 0.10);
            }
        }
        completed += 1;
        let n_matched = matched.len();
        if n_matched < MIN_ENTITIES {
            continue;
        }
        let population_shift = shares
            .iter()
            .enumerate()
            .filter_map(|(j, &(b, a, up, down))| {
                let (before, after) = (b / n_matched as f64, a / n_matched as f64);
                let concordant = if after > before { up } else { down };
                ((after - before).abs() >= 0.15 && 3 * concordant >= 2 * n_matched).then_some((
                    Shift {
                        cut,
                        level: j,
                        before,
                        after,
                        peer_before: 0.0,
                        peer_after: 0.0,
                        n_before: 0,
                        n_after: 0,
                        p: 1.0,
                    },
                    concordant,
                    n_matched,
                ))
            })
            .max_by(|(a, _, _), (b, _, _)| {
                (a.after - a.before)
                    .abs()
                    .total_cmp(&(b.after - b.before).abs())
                    .then_with(|| level_labels[b.level].cmp(&level_labels[a.level]))
            });
        if let Some(candidate) = population_shift {
            shared_cuts.push(cut);
            if global.is_none_or(|(g, _, _)| {
                (candidate.0.after - candidate.0.before).abs() > (g.after - g.before).abs()
            }) {
                global = Some(candidate);
            }
            // A compositional change can move every other share as
            // well. Do not interpret this window as individual drift.
            continue;
        }
        for (i, row, n, m) in matched {
            tested[i] |= 1 << cut;
            let mut best: Option<Shift> = None;
            for (j, &(b, a, _, _)) in shares.iter().enumerate() {
                let (x, y) = (row[mid + j] - row[left + j], row[right + j] - row[mid + j]);
                let (before, after) = (f64::from(x) / f64::from(n), f64::from(y) / f64::from(m));
                let peer_before = (b - before) / (n_matched - 1) as f64;
                let peer_after = (a - after) / (n_matched - 1) as f64;
                let change = after - before;
                // Require the entity's own move to exceed the
                // contemporaneous peer move in its direction.
                if change.abs() < MIN_CHANGE
                    || change.abs() - change.signum() * (peer_after - peer_before) < MIN_CHANGE
                {
                    continue;
                }
                let p = permutation_p(x, y, n, m, &log_fact, bar);
                if p <= bar
                    && best.is_none_or(|s| {
                        p < s.p || (p == s.p && level_labels[j] < level_labels[s.level])
                    })
                {
                    best = Some(Shift {
                        cut,
                        level: j,
                        before,
                        after,
                        peer_before,
                        peer_after,
                        n_before: n,
                        n_after: m,
                        p,
                    });
                }
            }
            picks[i].extend(best);
        }
    }

    let stopped = completed < n_periods - 1;
    // Incomplete screens cannot establish the strongest boundary or
    // rule out a shared change in a later comparison.
    if stopped {
        return (
            vec![],
            format!(
                "drift capped: {completed} of {} boundaries tested",
                n_periods - 1
            ),
            true,
        );
    }
    // Windows that overlap a detected population break can also
    // contain the same shift, diluted across more periods.
    let excluded_mask = (1..n_periods)
        .filter(|&cut| {
            shared_cuts.iter().any(|&shared| {
                cut.saturating_sub(WINDOW) < shared && shared < (cut + WINDOW).min(n_periods)
            })
        })
        .fold(0u32, |mask, cut| mask | (1 << cut));
    let n_tested = tested
        .into_iter()
        .filter(|mask| mask & !excluded_mask != 0)
        .count();
    let mut found: Vec<_> = picks
        .into_iter()
        .enumerate()
        .filter_map(|(i, shifts)| {
            shifts
                .into_iter()
                .filter(|s| excluded_mask & (1 << s.cut) == 0)
                .min_by(|a, b| a.p.total_cmp(&b.p).then_with(|| a.cut.cmp(&b.cut)))
                .map(|s| (i, s))
        })
        .collect();
    found.sort_by(|a, b| {
        a.1.p
            .total_cmp(&b.1.p)
            .then_with(|| entity_labels[entities[a.0]].cmp(&entity_labels[entities[b.0]]))
    });
    let note = format!(
        "drift: {completed} boundaries screened, {} overlapping a shared change excluded from individual comparisons, {n_tested} of {} entities tested for individual changes, {} detected, {} population-wide statement(s)",
        excluded_mask.count_ones(), entities.len(), found.len(), usize::from(global.is_some())
    );
    if found.is_empty() && global.is_none() {
        return (vec![], note, false);
    }
    let level = |s: Shift| level_labels[s.level].as_str();
    let period = |s: Shift| periods.labels[s.cut].as_str();
    let mut claims = Vec::new();
    let shared = global.map(|(s, n, matched)| {
        claims.push(format!(
            "A population-wide change in {dimension} around {}: across {matched} matched {entity} values, the mean share of '{}' moved from {} to {} ({n} moved in that direction by at least 10 percentage points)",
            period(s), level(s), pct(s.before), pct(s.after)
        ));
        json!({"period": period(s), "level": level(s), "before_share": s.before,
            "after_share": s.after, "matched_entities": matched, "concordant_entities": n})
    });
    if let Some(&(i, s)) = found.first() {
        claims.push(format!(
            "{} of {n_tested} tested {entity} values changed their {dimension} mix beyond the change among peers. For example, {} moved from {} to {} on '{}' around {} (peers: {} to {})",
            found.len(), entity_labels[entities[i]], pct(s.before), pct(s.after), level(s),
            period(s), pct(s.peer_before), pct(s.peer_after)
        ));
    }
    let examples: Vec<_> = found
        .iter()
        .take(MAX_EXAMPLES)
        .map(|&(i, s)| {
            json!({"entity": entity_labels[entities[i]], "period": period(s), "level": level(s),
            "before_share": s.before, "after_share": s.after, "peer_before_share": s.peer_before,
            "peer_after_share": s.peer_after, "p_value": s.p,
            "before_rows": s.n_before, "after_rows": s.n_after,
            "before_start": periods.labels[s.cut.saturating_sub(WINDOW)],
            "after_end_exclusive": periods.labels.get(s.cut + WINDOW)})
        })
        .collect();
    let metrics = BTreeMap::from([
        ("population_shift".into(), json!(shared)),
        ("entity_shifts".into(), json!(examples)),
        ("n_entity_shifts".into(), json!(found.len())),
        ("n_entities_tested".into(), json!(n_tested)),
        ("periods".into(), json!(periods.labels)),
        ("window_periods".into(), json!(WINDOW)),
        ("p_value_threshold".into(), json!(bar)),
    ]);
    let finding = Finding::new(
        Pattern::DistributionChange,
        BTreeMap::from([
            ("columns".into(), json!([entity, dimension])),
            ("time_column".into(), json!(periods.name)),
        ]),
        claims.join(". "),
        EvidenceBasis::Estimated,
        0.75,
        metrics,
        Some("Uses row shares in adjacent windows. Peer baselines use the same entities on both sides of the boundary.".into()),
        Method::new("entity_fingerprints", "entity_mix_change", "adjacent-window concentration changes with a conditional period-label permutation test and matched peer baseline"),
    );
    (vec![finding], note, false)
}
