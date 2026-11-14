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
//! A direction must also beat random pairing. Pairing the two columns
//! at random gives a violation count computed from their sorted values;
//! the direction is reported only when that count is at least 20 and at
//! least 4 times the observed violations. A heavy-tailed column set against
//! a positive one has a compatible range and almost no crossings, whichever
//! way the rows are paired, and this rule keeps it out.
//!
//! The complementary statement covers the pair whose order almost
//! holds and fails in bulk: an accepted count that exceeds the
//! generated count it is bounded by on a quarter of the rows is
//! silent under the certification bar alone, in both directions.
//! When one direction's violations exceed the certification share
//! but are themselves systematic — at least 20 rows and at least 5%
//! of the evaluated rows, while the reverse direction's violations
//! are at least twice as many — the pair is reported as a systematic
//! violation of the less-violated direction, with the violating
//! rows in the evidence. The statement also requires the pair to
//! share a scale: the median row distance |a − b| / max(|a|, |b|)
//! over sampled complete rows is at most 0.2, and the columns'
//! medians agree within a factor of 2.5 when both are non-zero.
//! A bound pair (a subset count against its total, a part against
//! its whole) tracks its diagonal; an age in years against a
//! duration in seconds crosses asymmetrically too, but from far
//! off the diagonal, and stays silent. Neither direction holds
//! there, and the
//! failures are too one-sided to read as chance: two unrelated
//! columns crossing in both directions at similar rates fail the
//! asymmetry requirement and stay silent, as do one-sided
//! violations below the share floor.
//!
//! The second complementary statement covers the pair the scale
//! gate itself excludes: a part against its whole. A part sits
//! far from its total by construction — a quarter-share part has
//! a median row distance near 0.75 — so the gate can never pass,
//! and a part corrupted to exceed its total in bulk is stated by
//! no pass: the identity scan states a decomposition only when
//! it holds on at least 99.9% of rows, which bulk corruption
//! breaks. When a direction's violations number at least 20 and
//! between 5% and 25% of the evaluated rows, and one column is a
//! component of the other, the direction is reported as a
//! part-against-whole violation in the same statement shape,
//! without the scale gate. The identity route needs no asymmetry
//! requirement — the decomposition is the evidence the pair is
//! a bound. The containment route keeps it: two unrelated
//! non-negative columns share a zero floor and touch their
//! diagonal by accident, and cross at comparable rates in both
//! directions, so a containment pair is reported only when the
//! reverse direction's violations are at least twice the stated
//! direction's; for a genuine component the whole exceeding the
//! part on the remaining rows is what a bound looks like, not
//! a competing order. The relation
//! is established two ways (see `part_whole_relation`). By
//! identity: the whole minus the part equals a sum of one to
//! three other columns, within the identity tolerance, on at
//! least 99.9% of the rows where the order holds, with the rest
//! non-negative on at least 99% of the evaluated rows — judged
//! on the held rows because the corrupted rows are the ones
//! where the decomposition breaks. By containment: both columns
//! respect the shared zero floor on at least 99% of the
//! evaluated rows, and the slack reaches the floor (within 0.1%
//! of the whole's range) on at least 5% of the held rows where
//! the whole is above it, but on fewer than half of them — the
//! whole is sometimes just the part, which shared zeros alone
//! cannot show, and columns equal on half or more of the held
//! rows are two recordings of one quantity. An age in years against
//! a duration in seconds meets neither test: no decomposition of
//! its slack exists, and the slack never reaches the floor.
//! Where the decomposition verifies on at least 99.9% of all
//! evaluated rows the identity scan states it, and this
//! statement stays silent for the pair: the same failure is not
//! stated twice.
//!
//! Every eligible pair is screened on the row sample, up to 64 times
//! the shared pair budget, nearest columns first. Pairs that stay
//! near-universal on the sample are verified on all rows, up to
//! `max_pairs` of them (the budget that also caps the dependence screen),
//! and coverage says how many were screened and verified. A pair whose columns are
//! functionally related in this run's exact functional
//! dependencies — one is a function of the other, so any order
//! between them may follow from their derivation — is reported
//! with that fact in its evidence and scored below order findings
//! between unrelated columns, and every violation finding scores
//! above every exact one: the exceptions to a near-universal
//! order are the data-error signal, an always-true order between
//! derived columns is the weakest statement this pass makes.

use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use rayon::prelude::*;
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
/// The systematic-violation statement: a direction that fails the
/// certification share is still reported when its violations reach
/// at least this many rows and at least MIN_SYSTEMATIC_SHARE of the
/// evaluated rows, while the reverse direction's violations are at
/// least SYSTEMATIC_ASYMMETRY times its own.
const MIN_SYSTEMATIC_VIOLATIONS: usize = 20;
const MIN_SYSTEMATIC_SHARE: f64 = 0.05;
const SYSTEMATIC_ASYMMETRY: f64 = 2.0;
/// Screening admission for the systematic statement: the better
/// direction's sample violation share must not pass this bound, so
/// pairs that sit closer to a reverse order than to a broken one
/// are not verified for the statement.
const SCREEN_SYSTEMATIC_MAX_SHARE: f64 = 0.25;
/// The systematic statement's scale gate: the two columns must
/// track their diagonal — the median over sampled complete rows
/// of |a − b| / max(|a|, |b|) is at most this. A bound pair (a
/// subset count against its total, a part against its whole)
/// sits near the diagonal; two columns on unrelated scales cross
/// asymmetrically too, but from far away.
const MAX_PAIR_DISTANCE: f64 = 0.2;
/// The columns' medians must agree within this factor when both
/// are non-zero: the same-scale half of the gate above.
const MAX_MEDIAN_RATIO: f64 = 2.5;
/// The identity tolerance of the identity scans, shared with the
/// part/whole component test: |y − predicted| ≤ 1e-9 · (1 + |y|).
const IDENTITY_TOL: f64 = 1e-9;
/// A decomposition of the whole into the part plus a rest must
/// match on at least this share of the rows it is judged on —
/// the identity scans' own acceptance bar (their fixed-form scan
/// requires the full-data fraction to reach 0.999).
const IDENTITY_MATCH_SHARE: f64 = 0.999;
/// The rest of a decomposition is the whole's other parts, so it
/// is non-negative on at least this share of the evaluated rows;
/// the identity scans' components-first chase admits candidate
/// parts by the same bound read the other way (a part lies
/// between zero and its target on at least 99% of rows).
const REST_NONNEG_SHARE: f64 = 0.99;
/// Smallest number of rows where a candidate rest and the slack
/// are both present before the rest is judged (the chase's own
/// minimum, read on the held rows here).
const MIN_REST_ROWS: usize = 50;
/// Candidate rests for a decomposition, largest mean share of
/// the slack first; the chase caps its candidate pool the same
/// way, at 24.
const REST_POOL_MAX: usize = 12;
/// A decomposition's rest has at most this many columns: totals
/// of up to four parts are covered.
const REST_MAX_TERMS: usize = 3;
/// The floor half of the containment test: a part and its whole
/// both respect the shared zero floor on at least this share of
/// the evaluated rows (the chase's part-bound share).
const FLOOR_SHARE: f64 = 0.99;
/// Containment route: on held rows where the whole is above the
/// floor, the slack (whole − part) must come within this share
/// of the whole's range of zero — the size of rounding in
/// reported totals, as the near-identity test measures it — on
/// at least SLACK_CONTACT_SHARE of those rows: the whole is
/// sometimes just the part.
const SLACK_CONTACT_REL_SHARE: f64 = 1e-3;
const SLACK_CONTACT_SHARE: f64 = 0.05;
/// The contact share stays below this: where the two columns
/// are equal on half or more of the held rows, the pair reads
/// as two recordings of one quantity, not a part and its whole.
const MAX_SLACK_CONTACT_SHARE: f64 = 0.5;
/// Rows sampled for the screening stage when the table has more
/// rows than this; survivors are verified on all rows.
const SCREEN_ROWS: usize = 4096;
/// Pairs screened on the sample, as a multiple of the pair budget.
const SCREEN_PAIRS_PER_BUDGET: usize = 64;
/// Below this many complete sampled rows the screen says nothing,
/// and both directions go on to verification.
const MIN_SCREEN_COMPLETE: usize = 30;
/// Screening survival bar: twice the reporting bar, so sampling
/// noise at the reporting bar cannot screen out a true pair.
const SCREEN_SHARE: f64 = 0.02;
/// Compatible scales: the two ranges' widths within this ratio,
/// and their overlap at least MIN_OVERLAP_SHARE of the narrower
/// range.
const MAX_WIDTH_RATIO: f64 = 100.0;
const MIN_OVERLAP_SHARE: f64 = 0.25;
/// A direction must beat random pairing: shuffling one column against the
/// other would have to produce at least this many violations, and at least
/// CHANCE_MARGIN times the violations observed. Two independent columns whose
/// values rarely cross satisfy a near-universal order by their marginals alone.
const MIN_CHANCE_VIOLATIONS: f64 = 20.0;
const CHANCE_MARGIN: f64 = 4.0;
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
    /// The shared pair budget: at most this many pairs are
    /// verified on all rows, nearest columns first.
    pub max_pairs: usize,
    /// Unordered column-index pairs (lo, hi) between which an
    /// exact single-column functional dependency holds in either
    /// direction in this run — the derived-from-one-another pairs.
    pub derived_pairs: BTreeSet<(usize, usize)>,
    /// Columns left out of every pair: row counters, labels and calendar
    /// parts, whose order against other columns says nothing about the data.
    pub excluded: BTreeSet<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct OrderOutcome {
    pub findings: Vec<Finding>,
    /// Eligible pairs (same kind, per-column bars passed, ranges
    /// compatible) before the pair budget.
    pub pairs_total: usize,
    /// Pairs screened on the row sample, or on all rows when the
    /// table is smaller than the sample.
    pub pairs_screened: usize,
    /// Pairs that stayed near-universal on the screen.
    pub pairs_surviving: usize,
    /// Surviving pairs verified on all rows, budget permitting.
    pub pairs_evaluated: usize,
    /// Directions that passed a reporting bar before the
    /// findings cap (at most one per pair): the certification bar
    /// or the systematic-violation rule.
    pub pairs_passing: usize,
    /// At least one pair was screened on the row sample.
    pub screened: bool,
    /// Numeric and temporal columns seen (the pass's universe).
    pub columns_seen: usize,
}

