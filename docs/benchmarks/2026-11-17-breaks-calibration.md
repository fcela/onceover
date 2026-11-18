# Breaks acceptance bars: calibration

Issue #361. The breaks pass (`crates/onceover-mines/src/breaks.rs`)
accepts a candidate break when a segmentation learned on training
rows clears two gain bars — `min_train_gain` on the training rows
themselves and `min_holdout_gain` on a held-out fifth of the rows —
and refuses segments shorter than `min_segment`. The shipped values
(0.10 / 0.15 / 30) had no measured basis. This record plants breaks
of known size, position, and segment length, sweeps the three bars
jointly, and sets the defaults from the measured frontier:
**train 0.075, holdout 0.10, segment 30**.

## How the pass accepts a break (what the bars sit inside)

Rows are split deterministically: a splitmix hash of the row index
sends about one row in five to a holdout set. Level breaks come from
recursive binary segmentation on the training rows (each split must
clear `min_train_gain`); trend and variance breaks are single
best splits of the same form. A result is stated only if the fixed
segmentation reduces holdout squared error by at least
`min_holdout_gain` — and, for level breaks, only if *every* segment
of the segmentation holds at least 10 holdout rows; one short
segment voids the whole result. That last rule is not one of the
three bars, but the sweep shows it decides as much as they do, so
it is measured here too.

## Planted suite and controls

Planted (99 configurations, 16 series each): level shifts at the
midpoint of series of length 120 / 240 / 609 / 2,000 / 6,000 with
sizes 0.25σ–2σ; level shifts near the edge (short side 20–120
points) at 609 and 2,000; level shifts on AR(1) ρ=0.5 noise; trend
kinks (continuous, slope change only) with end separations
0.5σ–4σ at the midpoint and at the quarter; variance breaks with
σ ratios 1.5–4 at the midpoint and near the edge; two-break level
series. Noise is Gaussian, σ=1 unless a background says otherwise.

Controls (23 shapes, 32 series each): iid noise at five lengths;
AR(1) ρ=0.5 and ρ=0.9 at 240 / 609 / 2,000; linear ramps rising
1σ and 3σ over the series; period-12 seasonals of amplitude 1σ
and 2σ. A control series has no break; every statement on it is
false.

Sweep: stage A crosses train gain {0.03, 0.05, 0.075, 0.10, 0.15,
0.20} with holdout gain {0.05, 0.10, 0.15, 0.20, 0.25} at segment
30 (30 cells). Stage B crosses segment {10, 15, 20, 30, 45, 60}
with the gain pairs (0.03, 0.05), (0.05, 0.10), (0.10, 0.15).
Series are paired across cells (same seed base per stage), so
cell differences come from the bars, not the draws. A statement
counts as locating a planted break when its breakpoint falls
within max(4, 2% of n) of the planted position.

## The detection frontier

Level shifts at the midpoint, stated of 16:

| planted | 0.10/0.15 (was) | 0.075/0.10 (now) | 0.05/0.10 | 0.05/0.15 | 0.03/0.05 |
|---|---|---|---|---|---|
| n=609, 0.5σ | 0 | 0 | 2 | 0 | 4 |
| n=609, 0.75σ | 6 | 7 | 8 | 6 | 10 |
| n=609, 1σ | 11 | 14 | 14 | 11 | 13 |
| n=609, 1.5σ | 15 | 15 | 15 | 15 | 15 |
| n=2000, 0.5σ | 0 | 0 | 0 | 0 | 8 |
| n=2000, 0.75σ | 1 | 10 | 10 | 1 | 15 |
| n=2000, 1σ | 15 | 15 | 15 | 15 | 15 |
| n=2000, 1.5σ | 16 | 16 | 16 | 16 | 16 |

Variance breaks at the midpoint, stated of 16:

| planted | 0.10/0.15 (was) | 0.075/0.10 (now) | 0.05/0.10 | 0.05/0.15 | 0.03/0.05 |
|---|---|---|---|---|---|
| n=609, ratio 2 | 1 | 4 | 4 | 1 | 8 |
| n=609, ratio 3 | 3 | 8 | 8 | 3 | 10 |
| n=609, ratio 4 | 5 | 9 | 9 | 5 | 10 |
| n=2000, ratio 1.5 | 0 | 0 | 0 | 0 | 9 |
| n=2000, ratio 2 | 0 | 13 | 13 | 0 | 16 |
| n=2000, ratio 3 | 12 | 16 | 16 | 12 | 16 |
| n=2000, ratio 4 | 14 | 16 | 16 | 14 | 16 |

The holdout bar is the lever. At 0.15, a 0.75σ level shift on a
2,000-point series is stated once in 16 and a variance doubling
never; the holdout gain of those segmentations concentrates just
under 0.15. Dropping the holdout bar to 0.10 moves the frontier a
full effect step at panel length and above. The train bar between
0.05 and 0.075 changes no planted row; its effect shows on the
control side and on short series (below).

Trend kinks are stated at the planted position rarely at *every*
cell (4σ end separation: 1 of 16 at n=609, 3 of 16 at n=2000) —
but the pass does produce a trend statement on nearly every such
series (15 of 16 at n=609 at the (0.05, 0.10) cell); the
unconstrained two-line fit places its split away from the kink,
trading a level jump for slope error. The bars do not gate this:
hit counts are identical across train gains and flat across
holdout gains until 0.25. Locating a continuous kink is a fit
problem, not a bar problem, and no bar setting in the grid
changes it.

