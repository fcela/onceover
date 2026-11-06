//! Rows that disagree with the table's own structure.
//!
//! Three checks, none of which a column-at-a-time scan can see:
//!
//! - Near-duplicate rows: rows that match another row on every column but one.
//!   A record entered twice with a new id or timestamp is not an exact duplicate.
//! - Residual outliers: for a pair of columns on a tight line, the few rows far
//!   from it. Each value can be ordinary for its column and still impossible
//!   next to the other.
//! - Swapped values: columns whose usual ranges do not overlap, on rows where
//!   each holds a value from the other's range.
//!
//! Every reported count comes from a scan of all rows in scope.

use std::collections::BTreeMap;

use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use rayon::prelude::*;
use serde_json::json;

use crate::dep::PairDep;

/// Near-duplicate scanning hashes every value once per column, so it is
/// skipped above this many cells.
const MAX_NEAR_DUPLICATE_CELLS: usize = 50_000_000;
const MIN_ROWS: usize = 100;
const MIN_OTHER_COLUMNS: usize = 3;
const MIN_NEAR_DUPLICATE_ROWS: usize = 5;
const MIN_NEAR_DUPLICATE_SHARE: f64 = 0.001;
/// With the column left out, this share of rows must still be distinct. Below
/// it the remaining columns are too coarse to tell records apart.
const MIN_DISTINCT_WITHOUT: f64 = 0.5;
const MAX_NEAR_DUPLICATE_STATEMENTS: usize = 3;
/// Chance must account for less than this share of the repeats:
/// the repeats are stated only when they exceed this multiple of
/// the coincidences expected if the compared columns were
/// independent (see `chance_pairs`).
const MIN_OVER_CHANCE: f64 = 4.0;
/// A compared column enters the chance model only when it raises
/// the distinct count of the columns already in it by more than this
/// share; otherwise it is determined by them and adds no resolution.
const REDUNDANT_SHARE: f64 = 0.01;
/// At most this many compared columns are tested, coarsest first;
/// columns past the limit are left out of the chance model, which
/// can only raise the chance estimate.
const MAX_RESOLUTION_TESTS: usize = 64;

const MIN_PAIR_R: f64 = 0.9;
const MAX_LINE_PAIRS: usize = 256;
const MIN_RESIDUAL_Z: f64 = 8.0;
const MAX_OUTLIER_SHARE: f64 = 0.005;
const MIN_OUTLIER_FLOOR: usize = 5;
/// The largest ordinary residual must stay below this share of the smallest
/// flagged one, so the outliers stand apart from a fat-tailed bulk.
const MAX_ORDINARY_OVER_FLAGGED: f64 = 0.6;
const MAX_LISTED_ROWS: usize = 20;

const SWAP_SAMPLE_ROWS: usize = 20_000;
const MAX_SWAP_PAIRS: usize = 5_000;
const MIN_SWAPPED_ROWS: usize = 2;
const MAX_SWAPPED_SHARE: f64 = 0.05;
/// A column's usual range is its middle 90%, widened by this share of itself
/// on each side, so that an ordinary value swapped across still falls inside.
const RANGE_MARGIN: f64 = 0.5;
/// Share of the rows where either value moved in which both did.
const MIN_SWAP_COINCIDENCE: f64 = 0.8;

