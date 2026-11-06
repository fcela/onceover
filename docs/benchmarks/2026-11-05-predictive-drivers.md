# Predictive claims name their drivers

## Problem

A predictive finding's claim listed every input the screening
model split on. On Bike Sharing the statement for `temp` named
fourteen inputs — the record index, the date, the calendar
parts, and every weather column — although `atemp` alone carries
the signal. On a 5,000-row probe with `p_y = p_x^2 + noise` and
24 unrelated columns, the claim was

> p_y is predictable from [f_num0, f_num1, f_num3, f_num4,
> f_num5, f_num6, f_num7, f_num8, f_num9, f_num10, f_num11,
> f_num12, f_num13, f_num14, f_num15, f_num16, f_cat0, f_cat1,
> f_cat3, f_cat4, f_cat5, p_x] (holdout R² = 0.97)

listing 22 inputs, 21 of them noise by construction, while the
same evidence put 99.9% of the model's split gain on `p_x`. The
gain measurements needed to name the actual drivers already
existed in the evidence (`top_features`, `gain_shares`); the
claim did not use them. Two related defects: columns roled as
scaffolding (record counters, timestamps) were excluded as
targets but stayed in the predictor pool, so a claim could
restate row order; and the passes that consume the screen's
results (rules, decision tables, local regimes, the interaction
statistic) each took the top of the gain list again rather than
a validated shortlist.

## The driver rule

A driver is an input whose share of the fitted model's total
split gain is at least max(0.02, 0.10 × the top input's share),
taken in gain order; the top input always qualifies and the list
is capped at eight. The rule is a per-feature floor, not a
cumulative-share cutoff: on the probe above the true driver
holds 99.9% of the gain, and a cumulative rule set at 90% would
still name noise columns behind it; the floor is what keeps them
out. It is implemented as a pure function of the ranked
(position, gain) list and unit-tested at its boundaries (the
relative floor binding, the absolute floor binding, the top
feature surviving a spread-out gain distribution, the cap).

## What changed

- Claims name the drivers with their gain shares:
  `temp is predictable from atemp (99% of gain)`. The full input
  list stays in the evidence under `inputs`.
- When the driver set is a proper subset of the inputs, the pass
  refits the model on the drivers alone — same split, same
  configuration — and the claim reports the refit's holdout score
  first and the full model's second:
  `... (holdout R² = 0.98 using only these drivers; 0.99 using
  all inputs)`. Both scores are in the evidence (`reduced_metric`,
  `reduced_rounds` next to `metric`). Which findings appear is
  unchanged: the strength bars still apply to the full model.
- Scaffolding columns (frame roles index, identifier, timestamp,
  and timestamp parts of an assembled axis) are excluded from the
  predictor pool as well as the target set; the screen's coverage
  states the exclusion as targets and predictors.
- The rule and decision-table passes and the local-regime models
  take their feature pool from the target's drivers when the
  screen produced them (falling back to the previous top-gain
  list, then to all features, exactly as before), and the
  interaction pass orders its per-target shortlist drivers-first
  before truncating, so downstream statements start from the
  validated shortlist instead of re-expanding to the full pool.
- The predictor shortlist sort (which picks the fitting pool
  when a frame has more features than the predictor cap) now
  pairs each feature with its own information score before
  sorting. It previously indexed scores by feature position,
  which the new predictor exclusions invalidate: on the
  state-unemployment corpus frame (50+ features, a timestamp
  exclusion) the old indexing panicked with an out-of-bounds
  access, and the corpus run caught it. A regression test with
  55 features and an excluded column pins the fix. The same
  paired sort is applied in the retained-model fit, whose
  exclusion parameter had the same latent defect.
- The claim/payload consistency gate now checks predictive
  findings: driver names and shares, the reduced and full
  scores, and the categorical baseline must equal the payload's
  values, with mutation tests for each corruption.

## Probe results

Measured with `tests/test_predictive_drivers.py`'s frames (the
same generators run as standalone probes before and after):

| Probe | Before | After (full score; drivers-only score) |
|---|---|---|
| `p_y = p_x² + noise`, 17 numeric + 7 categorical noise columns, 5,000 rows | 22 inputs named, R² = 0.97 | `p_x` alone (100% of gain); 0.9725 full, 0.9728 drivers-only |
| `y = x1 + 2·x2 + x3 + noise`, 10 noise columns, 5,000 rows | 12 inputs named, R² = 0.98 | `x2` (66%), `x3` (17%), `x1` (17%); 0.9796 full, 0.9795 drivers-only |
| pure-noise target, 10 noise columns | no predictive finding | no predictive finding (unchanged) |

## Corpus claim census

Full corpus run (`benchmarks/corpus/run_benchmarks.py`) before
and after, report dumps compared finding by finding. Machine-
verified expectations pass in both runs (87/87). Screen
predictive findings selected into reports: 175 before, 162
after; on the 157 targets present in both runs, every claim
changed to the driver format. Per dataset (screen findings
selected before → after; claims changed on matched targets):