## The control side

False statements per 736 control series (23 shapes × 32):

| bars | false | per 100 series |
|---|---|---|
| 0.10 / 0.15 (was) | 227 | 30.8 |
| 0.075 / 0.10 (now) | 257 | 34.9 |
| 0.05 / 0.10 | 267 | 36.3 |
| 0.05 / 0.15 | 240 | 32.6 |
| 0.03 / 0.05 | 380 | 51.6 |

The composition matters more than the total. iid, seasonal, and
AR(0.5) controls produce **zero** false statements at every cell
in the grid — for well-behaved noise the train/holdout
architecture, not the bars, is the control. Every false statement
in the sweep comes from two shapes:

- **AR(1) ρ=0.9.** At n=240 half the series draw a false trend
  statement at every cell; at n=609 all three kinds fire; at
  n=2000 the level count is the one number the bars move (13 of
  32 series at train 0.05, 1 of 32 at train 0.075, holdout 0.10).
  Neighbouring rows fall on both sides of the interleaved
  holdout split, so the holdout is not independent evidence
  under strong autocorrelation. No bar in the grid fixes this.
- **Steep ramps.** A 3σ ramp draws about 2.5–3 level statements
  per series at n=609 and n=2000 at every cell, including the
  strictest: a step function approximates a ramp well (a midpoint
  split captures about 75% of a linear ramp's variance), so the
  gains are genuinely large. A 1σ ramp is nearly silent at the
  calibrated bars (0–3 level statements per 32 series).

## Segment length and the holdout floor

`min_segment` does not act as the floor it names:

- A segment needs ~50 original points before its holdout share
  reaches 10 with any reliability (at 50 points the share is
  Binomial(50, 0.2), below 10 about 44% of the time; at 80,
  about 3%). Edge breaks with a short side of 20 or 35 points
  are stated **0 of 16 at every cell of both stages**, including
  segment 10 — the holdout-count rule, not `min_segment`, is
  the binding floor. A short side of 50 is a coin flip on the
  holdout draw; 120 works (9–13 of 16 at δ=1.5).
- An edge break's gain fraction also dilutes with series
  length (it scales with s(n−s)/n²), so the same 80-point,
  1.5σ edge segment is stated *less* often at n=2000 than at
  n=609. Lengthening the series does not rescue edge breaks.
- Lowering `min_segment` at low train gains *loses* planted
  mid-series breaks on short series: at n=120, δ=1.5, the
  aggressive recursion adds noise splits whose tiny segments
  void the whole level result under the holdout-count rule
  (0 of 16 at segment 10 / train 0.03, 13 of 16 at segment 30).
  Raising it to 45–60 unscans series below 130–160 points
  entirely and *raises* ramp false statements (fewer, larger
  segments survive the holdout-count rule more often — the
  same voiding mechanism, inverted). At 30 the two failure
  directions balance; the sweep gives no support for moving
  it, and none for scaling it with n (at n=6000 the segment
  value changes nothing measured).

## Chosen defaults

- `min_holdout_gain` 0.15 → **0.10**: the frontier move. It
  costs false statements only on shapes that already false-fire
  at 0.15 (AR(0.9), steep ramps); the well-behaved controls stay
  at zero.
- `min_train_gain` 0.10 → **0.075**: identical planted frontier
  to 0.05 on every row, with fewer AR(0.9) level falses at
  n=2000 (1 vs 13 of 32) and more margin than 0.05 against the
  short-series recursion voiding above. The larger cut to 0.05
  buys nothing planted.
- `min_segment` stays **30**: measured balance point between
  voiding (below) and unscanned short series plus ramp falses
  (above).

Regression tests in `breaks.rs` pin the ends of the sweep at the
shipped defaults: break-free iid / seasonal / AR(0.5) series at
n=609 stay silent; a variance doubling at n=2000 and a 0.75σ
level shift at n=2000 are stated (0 of 16 and 1 of 16 at the old
bars); an AR(0.9) series at n=2000 draws no level statement.

## What the bars cannot fix (recorded, not tuned around)

1. False breaks on strongly autocorrelated and steep-ramp
   series occur at every bar setting. The routes are
   architectural — a holdout that is independent under
   autocorrelation (blocked rather than interleaved), and a
   guard that compares a step fit against a trend fit before
   stating a level shift on a monotone series.
2. Breaks within ~50 points of the edge are invisible to the
   level scan at any bar setting because of the
   10-holdout-rows-per-segment rule, and that rule also voids
   whole results when recursion creates one short segment.
   Stating per-segment support in coverage, or validating
   segments independently instead of all-or-nothing, are the
   candidate directions.
3. Trend statements locate continuous kinks poorly because the
   two-line fit is unconstrained; a continuity-constrained fit
   is the direction.

## Provenance

Temporary Rust harness driving the shipped `profile_breaks` on
synthetic tables (planted suite and controls above; paired seeds
per stage). Stage A: 30 gain cells × (99 planted × 16 + 23
controls × 32). Stage B: 3 gain pairs × 6 segment values, same
suite. Numbers in the tables are stage A unless a stage is
named. The harness was deleted after the run, following the
ordered-partitions calibration precedent
(`2026-11-01-ordered-partitions.md`); the regression tests above
are the permanent residue.
