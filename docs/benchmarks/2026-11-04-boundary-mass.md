# Boundary mass: the `boundaries` detector (issue #119)

The censoring/support-boundary family deferred from issue #25:
values piled at the edge of a numeric column's observed support
(top-coded income, sensor floors) and unusual interior mass
points in zero-inflated columns. The case studies behind the
issue measured the misses exactly: on Adult, `capital_gain`
piles 244 values at its maximum 99,999 over a column that is 0
on 91.74% of rows, and `capital_loss` carries mass points at
1902 (304 rows), 1977 (253), and 1887 (233) over a column that
is 0 on 95.33% of rows; on Metro Interstate Traffic, `temp`
holds ten readings of exactly 0.0 against a bulk that starts at
243.39. The sentinel pass reports none of these: it looks for a
repeated implausible value concentrated in a row window and
outside a Tukey fence, a different shape.

## The rule

One exact value-count scan per numeric column (at least 100
evaluated rows, at least 20 distinct values). A value is
reported when all of these hold:

- it occurs at least 10 times at a support boundary (at least
  20 times at an interior value) and holds at most 25% of the
  evaluated rows;
- its count is at least 4x the median count of its nearest
  distinct neighbours on the support (up to 10 values inside
  the support for a boundary candidate) — the baseline is
  local because a global comparison fails on real data:
  Adult's interior heaps (7688 on 410 rows) exceed the boundary
  pile at 99,999 (244 rows) in raw count;
- for a pile at exactly zero: it is also isolated from the next
  distinct value by at least the interquartile range of the
  remaining bulk — ordinary zero inflation is a floor mass, not
  an isolated pile.

Interior mass points carry two extra guards, both forced by
measured counterexamples. The candidate must dominate the
median count on each side of it separately (up to 10 distinct
values per side, over the nonzero support): a one-sided
comparison fires all along a monotone decline — Bank
Marketing's zero-inflated contacts column declines 2772, 2106,
1142, 714, ... over 41 distinct values, and every low value
beats the median of its thinner upper neighbours — while a
genuine mass point stands out from both directions. And the
nonzero support's own edges are not interior candidates: the
smallest positive value of a rounded zero-inflated measure
collects a rounding pile (measured: 14 rows at 4.7x on a
2dp-rounded control), which is a boundary question about the
recording grid, not an interior mass point. The higher
interior count floor (20, against the boundary's 10) is the
same judgment: an interior claim has no support edge to stand
on, and every genuine mass point measured clears the floor
several times over (the smallest is 72 rows).

Two exemptions keep legitimate bounded scales quiet. A column
whose observed support is exactly a canonical scale pair —
(0, 1), (0, 5), (0, 10), (0, 100), (1, 5), (1, 7), (1, 10) —
never reports its own bounds: mass there is the scale working
as designed. And the share cap plus the local baseline keep
smooth distributions that merely reach their bound quiet:
neighbouring values carry comparable counts there, so the
dominance test fails.

Interior mass points are evaluated only in zero-inflated
columns (at least half the evaluated rows are zero), against
the nonzero values' local counts. At most 4 findings per column
(boundary piles first, then interior points by count).

Every finding reports the piled value and its exact count, its
share, the evaluated row count and the table's row count, the
null count, the local baseline and the dominance ratio, the
bulk's span, median and interquartile range with the pile
excluded, the zero count and share, the distance to the nearest
distinct value, and the implicated scope: the first example row
indices holding the value plus its first and last row.

Language discipline: the claim states the pile-up as observed.
The interpretation names a recording cap or floor as one
possible cause and states that a genuine bound of the measured
quantity produces the same shape; censoring is never asserted.
Whether a piled value can occur for the measured quantity at
all (0.0 Kelvin) depends on units and domain knowledge the
profile does not have: `ProfileHints` carries no units, bounds,
or domain field, so no declared-domain path exists to wire.
Metro is therefore covered by the distribution-evidence path —
the pile at the observed minimum, isolated from the bulk by a
gap of 243.39 against a bulk IQR of 19.63 — and the
declared-domain leg is recorded as a follow-up on issue #119.

## Evaluation

All numbers measured on the build in this change (standard
budget, default detector set unless a run says otherwise).

### Independent truth checks

Adult (corpus file, 48,842 rows; standard profile selects 72
findings, 10 of them from this detector). The headline claim,
verbatim:

> column capital_gain piles 244 values (0.5% of 48842 evaluated
> rows) at its maximum, 99999: the nearest lower value, 41310,
> occurs 3 times, and the median count among the 10 nearest
> lower values is 7 — the pile is 34.9x that local baseline;
> the rest of the column spans 0 to 41310 over 122 distinct
> values and is 0 on 91.7% of evaluated rows

The other nine: capital_gain interior heaps at 15024 (513
rows, 20.5x the larger side baseline), 7688 (410, 30.4x), and
7298 (364, 31.7x); capital_loss interior mass points at 1902
(304, 10.7x), 1977 (253, 15.3x), 1887 (233, 7.5x), and 2415
(72, 9.6x); hours_per_week piled at its maximum 99 (137 rows,
45.7x); age piled at its maximum 90 (55 rows, 6.5x). The age
pile is the Census Bureau's documented top-code for this
extract, and 99,999 is the documented capital-gain cap: the
detector states both as observed piles, and the
interpretations name a recording cap as one possible cause.

Metro (corpus file, 48,204 rows; 43 findings selected, 5 from
this detector). The headline claim, verbatim:

> column temp piles 10 values (0.0% of 48204 evaluated rows)
> at its minimum, 0: the nearest higher value, 243.39, occurs
> 1 times, and the median count among the 10 nearest higher
> values is 1 — the pile is 10.0x that local baseline; the
> rest of the column spans 243.39 to 310.07 over 5842 distinct
> values and is 0 on 0.0% of evaluated rows

The other four are rain_1h interior mass points at the rain
gauge's recording ticks: 0.51 (256 rows, 39.4x), 1.02 (123,
20.5x), 0.3 (121, 18.6x), 0.76 (109, 21.8x). The 0.3 value is
the one the sentinel pass already flags as a probable
sentinel; the rest of the tick grid is new.

