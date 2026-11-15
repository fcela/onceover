//! The predictability screen: an in-house, purpose-built histogram gradient
//! boosting machine (depth-3, shrinkage, deterministic early stopping on a
//! holdout) run for every eligible column as target against the rest.

use onceover_report::{Method, Pattern};
use std::collections::BTreeMap;

use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding};
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use serde_json::json;

use crate::encode::{EncodedMatrix, MISSING};

#[derive(Clone, Debug)]
pub struct ScreenConfig {
    pub max_rounds: usize,
    pub max_predictors: usize,
    pub max_model_rows: usize,
    pub patience: usize,
    pub learning_rate: f32,
    pub max_depth: u32,
    pub lambda: f32,
    pub cat_numeric_max_distinct: usize,
    pub cat_max_k: usize,
    pub min_target_rows: usize,
}

impl Default for ScreenConfig {
    fn default() -> Self {
        ScreenConfig {
            max_rounds: 30,
            max_predictors: 48,
            max_model_rows: 5_000,
            patience: 5,
            learning_rate: 0.3,
            max_depth: 3,
            lambda: 1.0,
            cat_numeric_max_distinct: 12,
            cat_max_k: 24,
            min_target_rows: 300,
        }
    }
}

pub use crate::targets::TargetProfile as TargetResult;

pub struct ScreenOutcome {
    pub findings: Vec<Finding>,
    pub results: Vec<TargetResult>,
    pub coverage: String,
}

// ---------- deterministic RNG ----------

pub struct Lcg(pub u64);

impl Lcg {
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Row-major packed copy of the feature bins (LightGBM-style): row p's
/// F bin values are contiguous, so histogram construction fetches g/h once
/// per row and streams the bins linearly.
pub fn pack_row_major(enc: &EncodedMatrix) -> Vec<u8> {
    let f = enc.bins.len();
    let mut packed = vec![0u8; enc.n * f];
    for (fi, bins) in enc.bins.iter().enumerate() {
        for (p, &b) in bins.iter().enumerate() {
            packed[p * f + fi] = b;
        }
    }
    packed
}

/// Pack the given encoded-feature indices (in `feats` order) row-major,
/// the layout the boosting loops consume.
fn pack_features(enc: &EncodedMatrix, feats: &[usize]) -> Vec<u8> {
    let n_feats = feats.len();
    let mut packed = vec![MISSING; enc.n * n_feats];
    for (f, &original) in feats.iter().enumerate() {
        for r in 0..enc.n {
            packed[r * n_feats + f] = enc.bins[original][r];
        }
    }
    packed
}

// ---------- driver shortlist ----------

/// A driver must hold at least this share of the model's total
/// split gain...
const DRIVER_MIN_SHARE: f64 = 0.02;
/// ...or this fraction of the top feature's share, whichever is
/// larger: a dominant driver raises the bar so the noise a strong
/// fit still splits on stays out of the claim.
const DRIVER_REL_SHARE: f64 = 0.10;
/// The claim names at most this many drivers.
const DRIVER_MAX: usize = 8;

/// Indices into `ranked` — (position, gain) pairs in descending
/// gain order — of the driver shortlist: the prefix whose gain
/// share is at least max(DRIVER_MIN_SHARE, DRIVER_REL_SHARE × the
/// top feature's share). The top feature always qualifies, even
/// when gains are spread so thin that its share is below the
/// absolute floor. Empty when there are no positive gains.
fn select_driver_idx(ranked: &[(usize, f64)], total: f64) -> Vec<usize> {
    if ranked.is_empty() || total <= 0.0 {
        return Vec::new();
    }
    let floor = DRIVER_MIN_SHARE.max(DRIVER_REL_SHARE * (ranked[0].1 / total));
    let mut idx: Vec<usize> = ranked
        .iter()
        .enumerate()
        .take_while(|(_, (_, g))| g / total >= floor)
        .map(|(i, _)| i)
        .collect();
    if idx.is_empty() {
        idx.push(0);
    }
    idx.truncate(DRIVER_MAX);
    idx
}

// ---------- tree ----------

#[derive(Debug, Clone)]
struct Node {
    feat: u32, // u32::MAX = leaf
    thr: u8,
    left: u32,
    right: u32,
    leaf: f32,
}

/// One round's tree, grown over gradient/hessian with a feature to skip
/// (the target's own column). Uses the packed row-major bin matrix for
/// single-pass histogram construction.
#[allow(clippy::too_many_arguments)]
fn grow_tree(
    packed: &[u8],
    n_feats: usize,
    train: &[u32],
    g: &[f32],
    h: &[f32],
    skip_feat: Option<usize>,
    lambda: f32,
    min_leaf: usize,
    max_depth: u32,
) -> (u32, Vec<Node>, Vec<f64>) {
    let mut nodes: Vec<Node> = Vec::new();
    let mut gains = vec![0.0f64; n_feats];
    grow_node(
        packed, n_feats, train, g, h, skip_feat, lambda, min_leaf, max_depth, 0, &mut nodes,
        &mut gains,
    );
    (0, nodes, gains)
}

#[allow(clippy::too_many_arguments)]
fn grow_node(
    packed: &[u8],
    n_feats: usize,
    positions: &[u32],
    g: &[f32],
    h: &[f32],
    skip_feat: Option<usize>,
    lambda: f32,
    min_leaf: usize,
    max_depth: u32,
    depth: u32,
    nodes: &mut Vec<Node>,
    gains: &mut Vec<f64>,
) -> u32 {
    let mut gt = 0.0f32;
    let mut ht = 0.0f32;
    for &p in positions {
        gt += g[p as usize];
        ht += h[p as usize];
    }
    if depth >= max_depth || positions.len() < 2 * min_leaf {
        return push_leaf(nodes, gt, ht, lambda);
    }

    // Right-sized histograms for ALL features at once (<= 32 bins each):
    // small enough to stay cache-resident while we stream rows.
    let mut hg = vec![[0.0f32; 32]; n_feats];
    let mut hh = vec![[0.0f32; 32]; n_feats];
    let mut hc = vec![[0u32; 32]; n_feats];
    let mut mg = vec![0.0f32; n_feats];
    let mut mh = vec![0.0f32; n_feats];
    let mut mc = vec![0u32; n_feats];

    // Single pass over node rows: g/h fetched once per row, bins streamed.
    for &p in positions {
        let row = &packed[p as usize * n_feats..][..n_feats];
        let gv = g[p as usize];
        let hv = h[p as usize];
        for f in 0..n_feats {
            let b = row[f];
            if b == MISSING {
                mg[f] += gv;
                mh[f] += hv;
                mc[f] += 1;
            } else {
                let b = (b as usize) & 31;
                hg[f][b] += gv;
                hh[f][b] += hv;
                hc[f][b] += 1;
            }
        }
    }

    // Split search.
    let mut best_gain = 0.0f32;
    let mut best_feat = usize::MAX;
    let mut best_thr = 0u8;
    let base = gt * gt / (ht + lambda);
    for f in 0..n_feats {
        if Some(f) == skip_feat {
            continue;
        }
        let nb = 32
            .min(mg[f].max(0.0) as usize + hc[f].iter().sum::<u32>() as usize)
            .max(1);
        let mut lg = 0.0f32;
        let mut lh = 0.0f32;
        let mut lc = 0u32;
        for t in 0..nb {
            lg += hg[f][t];
            lh += hh[f][t];
            lc += hc[f][t];
            let rc = positions.len() as u32 - lc - mc[f];
            if lc < min_leaf as u32 || rc < min_leaf as u32 {
                continue;
            }
            let rg = gt - lg - mg[f];
            let rh = ht - lh - mh[f];
            let gain = lg * lg / (lh + lambda) + rg * rg / (rh + lambda) - base;
            if gain > best_gain {
                best_gain = gain;
                best_feat = f;
                best_thr = t as u8;
            }
        }
    }

    if best_feat == usize::MAX {
        return push_leaf(nodes, gt, ht, lambda);
    }
    gains[best_feat] += best_gain as f64;

    let mut left_rows = Vec::with_capacity(positions.len() / 2);
    let mut right_rows = Vec::with_capacity(positions.len() / 2);
    for &p in positions {
        if packed[p as usize * n_feats + best_feat] <= best_thr {
            left_rows.push(p);
        } else {
            right_rows.push(p);
        }
    }
    let idx = nodes.len() as u32;
    nodes.push(Node {
        feat: best_feat as u32,
        thr: best_thr,
        left: 0,
        right: 0,
        leaf: 0.0,
    });
    let l = grow_node(
        packed,
        n_feats,
        &left_rows,
        g,
        h,
        skip_feat,
        lambda,
        min_leaf,
        max_depth,
        depth + 1,
        nodes,
        gains,
    );
    let r = grow_node(
        packed,
        n_feats,
        &right_rows,
        g,
        h,
        skip_feat,
        lambda,
        min_leaf,
        max_depth,
        depth + 1,
        nodes,
        gains,
    );
    nodes[idx as usize].left = l;
    nodes[idx as usize].right = r;
    idx
}

fn push_leaf(nodes: &mut Vec<Node>, g: f32, h: f32, lambda: f32) -> u32 {
    let idx = nodes.len() as u32;
    nodes.push(Node {
        feat: u32::MAX,
        thr: 0,
        left: 0,
        right: 0,
        leaf: -g / (h + lambda),
    });
    idx
}

fn tree_predict(nodes: &[Node], root: u32, packed: &[u8], n_feats: usize, pos: usize) -> f32 {
    let mut idx = root;
    while nodes[idx as usize].feat != u32::MAX {
        let n = &nodes[idx as usize];
        let v = packed[pos * n_feats + n.feat as usize];
        idx = if v <= n.thr { n.left } else { n.right };
    }
    nodes[idx as usize].leaf
}

// ---------- targets ----------

enum TargetKind {
    Numeric(Vec<f32>),
    Categorical(Vec<u8>, Vec<String>), // class ids (MISSING = absent), labels
}

fn class_str(col: &TypedCol, r: usize) -> Option<String> {
    match col {
        TypedCol::Str { vals } => vals[r].map(|s| s.to_string()),
        TypedCol::StrOwned { vals } => vals[r].clone(),
        TypedCol::Bool { vals } => vals[r].map(|b| if b { "true".into() } else { "false".into() }),
        TypedCol::I64 { vals } => vals.at(r).map(|v| v.to_string()),
        TypedCol::U64 { vals } => vals.at(r).map(|v| v.to_string()),
        TypedCol::F64 { vals } => vals.at(r).map(|v| v.to_string()),
        _ => None,
    }
}

fn prepare_target(
    table: &TypedTable,
    enc: &EncodedMatrix,
    ci: usize,
    cfg: &ScreenConfig,
) -> Option<TargetKind> {
    let col = &table.cols[ci];
    match col {
        TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. } => {
            // Target eligibility uses the same bounded row sample as fitting;
            // do not recount millions of full-column values for every model.
            let mut distinct = std::collections::BTreeSet::new();
            for &r in &enc.rows {
                let value = match col {
                    TypedCol::I64 { vals } => vals.at(r as usize).map(|v| v as f64),
                    TypedCol::U64 { vals } => vals.at(r as usize).map(|v| v as f64),
                    TypedCol::F64 { vals } => vals.at(r as usize),
                    _ => None,
                };
                if let Some(v) = value {
                    distinct.insert(v.to_bits());
                }
                if distinct.len() > cfg.cat_numeric_max_distinct {
                    break;
                }
            }
            if distinct.len() <= cfg.cat_numeric_max_distinct {
                cat_target(col, enc, cfg)
            } else if distinct.len() >= 2 {
                let mut ys = vec![f32::NAN; enc.n];
                for (j, y) in ys.iter_mut().enumerate() {
                    let v = match col {
                        TypedCol::I64 { vals } => vals.at(enc.rows[j] as usize).map(|x| x as f32),
                        TypedCol::U64 { vals } => vals.at(enc.rows[j] as usize).map(|x| x as f32),
                        TypedCol::F64 { vals } => vals.at(enc.rows[j] as usize).map(|x| x as f32),
                        _ => None,
                    };
                    if let Some(v) = v {
                        *y = v;
                    }
                }
                Some(TargetKind::Numeric(ys))
            } else {
                None
            }
        }
        TypedCol::Bool { .. } | TypedCol::Str { .. } | TypedCol::StrOwned { .. } => {
            cat_target(col, enc, cfg)
        }
        _ => None,
    }
}

