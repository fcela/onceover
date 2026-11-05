//! Pairwise order constraints: `a <= b` between two columns of
//! compatible kind and scale, holding on every evaluated row or
//! violated on a small share of them (issue #132).
//!
//! An end before its start, a part heavier than the whole, a
//! discharge before its admission: the row-level order between two
//! columns is one of the most common real data errors, and it is
//! cheap to check exactly. No other pass states it — sortedness is
//! about one column against row order, functional dependencies are
//! about equality of values, and the dependence screen measures
//! association, which a violated order barely dents.
//!
//! Candidate pairs share a value kind: numeric with numeric, dates
//! with dates, datetimes with datetimes of one unit. A pair is a
//! row-level candidate only when the columns' ranges overlap
//! substantially — overlap at least a quarter of the narrower
//! range, widths within 100x — because when one column's maximum
//! does not exceed the other's minimum, `a <= b` is a fact about
//! the two marginal ranges, not about how values pair up in rows,
//! and when the widths differ by orders of magnitude the columns
//! measure different quantities and any near-universal order
//! between them is a scale artifact. Both columns need at least
//! five distinct values: below that the pair is a levels question
//! for the dependency passes, and a constant column makes the
//! "constraint" a statement about one column's range.
//!
//! Each direction is tested on a deterministic sample of rows
//! when the table is large and verified on all rows before it is
//! reported: a direction is reported when its violations are zero
//! or at most 1% of the rows where both columns are present, with
//! the violating rows in the evidence. At most one direction of a
//! pair is reported — the one with fewer violations. When both
//! directions pass with equal violations the pair states no order:
//! zero in both directions means the columns are equal wherever
//! both are present (a duplicate-encoding statement, not an
//! order), and equal nonzero counts mean the columns cross as
//! often as they agree, a near-equality, not a near-universal
//! order.
//!
//! Pairs are evaluated in column order on the shared pair budget
//! (`max_pairs`, the same budget that caps the dependence
//! screen); when the budget binds, coverage says how many of the
//! eligible pairs were evaluated. A pair whose columns are
//! functionally related in this run's exact functional
//! dependencies — one is a function of the other, so any order
//! between them may follow from their derivation — is reported
//! with that fact in its evidence and scored below order findings
//! between unrelated columns, and every violation finding scores
//! above every exact one: the exceptions to a near-universal
//! order are the data-error signal, an always-true order between
//! derived columns is the weakest statement this pass makes.

use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashSet};

use onceover_core::{TypedCol, TypedTable};

/// Minimum rows where both columns are present before a pair
/// statement is meaningful.
const MIN_EVALUATED: usize = 100;
/// Minimum distinct values per column (over its non-null values).
const MIN_DISTINCT: usize = 5;
/// A direction is reported when its violations are at most this
/// share of the evaluated rows (zero violations always reports).
const MAX_VIOLATION_SHARE: f64 = 0.01;
/// Rows sampled per pair for the screening stage when the table
/// has more rows than this; survivors are verified on all rows.
const SCREEN_ROWS: usize = 4096;
/// Screening survival bar: twice the reporting bar, so sampling
/// noise at the reporting bar cannot screen out a true pair.
const SCREEN_SHARE: f64 = 0.02;
/// Compatible scales: the two ranges' widths within this ratio,
/// and their overlap at least MIN_OVERLAP_SHARE of the narrower
/// range.
const MAX_WIDTH_RATIO: f64 = 100.0;
const MIN_OVERLAP_SHARE: f64 = 0.25;
/// Integer magnitudes up to 2^53 are exactly representable in
/// f64; an integer column beyond that cannot be compared exactly
/// against a float column and pairs only with integer columns.
const F64_EXACT_LIMIT: i128 = 1 << 53;
/// Findings emitted by this pass per profile, in the ranking
/// order documented at the top of this module; coverage states
/// how many reportable directions the cap turned away.
const MAX_FINDINGS: usize = 25;
/// Violating rows listed in the evidence (with the first and
/// last violating row, so the implicated scope is retrievable).
const EXAMPLE_ROWS: usize = 10;

#[derive(Clone, Debug)]
pub struct OrderConfig {
    /// The shared pair budget: at most this many eligible pairs
    /// are evaluated, in column order.
    pub max_pairs: usize,
    /// Unordered column-index pairs (lo, hi) between which an
    /// exact single-column functional dependency holds in either
    /// direction in this run — the derived-from-one-another pairs.
    pub derived_pairs: BTreeSet<(usize, usize)>,
}

#[derive(Clone, Debug, Default)]
pub struct OrderOutcome {
    pub findings: Vec<Finding>,
    /// Eligible pairs (same kind, per-column bars passed, ranges
    /// compatible) before the pair budget.
    pub pairs_total: usize,
    /// Pairs whose rows were examined, budget permitting.
    pub pairs_evaluated: usize,
    /// Directions that passed the reporting bar before the
    /// findings cap (at most one per pair).
    pub pairs_passing: usize,
    /// At least one pair was screened on the row sample.
    pub screened: bool,
    /// Numeric and temporal columns seen (the pass's universe).
    pub columns_seen: usize,
}

