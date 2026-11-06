# Simpson check: reversal qualification for strong association statements

## What changed

The two tested-association screens (the numeric dependence
screen in dep.rs and the pair screen in association.rs) now run
a reversal check over their strong-tier findings, implemented
in a new simpson module in onceover-mines. For each strong pair
with a signed effect, candidate stratifying columns are taken
from the same evaluated rows the screen used — categorical
table columns for the numeric screen, nominal encoded features
for the pair screen — and the first stratifier (in column
order) whose every usable stratum reverses the pooled sign is
attached to the finding as a claim qualification plus a
`simpson` evidence payload. Weak-tier findings are not checked.
The finding set, tiers, and the associations table are
otherwise unchanged; no finding type, facet, or schema change
is involved.

Implementation notes against the protocol as registered:

- Stratifiers are the categorical columns of the evaluated
  frame. The frame role map is not threaded into these screens,
  so "dimension-roled" in the design is realized as:
  frame-role *withheld* (scaffold) columns are excluded, as
  they already are from findings, and every other categorical
  column with 2–8 observed levels is a candidate.

## Protocol — registered before the corpus run

The tested-associations change (docs/benchmarks/2026-11-05-tested-associations.md)
records in its limits that strong statements remain unconditional:
no Simpson-type reversal check over stratifying dimensions is
included, so a strong pair statement can still reverse inside a
subgroup. This change adds that check. The criterion below was
fixed before any corpus measurement under it.

### Scope

- Only strong-tier association statements are checked. They are
  the unconditional claims; weak-tier statements are already
  hedged by their tier.
- Only pairs with a signed effect are checked:
  - *Numeric pairs* — Pearson or Spearman, whichever the screen
    measured larger in absolute value (Pearson on a tie). The
    check applies only when that component's absolute value is
    at least 0.3: below it, the statement's strength comes from
    the unsigned ξ component and the pair carries no material
    direction to reverse.
  - *Mixed pairs whose nominal side has exactly two observed
    levels* — the point-biserial correlation between the numeric
    column and the indicator of the nominal side's second level
    in encoded order. For two groups the correlation ratio η
    equals |point-biserial r|, so the strong bar on η already
    bounds the signed effect.
  - *Nominal pairs with two observed levels on both sides* —
    the log odds ratio of the 2×2 table in encoded level order,
    with 0.5 added to all four cells when any cell is zero,
    applied identically to the pooled table and to every
    stratum table.
- Unsigned effects (Cramér's V beyond the 2×2 case, η over more
  than two levels, Chatterjee's ξ) have no direction to reverse.
  Pairs with no signed reading are not checked; this exclusion
  is stated in docs/findings.md rather than worked around.

### Stratifiers

- A stratifying column is a categorical column (string, enum,
  or boolean) that is not one of the pair's columns and is not
  withheld from findings by frame role, with 2–8 observed
  levels over the rows the screen evaluated.
- A stratum is *usable* when it holds at least 30 pair-complete
  rows and the signed effect is defined on them (for a
  correlation, both sides vary; for an odds ratio, both levels
  of both columns occur in the stratum).
- A stratifier is evaluated for a pair only when at least two
  strata are usable and the usable strata together cover at
  least half of the pair-complete rows.

### Reversal criterion

- A stratifier qualifies a pair when **every** usable stratum's
  signed effect has the opposite sign to the pooled signed
  effect. A stratum effect of exactly zero is not opposite.
  This is the textbook Simpson form: the pooled direction is
  wrong inside every subgroup large enough to read, not merely
  on a weighted average.
- The reported within effect combines the usable strata:
  Fisher-z average weighted by (n − 3) for correlations,
  inverse-variance (Woolf) weighting for log odds ratios.
  Under the criterion it necessarily carries the reversed sign;
  it is reported so the reader sees the within-stratum
  magnitude, not only the direction.
- When several stratifiers qualify a pair, the first in column
  order is the one attached.

### Outcome form

- The qualification is appended to the finding's claim: the
  stratifier is named, the pooled effect and the range of
  within-stratum effects are stated with their signs, and the
  combined within effect is stated.
- The finding's evidence gains a `simpson` payload: the
  statistic, the pooled effect, the combined within effect,
  the per-stratum levels with row counts and effects, the
  usable and total stratum counts, and the share of
  pair-complete rows the usable strata cover. For binary-coded
  statistics the payload also names the level coded as 1, so
  the direction is interpretable.
- The finding is never removed, demoted, or re-tiered. The
  associations table is unchanged.

### Work bound

- At most 24 stratifying columns are considered per pair and
  at most 400 pair×stratifier evaluations run per screen. When
  a bound stops a check, the screen's coverage entry discloses
  it.

### Acceptance

- Planted Simpson frames (pooled effect positive; within-stratum
  effect negative for every stratum of a planted dimension),
  across seeds and across the three signed pair kinds: the
  strong statement carries the qualification in every seed.
- Homogeneous controls (within-stratum effects of the same sign
  as the pooled effect): no qualification is attached.
- Null frames from the quality battery: no strong statement
  gains a qualification.
- Corpus run over the bundled registry: every strong statement
  that gains a qualification is enumerated, and each is
  recomputed independently (NumPy) to confirm a genuine
  measured reversal.

## Results

All runs below are on the final tree of this change, standard
budget unless a dataset's registry entry says otherwise.

### Planted frames

Each plant was profiled end to end and the qualification was
verified independently with NumPy: pooled and per-stratum
effects recomputed from the raw arrays must agree with the
payload's signs and magnitudes (within 0.05).

- Numeric plants (pooled Pearson/Spearman positive, both
  strata negative), 10 seeds: the strong statement carries the
  qualification in 10/10 seeds.
- Mixed plants (binary nominal side, point-biserial sign
  planted), 10 seeds: 10/10 qualified.
- Nominal 2×2 plants (rates drawn per seed around a fixed
  reversal design, log odds ratio planted), 10 seeds: 10/10
  qualified.
- Homogeneous controls (numeric plant with the within-stratum
  slope of the pooled sign), 5 seeds: the pair is still stated
  strong, and no finding in any report carries a qualification.

The same three plants and the homogeneous control are in the
test suite as regression tests (tests/test_associations.py).

### Null frames

The quality battery's null frames (eight independent noise
columns), 12 seeds: no strong statement on any frame (0 strong
findings in total), and no qualification. The check cannot
fire without a real strong pair.

