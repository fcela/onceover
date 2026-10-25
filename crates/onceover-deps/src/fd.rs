//! Functional dependency (exact + approximate) and unique column
//! combination discovery.
//!
//! Discovery steps:
//! 1. Hash-based candidate evaluation over combined per-column hashes
//!    (top-2 RHS tracking per LHS group) — fast, O(m) per (LHS, RHS).
//! 2. Level-wise LHS growth (1 → 2 → 3) with subset-FD pruning.
//! 3. *Exact canonical verification* of every emitted candidate (real
//!    value bytes, not hashes): hashing can only cost recall, never truth.
//!
//! Semantics: FDs are evaluated on tuple-complete rows (rows with a null
//! in the LHS or the RHS are excluded); `n_used` is always reported.

use std::collections::BTreeMap;

use onceover_report::{Confidence, Finding};
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::json;

use onceover_core::{TypedCol, TypedTable};

use crate::hashutil::{build_hashes, encode_value, subsample_rows, ColHashes};
use crate::ColInfo;

/// Configuration for the dependency pass.
#[derive(Clone, Debug)]
pub struct FdConfig {
    /// Max number of candidate LHS columns (levels 1–2 pool).
    pub lhs_cap: usize,
    /// Smaller pool for level-3 LHS triples (deep tier).
    pub lhs3_cap: usize,
    /// Max candidate RHS columns.
    pub rhs_cap: usize,
    /// Row cap for FD evaluation (stride-subsampled beyond it).
    pub row_cap: usize,
    /// Max LHS set size: 2 (standard) or 3 (deep).
    pub max_lhs_size: usize,
    /// Max g1 error for approximate FDs.
    pub afd_g1_max: f64,
    /// Pool size for composite-key (pair) UCC candidates.
    pub key_pair_cap: usize,
}

impl Default for FdConfig {
    fn default() -> Self {
        FdConfig {
            lhs_cap: 16,
            lhs3_cap: 12,
            rhs_cap: 60,
            row_cap: 200_000,
            max_lhs_size: 2,
            afd_g1_max: 0.01,
            key_pair_cap: 16,
        }
    }
}

/// Everything the pipeline needs from dependency discovery.
#[derive(Default, Debug)]
pub struct DepsOutcome {
    pub findings: Vec<Finding>,
    /// Minimal exact FDs as (lhs column indices, rhs column index).
    pub exact_fds: Vec<(Vec<usize>, usize)>,
    /// Minimal UCCs of size >= 2 (column indices).
    pub composite_uccs: Vec<Vec<usize>>,
    /// Single columns that are (null-free and) unique.
    pub single_unique: Vec<usize>,
    pub n_exact: usize,
    pub n_afd: usize,
    pub coverage: String,
}

// ---------- helpers ----------

fn value_is_null(col: &TypedCol, r: usize) -> bool {
    match col {
        TypedCol::I64 { vals } => vals[r].is_none(),
        TypedCol::F64 { vals } => vals[r].is_none(),
        TypedCol::Str { vals } => vals[r].is_none(),
        TypedCol::StrOwned { vals } => vals[r].is_none(),
        TypedCol::Bool { vals } => vals[r].is_none(),
        TypedCol::Date { vals } => vals[r].is_none(),
        TypedCol::Datetime { vals } => vals[r].is_none(),
        TypedCol::Null { .. } => true,
        TypedCol::Other { .. } => true,
    }
}