fn cat_target(col: &TypedCol, enc: &EncodedMatrix, cfg: &ScreenConfig) -> Option<TargetKind> {
    let mut counts: FxHashMap<String, u32> = FxHashMap::default();
    for j in 0..enc.n {
        if let Some(v) = class_str(col, enc.rows[j] as usize) {
            *counts.entry(v).or_insert(0) += 1;
        }
    }
    if counts.len() < 2 || counts.len() > cfg.cat_max_k {
        return None;
    }
    let mut levels: Vec<(String, u32)> = counts.into_iter().collect();
    levels.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let labels: Vec<String> = levels.iter().map(|(v, _)| v.clone()).collect();
    let code_of: FxHashMap<&str, u8> = labels
        .iter()
        .enumerate()
        .map(|(i, v)| (v.as_str(), i as u8))
        .collect();
    let mut ids = vec![MISSING; enc.n];
    for (j, slot) in ids.iter_mut().enumerate() {
        if let Some(v) = class_str(col, enc.rows[j] as usize) {
            if let Some(&c) = code_of.get(v.as_str()) {
                *slot = c;
            }
        }
    }
    Some(TargetKind::Categorical(ids, labels))
}

// ---------- screening ----------

pub fn screen(
    table: &TypedTable,
    enc: &EncodedMatrix,
    cfg: &ScreenConfig,
    excluded_targets: &[usize],
) -> ScreenOutcome {
    // Frame-role exclusions remove scaffolding columns (index,
    // identifier, timestamp parts) from the target set and from
    // every target's predictor pool, and the exclusion is stated
    // in coverage.
    let eligible_targets: Vec<usize> = enc
        .feat_cols
        .iter()
        .copied()
        .filter(|i| !excluded_targets.contains(i))
        .collect();

    let results: Vec<TargetResult> = eligible_targets
        .par_iter()
        .filter_map(|&ci| screen_target(table, enc, ci, cfg, excluded_targets))
        .collect();

    let mut findings = Vec::new();
    for r in &results {
        let strong = if r.is_categorical {
            r.metric >= 0.85 && r.metric >= r.baseline + 0.05
        } else {
            r.metric >= 0.5
        };
        let moderate = if r.is_categorical {
            r.metric >= 0.7 && r.metric >= r.baseline + 0.05
        } else {
            r.metric >= 0.3
        };
        let interest = if strong {
            0.78
        } else if moderate {
            0.55
        } else {
            continue;
        };
        // The claim names the drivers — the inputs that hold the
        // model's gain — not every input the model split on. When a
        // refit on the drivers alone was measured, its score leads
        // and the full model's score follows as context.
        let claim = if r.drivers.is_empty() {
            // No positive-gain input; unreachable for findings that
            // clear the strength bars. Keep the full-list form.
            if r.is_categorical {
                format!(
                    "{} is predictable from [{}] (holdout accuracy {:.2} vs {:.2} majority baseline)",
                    r.name,
                    r.inputs.join(", "),
                    r.metric,
                    r.baseline
                )
            } else {
                format!(
                    "{} is predictable from [{}] (holdout R² = {:.2})",
                    r.name,
                    r.inputs.join(", "),
                    r.metric
                )
            }
        } else {
            let named = r
                .drivers
                .iter()
                .map(|(n, s)| format!("{n} ({:.0}% of gain)", s * 100.0))
                .collect::<Vec<_>>()
                .join(", ");
            match (r.is_categorical, r.reduced_metric) {
                (false, None) => format!(
                    "{} is predictable from {} (holdout R² = {:.2})",
                    r.name, named, r.metric
                ),
                (false, Some(red)) => format!(
                    "{} is predictable from {} (holdout R² = {:.2} using only these drivers; {:.2} using all inputs)",
                    r.name, named, red, r.metric
                ),
                (true, None) => format!(
                    "{} is predictable from {} (holdout accuracy {:.2} vs {:.2} majority baseline)",
                    r.name, named, r.metric, r.baseline
                ),
                (true, Some(red)) => format!(
                    "{} is predictable from {} (holdout accuracy {:.2} vs {:.2} majority baseline; {:.2} using all inputs)",
                    r.name, named, red, r.baseline, r.metric
                ),
            }
        };
        findings.push(Finding::new(
            Pattern::PredictiveDependency,
            BTreeMap::from([("column".into(), json!(r.name))]),
            claim,
            EvidenceBasis::Estimated,
            interest,
            BTreeMap::from([
                ("metric".into(), json!(r.metric)),
                (
                    "task".into(),
                    json!(if r.is_categorical {
                        "classification"
                    } else {
                        "regression"
                    }),
                ),
                ("inputs".into(), json!(r.inputs)),
                ("baseline".into(), json!(r.baseline)),
                ("n_train".into(), json!(r.n_train)),
                ("n_holdout".into(), json!(r.n_holdout)),
                (
                    "top_features".into(),
                    json!(r
                        .top_features
                        .iter()
                        .map(|(name, _)| name)
                        .collect::<Vec<_>>()),
                ),
                (
                    "gain_shares".into(),
                    json!(r.top_features.iter().map(|(_, s)| s).collect::<Vec<_>>()),
                ),
                (
                    "drivers".into(),
                    json!(r.drivers.iter().map(|(name, _)| name).collect::<Vec<_>>()),
                ),
                (
                    "driver_shares".into(),
                    json!(r.drivers.iter().map(|(_, s)| s).collect::<Vec<_>>()),
                ),
                ("reduced_metric".into(), json!(r.reduced_metric)),
                ("reduced_rounds".into(), json!(r.reduced_rounds)),
                ("rounds".into(), json!(r.rounds)),
                ("rows_used".into(), json!(enc.n)),
            ]),
            Some(format!(
                "inspect the predictive inputs and validation before treating {} as derived",
                r.name
            )),
            Method::new(
                "gbm",
                "histogram_gradient_boosting",
                "in-house histogram GBM screener (depth-3, early-stopped; onceover v0.1)",
            ),
        ));
    }

    let mut coverage = format!(
        "{} target columns considered over {} encoded rows / {} features; {} predictable; predictors shortlisted from all features on training rows (max {} predictors / {} fitting rows per target)",
        eligible_targets.len(),
        enc.n,
        enc.bins.len(),
        results
            .iter()
            .filter(|r| {
                r.metric >= if r.is_categorical { 0.7 } else { 0.3 }
                    && (!r.is_categorical || r.metric >= r.baseline + 0.05)
            })
            .count(),
        cfg.max_predictors, cfg.max_model_rows,
    );
    let excluded_names: Vec<&str> = excluded_targets
        .iter()
        .filter(|i| enc.feat_cols.contains(i))
        .map(|&i| table.names[i].as_str())
        .collect();
    if !excluded_names.is_empty() {
        coverage.push_str(&format!(
            "; {} column(s) excluded as targets and predictors by frame role: {}",
            excluded_names.len(),
            excluded_names.join(", ")
        ));
    }
    ScreenOutcome {
        findings,
        results,
        coverage,
    }
}