Counts re-measured with Polars directly on the corpus files:
capital_gain maximum 99,999 on 244 rows; temp exactly 0.0 on
10 rows.

### Planted capped sensors (recall)

Synthetic continuous measures hard-capped (or floored) at a
known quantile, so the boundary collects the whole tail beyond
it; n = 20,000, 5 seeds per cell, two distribution families.
A cell counts as recalled when a finding names the planted
boundary value with the exact planted count.

Recall is 100/100: every cell of {Gaussian, lognormal} x
{10%, 5%, 2.5%, 1%, 0.5% tail} x {cap, floor} x 5 seeds
recalled, each with the exact planted count.

### Controls (false positives)

Legitimate shapes that must stay quiet, 5 seeds each:
graded exam scores 0-100 clipped at the bounds (two shapes:
mean 72 sd 14, and an easy exam at mean 85 sd 8 whose mass
crowds the top), task-completion percentages with atoms at 0
and 100, fractions on [0, 1] with atoms at both ends, Likert
1-5, small-support counts, wide smooth counts (Gaussian and
Poisson), a zero-inflated count column declining monotonically
over 41 distinct values (the Bank Marketing shape), a constant
column, ordinary zero inflation (continuous and 2dp-rounded
positives), uniform floats, calendar years, smooth integer
ages.

Result: 0 boundary findings in 75 control runs (15 suites x 5
seeds). Two candidate rules failed during development and were
corrected before this measurement, and the corrections are
part of the shipped rule: a first control run produced one
false positive (a 14-row rounding pile at the smallest
positive value of a 2dp-rounded zero-inflated measure, 4.7x),
which is why interior candidates carry the higher count floor
and the nonzero support's edges are excluded; and the first
corpus audit (next section) showed a one-sided interior rule
firing all along a declining count distribution, which is why
interior dominance is tested on each side separately.

### Activation decision

Default-active. The battery's own precedent runs the other
way (five detectors with zero measured production went
opt-in); this detector's measured production is the opposite
case: 100/100 planted recall, 0 false positives in 75 control
runs, and on the two corpus frames that motivated the issue
every fire is a verified recording artifact, two of them
documented top-codes. An opt-in detector would leave the
Adult ceiling mass unstated in exactly the default
single-turn profile the library is built around.

### Runtime

Added runtime, per-pass elapsed time from timed profiles
(release build, project build host, uncontended window; the
same host under multi-agent load inflates wall times
several-fold):

| Frame | Boundaries pass |
|---|---|
| Scale fixture 10,000 x 50 (38 numeric columns) | 19.6 ms |
| Scale fixture 1,000 x 10 | 1.0 ms |
| Adult (48,842 x 15) | 21.8 ms |
| Metro (48,204 x 9) | 25.5 ms |

The pass is one exact scan per numeric column plus one sort
of the column's values; bulk quartiles are read off the
sorted array with the pile's run skipped, so candidates cost
no re-sort. On the widest corpus frame (gse15258-expression,
86 rows x 5,006 columns, 5,000 numeric) the pass took 26.8 ms
of a 43.1 s profile.

### Corpus impact

Full-corpus runs (34 registered datasets): with the detector
in place and the base registry, 21 ok / 2 failed / 11 skipped
— the two failures are exactly the Adult and Metro
`max_findings` guards tripping on the new findings (62 -> 72
and 40 -> 43 selected). After the registry changes below, the
final run is 23 ok / 0 failed / 11 skipped, matching the base
run's shape.

A boundaries-only audit of every bundled dataset finds the
pass producing findings on 5 of the 23 runnable datasets:
Adult (10, above), Metro (5, above), Absenteeism at Work
(transportation_expense minimum 118 on 92 of 740 rows, 11.5x —
the minimum reported commute expense, a genuine floor pile;
employee_id maximum 36 on 34 rows, 5.7x — the most-recorded
employee is the highest-numbered one; the pass is role-blind
like the digits and sentinel scans, and the statement is
exact), Chinook (invoice total minimum 0.99 on 55 of 412 rows,
12.2x — the single-track basket at the price grid's minimum),
and Bank Marketing, where the audit caught the declining-count
cascade described above: after the two-sided correction the
column is quiet. The other 18 datasets produce nothing.

Registry changes, each individually justified in its note:

- Adult: the documented missed cases for the capital_gain
  top-code and the capital_loss mass points are promoted to
  `boundary_mass` finding expectations; the `max_findings`
  guard is re-baselined 62 -> 72.
- Metro: the documented missed case for the ten temp = 0.0
  rows is promoted to a `boundary_mass` finding expectation
  (distribution evidence; no impossibility asserted); the
  `max_findings` guard is re-baselined 40 -> 43.
