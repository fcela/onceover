# Leakage signatures against a designated target (issue #282)

## The problem

When a target is designated, the profiler screens predictors
jointly (the GBM screen) and decomposes shapes, but it never
states the single most consequential fact a practitioner checks
first: whether one column on its own nearly determines the
target. That shape — a column that is the target restated,
derived from it, or recorded after the outcome — invalidates
every downstream model, and on its own it reads as exciting
rather than suspicious. The 2026-11-13 hotel-bookings
demonstration showed the gap in the current build: the report
stated "`is_canceled` is predictable from `reservation_status`
(holdout accuracy 1.00)" as an ordinary predictive dependency,
with no indication that `reservation_status` is recorded after
the outcome and restates it.

## Design

A target-gated detector, `leakage`, beside `target_shapes`. It
runs only when a target is designated, evaluates every other
column's univariate strength against the target on complete
cases (minimum 100), and ranks the field. Strength scales, all
on 0..1 and chance-corrected:

- Categorical target: the reliable fraction of information
  (RFI) from the fixed-margin FD machinery in `onceover-deps` —
  the share of the target's entropy the column accounts for
  beyond what the permutation model expects at the same margins.
- Numeric target, numeric column: Chatterjee's xi in the target
  direction.
- Numeric target, categorical column: the between-group share
  of the target's variance, degrees-of-freedom adjusted
  (adjusted eta-squared).

## Bars, frozen before the corpus measurement below

- A column that determines the target exactly on the evaluated
  rows is stated, whatever the field looks like (at most three
  such columns).
- Otherwise the strongest column is stated only when its
  strength is at least 0.80 **and** at least 1.5 times the
  runner-up's: leakage is a column out of line with the field,
  not a strong field.
- The statement names the signature and the mundane explanations
  (derived column, post-outcome recording) and never asserts
  leakage. The coverage entry states the outcome either way,
  with the strongest column, its strength, and the runner-up.

Fixtures (in `tests/test_leakage_signatures.py`): a planted
derived column (the target with 2% of labels flipped) fires with
the column named; a post-outcome column that determines the
target exactly fires on the determination path; a strong but
legitimate field (five predictors each carrying a moderate share
of a majority-vote target) stays silent; a shuffled target stays
silent; a numeric target with a near-copy column fires on the xi
path; no designated target runs no screen.

## Corpus measurement

Measured 2026-11-13 with the shipped screen, standard budget.
The top five per dataset were obtained by re-running the screen
with the strongest column removed at each step, so every number
below is the shipped code's own output.

**Adult, target `income` (categorical).** No statement.
Strongest: `relationship` 0.208, `marital_status` 0.198,
`capital_gain` 0.148, `age` 0.122, `education` 0.116 — a
legitimate field, far below the bar.

**Hotel bookings, target `is_canceled` (categorical).** One
statement: `reservation_status` determines the target exactly
on the evaluated rows (strength 1.000; next strongest `company`
at 0.212, then `deposit_type` 0.199). This is the demonstration
case from the problem section, now named as a leakage signature
instead of passing as an ordinary predictive dependency.

**Taiwanese Bankruptcy, target `bankrupt` (categorical).** No
statement. Strongest: `Interest Expense Ratio` 0.259,
`Borrowing dependency` 0.224, `Persistent EPS in the Last Four
Seasons` 0.216, `Non-industry income and expenditure/revenue`
0.198, `Long-term Liability to Current Assets` 0.169. The first
measurement run, before the group-repetition guard below,
reported 27 ratio columns as determining the target exactly:
with a 3%-positive target, a near-unique continuous column
pairs every singleton group with a single target value
vacuously. Exact determination is now stated only when the
predictor's values repeat (average group size at least four),
and a regression fixture carries the shape.

**Bike Sharing, target `cnt` (numeric).** No statement.
Strongest: `registered` at xi 0.878 — above the bar, but the
runner-up `instant` (the row index) stands at 0.706, so the
dwarfing condition fails and the screen stays silent, stating
the reason. `cnt` is by construction `casual + registered`, so
`registered` is a component of the target rather than a
restatement of it; the field condition is what keeps the screen
off it. Then `casual` 0.519, `hr` 0.440, `atemp` 0.134.

**Appliances Energy, target `Appliances` (numeric, typed
frame).** No statement. Strongest: `RH_1` 0.095, `T9` 0.084,
`RH_6` 0.083, `T7` 0.081, `T1` 0.079. The frame's two random
variables (`rv1`, `rv2`) score near zero, as they should.

Verdict against the acceptance bars: the planted shapes fire,
the legitimate fields stay silent, and the one real
post-outcome column in the set is named. The vacuity failure on
Taiwanese Bankruptcy was found by this measurement and fixed
before the screen shipped, with the fix recorded here and in
the fixtures.

## Provenance

Adult: UCI Census Income, DOI 10.24432/C5XW20.
Hotel bookings: Antonio, de Almeida & Nunes, "Hotel Booking
Demand Datasets", Data in Brief (2019), CC BY 4.0; TidyTuesday
2020-02-11 mirror.
Taiwanese Bankruptcy: UCI 572, DOI 10.24432/C5004D.
Bike Sharing: UCI 275, DOI 10.24432/C5W894.