/// Screen one column as a target. Columns in `excluded_targets`
/// (frame-role scaffolding) are withheld from the predictor pool as
/// well as from the target set.
pub fn screen_target(
    table: &TypedTable,
    enc: &EncodedMatrix,
    ci: usize,
    cfg: &ScreenConfig,
    excluded_targets: &[usize],
) -> Option<TargetResult> {
    let target = prepare_target(table, enc, ci, cfg)?;
    let present: Vec<u32> = match &target {
        TargetKind::Numeric(ys) => (0..enc.n)
            .filter(|&j| !ys[j].is_nan())
            .map(|j| j as u32)
            .collect(),
        TargetKind::Categorical(ids, _) => (0..enc.n)
            .filter(|&j| ids[j] != MISSING)
            .map(|j| j as u32)
            .collect(),
    };
    if present.len() < cfg.min_target_rows {
        return None;
    }

    let cat_payload: Option<(Vec<u8>, Vec<String>)> = match &target {
        TargetKind::Categorical(ids, labels) => Some((ids.clone(), labels.clone())),
        _ => None,
    };

    // Deterministic holdout: seeded shuffle, 75/25.
    let mut rng = Lcg(0x9E37_79B9_7F4A_7C15 ^ (ci as u64).wrapping_mul(0x853C49E6748FEA9B));
    let mut order = present.clone();
    for i in (1..order.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    // Each target is considered. Bound fitting rows independently of table width.
    order.truncate(cfg.max_model_rows.max(cfg.min_target_rows));
    let n_hold = order.len() / 4;
    let (train, holdout) = order.split_at(order.len() - n_hold);
    let min_leaf = (train.len() / 200).clamp(10, 1_000);
    let own = enc.feat_cols.iter().position(|&c| c == ci)?;
    let screen_rows: Vec<u32> = train.iter().take(2048).copied().collect();
    let mut selected: Vec<usize> = (0..enc.bins.len())
        .filter(|&f| f != own && !excluded_targets.contains(&enc.feat_cols[f]))
        .collect();
    if selected.len() > cfg.max_predictors {
        // Pair each feature with its information score before
        // sorting: `selected` is not the full feature range once
        // exclusions apply, so scores cannot be indexed by feature
        // position.
        let mut scored: Vec<(usize, f64)> = selected
            .iter()
            .map(|&f| {
                (
                    f,
                    crate::screen::information(&enc.bins[own], &enc.bins[f], &screen_rows),
                )
            })
            .collect();
        scored.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then(enc.feat_names[a.0].cmp(&enc.feat_names[b.0]))
        });
        selected = scored.into_iter().map(|(f, _)| f).collect();
        selected.truncate(cfg.max_predictors);
    }
    if selected.is_empty() {
        return None;
    }
    let n_feats = selected.len();
    let packed = pack_features(enc, &selected);
    let own_feat: Option<usize> = None; // target excluded before packing

    // Full fit on every selected predictor.
    let (metric, baseline, rounds, gains) = match &target {
        TargetKind::Numeric(ys) => {
            let (m, r, g, _trees, _base) = boost_numeric(
                enc.n, cfg, ys, train, holdout, &packed, n_feats, own_feat, min_leaf,
            );
            if m <= 0.0 {
                return None;
            }
            (m, 0.0, r, g)
        }
        TargetKind::Categorical(ids, labels) => boost_categorical(
            enc.n,
            cfg,
            ids,
            labels.len(),
            train,
            holdout,
            &packed,
            n_feats,
            own_feat,
            min_leaf,
        ),
    };

    // Rank the positive-gain inputs by gain; the driver shortlist
    // is the prefix of that ranking clearing the driver floor.
    let total: f64 = gains.iter().sum();
    let mut ranked: Vec<(usize, f64)> = gains
        .iter()
        .enumerate()
        .filter(|(_, gv)| **gv > 0.0)
        .map(|(f, gv)| (f, *gv))
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let ranked_shares: Vec<(String, f64)> = ranked
        .iter()
        .map(|(f, gv)| {
            (
                enc.feat_names[selected[*f]].clone(),
                if total > 0.0 { gv / total } else { 0.0 },
            )
        })
        .collect();
    let driver_idx = select_driver_idx(&ranked, total);
    let drivers: Vec<(String, f64)> = driver_idx
        .iter()
        .map(|&i| ranked_shares[i].clone())
        .collect();

    // Validate the shortlist: when it is a proper subset of the
    // inputs and the full result clears the finding bars, refit on
    // the drivers alone (same split, same configuration) so the
    // claim's headline score is measured on the named drivers, not
    // inferred from their gain shares.
    let finding_strength = if matches!(target, TargetKind::Categorical(_, _)) {
        metric >= 0.7 && metric >= baseline + 0.05
    } else {
        metric >= 0.3
    };
    let (mut reduced_metric, mut reduced_rounds) = (None, None);
    if finding_strength && !driver_idx.is_empty() && driver_idx.len() < ranked.len() {
        let sub_feats: Vec<usize> = driver_idx.iter().map(|&i| selected[ranked[i].0]).collect();
        let sub_packed = pack_features(enc, &sub_feats);
        let (rm, rr) = match &target {
            TargetKind::Numeric(ys) => {
                let (m, r, _g, _t, _b) = boost_numeric(
                    enc.n,
                    cfg,
                    ys,
                    train,
                    holdout,
                    &sub_packed,
                    sub_feats.len(),
                    None,
                    min_leaf,
                );
                (m, r)
            }
            TargetKind::Categorical(ids, labels) => {
                let (m, _b, r, _g) = boost_categorical(
                    enc.n,
                    cfg,
                    ids,
                    labels.len(),
                    train,
                    holdout,
                    &sub_packed,
                    sub_feats.len(),
                    None,
                    min_leaf,
                );
                (m, r)
            }
        };
        if rm.is_finite() {
            reduced_metric = Some(rm);
            reduced_rounds = Some(rr);
        }
    }

    Some(TargetResult {
        col: ci,
        name: table.names[ci].clone(),
        is_categorical: matches!(target, TargetKind::Categorical(_, _)),
        metric,
        baseline,
        n_train: train.len(),
        n_holdout: holdout.len(),
        rounds,
        top_features: ranked_shares.iter().take(3).cloned().collect(),
        inputs: gains
            .iter()
            .enumerate()
            .filter(|(_, gain)| **gain > 0.0)
            .map(|(f, _)| enc.feat_names[selected[f]].clone())
            .collect(),
        drivers,
        reduced_metric,
        reduced_rounds,
        cat_ids: cat_payload,
    })
}

/// The result of screening one column's missingness indicator:
/// how well the other columns identify the rows where the column
/// is absent, measured on a held-out quarter.
pub struct IndicatorScreen {
    /// Holdout AUC of the boosted indicator model.
    pub auc: f64,
    /// Driver columns (name, share of total split gain), strongest
    /// first, under the same shortlist bars as the target screens.
    pub drivers: Vec<(String, f64)>,
    pub n_train: usize,
    pub n_holdout: usize,
    pub rounds: usize,
}

/// AUC of `scores` against a boolean label, by the rank-sum
/// identity with average ranks for tied scores. None when either
/// class is absent.
fn auc_rank_sum(scores: &[f64], labels: &[bool]) -> Option<f64> {
    let n_pos = labels.iter().filter(|&&l| l).count();
    let n_neg = labels.len() - n_pos;
    if n_pos == 0 || n_neg == 0 {
        return None;
    }
    let mut order: Vec<usize> = (0..scores.len()).collect();
    order.sort_by(|&a, &b| scores[a].total_cmp(&scores[b]).then(a.cmp(&b)));
    let mut rank_sum_pos = 0.0f64;
    let mut i = 0usize;
    while i < order.len() {
        let mut j = i + 1;
        while j < order.len() && scores[order[j]] == scores[order[i]] {
            j += 1;
        }
        // Ranks i+1..=j (1-based), averaged over the tie block.
        let avg_rank = (i + 1 + j) as f64 / 2.0;
        for &idx in &order[i..j] {
            if labels[idx] {
                rank_sum_pos += avg_rank;
            }
        }
        i = j;
    }
    let auc =
        (rank_sum_pos - n_pos as f64 * (n_pos as f64 + 1.0) / 2.0) / (n_pos as f64 * n_neg as f64);
    Some(auc)
}

