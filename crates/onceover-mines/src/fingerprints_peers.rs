//! Entity peer pools from checked, finest-to-coarsest hierarchies.

use std::collections::{BTreeMap, BTreeSet};

use onceover_core::TypedTable;
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::{json, Value};

use crate::expectations::{cell_value, inspect_hierarchy};
use crate::fingerprints::pct;

pub(crate) const MIN_PEERS: usize = 5;
const MIN_EFFECTIVE_ROWS: f64 = 10.0;
const MIN_GAP: f64 = 0.10;

pub(crate) struct PeerGroup {
    pub column: String,
    pub value: Value,
}

pub(crate) struct HierarchyPeers {
    pub groups: Vec<PeerGroup>,
    pub membership: Vec<Vec<usize>>,
    pub columns: BTreeSet<usize>,
}

impl HierarchyPeers {
    pub(crate) fn resolve(
        table: &TypedTable,
        entity: usize,
        entity_codes: &[Option<u32>],
        n_entities: usize,
        hierarchies: &[Vec<String>],
        notes: &mut Vec<String>,
    ) -> Option<Self> {
        if hierarchies.is_empty() {
            return None;
        }
        let mut levels = Vec::new();
        let mut seen = BTreeSet::new();
        let mut columns = BTreeSet::new();
        for chain in hierarchies {
            match inspect_hierarchy(table, chain) {
                Ok((cols, edges)) if edges.iter().all(|edge| edge.violating_rows == 0) => {
                    let ancestors = cols.iter().position(|&c| c == entity).map_or(0, |i| i + 1);
                    columns.extend(cols.iter().copied());
                    for &col in &cols[ancestors..] {
                        if seen.insert(col) {
                            levels.push(col);
                        }
                    }
                    notes.push(format!("checked peer hierarchy {}", chain.join(" → ")));
                }
                Ok(_) => notes.push(format!(
                    "peer hierarchy {} refused: a mapping is broken",
                    chain.join(" → ")
                )),
                Err(note) => notes.push(note),
            }
        }

        let mut membership = vec![Vec::new(); n_entities];
        let mut groups = Vec::new();
        for col in levels {
            let mut keys = vec![None; n_entities];
            let mut ambiguous = vec![false; n_entities];
            for (row, e) in entity_codes.iter().enumerate() {
                let Some(e) = e.map(|e| e as usize) else {
                    continue;
                };
                let Some(value) = cell_value(&table.cols[col], row) else {
                    ambiguous[e] = true;
                    continue;
                };
                if keys[e].as_ref().is_some_and(|prev| prev != &value) {
                    ambiguous[e] = true;
                }
                keys[e].get_or_insert(value);
            }
            let mut pools = BTreeMap::new();
            for (e, key) in keys.into_iter().enumerate() {
                if let Some(value) = key.filter(|_| !ambiguous[e]) {
                    let pool = *pools.entry(value.to_string()).or_insert_with(|| {
                        let i = groups.len();
                        groups.push(PeerGroup {
                            column: table.names[col].clone(),
                            value,
                        });
                        i
                    });
                    membership[e].push(pool);
                }
            }
            let missing = ambiguous.into_iter().filter(|&a| a).count();
            if missing > 0 {
                notes.push(format!(
                    "{}: {missing} entities lack a complete, stable affiliation",
                    table.names[col]
                ));
            }
        }
        Some(Self {
            groups,
            membership,
            columns,
        })
    }

    /// Support alone chooses the pool, before the response is compared.
    /// Finer ancestors are subsets of broader ancestors. Across
    /// independent hierarchies, prefer the smallest well-supported
    /// pool, breaking ties in declaration order.
    pub(crate) fn select(
        &self,
        entity: usize,
        minimum_peers: usize,
        size: impl Fn(usize) -> usize,
    ) -> Option<usize> {
        self.membership[entity]
            .iter()
            .copied()
            .filter(|&i| size(i) > minimum_peers)
            .min_by_key(|&i| size(i))
    }
}

struct Range {
    total: f64,
    low: [(f64, usize); 3],
    high: [(f64, usize); 3],
}

impl Range {
    fn new() -> Self {
        Self {
            total: 0.0,
            low: [(f64::INFINITY, usize::MAX); 3],
            high: [(f64::NEG_INFINITY, usize::MAX); 3],
        }
    }

    fn add(&mut self, entity: usize, share: f64) {
        self.total += share;
        for (values, high) in [(&mut self.low, false), (&mut self.high, true)] {
            let mut next = (share, entity);
            for value in values {
                if if high {
                    next.0 > value.0
                } else {
                    next.0 < value.0
                } {
                    std::mem::swap(value, &mut next);
                }
            }
        }
    }

    fn bound(&self, entity: usize, high: bool, peers: usize) -> f64 {
        let values = if high { &self.high } else { &self.low };
        // A single aberrant peer should not mask every other alert
        // in a substantial pool. Smaller pools use the full range.
        values
            .iter()
            .filter(|v| v.1 != entity)
            .nth(usize::from(peers >= 10))
            .expect("a supported pool has enough endpoints")
            .0
    }
}

struct Pool {
    n: usize,
    inverse_support: f64,
    levels: Vec<Range>,
}

pub(crate) struct PeerInput<'a> {
    pub hierarchy: &'a HierarchyPeers,
    pub entity: &'a str,
    pub entities: &'a [usize],
    pub entity_labels: &'a [String],
    pub dimension: &'a str,
    pub dimension_columns: &'a [String],
    pub level_labels: &'a [String],
    pub rows: &'a [&'a Vec<f64>],
    pub weight: Option<&'a str>,
    pub test_count: usize,
}

