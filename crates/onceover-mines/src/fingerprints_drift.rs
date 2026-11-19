//! Changes in an entity's categorical mix along a resolved axis.

use std::collections::BTreeMap;

use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;

use crate::fingerprints::{effective_rows, pct, MAX_LEVELS, MIN_ENTITIES};
use crate::fingerprints_peers::HierarchyPeers;
use crate::Deadline;

const MAX_PERIODS: usize = 24;
const WINDOW: usize = 6;
const MIN_ROWS: f64 = 12.0;
const MIN_CHANGE: f64 = 0.20;
const MAX_MARGINAL_MOVE: f64 = 0.10;
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
            "drift along {}: {} {} [{}]",
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
    pool: usize,
    peers: usize,
    before: f64,
    after: f64,
    peer_before: f64,
    peer_after: f64,
    before_support: Support,
    after_support: Support,
    p: f64,
}

#[derive(Clone, Copy, Default)]
struct Support {
    rows: u32,
    mass: f64,
    squares: f64,
}

impl Support {
    fn since(self, earlier: Self) -> Self {
        Self {
            rows: self.rows - earlier.rows,
            mass: self.mass - earlier.mass,
            squares: self.squares - earlier.squares,
        }
    }

    fn effective(self) -> f64 {
        effective_rows(self.mass, self.squares)
    }
}

pub(crate) struct DriftInput<'a> {
    pub periods: &'a FingerprintPeriods,
    pub entity: &'a str,
    pub dimension: &'a str,
    pub dimension_columns: &'a [String],
    pub joint_levels: &'a [(usize, usize)],
    pub weight: Option<(&'a str, &'a [f64])>,
    pub entity_labels: &'a [String],
    pub level_labels: &'a [String],
    pub entities: &'a [usize],
    pub entity_codes: &'a [Option<u32>],
    pub level_codes: &'a [Option<u32>],
    pub dimension_count: usize,
    pub peers: Option<&'a HierarchyPeers>,
    pub deadline: Deadline,
}

