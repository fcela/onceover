//! Duplicate-encoding advisory: two label-like columns that record
//! the same grouping (a code and its label, an id and its display
//! name), or one grouping that rolls up to a coarser one.
//!
//! The functional-dependency pass reports the mapping underneath
//! (`code -> label`); this pass states the actionable reading the
//! FD does not: the second column repeats a grouping the first
//! already carries. The check follows the shape of the hygiene
//! tier in mboostrs-dev's autospec frame findings, with two
//! strengthenings: candidates are verified on exact canonical
//! encodings before anything is emitted (hashing only discovers,
//! never decides — the deps crate's P7 rule), and many-to-one
//! rollups are reported as rollups, not as duplicate encodings.
//!
//! Eligibility mirrors the originating check: only label-like
//! columns can be two encodings of one grouping — text categories
//! and whole-number codes. A numeric measurement that takes one
//! value per group (a region's income) maps one-to-one onto the
//! group code too, but it carries an order and a size a new group
//! could be predicted from, so it is not an encoding. Identifier
//! columns are excluded for the symmetric reason: a key determines
//! its label trivially, and the key findings already say so.

use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use rustc_hash::FxHashMap;
use serde_json::json;
use std::collections::BTreeMap;

use crate::hashutil::{build_hashes, encode_value, subsample_rows};
use crate::ColInfo;

#[derive(Clone, Debug)]
pub struct DupeConfig {
    /// Rows examined per pair (deterministic sample above this). 0
    /// disables the pass. Above the cap, one-to-one and rollup claims
    /// are sample-scoped candidates, not full-table facts.
    pub row_cap: usize,
    /// Rows where both columns are present, required per pair.
    pub min_rows: usize,
    /// A grouping needs at least this many levels (a two-level
    /// column is a flag, not an encoding of a grouping).
    pub min_levels: usize,
    /// Near-functionality bar: the joint may hold at most this
    /// multiple of the larger margin's distinct pairs.
    pub pair_slack: f64,
    /// At or above this level-count ratio the pair reads as two
    /// encodings of one grouping; below it, as a rollup.
    pub level_share: f64,
    pub max_findings: usize,
    /// Cooperative deadline (issue #117, stage 2): polled before
    /// each first column of a pair. Pairs run in column order, so
    /// the examined pairs are a prefix of that order and the
    /// coverage detail states how many of the total were examined.
    pub deadline: Option<std::time::Instant>,
}

impl Default for DupeConfig {
    fn default() -> Self {
        Self {
            row_cap: 100_000,
            min_rows: 20,
            min_levels: 3,
            pair_slack: 1.02,
            level_share: 0.9,
            max_findings: 50,
            deadline: None,
        }
    }
}