/// Screen one column's missingness indicator against every other
/// column: fit the house GBM (squared loss on the 0/1 indicator)
/// on a seeded 75/25 split of the encoded rows and measure the
/// holdout AUC. The target column's own features and the columns
/// in `excluded_cols` never enter the predictor pool. Returns
/// None when the indicator carries no learnable signal (holdout
/// R² at or below zero) or the split lacks either class in
/// usable numbers.
pub fn screen_missing_indicator(
    table: &TypedTable,
    enc: &EncodedMatrix,
    ci: usize,
    missing: &[bool],
    cfg: &ScreenConfig,
    excluded_cols: &[usize],
) -> Option<IndicatorScreen> {
    if missing.len() != enc.n || enc.n < 100 {
        return None;
    }
    let n_pos_total = missing.iter().filter(|&&m| m).count();
    if n_pos_total == 0 || n_pos_total == enc.n {
        return None;
    }
    let y: Vec<f32> = missing.iter().map(|&m| if m { 1.0 } else { 0.0 }).collect();

    // Deterministic holdout, seeded like the target screens but
    // salted so the split differs from the column's own screen.
    let mut rng = Lcg(0x9E37_79B9_7F4A_7C15
        ^ (ci as u64).wrapping_mul(0x853C49E6748FEA9B)
        ^ 0x4D49_5353_494E_4721);
    let mut order: Vec<u32> = (0..enc.n as u32).collect();
    for i in (1..order.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order.truncate(cfg.max_model_rows.max(cfg.min_target_rows));
    let n_hold = order.len() / 4;
    let (train, holdout) = order.split_at(order.len() - n_hold);
    let count_pos = |rows: &[u32]| rows.iter().filter(|&&r| missing[r as usize]).count();
    if count_pos(holdout) < 5 || count_pos(holdout) == holdout.len() {
        return None;
    }
    if count_pos(train) < 10 || count_pos(train) == train.len() {
        return None;
    }
    let min_leaf = (train.len() / 200).clamp(10, 1_000);

    let mut selected: Vec<usize> = (0..enc.bins.len())
        .filter(|&f| enc.feat_cols[f] != ci && !excluded_cols.contains(&enc.feat_cols[f]))
        .collect();
    if selected.len() > cfg.max_predictors {
        let ind_bins: Vec<u8> = missing.iter().map(|&m| u8::from(m)).collect();
        let screen_rows: Vec<u32> = train.iter().take(2048).copied().collect();
        let mut scored: Vec<(usize, f64)> = selected
            .iter()
            .map(|&f| {
                (
                    f,
                    crate::screen::information(&ind_bins, &enc.bins[f], &screen_rows),
                )
            })
            .collect();
        scored.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then(enc.feat_names[a.0].cmp(&enc.feat_names[b.0]))
        });
        selected = scored.into_iter().map(|(f, _)| f).collect();
        selected.truncate(cfg.max_predictors);
    }
    if selected.is_empty() {
        return None;
    }
    let n_feats = selected.len();
    let packed = pack_features(enc, &selected);
    let (r2, rounds, gains, trees, base) = boost_numeric(
        enc.n, cfg, &y, train, holdout, &packed, n_feats, None, min_leaf,
    );
    if r2.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return None;
    }
    let model = FittedGbm {
        selected: selected.clone(),
        best_gains: gains.clone(),
        holdout_r2: r2,
        n_holdout: holdout.len(),
        rounds,
        present: Vec::new(),
        y: Vec::new(),
        base,
        learning_rate: cfg.learning_rate,
        trees,
    };
    let scores: Vec<f64> = holdout
        .iter()
        .map(|&p| {
            let row = &packed[p as usize * n_feats..(p as usize + 1) * n_feats];
            model.predict_row(row)
        })
        .collect();
    let labels: Vec<bool> = holdout.iter().map(|&p| missing[p as usize]).collect();
    let auc = auc_rank_sum(&scores, &labels)?;

    // Aggregate split gains to the column level before ranking:
    // a column can own several encoded features, and the claim
    // names columns.
    let mut by_col: Vec<(usize, f64)> = Vec::new();
    for (f, gv) in gains.iter().enumerate() {
        if *gv <= 0.0 {
            continue;
        }
        let col = enc.feat_cols[selected[f]];
        match by_col.iter_mut().find(|(c, _)| *c == col) {
            Some(entry) => entry.1 += gv,
            None => by_col.push((col, *gv)),
        }
    }
    let total: f64 = by_col.iter().map(|(_, g)| g).sum();
    by_col.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let driver_idx = select_driver_idx(&by_col, total);
    let drivers: Vec<(String, f64)> = driver_idx
        .iter()
        .map(|&i| {
            let (col, gv) = by_col[i];
            (
                table.names[col].clone(),
                if total > 0.0 { gv / total } else { 0.0 },
            )
        })
        .collect();
    if drivers.is_empty() {
        return None;
    }
    Some(IndicatorScreen {
        auc,
        drivers,
        n_train: train.len(),
        n_holdout: holdout.len(),
        rounds,
    })
}

/// Out-of-fold residuals from cross-fitting the house GBM to one
/// designated target: every modelled row's prediction comes from
/// the fold model that held the row out, so the residuals measure
/// the model's error on rows it never fit. `positions` are
/// encoded-space rows in ascending order; `signed` and `abs` are
/// the residuals against them (target − prediction). For a binary
/// categorical target the prediction is the predicted probability
/// of the second label (labels are in frequency order) and the
/// residual is the 0/1 outcome minus that probability.
pub struct CrossfitOutcome {
    pub positions: Vec<u32>,
    pub signed: Vec<f64>,
    pub abs: Vec<f64>,
    /// The target's value per position (the numeric value, or the
    /// 0/1 outcome of the second label for a binary target) and the
    /// model's out-of-fold prediction for it (the predicted value,
    /// or the predicted event probability).
    pub actual: Vec<f64>,
    pub pred: Vec<f64>,
    pub categorical: bool,
    /// Class labels for a categorical target (frequency order);
    /// empty for a numeric target.
    pub labels: Vec<String>,
    /// Held-out skill pooled over folds: R² for a numeric target,
    /// AUC for a binary target.
    pub skill: f64,
    /// The no-skill reference the screen reports beside `skill`:
    /// the majority-class rate for a binary target, 0 for numeric.
    pub baseline: f64,
    pub overall_bias: f64,
    pub overall_mae: f64,
    /// Binary targets only: the overall event (second-label) rate
    /// and mean predicted probability — the baseline pair a bias
    /// statement is read against. Both 0 for numeric targets.
    pub event_rate: f64,
    pub mean_pred: f64,
    /// Boosting rounds at best, per fold.
    pub fold_rounds: Vec<usize>,
}

/// The categorical boosting loop with the score matrix snapshotted
/// at the best round, so a caller can turn the held-out ensemble's
/// scores into probabilities. Mirrors `boost_categorical` exactly
/// (same rounds, same early stopping); the one addition is that
/// `extra_rows` — rows the caller wants predictions for, such as a
/// cross-fitting fold's held-out rows — receive the same per-round
/// score updates the holdout rows receive for evaluation. They are
/// never fit on. Returns (best holdout accuracy, majority
/// baseline, rounds at best, scores at best).
#[allow(clippy::too_many_arguments)]
fn boost_categorical_scored(
    n_rows: usize,
    cfg: &ScreenConfig,
    ids: &[u8],
    k: usize,
    train: &[u32],
    holdout: &[u32],
    extra_rows: &[u32],
    packed: &[u8],
    n_feats: usize,
    own_feat: Option<usize>,
    min_leaf: usize,
) -> (f64, f64, usize, Vec<Vec<f32>>) {
    let mut class_count = vec![0u32; k];
    for &p in train {
        class_count[ids[p as usize] as usize] += 1;
    }
    let prior: Vec<f32> = class_count
        .iter()
        .map(|&x| x as f32 / train.len() as f32)
        .collect();
    let base: Vec<f32> = prior.iter().map(|&p| (p / (1.0 - p + 1e-9)).ln()).collect();
    let mut scores = vec![base.clone(); n_rows];
    let mut best_scores = scores.clone();
    let baseline = prior.iter().cloned().fold(f32::MIN, f32::max) as f64;
    let mut best_metric = 0.0f64;
    let mut best_rounds = 0;
    let mut stale = 0;
    let max_rounds = if k > 5 {
        cfg.max_rounds / 2
    } else {
        cfg.max_rounds
    };
    for round in 1..=max_rounds {
        // Class-major sweeps over row-major `scores` (n_rows × k):
        // the range loop over classes is intentional.
        #[allow(clippy::needless_range_loop)]
        for c in 0..k {
            let mut g = vec![0.0f32; n_rows];
            let mut h = vec![0.0f32; n_rows];
            for &p in train {
                let z = scores[p as usize][c];
                let pc = 1.0 / (1.0 + (-z).exp());
                let yc = if ids[p as usize] as usize == c {
                    1.0
                } else {
                    0.0
                };
                g[p as usize] = pc - yc;
                h[p as usize] = pc * (1.0 - pc);
            }
            let (root, nodes, _gains) = grow_tree(
                packed,
                n_feats,
                train,
                &g,
                &h,
                own_feat,
                cfg.lambda,
                min_leaf,
                cfg.max_depth,
            );
            for &p in train.iter().chain(holdout.iter()).chain(extra_rows.iter()) {
                scores[p as usize][c] +=
                    cfg.learning_rate * tree_predict(&nodes, root, packed, n_feats, p as usize);
            }
        }
        // Holdout accuracy (argmax score).
        let correct = holdout
            .iter()
            .filter(|&&p| {
                let mut best_c = 0usize;
                let mut best_s = f32::MIN;
                for (c, s) in scores[p as usize].iter().enumerate() {
                    if *s > best_s {
                        best_s = *s;
                        best_c = c;
                    }
                }
                best_c == ids[p as usize] as usize
            })
            .count();
        let acc = correct as f64 / holdout.len().max(1) as f64;
        if acc > best_metric + 1e-4 {
            best_metric = acc;
            best_rounds = round;
            best_scores = scores.clone();
            stale = 0;
        } else {
            stale += 1;
            if stale >= cfg.patience {
                break;
            }
        }
    }
    (best_metric, baseline, best_rounds, best_scores)
}

/// Quantile-bin a numeric target into at most 32 bins over its
/// present rows (MISSING elsewhere), as the reference vector for
/// the predictor shortlist's information score when the target is
/// not itself an encoded feature.
fn bin_numeric_target(ys: &[f32], present: &[u32]) -> Vec<u8> {
    let mut bins = vec![MISSING; ys.len()];
    let mut sorted: Vec<f64> = present.iter().map(|&p| ys[p as usize] as f64).collect();
    if sorted.is_empty() {
        return bins;
    }
    sorted.sort_by(|a, b| a.total_cmp(b));
    let n_bins = 32.min(sorted.len());
    for &p in present {
        let v = ys[p as usize] as f64;
        // Rank of v within the sorted present values.
        let r = sorted.partition_point(|&x| x < v);
        let b = (r * n_bins / sorted.len()).min(n_bins - 1);
        bins[p as usize] = b as u8;
    }
    bins
}