### Corpus

The bundled registry: 23 datasets profiled, 11 test-only
entries skipped (their data files are not present locally).
The profiles carry 158 strong statistical statements. An
eligibility tally over the same profiles counts 126 of the
158 carrying a signed effect in the check's scope (a numeric
pair whose larger correlation reaches 0.3, a 2×2 nominal pair,
or a mixed pair with a two-level nominal side), and 64 of
those 126 also had at least one candidate stratifier (another
categorical column with 2–8 distinct values).

No corpus statement gained a qualification: none of the 64
met the reversal criterion. The criterion is the textbook
form — reversal inside *every* usable stratum, not a
weighted-average sign flip — and on this corpus no strong
pair reverses that completely. The enumeration is the full
acceptance check for this leg: every strong statement that
gained a qualification (none) would have been recomputed
independently from the raw frame before being counted as a
genuine reversal.

## Amendment, 6 November 2026: a significance bar on the within effect

The criterion above requires only that every usable stratum's
effect have the sign opposite to the pooled effect. The null
frames above never produced a strong pair, so they did not test
the case where the pooled association is strong and the
within-stratum association is zero: x and y share a group mean
shift and are independent inside each group. There the
within-stratum effects are noise, and with two strata both come
out opposite to the pooled sign about a quarter of the time.

Measured on that frame (two levels, 2,000 rows, x and y each
shifted by 2 in level B, unit-variance independent noise), 20
seeds: 6 of 20 strong statements carried "the direction
reverses within every level of g", with within-level Spearman
rho between −0.03 and −0.06, about one standard error from zero.

The criterion now adds: the combined within effect must differ
from zero at |z| ≥ 3.84, with z = (weighted mean on the
combining scale) × sqrt(sum of weights). The weights are the
inverse variances already used for the combination: n − 3 for
Pearson and point-biserial r (Fisher z), (n − 3) / 1.06 for
Spearman's rho, and the inverse Woolf variance for log odds
ratios. 3.84 is the two-sided normal quantile at 0.05 / 400, a
Bonferroni bar over the screen's evaluation budget. The payload
records the z as `within_z`.

Results on the amended criterion:

- The null-within frame above, 20 seeds: 0 qualifications.
- Numeric plants, 10 seeds: 10/10 qualified. Mixed plants, 10
  seeds: 10/10 qualified. The nominal 2×2 plant and the
  homogeneous control behave as before (tests/test_associations.py).
- The amendment can only withdraw qualifications. The corpus had
  none before it (Results above), so it has none after it. The
  corpus runner's machine-verified results are unchanged (84/89
  on this tree, the five failures being the FD expectations that
  PR #183 updates).
