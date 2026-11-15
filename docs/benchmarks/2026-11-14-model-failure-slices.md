# Model-failure slices: cross-fitted residual slice search

Date: 2026-11-14. Change: the `model_failures` screener and the
`model_failure` finding type (issue #299).

## Problem

After a model is fitted to a target, nothing in the report states
where the model fails. A model can post a good overall score
while its errors are large, or one-sided, inside one conjunction
of conditions. The `slice_shifts` detector finds slices where a
column's distribution shifts; this change runs the same search
against a fitted model's out-of-fold residuals. The statement is
different: not "the distribution differs here" but "the model is
wrong here, by this much, against its overall error of that
much".

## Protocol

The pass is target-gated and outside the default detector set,
in the family of the target-shape and leakage passes. It fits
the house GBM (the model the predictability screen fits) to the
designated target with 5-fold cross-fitting over a seeded
shuffle of the rows with a present target, so every residual is
out-of-fold; in-sample residuals are never used. Each fold's
predictor shortlist is computed on that fold's training rows
only, and each fold model early-stops on an internal split of
its training pool. The pooled modelled rows are capped at four
times the encoding row cap.

The residuals feed the existing slice search unchanged:
conjunctions of categorical levels and numeric thresholds,
depth at most 2, a search half and a confirmation half,
Mann–Whitney confirmation tests, and Benjamini–Hochberg control
over the candidate family. Two failure forms are searched:

- **bias**: the slice's mean signed residual against the overall
  mean residual. For a binary target the statement reads as the
  slice's event rate against its mean predicted probability,
  with the overall pair as the baseline.
- **accuracy**: the slice's mean absolute residual against the
  overall mean absolute residual.

Every statement names the slice, its size, its error, and the
model's overall error. Effect-size floors, frozen before the
fixtures passed and unchanged since:

- Chance gate: no statements when the pooled out-of-fold fit is
  at or near chance — R² ≤ 0.01 for a numeric target, AUC
  ≤ 0.55 for a binary target. The coverage entry records the
  measured skill either way.
- Bias: the search-half slice mean residual differs from the
  overall mean residual by at least 0.5 × the overall MAE.
  Confirmation requires the same direction, at least 0.3 × the
  overall MAE, and at least half of the search-half effect
  retained.
- Accuracy: the search-half slice MAE reaches 1.5 × the overall
  MAE. Confirmation requires a ratio of at least 1.25 and at
  least half of the search-half excess retained.
- Confirmation q ≤ 0.05 (Benjamini–Hochberg over all candidate
  slice × form pairs). Support of at least max(50, 1% of
  modelled rows) in the full modelled set and 25 in each half.
  At most 20 statements, 10 per form. Within a form, a statement
  whose row set is at least 90% Jaccard-similar to a stronger
  stated slice is demoted as a restatement.

Scope: numeric and binary categorical targets. A categorical
target with more than two classes records that in coverage and
states nothing.

## Fixture results

Rust unit tests drive the search with synthetic residuals: a
planted bias slice fires, a planted accuracy slice fires,
uniform residuals are silent, and shuffled residuals are silent.
A separate test cross-fits a binary target determined by its
predictors and reaches out-of-fold AUC 0.972.

Python fixtures (`tests/test_model_failures.py`, 8 tests):

- A constant +4 shift inside the conjunction g = 'D' and
  z > 0.90 (2.5% of rows) is stated in the bias form, naming
  the planted conjunction; the depth-1 parent (g = 'D' alone)
  is not stated. A larger cell (25% of rows) with a +3 shift is
  fitted completely by the model and correctly states nothing.
- A sign-reversed effect inside g = 'D' and z > 0.75 is stated
  in the accuracy form, naming the planted conjunction (slice
  MAE 0.485 against 0.270 overall, 1.80×).
- A binary target with one noisy group (the event a coin flip
  there, determined elsewhere) is stated in the accuracy form
  naming the noisy group, with slice MAE above 1.5 × overall.
- Controls: a model that fits uniformly states nothing; a
  pure-noise numeric target states nothing and its coverage
  records the at-or-below-floor R²; an uninformative binary
  target states nothing with the chance-level AUC in coverage.
- With no target designated, the pass is skipped in coverage;
  explicit selection without a target also skips. A multiclass
  target records the out-of-scope note and states nothing.

## Real-data evaluation: Adult (UCI), target income

Frame: the corpus copy of UCI Adult, 48,842 rows; the pass
modelled a 20,000-row sample under the encoding cap.
Out-of-fold AUC 0.904 (majority baseline 0.763); overall mean
absolute probability error 0.173. The search examined 747
supported slices (1,494 slice × form candidates); 194 cleared
the search bars, 168 confirmed at q ≤ 0.05, and 18 statements
were stated.

Each statement was verified independently: slice membership
and size recounted from the raw frame, and a 5-fold cross-fit
of scikit-learn's HistGradientBoostingClassifier on all 48,842
rows produced its own out-of-fold residuals (overall MAE
0.176).

- All 10 accuracy statements reproduce: the independent model's
  slice MAE is 2.01–2.16× its overall MAE on every named slice
  (stated ratios 2.18–2.59×; the independent model is
  higher-capacity, so its ratios run lower). The recurring
  family is married and husband rows in Tech-support,
  Adm-clerical, and associate-degree slices, which carry about
  twice the model's error.
- All 8 bias statements have their event rates confirmed
  against the raw frame (the married Tech-support slice:
  '>50K' occurs for 58.0% of the modelled rows and 58.5% over
  the full frame). The predicted-probability gaps are
  properties of the house model: the higher-capacity
  independent model closes most of them (58.5% occurring
  against 56.5% predicted on that slice). That is the
  statement's content: at the house model's settings the
  interaction is not reached and the residuals are one-sided
  there.
- Two Adult columns are near-duplicates (marital_status and
  relationship), so several statements come in pairs whose row
  sets overlap just under the 90% restatement threshold. Both
  members of each pair are kept.

## Real-data evaluation: hotel bookings, target is_canceled

Frame: 119,390 bookings; the pass modelled a 20,000-row
sample. The house cross-fit ranks almost perfectly (out-of-fold
AUC 1.000 to three decimals; the reservation_status column
nearly determines the outcome), but its probabilities are
shrunk: overall mean absolute error 0.209, because early
stopping on holdout accuracy stops once the ranking is right,
long before the probabilities are calibrated. The pass stated
11 slices; the report's 100-finding cap admitted 3 of them,
all in the bias form and all sharing one condition:

- arrival_date_year = 2015 and lead_time > 263 (473 modelled
  rows): '1' occurs for 85.8%, mean predicted probability
  56.2%.
- market_segment = 'Groups' and lead_time > 263 (1,019 rows):
  78.6% occurring, 52.5% predicted.
- total_of_special_requests = 0 and lead_time > 263 (1,544
  rows): 77.8% occurring, 52.1% predicted.

Overall: 37.1% occurring, 31.0% predicted.

Verification: the slice event rates were recounted exactly
from the raw frame — 85.5% over 2,865 rows, 79.4% over 6,074
rows, 78.4% over 9,307 rows, against 37.0% over all 119,390
rows. An independent 5-fold cross-fit (scikit-learn
HistGradientBoosting, 80 iterations, the agent and company id
columns dropped) reproduces the event rates and, being
higher-capacity, calibrates to them almost exactly (85.4%
predicted on the first slice). The under-prediction is the
house model's, and it concentrates where bookings are made far
ahead: long-lead bookings cancel at about twice the overall
rate and the house model's probabilities do not follow.

The displacement of the other 8 stated slices at the report
cap is the selection problem tracked separately (issue #280);
it is recorded here because it bounds what a reader of the
default report sees from this pass on finding-dense frames.

## Corpus

`benchmarks/corpus/run_benchmarks.py` exits 1 with the three
standing failures (thyroid `pregnant`; adventureworks
`revision_number`; bike-sharing-hourly, the (cnt, casual)
grouped family) and 90/93 machine-verified expectations
passing — unchanged from the baseline. The pass is
target-gated, so default corpus profiles gain only a skipped
coverage entry for `model_failures`; no expectation changed.

## Gate

In CI order: release metadata check passes; the release tests
pass 6; `cargo fmt --all -- --check` clean; clippy over the
workspace with warnings denied is clean; the workspace test
run passes 422 tests with 0 failures (the onceover-mines suite
is 237, including 5 new tests for the search and the
cross-fit); the Python suite passes 650 with 0 failures
(including the 8 new fixtures). The strict book build exits 0.

Environment note: in the sandbox where this change was built,
the workspace test build repeatedly failed to link under a
disk quota (the linker was killed writing the largest test
binaries), so the passing run used a reduced-debug local build
profile (CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0). That
profile changes build artifacts only, not test semantics; CI
runs the default profile.