/// Cross-fit the house GBM to the target in column `ci` and return
/// every modelled row's out-of-fold residual. The fitting procedure
/// is the predictability screen's — same target preparation, same
/// per-fold predictor shortlist (information score on the fold's
/// training rows, capped at `cfg.max_predictors`), same boosting
/// loop with an internal 75/25 early-stopping split of the fold's
/// training rows. Each fold's predictions for its held-out rows
/// come from that fold's best-round ensemble.
///
/// Errors name why no residuals could be produced (the caller
/// records the reason in coverage): an unfittable target, a
/// categorical target with more than two classes (this evaluation
/// covers numeric and binary targets), too few present rows, too
/// few rows in the rarer class, or no predictor columns.
pub fn crossfit_residuals(
    table: &TypedTable,
    enc: &EncodedMatrix,
    ci: usize,
    cfg: &ScreenConfig,
    excluded_cols: &[usize],
    folds: usize,
) -> Result<CrossfitOutcome, String> {
    let folds = folds.max(2);
    let target = prepare_target(table, enc, ci, cfg).ok_or_else(|| {
        "the designated target is not a fittable column (constant, unsupported kind, or too many classes)"
            .to_string()
    })?;
    let (present, labels): (Vec<u32>, Vec<String>) = match &target {
        TargetKind::Numeric(ys) => (
            (0..enc.n)
                .filter(|&j| !ys[j].is_nan())
                .map(|j| j as u32)
                .collect(),
            Vec::new(),
        ),
        TargetKind::Categorical(ids, labels) => {
            if labels.len() != 2 {
                return Err(format!(
                    "categorical target '{}' has {} classes; the model-failure pass evaluates numeric and binary targets",
                    table.names[ci],
                    labels.len()
                ));
            }
            (
                (0..enc.n)
                    .filter(|&j| ids[j] != MISSING)
                    .map(|j| j as u32)
                    .collect(),
                labels.clone(),
            )
        }
    };
    if present.len() < 200 {
        return Err(format!(
            "fewer than 200 rows with a present target ({})",
            present.len()
        ));
    }
    if let TargetKind::Categorical(ids, _) = &target {
        let n_event = present.iter().filter(|&&p| ids[p as usize] == 1).count();
        let n_other = present.len() - n_event;
        if n_event.min(n_other) < 20 {
            return Err(format!(
                "the rarer class of '{}' has {} rows; too few to cross-fit",
                table.names[ci],
                n_event.min(n_other)
            ));
        }
    }
    // Reference bins for the predictor shortlist: the target's own
    // encoded feature when it has one, else bins built from the
    // prepared target.
    let own = enc.feat_cols.iter().position(|&c| c == ci);
    let target_bins: Vec<u8> = match &target {
        TargetKind::Numeric(ys) => bin_numeric_target(ys, &present),
        TargetKind::Categorical(ids, _) => ids.clone(),
    };
    let reference: &[u8] = match own {
        Some(o) => &enc.bins[o],
        None => &target_bins,
    };

    // Seeded shuffle of the present rows; fold f is every
    // `folds`-th row of the shuffled order.
    let mut rng = Lcg(0x9E37_79B9_7F4A_7C15
        ^ (ci as u64).wrapping_mul(0x853C49E6748FEA9B)
        ^ 0x4D4F_4445_4C46_4149);
    let mut order = present.clone();
    for i in (1..order.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order.truncate((cfg.max_model_rows * 4).max(cfg.min_target_rows));
    let fold_of: Vec<usize> = {
        let mut f = vec![0usize; enc.n];
        for (i, &p) in order.iter().enumerate() {
            f[p as usize] = i % folds;
        }
        f
    };
    let modelled: Vec<u32> = order.clone();

    let mut signed = vec![f64::NAN; enc.n];
    let mut fold_rounds: Vec<usize> = Vec::new();
    for fold in 0..folds {
        let test: Vec<u32> = modelled
            .iter()
            .copied()
            .filter(|&p| fold_of[p as usize] == fold)
            .collect();
        let pool: Vec<u32> = modelled
            .iter()
            .copied()
            .filter(|&p| fold_of[p as usize] != fold)
            .collect();
        if test.is_empty() || pool.len() < 100 {
            continue;
        }
        // Internal 75/25 split of the training pool for early
        // stopping, seeded per fold from the same stream discipline
        // as the screens (a fixed salt per fold index).
        let mut frng = Lcg(0x9E37_79B9_7F4A_7C15
            ^ (ci as u64).wrapping_mul(0x853C49E6748FEA9B)
            ^ 0x4D4F_4445_4C46_4149
            ^ ((fold as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
        let mut shuffled = pool.clone();
        for i in (1..shuffled.len()).rev() {
            let j = (frng.next_u64() % (i as u64 + 1)) as usize;
            shuffled.swap(i, j);
        }
        let n_hold = shuffled.len() / 4;
        let (train, holdout) = shuffled.split_at(shuffled.len() - n_hold);
        let train: Vec<u32> = train.to_vec();
        let holdout: Vec<u32> = holdout.to_vec();
        let min_leaf = (train.len() / 200).clamp(10, 1_000);

        // Predictor shortlist on this fold's training rows only.
        let screen_rows: Vec<u32> = train.iter().take(2048).copied().collect();
        let mut selected: Vec<usize> = (0..enc.bins.len())
            .filter(|&f| enc.feat_cols[f] != ci && !excluded_cols.contains(&enc.feat_cols[f]))
            .collect();
        if selected.len() > cfg.max_predictors {
            let mut scored: Vec<(usize, f64)> = selected
                .iter()
                .map(|&f| {
                    (
                        f,
                        crate::screen::information(reference, &enc.bins[f], &screen_rows),
                    )
                })
                .collect();
            scored.sort_by(|a, b| {
                b.1.total_cmp(&a.1)
                    .then(enc.feat_names[a.0].cmp(&enc.feat_names[b.0]))
            });
            selected = scored.into_iter().map(|(f, _)| f).collect();
            selected.truncate(cfg.max_predictors);
        }
        if selected.is_empty() {
            return Err("no predictor columns available for the target".into());
        }
        let n_feats = selected.len();
        let packed = pack_features(enc, &selected);
        match &target {
            TargetKind::Numeric(ys) => {
                let (_r2, rounds, _gains, trees, base) = boost_numeric(
                    enc.n, cfg, ys, &train, &holdout, &packed, n_feats, None, min_leaf,
                );
                fold_rounds.push(rounds);
                for &p in &test {
                    let mut pred = base as f64;
                    for tree in &trees {
                        pred += cfg.learning_rate as f64
                            * tree_predict(tree, 0, &packed, n_feats, p as usize) as f64;
                    }
                    signed[p as usize] = ys[p as usize] as f64 - pred;
                }
            }
            TargetKind::Categorical(ids, _) => {
                let (_acc, _baseline, rounds, scores) = boost_categorical_scored(
                    enc.n, cfg, ids, 2, &train, &holdout, &test, &packed, n_feats, None, min_leaf,
                );
                fold_rounds.push(rounds);
                for &p in &test {
                    let z = scores[p as usize][1] as f64 - scores[p as usize][0] as f64;
                    let p1 = 1.0 / (1.0 + (-z).exp());
                    let y01 = if ids[p as usize] == 1 { 1.0 } else { 0.0 };
                    signed[p as usize] = y01 - p1;
                }
            }
        }
    }

    // Assemble the outcome over the rows that received a prediction.
    let mut positions: Vec<u32> = Vec::new();
    let mut out_signed: Vec<f64> = Vec::new();
    let mut out_abs: Vec<f64> = Vec::new();
    let mut out_actual: Vec<f64> = Vec::new();
    let mut out_pred: Vec<f64> = Vec::new();
    for &p in &modelled {
        let r = signed[p as usize];
        if r.is_finite() {
            positions.push(p);
            out_signed.push(r);
            out_abs.push(r.abs());
            let actual = match &target {
                TargetKind::Numeric(ys) => ys[p as usize] as f64,
                TargetKind::Categorical(ids, _) => {
                    if ids[p as usize] == 1 {
                        1.0
                    } else {
                        0.0
                    }
                }
            };
            out_actual.push(actual);
            out_pred.push(actual - r);
        }
    }
    if positions.len() < 200 {
        return Err(format!(
            "cross-fitting produced residuals for {} rows; too few to search",
            positions.len()
        ));
    }
    // `positions` in ascending encoded order for a stable search.
    let mut idx: Vec<usize> = (0..positions.len()).collect();
    idx.sort_by_key(|&i| positions[i]);
    let positions: Vec<u32> = idx.iter().map(|&i| positions[i]).collect();
    let out_signed: Vec<f64> = idx.iter().map(|&i| out_signed[i]).collect();
    let out_abs: Vec<f64> = idx.iter().map(|&i| out_abs[i]).collect();
    let out_actual: Vec<f64> = idx.iter().map(|&i| out_actual[i]).collect();
    let out_pred: Vec<f64> = idx.iter().map(|&i| out_pred[i]).collect();

    let n = out_signed.len() as f64;
    let overall_bias = out_signed.iter().sum::<f64>() / n;
    let overall_mae = out_abs.iter().sum::<f64>() / n;
    let (skill, baseline, event_rate, mean_pred) = match &target {
        TargetKind::Numeric(ys) => {
            let mean_y = positions
                .iter()
                .map(|&p| ys[p as usize] as f64)
                .sum::<f64>()
                / n;
            let sse: f64 = out_signed.iter().map(|r| r * r).sum();
            let sst: f64 = positions
                .iter()
                .map(|&p| (ys[p as usize] as f64 - mean_y).powi(2))
                .sum();
            let r2 = if sst > 0.0 { 1.0 - sse / sst } else { 0.0 };
            (r2, 0.0, 0.0, 0.0)
        }
        TargetKind::Categorical(ids, _) => {
            let labs: Vec<bool> = positions.iter().map(|&p| ids[p as usize] == 1).collect();
            let auc = auc_rank_sum(&out_pred, &labs).unwrap_or(0.5);
            let event_rate = labs.iter().filter(|&&l| l).count() as f64 / n;
            let mean_pred = out_pred.iter().sum::<f64>() / n;
            (auc, event_rate.max(1.0 - event_rate), event_rate, mean_pred)
        }
    };
    Ok(CrossfitOutcome {
        positions,
        signed: out_signed,
        abs: out_abs,
        actual: out_actual,
        pred: out_pred,
        categorical: matches!(target, TargetKind::Categorical(..)),
        labels,
        skill,
        baseline,
        overall_bias,
        overall_mae,
        event_rate,
        mean_pred,
        fold_rounds,
    })
}

/// The categorical boosting loop: one-versus-rest logistic trees
/// per class, early-stopped on holdout accuracy. Returns (best
/// holdout accuracy, majority baseline, rounds at best, cumulative
/// gains at best).
#[allow(clippy::too_many_arguments)]
fn boost_categorical(
    n_rows: usize,
    cfg: &ScreenConfig,
    ids: &[u8],
    k: usize,
    train: &[u32],
    holdout: &[u32],
    packed: &[u8],
    n_feats: usize,
    own_feat: Option<usize>,
    min_leaf: usize,
) -> (f64, f64, usize, Vec<f64>) {
    let mut class_count = vec![0u32; k];
    for &p in train {
        class_count[ids[p as usize] as usize] += 1;
    }
    let prior: Vec<f32> = class_count
        .iter()
        .map(|&x| x as f32 / train.len() as f32)
        .collect();
    let base: Vec<f32> = prior.iter().map(|&p| (p / (1.0 - p + 1e-9)).ln()).collect();
    let mut scores = vec![base.clone(); n_rows];
    let baseline = prior.iter().cloned().fold(f32::MIN, f32::max) as f64;
    let mut cum_gains = vec![0.0f64; n_feats];
    let mut best_gains = cum_gains.clone();
    let mut best_metric = 0.0f64;
    let mut best_rounds = 0;
    let mut stale = 0;
    let max_rounds = if k > 5 {
        cfg.max_rounds / 2
    } else {
        cfg.max_rounds
    };
    for round in 1..=max_rounds {
        // Class-major sweeps over row-major `scores` (n_rows × k):
        // the range loop over classes is intentional.
        #[allow(clippy::needless_range_loop)]
        for c in 0..k {
            let mut g = vec![0.0f32; n_rows];
            let mut h = vec![0.0f32; n_rows];
            for &p in train {
                let z = scores[p as usize][c];
                let pc = 1.0 / (1.0 + (-z).exp());
                let yc = if ids[p as usize] as usize == c {
                    1.0
                } else {
                    0.0
                };
                g[p as usize] = pc - yc;
                h[p as usize] = pc * (1.0 - pc);
            }
            let (root, nodes, gains) = grow_tree(
                packed,
                n_feats,
                train,
                &g,
                &h,
                own_feat,
                cfg.lambda,
                min_leaf,
                cfg.max_depth,
            );
            for f in 0..gains.len() {
                cum_gains[f] += gains[f];
            }
            for &p in train.iter().chain(holdout.iter()) {
                scores[p as usize][c] +=
                    cfg.learning_rate * tree_predict(&nodes, root, packed, n_feats, p as usize);
            }
        }
        // Holdout accuracy (argmax score).
        let correct = holdout
            .iter()
            .filter(|&&p| {
                let mut best_c = 0usize;
                let mut best_s = f32::MIN;
                for (c, s) in scores[p as usize].iter().enumerate() {
                    if *s > best_s {
                        best_s = *s;
                        best_c = c;
                    }
                }
                best_c == ids[p as usize] as usize
            })
            .count();
        let acc = correct as f64 / holdout.len().max(1) as f64;
        if acc > best_metric + 1e-4 {
            best_metric = acc;
            best_rounds = round;
            best_gains = cum_gains.clone();
            stale = 0;
        } else {
            stale += 1;
            if stale >= cfg.patience {
                break;
            }
        }
    }
    (best_metric, baseline, best_rounds, best_gains)
}

/// The numeric boosting loop, shared by `screen_target` (which
/// reports its metric and gains) and `fit_gbm_model` (which also
/// keeps the ensemble). Runs the same rounds with the same early
/// stopping; snapshots the trees whenever the holdout R² improves,
/// so the returned ensemble is the one that produced the returned
/// metric. Returns (holdout R², rounds at best, cumulative gains at
/// best, trees at best, base prediction); the R² is not positive
/// when no round improved on the mean, and callers that require a
/// positive score check it.
/// (best holdout metric, best round count, gains at best, trees at
/// best, base prediction) from a numeric boosting run.
type BoostNumeric = (f64, usize, Vec<f64>, Vec<Vec<Node>>, f32);

#[allow(clippy::too_many_arguments)]
fn boost_numeric(
    n_rows: usize,
    cfg: &ScreenConfig,
    y: &[f32],
    train: &[u32],
    holdout: &[u32],
    packed: &[u8],
    n_feats: usize,
    own_feat: Option<usize>,
    min_leaf: usize,
) -> BoostNumeric {
    let base: f32 = train.iter().map(|&p| y[p as usize]).sum::<f32>() / train.len() as f32;
    let mut pred = vec![base; n_rows];
    let mut cum_gains = vec![0.0f64; n_feats];
    let mut best_gains = cum_gains.clone();
    let mut best_metric = f64::MIN;
    let mut best_rounds = 0;
    let mut trees: Vec<Vec<Node>> = Vec::new();
    let mut best_trees: Vec<Vec<Node>> = Vec::new();
    let mut stale = 0;
    for round in 1..=cfg.max_rounds {
        let mut g = vec![0.0f32; n_rows];
        let h = vec![1.0f32; n_rows];
        for &p in train {
            g[p as usize] = pred[p as usize] - y[p as usize];
        }
        let (root, nodes, gains) = grow_tree(
            packed,
            n_feats,
            train,
            &g,
            &h,
            own_feat,
            cfg.lambda,
            min_leaf,
            cfg.max_depth,
        );
        for f in 0..gains.len() {
            cum_gains[f] += gains[f];
        }
        for &p in train.iter().chain(holdout.iter()) {
            pred[p as usize] +=
                cfg.learning_rate * tree_predict(&nodes, root, packed, n_feats, p as usize);
        }
        trees.push(nodes);
        let sse: f64 = holdout
            .iter()
            .map(|&p| {
                let d = (pred[p as usize] - y[p as usize]) as f64;
                d * d
            })
            .sum();
        let sst: f64 = holdout
            .iter()
            .map(|&p| (y[p as usize] as f64 - base as f64).powi(2))
            .sum();
        let r2 = if sst > 0.0 { 1.0 - sse / sst } else { 0.0 };
        if r2 > best_metric + 1e-4 {
            best_metric = r2;
            best_rounds = round;
            best_gains = cum_gains.clone();
            best_trees = trees.clone();
            stale = 0;
        } else {
            stale += 1;
            if stale >= cfg.patience {
                break;
            }
        }
    }
    (best_metric, best_rounds, best_gains, best_trees, base)
}

fn tree_predict_row(nodes: &[Node], row: &[u8]) -> f32 {
    let mut idx = 0usize;
    while nodes[idx].feat != u32::MAX {
        let n = &nodes[idx];
        idx = if row[n.feat as usize] <= n.thr {
            n.left as usize
        } else {
            n.right as usize
        };
    }
    nodes[idx].leaf
}

/// A fitted numeric screen model, retained for partial-dependence
/// work (the H-statistic pass). Produced by the same procedure the
/// predictability screen uses — same target preparation, seeded
/// split, predictor pre-screen and boosting loop — so a target's
/// model here is the model its screen result describes.
pub struct FittedGbm {
    /// Feature indices into `EncodedMatrix::bins`, in packed order.
    pub selected: Vec<usize>,
    /// Split gains per selected feature, at the best round.
    pub best_gains: Vec<f64>,
    pub holdout_r2: f64,
    pub n_holdout: usize,
    pub rounds: usize,
    /// Encoded-space rows where the target is present, and the
    /// target's values in encoded space (NaN where absent) — the
    /// coordinate system `EncodedMatrix::bins` uses, which is a
    /// row sample of the table at larger sizes.
    pub present: Vec<u32>,
    pub y: Vec<f32>,
    base: f32,
    learning_rate: f32,
    trees: Vec<Vec<Node>>,
}

impl FittedGbm {
    /// Predict one packed row (bin values in `selected` order).
    pub fn predict_row(&self, row: &[u8]) -> f64 {
        let mut pred = self.base as f64;
        for tree in &self.trees {
            pred += self.learning_rate as f64 * tree_predict_row(tree, row) as f64;
        }
        pred
    }
}

// ---------- drift ----------

/// Chi-square statistic of a 2x2 contingency table `[[a, b], [c, d]]`.
/// Returns NaN when a margin is zero (the table carries no routing
/// information).
pub fn chi2_2x2(a: f64, b: f64, c: f64, d: f64) -> f64 {
    let n = a + b + c + d;
    let denom = (a + b) * (c + d) * (a + c) * (b + d);
    if n == 0.0 || denom == 0.0 {
        return f64::NAN;
    }
    let diff = a * d - b * c;
    n * diff * diff / denom
}

impl FittedGbm {
    /// Drift of a comparison row set against a reference row set, as
    /// seen by this ensemble: for every split, the reference and
    /// comparison counts routed to each child form a 2x2 table, and
    /// the score is the mean chi-square across splits. Rows are bin
    /// values in `selected` order, as `predict_row` takes, and are
    /// routed with the same comparison prediction uses. Under no
    /// drift the statistic averages ~1 (chi-square, 1 df); for a
    /// fixed proportional shift it grows with row counts, so scores
    /// are comparable only between row sets of equal size.
    ///
    /// `concept = true` restricts the average to splits with a leaf
    /// child (the splits closest to the prediction surface);
    /// `false` averages over all splits.
    pub fn drift_score(&self, reference: &[Vec<u8>], comparison: &[Vec<u8>], concept: bool) -> f64 {
        // Splits are weighted by their feature's share of the model's
        // total gain: a split on a feature the model barely uses (a
        // noise split picked up while chasing residuals) must not be
        // able to produce a drift signal when that feature's
        // distribution moves. With no recorded gains the weighting
        // falls back to the plain mean.
        let total_gain: f64 = self.best_gains.iter().sum();
        let mut stats: Vec<(f64, f64)> = Vec::new();
        for tree in &self.trees {
            // (left, right) routed counts per node index, per row set.
            let mut ref_counts = vec![(0u64, 0u64); tree.len()];
            let mut new_counts = vec![(0u64, 0u64); tree.len()];
            for (rows, counts) in [(reference, &mut ref_counts), (comparison, &mut new_counts)] {
                for row in rows {
                    let mut idx = 0usize;
                    while tree[idx].feat != u32::MAX {
                        let node = &tree[idx];
                        if row[node.feat as usize] <= node.thr {
                            counts[idx].0 += 1;
                            idx = node.left as usize;
                        } else {
                            counts[idx].1 += 1;
                            idx = node.right as usize;
                        }
                    }
                }
            }
            for (idx, node) in tree.iter().enumerate() {
                if node.feat == u32::MAX {
                    continue;
                }
                if concept {
                    let left_leaf = tree[node.left as usize].feat == u32::MAX;
                    let right_leaf = tree[node.right as usize].feat == u32::MAX;
                    if !left_leaf && !right_leaf {
                        continue;
                    }
                }
                let (rl, rr) = ref_counts[idx];
                let (nl, nr) = new_counts[idx];
                if (rl + rr) == 0 || (nl + nr) == 0 {
                    continue;
                }
                let stat = chi2_2x2(rl as f64, nl as f64, rr as f64, nr as f64);
                if stat.is_finite() {
                    let w = if total_gain > 0.0 {
                        self.best_gains
                            .get(node.feat as usize)
                            .copied()
                            .unwrap_or(0.0)
                            / total_gain
                    } else {
                        1.0
                    };
                    stats.push((stat, w));
                }
            }
        }
        let wsum: f64 = stats.iter().map(|(_, w)| w).sum();
        if stats.is_empty() || wsum <= 0.0 {
            0.0
        } else {
            stats.iter().map(|(s, w)| s * w).sum::<f64>() / wsum
        }
    }
}

/// Fit the screen's numeric model for one target column and retain
/// the ensemble. Returns None for categorical targets, ineligible
/// targets, and targets whose best holdout R² is not positive —
/// the same conditions under which `screen_target` returns None
/// for a numeric target.
pub fn fit_gbm_model(
    table: &TypedTable,
    enc: &EncodedMatrix,
    ci: usize,
    cfg: &ScreenConfig,
) -> Option<FittedGbm> {
    fit_gbm_model_restricted(table, enc, ci, cfg, None, &[])
}

/// Like `fit_gbm_model`, but the target's present rows are first
/// restricted to `restrict` (encoded-space row indices) when given —
/// e.g. the reference period of a drift comparison — and the columns
/// in `exclude_cols` are withheld from the predictor set (a drift
/// reference model must not split on the time axis itself: later
/// periods route through such a split one-sidedly by construction,
/// which reads as maximal drift on stable data). Preparation, split,
/// shortlist and boosting are otherwise identical, so the retained
/// ensemble is the model the screen would have fit on that row
/// subset alone.
/// Fit a target's screen model on a restricted row set. `restrict`
/// holds encoded positions — the coordinate system of
/// `EncodedMatrix::bins` and of the returned `FittedGbm::present` —
/// not original table row ids.
pub fn fit_gbm_model_restricted(
    table: &TypedTable,
    enc: &EncodedMatrix,
    ci: usize,
    cfg: &ScreenConfig,
    restrict: Option<&std::collections::BTreeSet<u32>>,
    exclude_cols: &[usize],
) -> Option<FittedGbm> {
    let target = prepare_target(table, enc, ci, cfg)?;
    let TargetKind::Numeric(ys) = target else {
        return None;
    };
    let present: Vec<u32> = (0..enc.n)
        .filter(|&j| !ys[j].is_nan())
        .map(|j| j as u32)
        .filter(|j| restrict.is_none_or(|set| set.contains(j)))
        .collect();
    if present.len() < cfg.min_target_rows {
        return None;
    }
    let mut rng = Lcg(0x9E37_79B9_7F4A_7C15 ^ (ci as u64).wrapping_mul(0x853C49E6748FEA9B));
    let mut order = present.clone();
    for i in (1..order.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order.truncate(cfg.max_model_rows.max(cfg.min_target_rows));
    let n_hold = order.len() / 4;
    let (train, holdout) = order.split_at(order.len() - n_hold);
    let min_leaf = (train.len() / 200).clamp(10, 1_000);
    let own = enc.feat_cols.iter().position(|&c| c == ci)?;
    let screen_rows: Vec<u32> = train.iter().take(2048).copied().collect();
    let mut selected: Vec<usize> = (0..enc.bins.len())
        .filter(|&f| f != own && !exclude_cols.contains(&enc.feat_cols[f]))
        .collect();
    if selected.len() > cfg.max_predictors {
        // Pair each feature with its information score before
        // sorting: `selected` is not the full feature range once
        // exclusions apply, so scores cannot be indexed by feature
        // position.
        let mut scored: Vec<(usize, f64)> = selected
            .iter()
            .map(|&f| {
                (
                    f,
                    crate::screen::information(&enc.bins[own], &enc.bins[f], &screen_rows),
                )
            })
            .collect();
        scored.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then(enc.feat_names[a.0].cmp(&enc.feat_names[b.0]))
        });
        selected = scored.into_iter().map(|(f, _)| f).collect();
        selected.truncate(cfg.max_predictors);
    }
    if selected.is_empty() {
        return None;
    }
    let n_feats = selected.len();
    let mut packed = vec![MISSING; enc.n * n_feats];
    for (f, &original) in selected.iter().enumerate() {
        for r in 0..enc.n {
            packed[r * n_feats + f] = enc.bins[original][r];
        }
    }
    let (metric, rounds, gains, trees, base) = boost_numeric(
        enc.n, cfg, &ys, train, holdout, &packed, n_feats, None, min_leaf,
    );
    if metric <= 0.0 {
        return None;
    }
    Some(FittedGbm {
        selected,
        best_gains: gains,
        holdout_r2: metric,
        n_holdout: holdout.len(),
        rounds,
        present,
        y: ys,
        base,
        learning_rate: cfg.learning_rate,
        trees,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::{encode, EncodeConfig};
    use onceover_core::extract;
    use polars::df;

    #[test]
    fn recovers_planted_numeric_relationship() {
        let mut x1: Vec<Option<f64>> = Vec::new();
        let mut x2: Vec<Option<f64>> = Vec::new();
        let mut y: Vec<Option<f64>> = Vec::new();
        let mut rng = Lcg(42);
        for _ in 0..2000 {
            let a = rng.next_f64();
            let b = rng.next_f64();
            x1.push(Some(a));
            x2.push(Some(b));
            y.push(Some(a + 2.0 * b + 0.01 * rng.next_f64()));
        }
        let df = df! {
            "x1" => x1,
            "x2" => x2,
            "y" => y,
        }
        .unwrap();
        let table = extract(&df);
        let (enc, _) = encode(&table, &EncodeConfig::default());
        let out = screen(&table, &enc, &ScreenConfig::default(), &[]);
        let y_res = out.results.iter().find(|r| r.name == "y").unwrap();
        assert!(y_res.metric > 0.9, "r2 = {}", y_res.metric);
        assert!(out.findings.iter().any(|f| (f
            .is(onceover_report::Category::Dependencies, Some("predictive")))
            && f.claim.contains("y is predictable")));
    }

    #[test]
    fn recovers_planted_categorical_relationship() {
        let mut x1: Vec<Option<f64>> = Vec::new();
        let mut class: Vec<Option<&str>> = Vec::new();
        let mut rng = Lcg(7);
        for _ in 0..1500 {
            let a = rng.next_f64();
            x1.push(Some(a));
            if rng.next_f64() < 0.9 {
                class.push(Some(if a > 0.5 { "hi" } else { "lo" }));
            } else {
                class.push(Some(if a > 0.5 { "lo" } else { "hi" }));
            }
        }
        let df = df! {
            "x1" => x1,
            "class" => class,
        }
        .unwrap();
        let table = extract(&df);
        let (enc, _) = encode(&table, &EncodeConfig::default());
        let out = screen(&table, &enc, &ScreenConfig::default(), &[]);
        let res = out.results.iter().find(|r| r.name == "class").unwrap();
        assert!(res.metric > 0.85, "acc = {}", res.metric);
        assert!(res.metric > res.baseline + 0.2);
    }

    fn one_split_model() -> FittedGbm {
        // One tree: split feature 0 at bin 5; left leaf 0.0, right leaf 1.0.
        let tree = vec![
            Node {
                feat: 0,
                thr: 5,
                left: 1,
                right: 2,
                leaf: 0.0,
            },
            Node {
                feat: u32::MAX,
                thr: 0,
                left: 0,
                right: 0,
                leaf: 0.0,
            },
            Node {
                feat: u32::MAX,
                thr: 0,
                left: 0,
                right: 0,
                leaf: 1.0,
            },
        ];
        FittedGbm {
            selected: vec![0],
            best_gains: vec![0.0],
            holdout_r2: 0.0,
            n_holdout: 0,
            rounds: 1,
            present: vec![],
            y: vec![],
            base: 0.0,
            learning_rate: 1.0,
            trees: vec![tree],
        }
    }

    #[test]
    fn drift_score_zero_for_identical_rows() {
        let m = one_split_model();
        let rows: Vec<Vec<u8>> = (0..10).map(|i| vec![i as u8]).collect();
        assert_eq!(m.drift_score(&rows, &rows, false), 0.0);
        assert_eq!(m.drift_score(&rows, &rows, true), 0.0);
    }

    #[test]
    fn drift_score_maximal_for_opposite_routing() {
        let m = one_split_model();
        let reference: Vec<Vec<u8>> = (0..=5).map(|v| vec![v]).collect();
        let shifted: Vec<Vec<u8>> = (6..=11).map(|v| vec![v]).collect();
        // Table [[6, 0], [0, 6]]: chi-square = 12.
        let score = m.drift_score(&reference, &shifted, false);
        assert!((score - 12.0).abs() < 1e-9, "score = {score}");
    }

    #[test]
    fn chi2_2x2_known_values() {
        assert_eq!(chi2_2x2(10.0, 10.0, 10.0, 10.0), 0.0);
        assert!(chi2_2x2(0.0, 0.0, 3.0, 4.0).is_nan());
    }

    #[test]
    fn independent_target_not_flagged() {
        let mut a: Vec<Option<f64>> = Vec::new();
        let mut noise: Vec<Option<f64>> = Vec::new();
        let mut rng = Lcg(13);
        for _ in 0..1000 {
            a.push(Some(rng.next_f64()));
            noise.push(Some(rng.next_f64()));
        }
        let df = df! {
            "a" => a,
            "noise" => noise,
        }
        .unwrap();
        let table = extract(&df);
        let (enc, _) = encode(&table, &EncodeConfig::default());
        let out = screen(&table, &enc, &ScreenConfig::default(), &[]);
        assert!(out
            .results
            .iter()
            .all(|r| r.metric < 0.5 || r.baseline > 0.0 && r.is_categorical));
    }

    #[test]
    fn driver_rule_applies_relative_and_absolute_floors() {
        // (position, gain) in gain order; total gain 100.
        let ranked = vec![(0, 85.0), (1, 9.0), (2, 8.4), (3, 3.0)];
        // Floor is max(0.02, 0.10 * 0.85) = 0.085: position 1 (0.09)
        // qualifies, position 2 (0.084) does not.
        assert_eq!(select_driver_idx(&ranked, 100.0), vec![0, 1]);
    }

    #[test]
    fn driver_rule_absolute_floor_binds_when_gains_are_spread() {
        let ranked = vec![(0, 10.0), (1, 5.0), (2, 2.1), (3, 1.9)];
        // Top share 0.10 -> floor is max(0.02, 0.01) = 0.02:
        // shares 0.10, 0.05, 0.021 qualify; 0.019 does not.
        assert_eq!(select_driver_idx(&ranked, 100.0), vec![0, 1, 2]);
    }

    #[test]
    fn driver_rule_always_keeps_the_top_feature_and_caps_at_eight() {
        // Top share below the absolute floor (gains spread over
        // many features): the top feature still qualifies.
        let ranked = vec![(0, 1.5), (1, 1.4), (2, 1.3)];
        assert_eq!(select_driver_idx(&ranked, 100.0), vec![0]);
        // Twenty equal features: capped at DRIVER_MAX.
        let ranked: Vec<(usize, f64)> = (0..20).map(|i| (i, 5.0)).collect();
        assert_eq!(select_driver_idx(&ranked, 100.0).len(), DRIVER_MAX);
        assert!(select_driver_idx(&[], 0.0).is_empty());
    }

    fn driver_frame() -> polars::prelude::DataFrame {
        // y = x^2 plus six noise columns.
        let mut rng = Lcg(123);
        let mut x: Vec<Option<f64>> = Vec::new();
        let mut y: Vec<Option<f64>> = Vec::new();
        let mut n: Vec<Vec<Option<f64>>> = (0..6).map(|_| Vec::new()).collect();
        for _ in 0..3000 {
            let v = rng.next_f64();
            x.push(Some(v));
            y.push(Some(v * v + 0.01 * rng.next_f64()));
            for col in n.iter_mut() {
                col.push(Some(rng.next_f64()));
            }
        }
        df! {
            "x" => x, "y" => y,
            "n0" => n[0].clone(), "n1" => n[1].clone(), "n2" => n[2].clone(),
            "n3" => n[3].clone(), "n4" => n[4].clone(), "n5" => n[5].clone(),
        }
        .unwrap()
    }

    #[test]
    fn screen_claim_names_only_the_planted_driver() {
        let df = driver_frame();
        let table = extract(&df);
        let (enc, _) = encode(&table, &EncodeConfig::default());
        let out = screen(&table, &enc, &ScreenConfig::default(), &[]);
        let res = out.results.iter().find(|r| r.name == "y").unwrap();
        assert_eq!(res.drivers.len(), 1, "drivers: {:?}", res.drivers);
        assert_eq!(res.drivers[0].0, "x");
        let f = out
            .findings
            .iter()
            .find(|f| f.claim.starts_with("y is predictable"))
            .expect("a predictive finding for y");
        assert!(
            f.claim.starts_with("y is predictable from x ("),
            "claim: {}",
            f.claim
        );
        // When the full model used more inputs than the driver set,
        // a refit on the drivers alone is recorded and the claim
        // reports it.
        if res.inputs.len() > 1 {
            let red = res.reduced_metric.expect("a refit score is recorded");
            assert!(
                red > res.metric - 0.1,
                "reduced {red} vs full {}",
                res.metric
            );
            assert!(f.claim.contains("using only these drivers"));
        }
    }

    #[test]
    fn excluded_columns_leave_the_predictor_pool() {
        let df = driver_frame();
        let table = extract(&df);
        let (enc, _) = encode(&table, &EncodeConfig::default());
        let x_col = table.names.iter().position(|n| n == "x").unwrap();
        let out = screen(&table, &enc, &ScreenConfig::default(), &[x_col]);
        for r in &out.results {
            assert!(
                !r.inputs.iter().any(|i| i == "x"),
                "excluded column in inputs of {}: {:?}",
                r.name,
                r.inputs
            );
        }
        // With its only driver excluded, y is not predictable.
        assert!(out
            .findings
            .iter()
            .all(|f| !f.claim.starts_with("y is predictable")));
    }

    #[test]
    fn excluded_columns_survive_the_predictor_shortlist() {
        // Regression: with more encoded features than the predictor
        // cap, the shortlist sort pairs each feature with its own
        // score; indexing scores by feature position panics once an
        // exclusion makes the selected set non-contiguous (the
        // state-unemployment corpus frame: 50+ features, a timestamp
        // exclusion, index 50 into a 50-long score vector).
        use polars::prelude::*;
        let mut rng = Lcg(77);
        let n = 1200;
        let mut cols: Vec<Series> = Vec::new();
        let mut x: Vec<f64> = Vec::new();
        let mut y: Vec<f64> = Vec::new();
        for _ in 0..n {
            let v = rng.next_f64();
            x.push(v);
            y.push(v * v + 0.01 * rng.next_f64());
        }
        cols.push(Series::new("x".into(), x));
        cols.push(Series::new("y".into(), y));
        for i in 0..53 {
            let vals: Vec<f64> = (0..n).map(|_| rng.next_f64()).collect();
            cols.push(Series::new(format!("n{i}").into(), vals));
        }
        let df =
            DataFrame::new_infer_height(cols.into_iter().map(|s| s.into()).collect::<Vec<_>>())
                .unwrap();
        let table = extract(&df);
        let (enc, _) = encode(&table, &EncodeConfig::default());
        let excluded = table.names.iter().position(|nm| nm == "n0").unwrap();
        let out = screen(&table, &enc, &ScreenConfig::default(), &[excluded]);
        let res = out.results.iter().find(|r| r.name == "y").unwrap();
        assert_eq!(res.drivers.len(), 1, "drivers: {:?}", res.drivers);
        assert_eq!(res.drivers[0].0, "x");
        assert!(out
            .results
            .iter()
            .all(|r| !r.inputs.iter().any(|i| i == "n0")));
    }

    #[test]
    fn auc_rank_sum_orders_and_ties() {
        // Perfect separation scores 1, reversed scores 0.
        let scores = [0.1, 0.2, 0.8, 0.9];
        let labels = [false, false, true, true];
        assert_eq!(auc_rank_sum(&scores, &labels), Some(1.0));
        let labels = [true, true, false, false];
        assert_eq!(auc_rank_sum(&scores, &labels), Some(0.0));
        // All scores tied: the labels carry no ranking, AUC 0.5.
        let scores = [0.5, 0.5, 0.5, 0.5];
        let labels = [true, false, true, false];
        assert_eq!(auc_rank_sum(&scores, &labels), Some(0.5));
        // One cross-class tie, counted as half a concordant pair:
        // 3.5 of 4 pairs.
        let scores = [0.1, 0.5, 0.5, 0.9];
        let labels = [false, true, false, true];
        assert_eq!(auc_rank_sum(&scores, &labels), Some(0.875));
        // An absent class is not a measurement.
        let labels = [true, true, true, true];
        assert_eq!(auc_rank_sum(&scores, &labels), None);
    }
}
