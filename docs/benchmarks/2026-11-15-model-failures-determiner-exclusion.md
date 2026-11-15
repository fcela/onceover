# Model-failure screen: columns that determine the target are excluded before fitting

Date: 2026-11-15. Change: the `model_failures` screen excludes
columns that determine the designated target exactly from its
model inputs and its slice variables, and names the excluded
columns in its coverage entry (issue #306).

## Problem

The model-failure screen cross-fits a model of the target and
searches the residuals for failure slices. When a column that
determines the target exactly is left in the frame — the column
a derived target was built from, or one derived from the
outcome — the screen fitted with it as an input, reached a
perfect out-of-fold score, and spent its statement budget on
slices of that column. Within an all-events slice the error is
one minus the mean prediction by construction, so the
statements restated the leak the leakage screen in the same run
had just flagged, instead of locating a failure.

## Change

Before fitting, the screen computes the columns that determine
the target exactly, using the leakage screen's determination
test — the per-column strength evaluation in the leakage module
is now shared by both screens, so the test exists in one place:

- categorical target: zero functional-dependence violations,
  with the guard that the predictor's groups average at least
  four rows (a near-unique column does not determine vacuously);
- numeric target, categorical predictor: the target is constant
  within every level (zero within-group variance);
- numeric target, numeric predictor: Chatterjee's xi at 0.999
  or above.

The determining columns are added to the exclusions the caller
already passes (frame-role and axis columns) before the
cross-fit and before slice-variable selection, and the coverage
entry names them: "1 column(s) excluded from the model inputs
and the slice variables because each determines the target
exactly: 'results'". No column, file, or target is special-cased:
any column that determines any target exactly is excluded. The
leakage screen's own output is unchanged (its coverage text on
both frames below is identical before and after the change, and
its test suite passes unmodified).

## Protocol

Two frames, each run with `profile(df, budget="standard",
target=...)` on the build before the change (main as of
2026-11-15) and on the build with the change.

1. **Chicago Food Inspections** (Socrata dataset 4ijn-s7e5;
   the 50,000 most recent inspections, as measured for the
   issue): the frame is the Pass/Fail subset — 35,516 rows,
   8,840 of them Fail — with a binary target `failed` derived
   from the `results` column (1 for Fail, 0 for Pass) and all
   17 original columns left in the frame.
2. **Synthetic fixture** (5,000 rows, fixed seed): `failed`
   derived exactly from a `results` column left in the frame;
   outside one slice (`g = 'wild'`, 20% of rows) the event is
   nearly determined by a numeric column `a`, and inside the
   slice it is a coin flip — a planted failure slice on the
   remaining columns. A second fixture (4,000 rows) keeps only
   the determining column and noise columns.

## Results

### Inspections frame

| | Before | After |
|---|---|---|
| Out-of-fold AUC | 1 (exactly) | 0.600 |
| Overall mean absolute error | 0.146 | 0.293 |
| Statements stated | 20 (the cap) | 12 |
| Statements conditioned on `results` | 20 of 20 | 0 of 12 |
| Slice mean absolute errors | 0.451421–0.451441 (one value to the third decimal) | 0.111–0.498 |
| Coverage names the exclusion | no | yes: `'results'` |

After the change the stated slices are on the remaining
columns: complaint inspections carry the largest errors
(accuracy form), and licence re-inspections show one-sided
errors (bias form). The leakage screen in the same run states
the determination before and after, unchanged.

### Synthetic fixtures

Planted-slice fixture: out-of-fold AUC 1 → 0.956; statements
10 → 5, with the condition columns changing from
{`a`, `g`, `results`} to {`a`, `g`}; the planted slice is
stated in the accuracy form — within `g = 'wild'` the model's
mean absolute probability error is 0.500 against 0.233 overall.
Only-source fixture: out-of-fold AUC 1 → 0.496, at or below the
0.55 chance floor, so the pass records coverage only (naming
the exclusion) and states nothing — the existing chance gate
absorbs the case where nothing learnable remains.

## Regression tests

`tests/test_model_failures.py` gains three tests, each failing
on the build before the change and passing after: the planted-
slice fixture above (exclusion named, no statement conditioned
on the determining column, the genuine slice still stated, the
sibling leakage statement intact), the only-source fixture
(coverage-only through the chance gate, exclusion named), and a
numeric-target fixture whose categorical column determines the
target by the between-group test (40 levels, target constant
within each; exclusion named, coverage-only).