pub(crate) fn scan(input: DriftInput<'_>) -> (Vec<Finding>, String, bool) {
    let DriftInput {
        periods,
        entity,
        dimension,
        dimension_columns,
        joint_levels,
        weight,
        entity_labels,
        level_labels,
        entities,
        entity_codes,
        level_codes,
        dimension_count,
        peers,
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
    let dimension_label = weight.map_or_else(
        || dimension.to_string(),
        |(w, _)| format!("{dimension} weighted by '{w}'"),
    );
    let l = level_labels.len();
    let stride = l * (n_periods + 1);
    let mut entities = entities.to_vec();
    entities.sort_by_key(|&e| &entity_labels[e]);
    let mut selected = vec![None; entity_labels.len()];
    for (i, &e) in entities.iter().enumerate() {
        selected[e] = Some(i);
    }
    let mut prefix = vec![0.0; stride * entities.len()];
    let support_stride = n_periods + 1;
    let mut support = vec![Support::default(); support_stride * entities.len()];
    for (index, ((e, j), p)) in entity_codes
        .iter()
        .zip(level_codes)
        .zip(&periods.period)
        .enumerate()
    {
        if let (Some(i), Some(j), Some(p)) = (e.and_then(|e| selected[e as usize]), j, p) {
            let w = weight.map_or(1.0, |(_, values)| values[index]);
            if w > 0.0 {
                prefix[i * stride + (p + 1) * l + *j as usize] += w;
                let s = &mut support[i * support_stride + p + 1];
                s.rows += 1;
                s.mass += w;
                s.squares += w * w;
            }
        }
    }
    let mut max_rows = 0;
    for row in prefix.chunks_exact_mut(stride) {
        for i in l..stride {
            row[i] += row[i - l];
        }
    }
    for s in support.chunks_exact_mut(support_stride) {
        for p in 1..s.len() {
            s[p].rows += s[p - 1].rows;
            s[p].mass += s[p - 1].mass;
            s[p].squares += s[p - 1].squares;
        }
        max_rows = max_rows.max(s[n_periods].rows);
    }
    let mut log_fact = if weight.is_none() {
        vec![0.0; max_rows as usize + 1]
    } else {
        Vec::new()
    };
    for k in 1..log_fact.len() {
        log_fact[k] = log_fact[k - 1] + (k as f64).ln();
    }
    let bar = 0.05 / ((n_periods - 1) * l * entities.len() * dimension_count) as f64;
    let mut picks: Vec<Vec<Shift>> = vec![Vec::new(); entities.len()];
    let pool_count = peers.map_or(1, |p| p.groups.len());
    let mut global: Vec<Option<(Shift, usize, usize)>> = vec![None; pool_count];
    let mut completed = 0;
    let mut tested = vec![Vec::new(); entities.len()];
    let mut shared_cuts = vec![Vec::new(); pool_count];
    for cut in 1..n_periods {
        if deadline.exceeded() {
            break;
        }
        let left = cut.saturating_sub(WINDOW) * l;
        let mid = cut * l;
        let right = (cut + WINDOW).min(n_periods) * l;
        let mut matched = Vec::new();
        for (i, row) in prefix.chunks_exact(stride).enumerate() {
            let windows = &support[i * support_stride..(i + 1) * support_stride];
            let n = windows[cut].since(windows[cut.saturating_sub(WINDOW)]);
            let m = windows[(cut + WINDOW).min(n_periods)].since(windows[cut]);
            let ne = n.effective();
            let me = m.effective();
            if ne < MIN_ROWS - 1e-9 || me < MIN_ROWS - 1e-9 {
                continue;
            }
            let mut stable = u32::MAX;
            if !joint_levels.is_empty() {
                // Joint changes must move while the containing
                // marginal levels stay nearly steady.
                let mut left_change = [0.0; MAX_LEVELS];
                let mut right_change = [0.0; MAX_LEVELS];
                for (j, &(a, b)) in joint_levels.iter().enumerate() {
                    let delta = (row[right + j] - row[mid + j]) / m.mass
                        - (row[mid + j] - row[left + j]) / n.mass;
                    left_change[a] += delta;
                    right_change[b] += delta;
                }
                stable = 0;
                for (j, &(a, b)) in joint_levels.iter().enumerate() {
                    if left_change[a].abs() < MAX_MARGINAL_MOVE
                        && right_change[b].abs() < MAX_MARGINAL_MOVE
                    {
                        stable |= 1 << j;
                    }
                }
            }
            matched.push((i, row, n, m, ne, me, stable));
        }
        completed += 1;
        // Build each declared pool from all its members supported on
        // both sides, even when some of those peers have a narrower
        // pool of their own. Only use pools selected by a subject.
        let mut pools = vec![Vec::new(); pool_count];
        for row in &matched {
            if let Some(hierarchy) = peers {
                for &g in &hierarchy.membership[entities[row.0]] {
                    pools[g].push(row);
                }
            } else {
                pools[0].push(row);
            }
        }
        let mut choices = vec![None; entities.len()];
        let mut active = vec![false; pool_count];
        for &(i, _, _, _, _, _, _) in &matched {
            let pool = if let Some(hierarchy) = peers {
                hierarchy.select(entities[i], MIN_ENTITIES - 1, |g| pools[g].len())
            } else {
                (pools[0].len() >= MIN_ENTITIES).then_some(0)
            };
            if let Some(g) = pool {
                choices[i] = Some(g);
                active[g] = true;
            }
        }
        for (pool, cohort) in pools.into_iter().enumerate().filter(|(g, _)| active[*g]) {
            let n_matched = cohort.len();
            let mut shares = vec![(0.0, 0.0, 0, 0); l];
            for &&(_, row, n, m, _, _, stable) in &cohort {
                for (j, (b, a, up, down)) in shares.iter_mut().enumerate() {
                    let before = ((row[mid + j] - row[left + j]) / n.mass).clamp(0.0, 1.0);
                    let after = ((row[right + j] - row[mid + j]) / m.mass).clamp(0.0, 1.0);
                    *b += before;
                    *a += after;
                    if stable & (1 << j) != 0 {
                        *up += usize::from(after - before >= 0.10);
                        *down += usize::from(before - after >= 0.10);
                    }
                }
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
                            pool,
                            peers: n_matched - 1,
                            before,
                            after,
                            peer_before: 0.0,
                            peer_after: 0.0,
                            before_support: Support::default(),
                            after_support: Support::default(),
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
                shared_cuts[pool].push(cut);
                if global[pool].is_none_or(|(g, _, _)| {
                    (candidate.0.after - candidate.0.before).abs() > (g.after - g.before).abs()
                }) {
                    global[pool] = Some(candidate);
                }
                // A compositional change can move every other share as
                // well. Do not interpret this window as individual drift.
                continue;
            }
            for &(i, row, n, m, ne, me, stable) in cohort {
                if choices[i] != Some(pool) || stable == 0 {
                    continue;
                }
                tested[i].push((cut, pool));
                let mut best: Option<Shift> = None;
                for (j, &(b, a, _, _)) in shares.iter().enumerate() {
                    if stable & (1 << j) == 0 {
                        continue;
                    }
                    let (x, y) = (row[mid + j] - row[left + j], row[right + j] - row[mid + j]);
                    let (before, after) =
                        ((x / n.mass).clamp(0.0, 1.0), (y / m.mass).clamp(0.0, 1.0));
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
                    let p = if weight.is_some() {
                        // Hoeffding's two-sided bound for independent
                        // level indicators with fixed, normalized weights.
                        (2.0 * (-2.0 * change.powi(2) / (1.0 / ne + 1.0 / me)).exp()).min(1.0)
                    } else {
                        permutation_p(x as u32, y as u32, n.rows, m.rows, &log_fact, bar)
                    };
                    if p <= bar
                        && best.is_none_or(|s| {
                            p < s.p || (p == s.p && level_labels[j] < level_labels[s.level])
                        })
                    {
                        best = Some(Shift {
                            cut,
                            level: j,
                            pool,
                            peers: n_matched - 1,
                            before,
                            after,
                            peer_before,
                            peer_after,
                            before_support: n,
                            after_support: m,
                            p,
                        });
                    }
                }
                picks[i].extend(best);
            }
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
    let excluded: Vec<u32> = shared_cuts
        .iter()
        .map(|cuts| {
            (1..n_periods)
                .filter(|&cut| {
                    cuts.iter().any(|&shared| {
                        cut.saturating_sub(WINDOW) < shared
                            && shared < (cut + WINDOW).min(n_periods)
                    })
                })
                .fold(0, |mask, cut| mask | (1 << cut))
        })
        .collect();
    let n_tested = tested
        .into_iter()
        .filter(|windows| {
            windows
                .iter()
                .any(|&(cut, pool)| excluded[pool] & (1 << cut) == 0)
        })
        .count();
    let mut found: Vec<_> = picks
        .into_iter()
        .enumerate()
        .filter_map(|(i, shifts)| {
            shifts
                .into_iter()
                .filter(|s| excluded[s.pool] & (1 << s.cut) == 0)
                .min_by(|a, b| a.p.total_cmp(&b.p).then_with(|| a.cut.cmp(&b.cut)))
                .map(|s| (i, s))
        })
        .collect();
    found.sort_by(|a, b| {
        a.1.p
            .total_cmp(&b.1.p)
            .then_with(|| entity_labels[entities[a.0]].cmp(&entity_labels[entities[b.0]]))
    });
    let mut global: Vec<_> = global.into_iter().flatten().collect();
    global.sort_by(|(a, _, _), (b, _, _)| {
        (b.after - b.before)
            .abs()
            .total_cmp(&(a.after - a.before).abs())
            .then_with(|| {
                peers.map_or(std::cmp::Ordering::Equal, |h| {
                    let (a, b) = (&h.groups[a.pool], &h.groups[b.pool]);
                    a.column
                        .cmp(&b.column)
                        .then_with(|| a.value.to_string().cmp(&b.value.to_string()))
                })
            })
    });
    let note = format!(
        "drift: {completed} boundaries screened, {} overlapping a shared change excluded from individual comparisons, {n_tested} of {} entities tested for individual changes, {} detected, {} population-wide statement(s){}",
        excluded.iter().map(|x| x.count_ones()).sum::<u32>(), entities.len(), found.len(),
        global.len(), if peers.is_some() { " in hierarchy peer pools" } else { "" }
    );
    if found.is_empty() && global.is_empty() {
        return (vec![], note, false);
    }
    let level = |s: Shift| level_labels[s.level].as_str();
    let period = |s: Shift| periods.labels[s.cut].as_str();
    let peer_group = |s: Shift| {
        peers.map(|h| {
            let g = &h.groups[s.pool];
            json!({"column":g.column, "value":g.value, "entities":s.peers})
        })
    };
    let describe_group = |s: Shift| {
        peers.map_or_else(String::new, |h| {
            let g = &h.groups[s.pool];
            format!(" with {} = {}", g.column, g.value)
        })
    };
    let mut claims = Vec::new();
    if let Some(&(s, n, matched)) = global.first() {
        if peers.is_some() {
            claims.push(format!("{} shared {} shifts across hierarchy peer pools. For example, across {matched} matched {entity} values{} around {}, the mean share of '{}' moved from {} to {}", global.len(), dimension_label, describe_group(s), period(s), level(s), pct(s.before), pct(s.after)));
        } else {
            claims.push(format!(
                "A population-wide change in {dimension_label} around {}: across {matched} matched {entity} values, the mean share of '{}' moved from {} to {} ({n} moved in that direction by at least 10 percentage points)",
                period(s), level(s), pct(s.before), pct(s.after)
            ));
        }
    }
    let shared: Vec<_> = global.into_iter().map(|(s, n, matched)| {
        let mut detail = json!({"period": period(s), "level": level(s), "before_share": s.before,
            "after_share": s.after, "matched_entities": matched, "concordant_entities": n});
        if let Some(group) = peer_group(s) {
            detail["peer_group"] = group;
        }
        detail
    }).collect();
    if let Some(&(i, s)) = found.first() {
        claims.push(format!(
            "{} of {n_tested} tested {entity} values changed their {dimension_label} mix beyond the change among peers. For example, {} moved from {} to {} on '{}' around {} (peers{}: {} to {})",
            found.len(), entity_labels[entities[i]], pct(s.before), pct(s.after), level(s),
            period(s), describe_group(s), pct(s.peer_before), pct(s.peer_after)
        ));
    }
    let examples: Vec<_> = found
        .iter()
        .take(MAX_EXAMPLES)
        .map(|&(i, s)| {
            let mut example = json!({"entity": entity_labels[entities[i]], "period": period(s), "level": level(s),
            "before_share": s.before, "after_share": s.after, "peer_before_share": s.peer_before,
            "peer_after_share": s.peer_after, "p_value": s.p,
            "before_rows": s.before_support.rows, "after_rows": s.after_support.rows,
            "before_start": periods.labels[s.cut.saturating_sub(WINDOW)],
            "after_end_exclusive": periods.labels.get(s.cut + WINDOW)});
            if weight.is_some() {
                example["before_effective_rows"] = json!(s.before_support.effective());
                example["after_effective_rows"] = json!(s.after_support.effective());
            }
            if let Some(group) = peer_group(s) {
                example["peer_group"] = group;
            }
            example
        })
        .collect();
    let mut metrics = BTreeMap::from([
        ("population_shift".into(), json!(shared.first())),
        ("entity_shifts".into(), json!(examples)),
        ("n_entity_shifts".into(), json!(found.len())),
        ("n_entities_tested".into(), json!(n_tested)),
        ("periods".into(), json!(periods.labels)),
        ("window_periods".into(), json!(WINDOW)),
        ("p_value_threshold".into(), json!(bar)),
    ]);
    if peers.is_some() {
        metrics.insert("population_shifts".into(), json!(shared));
    }
    let mut columns = vec![entity.to_string()];
    columns.extend_from_slice(dimension_columns);
    if dimension_columns.len() > 1 {
        metrics.insert("dimensions".into(), json!(dimension_columns));
    }
    let (interpretation, provenance) = if let Some((name, _)) = weight {
        metrics.insert("weight".into(), json!(name));
        columns.push(name.to_string());
        (
            format!("Uses shares of '{name}' in adjacent windows and equal representation of matched peers."),
            "adjacent-window changes in weighted shares using a Hoeffding probability bound and matched peer baseline",
        )
    } else {
        (
            "Uses row shares in adjacent windows. Peer baselines use the same entities on both sides of the boundary.".to_string(),
            "adjacent-window concentration changes with a conditional period-label permutation test and matched peer baseline",
        )
    };
    let interpretation = if joint_levels.is_empty() {
        interpretation
    } else {
        format!("{interpretation} A joint level is tested only if its two marginal shares each move by less than 10 percentage points.")
    };
    let finding = Finding::new(
        Pattern::DistributionChange,
        BTreeMap::from([
            ("columns".into(), json!(columns)),
            ("time_column".into(), json!(periods.name)),
        ]),
        claims.join(". "),
        EvidenceBasis::Estimated,
        0.75,
        metrics,
        Some(interpretation),
        Method::new("entity_fingerprints", "entity_mix_change", provenance),
    );
    (vec![finding], note, false)
}