/// Exact FD verification via canonical encodings. Returns (n_used, violations).
pub fn verify_fd(table: &TypedTable, rows: &[u32], lhs: &[usize], rhs: usize) -> (usize, usize) {
    let mut groups: FxHashMap<Vec<u8>, FxHashMap<Vec<u8>, u32>> = FxHashMap::default();
    for &rr in rows {
        let r = rr as usize;
        if lhs.iter().any(|&c| value_is_null(&table.cols[c], r)) {
            continue;
        }
        if value_is_null(&table.cols[rhs], r) {
            continue;
        }
        let mut key = Vec::with_capacity(16 * lhs.len());
        for &c in lhs {
            encode_value(&table.cols[c], r, &mut key);
        }
        let mut rv = Vec::with_capacity(16);
        encode_value(&table.cols[rhs], r, &mut rv);
        *groups.entry(key).or_default().entry(rv).or_insert(0) += 1;
    }
    let mut n_used = 0usize;
    let mut violations = 0usize;
    for m in groups.values() {
        let total: u32 = m.values().sum();
        n_used += total as usize;
        let mx = m.values().copied().max().unwrap_or(0);
        violations += (total - mx) as usize;
    }
    (n_used, violations)
}

/// Exact distinct-count of complete-row tuples over the given columns;
/// returns usize::MAX when any null is present (not a strict key).
fn count_distinct_tuples(table: &TypedTable, cols: &[usize]) -> usize {
    let n = table.n;
    let mut set: FxHashSet<Vec<u8>> = FxHashSet::default();
    for r in 0..n {
        if cols.iter().any(|&c| value_is_null(&table.cols[c], r)) {
            return usize::MAX;
        }
        let mut key = Vec::with_capacity(16 * cols.len());
        for &c in cols {
            encode_value(&table.cols[c], r, &mut key);
        }
        set.insert(key);
        if set.len() > n {
            break;
        }
    }
    set.len()
}

// ---------- candidate evaluation (hash-based) ----------

#[derive(Default, Clone, Copy)]
struct Acc {
    n: u32,
    first: u64,
    first_c: u32,
    first_set: bool,
    second: u64,
    second_c: u32,
    any_second: bool,
    overflow: bool,
}

struct LhsGroups {
    gid: Vec<u32>, // u32::MAX = excluded
    n_groups: u32,
}

#[allow(clippy::needless_range_loop)]
fn phase_a(lhs: &[usize], ch: &ColHashes) -> LhsGroups {
    let m = ch.m;
    let mut gid = vec![u32::MAX; m];
    let mut groups: FxHashMap<u64, u32> = FxHashMap::default();
    let mut next: u32 = 0;
    for j in 0..m {
        let mut h: u64 = 0xcbf29ce484222325;
        let mut excluded = false;
        for &c in lhs {
            if ch.nulls[c][j] {
                excluded = true;
                break;
            }
            h = (h ^ ch.hashes[c][j]).wrapping_mul(0x100_0000_01B3);
        }
        if excluded {
            continue;
        }
        let g = *groups.entry(h).or_insert_with(|| {
            let g = next;
            next += 1;
            g
        });
        gid[j] = g;
    }
    LhsGroups {
        gid,
        n_groups: next,
    }
}

/// Evaluate one (LHS, RHS): (exact_candidate, est_violations, n_used).
fn eval_rhs(groups: &LhsGroups, rhs: usize, ch: &ColHashes) -> Option<(bool, usize, usize, usize)> {
    let mut accs = vec![Acc::default(); groups.n_groups as usize];
    let mut n_used = 0usize;
    for j in 0..ch.m {
        let g = groups.gid[j];
        if g == u32::MAX || ch.nulls[rhs][j] {
            continue;
        }
        let rh = ch.hashes[rhs][j];
        let a = &mut accs[g as usize];
        a.n += 1;
        n_used += 1;
        if !a.first_set {
            a.first = rh;
            a.first_set = true;
            a.first_c = 1;
        } else if rh == a.first {
            a.first_c += 1;
        } else if a.any_second && rh == a.second {
            a.second_c += 1;
        } else if !a.any_second {
            a.second = rh;
            a.second_c = 1;
            a.any_second = true;
        } else {
            a.overflow = true;
        }
    }
    if n_used < 30 {
        return None;
    }
    let mut exact = true;
    let mut violations = 0usize;
    let mut n_groups = 0usize;
    for a in &accs {
        if a.n == 0 {
            continue;
        }
        n_groups += 1;
        if a.any_second || a.overflow {
            exact = false;
        }
        let max_sub = if a.any_second {
            a.first_c.max(a.second_c)
        } else {
            a.first_c
        };
        violations += (a.n - max_sub.min(a.n)) as usize;
    }
    Some((exact, violations, n_used, n_groups))
}