impl DupeConfig {
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Form {
    Duplicate,
    Rollup,
}

/// Joint statistics for one column pair over the rows where both
/// are present: distinct levels per side, distinct value pairs,
/// and the rows that break each direction's majority mapping.
#[derive(Clone, Copy, Default, Debug)]
struct PairStats {
    n_used: usize,
    levels_a: usize,
    levels_b: usize,
    top_a: usize,
    top_b: usize,
    pairs: usize,
    violations_ab: usize,
    violations_ba: usize,
    exceptions: usize,
}

fn classify(stats: &PairStats, cfg: &DupeConfig) -> Option<Form> {
    let larger = stats.levels_a.max(stats.levels_b) as f64;
    let smaller = stats.levels_a.min(stats.levels_b) as f64;
    if larger == 0.0 || (stats.pairs as f64) > cfg.pair_slack * larger {
        return None;
    }
    if smaller >= cfg.level_share * larger {
        Some(Form::Duplicate)
    } else {
        Some(Form::Rollup)
    }
}

/// Modal-mapping statistics from a joint count table keyed by
/// opaque level ids of either side.
fn stats_from_joint<K: Copy + Eq + std::hash::Hash>(joint: &FxHashMap<(K, K), u64>) -> PairStats {
    let mut totals_a: FxHashMap<K, u64> = FxHashMap::default();
    let mut totals_b: FxHashMap<K, u64> = FxHashMap::default();
    let mut best_a: FxHashMap<K, (K, u64)> = FxHashMap::default();
    let mut best_b: FxHashMap<K, (K, u64)> = FxHashMap::default();
    let mut n_used = 0usize;
    for (&(a, b), &c) in joint {
        n_used += c as usize;
        *totals_a.entry(a).or_default() += c;
        *totals_b.entry(b).or_default() += c;
        let ea = best_a.entry(a).or_insert((b, 0));
        if c > ea.1 {
            *ea = (b, c);
        }
        let eb = best_b.entry(b).or_insert((a, 0));
        if c > eb.1 {
            *eb = (a, c);
        }
    }
    let mut stats = PairStats {
        n_used,
        levels_a: totals_a.len(),
        levels_b: totals_b.len(),
        top_a: totals_a.values().copied().max().unwrap_or(0) as usize,
        top_b: totals_b.values().copied().max().unwrap_or(0) as usize,
        pairs: joint.len(),
        ..PairStats::default()
    };
    for (&a, &t) in &totals_a {
        stats.violations_ab += (t - best_a[&a].1) as usize;
    }
    for (&b, &t) in &totals_b {
        stats.violations_ba += (t - best_b[&b].1) as usize;
    }
    for (&(a, b), &c) in joint {
        if best_a[&a].0 != b || best_b[&b].0 != a {
            stats.exceptions += c as usize;
        }
    }
    stats
}

/// Label-like test on the column's own representation. The F64
/// case is decided by the caller (it needs the row window).
fn label_like(col: &TypedCol) -> bool {
    matches!(
        col,
        TypedCol::Str { .. }
            | TypedCol::StrOwned { .. }
            | TypedCol::I64 { .. }
            | TypedCol::U64 { .. }
            | TypedCol::F64 { .. }
    )
}

/// True when every non-null value in the window is a whole number
/// of moderate size — a code stored as a float, not a measurement.
fn f64_all_whole(col: &TypedCol, rows: &[u32]) -> bool {
    let TypedCol::F64 { vals } = col else {
        return true;
    };
    rows.iter().all(|&r| match vals.at(r as usize) {
        Some(v) => v.fract() == 0.0 && v.abs() < 9_007_199_254_740_992.0,
        None => true,
    })
}

pub fn profile_duplicate_encodings(
    table: &TypedTable,
    info: &[ColInfo],
    cfg: &DupeConfig,
) -> (Vec<Finding>, String) {
    profile_duplicate_encodings_stopped(table, info, cfg).0
}

/// `profile_duplicate_encodings`, also returning whether the
/// deadline stopped the pair scan before every pair was examined.
pub fn profile_duplicate_encodings_stopped(
    table: &TypedTable,
    info: &[ColInfo],
    cfg: &DupeConfig,
) -> ((Vec<Finding>, String), bool) {
    if !cfg.is_enabled() {
        return ((Vec::new(), "disabled at this budget".into()), false);
    }
    let rows = subsample_rows(table, cfg.row_cap);
    let sampled = rows.len() < table.n;

    // Eligible columns: label-like, not an identifier, with a
    // level count in [min_levels, n/2].
    let mut eligible: Vec<usize> = Vec::new();
    for (i, col) in table.cols.iter().enumerate() {
        let ci = &info[i];
        if ci.card_role == "unique" || !label_like(col) {
            continue;
        }
        if ci.distinct < cfg.min_levels || ci.distinct * 2 > table.n {
            continue;
        }
        if matches!(col, TypedCol::F64 { .. }) && !f64_all_whole(col, &rows) {
            continue;
        }
        eligible.push(i);
    }
    if eligible.len() < 2 {
        return (
            (
                Vec::new(),
                format!(
                    "{} label-like columns eligible; no pair to evaluate",
                    eligible.len()
                ),
            ),
            false,
        );
    }

    let cols: Vec<&TypedCol> = eligible.iter().map(|&i| &table.cols[i]).collect();
    let hashes = build_hashes(&cols, &rows);

    let mut findings: Vec<Finding> = Vec::new();
    let mut n_pairs = 0usize;
    let mut n_duplicates = 0usize;
    let mut n_rollups = 0usize;
    let mut n_chance = 0usize;
    let total_pairs = eligible.len() * (eligible.len() - 1) / 2;
    let mut pairs_examined = 0usize;
    let mut stopped = false;
    'pairs: for x in 0..eligible.len() {
        if cfg.deadline.is_some_and(|t| std::time::Instant::now() >= t) {
            stopped = true;
            break;
        }
        for y in (x + 1)..eligible.len() {
            pairs_examined += 1;
            // Hash-stage discovery over rows where both are present.
            let mut joint: FxHashMap<(u64, u64), u64> = FxHashMap::default();
            for r in 0..hashes.m {
                if hashes.nulls[x][r] || hashes.nulls[y][r] {
                    continue;
                }
                *joint
                    .entry((hashes.hashes[x][r], hashes.hashes[y][r]))
                    .or_default() += 1;
            }
            let hstats = stats_from_joint(&joint);
            if hstats.n_used < cfg.min_rows {
                continue;
            }
            n_pairs += 1;
            if classify(&hstats, cfg).is_none() {
                continue;
            }
            // Exact verification over the same window before
            // anything is emitted (P7): rebuild the joint on
            // canonical encodings (recoded to ids) and recompute
            // every number the finding will report. Null rows are
            // excluded through the hash stage's masks (a null
            // encodes as a tag byte, not an empty buffer).
            let (ca, cb) = (&table.cols[eligible[x]], &table.cols[eligible[y]]);
            let mut ids_a: FxHashMap<Vec<u8>, u32> = FxHashMap::default();
            let mut ids_b: FxHashMap<Vec<u8>, u32> = FxHashMap::default();
            let mut ejoint: FxHashMap<(u32, u32), u64> = FxHashMap::default();
            for (pos, &r) in rows.iter().enumerate() {
                if hashes.nulls[x][pos] || hashes.nulls[y][pos] {
                    continue;
                }
                let (mut ea, mut eb) = (Vec::new(), Vec::new());
                encode_value(ca, r as usize, &mut ea);
                encode_value(cb, r as usize, &mut eb);
                let next_a = ids_a.len() as u32;
                let a = *ids_a.entry(ea).or_insert(next_a);
                let next_b = ids_b.len() as u32;
                let b = *ids_b.entry(eb).or_insert(next_b);
                *ejoint.entry((a, b)).or_default() += 1;
            }
            let stats = stats_from_joint(&ejoint);
            let Some(form) = classify(&stats, cfg) else {
                continue;
            };
            // The mapping must carry information beyond what the
            // marginals give by chance: a near-unique column maps
            // onto anything, and anything maps onto a near-constant
            // column (see `fd::MIN_RFI`). A duplicate encoding needs
            // it in both directions, a rollup from finer to coarser.
            let (fine, coarse, coarse_top) = if stats.levels_a >= stats.levels_b {
                (eligible[x], eligible[y], stats.top_b)
            } else {
                (eligible[y], eligible[x], stats.top_a)
            };
            // Reject rollups into near-constant columns (e.g. 99.9% one level
            // or fewer than 5 non-modal rows): any high-cardinality column
            // trivially appears to "roll up" onto a near-constant target.
            if matches!(form, Form::Rollup)
                && (stats.n_used.saturating_sub(coarse_top) < 5
                    || (coarse_top as f64 / stats.n_used.max(1) as f64) >= 0.99)
            {
                n_chance += 1;
                continue;
            }
            let rfi_fc = crate::fd::fd_stats(table, &rows, &[fine], coarse).rfi;
            let reliable = match form {
                Form::Rollup => rfi_fc >= crate::fd::MIN_RFI,
                Form::Duplicate => {
                    rfi_fc >= crate::fd::MIN_RFI
                        && crate::fd::fd_stats(table, &rows, &[coarse], fine).rfi
                            >= crate::fd::MIN_RFI
                }
            };
            if !reliable {
                n_chance += 1;
                continue;
            }
            if findings.len() >= cfg.max_findings {
                break 'pairs;
            }
            match form {
                Form::Duplicate => n_duplicates += 1,
                Form::Rollup => n_rollups += 1,
            }
            findings.push(make_finding(
                table,
                eligible[x],
                eligible[y],
                form,
                &stats,
                sampled,
            ));
        }
    }