fn value_hash(col: &TypedCol, r: usize, salt: u64) -> u64 {
    const NULL: u64 = 0x9E37_79B9_7F4A_7C15;
    let raw = match col {
        TypedCol::I64 { vals } => vals.at(r).map(|v| v as u64),
        TypedCol::U64 { vals } => vals.at(r),
        TypedCol::F64 { vals } => vals.at(r).map(f64::to_bits),
        TypedCol::Bool { vals } => vals[r].map(u64::from),
        TypedCol::Date { vals } => vals[r].map(|v| v as u64),
        TypedCol::Datetime { vals, .. } => vals[r].map(|v| v as u64),
        _ => col.str_at(r).map(|s| {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for b in s.as_bytes() {
                h ^= *b as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
            h
        }),
    };
    // A strong mix, so that summing values across columns is order-aware.
    let mut x = raw.unwrap_or(NULL) ^ salt;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn distinct_count(mut keys: Vec<u64>) -> usize {
    keys.sort_unstable();
    keys.dedup();
    keys.len()
}

fn salt_of(c: usize) -> u64 {
    (c as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

/// Probability that two rows drawn at random agree on one column:
/// the sum of squared value shares, nulls counted as one value.
fn collision_probability(table: &TypedTable, c: usize) -> f64 {
    let salt = salt_of(c);
    let mut keys: Vec<u64> = (0..table.n)
        .map(|r| value_hash(&table.cols[c], r, salt))
        .collect();
    keys.sort_unstable();
    let n = table.n as f64;
    let mut p = 0.0;
    let mut start = 0;
    for i in 1..=keys.len() {
        if i == keys.len() || keys[i] != keys[start] {
            let share = (i - start) as f64 / n;
            p += share * share;
            start = i;
        }
    }
    p
}

/// Coincident row pairs expected on the compared columns (every
/// column but `left_out`) if those columns were independent:
/// C(n, 2) times the product of their collision probabilities.
///
/// Independence understates chance when the compared columns
/// depend on each other — a price fixed by store and product
/// repeats exactly when store and product do — so the product runs
/// only over columns that add resolution. Columns enter coarsest
/// first; a column is kept when it splits the value combinations
/// of the columns already kept (it raises their distinct count by
/// more than `REDUNDANT_SHARE`), and skipped otherwise, because it
/// is then determined by the coarser columns.
fn chance_pairs(table: &TypedTable, left_out: usize, probs: &[(usize, f64)]) -> f64 {
    let mut order: Vec<(usize, f64)> = probs
        .iter()
        .copied()
        .filter(|(c, _)| *c != left_out)
        .collect();
    order.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut keys = vec![0u64; table.n];
    let mut distinct = 1usize;
    let n = table.n as f64;
    let mut chance = n * (n - 1.0) / 2.0;
    for &(c, p) in order.iter().take(MAX_RESOLUTION_TESTS) {
        let salt = salt_of(c);
        let with: Vec<u64> = keys
            .iter()
            .enumerate()
            .map(|(r, k)| k.wrapping_add(value_hash(&table.cols[c], r, salt)))
            .collect();
        let distinct_with = distinct_count(with.clone());
        if distinct_with as f64 > distinct as f64 * (1.0 + REDUNDANT_SHARE) + 1.0 {
            keys = with;
            distinct = distinct_with;
            chance *= p;
        }
    }
    chance
}

fn hashable(col: &TypedCol) -> bool {
    !matches!(col, TypedCol::Null { .. } | TypedCol::Other { .. })
}

fn near_duplicates(table: &TypedTable, findings: &mut Vec<Finding>) -> Option<String> {
    let columns: Vec<usize> = (0..table.cols.len())
        .filter(|&c| hashable(&table.cols[c]))
        .collect();
    if table.n < MIN_ROWS || columns.len() <= MIN_OTHER_COLUMNS {
        return None;
    }
    if table.n.saturating_mul(columns.len()) > MAX_NEAR_DUPLICATE_CELLS {
        return Some(format!(
            "near-duplicate rows not checked above {MAX_NEAR_DUPLICATE_CELLS} cells"
        ));
    }
    let mut totals = vec![0u64; table.n];
    for &c in &columns {
        let salt = salt_of(c);
        for (r, total) in totals.iter_mut().enumerate() {
            *total = total.wrapping_add(value_hash(&table.cols[c], r, salt));
        }
    }
    let exact_repeats = table.n - distinct_count(totals.clone());
    let bar = MIN_NEAR_DUPLICATE_ROWS.max((MIN_NEAR_DUPLICATE_SHARE * table.n as f64) as usize);
    let hits: Vec<(usize, usize)> = columns
        .par_iter()
        .filter_map(|&c| {
            let salt = salt_of(c);
            let keys: Vec<u64> = totals
                .iter()
                .enumerate()
                .map(|(r, t)| t.wrapping_sub(value_hash(&table.cols[c], r, salt)))
                .collect();
            let distinct = distinct_count(keys);
            if (distinct as f64) < MIN_DISTINCT_WITHOUT * table.n as f64 {
                return None;
            }
            let extra = (table.n - distinct).saturating_sub(exact_repeats);
            (extra >= bar).then_some((c, extra))
        })
        .collect();
    // Repeats that chance coincidence on the compared columns would
    // produce are not evidence of a record entered twice: five
    // independent six-level answers already coincide on about one
    // pair in 7,800.
    let probs: Vec<(usize, f64)> = if hits.is_empty() {
        Vec::new()
    } else {
        columns
            .par_iter()
            .map(|&c| (c, collision_probability(table, c)))
            .collect()
    };
    let mut hits: Vec<(usize, usize, f64)> = hits
        .par_iter()
        .filter_map(|&(c, extra)| {
            let chance = chance_pairs(table, c, &probs);
            (extra as f64 > MIN_OVER_CHANCE * chance).then_some((c, extra, chance))
        })
        .collect();
    hits.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (c, extra, chance) in hits.into_iter().take(MAX_NEAR_DUPLICATE_STATEMENTS) {
        let name = &table.names[c];
        findings.push(Finding::new(
            Pattern::NearDuplicateRows,
            BTreeMap::from([("columns".to_string(), json!([name]))]),
            format!(
                "{extra} rows repeat another row on every column except {name} ({:.2}% of {} rows)",
                100.0 * extra as f64 / table.n as f64,
                table.n
            ),
            EvidenceBasis::Measured,
            0.68 + 0.2 * (extra as f64 / table.n as f64 / 0.1).min(1.0),
            BTreeMap::from([
                ("n".to_string(), json!(table.n)),
                ("varying_column".to_string(), json!(name)),
                ("near_duplicates".to_string(), json!(extra)),
                ("exact_repeats".to_string(), json!(exact_repeats)),
                ("expected_by_chance".to_string(), json!(chance)),
            ]),
            Some(format!(
                "check whether these are one record entered twice with a different {name}"
            )),
            Method::new(
                "row_consistency",
                "leave_one_column_out_hash",
                "row hashes with each column left out in turn (onceover v0.1)",
            ),
        ));
    }
    None
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    sorted[((q * (sorted.len() - 1) as f64) as usize).min(sorted.len() - 1)]
}

fn fit_line(pairs: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = pairs.len() as f64;
    let (mx, my) = pairs
        .iter()
        .fold((0.0, 0.0), |(a, b), (x, y)| (a + x / n, b + y / n));
    let (mut sxx, mut sxy) = (0.0, 0.0);
    for (x, y) in pairs {
        sxx += (x - mx) * (x - mx);
        sxy += (x - mx) * (y - my);
    }
    (sxx > 0.0).then(|| (sxy / sxx, my - sxy / sxx * mx))
}

/// Slope, intercept, largest ordinary residual, complete rows, and the
/// outlying rows with their robust z.
type LineOutliers = (f64, f64, f64, usize, Vec<(usize, f64)>);

/// Outlying rows for one tight pair.
fn residual_outliers(table: &TypedTable, a: usize, b: usize) -> Option<LineOutliers> {
    let rows: Vec<(usize, f64, f64)> = (0..table.n)
        .filter_map(|r| {
            let (x, y) = (table.cols[a].numeric_at(r)?, table.cols[b].numeric_at(r)?);
            (x.is_finite() && y.is_finite()).then_some((r, x, y))
        })
        .collect();
    if rows.len() < MIN_ROWS {
        return None;
    }
    // Fit, drop what is far from the first line, and fit again, so the
    // outliers being looked for don't pull the line towards themselves.
    let all: Vec<(f64, f64)> = rows.iter().map(|&(_, x, y)| (x, y)).collect();
    let (s0, i0) = fit_line(&all)?;
    let mut abs0: Vec<f64> = all.iter().map(|(x, y)| (y - s0 * x - i0).abs()).collect();
    abs0.sort_by(f64::total_cmp);
    let trim = 3.0 * 1.4826 * quantile(&abs0, 0.5);
    let kept: Vec<(f64, f64)> = all
        .iter()
        .copied()
        .filter(|(x, y)| (y - s0 * x - i0).abs() <= trim)
        .collect();
    let (slope, intercept) = fit_line(&kept)?;
    let residual = |x: f64, y: f64| (y - slope * x - intercept).abs();
    let mut abs: Vec<f64> = all.iter().map(|(x, y)| residual(*x, *y)).collect();
    abs.sort_by(f64::total_cmp);
    let scale = 1.4826 * quantile(&abs, 0.5);
    // Exact relations belong to the identity passes.
    if scale <= 1e-9 * (1.0 + intercept.abs()) {
        return None;
    }
    let mut flagged: Vec<(usize, f64)> = rows
        .iter()
        .map(|&(r, x, y)| (r, residual(x, y) / scale))
        .filter(|(_, z)| *z >= MIN_RESIDUAL_Z)
        .collect();
    let limit = MIN_OUTLIER_FLOOR.max((MAX_OUTLIER_SHARE * rows.len() as f64) as usize);
    if flagged.is_empty() || flagged.len() > limit {
        return None;
    }
    let smallest = flagged.iter().map(|f| f.1).fold(f64::INFINITY, f64::min);
    let ordinary = rows
        .iter()
        .map(|&(_, x, y)| residual(x, y) / scale)
        .filter(|z| *z < MIN_RESIDUAL_Z)
        .fold(0.0, f64::max);
    if ordinary > MAX_ORDINARY_OVER_FLAGGED * smallest {
        return None;
    }
    flagged.sort_by(|p, q| q.1.total_cmp(&p.1).then(p.0.cmp(&q.0)));
    Some((slope, intercept, ordinary * scale, rows.len(), flagged))
}

fn swapped_values(table: &TypedTable, findings: &mut Vec<Finding>, measures: &[usize]) {
    let sample = crate::stride_rows(table.n, SWAP_SAMPLE_ROWS);
    // Each column's usual range is based on the middle 90%, so up to 5% of
    // swapped rows can't widen it.
    let bulk: Vec<Option<(f64, f64)>> = measures
        .iter()
        .map(|&c| {
            let mut v: Vec<f64> = sample
                .iter()
                .filter_map(|&r| table.cols[c].numeric_at(r as usize))
                .filter(|x| x.is_finite())
                .collect();
            if v.len() < MIN_ROWS {
                return None;
            }
            v.sort_by(f64::total_cmp);
            let (lo, hi) = (quantile(&v, 0.05), quantile(&v, 0.95));
            let margin = RANGE_MARGIN * (hi - lo);
            (hi > lo).then_some((lo - margin, hi + margin))
        })
        .collect();
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for x in 0..measures.len() {
        for y in x + 1..measures.len() {
            if let (Some(a), Some(b)) = (bulk[x], bulk[y]) {
                if (a.1 < b.0 || b.1 < a.0) && pairs.len() < MAX_SWAP_PAIRS {
                    pairs.push((x, y));
                }
            }
        }
    }
    let inside = |v: f64, range: (f64, f64)| v >= range.0 && v <= range.1;
    let mut hits: Vec<Finding> = pairs
        .par_iter()
        .filter_map(|&(x, y)| {
            let (ca, cb) = (measures[x], measures[y]);
            let (ra, rb) = (bulk[x]?, bulk[y]?);
            let is_swap = |r: usize| -> Option<bool> {
                let (a, b) = (table.cols[ca].numeric_at(r)?, table.cols[cb].numeric_at(r)?);
                Some(inside(a, rb) && inside(b, ra))
            };
            // A pair with no swap in the sample costs no full scan.
            if !sample.iter().any(|&r| is_swap(r as usize) == Some(true)) {
                return None;
            }
            let rows: Vec<usize> = (0..table.n)
                .filter(|&r| is_swap(r) == Some(true))
                .collect();
            let (mut complete, mut a_moved, mut b_moved) = (0usize, 0usize, 0usize);
            for r in 0..table.n {
                let values = table.cols[ca].numeric_at(r).zip(table.cols[cb].numeric_at(r));
                if let Some((a, b)) = values {
                    complete += 1;
                    a_moved += usize::from(inside(a, rb));
                    b_moved += usize::from(inside(b, ra));
                }
            }
            // In a real swap the two moves coincide. A tail value of one
            // column can land in the other's range by chance, and then the
            // other column stays put.
            let coincide = rows.len() as f64 >= MIN_SWAP_COINCIDENCE * a_moved.max(b_moved) as f64;
            if rows.len() < MIN_SWAPPED_ROWS
                || rows.len() as f64 > MAX_SWAPPED_SHARE * complete as f64
                || !coincide
            {
                return None;
            }
            let (na, nb) = (&table.names[ca], &table.names[cb]);
            Some(Finding::new(
                Pattern::RowInconsistency,
                BTreeMap::from([("columns".to_string(), json!([na, nb]))]),
                format!(
                    "{na} and {nb} look swapped on {} of {complete} rows: each holds a value from the other's usual range",
                    rows.len()
                ),
                EvidenceBasis::Measured,
                0.82,
                BTreeMap::from([
                    ("kind".to_string(), json!("swapped_values")),
                    ("n".to_string(), json!(complete)),
                    ("swapped_rows".to_string(), json!(rows.len())),
                    ("rows".to_string(), json!(rows.iter().take(MAX_LISTED_ROWS).collect::<Vec<_>>())),
                    ("range_first".to_string(), json!([ra.0, ra.1])),
                    ("range_second".to_string(), json!([rb.0, rb.1])),
                ]),
                Some("check these rows at the source; two fields may have been entered in the wrong order".into()),
                Method::new(
                    "row_consistency",
                    "swapped_range_scan",
                    "rows where each of two columns with disjoint usual ranges falls in the other's range (onceover v0.1)",
                ),
            ))
        })
        .collect();
    findings.append(&mut hits);
}

/// Near duplicates, swapped values, and rows off a tight line.
pub fn profile_row_consistency(
    table: &TypedTable,
    deps: &[PairDep],
    excluded: &[usize],
    measures: &[usize],
) -> (Vec<Finding>, String) {
    let mut findings = Vec::new();
    let skipped = near_duplicates(table, &mut findings);
    swapped_values(table, &mut findings, measures);

    let mut tight: Vec<&PairDep> = deps
        .iter()
        .filter(|d| !excluded.contains(&d.a) && !excluded.contains(&d.b))
        .filter(|d| d.pearson.is_some_and(|p| p.abs() >= MIN_PAIR_R))
        .collect();
    tight.sort_by(|x, y| {
        let r = |d: &PairDep| d.pearson.unwrap_or(0.0).abs();
        r(y).total_cmp(&r(x))
            .then(x.a.cmp(&y.a))
            .then(x.b.cmp(&y.b))
    });
    tight.truncate(MAX_LINE_PAIRS);
    let examined = tight.len();
    let mut lines: Vec<Finding> = tight
        .par_iter()
        .filter_map(|d| {
            let (slope, intercept, band, n, flagged) = residual_outliers(table, d.a, d.b)?;
            let (x, y) = (&table.names[d.a], &table.names[d.b]);
            let listed: Vec<usize> = flagged.iter().take(MAX_LISTED_ROWS).map(|f| f.0).collect();
            Some(Finding::new(
                Pattern::RowInconsistency,
                BTreeMap::from([("columns".to_string(), json!([x, y]))]),
                format!(
                    "{} far from the line {y} = {:.4} * {x} {} {:.4}: the other {} rows stay within {:.4} of it",
                    if flagged.len() == 1 {
                        "1 row sits".to_string()
                    } else {
                        format!("{} rows sit", flagged.len())
                    },
                    slope,
                    if intercept < 0.0 { "-" } else { "+" },
                    intercept.abs(),
                    n - flagged.len(),
                    band,
                ),
                EvidenceBasis::Measured,
                0.78,
                BTreeMap::from([
                    ("kind".to_string(), json!("residual_outlier")),
                    ("n".to_string(), json!(n)),
                    ("x".to_string(), json!(x)),
                    ("y".to_string(), json!(y)),
                    ("slope".to_string(), json!(slope)),
                    ("intercept".to_string(), json!(intercept)),
                    ("outliers".to_string(), json!(flagged.len())),
                    ("rows".to_string(), json!(listed)),
                    ("ordinary_band".to_string(), json!(band)),
                ]),
                Some("each value may look ordinary alone; check these rows, where the pair disagrees".into()),
                Method::new(
                    "row_consistency",
                    "residual_scan",
                    "robust line fit over all complete rows, isolated residuals beyond 8 robust z (onceover v0.1)",
                ),
            ))
        })
        .collect();
    findings.append(&mut lines);

    let mut detail = format!(
        "{} statements; {} tight numeric pairs examined for rows off their line",
        findings.len(),
        examined
    );
    if let Some(note) = skipped {
        detail.push_str("; ");
        detail.push_str(&note);
    }
    (findings, detail)
}