// ---------- driver ----------

#[derive(Debug, Clone)]
struct Cand {
    lhs: Vec<usize>, // hash slots
    rhs: usize,      // hash slot
}

fn fd_finding(
    table: &TypedTable,
    lhs: &[usize],
    rhs: usize,
    violations: usize,
    n_used: usize,
    sampled: bool,
) -> Finding {
    let lhs_names: Vec<&str> = lhs.iter().map(|&i| table.names[i].as_str()).collect();
    let rhs_name = &table.names[rhs];
    let mut subject = BTreeMap::new();
    subject.insert("lhs".to_string(), json!(lhs_names));
    subject.insert("rhs".to_string(), json!(rhs_name));
    let mut evidence = BTreeMap::new();
    evidence.insert("violations".to_string(), json!(violations));
    evidence.insert("n_used".to_string(), json!(n_used));
    let conf = if sampled {
        Confidence::Sampled
    } else {
        Confidence::Exact
    };
    evidence.insert(
        "method".to_string(),
        json!(if sampled { "sampled_exact" } else { "exact" }),
    );
    if violations == 0 {
        Finding::new(
            "functional_dependency",
            subject,
            format!(
                "{} → {} ({} is a function of {}; 0 violations in {} rows)",
                lhs_names.join(", "),
                rhs_name,
                rhs_name,
                lhs_names.join(", "),
                n_used
            ),
            conf,
            0.85,
            evidence,
            Some(format!(
                "consider dropping {rhs_name} or using it as a lookup"
            )),
            "tane-style lattice + exact verification (onceover v0.1)",
        )
    } else {
        let g1 = violations as f64 / n_used as f64;
        evidence.insert("g1".to_string(), json!(g1));
        Finding::new(
            "approximate_fd",
            subject,
            format!(
                "{} → {} holds on {:.2}% of rows ({} violations in {} rows)",
                lhs_names.join(", "),
                rhs_name,
                100.0 * (1.0 - g1),
                violations,
                n_used
            ),
            conf,
            0.72,
            evidence,
            Some(format!(
                "{} is almost determined by {}; inspect the {} violations",
                rhs_name,
                lhs_names.join(", "),
                violations
            )),
            "tane-style lattice + exact verification (onceover v0.1)",
        )
    }
}