/// A column prepared for pair comparison: values in the exact
/// comparison domain of its kind, plus the marginal statistics
/// the candidate guards read.
enum ColPrep {
    Ints {
        vals: Vec<Option<i128>>,
        min: f64,
        max: f64,
        distinct: usize,
        nonnull: usize,
        f64_exact: bool,
    },
    Floats {
        vals: Vec<Option<f64>>,
        min: f64,
        max: f64,
        distinct: usize,
        nonnull: usize,
    },
    Ticks {
        vals: Vec<Option<i128>>,
        /// Nanoseconds per tick (a day for dates), identifying
        /// the unit: ticks of different units never pair.
        unit_ns: u64,
        min: f64,
        max: f64,
        distinct: usize,
        nonnull: usize,
    },
}

impl ColPrep {
    fn min(&self) -> f64 {
        match self {
            ColPrep::Ints { min, .. }
            | ColPrep::Floats { min, .. }
            | ColPrep::Ticks { min, .. } => *min,
        }
    }
    fn max(&self) -> f64 {
        match self {
            ColPrep::Ints { max, .. }
            | ColPrep::Floats { max, .. }
            | ColPrep::Ticks { max, .. } => *max,
        }
    }
    fn distinct(&self) -> usize {
        match self {
            ColPrep::Ints { distinct, .. }
            | ColPrep::Floats { distinct, .. }
            | ColPrep::Ticks { distinct, .. } => *distinct,
        }
    }
    fn nonnull(&self) -> usize {
        match self {
            ColPrep::Ints { nonnull, .. }
            | ColPrep::Floats { nonnull, .. }
            | ColPrep::Ticks { nonnull, .. } => *nonnull,
        }
    }
    fn is_temporal(&self) -> bool {
        matches!(self, ColPrep::Ticks { .. })
    }
}

fn prep_column(col: &TypedCol) -> Option<ColPrep> {
    match col {
        TypedCol::I64 { vals } => {
            let mut out = Vec::with_capacity(vals.len());
            let mut seen: HashSet<i128> = HashSet::new();
            let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
            let mut exact = true;
            for v in vals.iter() {
                match v {
                    Some(&x) => {
                        let x = x as i128;
                        seen.insert(x);
                        let f = x as f64;
                        min = min.min(f);
                        max = max.max(f);
                        if x.abs() > F64_EXACT_LIMIT {
                            exact = false;
                        }
                        out.push(Some(x));
                    }
                    None => out.push(None),
                }
            }
            Some(ColPrep::Ints {
                nonnull: seen_count(&out),
                distinct: seen.len(),
                vals: out,
                min,
                max,
                f64_exact: exact,
            })
        }
        TypedCol::U64 { vals } => {
            let mut out = Vec::with_capacity(vals.len());
            let mut seen: HashSet<i128> = HashSet::new();
            let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
            let mut exact = true;
            for v in vals.iter() {
                match v {
                    Some(&x) => {
                        let x = x as i128;
                        seen.insert(x);
                        let f = x as f64;
                        min = min.min(f);
                        max = max.max(f);
                        if x > F64_EXACT_LIMIT {
                            exact = false;
                        }
                        out.push(Some(x));
                    }
                    None => out.push(None),
                }
            }
            Some(ColPrep::Ints {
                nonnull: seen_count(&out),
                distinct: seen.len(),
                vals: out,
                min,
                max,
                f64_exact: exact,
            })
        }
        TypedCol::F64 { vals } => {
            let mut out = Vec::with_capacity(vals.len());
            let mut seen: HashSet<u64> = HashSet::new();
            let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
            for v in vals.iter() {
                match v {
                    Some(&x) => {
                        seen.insert(x.to_bits());
                        min = min.min(x);
                        max = max.max(x);
                        out.push(Some(x));
                    }
                    None => out.push(None),
                }
            }
            Some(ColPrep::Floats {
                nonnull: seen_count(&out),
                distinct: seen.len(),
                vals: out,
                min,
                max,
            })
        }
        TypedCol::Date { vals } => {
            let mut out = Vec::with_capacity(vals.len());
            let mut seen: HashSet<i128> = HashSet::new();
            let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
            for v in vals.iter() {
                match v {
                    Some(x) => {
                        let x = *x as i128;
                        seen.insert(x);
                        let f = x as f64;
                        min = min.min(f);
                        max = max.max(f);
                        out.push(Some(x));
                    }
                    None => out.push(None),
                }
            }
            Some(ColPrep::Ticks {
                nonnull: seen_count(&out),
                distinct: seen.len(),
                vals: out,
                unit_ns: 86_400_000_000_000,
                min,
                max,
            })
        }
        TypedCol::Datetime { vals, .. } => {
            let unit_ns = col
                .datetime_unit_ns()
                .expect("a datetime column carries a unit");
            let mut out = Vec::with_capacity(vals.len());
            let mut seen: HashSet<i128> = HashSet::new();
            let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
            for v in vals.iter() {
                match v {
                    Some(x) => {
                        let x = *x as i128;
                        seen.insert(x);
                        let f = x as f64;
                        min = min.min(f);
                        max = max.max(f);
                        out.push(Some(x));
                    }
                    None => out.push(None),
                }
            }
            Some(ColPrep::Ticks {
                nonnull: seen_count(&out),
                distinct: seen.len(),
                vals: out,
                unit_ns,
                min,
                max,
            })
        }
        _ => None,
    }
}

fn seen_count<T>(vals: &[Option<T>]) -> usize {
    vals.iter().filter(|v| v.is_some()).count()
}

