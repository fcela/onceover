# Target-aware shape decomposition (issue #43)

Date: 2026-10-28. Library change: new detector `target_shapes`
(category `dependencies`, type `predictive`, forms `step_shape` and
`curved_shape`), plus a new optional argument `target` on `profile`.
The pass is off unless a target is designated; every other detector
and the default profile are unchanged (corpus expectations: 0
failures, and the corpus harness designates no target).

## Design settlement

The issue posed a design question first: does onceover grow an
optional target-aware mode, or leave shape-against-response to
modelling libraries? The mode is worth having here for the same
reason the predictability screen is: the profiler's reader — human
or agent — benefits from the interpretation ("this predictor acts
as a threshold on the response") at the one-turn first look, and
the statistics cost one sorted pass per predictor. The boundary is
that the mode stays opt-in and additive:

- **API**: `profile(df, target="y")` (Rust: `ProfileConfig.target`).
  Designating a target activates `target_shapes` when the caller
  did not explicitly restrict `detectors`; with an explicit list,
  the pass runs only if listed. The default profile is byte-for-
  byte the profile before this change.
- **Report surface**: findings in the existing `dependencies`
  category, type `predictive`, distinguished by form. No new
  category, no new report field.
- **Schema**: `schema_version` stays "0.2". The forms are additive
  vocabulary under an existing type, as `interaction_h` was; a
  reader that does not know the forms still sees a predictive
  dependency with named metrics.
- **Failure mode**: an unknown or non-numeric target is reported in
  the pass's coverage line, not raised (the hints policy).

## Method

Per numeric predictor, on complete cases (rows missing the
predictor or the target excluded; at least 30 required), a port of
mboostrs-dev's `column_diagnostics`: Pearson and Spearman
correlations; `linear_r2` = Pearson²; `curve_r2` from binned means
in up to 20 equal-count bins (never splitting a tie), adjusted for
the bins' degrees of freedom as `1 − (1 − R²)(n − 1)/(n − bins)`
and floored at 0; `step_r2` from the best single split of the
sorted predictor (splits only between distinct values), with the
located cut recorded as `step_at` (the original returns only the
R² shares; the cut is added here so the statement can name it).
Classification, with `curvature = curve_r2 − linear_r2` and
`step_share = (step_r2 − linear_r2) / curvature`:

- `step_shape` when `step_r2` ≥ 0.05, `step_r2 − linear_r2` ≥ 0.05,
  and `step_share` ≥ 0.8.
- `curved_shape` when not a step, `curve_r2` ≥ 0.10, and
  `curvature` ≥ 0.05; the claim states monotone increasing /
  decreasing (|Spearman| ≥ 0.5) or non-monotone.

At most 10 findings per target, ranked by the decisive R².

## Fixtures (measured, this change's test suite)

- Step: `y = 1 + 2·[x ≥ 4] + noise`, x ~ U(0, 10), n = 1200 →
  `step_shape` on x, `step_at` within 0.5 of 4.0, `step_r2` > 0.8.
- Monotone curve: `y = x³ + noise`, x ~ U(−1, 1), n = 1200 →
  `curved_shape`, `curvature` ≥ 0.05, Spearman > 0.9.
- Linear + null against the same target → no shape findings.
- No target designated → no shape findings; the pass records
  itself skipped in coverage.
- Mixed frame (measured separately): `y = step(dose at 4) +
  0.004·(age − 20)^1.5 + noise`, n = 1500 → `step_shape` on dose
  with `step_at` 4.000461 and `step_r2` 0.831; age is not reported
  (its adjusted curve explains no more than its line: curve_r2
  0.153 against linear_r2 0.155).

## Scope limits and deviations from the original

- **Marginal, not sequential.** mboostrs-dev's diagnostics feed a
  specification search that refits terms against partial
  residuals, so a curved term is judged against what remains.
  This pass is a marginal screen: all shares are of the target's
  total variance. A genuinely curved predictor whose effect is
  small next to a dominant second effect is not reported (the
  mixed fixture above). That is the stated bar working, recorded
  here so it is not mistaken for a detection failure.
- **Numeric targets and predictors only.** The original's screen
  also produces diagnostics the profiler already carries elsewhere
  (skewness, distinct count); those stay on the column cards and
  are not duplicated in the finding.
- **Cross-reference with the discontinuity screen** is by value:
  `step_at` is the same located cut the discontinuities detector
  reports for its jumps, so when both fire on a pair they
  corroborate each other; neither finding rewrites the other.