/// Discover FDs, AFDs and UCCs.
pub fn discover(table: &TypedTable, info: &[ColInfo], cfg: &FdConfig) -> DepsOutcome {
    let mut out = DepsOutcome::default();
    let n = table.n;
    if n < 30 || info.is_empty() {
        out.coverage = "skipped: fewer than 30 rows".into();
        return out;
    }

    // ---- eligibility ----
    let eligible: Vec<usize> = (0..info.len())
        .filter(|&i| {
            info[i].distinct >= 2
                && info[i].n_nonnull() >= 30
                && info[i].role != "other"
                && info[i].dtype != "null"
        })
        .collect();

    let n_eligible_lhs = eligible
        .iter()
        .filter(|&&i| info[i].distinct_ratio() < 0.98 && info[i].distinct <= 100_000)
        .count();
    let mut lhs_pool: Vec<usize> = eligible
        .iter()
        .copied()
        .filter(|&i| info[i].distinct_ratio() < 0.98 && info[i].distinct <= 100_000)
        .collect();
    lhs_pool.sort_by(|&a, &b| {
        let ca = (info[a].role == "categorical") as u8;
        let cb = (info[b].role == "categorical") as u8;
        cb.cmp(&ca)
            .then(info[a].distinct.cmp(&info[b].distinct))
            .then(a.cmp(&b))
    });
    let lhs_capped = lhs_pool.len() > cfg.lhs_cap;
    lhs_pool.truncate(cfg.lhs_cap);

    let n_eligible_rhs = eligible.len();
    let mut rhs_pool = eligible.clone();
    rhs_pool.sort_by(|&a, &b| info[a].distinct.cmp(&info[b].distinct).then(a.cmp(&b)));
    let rhs_capped = rhs_pool.len() > cfg.rhs_cap;
    rhs_pool.truncate(cfg.rhs_cap);

    if lhs_pool.is_empty() || rhs_pool.is_empty() {
        out.coverage = "skipped: no eligible columns".into();
        return out;
    }

    // ---- rows ----
    let rows = subsample_rows(n, cfg.row_cap);
    let sampled = rows.len() < n;

    // ---- hashes over pooled columns ----
    let mut slot_of: FxHashMap<usize, usize> = FxHashMap::default();
    let mut hash_cols: Vec<usize> = Vec::new();
    for &c in lhs_pool.iter().chain(rhs_pool.iter()) {
        if let std::collections::hash_map::Entry::Vacant(v) = slot_of.entry(c) {
            v.insert(hash_cols.len());
            hash_cols.push(c);
        }
    }
    let col_refs: Vec<&TypedCol> = hash_cols.iter().map(|&c| &table.cols[c]).collect();
    let ch = build_hashes(&col_refs, &rows);

    let lhs_slots: Vec<usize> = lhs_pool.iter().map(|&c| slot_of[&c]).collect();
    let rhs_slots: Vec<usize> = rhs_pool.iter().map(|&c| slot_of[&c]).collect();

    // ---- level-wise discovery ----
    let mut known_exact: FxHashSet<(usize, u64)> = FxHashSet::default();
    let mut all_cands: Vec<Cand> = Vec::new();

    let level1: Vec<Vec<usize>> = lhs_slots.iter().map(|&a| vec![a]).collect();
    let (new_exact, c1, a1) = run_level(&level1, &rhs_slots, &ch, &known_exact, cfg);
    for e in &new_exact {
        known_exact.insert(*e);
    }
    all_cands.extend(c1);
    all_cands.extend(a1);

    if cfg.max_lhs_size >= 2 {
        let mut pairs = Vec::new();
        for (i, &a) in lhs_slots.iter().enumerate() {
            for &b in lhs_slots.iter().skip(i + 1) {
                pairs.push(vec![a, b]);
            }
        }
        let (new_exact, c2, a2) = run_level(&pairs, &rhs_slots, &ch, &known_exact, cfg);
        for e in &new_exact {
            known_exact.insert(*e);
        }
        all_cands.extend(c2);
        all_cands.extend(a2);
    }

    if cfg.max_lhs_size >= 3 {
        let prolific = level3_pool(&known_exact, &lhs_slots, cfg.lhs3_cap);
        if prolific.len() >= 3 {
            let triples = combinations(&prolific, 3);
            let (new_exact, c3, a3) = run_level(&triples, &rhs_slots, &ch, &known_exact, cfg);
            for e in &new_exact {
                known_exact.insert(*e);
            }
            all_cands.extend(c3);
            all_cands.extend(a3);
        }
    }

    // ---- exact verification of all candidates (parallel) ----
    let verified: Vec<Option<(usize, usize)>> = all_cands
        .par_iter()
        .map(|c| {
            let lhs_orig: Vec<usize> = c.lhs.iter().map(|&s| hash_cols[s]).collect();
            let rhs_orig = hash_cols[c.rhs];
            let (n_used, violations) = verify_fd(table, &rows, &lhs_orig, rhs_orig);
            if n_used < 30 {
                return None;
            }
            Some((n_used, violations))
        })
        .collect();

    // ---- classification ----
    let mut exact_fds: Vec<(Vec<usize>, usize)> = Vec::new();
    let mut best_afd: FxHashMap<usize, (f64, Vec<usize>, usize, usize)> = FxHashMap::default();
    for (cand, v) in all_cands.iter().zip(verified) {
        let Some((n_used, violations)) = v else {
            continue;
        };
        let lhs_orig: Vec<usize> = cand.lhs.iter().map(|&s| hash_cols[s]).collect();
        let rhs_orig = hash_cols[cand.rhs];
        if violations == 0 {
            out.findings
                .push(fd_finding(table, &lhs_orig, rhs_orig, 0, n_used, sampled));
            exact_fds.push((lhs_orig, rhs_orig));
        } else {
            let g1 = violations as f64 / n_used as f64;
            if g1 <= cfg.afd_g1_max {
                let better = match best_afd.get(&rhs_orig) {
                    Some((g, l, _, _)) => {
                        g1 < *g - 1e-12 || (g1 < *g + 1e-12 && lhs_orig.len() < l.len())
                    }
                    None => true,
                };
                if better {
                    best_afd.insert(rhs_orig, (g1, lhs_orig, violations, n_used));
                }
            }
        }
    }

    // An exact refinement (X, Z -> Y) does not explain away exceptions
    // to a simpler dependency (X ~> Y). Suppress only AFDs already implied
    // by an exact dependency on a subset of their own determinant.
    type AfdEntry = (usize, (f64, Vec<usize>, usize, usize));
    let mut afd_sorted: Vec<AfdEntry> = best_afd.into_iter().collect();
    afd_sorted.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1 .0.total_cmp(&b.1 .0))
            .then_with(|| a.1 .1.cmp(&b.1 .1))
    });
    for (rhs_orig, (_g1, lhs_orig, violations, n_used)) in afd_sorted {
        if exact_fds
            .iter()
            .any(|(lhs, rhs)| *rhs == rhs_orig && lhs.iter().all(|c| lhs_orig.contains(c)))
        {
            continue;
        }
        out.findings.push(fd_finding(
            table, &lhs_orig, rhs_orig, violations, n_used, sampled,
        ));
        out.n_afd += 1;
    }

    out.n_exact = exact_fds.len();
    out.exact_fds = exact_fds;

    // ---- UCCs ----
    discover_uccs(table, info, cfg, &mut out);

    // ---- coverage ----
    let mut cov = format!(
        "levels <= {}, lhs {} of {} eligible ({}), rhs {} of {} eligible ({}), rows {} ({})",
        cfg.max_lhs_size,
        lhs_pool.len(),
        n_eligible_lhs,
        if lhs_capped { "capped" } else { "full" },
        rhs_pool.len(),
        n_eligible_rhs,
        if rhs_capped { "capped" } else { "full" },
        rows.len(),
        if sampled { "stride subsample" } else { "full" },
    );
    cov.push_str(&format!("; {} exact FDs, {} AFDs", out.n_exact, out.n_afd));
    out.coverage = cov;

    out
}

