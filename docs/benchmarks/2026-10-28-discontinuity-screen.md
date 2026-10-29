# Discontinuity screen (issue #40)

Date: 2026-10-28. Library change: new default-on pass
`discontinuities` (detector id `discontinuities`, category
`changes`, finding type `step_discontinuity`, form `step`); the
detector count goes from 19 to 20. The screen is a port of the
mboostrs-dev discontinuity screen (`screen_discontinuities`, its
Rust `disc` kernels), generalized from a single designated target
to the profiler's rotation: every numeric column takes the target
role against every other numeric column.

## Why

The interestingness taxonomy's remaining structural gap: onceover
found level changes only in an ordered series (the `breaks` pass).
A step in an unordered table — a target column whose level jumps
across a threshold in another column, or across a diagonal boundary
in two — was invisible. The M5-style `regimes` pass does not cover
it: its finding reports local linear regimes, never the jump (the
jump's size is entangled with the leaf slopes); ancestor smoothing
shrinks leaf predictions toward the parent, blurring the boundary;
its splits are axis-aligned, so a diagonal step can only be
staircased against bounded depth and pruning; and it is gated
(4 targets, 5,000 rows, holdout R² >= 0.7), so a modest jump inside
a weak relationship produces no finding.

## Method

For each numeric target: axis units (step indicator along each
other numeric column, background `[1, u, u^2]` on the standardized
axis) and diagonal units (step across `cos*z1 + sin*z2` over 16
directions, background `[1, z1, z2]`; directions within one step of
an axis are skipped). Cutpoints are 20 interior quantiles; the
winning threshold is refined on a finer grid inside the winning
interval, fit half only — at the coarse grid, a threshold one cell
from the true boundary reads a jump of 1.49 where the true jump is
2.0, because the quadratic background absorbs the misplaced step.

Rows split deterministically (parity) into fit and evaluation
halves. The fit half ranks candidates by `|t_fit|`; the evaluation
half estimates `jump`, `se`, `z` and the held-out gain, so the
winner's `z` keeps its null calibration under the search. Flagged
candidates (`|z| >= 2`, gain > 0) are typed by scale-invariance of
the local gap (nearest 60 vs nearest 15 evaluation points per
side): a jump's gap is the same at both scales; a steep smooth
transition's grows. The gap is measured on evaluation-half
residuals after the scan's smooth background is removed — on raw
values, a target trend along the boundary inflates the local
variance and a true jump types as smooth. Reporting bars, stricter
than flagging because the screen runs per target: `|z| >= 4` and
held-out gain >= 0.5% of the target's evaluation-half variance.
At most one finding per (target, axis set); an axis step beats a
near-parallel diagonal step (within 25 degrees) unless the
diagonal's `|z|` is at least 1.5x the axis step's.

Adaptations from the originating screen, recorded: the
deterministic parity split replaces the seeded random split
(onceover profiles are reproducible); threshold refinement, which
the original applies only to diagonal units, applies to both
families (measured reason above) and only to flagged units (cost);
the gap is measured on background residuals (reason above); and
the small-scale gap bar is `type_gap_t > 3`, not the original's
4.0 — on scan-background residuals in a raw-column rotation the
local variance also carries the smooth background's misfit along a
slanted boundary, and the planted diagonal case measures ~3.5 at
the 4.0 bar against ~11 for the idealized axis case. The
scale-invariance ratio (< 1.5), not the gap bar, is what rejects
steep smooth transitions in the controls below.

## Measured

Planted fixtures (Rust and Python tests assert the recoveries
within stated tolerances): an axis step (+2.0 at x = 0.6 on a
linear trend) is recovered with its jump within 0.35 of 2.0; a
diagonal step (+2.0 across `s1 = 0.45 + 0.35*s0`) is recovered
with its jump within 0.5 of 2.0 and typed as a jump; a logistic
transition of width 0.08 flags but types as smooth and is not
reported; independent noise columns produce no findings. In the
book chapter's executed demo (n = 1200), the axis step reads
jump 1.77 at threshold 0.588 (z 28.8) and the diagonal step
jump 1.62 (z 18.1).

Corpus (every registered expectation still passes — 0 failures in
the run record): bank-marketing gains the screen's clearest real
reading — `pdays` steps by +242.877 where `previous` crosses 0,
the never-contacted encoding (`pdays = -1` exactly when
`previous = 0`) stated as a level jump (z 117.3), plus the mirror
reading of `previous` across a (`day_of_week`, `pdays`) boundary.
Abalone gains three diagonal steps among the size/weight columns
(`whole_weight` +0.318 across a (length, diameter) boundary,
z 27.7). Thyroid gains three diagonal steps among the assay
columns (e.g. `tsh` -16.0 across an (`fti`, `t3`) boundary) — the
mirror phenomenon the book chapter describes: the assays are
near-deterministic functions of one underlying state, so each
assay's level differs across boundaries drawn in the others.
Hospital-dha gains none.

Runtime: the scan evaluates cuts by walking the sorted rows once
per unit (no per-scan prefix matrices) and refines only flagged
units; the standard profile of the 10,000-row end-to-end frame
takes 2.7s with the screen included, inside the suite's 5s budget
for that frame.

## Not covered

Residual screening (screening the residuals of a fitted smooth
model, the originating screen's recommended second pass), multiple
jumps per (target, axis set), and categorical axes. Flagged
candidates below the reporting bars are counted in coverage, not
reported.