/// Whether two prepared columns may be compared exactly, and in
/// which domain: `Some(true)` compares as f64, `Some(false)` as
/// integers, `None` never pairs (different kinds, different
/// datetime units, or an inexact integer against a float).
fn pair_domain(a: &ColPrep, b: &ColPrep) -> Option<bool> {
    match (a, b) {
        (ColPrep::Ticks { unit_ns: u, .. }, ColPrep::Ticks { unit_ns: v, .. }) => {
            if u == v {
                Some(false)
            } else {
                None
            }
        }
        (ColPrep::Ticks { .. }, _) | (_, ColPrep::Ticks { .. }) => None,
        (ColPrep::Floats { .. }, ColPrep::Floats { .. }) => Some(true),
        (ColPrep::Ints { .. }, ColPrep::Ints { .. }) => Some(false),
        (ColPrep::Ints { f64_exact, .. }, ColPrep::Floats { .. })
        | (ColPrep::Floats { .. }, ColPrep::Ints { f64_exact, .. }) => {
            if *f64_exact {
                Some(true)
            } else {
                None
            }
        }
    }
}

/// The compatible-scales guard on the marginal ranges: overlap
/// at least a quarter of the narrower range, widths within the
/// ratio cap. Widths are in each column's own units (ticks for
/// temporal columns, which only pair within one unit).
fn ranges_compatible(a: &ColPrep, b: &ColPrep) -> bool {
    let (amin, amax) = (a.min(), a.max());
    let (bmin, bmax) = (b.min(), b.max());
    let (wa, wb) = (amax - amin, bmax - bmin);
    if wa <= 0.0 || wb <= 0.0 {
        return false;
    }
    let (narrow, wide) = if wa <= wb { (wa, wb) } else { (wb, wa) };
    if wide > MAX_WIDTH_RATIO * narrow {
        return false;
    }
    let overlap = amax.min(bmax) - amin.max(bmin);
    overlap > 0.0 && overlap >= MIN_OVERLAP_SHARE * narrow
}

trait ScanVal: Copy + PartialOrd + std::ops::Sub<Output = Self> + Default {
    fn as_f64(self) -> f64;
    /// The exact integer value, for temporal scans whose values
    /// are ticks; zero for float scans (never temporal).
    fn as_ticks(self) -> i128;
}
impl ScanVal for f64 {
    fn as_f64(self) -> f64 {
        self
    }
    fn as_ticks(self) -> i128 {
        0
    }
}
impl ScanVal for i128 {
    fn as_f64(self) -> f64 {
        self as f64
    }
    fn as_ticks(self) -> i128 {
        self
    }
}

/// Violation counts for both directions over the given rows, and
/// how many of those rows had both values present.
fn count_directions<T: ScanVal>(
    a: &[Option<T>],
    b: &[Option<T>],
    rows: &[u32],
) -> (usize, usize, usize) {
    let (mut viol_ab, mut viol_ba, mut complete) = (0usize, 0usize, 0usize);
    for &r in rows {
        let r = r as usize;
        if let (Some(x), Some(y)) = (a[r], b[r]) {
            complete += 1;
            if x > y {
                viol_ab += 1;
            } else if y > x {
                viol_ba += 1;
            }
        }
    }
    (viol_ab, viol_ba, complete)
}

#[derive(Default)]
struct DirDetail<T> {
    violations: usize,
    example_rows: Vec<usize>,
    first_row: Option<usize>,
    last_row: Option<usize>,
    /// Largest (lower - upper) over the violating rows, and the
    /// row and values where it occurs.
    worst: T,
    worst_row: Option<usize>,
    worst_lower: T,
    worst_upper: T,
    /// Smallest (upper - lower) over the satisfied rows.
    min_slack: Option<T>,
}

struct PairDetail<T> {
    n: usize,
    equal: usize,
    lo_min: T,
    lo_max: T,
    hi_min: T,
    hi_max: T,
    /// Detail for `first <= second`.
    fs: DirDetail<T>,
    /// Detail for `second <= first`.
    sf: DirDetail<T>,
}

/// Full-row verification of one pair: every statistic a finding
/// may state, for both directions, in one pass.
fn verify_pair<T: ScanVal>(
    a: &[Option<T>],
    b: &[Option<T>],
    want_fs: bool,
    want_sf: bool,
) -> PairDetail<T> {
    let mut d = PairDetail {
        n: 0,
        equal: 0,
        lo_min: T::default(),
        lo_max: T::default(),
        hi_min: T::default(),
        hi_max: T::default(),
        fs: DirDetail::default(),
        sf: DirDetail::default(),
    };
    let mut first = true;
    for r in 0..a.len() {
        let (Some(x), Some(y)) = (a[r], b[r]) else {
            continue;
        };
        d.n += 1;
        if first {
            d.lo_min = x;
            d.lo_max = x;
            d.hi_min = y;
            d.hi_max = y;
            first = false;
        } else {
            if x < d.lo_min {
                d.lo_min = x;
            }
            if x > d.lo_max {
                d.lo_max = x;
            }
            if y < d.hi_min {
                d.hi_min = y;
            }
            if y > d.hi_max {
                d.hi_max = y;
            }
        }
        if x == y {
            d.equal += 1;
            if want_fs {
                note_satisfied(&mut d.fs, T::default());
            }
            if want_sf {
                note_satisfied(&mut d.sf, T::default());
            }
        } else if x > y {
            // Violates first <= second; satisfies second <= first.
            if want_fs {
                note_violation(&mut d.fs, r, x - y, x, y);
            }
            if want_sf {
                note_satisfied(&mut d.sf, x - y);
            }
        } else {
            if want_sf {
                note_violation(&mut d.sf, r, y - x, y, x);
            }
            if want_fs {
                note_satisfied(&mut d.fs, y - x);
            }
        }
    }
    d
}