fn combinations(items: &[usize], k: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    if k == 0 || items.len() < k {
        return out;
    }
    let mut idx: Vec<usize> = (0..k).collect();
    loop {
        out.push(idx.iter().map(|&i| items[i]).collect());
        let mut i = k;
        loop {
            if i == 0 {
                return out;
            }
            i -= 1;
            if idx[i] < items.len() - (k - i) {
                idx[i] += 1;
                for j in i + 1..k {
                    idx[j] = idx[j - 1] + 1;
                }
                break;
            }
        }
    }
}

fn level3_pool(
    known_exact: &FxHashSet<(usize, u64)>,
    lhs_slots: &[usize],
    cap: usize,
) -> Vec<usize> {
    // LHS slots that produced the most exact FDs — most likely determinants.
    let mut score = vec![0u32; lhs_slots.len()];
    for (_rhs, bits) in known_exact.iter() {
        for (i, &s) in lhs_slots.iter().enumerate() {
            if (bits & (1u64 << s)) != 0 {
                score[i] += 1;
            }
        }
    }
    let mut ranked: Vec<usize> = (0..lhs_slots.len()).collect();
    ranked.sort_by(|&a, &b| score[b].cmp(&score[a]).then(b.cmp(&a)));
    ranked.truncate(cap);
    let mut slots: Vec<usize> = ranked.iter().map(|&i| lhs_slots[i]).collect();
    slots.sort_unstable();
    slots
}