    let mut coverage = format!(
        "{} label-like columns, {} pairs evaluated on {} rows{}; {} duplicate encodings, {} rollups",
        eligible.len(),
        n_pairs,
        rows.len(),
        if sampled {
            format!(
                " (sampled from {} rows; claims are sample-scoped, not full-table facts)",
                table.n
            )
        } else {
            String::new()
        },
        n_duplicates,
        n_rollups
    );
    if n_chance > 0 {
        coverage.push_str(&format!(
            "; {n_chance} more mapping(s) not stated: reliable fraction of information below {}, the agreement chance predicts",
            crate::fd::MIN_RFI
        ));
    }
    if stopped {
        coverage.push_str(&format!(
            "; deadline reached mid-pass: {pairs_examined} of {total_pairs} column pairs examined"
        ));
    }
    ((findings, coverage), stopped)
}

fn make_finding(
    table: &TypedTable,
    i: usize,
    j: usize,
    form: Form,
    stats: &PairStats,
    sampled: bool,
) -> Finding {
    let (name_i, name_j) = (&table.names[i], &table.names[j]);
    match form {
        Form::Duplicate => {
            let exact = stats.violations_ab == 0 && stats.violations_ba == 0;
            let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            params.insert("columns".into(), json!([name_i.clone(), name_j.clone()]));
            params.insert("column".into(), json!(name_i.clone()));
            let mut metrics: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            metrics.insert("form".into(), json!("duplicate_encoding"));
            metrics.insert("exact".into(), json!(exact));
            metrics.insert("levels_first".into(), json!(stats.levels_a));
            metrics.insert("levels_second".into(), json!(stats.levels_b));
            metrics.insert("pairs".into(), json!(stats.pairs));
            metrics.insert(
                "violations_first_to_second".into(),
                json!(stats.violations_ab),
            );
            metrics.insert(
                "violations_second_to_first".into(),
                json!(stats.violations_ba),
            );
            metrics.insert("exceptions".into(), json!(stats.exceptions));
            metrics.insert("n_used".into(), json!(stats.n_used));
            // The scope travels with the finding, not just the pass's
            // coverage entry: `sampled` drives evidence.scope.sampling
            // and `population_rows` names the table the sample came from.
            metrics.insert("sampled".into(), json!(sampled));
            metrics.insert("population_rows".into(), json!(table.n));
            // "One-to-one on all rows" is reserved for a full-data
            // check. On a sample the one-to-one reading is a candidate
            // with both row counts stated in the claim.
            let claim = match (exact, sampled) {
                (true, false) => format!(
                    "{} and {} encode the same grouping: {} levels, paired one-to-one on all {} rows",
                    name_i, name_j, stats.levels_a, stats.n_used
                ),
                (true, true) => format!(
                    "On {} sampled rows out of {} rows: {} and {} encode the same grouping: {} levels, paired one-to-one in the sample; not verified on the full table",
                    stats.n_used, table.n, name_i, name_j, stats.levels_a
                ),
                (false, false) => format!(
                    "{} and {} encode the same grouping: {} and {} levels with {} distinct value pairs; {} of {} rows break the majority pairing",
                    name_i,
                    name_j,
                    stats.levels_a,
                    stats.levels_b,
                    stats.pairs,
                    stats.exceptions,
                    stats.n_used
                ),
                (false, true) => format!(
                    "On {} sampled rows out of {} rows: {} and {} encode the same grouping: {} and {} levels with {} distinct value pairs; {} of the sampled rows break the majority pairing",
                    stats.n_used,
                    table.n,
                    name_i,
                    name_j,
                    stats.levels_a,
                    stats.levels_b,
                    stats.pairs,
                    stats.exceptions
                ),
            };
            Finding::new(
                Pattern::DuplicateEncoding,
                params,
                claim,
                EvidenceBasis::Measured,
                if exact { 0.62 } else { 0.56 },
                metrics,
                Some(if sampled {
                    format!("if the sample is representative, keep one of {name_i} and {name_j} in models and joins; the other repeats the same grouping in the sample")
                } else {
                    format!("keep one of {name_i} and {name_j} in models and joins; the other repeats the same grouping and counts it twice")
                }),
                Method::new(
                    "duplicate_encodings",
                    "pair contingency + majority mapping, exact verification",
                    "onceover duplicate-encoding advisory v1 (issue #41)",
                ),
            )
        }
        Form::Rollup => {
            // Subject order is finer first, coarser second.
            let (finer, coarser, lf, lc, v_fc) = if stats.levels_a >= stats.levels_b {
                (
                    name_i,
                    name_j,
                    stats.levels_a,
                    stats.levels_b,
                    stats.violations_ab,
                )
            } else {
                (
                    name_j,
                    name_i,
                    stats.levels_b,
                    stats.levels_a,
                    stats.violations_ba,
                )
            };
            let v_cf = if stats.levels_a >= stats.levels_b {
                stats.violations_ba
            } else {
                stats.violations_ab
            };
            // For a rollup the finding states one mapping (finer to
            // coarser), so `exceptions` counts the rows breaking
            // that mapping — the either-direction count would be
            // dominated by the coarse side's many preimages and
            // would contradict the claim.
            let exceptions = v_fc;
            let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            params.insert("columns".into(), json!([finer.clone(), coarser.clone()]));
            params.insert("column".into(), json!(finer.clone()));
            let mut metrics: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            metrics.insert("form".into(), json!("rollup"));
            metrics.insert("finer".into(), json!(finer.clone()));
            metrics.insert("coarser".into(), json!(coarser.clone()));
            metrics.insert("levels_first".into(), json!(lf));
            metrics.insert("levels_second".into(), json!(lc));
            metrics.insert("pairs".into(), json!(stats.pairs));
            metrics.insert("violations_first_to_second".into(), json!(v_fc));
            metrics.insert("violations_second_to_first".into(), json!(v_cf));
            metrics.insert("exceptions".into(), json!(exceptions));
            metrics.insert("n_used".into(), json!(stats.n_used));
            metrics.insert("sampled".into(), json!(sampled));
            metrics.insert("population_rows".into(), json!(table.n));
            let claim = match (v_fc == 0, sampled) {
                (true, false) => format!(
                    "{} rolls up to {}: each of its {} levels maps to one of {} levels, on all {} rows",
                    finer, coarser, lf, lc, stats.n_used
                ),
                (true, true) => format!(
                    "On {} sampled rows out of {} rows: {} rolls up to {}: each of its {} levels maps to one of {} levels in the sample; not verified on the full table",
                    stats.n_used, table.n, finer, coarser, lf, lc
                ),
                (false, false) => format!(
                    "{} rolls up to {}: {} levels map onto {} levels ({} distinct value pairs); {} of {} rows break the mapping",
                    finer, coarser, lf, lc, stats.pairs, v_fc, stats.n_used
                ),
                (false, true) => format!(
                    "On {} sampled rows out of {} rows: {} rolls up to {}: {} levels map onto {} levels ({} distinct value pairs); {} of those sampled rows break the mapping",
                    stats.n_used, table.n, finer, coarser, lf, lc, stats.pairs, v_fc
                ),
            };
            Finding::new(
                Pattern::DuplicateEncoding,
                params,
                claim,
                EvidenceBasis::Measured,
                0.5,
                metrics,
                Some(if sampled {
                    format!("in the sample, {coarser} is a coarser grouping of {finer}; if confirmed on the full table, keep the level of detail the analysis needs — carrying both counts the grouping twice")
                } else {
                    format!("{coarser} is a coarser grouping of {finer}; keep the level of detail the analysis needs — carrying both counts the grouping twice")
                }),
                Method::new(
                    "duplicate_encodings",
                    "pair contingency + majority mapping, exact verification",
                    "onceover duplicate-encoding advisory v1 (issue #41)",
                ),
            )
        }
    }
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
                    card_role: String::new(),
                    n: table.n,
                    null_count: col.null_count(),
                    distinct: counts.distinct,
                    dup_examples: vec![],
                }
            })
            .collect()
    }

    #[test]
    fn past_deadline_examines_no_pair_and_says_so() {
        let (codes, labels) = codes_labels(5, 200);
        let df = df!["code" => codes, "label" => labels].unwrap();
        let table = extract(&df);
        let info = info_for(&table);
        let cfg = DupeConfig {
            deadline: Some(std::time::Instant::now() - std::time::Duration::from_secs(1)),
            ..DupeConfig::default()
        };
        let ((findings, cov), stopped) = profile_duplicate_encodings_stopped(&table, &info, &cfg);
        assert!(stopped);
        assert!(findings.is_empty());
        assert!(
            cov.contains("deadline reached mid-pass: 0 of 1 column pairs examined"),
            "{cov}"
        );
        let ((plain, plain_cov), plain_stopped) =
            profile_duplicate_encodings_stopped(&table, &info, &DupeConfig::default());
        assert!(!plain_stopped);
        assert_eq!(plain.len(), 1, "{plain_cov}");
    }

    fn codes_labels(n_groups: usize, n: usize) -> (Vec<i64>, Vec<String>) {
        let codes: Vec<i64> = (0..n).map(|r| (r % n_groups) as i64 + 1).collect();
        let labels: Vec<String> = codes.iter().map(|c| format!("group_{c}")).collect();
        (codes, labels)
    }

    #[test]
    fn rare_unsampled_violation_is_sample_scoped_not_a_table_fact() {
        // One row outside the deterministic verification sample breaks
        // an otherwise exact 50-level encoding. The sampled claim must
        // not read as a statement about the whole table. The 600-row
        // sample repeats each level enough to clear the main-line
        // reliability bar; a near-unique 50-level code on 60 rows would
        // be suppressed as chance.
        let n = 6000usize;
        let cap = 600usize;
        let (codes, base_labels) = codes_labels(50, n);
        // Pick a violating row that stays outside the content-keyed
        // sample of the final frame, not merely outside a sample of the
        // pristine frame.
        let mut chosen = None;
        for candidate in 0..n {
            let mut labels = base_labels.clone();
            let other = (codes[candidate] % 50) + 1;
            labels[candidate] = format!("group_{other}");
            let frame = df!["code" => &codes, "label" => &labels].unwrap();
            let table = extract(&frame);
            let sampled = subsample_rows(&table, cap);
            if !sampled.contains(&(candidate as u32)) {
                chosen = Some((candidate, sampled));
                break;
            }
        }
        let (violation_row, sampled) =
            chosen.expect("a 600-row content sample of 6000 rows leaves an unsampled row");
        assert!(!sampled.contains(&(violation_row as u32)));
        let mut labels = base_labels;
        let other = (codes[violation_row] % 50) + 1;
        labels[violation_row] = format!("group_{other}");
        let frame = df!["code" => &codes, "label" => &labels].unwrap();
        let table = extract(&frame);
        let info = info_for(&table);

        let cfg = DupeConfig {
            row_cap: cap,
            ..DupeConfig::default()
        };
        let (findings, cov) = profile_duplicate_encodings(&table, &info, &cfg);
        assert_eq!(findings.len(), 1, "{findings:?}");
        let f = &findings[0];
        assert!(f.claim.contains("sampled"), "{}", f.claim);
        assert!(f.claim.contains(&format!("of {n} rows")), "{}", f.claim);
        assert!(f.claim.contains("in the sample"), "{}", f.claim);
        assert!(!f.claim.contains("on all"), "{}", f.claim);
        let interpretation = f.interpretation.as_deref().unwrap_or("");
        assert!(interpretation.contains("in the sample"), "{interpretation}");
        let m = &f.evidence[0].metrics;
        assert_eq!(m.get("sampled").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            m.get("population_rows").and_then(|v| v.as_u64()),
            Some(n as u64)
        );
        assert_eq!(
            f.evidence[0].scope.sampling,
            onceover_report::Sampling::Sample
        );
        assert_eq!(f.evidence[0].scope.population_rows, Some(n));
        assert!(cov.contains("sampled from 6000 rows"), "{cov}");

        // Checking the same frame in full recounts the rare pair: the
        // claim no longer reads as a bijection.
        let full_cfg = DupeConfig {
            row_cap: n,
            ..DupeConfig::default()
        };
        let (full_findings, _) = profile_duplicate_encodings(&table, &info, &full_cfg);
        assert_eq!(full_findings.len(), 1, "{full_findings:?}");
        assert!(
            !full_findings[0].claim.contains("one-to-one"),
            "{}",
            full_findings[0].claim
        );
        assert!(
            full_findings[0]
                .claim
                .contains("break the majority pairing"),
            "{}",
            full_findings[0].claim
        );
    }

    #[test]
    fn exact_duplicate_encoding_fires() {
        let (codes, labels) = codes_labels(4, 400);
        let frame = df!["code" => &codes, "label" => &labels].unwrap();
        let table = extract(&frame);
        let info = info_for(&table);
        let (findings, _) = profile_duplicate_encodings(&table, &info, &DupeConfig::default());
        assert_eq!(findings.len(), 1, "{findings:?}");
        let f = &findings[0];
        assert_eq!(f.finding_type.as_deref(), Some("duplicate_encoding"));
        assert!(f.claim.contains("one-to-one"), "{}", f.claim);
    }

    #[test]
    fn rollup_reports_as_rollup() {
        // 9 cities rolling up to 3 states.
        let cities: Vec<String> = (0..450).map(|r| format!("city_{}", r % 9)).collect();
        let states: Vec<String> = (0..450).map(|r| format!("state_{}", (r % 9) / 3)).collect();
        let frame = df!["city" => &cities, "state" => &states].unwrap();
        let table = extract(&frame);
        let info = info_for(&table);
        let (findings, _) = profile_duplicate_encodings(&table, &info, &DupeConfig::default());
        assert_eq!(findings.len(), 1, "{findings:?}");
        let f = &findings[0];
        assert!(f.claim.contains("rolls up to"), "{}", f.claim);
        assert_eq!(
            f.evidence[0].metrics.get("form").and_then(|v| v.as_str()),
            Some("rollup")
        );
    }

    #[test]
    fn exact_rollup_counts_exceptions_on_the_stated_mapping() {
        // Each fine level determines its coarse level exactly, but
        // a coarse level has several fine preimages: the rollup's
        // exception count is about the stated mapping (0), not the
        // either-direction count.
        let fine: Vec<String> = (0..360).map(|r| format!("fine_{}", r % 6)).collect();
        let coarse: Vec<String> = (0..360)
            .map(|r| format!("coarse_{}", (r % 6) / 2))
            .collect();
        let frame = df!["fine" => &fine, "coarse" => &coarse].unwrap();
        let table = extract(&frame);
        let info = info_for(&table);
        let (findings, _) = profile_duplicate_encodings(&table, &info, &DupeConfig::default());
        assert_eq!(findings.len(), 1, "{findings:?}");
        let m = &findings[0].evidence[0].metrics;
        assert_eq!(m.get("form").and_then(|v| v.as_str()), Some("rollup"));
        assert_eq!(m.get("exceptions").and_then(|v| v.as_u64()), Some(0));
        assert!(
            findings[0].claim.contains("on all 360 rows"),
            "{}",
            findings[0].claim
        );
    }

    #[test]
    fn identifier_side_does_not_fire() {
        let (codes, labels) = codes_labels(4, 400);
        let frame = df!["code" => &codes, "label" => &labels].unwrap();
        let table = extract(&frame);
        let mut info = info_for(&table);
        info[0].card_role = "unique".into();
        let (findings, _) = profile_duplicate_encodings(&table, &info, &DupeConfig::default());
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn partial_mapping_reports_exceptions() {
        // 50 groups: the pair slack (1.02 x the larger margin)
        // admits a few extra pairs, so a concentrated mislabeling
        // still reads as one grouping with exceptions.
        let (codes, mut labels) = codes_labels(50, 1000);
        // Relabel 5 rows of code 1 with group 7's label: one extra
        // value pair, 5 exceptions.
        let mut done = 0;
        for k in 0..labels.len() {
            if codes[k] == 1 && done < 5 {
                labels[k] = "group_7".into();
                done += 1;
            }
        }
        assert_eq!(done, 5);
        let frame = df!["code" => &codes, "label" => &labels].unwrap();
        let table = extract(&frame);
        let info = info_for(&table);
        let (findings, _) = profile_duplicate_encodings(&table, &info, &DupeConfig::default());
        assert_eq!(findings.len(), 1, "{findings:?}");
        let m = &findings[0].evidence[0].metrics;
        assert_eq!(m.get("exact").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(m.get("exceptions").and_then(|v| v.as_u64()), Some(5));
    }

    #[test]
    fn independent_columns_do_not_fire() {
        let a: Vec<String> = (0..400).map(|r| format!("a_{}", r % 4)).collect();
        let b: Vec<String> = (0..400).map(|r| format!("b_{}", (r / 4) % 5)).collect();
        let frame = df!["a" => &a, "b" => &b].unwrap();
        let table = extract(&frame);
        let info = info_for(&table);
        let (findings, _) = profile_duplicate_encodings(&table, &info, &DupeConfig::default());
        assert!(findings.is_empty(), "{findings:?}");
    }
}