fn note_violation<T: ScanVal>(dir: &mut DirDetail<T>, row: usize, by: T, lower: T, upper: T) {
    dir.violations += 1;
    if dir.example_rows.len() < EXAMPLE_ROWS {
        dir.example_rows.push(row);
    }
    if dir.first_row.is_none() {
        dir.first_row = Some(row);
    }
    dir.last_row = Some(row);
    if dir.worst_row.is_none() || by > dir.worst {
        dir.worst = by;
        dir.worst_row = Some(row);
        dir.worst_lower = lower;
        dir.worst_upper = upper;
    }
}

fn note_satisfied<T: ScanVal>(dir: &mut DirDetail<T>, slack: T) {
    dir.min_slack = Some(match dir.min_slack {
        Some(s) if s <= slack => s,
        _ => slack,
    });
}

/// A reportable direction, ready to rank and emit.
struct Candidate {
    lower: usize,
    upper: usize,
    violations: usize,
    n: usize,
    equal: usize,
    derived: bool,
    temporal: bool,
    unit_ns: u64,
    example_rows: Vec<usize>,
    first_row: Option<usize>,
    last_row: Option<usize>,
    worst: f64,
    worst_ticks: i128,
    worst_row: Option<usize>,
    worst_lower: f64,
    worst_upper: f64,
    worst_lower_ticks: i128,
    worst_upper_ticks: i128,
    min_slack: Option<f64>,
    min_slack_ticks: Option<i128>,
    lo_min: f64,
    lo_max: f64,
    hi_min: f64,
    hi_max: f64,
    lo_min_ticks: i128,
    lo_max_ticks: i128,
    hi_min_ticks: i128,
    hi_max_ticks: i128,
    screened: bool,
}

impl Candidate {
    fn violation_share(&self) -> f64 {
        self.violations as f64 / self.n.max(1) as f64
    }
    /// The acceptance order: violations of a near-universal order
    /// first (the data-error signal), then exact orders between
    /// unrelated columns, then exact orders between functionally
    /// related columns. Within a class, tighter violation shares
    /// and wider row support rank first; the column order breaks
    /// remaining ties, so the ranking is deterministic.
    fn rank_key(&self) -> (u8, f64, std::cmp::Reverse<usize>, usize, usize) {
        let class = match (self.violations, self.derived) {
            (0, true) => 2,
            (0, false) => 1,
            _ => 0,
        };
        (
            class,
            self.violation_share(),
            std::cmp::Reverse(self.n),
            self.lower,
            self.upper,
        )
    }
    fn score(&self) -> f64 {
        match (self.violations, self.derived) {
            (0, true) => 0.45,
            (0, false) => 0.62,
            _ => 0.80,
        }
    }
}

pub fn order_constraints(table: &TypedTable, cfg: &OrderConfig) -> OrderOutcome {
    let mut out = OrderOutcome::default();
    let preps: Vec<Option<ColPrep>> = table.cols.iter().map(prep_column).collect();
    out.columns_seen = preps.iter().filter(|p| p.is_some()).count();
    let eligible_cols: Vec<usize> = (0..table.cols.len())
        .filter(|&i| {
            preps[i]
                .as_ref()
                .is_some_and(|p| p.distinct() >= MIN_DISTINCT && p.nonnull() >= MIN_EVALUATED)
        })
        .collect();
    // The eligible universe, counted arithmetically over the
    // pair space (never materialized): a pair is eligible when
    // the two columns share an exact comparison domain and their
    // ranges are compatible.
    let pair_eligible = |i: usize, j: usize| -> Option<bool> {
        let (a, b) = (preps[i].as_ref().unwrap(), preps[j].as_ref().unwrap());
        let domain = pair_domain(a, b)?;
        if ranges_compatible(a, b) {
            Some(domain)
        } else {
            None
        }
    };
    for x in 0..eligible_cols.len() {
        for y in (x + 1)..eligible_cols.len() {
            if pair_eligible(eligible_cols[x], eligible_cols[y]).is_some() {
                out.pairs_total += 1;
            }
        }
    }

    let mut candidates: Vec<Candidate> = Vec::new();
    'pairs: for x in 0..eligible_cols.len() {
        for y in (x + 1)..eligible_cols.len() {
            let (i, j) = (eligible_cols[x], eligible_cols[y]);
            let Some(as_float) = pair_eligible(i, j) else {
                continue;
            };
            if out.pairs_evaluated >= cfg.max_pairs {
                break 'pairs;
            }
            out.pairs_evaluated += 1;
            let (a, b) = (preps[i].as_ref().unwrap(), preps[j].as_ref().unwrap());
            let derived = cfg.derived_pairs.contains(&(i.min(j), i.max(j)));
            if let Some(c) = evaluate_pair(table, a, b, i, j, as_float, derived, &mut out) {
                candidates.push(c);
            }
        }
    }

    out.pairs_passing = candidates.len();
    candidates.sort_by(|p, q| p.rank_key().partial_cmp(&q.rank_key()).unwrap());
    for c in candidates.into_iter().take(MAX_FINDINGS) {
        out.findings.push(build_finding(table, &c));
    }
    out
}