/// Evaluate one level of LHS sets against all RHS candidates.
/// Returns (new exact (rhs, lhs-mask) pairs, exact candidates, afd candidates).
fn run_level(
    sets: &[Vec<usize>],
    rhs_slots: &[usize],
    ch: &ColHashes,
    known_exact: &FxHashSet<(usize, u64)>,
    cfg: &FdConfig,
) -> (Vec<(usize, u64)>, Vec<Cand>, Vec<Cand>) {
    type LevelRow = Vec<Option<(usize, bool, usize, usize, usize)>>;
    let results: Vec<LevelRow> = sets
        .par_iter()
        .map(|lhs| {
            let groups = phase_a(lhs, ch);
            let mut row_out = Vec::with_capacity(rhs_slots.len());
            for &rhs in rhs_slots {
                if lhs.contains(&rhs) || subset_determines(lhs, rhs, known_exact) {
                    row_out.push(None);
                    continue;
                }
                match eval_rhs(&groups, rhs, ch) {
                    Some((exact, violations, n_used, n_groups)) => {
                        row_out.push(Some((rhs, exact, violations, n_used, n_groups)))
                    }
                    None => row_out.push(None),
                }
            }
            row_out
        })
        .collect();

    let mut new_exact: Vec<(usize, u64)> = Vec::new();
    let mut cands = Vec::new();
    let mut afd_cands = Vec::new();
    for (lhs, row_out) in sets.iter().zip(results) {
        for item in row_out.into_iter().flatten() {
            let (rhs, exact, violations, n_used, n_groups) = item;
            // A near-unique LHS (approximate composite key) trivially
            // determines every RHS — that is the key story, not K spurious
            // "X, Y -> everything" findings.
            if n_groups as f64 >= 0.95 * n_used as f64 {
                continue;
            }
            let bits = mask_of(lhs);
            if exact {
                new_exact.push((rhs, bits));
                cands.push(Cand {
                    lhs: lhs.clone(),
                    rhs,
                });
            } else {
                let g1 = violations as f64 / n_used.max(1) as f64;
                if g1 <= cfg.afd_g1_max {
                    afd_cands.push(Cand {
                        lhs: lhs.clone(),
                        rhs,
                    });
                }
            }
        }
    }
    (new_exact, cands, afd_cands)
}

fn mask_of(lhs: &[usize]) -> u64 {
    let mut m = 0u64;
    for &s in lhs {
        m |= 1u64 << s;
    }
    m
}

fn subset_determines(lhs: &[usize], rhs: usize, known_exact: &FxHashSet<(usize, u64)>) -> bool {
    let n = lhs.len();
    for mask in 1..(1u64 << n) {
        let mut sub = 0u64;
        for (i, &s) in lhs.iter().enumerate() {
            if (mask & (1 << i)) != 0 {
                sub |= 1u64 << s;
            }
        }
        if known_exact.contains(&(rhs, sub)) {
            return true;
        }
    }
    false
}

