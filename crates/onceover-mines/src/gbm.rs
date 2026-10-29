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
        TypedCol::I64 { .. } | TypedCol::F64 { .. } => {
            // Target eligibility uses the same bounded row sample as fitting;
            // do not recount millions of full-column values for every model.
            let mut distinct = std::collections::BTreeSet::new();
            for &r in &enc.rows {
                let value = match col {
                    TypedCol::I64 { vals } => vals.at(r as usize).map(|v| v as f64),
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

pub fn screen(table: &TypedTable, enc: &EncodedMatrix, cfg: &ScreenConfig) -> ScreenOutcome {
    let eligible_targets: Vec<usize> = enc.feat_cols.clone();

    let results: Vec<TargetResult> = eligible_targets
        .par_iter()
        .filter_map(|&ci| screen_target(table, enc, ci, cfg))
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
        let feats: Vec<String> = r.inputs.clone();
        let claim = if r.is_categorical {
            format!(
                "{} is predictable from [{}] (holdout accuracy {:.2} vs {:.2} majority baseline)",
                r.name,
                feats.join(", "),
                r.metric,
                r.baseline
            )
        } else {
            format!(
                "{} is predictable from [{}] (holdout R² = {:.2})",
                r.name,
                feats.join(", "),
                r.metric
            )
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

    let coverage = format!(
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
    ScreenOutcome {
        findings,
        results,
        coverage,
    }
}

/// Screen one column as a target.
pub fn screen_target(
    table: &TypedTable,
    enc: &EncodedMatrix,
    ci: usize,
    cfg: &ScreenConfig,
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
    let mut selected: Vec<usize> = (0..enc.bins.len()).filter(|&f| f != own).collect();
    if selected.len() > cfg.max_predictors {
        let scores: Vec<f64> = selected
            .iter()
            .map(|&f| crate::screen::information(&enc.bins[own], &enc.bins[f], &screen_rows))
            .collect();
        selected.sort_by(|&a, &b| {
            // selected initially contains every feature except own.
            let ai = if a < own { a } else { a - 1 };
            let bi = if b < own { b } else { b - 1 };
            scores[bi]
                .total_cmp(&scores[ai])
                .then(enc.feat_names[a].cmp(&enc.feat_names[b]))
        });
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
    let own_feat: Option<usize> = None; // target excluded before packing

    let finish =
        |metric: f64, baseline: f64, rounds: usize, cum_gains: &[f64]| -> Option<TargetResult> {
            let total: f64 = cum_gains.iter().sum();
            let mut ranked: Vec<(usize, f64)> = cum_gains
                .iter()
                .enumerate()
                .filter(|(f, _)| Some(*f) != own_feat)
                .map(|(f, gv)| (f, *gv))
                .collect();
            ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            let top: Vec<(String, f64)> = ranked
                .iter()
                .take(3)
                .filter(|(_, gv)| *gv > 0.0)
                .map(|(f, gv)| {
                    (
                        enc.feat_names[selected[*f]].clone(),
                        if total > 0.0 { gv / total } else { 0.0 },
                    )
                })
                .collect();
            Some(TargetResult {
                col: ci,
                name: table.names[ci].clone(),
                is_categorical: matches!(target, TargetKind::Categorical(_, _)),
                metric,
                baseline,
                n_train: train.len(),
                n_holdout: holdout.len(),
                rounds,
                top_features: top,
                inputs: cum_gains
                    .iter()
                    .enumerate()
                    .filter(|(_, gain)| **gain > 0.0)
                    .map(|(f, _)| enc.feat_names[selected[f]].clone())
                    .collect(),
                cat_ids: cat_payload,
            })
        };

    match &target {
        TargetKind::Numeric(ys) => {
            let (metric, rounds, gains, _trees, _base) = boost_numeric(
                enc.n, cfg, ys, train, holdout, &packed, n_feats, own_feat, min_leaf,
            )?;
            finish(metric, 0.0, rounds, &gains)
        }
        TargetKind::Categorical(ids, labels) => {
            let k = labels.len();
            let mut class_count = vec![0u32; k];
            for &p in train {
                class_count[ids[p as usize] as usize] += 1;
            }
            let prior: Vec<f32> = class_count
                .iter()
                .map(|&x| x as f32 / train.len() as f32)
                .collect();
            let base: Vec<f32> = prior.iter().map(|&p| (p / (1.0 - p + 1e-9)).ln()).collect();
            let mut scores = vec![base.clone(); enc.n];
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
                // Class-major sweeps over row-major `scores` (enc.n × k):
                // the range loop over classes is intentional.
                #[allow(clippy::needless_range_loop)]
                for c in 0..k {
                    let mut g = vec![0.0f32; enc.n];
                    let mut h = vec![0.0f32; enc.n];
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
                        &packed,
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
                        scores[p as usize][c] += cfg.learning_rate
                            * tree_predict(&nodes, root, &packed, n_feats, p as usize);
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
            finish(best_metric, baseline, best_rounds, &best_gains)
        }
    }
}

/// The numeric boosting loop, shared by `screen_target` (which
/// reports its metric and gains) and `fit_gbm_model` (which also
/// keeps the ensemble). Runs the same rounds with the same early
/// stopping; snapshots the trees whenever the holdout R² improves,
/// so the returned ensemble is the one that produced the returned
/// metric. Returns (holdout R², rounds at best, cumulative gains at
/// best, trees at best, base prediction), or None when the best
/// holdout R² is not positive.
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
) -> Option<BoostNumeric> {
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
    if best_metric <= 0.0 {
        return None;
    }
    Some((best_metric, best_rounds, best_gains, best_trees, base))
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
    let target = prepare_target(table, enc, ci, cfg)?;
    let TargetKind::Numeric(ys) = target else {
        return None;
    };
    let present: Vec<u32> = (0..enc.n)
        .filter(|&j| !ys[j].is_nan())
        .map(|j| j as u32)
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
    let mut selected: Vec<usize> = (0..enc.bins.len()).filter(|&f| f != own).collect();
    if selected.len() > cfg.max_predictors {
        let scores: Vec<f64> = selected
            .iter()
            .map(|&f| crate::screen::information(&enc.bins[own], &enc.bins[f], &screen_rows))
            .collect();
        selected.sort_by(|&a, &b| {
            let ai = if a < own { a } else { a - 1 };
            let bi = if b < own { b } else { b - 1 };
            scores[bi]
                .total_cmp(&scores[ai])
                .then(enc.feat_names[a].cmp(&enc.feat_names[b]))
        });
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
    )?;
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
        let out = screen(&table, &enc, &ScreenConfig::default());
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
        let out = screen(&table, &enc, &ScreenConfig::default());
        let res = out.results.iter().find(|r| r.name == "class").unwrap();
        assert!(res.metric > 0.85, "acc = {}", res.metric);
        assert!(res.metric > res.baseline + 0.2);
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
        let out = screen(&table, &enc, &ScreenConfig::default());
        assert!(out
            .results
            .iter()
            .all(|r| r.metric < 0.5 || r.baseline > 0.0 && r.is_categorical));
    }
}