pub(crate) fn outliers(input: PeerInput<'_>) -> (Option<Finding>, String) {
    let PeerInput {
        hierarchy,
        entity,
        entities,
        entity_labels,
        dimension,
        dimension_columns,
        level_labels,
        rows,
        weight,
        test_count,
    } = input;
    let support: Vec<f64> = rows.iter().map(|r| r.iter().sum()).collect();
    let mut pools: Vec<Option<Pool>> = (0..hierarchy.groups.len()).map(|_| None).collect();
    for (i, &e) in entities.iter().enumerate() {
        if support[i] < MIN_EFFECTIVE_ROWS - 1e-9 {
            continue;
        }
        for &g in &hierarchy.membership[e] {
            let pool = pools[g].get_or_insert_with(|| Pool {
                n: 0,
                inverse_support: 0.0,
                levels: (0..level_labels.len()).map(|_| Range::new()).collect(),
            });
            pool.n += 1;
            pool.inverse_support += 1.0 / support[i];
            for (range, &count) in pool.levels.iter_mut().zip(rows[i]) {
                range.add(e, count / support[i]);
            }
        }
    }
    let bar = 0.05 / (entities.len() * level_labels.len() * test_count.max(1)) as f64;
    let mut tested = 0;
    let mut levels_used = BTreeMap::new();
    let mut hits = Vec::new();
    for (i, &e) in entities.iter().enumerate() {
        if support[i] < MIN_EFFECTIVE_ROWS - 1e-9 {
            continue;
        }
        let size = |g: usize| pools[g].as_ref().map_or(0, |p| p.n);
        let Some(g) = hierarchy.select(e, MIN_PEERS, size) else {
            continue;
        };
        tested += 1;
        *levels_used
            .entry(hierarchy.groups[g].column.as_str())
            .or_insert(0usize) += 1;
        let pool = pools[g]
            .as_ref()
            .expect("the selected peer pool has support");
        let peers = pool.n - 1;
        let peer_count = peers as f64;
        let inverse = 1.0 / support[i];
        // Equal weight per peer entity. Normalize the observation
        // weights within each entity, then apply a two-sided
        // Hoeffding bound to the difference from the peer mean.
        let variance = inverse + (pool.inverse_support - inverse).max(0.0) / peer_count.powi(2);
        let best = pool
            .levels
            .iter()
            .zip(rows[i])
            .enumerate()
            .filter_map(|(j, (range, count))| {
                let share = count / support[i];
                let mean = ((range.total - share) / peer_count).clamp(0.0, 1.0);
                let high = share >= mean;
                let bound = range.bound(e, high, peers);
                let gap = (share - bound) * if high { 1.0 } else { -1.0 };
                if gap < MIN_GAP {
                    return None;
                }
                let p = (2.0 * (-2.0 * (share - mean).powi(2) / variance).exp()).min(1.0);
                (p <= bar).then_some((p, j, share, mean, bound))
            })
            .min_by(|a, b| {
                a.0.total_cmp(&b.0)
                    .then_with(|| level_labels[a.1].cmp(&level_labels[b.1]))
            });
        if let Some((p, j, share, mean, bound)) = best {
            hits.push((p, e, g, j, peers, share, mean, bound, support[i]));
        }
    }
    let detail = format!("hierarchy peers: {tested} of {} entities compared using at least {MIN_PEERS} eligible peers", entities.len());
    hits.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then_with(|| entity_labels[a.1].cmp(&entity_labels[b.1]))
    });
    let Some(&(_, e, g, j, _, share, mean, _, _)) = hits.first() else {
        return (None, detail);
    };
    let group = &hierarchy.groups[g];
    let basis = weight.map_or_else(|| "rows".to_string(), |w| format!("'{w}'"));
    let claim = format!(
        "{} of {tested} {entity} values have an unusual {dimension} mix within their hierarchy peers. For example, {} puts {} of {basis} on '{}' versus {} among peers with {} = {}",
        hits.len(), entity_labels[e], pct(share), level_labels[j], pct(mean), group.column, group.value
    );
    let payload: Vec<_> = hits.iter().map(|&(p, e, g, j, n, share, mean, bound, effective_rows)| {
        let group = &hierarchy.groups[g];
        json!({"entity": entity_labels[e], "level": level_labels[j], "share": share,
            "reference_share": mean, "peer_edge": bound, "effective_rows": effective_rows,
            "p_value": p, "peer_group": {"column": group.column, "value": group.value, "entities": n}})
    }).collect();
    let mut columns = vec![entity.to_string()];
    columns.extend_from_slice(dimension_columns);
    columns.extend(weight.map(str::to_string));
    let mut metrics = BTreeMap::from([
        ("entity_column".into(), json!(entity)),
        ("dimension".into(), json!(dimension)),
        ("dimensions".into(), json!(dimension_columns)),
        ("misfits".into(), json!(payload)),
        ("bar".into(), json!(bar)),
        ("entities_tested".into(), json!(tested)),
    ]);
    if let Some(w) = weight {
        metrics.insert("weight".into(), json!(w));
    }
    (Some(Finding::new(
        Pattern::EntityMisfits,
        BTreeMap::from([("columns".into(), json!(columns))]),
        claim, EvidenceBasis::Estimated, 0.62, metrics,
        Some("Each entity uses its smallest supported peer pool and is excluded from the reference. The reported share clears the peer range by 10 percentage points and passes the corrected probability bound. In pools of at least ten peers, the most extreme peer in that direction is omitted from the range.".into()),
        Method::new("entity_fingerprints", "hierarchy_peer_mix", "leave-one-out equal-entity peer shares, observed peer ranges and a two-sided Hoeffding bound"),
    )), detail)
}
