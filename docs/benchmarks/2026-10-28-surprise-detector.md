# Subset-conditional surprise detector (issue #31)

Date: 2026-10-28. Library change: new default-on pass `surprises`
(detector id `surprises`, category `intervals`, finding type
`extreme_value`); the detector count goes from 18 to 19.

## Why

The initial corpus run (2026-10-28-initial-corpus.md) found the
structures in the GritBot thyroid table — measured-flag dependencies,
concentration — but isolated none of the five documented cases. The
interestingness taxonomy (2026-10-28-interestingness-taxonomy.md)
ranked the missing family first: a value that is surprising within a
subset defined by other columns, reported with the subset and the
contrasting bulk. This entry records the detector built for that
family and its measured effect on the corpus.

## Method

For each numeric column, on each side of its distribution:

1. Candidate clusters of size 1 to 3 at the extreme end are tested
   against the remaining values (the bulk). A cluster qualifies when
   its inner edge lies beyond the bulk's Tukey fence
   (Q3 + 1.5 × IQR) and the gap between the cluster's inner edge and
   the bulk's edge is at least 1.0 × the bulk's central spread
   (p90 − p10). Cluster sizes are tested largest first: two adjacent
   extremes (ages 73 and 75 among pregnant patients) mask each other
   under a single-value gap test, because each one's nearest
   neighbour is the other.
2. The same test runs within each level of each categorical column
   with 2–32 levels (levels with at least 30 target values). A row
   already reported by the global test is not reported again for a
   subset.
3. Dual-scale guard: when every value in the scope is positive, the
   cluster must also be isolated on the log scale at half the gap
   bar. Without this, the thyroid TSH column (values 0.01–530, a
   log-scale assay) produced 11 findings in its ordinary tail across
   overlapping subsets; with it, TSH produces none, and the age
   findings below are unchanged.

Each finding names the extreme values and their row indices, the
bulk's edge, the gap, and the gap as a multiple of the central
spread. Findings are capped at 20 per profile.

## Measured effect: thyroid (3,772 rows, standard budget)

Before: 68 findings, 0 of the 5 documented GritBot cases isolated.
After: 76 findings, 8 of type `extreme_value`, of which:

- `age` = 455 at row 1364, global. Bulk edge 94; gap 361 = 7.2 × the
  central spread. This is GritBot case data:1365 (record 861).
- `age` = 75 and 73 at rows 2223 and 1609, within `pregnant = true`
  (53 rows). Bulk edge 42; gap 31 = 2.8 × the subset's central
  spread. These are GritBot cases data:2224 and data:1610, reported
  as one finding.
- `fti` = 395 within `pregnant = true` (row 1415, gap 5.6 × spread);
  `fti` = 349 within `sex = M` (row 1671, 2.0×); `tt4` = 430 twice
  within `on_thyroxine = false` (rows 1415, 743, 1.8×); `tt4` = 252
  within `sick = true` (row 2563, 1.3×); `fti` = 222 within
  `sick = true` (row 2563, 1.1×); `t3` = 4.6 within
  `i131_treatment = true` (row 701, 1.0×). These six have no external
  ground truth in the corpus; the last three sit at the bottom of
  the interestingness ranking.

The two remaining GritBot cases (data:2215, data:2216) are still
missed. Both are values that are unremarkable within every
single-column subset and surprising only against the level predicted
by the other assays — a residual form, not a subset form. GritBot
itself marked these as needing expert judgement.

Registry change: the two thyroid known-miss tripwires are promoted
to positive expectations, and the three cases above move from
`missed` to `found`. Corpus run after the change: every bundled
dataset's expectations pass (thyroid 8/8); finding counts elsewhere
change only where the new pass fires.

## Limits

- Conditioning is one column at a time. A value surprising only
  within a two-column subset (the data:2215 shape) is out of reach.
- Two separated extreme groups on the same side: only the outer
  group is tested as a cluster; an inner isolated value can be
  missed.
- Categorical targets are not covered; a surprising category within
  a subset (the soybean shape) belongs to the rule passes.
- On heavily skewed positive columns the log-scale guard is the
  only thing standing between the detector and tail noise; the
  1.0–1.3 × findings above show the border is a judgement call, and
  the gap multiple is printed in every claim so a reader can apply
  their own bar.