fn discover_uccs(table: &TypedTable, info: &[ColInfo], cfg: &FdConfig, out: &mut DepsOutcome) {
    let n = table.n;
    // Single-column keys: null-free and unique.
    let mut single: Vec<usize> = (0..info.len())
        .filter(|&i| info[i].null_count == 0 && info[i].distinct == n && n > 0)
        .collect();
    single.sort_by(|&a, &b| {
        crate::fd::key_score(&info[b])
            .total_cmp(&crate::fd::key_score(&info[a]))
            .then(a.cmp(&b))
    });
    out.single_unique = single;

    if cfg.max_lhs_size < 2 || n < 30 {
        return;
    }

    let plausible: Vec<usize> = (0..info.len())
        .filter(|&i| {
            info[i].null_count == 0
                && info[i].distinct >= 2
                && info[i].distinct < n
                && info[i].role != "other"
        })
        .collect();
    if plausible.len() < 2 {
        return;
    }
    let mut ranked = plausible;
    ranked.sort_by(|&a, &b| {
        info[b]
            .distinct_ratio()
            .total_cmp(&info[a].distinct_ratio())
            .then(a.cmp(&b))
    });
    ranked.truncate(cfg.key_pair_cap);

    let mut composite: Vec<Vec<usize>> = Vec::new();
    for (i, &a) in ranked.iter().enumerate() {
        for &b in ranked.iter().skip(i + 1) {
            if out.single_unique.contains(&a) || out.single_unique.contains(&b) {
                continue;
            }
            // Cheap hash distinctness prefilter (exact check only if it passes).
            if tuple_hash_unique(table, &[a, b]) && count_distinct_tuples(table, &[a, b]) == n {
                composite.push(vec![a, b]);
            }
        }
    }
    if cfg.max_lhs_size >= 3 {
        let base: Vec<usize> = ranked
            .iter()
            .copied()
            .filter(|&c| {
                !composite.iter().any(|p: &Vec<usize>| p.contains(&c))
                    && !out.single_unique.contains(&c)
            })
            .take(12)
            .collect();
        for comb in combinations(&base, 3) {
            if tuple_hash_unique(table, &comb) && count_distinct_tuples(table, &comb) == n {
                composite.push(comb);
            }
        }
    }
    out.composite_uccs = composite;
}

fn fnv64_local(bytes: &[u8]) -> u64 {
    let mut h: u64 = 14695981039346656037;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

fn value_hash64(col: &TypedCol, r: usize) -> u64 {
    match col {
        TypedCol::I64 { vals } => vals[r].map(|v| v as u64).unwrap_or(0),
        TypedCol::F64 { vals } => vals[r].map(|v| v.to_bits()).unwrap_or(0),
        TypedCol::Str { vals } => match vals[r] {
            Some(s) => fnv64_local(s.as_bytes()),
            None => 0,
        },
        TypedCol::StrOwned { vals } => match &vals[r] {
            Some(s) => fnv64_local(s.as_bytes()),
            None => 0,
        },
        TypedCol::Bool { vals } => match vals[r] {
            Some(true) => 1,
            Some(false) => 2,
            None => 0,
        },
        TypedCol::Date { vals } => vals[r].map(|v| v as u64).unwrap_or(0),
        TypedCol::Datetime { vals } => vals[r].map(|v| v as u64).unwrap_or(0),
        TypedCol::Null { .. } | TypedCol::Other { .. } => 0,
    }
}

/// Fast distinctness check over combined per-column hashes. False means
/// "definitely not unique (or a 2^-64 collision)"; true requires exact
/// confirmation by the caller.
fn tuple_hash_unique(table: &TypedTable, cols: &[usize]) -> bool {
    let mut set: FxHashSet<u64> = FxHashSet::default();
    for r in 0..table.n {
        let mut h: u64 = 0xcbf29ce484222325;
        for &c in cols {
            h = (h ^ value_hash64(&table.cols[c], r)).wrapping_mul(0x100_0000_01B3);
        }
        if !set.insert(h) {
            return false;
        }
    }
    true
}

/// Key-plausibility score (higher is better).
pub fn key_score(info: &ColInfo) -> f64 {
    let affinity = match info.dtype.as_str() {
        "int8" | "int16" | "int32" | "int64" => 1.0,
        "uint8" | "uint16" | "uint32" | "uint64" => 1.0,
        "string" => 0.8,
        "datetime" | "date" => 0.4,
        "float32" | "float64" => 0.2,
        _ => 0.0,
    };
    let name_hit = [
        "id", "key", "keys", "code", "no", "num", "number", "uuid", "guid", "sku", "ref",
    ];
    let lower = info.name.to_lowercase();
    let tokens: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).collect();
    let name = if tokens.iter().any(|t| name_hit.contains(t)) {
        1.0
    } else {
        0.0
    };
    let completeness = 1.0 - info.null_count as f64 / info.n.max(1) as f64;
    0.4 * completeness + 0.4 * affinity + 0.2 * name
}