| Dataset | Screen findings | Claims changed |
|---|---|---|
| abalone | 8 → 8 | 8 |
| absenteeism-at-work | 17 → 17 | 17 |
| adult-census-income | 7 → 7 | 7 |
| adventureworks | 0 → 2 | 0 |
| bank-marketing | 6 → 6 | 6 |
| bike-sharing-hourly | 6 → 7 | 6 |
| chinook | 1 → 0 | 0 |
| congressional-voting | 14 → 14 | 14 |
| gse15258-expression | 0 → 0 | 0 |
| hospital-dha | 20 → 20 | 20 |
| lace-animals | 0 → 0 | 0 |
| lace-satellites | 12 → 12 | 12 |
| metro-interstate-traffic | 1 → 1 | 1 |
| nab-ec2-latency | 0 → 0 | 0 |
| nab-nyc-taxi | 0 → 0 | 0 |
| sec-edgar-company-facts | 5 → 5 | 5 |
| south-german-credit | 6 → 6 | 6 |
| soybean | 20 → 20 | 20 |
| splice | 1 → 1 | 1 |
| state-unemployment | 20 → 20 | 18 (2 targets rotated out of the capped report, 2 in) |
| state-unemployment-long | 0 → 0 | 0 |
| taiwanese-bankruptcy | 20 → 5 | 5 |
| thyroid | 11 → 11 | 11 |

The count differences on capped reports (taiwanese-bankruptcy,
state-unemployment, bike-sharing-hourly, adventureworks) are
selection rotations in reports limited to 100 findings, not
changes in what the screen finds: on taiwanese-bankruptcy the
after report admits 25 order-constraint findings that displace
screen findings from the capped list, and the corpus
expectations, which check detector-level behaviour, pass
unchanged. One difference is the exclusion itself, on an
uncapped report: chinook's only screen finding was
`billing_country is predictable from [invoice_id, customer_id,
billing_state, total] (holdout accuracy 0.79 vs 0.23 majority
baseline)` — the two identifier columns memorized each
customer's country. With them excluded as predictors the screen
finds nothing above the bar on that frame (coverage: "2
column(s) excluded as targets and predictors by frame role:
invoice_id, customer_id"), and the finding is gone. That is the
behaviour the issue asks for: a claim that restated row identity
no longer ships.

Three changed claims verified by hand against their payloads
(driver shares round to the stated percents; the two stated
scores equal the payload's `reduced_metric` and `metric` at two
decimals):

- bike-sharing-hourly, `temp`: atemp 0.9873 → "99% of gain";
  reduced 0.9799 → 0.98; full 0.9913 → 0.99.
- abalone, `diameter`: length 0.7853 → 79%, whole_weight
  0.1463 → 15%; reduced 0.9700 → 0.97; full 0.9699 → 0.97.
- adult-census-income, `education_num`: education 0.8881 →
  89%; reduced 0.9983 → 1.00; full 0.9815 → 0.98.
- hospital-dha, `HOSP_REIMB_RATIO`: 0.454 → 45%, 0.4343 → 43%,
  0.063 → 6%; reduced 0.9728 → 0.97; full 0.976 → 0.98.

## Quality battery

`benchmarks/quality/battery.py --full` re-run on this tree. The
plant recalls match the record committed with the battery
(`benchmarks/quality/results/2026-11-05-full.json`) exactly:
affine_identity 1.0, categorical_fd 1.0 (detector), weak and
moderate association 1.0, nonlinear_only 1.0, interaction_only
1.0, level_shift 1.0; the surprises plants 0.0; null frames
clean except dependence at 0.75 findings per frame, as recorded.
The association plants are dependence-detector territory; this
change does not touch those detectors, and their readings on
this tree are the recorded ones.

## Follow-up: a date shift the scaffolding exclusion priced out

The full Python suite on this tree failed one test that passes
on main: `test_date_residual_shift_reaches_python_report`. The
frame is 2,000 rows: `response = 3x` plus a +5 step halfway
along a daily date column, with `x ~ U(0,1)` the only numeric
predictor. The expected finding is the residual level shift
along the date, and the branch produced no `changes` finding
for the response at all.

Mechanism: the local-regimes pass fits the models whose
residuals the date-shift scan reads, and it admits a screened
target to those fits only when the screen's holdout score
reaches 0.2. On main the timestamp was in the predictor pool,
so the step itself made the response predictable and the target
was admitted. With timestamps excluded from the pool — the
scaffolding change above, which stands — the response's only
predictor is `x`, which carries the slope but not the step, and
its screen score falls below the bar. No fit, so no residuals,
so the shift scan never ran. The scan itself was intact: the
same construction with a step small enough that `x` alone
clears the bar reports the shift on the unfixed tree.

Fix, in the regimes pass: when the frame has a date column,
screened numeric targets below the bar are fitted as shift-only
candidates — the baseline fit and the residual-shift scan run,
and the piecewise-regime finding stays behind the bar. A screen
result exists only when the screen measured a positive holdout
score, so a shift-only candidate always has some measured
structure behind it; whether anything is reported is then
decided by the scan's own train and holdout gain bars.
Shift-only candidates are ordered by screen score, truncated to
the same target cap as full candidates, and the pass's coverage
states the count ("1 further screened target(s) below the
predictability bar scanned for date residual shifts only").
Three unit tests pin the behaviour: a below-bar target with a
date column and a step reports exactly the shift finding (with
the breakpoint and holdout error reduction the Python test
expects); the same target with no date column reports nothing;
with a date column but no step it reports nothing.

Corpus check on the fixed tree: the expectations run is
unchanged (23 ok, 11 skipped, machine-verified expectations
87/87), and the full report dumps match the dumps from the tree
before this fix on all 23 profiled datasets — every report
byte-identical, so the fix restores the priced-out shift scan
without moving any corpus finding.