/// A column prepared for pair comparison: values in the exact
/// comparison domain of its kind, plus the marginal statistics
/// the candidate guards read. `distinct` counts up to MIN_DISTINCT.
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
    /// Whether every value is exactly representable in f64: all
    /// float columns, and integer columns within the exact limit.
    /// The part/whole component test works in f64 and requires
    /// this of both columns.
    fn f64_exact(&self) -> bool {
        match self {
            ColPrep::Floats { .. } => true,
            ColPrep::Ints { f64_exact, .. } => *f64_exact,
            ColPrep::Ticks { .. } => false,
        }
    }
    /// The same column with only the given rows' values; the marginal
    /// statistics stay those of the whole column.
    fn restricted(self, rows: &[u32]) -> ColPrep {
        fn pick<T: Copy>(vals: &[Option<T>], rows: &[u32]) -> Vec<Option<T>> {
            rows.iter().map(|&r| vals[r as usize]).collect()
        }
        match self {
            ColPrep::Ints {
                vals,
                min,
                max,
                distinct,
                nonnull,
                f64_exact,
            } => ColPrep::Ints {
                vals: pick(&vals, rows),
                min,
                max,
                distinct,
                nonnull,
                f64_exact,
            },
            ColPrep::Floats {
                vals,
                min,
                max,
                distinct,
                nonnull,
            } => ColPrep::Floats {
                vals: pick(&vals, rows),
                min,
                max,
                distinct,
                nonnull,
            },
            ColPrep::Ticks {
                vals,
                unit_ns,
                min,
                max,
                distinct,
                nonnull,
            } => ColPrep::Ticks {
                vals: pick(&vals, rows),
                unit_ns,
                min,
                max,
                distinct,
                nonnull,
            },
        }
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
                        if seen.len() < MIN_DISTINCT {
                            seen.insert(x);
                        }
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
                        if seen.len() < MIN_DISTINCT {
                            seen.insert(x);
                        }
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
                        if seen.len() < MIN_DISTINCT {
                            seen.insert(x.to_bits());
                        }
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
                        if seen.len() < MIN_DISTINCT {
                            seen.insert(x);
                        }
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
                        if seen.len() < MIN_DISTINCT {
                            seen.insert(x);
                        }
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

/// Violation counts for both directions, and how many rows had both
/// values present.
fn count_directions<T: ScanVal>(a: &[Option<T>], b: &[Option<T>]) -> (usize, usize, usize) {
    let (mut viol_ab, mut viol_ba, mut complete) = (0usize, 0usize, 0usize);
    for (x, y) in a.iter().zip(b) {
        if let (Some(x), Some(y)) = (x, y) {
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

/// The systematic statement's scale gate, computed on a
/// deterministic stride sample of the rows where both columns are
/// present: the median of |a − b| / max(|a|, |b|) (0 where both
/// values are 0), and whether the two columns' medians are
/// comparable — within [`MAX_MEDIAN_RATIO`] of each other, a test
/// skipped when either median is zero.
fn pair_closeness<T: ScanVal>(a: &[Option<T>], b: &[Option<T>]) -> (f64, bool) {
    let step = (a.len() / 1024).max(1);
    let mut dist: Vec<f64> = Vec::new();
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for k in (0..a.len()).step_by(step) {
        if let (Some(x), Some(y)) = (a[k], b[k]) {
            let (x, y) = (x.as_f64(), y.as_f64());
            xs.push(x);
            ys.push(y);
            let m = x.abs().max(y.abs());
            dist.push(if m == 0.0 { 0.0 } else { (x - y).abs() / m });
        }
    }
    if dist.is_empty() {
        return (1.0, false);
    }
    let median = |v: &mut Vec<f64>| {
        v.sort_by(|p, q| p.partial_cmp(q).unwrap_or(std::cmp::Ordering::Equal));
        v[v.len() / 2]
    };
    let d = median(&mut dist);
    let (mx, my) = (median(&mut xs), median(&mut ys));
    let comparable = mx == 0.0 || my == 0.0 || mx.max(my) / mx.min(my) <= MAX_MEDIAN_RATIO;
    (d, comparable)
}

/// Violations of `lower <= upper` that random pairing would give on the
/// rows where both are present: the share of (lower, upper) value pairs out
/// of order, times the rows.
fn chance_violations<T: ScanVal>(lower: &[Option<T>], upper: &[Option<T>]) -> f64 {
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for (x, y) in lower.iter().zip(upper) {
        if let (Some(x), Some(y)) = (x, y) {
            xs.push(*x);
            ys.push(*y);
        }
    }
    let by_value = |p: &T, q: &T| p.partial_cmp(q).unwrap_or(std::cmp::Ordering::Equal);
    xs.sort_by(by_value);
    ys.sort_by(by_value);
    let (mut below, mut crossed) = (0usize, 0u128);
    for x in &xs {
        while below < ys.len() && ys[below] < *x {
            below += 1;
        }
        crossed += below as u128;
    }
    crossed as f64 / xs.len().max(1) as f64
}

/// Whether `violations` is far fewer than random pairing would give.
fn beats_chance(violations: usize, chance: f64) -> bool {
    chance >= MIN_CHANCE_VIOLATIONS && chance >= CHANCE_MARGIN * violations as f64
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
    /// Violations random pairing would give.
    chance: f64,
    /// The statement is the systematic-violation form: the
    /// direction failed certification but its violations are
    /// numerous, substantial, and one-sided.
    systematic: bool,
    /// The statement is the part/whole variant of the systematic
    /// form: the scale gate was replaced by component evidence.
    part_whole: bool,
    /// How the component relation was established: `"identity"`
    /// (the slack decomposes exactly over the rows where the
    /// order holds) or `"containment"` (the pair respects the
    /// shared zero floor and the slack reaches it).
    part_whole_basis: Option<&'static str>,
    /// The pair's median row distance |a − b| / max(|a|, |b|) on
    /// sampled complete rows — the scale gate's measurement,
    /// recorded as evidence on systematic findings.
    pair_distance: f64,
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
    /// Violations of the reverse direction: every evaluated row
    /// violates one direction, violates the other, or ties.
    fn reverse_violations(&self) -> usize {
        self.n - self.equal - self.violations
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
    let sampled = table.n > SCREEN_ROWS;
    let rows: Vec<u32> = if sampled {
        onceover_core::sample_rows(table.n, SCREEN_ROWS)
    } else {
        (0..table.n as u32).collect()
    };
    out.screened = sampled;
    // Whole columns are only needed for pairs that survive the screen, so
    // only the sampled values are kept here.
    let previews: Vec<Option<ColPrep>> = table
        .cols
        .par_iter()
        .map(|col| prep_column(col).map(|p| p.restricted(&rows)))
        .collect();
    out.columns_seen = previews.iter().filter(|p| p.is_some()).count();
    let eligible_cols: Vec<usize> = (0..table.cols.len())
        .filter(|i| !cfg.excluded.contains(i))
        .filter(|&i| {
            previews[i]
                .as_ref()
                .is_some_and(|p| p.distinct() >= MIN_DISTINCT && p.nonnull() >= MIN_EVALUATED)
        })
        .collect();
    // A pair is eligible when the two columns share an exact comparison
    // domain and their ranges are compatible. Nearest columns come first:
    // columns that bound each other tend to sit together, and this is the
    // order in which the screen budget is spent.
    let screen_budget = cfg.max_pairs.saturating_mul(SCREEN_PAIRS_PER_BUDGET);
    let mut to_screen: Vec<(usize, usize, bool)> = Vec::new();
    for distance in 1..eligible_cols.len() {
        for x in 0..eligible_cols.len() - distance {
            let (i, j) = (eligible_cols[x], eligible_cols[x + distance]);
            let (a, b) = (previews[i].as_ref().unwrap(), previews[j].as_ref().unwrap());
            let Some(as_float) = pair_domain(a, b) else {
                continue;
            };
            if ranges_compatible(a, b) {
                out.pairs_total += 1;
                if to_screen.len() < screen_budget {
                    to_screen.push((i, j, as_float));
                }
            }
        }
    }
    out.pairs_screened = to_screen.len();

    let survivors: Vec<(usize, usize, bool, bool, bool)> = to_screen
        .par_iter()
        .filter_map(|&(i, j, as_float)| {
            let (a, b) = (previews[i].as_ref()?, previews[j].as_ref()?);
            let (want_fs, want_sf) = screen_pair(a, b, as_float)?;
            Some((i, j, as_float, want_fs, want_sf))
        })
        .collect();
    drop(previews);
    out.pairs_surviving = survivors.len();
    let verified = &survivors[..survivors.len().min(cfg.max_pairs)];
    out.pairs_evaluated = verified.len();

    let columns: BTreeSet<usize> = verified.iter().flat_map(|p| [p.0, p.1]).collect();
    let whole: BTreeMap<usize, ColPrep> = columns
        .into_par_iter()
        .filter_map(|c| prep_column(&table.cols[c]).map(|p| (c, p)))
        .collect();
    let mut candidates: Vec<Candidate> = verified
        .par_iter()
        .filter_map(|&(i, j, as_float, want_fs, want_sf)| {
            let derived = cfg.derived_pairs.contains(&(i.min(j), i.max(j)));
            let (a, b) = (whole.get(&i)?, whole.get(&j)?);
            let want = (want_fs, want_sf);
            evaluate_pair(
                a,
                b,
                i,
                j,
                as_float,
                derived,
                want,
                sampled,
                table,
                &cfg.excluded,
            )
        })
        .collect();

    out.pairs_passing = candidates.len();
    candidates.sort_by(|p, q| p.rank_key().partial_cmp(&q.rank_key()).unwrap());
    for c in candidates.into_iter().take(MAX_FINDINGS) {
        out.findings.push(build_finding(table, &c));
    }
    out
}

/// The directions of a pair still near-universal on the screening rows.
fn screen_pair(a: &ColPrep, b: &ColPrep, as_float: bool) -> Option<(bool, bool)> {
    let counts = match (a, b, as_float) {
        (ColPrep::Floats { vals: av, .. }, ColPrep::Floats { vals: bv, .. }, true) => {
            count_directions(av, bv)
        }
        (ColPrep::Ints { vals: av, .. }, ColPrep::Floats { vals: bv, .. }, true) => {
            count_directions(&as_floats(av), bv)
        }
        (ColPrep::Floats { vals: av, .. }, ColPrep::Ints { vals: bv, .. }, true) => {
            count_directions(av, &as_floats(bv))
        }
        (ColPrep::Ints { vals: av, .. }, ColPrep::Ints { vals: bv, .. }, false)
        | (ColPrep::Ticks { vals: av, .. }, ColPrep::Ticks { vals: bv, .. }, false) => {
            count_directions(av, bv)
        }
        _ => return None,
    };
    let (v_fs, v_sf, complete) = counts;
    if complete < MIN_SCREEN_COMPLETE {
        return Some((true, true));
    }
    let want_fs = v_fs as f64 <= SCREEN_SHARE * complete as f64;
    let want_sf = v_sf as f64 <= SCREEN_SHARE * complete as f64;
    if want_fs || want_sf {
        return Some((want_fs, want_sf));
    }
    // Systematic-violation admission: neither direction is
    // near-universal on the sample, but the better direction fails
    // on a substantial share while the worse fails at least twice
    // as often — the shape of an order that holds in one direction
    // and breaks in bulk. Verify the better direction on all rows;
    // the full-row rule in `dispatch` decides the statement.
    let (lo, hi) = (v_fs.min(v_sf), v_fs.max(v_sf));
    let lo_share = lo as f64 / complete as f64;
    if lo_share > SCREEN_SHARE
        && lo_share <= SCREEN_SYSTEMATIC_MAX_SHARE
        && (hi as f64) >= SYSTEMATIC_ASYMMETRY * lo as f64
    {
        return Some((v_fs <= v_sf, v_sf < v_fs));
    }
    // Part/whole admission: a component pair can fail the
    // asymmetry term — the whole is often just the part, so the
    // reverse direction rarely violates — so a pair in the same
    // share band whose columns respect the shared zero floor on
    // the sample is also verified. The full-row component test
    // in `dispatch` decides the statement.
    if lo_share > SCREEN_SHARE
        && lo_share <= SCREEN_SYSTEMATIC_MAX_SHARE
        && floor_respected(a, b, as_float)
    {
        return Some((v_fs <= v_sf, v_sf < v_fs));
    }
    None
}

/// Whether both columns respect the shared zero floor on at
/// least FLOOR_SHARE of the sample rows where both are present,
/// each row's tolerance taken from the other column's magnitude
/// (the identity tolerance): the screening half of the
/// part/whole component test.
fn floor_respected(a: &ColPrep, b: &ColPrep, as_float: bool) -> bool {
    fn check<T: ScanVal>(av: &[Option<T>], bv: &[Option<T>]) -> bool {
        let (mut n, mut ok) = (0usize, 0usize);
        for (x, y) in av.iter().zip(bv) {
            if let (Some(x), Some(y)) = (x, y) {
                let (x, y) = (x.as_f64(), y.as_f64());
                n += 1;
                if x >= -IDENTITY_TOL * (1.0 + y.abs()) && y >= -IDENTITY_TOL * (1.0 + x.abs()) {
                    ok += 1;
                }
            }
        }
        n > 0 && ok as f64 >= FLOOR_SHARE * n as f64
    }
    match (a, b, as_float) {
        (ColPrep::Floats { vals: av, .. }, ColPrep::Floats { vals: bv, .. }, true) => check(av, bv),
        (ColPrep::Ints { vals: av, .. }, ColPrep::Floats { vals: bv, .. }, true) => {
            check(&as_floats(av), bv)
        }
        (ColPrep::Floats { vals: av, .. }, ColPrep::Ints { vals: bv, .. }, true) => {
            check(av, &as_floats(bv))
        }
        (ColPrep::Ints { vals: av, .. }, ColPrep::Ints { vals: bv, .. }, false)
        | (ColPrep::Ticks { vals: av, .. }, ColPrep::Ticks { vals: bv, .. }, false) => {
            check(av, bv)
        }
        _ => false,
    }
}

fn as_floats(vals: &[Option<i128>]) -> Vec<Option<f64>> {
    vals.iter().map(|v| v.map(|x| x as f64)).collect()
}

/// Verify one screened pair on all rows; return the reportable direction,
/// if any.
#[allow(clippy::too_many_arguments)]
fn evaluate_pair(
    a: &ColPrep,
    b: &ColPrep,
    i: usize,
    j: usize,
    as_float: bool,
    derived: bool,
    want: (bool, bool),
    screened: bool,
    table: &TypedTable,
    excluded: &BTreeSet<usize>,
) -> Option<Candidate> {
    let exact_f64 = a.f64_exact() && b.f64_exact();
    match (a, b, as_float) {
        (ColPrep::Floats { vals: av, .. }, ColPrep::Floats { vals: bv, .. }, true) => dispatch(
            av, bv, i, j, a, derived, want, screened, table, excluded, exact_f64,
        ),
        (ColPrep::Ints { vals: av, .. }, ColPrep::Floats { vals: bv, .. }, true) => dispatch(
            &as_floats(av),
            bv,
            i,
            j,
            a,
            derived,
            want,
            screened,
            table,
            excluded,
            exact_f64,
        ),
        (ColPrep::Floats { vals: av, .. }, ColPrep::Ints { vals: bv, .. }, true) => dispatch(
            av,
            &as_floats(bv),
            i,
            j,
            a,
            derived,
            want,
            screened,
            table,
            excluded,
            exact_f64,
        ),
        (ColPrep::Ints { vals: av, .. }, ColPrep::Ints { vals: bv, .. }, false)
        | (ColPrep::Ticks { vals: av, .. }, ColPrep::Ticks { vals: bv, .. }, false) => dispatch(
            av, bv, i, j, a, derived, want, screened, table, excluded, exact_f64,
        ),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch<T: ScanVal>(
    av: &[Option<T>],
    bv: &[Option<T>],
    i: usize,
    j: usize,
    a_prep: &ColPrep,
    derived: bool,
    (want_fs, want_sf): (bool, bool),
    screened: bool,
    table: &TypedTable,
    excluded: &BTreeSet<usize>,
    exact_f64: bool,
) -> Option<Candidate> {
    let d = verify_pair(av, bv, want_fs, want_sf);
    if d.n < MIN_EVALUATED {
        return None;
    }
    let bar = |viol: usize| viol == 0 || viol as f64 <= MAX_VIOLATION_SHARE * d.n as f64;
    let chance_fs = if want_fs && bar(d.fs.violations) {
        chance_violations(av, bv)
    } else {
        0.0
    };
    let chance_sf = if want_sf && bar(d.sf.violations) {
        chance_violations(bv, av)
    } else {
        0.0
    };
    let pass_fs = beats_chance(d.fs.violations, chance_fs);
    let pass_sf = beats_chance(d.sf.violations, chance_sf);
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
                hi_max: T,
                chance: f64,
                systematic: bool|
     -> Candidate {
        Candidate {
            chance,
            systematic,
            part_whole: false,
            part_whole_basis: None,
            pair_distance: 0.0,
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
                Some(make(
                    i, j, &d.fs, d.lo_min, d.lo_max, d.hi_min, d.hi_max, chance_fs, false,
                ))
            } else {
                Some(make(
                    j, i, &d.sf, d.hi_min, d.hi_max, d.lo_min, d.lo_max, chance_sf, false,
                ))
            }
        }
        (true, false) => Some(make(
            i, j, &d.fs, d.lo_min, d.lo_max, d.hi_min, d.hi_max, chance_fs, false,
        )),
        (false, true) => Some(make(
            j, i, &d.sf, d.hi_min, d.hi_max, d.lo_min, d.lo_max, chance_sf, false,
        )),
        (false, false) => {
            // Neither direction certifies. The complementary
            // statement: a wanted direction whose violations are
            // numerous and substantial while the reverse direction's
            // are at least twice as many, on a pair whose columns
            // share a scale and track their diagonal — the shape of
            // a bound that held by construction (a subset count
            // against its total) and is broken in bulk. The reverse
            // count is exact even for an unwanted direction — every
            // evaluated row violates one direction, violates the
            // other, or ties. The scale gate keeps the statement
            // off pairs that merely overlap numerically: an age in
            // years against a duration in seconds also crosses
            // asymmetrically, but from far off the diagonal.
            let (pair_distance, comparable) = pair_closeness(av, bv);
            let systematic = |viol: usize| {
                let other = d.n - d.equal - viol;
                viol >= MIN_SYSTEMATIC_VIOLATIONS
                    && (viol as f64) >= MIN_SYSTEMATIC_SHARE * d.n as f64
                    && (other as f64) >= SYSTEMATIC_ASYMMETRY * viol as f64
                    && comparable
                    && pair_distance <= MAX_PAIR_DISTANCE
            };
            if want_fs && systematic(d.fs.violations) {
                let mut c = make(
                    i,
                    j,
                    &d.fs,
                    d.lo_min,
                    d.lo_max,
                    d.hi_min,
                    d.hi_max,
                    chance_violations(av, bv),
                    true,
                );
                c.pair_distance = pair_distance;
                Some(c)
            } else if want_sf && systematic(d.sf.violations) {
                let mut c = make(
                    j,
                    i,
                    &d.sf,
                    d.hi_min,
                    d.hi_max,
                    d.lo_min,
                    d.lo_max,
                    chance_violations(bv, av),
                    true,
                );
                c.pair_distance = pair_distance;
                Some(c)
            } else if exact_f64 && !temporal {
                // The part/whole variant: the same count and
                // share bars as the systematic rule, with the
                // share also capped at the screening admission
                // bound (past it the part is above the whole as
                // often as below it), and the scale gate
                // replaced by the component test. The asymmetry
                // term is kept for a component established by
                // containment — two unrelated columns that
                // share a zero floor cross at comparable rates
                // in both directions, which is not a broken
                // bound — while a component established by
                // identity needs no such term: the decomposition
                // itself certifies the pair. Temporal pairs are
                // not components of anything, and the test works
                // in f64, so both columns must be exactly
                // representable there.
                let variant = |viol: usize| {
                    viol >= MIN_SYSTEMATIC_VIOLATIONS
                        && (viol as f64) >= MIN_SYSTEMATIC_SHARE * d.n as f64
                        && (viol as f64) <= SCREEN_SYSTEMATIC_MAX_SHARE * d.n as f64
                };
                // A component verdict admits the statement,
                // except a containment verdict on a pair whose
                // reverse direction fails comparably often.
                // The reverse count is exact, as in the
                // systematic rule: every evaluated row violates
                // one direction, violates the other, or ties.
                let component = |viol: usize, verdict: PartWhole| match verdict {
                    PartWhole::Component(basis) if basis != "containment" => Some(basis),
                    PartWhole::Component(basis) => {
                        let other = d.n - d.equal - viol;
                        ((other as f64) >= SYSTEMATIC_ASYMMETRY * viol as f64).then_some(basis)
                    }
                    _ => None,
                };
                if want_fs && variant(d.fs.violations) {
                    if let Some(basis) = component(
                        d.fs.violations,
                        part_whole_relation(av, bv, table, i, j, excluded),
                    ) {
                        let mut c = make(
                            i,
                            j,
                            &d.fs,
                            d.lo_min,
                            d.lo_max,
                            d.hi_min,
                            d.hi_max,
                            chance_violations(av, bv),
                            true,
                        );
                        c.pair_distance = pair_distance;
                        c.part_whole = true;
                        c.part_whole_basis = Some(basis);
                        return Some(c);
                    }
                }
                if want_sf && variant(d.sf.violations) {
                    if let Some(basis) = component(
                        d.sf.violations,
                        part_whole_relation(bv, av, table, j, i, excluded),
                    ) {
                        let mut c = make(
                            j,
                            i,
                            &d.sf,
                            d.hi_min,
                            d.hi_max,
                            d.lo_min,
                            d.lo_max,
                            chance_violations(bv, av),
                            true,
                        );
                        c.pair_distance = pair_distance;
                        c.part_whole = true;
                        c.part_whole_basis = Some(basis);
                        return Some(c);
                    }
                }
                None
            } else {
                None
            }
        }
    }
}

/// How an ordered pair stands as part and whole, judged on the
/// evaluated rows of `part <= whole` (see `part_whole_relation`).
enum PartWhole {
    /// No component relation: the pair keeps the gated behaviour.
    None,
    /// A component relation holds and no exact identity states
    /// it: the part/whole variant may state the exceedances. The
    /// payload is the route that established it.
    Component(&'static str),
    /// A component relation holds and an exact sum identity
    /// states it at the identity scans' own bar: that statement
    /// is the one that fires; the variant stays silent.
    IdentityCovered,
}

/// Judge whether `part` is a component of `whole`.
///
/// By identity: the whole minus the part — the slack — equals a
/// sum of one to three other columns (unit coefficients, as in
/// the identity scans' fixed-form scan), within the identity
/// tolerance, on at least [`IDENTITY_MATCH_SHARE`] of the rows
/// where the order holds and all the decomposition's columns
/// are present, with the rest non-negative on at least
/// [`REST_NONNEG_SHARE`] of the evaluated rows. The judgement is
/// made on the held rows because the corrupted rows are the
/// ones where the decomposition breaks; a decomposition that
/// also matches at the same bar on all evaluated rows is one
/// the identity scans state, and the pair is then
/// [`PartWhole::IdentityCovered`], not a component for this
/// pass. Candidate rests are screened against the slack on the
/// held rows — a rest lies between zero and the slack on at
/// least [`FLOOR_SHARE`] of them, the part-bound screen of the
/// identity scans read against the slack — and capped at
/// [`REST_POOL_MAX`], largest mean share of the slack first.
///
/// By containment, when no decomposition verifies: both columns
/// respect the shared zero floor on at least [`FLOOR_SHARE`] of
/// the evaluated rows, and the slack reaches the floor — within
/// [`SLACK_CONTACT_REL_SHARE`] of the whole's range — on at
/// least [`SLACK_CONTACT_SHARE`] of the held rows where the
/// whole is above the floor, and on fewer than
/// [`MAX_SLACK_CONTACT_SHARE`] of them: the whole is sometimes
/// just the part, which rows where both columns are zero
/// cannot show, and columns equal on half or more of the held
/// rows are two recordings of one quantity, not a part and
/// its whole.
///
/// The test works in f64; the caller guarantees both columns
/// are exactly representable there.
fn part_whole_relation<T: ScanVal>(
    part: &[Option<T>],
    whole: &[Option<T>],
    table: &TypedTable,
    part_idx: usize,
    whole_idx: usize,
    excluded: &BTreeSet<usize>,
) -> PartWhole {
    let tol = |y: f64| IDENTITY_TOL * (1.0 + y.abs());
    let p: Vec<Option<f64>> = part.iter().map(|v| v.map(|x| x.as_f64())).collect();
    let w: Vec<Option<f64>> = whole.iter().map(|v| v.map(|x| x.as_f64())).collect();
    let mut n = 0usize;
    let mut floor_ok = 0usize;
    let mut held: Vec<usize> = Vec::new();
    let (mut w_min, mut w_max) = (f64::INFINITY, f64::NEG_INFINITY);
    for r in 0..p.len() {
        let (Some(x), Some(y)) = (p[r], w[r]) else {
            continue;
        };
        n += 1;
        if x >= -tol(y) && y >= -tol(y) {
            floor_ok += 1;
        }
        if x <= y + tol(y) {
            held.push(r);
        }
        w_min = w_min.min(y);
        w_max = w_max.max(y);
    }
    if n < MIN_EVALUATED || held.is_empty() {
        return PartWhole::None;
    }

    // Identity route: screen candidate rests against the slack.
    let slack: Vec<Option<f64>> = p
        .iter()
        .zip(&w)
        .map(|(x, y)| match (x, y) {
            (Some(x), Some(y)) => Some(y - x),
            _ => None,
        })
        .collect();
    let mut pool: Vec<(usize, f64)> = Vec::new();
    for c in 0..table.cols.len() {
        if c == part_idx || c == whole_idx || excluded.contains(&c) {
            continue;
        }
        let col = &table.cols[c];
        if !matches!(
            col,
            TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. }
        ) {
            continue;
        }
        let (mut m, mut inside, mut s_sum, mut r_sum) = (0usize, 0usize, 0.0, 0.0);
        for &r in &held {
            let (Some(s), Some(v)) = (slack[r], col.numeric_at(r)) else {
                continue;
            };
            m += 1;
            if v >= -tol(s) && v <= s + tol(s) {
                inside += 1;
            }
            s_sum += s;
            r_sum += v;
        }
        if m >= MIN_REST_ROWS && inside as f64 >= FLOOR_SHARE * m as f64 {
            pool.push((c, if s_sum > 0.0 { r_sum / s_sum } else { 0.0 }));
        }
    }
    pool.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    pool.truncate(REST_POOL_MAX);
    let pool: Vec<usize> = pool.into_iter().map(|(c, _)| c).collect();

    // Verify a candidate rest against the decomposition
    // whole = part + rest. Some(true): it also matches at the
    // identity bar on all evaluated rows (identity-covered).
    // Some(false): it matches on the held rows only.
    let held_total = held.len() as f64;
    let verify = |rest: &[usize]| -> Option<bool> {
        let (mut hn, mut h_match, mut h_miss) = (0usize, 0usize, 0usize);
        let (mut an, mut a_match, mut nonneg, mut neg) = (0usize, 0usize, 0usize, 0usize);
        for r in 0..p.len() {
            let (Some(x), Some(y)) = (p[r], w[r]) else {
                continue;
            };
            let mut rest_sum = 0.0;
            let mut present = true;
            for &c in rest {
                match table.cols[c].numeric_at(r) {
                    Some(v) => rest_sum += v,
                    None => {
                        present = false;
                        break;
                    }
                }
            }
            if !present {
                continue;
            }
            an += 1;
            let matches = (y - (x + rest_sum)).abs() <= tol(y);
            if matches {
                a_match += 1;
            }
            if rest_sum >= -tol(y) {
                nonneg += 1;
            } else {
                neg += 1;
                if neg as f64 > (1.0 - REST_NONNEG_SHARE) * n as f64 {
                    return None;
                }
            }
            if x <= y + tol(y) {
                hn += 1;
                if matches {
                    h_match += 1;
                } else {
                    h_miss += 1;
                    if h_miss as f64 > (1.0 - IDENTITY_MATCH_SHARE) * held_total {
                        return None;
                    }
                }
            }
        }
        if hn == 0 || an == 0 {
            return None;
        }
        if (h_match as f64) < IDENTITY_MATCH_SHARE * hn as f64 {
            return None;
        }
        if (nonneg as f64) < REST_NONNEG_SHARE * an as f64 {
            return None;
        }
        Some((a_match as f64) >= IDENTITY_MATCH_SHARE * an as f64)
    };

    // Subsets of the pool, smallest first, in pool order: the
    // simplest decomposition is the witness.
    fn search<F: Fn(&[usize]) -> Option<bool>>(
        pool: &[usize],
        start: usize,
        depth: usize,
        subset: &mut Vec<usize>,
        verify: &F,
    ) -> Option<bool> {
        if depth == 0 {
            return verify(subset);
        }
        for k in start..pool.len() {
            subset.push(pool[k]);
            let hit = search(pool, k + 1, depth - 1, subset, verify);
            subset.pop();
            if hit.is_some() {
                return hit;
            }
        }
        None
    }
    let mut subset: Vec<usize> = Vec::new();
    for size in 1..=REST_MAX_TERMS.min(pool.len()) {
        if let Some(covered) = search(&pool, 0, size, &mut subset, &verify) {
            return if covered {
                PartWhole::IdentityCovered
            } else {
                PartWhole::Component("identity")
            };
        }
    }

    // Containment route.
    if floor_ok as f64 >= FLOOR_SHARE * n as f64 {
        let band = SLACK_CONTACT_REL_SHARE * (w_max - w_min);
        let (mut above, mut contact) = (0usize, 0usize);
        for &r in &held {
            let y = w[r].unwrap();
            if y > tol(y) {
                above += 1;
                if y - p[r].unwrap() <= band {
                    contact += 1;
                }
            }
        }
        if above > 0
            && contact as f64 >= SLACK_CONTACT_SHARE * above as f64
            && (contact as f64) < MAX_SLACK_CONTACT_SHARE * above as f64
        {
            return PartWhole::Component("containment");
        }
    }
    PartWhole::None
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
    let (claim, interpretation) = if c.systematic {
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
        let reverse = c.reverse_violations();
        let claim = format!(
            "{} <= {} is violated on {} of {} evaluated rows ({:.1}%), while the reverse order is violated on {} rows ({:.1}%): {}",
            lower_name,
            upper_name,
            c.violations,
            c.n,
            100.0 * share,
            reverse,
            100.0 * reverse as f64 / c.n.max(1) as f64,
            worst,
        );
        let interp = if c.part_whole {
            let basis = match c.part_whole_basis {
                Some("identity") => {
                    "the whole minus the part decomposes exactly into other columns on the rows where the order holds"
                }
                _ => {
                    "the pair respects the shared zero floor and the whole is sometimes just the part"
                }
            };
            format!(
                "{lower_name} is a component of {upper_name} — {basis} — so {lower_name} <= {upper_name} holds by construction wherever the data is sound, and a part sits far from its whole by construction, which is why this statement carries no comparable-scale requirement. Exceedances on {:.1}% of the rows where both are present are the data-error signal: a part recorded against the wrong whole, a part counted twice, or a whole recorded before all its parts arrived. The violating rows are listed in the evidence",
                100.0 * share,
            )
        } else {
            format!(
                "the order {lower_name} <= {upper_name} holds on far more rows than its reverse, which is the shape of a constraint between the two columns; violations on {:.1}% of the rows where both are present are too many for scattered recording errors and too one-sided for two unrelated columns crossing. The usual causes are a definition that changed partway through the data, two populations merged under one pair of columns, or one column not measuring the bound its pairing suggests. The violating rows are listed in the evidence",
                100.0 * share,
            )
        };
        (claim, interp)
    } else if c.violations == 0 {
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
            "violations_if_shuffled".into(),
            json!(c.chance),
        ),
        (
            "method".into(),
            json!("exact pairwise order scan: candidate pairs share a value kind (numeric with numeric, dates with dates, datetimes with datetimes of one unit), carry at least 5 distinct values each, and have compatible ranges (overlap at least 25% of the narrower range, widths within 100x); a direction is reported when its violations are zero or at most 1% of the rows where both columns are present, and random pairing of the two columns would give at least 20 violations and at least 4 times those observed, after screening on a deterministic row sample when the table is large and verification on all rows; at most one direction per pair, the one with fewer violations; pairs are screened nearest columns first, up to 64 times the shared max_pairs budget, and at most max_pairs survivors are verified"),
        ),
    ]);
    if c.systematic {
        metrics.insert(
            "violation_kind".into(),
            json!(if c.part_whole {
                "part_whole"
            } else {
                "systematic"
            }),
        );
        metrics.insert("reverse_violations".into(), json!(c.reverse_violations()));
        metrics.insert(
            "pair_distance".into(),
            json!((c.pair_distance * 1000.0).round() / 1000.0),
        );
        if c.part_whole {
            metrics.insert(
                "part_whole_basis".into(),
                json!(c.part_whole_basis.unwrap_or("containment")),
            );
            metrics.insert(
                "method".into(),
                json!("exact pairwise order scan: candidate pairs share a value kind (numeric with numeric, dates with dates, datetimes with datetimes of one unit), carry at least 5 distinct values each, and have compatible ranges (overlap at least 25% of the narrower range, widths within 100x); a direction that fails the certification bar (violations zero or at most 1% of the rows where both columns are present, beating random pairing) is reported as a part-against-whole violation when its violations number at least 20 and between 5% and 25% of the evaluated rows and one column is a component of the other — by identity, when the whole minus the part equals a sum of one to three other columns within the identity tolerance (1e-9 absolute + 1e-9 relative) on at least 99.9% of the rows where the order holds, with the rest non-negative on at least 99% of the evaluated rows, or by containment, when both columns respect the shared zero floor on at least 99% of the evaluated rows and the slack reaches the floor (within 0.1% of the whole's range) on at least 5% of the held rows where the whole is above it but on fewer than half of them (columns equal on half or more of the held rows are two recordings of one quantity, not a part and its whole), and the reverse direction's violations are at least twice the stated direction's — and no exact sum identity states the decomposition (one matching on at least 99.9% of all evaluated rows is the identity pass's statement, and this one stays silent for the pair); the comparable-scale gate of the systematic form does not apply, because a part's distance from its whole is set by the other components, after screening on a deterministic row sample when the table is large and verification on all rows; at most one direction per pair; pairs are screened nearest columns first, up to 64 times the shared max_pairs budget, and at most max_pairs survivors are verified"),
            );
        } else {
            metrics.insert(
                "method".into(),
                json!("exact pairwise order scan: candidate pairs share a value kind (numeric with numeric, dates with dates, datetimes with datetimes of one unit), carry at least 5 distinct values each, and have compatible ranges (overlap at least 25% of the narrower range, widths within 100x); a direction that fails the certification bar (violations zero or at most 1% of the rows where both columns are present, beating random pairing) is reported as a systematic violation when its violations number at least 20 and at least 5% of the evaluated rows and the reverse direction's violations are at least twice as many, and the two columns track each other on a comparable scale (median |a - b| / max(|a|, |b|) over sampled complete rows at most 0.2, and the columns' medians within a factor of 2.5 of each other when both are non-zero), after screening on a deterministic row sample when the table is large and verification on all rows; at most one direction per pair; pairs are screened nearest columns first, up to 64 times the shared max_pairs budget, and at most max_pairs survivors are verified"),
            );
        }
    }
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
        if c.systematic {
            Pattern::OrderViolation
        } else {
            Pattern::OrderConstraint
        },
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
                excluded: BTreeSet::new(),
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
                excluded: BTreeSet::new(),
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
                excluded: BTreeSet::new(),
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

    #[test]
    fn systematic_violation_is_reported_with_its_count() {
        // generated in [50, 249]; accepted exceeds it on 480 of
        // 2,000 rows (24%), falls below it on 1,300 (65%), and ties
        // on 220. The direction accepted <= generated fails far
        // past the 1% certification bar, and the reverse fails far
        // more often: a systematic violation, stated with its count.
        let gen: Vec<f64> = (0..2000).map(|k| 50.0 + ((k * 37) % 200) as f64).collect();
        let mut acc = gen.clone();
        for (k, a) in acc.iter_mut().enumerate() {
            if k < 480 {
                *a += 5.0;
            } else if k < 1780 {
                *a -= 3.0;
            }
        }
        let df = df! { "generated" => gen, "accepted" => acc }.unwrap();
        let out = scan(&df);
        let hits = pair_findings(&out, "accepted", "generated");
        assert_eq!(
            hits.len(),
            1,
            "{:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        let f = hits[0];
        assert_eq!(f.finding_type.as_deref(), Some("order_violation"));
        let m = &f.evidence[0].metrics;
        assert_eq!(m["violations"], json!(480));
        assert_eq!(m["reverse_violations"], json!(1300));
        assert_eq!(m["violation_kind"], json!("systematic"));
        assert!(f.claim.contains("480 of 2000"), "{}", f.claim);
        f.validate().unwrap();
    }

    #[test]
    fn crossing_counts_at_similar_rates_stay_quiet() {
        // Two unrelated zero-inflated counts: each exceeds the
        // other on a substantial share of rows, at similar rates.
        // No direction certifies, and the asymmetry the systematic
        // statement requires is absent.
        let a: Vec<f64> = (0..2000)
            .map(|k| {
                if k % 5 < 3 {
                    0.0
                } else {
                    ((k * 37) % 60) as f64
                }
            })
            .collect();
        let b: Vec<f64> = (0..2000)
            .map(|k| {
                if k % 7 < 4 {
                    0.0
                } else {
                    ((k * 91 + 13) % 60) as f64
                }
            })
            .collect();
        let df = df! { "a" => a, "b" => b }.unwrap();
        let out = scan(&df);
        assert!(
            out.findings.is_empty(),
            "{:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
    }

    #[test]
    fn one_sided_but_near_symmetric_crossings_stay_quiet() {
        // accepted exceeds generated on 24% of rows, generated
        // exceeds accepted on 30%, ties on 46%: one direction
        // fails in bulk, but the reverse is not twice as violated,
        // so neither a certification nor a systematic violation is
        // stated — the pair crosses both ways.
        let gen: Vec<f64> = (0..2000).map(|k| 50.0 + ((k * 37) % 200) as f64).collect();
        let mut acc = gen.clone();
        for (k, a) in acc.iter_mut().enumerate() {
            if k < 480 {
                *a += 5.0;
            } else if k < 1080 {
                *a -= 3.0;
            }
        }
        let df = df! { "generated" => gen, "accepted" => acc }.unwrap();
        let out = scan(&df);
        assert!(
            out.findings.is_empty(),
            "{:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
    }

    #[test]
    fn part_exceeding_its_whole_is_stated_without_the_scale_gate() {
        // total = part + rest as recorded; the part is then
        // corrupted upward on 480 of 2,000 rows (24%), so it
        // exceeds its whole there. The pair's median row distance
        // is the rest's share of the total — far past the
        // systematic rule's 0.2 scale gate — and the sum identity
        // holds on only 76% of rows, below the identity scan's
        // 99.9% bar, so before the variant no pass stated the
        // exceedances.
        let n = 2000;
        let rest: Vec<f64> = (0..n).map(|k| 5.0 + ((k * 53) % 40) as f64).collect();
        let mut part: Vec<f64> = (0..n).map(|k| 10.0 + ((k * 29) % 40) as f64).collect();
        let total: Vec<f64> = part.iter().zip(&rest).map(|(p, r)| p + r).collect();
        for k in 0..480 {
            part[k] = total[k] + 5.0;
        }
        let df = df! { "part" => part, "rest" => rest, "total" => total }.unwrap();
        let out = scan(&df);
        let hits = pair_findings(&out, "part", "total");
        assert_eq!(
            hits.len(),
            1,
            "{:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        let f = hits[0];
        assert_eq!(f.finding_type.as_deref(), Some("order_violation"));
        let m = &f.evidence[0].metrics;
        assert_eq!(m["violations"], json!(480));
        assert_eq!(m["violation_kind"], json!("part_whole"));
        assert_eq!(m["part_whole_basis"], json!("identity"));
        assert!(f.claim.contains("480 of 2000"), "{}", f.claim);
        f.validate().unwrap();
    }

    #[test]
    fn healthy_part_whole_pair_states_the_order_not_a_violation() {
        // The same decomposition, uncorrupted: part <= total
        // holds exactly and is stated as an order.
        let n = 2000;
        let rest: Vec<f64> = (0..n).map(|k| 5.0 + ((k * 53) % 40) as f64).collect();
        let part: Vec<f64> = (0..n).map(|k| 10.0 + ((k * 29) % 40) as f64).collect();
        let total: Vec<f64> = part.iter().zip(&rest).map(|(p, r)| p + r).collect();
        let df = df! { "part" => part, "rest" => rest, "total" => total }.unwrap();
        let out = scan(&df);
        let hits = pair_findings(&out, "part", "total");
        assert_eq!(
            hits.len(),
            1,
            "{:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        assert_eq!(hits[0].finding_type.as_deref(), Some("order_constraint"));
        assert_eq!(hits[0].evidence[0].metrics["violations"], json!(0));
        hits[0].validate().unwrap();
    }

    #[test]
    fn contained_part_without_a_decomposition_is_stated() {
        // The whole is recorded independently — no rest column
        // exists to decompose the slack. The part sits inside
        // the whole, the slack is zero on a tenth of the held
        // rows (the whole is sometimes just the part), and the
        // part exceeds the whole on 480 of 2,000 rows (24%).
        let n = 2000;
        let whole: Vec<f64> = (0..n).map(|k| 40.0 + ((k * 37) % 120) as f64).collect();
        let mut part: Vec<f64> = whole
            .iter()
            .enumerate()
            .map(|(k, w)| {
                if (k * 13) % 10 == 9 {
                    *w
                } else {
                    w * (((k * 13) % 10) as f64 / 10.0)
                }
            })
            .collect();
        for k in 0..480 {
            part[k] = whole[k] + 5.0;
        }
        let df = df! { "part" => part, "whole" => whole }.unwrap();
        let out = scan(&df);
        let hits = pair_findings(&out, "part", "whole");
        assert_eq!(
            hits.len(),
            1,
            "{:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        let f = hits[0];
        assert_eq!(f.finding_type.as_deref(), Some("order_violation"));
        let m = &f.evidence[0].metrics;
        assert_eq!(m["violations"], json!(480));
        assert_eq!(m["violation_kind"], json!("part_whole"));
        assert_eq!(m["part_whole_basis"], json!("containment"));
        f.validate().unwrap();
    }

    #[test]
    fn unordered_pair_sharing_a_zero_floor_is_not_stated() {
        // Two independent columns over {0, .., 4}: both respect
        // the zero floor and the slack reaches it on about a
        // fifth of the held rows, so the containment reading
        // certifies the pair — but each direction violates the
        // other on about 40% of rows. Neither direction is a
        // broken bound, and the pair earns no order finding.
        fn draws(seed: u64, n: usize) -> Vec<i64> {
            let mut state = seed;
            (0..n)
                .map(|_| {
                    state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
                    let mut z = state;
                    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
                    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
                    ((z ^ (z >> 31)) % 5) as i64
                })
                .collect()
        }
        let df = df! { "a" => draws(1, 5000), "b" => draws(2, 5000) }.unwrap();
        let out = scan(&df);
        assert!(
            pair_findings(&out, "a", "b").is_empty(),
            "{:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
    }

    #[test]
    fn negative_rest_pair_is_not_a_part_whole_violation() {
        // total = part + rest holds on every row, but the rest
        // is negative on 480 rows (a contra component), so the
        // part exceeds the total there by construction. The
        // decomposition exists and the identity scan states it;
        // the exceedances are not a bound's failure and the
        // variant stays silent.
        let n = 2000;
        let part: Vec<f64> = (0..n).map(|k| 10.0 + ((k * 29) % 40) as f64).collect();
        let rest: Vec<f64> = (0..n)
            .map(|k| {
                if k < 480 {
                    -5.0
                } else {
                    30.0 + ((k * 53) % 90) as f64
                }
            })
            .collect();
        let total: Vec<f64> = part.iter().zip(&rest).map(|(p, r)| p + r).collect();
        let df = df! { "part" => part, "rest" => rest, "total" => total }.unwrap();
        let out = scan(&df);
        assert!(
            pair_findings(&out, "part", "total").is_empty(),
            "{:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
    }

    #[test]
    fn part_whole_relation_distinguishes_its_cases() {
        let n = 2000;
        let rest: Vec<f64> = (0..n).map(|k| 5.0 + ((k * 53) % 40) as f64).collect();
        let part: Vec<f64> = (0..n).map(|k| 10.0 + ((k * 29) % 40) as f64).collect();
        let total: Vec<f64> = part.iter().zip(&rest).map(|(p, r)| p + r).collect();
        let opts = |v: &[f64]| v.iter().copied().map(Some).collect::<Vec<_>>();
        let df = df! { "part" => part.clone(), "rest" => rest, "total" => total.clone() }.unwrap();
        let table = extract(&df);
        let idx = |name: &str| table.names.iter().position(|x| x == name).unwrap();
        // The decomposition holds everywhere: the identity scans
        // state it, so the pair is covered.
        match part_whole_relation(
            &opts(&part),
            &opts(&total),
            &table,
            idx("part"),
            idx("total"),
            &BTreeSet::new(),
        ) {
            PartWhole::IdentityCovered => {}
            _ => panic!("an exact decomposition must read as identity-covered"),
        }
        // Corrupt the part on 24% of rows: the decomposition
        // holds only on the held rows — a component, by identity.
        let mut bad = part.clone();
        for k in 0..480 {
            bad[k] = total[k] + 5.0;
        }
        match part_whole_relation(
            &opts(&bad),
            &opts(&total),
            &table,
            idx("part"),
            idx("total"),
            &BTreeSet::new(),
        ) {
            PartWhole::Component("identity") => {}
            _ => panic!("a corrupted decomposition must read as a component by identity"),
        }
        // An age in years against a duration in seconds: no
        // decomposition of the slack, and the slack never reaches
        // the floor — no relation.
        let age: Vec<f64> = (0..n).map(|k| 21.0 + ((k * 29) % 70) as f64).collect();
        let dur: Vec<f64> = (0..n)
            .map(|k| {
                if k % 8 == 0 {
                    12.0
                } else {
                    240.0 + ((k * 53) % 900) as f64
                }
            })
            .collect();
        let df2 = df! { "age" => age.clone(), "duration" => dur.clone() }.unwrap();
        let table2 = extract(&df2);
        match part_whole_relation(&opts(&age), &opts(&dur), &table2, 0, 1, &BTreeSet::new()) {
            PartWhole::None => {}
            _ => panic!("unrelated scales must not read as part and whole"),
        }
    }

    #[test]
    fn different_scales_crossing_asymmetrically_stay_quiet() {
        // An age in years against a duration in seconds: the age
        // exceeds the duration on 12.5% of rows (the short
        // durations) and the reverse direction fails on 87.5% —
        // the count, share, and asymmetry terms of the systematic
        // rule all pass. But the columns share no scale and do not
        // track their diagonal, so no systematic violation is
        // stated: the pairing is two measurements in different
        // units whose ranges happen to overlap.
        let age: Vec<f64> = (0..2000).map(|k| 21.0 + ((k * 29) % 70) as f64).collect();
        let dur: Vec<f64> = (0..2000)
            .map(|k| {
                if k % 8 == 0 {
                    12.0
                } else {
                    240.0 + ((k * 53) % 900) as f64
                }
            })
            .collect();
        let df = df! { "age" => age, "duration" => dur }.unwrap();
        let out = scan(&df);
        assert!(
            out.findings.is_empty(),
            "{:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
    }
}
