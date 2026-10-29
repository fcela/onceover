# Friedman–Popescu H interactions (issue #42)

Date: 2026-10-28. Library change: new detector `interactions`
(category `dependencies`, type `predictive`, form `interaction_h`),
default-on at standard and deep budgets, off at quick. It runs after
the predictability screen and reuses the screen's GBM machinery: the
screen's per-target loop was refactored so the fitted ensemble at the
best holdout round can be retained (`fit_gbm_model`), with no change
to the screen's own findings (corpus expectations unchanged, below).

## Why

The profiler's pairwise dependence measures are model-free: they say
two columns are related. They do not say, for a given target, whether
two predictors act jointly — whether an additive model in the pair
leaves structure unmodelled. mboostrs-dev's gbm screen computes the
Friedman–Popescu H statistic for exactly this; this pass ports it.

## Method

For each numeric target whose screen holdout R² clears 0.10 (at most
3 targets, 4 at deep):

1. Refit the screen's GBM for the target, retaining the ensemble at
   the best holdout round, with the screen's split, pre-screen and
   packing reproduced exactly.
2. Additive gate. Refit the same target with depth-1 trees (ten
   times the rounds; stumps converge slowly, and an underfit
   additive refit would fake the margin). If the full surrogate's
   holdout R² does not exceed the additive refit's by at least
   0.02, no pair of this target is reported. Reason under
   "Deviations".
3. Rank the surrogate's features by split gain at the best round;
   take the top 6. Evaluate partial dependences on a deterministic
   stride sample of the target's present rows (128 rows, 192 at
   deep): for each sample point, the mean prediction over the
   sample with the evaluated columns replaced by the point's
   values — the empirical-background construction of mboostrs-dev's
   `preexplore`.
4. Per pair, with the three partial dependences centred:
   H² = Σ(f_jk − f_j − f_k)² / Σf_jk², interaction variance =
   mean of the squared interaction terms, and its standard error
   from the spread of those terms over the sample points.
5. Report the pair (at most 5 per target, ranked by interaction
   variance) when H² ≥ 0.05, interaction z ≥ 4, and interaction
   variance ≥ 1% of the target's variance on the sample.

## Fixtures (measured, this change's test suite)

Planted product, `y = 2·x1·x2 + 0.3·x3 + noise`, n = 1500: the
generating pair ranks first for `y` — H² 0.943, interaction variance
0.352 against a target variance of 0.539 (65.3%), z 7.4, surrogate
holdout R² 0.906 against 0.061 for the additive refit. The reverse
targets fire as well (`x1` on (`x2`, `y`), H² 1.0; `x2` on
(`x1`, `y`), H² 1.0): the inverse of a product is non-additive, so
these are correct readings of the fitted functions, not false
positives. Two small pairs involving the additive column `x3` fire
on the reverse targets at 3.7–3.9% interaction share; `x3` enters
those surrogates as a proxy for `y`'s noise term.

Purely additive, `y = 1.5·x1 − x2 + noise`, n = 1500: no findings.
All three eligible targets are stopped by the additive gate with
the ten-times-rounds refit (the pass's coverage line reports 3
fitted, 0 clearing the gate).

## Corpus (measured)

Full corpus run after the change: 23 datasets, 0 failed
expectations. Only one bundled dataset gains interaction findings —
sec-edgar-company-facts, 3 findings:

- `value` on (`cik`, `concept`): H² 0.321, interaction share of
  target 23.7%, z 4.0, holdout R² 0.98 against 0.56 additive. The
  scale of a reported value depends on which company reports which
  concept; an additive model in the pair misses most of it.
- `fiscal_year` on (`cik`, `company`): H² 0.083, share 1.2%, z 7.1,
  R² 0.41 against 0.30.
- `fiscal_year` on (`company`, `value`): H² 0.051, share 2.6%,
  z 5.3, R² 0.41 against 0.30.

No other bundled dataset's eligible targets clear the additive gate
with a pair above the bars.

## Deviations from the mboostrs-dev original

- **Numeric targets only.** mboostrs-dev computes H for any target
  its screen fits, including categorical ones (probability scale).
  Here categorical targets are skipped in v1; the screen's
  categorical findings are unaffected.
- **The additive gate is new.** Without it, the additive fixture
  above produces a finding: target `x2` on (`y`, `x1`) at H² 0.118,
  interaction variance 0.043 (11.2% of target variance), z 8.2 —
  the depth-3 ensemble's staircase fit of a plane is not additive
  as a function, and its partial dependences say so. No plausible
  bar on H², z or share separates that reading from a real one, so
  eligibility is decided out of sample instead. Known residue: on
  a target that clears the gate through one genuinely interacting
  pair, a second, purely additive pair can still pick up staircase
  artifact; the per-pair bars bound its size but do not eliminate
  it.
- **Standard error and z are new.** mboostrs-dev's
  `h_statistic_with_variance` returns the interaction variance
  (which it ranks by) but no sampling error; the z here is the
  interaction variance over the standard error of its per-point
  squared terms.
- **Ranking.** mboostrs-dev ranks pairs by interaction variance;
  kept. Its hit threshold in preexplore is H² ≥ 0.05; kept as the
  H² bar, with the z and share bars added.