/// Screen (when the table is large) and verify one pair; return
/// the reportable direction, if any.
#[allow(clippy::too_many_arguments)]
fn evaluate_pair(
    table: &TypedTable,
    a: &ColPrep,
    b: &ColPrep,
    i: usize,
    j: usize,
    as_float: bool,
    derived: bool,
    out: &mut OrderOutcome,
) -> Option<Candidate> {
    match (a, b, as_float) {
        (ColPrep::Floats { vals: av, .. }, ColPrep::Floats { vals: bv, .. }, true) => {
            dispatch(table, av, bv, i, j, a, b, derived, out)
        }
        (ColPrep::Ints { vals: av, .. }, ColPrep::Floats { vals: bv, .. }, true) => {
            let avf: Vec<Option<f64>> = av.iter().map(|v| v.map(|x| x as f64)).collect();
            dispatch(table, &avf, bv, i, j, a, b, derived, out)
        }
        (ColPrep::Floats { vals: av, .. }, ColPrep::Ints { vals: bv, .. }, true) => {
            let bvf: Vec<Option<f64>> = bv.iter().map(|v| v.map(|x| x as f64)).collect();
            dispatch(table, av, &bvf, i, j, a, b, derived, out)
        }
        (ColPrep::Ints { vals: av, .. }, ColPrep::Ints { vals: bv, .. }, false) => {
            dispatch(table, av, bv, i, j, a, b, derived, out)
        }
        (ColPrep::Ticks { vals: av, .. }, ColPrep::Ticks { vals: bv, .. }, false) => {
            dispatch(table, av, bv, i, j, a, b, derived, out)
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch<T: ScanVal>(
    table: &TypedTable,
    av: &[Option<T>],
    bv: &[Option<T>],
    i: usize,
    j: usize,
    a_prep: &ColPrep,
    b_prep: &ColPrep,
    derived: bool,
    out: &mut OrderOutcome,
) -> Option<Candidate> {
    // Screening stage: when the table is larger than the screen
    // and the pair's complete rows cannot be bounded cheaply,
    // count violations on a deterministic row sample first. A
    // direction that is not near-universal on the sample is not
    // verified; the reporting decision always rests on the full
    // verification below.
    let cheap_full = a_prep.nonnull().min(b_prep.nonnull()) <= SCREEN_ROWS;
    let mut screened = false;
    let (mut want_fs, mut want_sf) = (true, true);
    if table.n > SCREEN_ROWS && !cheap_full {
        screened = true;
        out.screened = true;
        let rows = onceover_core::sample_rows(table.n, SCREEN_ROWS);
        let (v_fs, v_sf, complete) = count_directions(av, bv, &rows);
        if complete >= 30 {
            want_fs = v_fs as f64 <= SCREEN_SHARE * complete as f64;
            want_sf = v_sf as f64 <= SCREEN_SHARE * complete as f64;
            if !want_fs && !want_sf {
                return None;
            }
        }
    }
    let d = verify_pair(av, bv, want_fs, want_sf);
    if d.n < MIN_EVALUATED {
        return None;
    }
    let bar = |viol: usize| viol == 0 || viol as f64 <= MAX_VIOLATION_SHARE * d.n as f64;
    let pass_fs = want_fs && bar(d.fs.violations);
    let pass_sf = want_sf && bar(d.sf.violations);
    let temporal = a_prep.is_temporal();
    let unit_ns = match a_prep {
        ColPrep::Ticks { unit_ns, .. } => *unit_ns,
        _ => 0,
    };
    let make = |lower: usize,
                upper: usize,
                dir: &DirDetail<T>,
                lo_min: T,
                lo_max: T,
                hi_min: T,
                hi_max: T|
     -> Candidate {
        Candidate {
            lower,
            upper,
            violations: dir.violations,
            n: d.n,
            equal: d.equal,
            derived,
            temporal,
            unit_ns,
            example_rows: dir.example_rows.clone(),
            first_row: dir.first_row,
            last_row: dir.last_row,
            worst: dir.worst.as_f64(),
            worst_ticks: dir.worst.as_ticks(),
            worst_row: dir.worst_row,
            worst_lower: dir.worst_lower.as_f64(),
            worst_upper: dir.worst_upper.as_f64(),
            worst_lower_ticks: dir.worst_lower.as_ticks(),
            worst_upper_ticks: dir.worst_upper.as_ticks(),
            min_slack: dir.min_slack.map(|s| s.as_f64()),
            min_slack_ticks: dir.min_slack.map(|s| s.as_ticks()),
            lo_min: lo_min.as_f64(),
            lo_max: lo_max.as_f64(),
            hi_min: hi_min.as_f64(),
            hi_max: hi_max.as_f64(),
            lo_min_ticks: lo_min.as_ticks(),
            lo_max_ticks: lo_max.as_ticks(),
            hi_min_ticks: hi_min.as_ticks(),
            hi_max_ticks: hi_max.as_ticks(),
            screened,
        }
    };
    match (pass_fs, pass_sf) {
        (true, true) => {
            if d.fs.violations == d.sf.violations {
                // Equal in both directions: the columns are equal
                // wherever both are present (zero) or cross as
                // often as they agree (nonzero). Neither states an
                // order.
                None
            } else if d.fs.violations < d.sf.violations {
                Some(make(i, j, &d.fs, d.lo_min, d.lo_max, d.hi_min, d.hi_max))
            } else {
                Some(make(j, i, &d.sf, d.hi_min, d.hi_max, d.lo_min, d.lo_max))
            }
        }
        (true, false) => Some(make(i, j, &d.fs, d.lo_min, d.lo_max, d.hi_min, d.hi_max)),
        (false, true) => Some(make(j, i, &d.sf, d.hi_min, d.hi_max, d.lo_min, d.lo_max)),
        (false, false) => None,
    }
}

fn build_finding(table: &TypedTable, c: &Candidate) -> Finding {
    let lower_name = &table.names[c.lower];
    let upper_name = &table.names[c.upper];
    let share = c.violation_share();
    // A gap (margin or overrun) in the pair's own units: a
    // formatted number for numeric pairs, a duration for temporal
    // pairs, always the formatting of a value in the payload.
    let fmt_gap = |v: f64, ticks: i128| -> String {
        if c.temporal {
            fmt_duration(ticks, c.unit_ns)
        } else {
            fmt_num(v)
        }
    };
    let (claim, interpretation) = if c.violations == 0 {
        let mut claim = format!(
            "{} <= {} on all {} evaluated rows (0 violations; the two are equal on {} of them",
            lower_name, upper_name, c.n, c.equal,
        );
        if let (Some(s), Some(st)) = (c.min_slack, c.min_slack_ticks) {
            if s > 0.0 {
                claim.push_str(&format!("; the tightest margin is {}", fmt_gap(s, st)));
            }
        }
        claim.push(')');
        let interp = if c.derived {
            "the two columns are functionally related in this data (one is a function of the other), so the order may follow from their derivation rather than from a constraint on the source; the finding is ranked below order findings between columns with no such relation, and below every order finding that reports violations".to_string()
        } else {
            format!(
                "an ordering constraint that holds exactly on every row where both columns are present: if the source schema declares {lower_name} <= {upper_name}, this confirms it on the measured rows; if it does not, a future violation of the order is a data-error signal"
            )
        };
        (claim, interp)
    } else {
        let worst = if c.temporal {
            format!(
                "the worst violation is at row {}, where {} is {} after {}",
                c.worst_row.unwrap_or(0),
                lower_name,
                fmt_gap(c.worst, c.worst_ticks),
                upper_name,
            )
        } else {
            format!(
                "the worst violation is at row {}, where {} = {} against {} = {}",
                c.worst_row.unwrap_or(0),
                lower_name,
                fmt_num(c.worst_lower),
                upper_name,
                fmt_num(c.worst_upper),
            )
        };
        let claim = format!(
            "{} <= {} on {} of {} evaluated rows ({} violations, {:.2}%): {}",
            lower_name,
            upper_name,
            c.n - c.violations,
            c.n,
            c.violations,
            100.0 * share,
            worst,
        );
        let interp = format!(
            "the order holds on {:.2}% of the rows where both columns are present, and the {} exceptions are listed in the evidence: a near-universal order with few exceptions usually means the exceptions are recording errors (an end recorded before its start is the common shape), or that the constraint is narrower than the column pair suggests",
            100.0 * (1.0 - share),
            c.violations,
        );
        (claim, interp)
    };
    let mut metrics = BTreeMap::from([
        ("lower".into(), json!(lower_name)),
        ("upper".into(), json!(upper_name)),
        ("relation".into(), json!("<=")),
        ("n".into(), json!(c.n)),
        ("population_rows".into(), json!(table.n)),
        ("null_rows".into(), json!(table.n - c.n)),
        ("violations".into(), json!(c.violations)),
        (
            "satisfied".into(),
            json!(c.n - c.violations),
        ),
        ("violation_share".into(), json!(share)),
        ("equal_rows".into(), json!(c.equal)),
        (
            "functionally_related".into(),
            json!(c.derived),
        ),
        ("row_sampled".into(), json!(false)),
        (
            "method".into(),
            json!("exact pairwise order scan: candidate pairs share a value kind (numeric with numeric, dates with dates, datetimes with datetimes of one unit), carry at least 5 distinct values each, and have compatible ranges (overlap at least 25% of the narrower range, widths within 100x); a direction is reported when its violations are zero or at most 1% of the rows where both columns are present, after screening on a deterministic row sample when the table is large and verification on all rows; at most one direction per pair, the one with fewer violations; pairs are evaluated on the shared max_pairs budget"),
        ),
    ]);
    if c.temporal {
        metrics.insert("lower_min".into(), json!(c.lo_min_ticks));
        metrics.insert("lower_max".into(), json!(c.lo_max_ticks));
        metrics.insert("upper_min".into(), json!(c.hi_min_ticks));
        metrics.insert("upper_max".into(), json!(c.hi_max_ticks));
        metrics.insert("tick_unit_ns".into(), json!(c.unit_ns));
        if c.violations > 0 {
            metrics.insert("worst_violation".into(), json!(c.worst_ticks));
            metrics.insert("worst_lower_value".into(), json!(c.worst_lower_ticks));
            metrics.insert("worst_upper_value".into(), json!(c.worst_upper_ticks));
        }
        if let Some(s) = c.min_slack_ticks {
            metrics.insert("min_slack".into(), json!(s));
        }
    } else {
        metrics.insert("lower_min".into(), json!(c.lo_min));
        metrics.insert("lower_max".into(), json!(c.lo_max));
        metrics.insert("upper_min".into(), json!(c.hi_min));
        metrics.insert("upper_max".into(), json!(c.hi_max));
        if c.violations > 0 {
            metrics.insert("worst_violation".into(), json!(c.worst));
            metrics.insert("worst_lower_value".into(), json!(c.worst_lower));
            metrics.insert("worst_upper_value".into(), json!(c.worst_upper));
        }
        if let Some(s) = c.min_slack {
            metrics.insert("min_slack".into(), json!(s));
        }
    }
    if c.violations > 0 {
        metrics.insert("example_rows".into(), json!(c.example_rows));
        metrics.insert("first_row".into(), json!(c.first_row));
        metrics.insert("last_row".into(), json!(c.last_row));
        metrics.insert("worst_violation_row".into(), json!(c.worst_row));
    }
    if c.screened {
        metrics.insert("screen_rows".into(), json!(SCREEN_ROWS));
    }
    Finding::new(
        Pattern::OrderConstraint,
        BTreeMap::from([
            ("inputs".into(), json!([lower_name])),
            ("target".into(), json!(upper_name)),
        ]),
        claim,
        EvidenceBasis::Measured,
        c.score(),
        metrics,
        Some(interpretation),
        Method::new(
            "order_constraints",
            "pairwise_order_scan",
            "exact pairwise order verification (onceover v0.1)",
        ),
    )
}

fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.4}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Format a tick difference as a duration in the largest whole
/// unit it divides by (days for date pairs via the day unit).
fn fmt_duration(ticks: i128, unit_ns: u64) -> String {
    let total_ns = ticks * unit_ns as i128;
    const UNITS: [(&str, i128); 7] = [
        ("day", 86_400_000_000_000),
        ("hour", 3_600_000_000_000),
        ("minute", 60_000_000_000),
        ("second", 1_000_000_000),
        ("millisecond", 1_000_000),
        ("microsecond", 1_000),
        ("nanosecond", 1),
    ];
    for (name, size) in UNITS {
        if total_ns >= size && total_ns % size == 0 {
            let q = total_ns / size;
            return format!("{} {}{}", q, name, if q == 1 { "" } else { "s" });
        }
    }
    format!("{} nanoseconds", total_ns)
}

#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::extract;
    use polars::prelude::*;

    fn scan(df: &DataFrame) -> OrderOutcome {
        let table = extract(df);
        order_constraints(
            &table,
            &OrderConfig {
                max_pairs: 10_000,
                derived_pairs: BTreeSet::new(),
            },
        )
    }

    fn pair_findings<'a>(out: &'a OrderOutcome, lower: &str, upper: &str) -> Vec<&'a Finding> {
        out.findings
            .iter()
            .filter(|f| {
                f.evidence[0].metrics["lower"] == json!(lower)
                    && f.evidence[0].metrics["upper"] == json!(upper)
            })
            .collect()
    }

    #[test]
    fn exact_order_fires_with_zero_violations() {
        // lo in [0, 50], hi = lo + [0, 40]: exact, and the reverse
        // direction fails outright.
        let lo: Vec<f64> = (0..2000).map(|k| (k % 500) as f64 * 0.1).collect();
        let hi: Vec<f64> = lo
            .iter()
            .enumerate()
            .map(|(k, x)| x + (k % 41) as f64)
            .collect();
        let df = df! { "lo" => lo, "hi" => hi }.unwrap();
        let out = scan(&df);
        let hits = pair_findings(&out, "lo", "hi");
        assert_eq!(
            hits.len(),
            1,
            "{:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        let m = &hits[0].evidence[0].metrics;
        assert_eq!(m["violations"], json!(0));
        assert_eq!(m["n"], json!(2000));
        assert!(hits[0]
            .claim
            .contains("lo <= hi on all 2000 evaluated rows"));
        hits[0].validate().unwrap();
    }

    #[test]
    fn few_violations_fire_with_retrievable_rows() {
        // 10 swapped rows in 2,000 (0.5%).
        let mut lo: Vec<f64> = (0..2000).map(|k| (k % 500) as f64 * 0.1).collect();
        let hi: Vec<f64> = lo
            .iter()
            .enumerate()
            .map(|(k, x)| x + (k % 41) as f64)
            .collect();
        let mut bad = Vec::new();
        for k in (0..2000).step_by(200) {
            lo[k] = hi[k] + 5.0;
            bad.push(k);
        }
        let df = df! { "lo" => lo, "hi" => hi }.unwrap();
        let out = scan(&df);
        let hits = pair_findings(&out, "lo", "hi");
        assert_eq!(hits.len(), 1);
        let m = &hits[0].evidence[0].metrics;
        assert_eq!(m["violations"], json!(10));
        let rows: Vec<usize> = serde_json::from_value(m["example_rows"].clone()).unwrap();
        assert_eq!(rows, bad[..10].to_vec());
        hits[0].validate().unwrap();
    }

    #[test]
    fn independent_columns_stay_quiet() {
        // Deterministic interleavings with no row-level relation:
        // each direction is violated on roughly half the rows.
        let a: Vec<f64> = (0..2000).map(|k| ((k * 37) % 1000) as f64).collect();
        let b: Vec<f64> = (0..2000).map(|k| ((k * 91 + 13) % 1000) as f64).collect();
        let df = df! { "a" => a, "b" => b }.unwrap();
        let out = scan(&df);
        assert!(
            out.findings.is_empty(),
            "{:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
    }

    #[test]
    fn identical_columns_state_no_order() {
        let a: Vec<f64> = (0..2000).map(|k| (k % 700) as f64 * 0.13).collect();
        let df = df! { "a" => a.clone(), "b" => a }.unwrap();
        let out = scan(&df);
        assert!(out.findings.is_empty());
    }

    #[test]
    fn range_disjoint_pair_is_a_marginal_fact_not_an_order() {
        // a in [0, 1], b in [1000, 2000]: a <= b for every pairing,
        // so the ranges never overlap and no pair is evaluated.
        let a: Vec<f64> = (0..2000).map(|k| (k % 100) as f64 * 0.01).collect();
        let b: Vec<f64> = (0..2000).map(|k| 1000.0 + (k % 1000) as f64).collect();
        let df = df! { "a" => a, "b" => b }.unwrap();
        let out = scan(&df);
        assert_eq!(out.pairs_total, 0);
        assert!(out.findings.is_empty());
    }

    #[test]
    fn derived_pair_scores_below_unrelated_exact_and_violations() {
        // base -> derived = 2*base + 3 (functionally related);
        // lo <= hi exact and unrelated; end >= start with 0.5%
        // violations. Expected report order: violations, exact
        // unrelated, exact derived.
        let base: Vec<f64> = (0..2000).map(|k| 1.0 + (k % 400) as f64 * 0.1).collect();
        let derived: Vec<f64> = base.iter().map(|x| 2.0 * x + 3.0).collect();
        let lo: Vec<f64> = (0..2000).map(|k| (k % 500) as f64 * 0.1).collect();
        let hi: Vec<f64> = lo
            .iter()
            .enumerate()
            .map(|(k, x)| x + (k % 41) as f64)
            .collect();
        let mut lo2 = lo.clone();
        let hi2 = hi.clone();
        for k in (0..2000).step_by(200) {
            lo2[k] = hi2[k] + 5.0;
        }
        let df = df! {
            "base" => base, "derived" => derived,
            "lo" => lo, "hi" => hi, "lo2" => lo2, "hi2" => hi2
        }
        .unwrap();
        let table = extract(&df);
        let names: Vec<&str> = table.names.iter().map(|s| s.as_str()).collect();
        let idx = |n: &str| names.iter().position(|&x| x == n).unwrap();
        let out = order_constraints(
            &table,
            &OrderConfig {
                max_pairs: 10_000,
                derived_pairs: BTreeSet::from([(
                    idx("base").min(idx("derived")),
                    idx("base").max(idx("derived")),
                )]),
            },
        );
        let order: Vec<(&str, &str)> = out
            .findings
            .iter()
            .map(|f| {
                (
                    f.evidence[0].metrics["lower"].as_str().unwrap(),
                    f.evidence[0].metrics["upper"].as_str().unwrap(),
                )
            })
            .collect();
        let pos = |l: &str, u: &str| order.iter().position(|&x| x == (l, u)).unwrap();
        assert!(pos("lo2", "hi2") < pos("lo", "hi"), "{order:?}");
        assert!(pos("lo", "hi") < pos("base", "derived"), "{order:?}");
        for f in &out.findings {
            f.validate().unwrap();
        }
    }

    #[test]
    fn pair_budget_bounds_evaluation_and_is_disclosed_in_counts() {
        let lo: Vec<f64> = (0..500).map(|k| (k % 250) as f64 * 0.2).collect();
        let hi: Vec<f64> = lo.iter().map(|x| x + 1.0).collect();
        let mid: Vec<f64> = lo.iter().map(|x| x + 0.5).collect();
        let df = df! { "lo" => lo, "mid" => mid, "hi" => hi }.unwrap();
        let table = extract(&df);
        let out = order_constraints(
            &table,
            &OrderConfig {
                max_pairs: 1,
                derived_pairs: BTreeSet::new(),
            },
        );
        assert_eq!(out.pairs_evaluated, 1);
        assert!(out.pairs_total >= out.pairs_evaluated);
    }

    #[test]
    fn datetime_pair_with_violations_fires() {
        // start/end as microsecond datetimes; 10 of 2,000 ends
        // planted before their starts.
        let start: Vec<i64> = (0..2000)
            .map(|k| 1_704_067_200_000_000i64 + (k as i64) * 3_600_000_000)
            .collect();
        let mut end: Vec<i64> = start
            .iter()
            .enumerate()
            .map(|(k, s)| s + ((k % 48) as i64 + 1) * 3_600_000_000)
            .collect();
        for k in (0..2000).step_by(200) {
            end[k] = start[k] - 7_200_000_000;
        }
        let s = Series::new("start".into(), start)
            .cast(&DataType::Datetime(TimeUnit::Microseconds, None))
            .unwrap();
        let e = Series::new("end".into(), end)
            .cast(&DataType::Datetime(TimeUnit::Microseconds, None))
            .unwrap();
        let df = DataFrame::new(s.len(), vec![s.into(), e.into()]).unwrap();
        let out = scan(&df);
        let hits = pair_findings(&out, "start", "end");
        assert_eq!(
            hits.len(),
            1,
            "{:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        let m = &hits[0].evidence[0].metrics;
        assert_eq!(m["violations"], json!(10));
        assert!(hits[0].claim.contains("2 hours after"), "{}", hits[0].claim);
        hits[0].validate().unwrap();
    }
}