#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::extract;
    use polars::df;

    fn info_for(table: &TypedTable) -> Vec<ColInfo> {
        table
            .cols
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let counts = onceover_core::build_counts(col, 1_000_000);
                ColInfo {
                    name: table.names[i].clone(),
                    dtype: table.dtypes[i].clone(),
                    role: String::new(),
                    n: table.n,
                    null_count: col.null_count(),
                    distinct: counts.distinct,
                    dup_examples: vec![],
                }
            })
            .collect()
    }

    #[test]
    fn finds_exact_and_approximate_fds() {
        // region -> region_name exact; region -> manager with 2 violations.
        let mut region: Vec<Option<i64>> = Vec::new();
        let mut rname: Vec<Option<&str>> = Vec::new();
        let mut mgr: Vec<Option<&str>> = Vec::new();
        let teams = ["alpha", "beta", "gamma"];
        for i in 0..300 {
            let t = (i % 3) as usize;
            region.push(Some(t as i64));
            rname.push(Some(teams[t]));
            if i == 100 || i == 200 {
                mgr.push(Some("odd_manager")); // 2 violations
            } else {
                mgr.push(Some(teams[t]));
            }
        }
        let df = df! {
            "region_id" => region,
            "region_name" => rname,
            "manager" => mgr,
        }
        .unwrap();
        let table = extract(&df);
        let info = info_for(&table);
        let out = discover(&table, &info, &FdConfig::default());
        let exact: Vec<_> = out
            .findings
            .iter()
            .filter(|f| f.kind == "functional_dependency")
            .collect();
        assert!(
            exact
                .iter()
                .any(|f| f.claim.contains("region_id → region_name")),
            "claims: {:?}",
            out.findings
                .iter()
                .map(|f| f.claim.clone())
                .collect::<Vec<_>>()
        );
        let afd: Vec<_> = out
            .findings
            .iter()
            .filter(|f| f.kind == "approximate_fd")
            .collect();
        assert!(afd.iter().any(|f| f.claim.contains("region_id → manager")));
        assert!(afd
            .iter()
            .any(|f| f.evidence["violations"].as_u64() == Some(2)));
    }

    #[test]
    fn exact_refinement_does_not_hide_simpler_approximate_dependency() {
        let n = 600;
        let region: Vec<_> = (0..n).map(|i| (i % 3) as i64).collect();
        let exceptions: Vec<_> = (0..n).map(|i| i == 100 || i == 200).collect();
        let manager: Vec<_> = region
            .iter()
            .zip(&exceptions)
            .map(|(&r, &exception)| if exception { 3 } else { r })
            .collect();
        let df = df! {
            "region_id" => region,
            "exception" => exceptions,
            "manager" => manager,
        }
        .unwrap();
        let table = extract(&df);
        let out = discover(&table, &info_for(&table), &FdConfig::default());
        assert!(
            out.exact_fds.contains(&(vec![1, 0], 2)) || out.exact_fds.contains(&(vec![0, 1], 2))
        );
        let afd = out
            .findings
            .iter()
            .find(|f| f.kind == "approximate_fd" && f.subject["rhs"] == "manager")
            .expect("exact refinement must not suppress region_id ~> manager");
        assert_eq!(afd.subject["lhs"], json!(["region_id"]));
        assert_eq!(afd.evidence["violations"], json!(2));
        assert_eq!(afd.evidence["n_used"], json!(n));
    }

    #[test]
    fn finds_composite_ucc() {
        let a: Vec<Option<i64>> = (0..100).map(|i| Some(i % 10)).collect();
        let b: Vec<Option<i64>> = (0..100).map(|i| Some(i / 10)).collect();
        let df = df! {
            "a" => a,
            "b" => b,
        }
        .unwrap();
        let table = extract(&df);
        let info = info_for(&table);
        let out = discover(&table, &info, &FdConfig::default());
        assert!(out.composite_uccs.iter().any(|u| u.len() == 2));
        assert!(out.single_unique.is_empty());
    }
}
