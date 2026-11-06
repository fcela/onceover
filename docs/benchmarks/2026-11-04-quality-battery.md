# Planted-signal discovery quality: first battery

On frames built to contain exactly one known signal each, the
profiler reports the planted signal every time for five of nine
plants: an exact affine identity, an exact categorical mapping, a
nonlinear (y = x²) relation, an interaction-only pattern, and a
level shift — median interestingness ranks 4, 20, 2, 1, and 3. It
reports nothing for a planted correlation of 0.15 or of 0.4 at
4,000 rows: both the dependence screen and the predictability
screen set their reporting bars above moderate correlation (their
coverage lines read "0 strong pairwise" and "0 predictable" on
those frames). For the two slice plants — six extreme values inside
one slice, three exception rows inside one slice — it reports the
planted values on every seed but under other groupings, never under
the planted slice context; a permuted control, where the same
values land in slice contexts by chance as singletons, is reported
under the slice context on 11 of 12 (extremes) and 12 of 12
(outliers) seeds. Across 12 null frames and every planted frame,
no family ever reported a signal involving only noise columns:
where the battery can define a false positive, it measured none.

This is the first run of the battery specified in the evaluation
issue (#115). Protocol, per-plant numbers, and the regression
tolerances proposed from these measurements follow. The full
machine-readable record, with provenance, is
`benchmarks/quality/results/2026-11-04-full.json`; the battery
itself is `benchmarks/quality/battery.py` (fast subset wired into
CI as `tests/test_quality_battery.py`). Two independent full runs
produced identical aggregates.

## Protocol

Nine plants (benchmarks/quality/plants.py), each verified on the
generated frame with NumPy before profiling — the truth never
comes from the library. Twelve seeds per plant and per variant:
response permuted (signal destroyed),
5% MCAR missingness in the signal columns, column order reversed,
and a wide frame with the signal beyond column 64. Twelve null
frames (independent columns only) give the false-positive
accounting: any family-matching finding there is a false positive
by construction.

A finding matches a plant when its type belongs to the plant's
family and its subject involves all the planted signal columns.
Detector recall uses a profile restricted to the family's
detectors; post-selection recall uses the default profile's
selected findings; rank orders the selected findings by the
report's `interestingness`, descending. Target recall (secondary)
counts a family-type finding that reports the planted response
column in any context.

## Results (12 seeds, base variant)

| plant | detector | selected | top-10 | top-25 | median rank | target |
|---|---|---|---|---|---|---|
| affine_identity | 1.00 | 1.00 | 1.00 | 1.00 | 4 | 1.00 |
| categorical_fd | 1.00 | 1.00 | 0.00 | 1.00 | 20 | 1.00 |
| weak_association (ρ=0.15) | 0.00 | 0.00 | 0.00 | 0.00 | — | 0.00 |
| association_moderate (ρ=0.4) | 0.00 | 0.00 | 0.00 | 0.00 | — | 0.00 |
| nonlinear_only | 1.00 | 1.00 | 1.00 | 1.00 | 2 | 1.00 |
| interaction_only | 1.00 | 1.00 | 1.00 | 1.00 | 1 | 1.00 |
| conditional_extremes | 0.00 | 0.00 | 0.00 | 0.00 | — | 1.00 |
| rare_slice_outlier | 0.00 | 0.00 | 0.00 | 0.00 | — | 1.00 |
| level_shift | 1.00 | 1.00 | 1.00 | 1.00 | 3 | 1.00 |

Variants (12 seeds):

- **Permuted**: recall collapses to 0.00 for
  affine_identity, categorical_fd, nonlinear_only,
  interaction_only, and both association plants. The two slice
  plants invert: conditional_extremes strict recall 0.92 and
  rare_slice_outlier 1.00 on permuted frames, against 0.00 on the
  planted frames — after permutation the extreme/exception values
  sit in the slice as singletons, which the pass does report under
  the slice context; the planted configuration (a six-value
  cluster, a three-exception slice) it does not. Target recall on
  permuted slice frames is 1.00.
- **Missingness (5% MCAR)**: affine_identity still 1.00 (the
  identity is verified on the rows that remain); weak_association
  still 0.00; conditional_extremes strict still 0.00, target 1.00.
- **Wide (signal beyond column 64)**: affine_identity and
  categorical_fd both 1.00 detector and selected — consistent with
  the 2M×200 scale check's late-column recovery.
- **Column order reversed**: identical recalls to base on every
  plant.

Precision, where truth is defined: on null frames the five
families produced zero matching findings in 12 frames (mean and
max 0 per frame). On planted frames, family-type findings
involving no signal column averaged 0.0 for every plant. The
surprises slice findings under noise contexts are not counted as
false positives: the values they report are genuinely extreme;
what differs is the grouping they are attributed to, which the
strict/target recall split records.

## What the battery cannot yet say

- The dependence bar is located only within (0.4, ~1.0): the two
  association plants bracket it from below and the deterministic
  plants from above. A strength ladder is the obvious next plant
  family.
- Slice-context attribution is measured for two configurations
  (one numeric cluster, one categorical exception set). Whether
  the inversion generalizes — singleton contexts reported,
  clustered contexts not — needs more slice shapes before it
  reads as a property of the pass rather than of these plants.
- Ranks are interestingness ranks within one profile. Nothing here
  measures usefulness of the surrounding findings, only of the
  planted signal's placement.

## Baselines and proposed regression tolerances

Recorded baselines are the table above (record:
`benchmarks/quality/results/2026-11-04-full.json`, onceover
0.1.3, commit 613303d4). Proposed tolerances for future runs,
from these measurements — every non-zero recall above is 1.00
with no seed variance, and every zero is 0.00 with no seed
variance, so the tolerances are set by the measurement's
granularity (one seed of twelve), not by observed spread:

- A plant at baseline 1.00 regressing below 11/12 seeds on
  detector or post-selection recall is a regression.
- A plant at baseline 0.00 rising above 0/12 is a capability
  change: it should fail the fast-subset pin (as
  `tests/test_quality_battery.py` does for the dependence bar)
  until the battery doc and baselines are updated in the same
  change — the corpus known-miss promotion rule, applied here.
- Null false positives: baseline 0 per family per frame; any
  family finding on a null frame is a regression.
- Top-25 recall at baseline 1.00 (all five detected plants) must
  not fall below 11/12; categorical_fd's median rank of 20 leaves
  it the least headroom, and a rank regression there is worth
  catching before it becomes a recall regression.

These tolerances are proposals recorded with the baselines; the
issue stays open until they are reviewed.

## Addendum, 2026-11-05: the dependence bar is now a test

The association screens no longer report against fixed effect
bars. Each screened pair carries a p-value (numeric pairs:
Bonferroni combination over the Pearson, Spearman, and Chatterjee
ξ components; nominal pairs: a G-test; mixed pairs: a one-way
F-test), Benjamini–Hochberg adjusted over the pairs tested in the
run, and statements split into a strong tier (adjusted q ≤ 0.05
and the previous effect bars) and a weak tier (adjusted q ≤ 0.05,
below the effect bars). The full measurement is recorded in
`docs/benchmarks/2026-11-05-tested-associations.md`; the run is
`benchmarks/quality/results/2026-11-05-full.json` (onceover
0.1.3, tree at main 9dd66df9 plus the tested-associations change).

Capability change, recorded per the promotion rule above: both
dependence plants move from baseline 0.00 to 1.00.

| plant | detector | selected | stated | top-25 | median rank |
|---|---|---|---|---|---|
| weak_association (ρ=0.15) | 1.00 | 1.00 | 1.00 | 1.00 | 2 |
| association_moderate (ρ=0.4) | 1.00 | 1.00 | 1.00 | 1.00 | 2 |

Both are stated as weak-tier associations (their effects are
below the strong bars). The new baselines are 1.00, judged by the
same 11/12 rule as the other plants. The fast-subset pin in
`tests/test_quality_battery.py` now asserts the stated recall on
seeds 0 and 1 instead of the old not-reported pin.

One bookkeeping change in the same run, not a recall change:
categorical_fd's statement is grouped under its relationship
family's lead on all 12 seeds (schema 0.6 grouped statements), so
its selected recall reads 0.00 while its stated recall — selected
or grouped — stays 1.00 with detector recall 1.00. This is the
first full battery recorded since grouped statements landed;
stated recall is the comparable figure across the two runs.

The null tolerance for the dependence family is revised, because
the property it protected has changed form. A tested screen at
q = 0.05 states a weak association on a purely null frame by
design, at a rate the correction budgets; "any finding is a
regression" would pin the screen to never use its stated error
rate. The recorded run states 3 weak-tier statistical findings
across the 12 null frames — one numeric pair (ξ component,
effect 0.029, q = 0.042) and two mixed pairs (η = 0.057,
q = 0.036; η = 0.066, q = 0.003) — and no strong-tier statement
on any null frame. Counted per tested pair, that is 3
discoveries in 336 tested null pairs (0.9%) against the 5%
nominal level; counted per screen family, 1 of 12 frames
(numeric) and 2 of 12 frames (nominal/mixed) carried a
discovery, consistent with the nominal rate at this sample size
(the probability of two or more discovery frames in 12 at an
exact 5% rate is about 0.12). The other four families stay at
zero findings on every null frame, under the original tolerance.
The pin now asserts the rate form: no strong-tier or predictive
statement on any of the 12 null frames, at most 4 weak-tier
statistical findings across them, and zero dependency and break
findings. A calibration failure would state dozens of weak
findings, not five, so the bound keeps its signal.
