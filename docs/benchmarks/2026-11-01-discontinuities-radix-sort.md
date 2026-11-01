# Discontinuities screen: radix sort order, cache removal

Date: 2026-11-01. Follow-up to the 2026-10-30 optimization
(`2026-10-30-discontinuities-optimization.md`). On the scale
fixture at 200,000 rows the pass still took 230.29s of a
246.61s profile at 200 numeric columns. This change cuts the
pass 1.4–1.6x at 50–200 columns with no behavior change:
finding ids, claims, and the coverage string are identical to
the pre-change build on the 200-column fixture, the detection
battery is unchanged (57 pass / 2 fail / 0 gap / 9 observed),
and the corpus run is 16/16 with every expectation met.

## Protocol

Scale fixture (seed 20241024, uniform noise columns plus the
planted late_x / late_affine / late_curve / dirty_id columns),
200,000 rows, standard budget, `timing=True`, release build,
2 cores, `RAYON_NUM_THREADS=2`. Pass times are the
`elapsed_ms` values the profile records in coverage.

| columns | discontinuities before | after | wall before | after |
|---|---|---|---|---|
| 50 | 48.92s | 31.28s | 65.17s | 38.68s |
| 100 | 108.29s | 70.57s | 124.61s | 80.68s |
| 200 | 230.29s | 163.20s | 246.61s | 179.54s |

## Where the time went

Instrumenting the pass on the 200-column fixture (162,722
units: 38,612 axis, 124,110 diagonal) gave, in thread-seconds
of a 387.6 total: axis comparison sorts 61.8, staged-screen
scans 31.6, full ordered scans 40.0, per-unit data assembly
(`unit_data`) 12.7, projection builds 7.7 — and two cache
counters that reframed the problem: the pair cache built
8,858 times for 9,000 (target, pair) combinations, and the
projection order cache built 124,056 times for 124,110
diagonal units. The caches shared almost nothing at this
width. The 2026-10-30 design assumed the diagonal projections
are target-independent; they are, but the *choice* of pairs is
not — above `all_pairs_max_cols` each target scans pairs drawn
from its own top-10 axis ranking, and on noise columns those
rankings are near-random per target, so each target's pairs
are almost disjoint from every other target's. A cache keyed
by pair therefore misses, and its clear-on-overflow policy
turned the misses into rebuild cycles.

Isolated measurements (single thread, 20,000-row unit, q = 3
background): comparison `sort_order` 0.955ms;
`disc_scan_ordered` on warm arrays 0.48ms; a fresh-arrays
scan (build the unit's arrays, radix order, ordered scan)
2.32ms — the in-profile per-unit cost. The scan's arithmetic
is not the floor; moving and first-touching each unit's
~1MB of working arrays is.

## Changes

- `sort_order` is an LSD radix sort over the
  order-preserving bit transform of the values (eight 8-bit
  counting passes over the index array). The transform orders
  bit patterns exactly as `f64::total_cmp`, the sort is
  stable, and ties keep row-index order, so the returned
  order is identical to the comparison sort's.
- The projection order cache is removed, with the
  `scan_unit_ordered` entry point folded into `scan_unit`:
  its measured hit rate at this width was ~0 (124,056
  builds for 124,110 diagonal units), so it bought nothing
  and cost a lock, a kept-row vector comparison per lookup,
  and the rebuild cycles above. The pair cache stays (pair
  data is reused by the ~14 directions of each
  (target, pair)).
- The scan core is refactored, arithmetic unchanged: the
  per-half totals and base model are computed once
  (`scan_totals`), and the per-cut step-model evaluation is
  shared (`eval_cut`). A unit test continues to pin
  `disc_scan_ordered` to bit-identical candidates against
  `disc_scan`.

## Approaches measured and not shipped

- **Factorized scaffold scan** (per-cut Cholesky factors
  built once per scan unit): bit-identical results, slower
  at every width — discontinuities 56.85 / 131.49 / 295.12s
  at 50 / 100 / 200 columns against the 48.92 / 108.29 /
  230.29s baseline. Factorizing every cut of every unit up
  front costs more than the direct path's repeated Gram
  accumulation.
- **Staged row screen** (2,000-row subsample gate, full scan
  only for units clearing it): at a |t| gate of 3.5 it
  resolved 109,787 of 162,722 units on the subsample and
  still only reached 192.79s at 200 columns, because the
  per-unit setup and the stage scans themselves dominate
  once full scans are rare. It also changes internal counts
  (1,025 candidates flagged against the direct path's
  2,868 — stage candidates replace full-scan candidates for
  screened units), so it is not behavior-preserving.
- **Binned coarse scan** (per-cut-interval stream sums in
  one unsorted pass, no sort at all): 1.074ms isolated
  against the ordered scan's 0.54ms on the same warm
  arrays. The per-row binary search over the cut edges
  costs more than the sorted walk it replaces.

## What remains

Per-unit cost at 200 columns is ~2.0ms of which the scan
arithmetic is ~0.5ms (warm). The rest is data assembly:
row selection, standardization, background builds, and the
passes each unit streams over its own arrays. Reducing it
means fusing those passes or sharing axis data across
targets whose kept rows coincide; neither is attempted
here. The diagonal unit count (76% of units at 200
columns) is set by the top-10 fallback design and is a
product decision, not an optimization target.
