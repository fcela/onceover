# Discontinuities screen: cost model and optimization

Date: 2026-10-30. Issue #65: the discontinuities pass took
81–85% of profile wall time on every measured table with 30+
numeric columns. This change cuts it 1.8–2.9x on the issue's
fixtures with no behavior change: every optimization shares
work that is provably target-independent or parallelizes
independent targets; a unit test pins the shared-order scan
to bit-identical candidates against the plain scan.

## Cost model

Per numeric target the screen runs one axis unit per other
numeric column, plus — when axes exceed `all_pairs_max_cols`
(20) — diagonal units for the top-10 fallback axes:
C(10, 2) = 45 pairs × ~13 non-axis directions = ~585 units.
At 38 numeric targets (the 10,000 x 50 fixture) that is
~23,600 scan units. Each unit costs one sort of the projection
(O(n log n)) plus one O(n q²) Gram/prefix pass in `disc_scan`
(q = 3 background columns), plus per-unit setup: row
selection, standardization, and a flat background build
(O(n)–O(n q)). The diagonal units dominate the count (96%),
and their projections, sort orders, standardized axes, and
backgrounds do not depend on the target at all — only on the
axis pair and the kept rows. The original code rebuilt all of
them for every target.

## Changes

- `disc_scan` splits into a wrapper and `disc_scan_ordered`:
  the sort order is computed once per unit and shared between
  the coarse scan and the refinement scan; the fit-half
  quantile grid is read off the same order instead of sorting
  the fit half again. Per-cut Gram buffers are hoisted out of
  the cut loop.
- Diagonal pair data (kept rows, standardized axes, fit mask,
  flat background) is cached per axis pair and reused across
  targets, validated by exact kept-row equality; projection
  sort orders are cached per (pair, direction) under the same
  validation, with a bounded total size (8M elements, cleared
  when full).
- Targets are screened in parallel (rayon, already used by
  the gbm and dependence passes in this crate) and merged in
  target order.

## Before / after

Release build, this machine (2 cores), standard budget,
`timing=True`; baseline is the same fixtures on the pre-change
build measured the same day (the 10,000 x 50 baseline
reproduces the issue's numbers: 41.4s wall / 33.3s pass
against the issue's 41.8s / 33.7s).

| fixture | discontinuities before | after | wall before | after |
|---|---|---|---|---|
| synthetic 10,000 x 50 | 33.3s | 18.2s | 41.4s | 26.5s |
| synthetic 2,000 x 14 | 1.72s | 0.60s | 2.26s | 1.15s |
| state-unemployment 609 x 52 | 5.0s (issue) | 2.36s | 5.9s (issue) | 3.41s |

Finding counts are unchanged on all three fixtures
(100 / 38 / 100).

## Behavior contract

- New unit test: `disc_scan_ordered` with a precomputed order
  returns bit-identical candidates (threshold, jump, z,
  t_fit, gain) to `disc_scan` on a planted step.
- Detection battery: all discontinuity cases pass; the full
  battery on this build reads 51 pass / 6 fail / 0 gap /
  9 observed of 66, the 6 fails being the two standing drift
  fails and the four cases whose detectors live in PRs
  #77–#79 (not in this branch). No disc case changed.
- Corpus expectations unchanged; Python suite 125 passed /
  1 skipped; cargo workspace 166 passed.

What remains on the table: the per-unit Gram/prefix pass is
now the floor (it is the statistic itself), and the axis
units still rebuild their (target-specific) row selections
per target — that part is genuinely target-dependent.
