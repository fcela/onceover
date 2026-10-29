# Exploration: information-theoretic dependence measures (issue #44)

Date: 2026-10-28. Exploration only — no library change. Prototype:
numpy implementations of binned mutual information (raw and
Miller–Madow corrected), normalized MI (MI / √(H(X)·H(Y))), and
Theil's U in both directions, on quantile bins, against the measures
the dependence screen already reports (Pearson, Spearman, Chatterjee
ξ both directions). n = 2000 per fixture unless stated.

## Q1. Which measures earn a finding?

Planted structures, strongest readings per measure:

| structure | Pearson | Spearman | ξ(x→y) | ξ(y→x) | NMI (16 bins) |
|---|---|---|---|---|---|
| linear | 0.97 | 0.97 | 0.77 | 0.76 | 0.46 |
| monotone bend (x³) | 0.89 | 0.90 | 0.67 | 0.66 | 0.41 |
| parabola (x²) | −0.01 | 0.00 | 0.81 | 0.20 | 0.49 |
| sine | −0.77 | −0.75 | 0.84 | 0.56 | 0.53 |
| many-to-one (3u mod 1) | 0.34 | 0.35 | 0.93 | 0.12 | 0.52 |
| XOR, marginal pair | 0.03 | 0.02 | −0.02 | 0.02 | 0.02 |
| independent | −0.01 | −0.02 | 0.03 | 0.01 | 0.02 |

On every structure the current set handles badly — parabola, sine,
many-to-one — ξ already reads as high as or higher than normalized
MI, and ξ also carries the direction (parabola 0.81 vs 0.20;
many-to-one 0.93 vs 0.12) that Theil's U was expected to add. U adds
nothing on quantile-binned numerics for a structural reason: equal-
count bins give both marginals the same entropy, so U(x→y) = U(y→x)
exactly. U is only asymmetric where marginal entropies differ,
which is the categorical case, not the binned-numeric one. XOR's
marginal pair reads zero under every pairwise measure, as it must;
that structure is the interaction pass's territory (#42).

What the MI family does add is not detection but calibration and a
different domain:

- **Entropies as column summaries.** H(X) and the effective number
  of levels exp(H(X)) are computable off the bins the encoder
  already builds, need no null calibration, and say something no
  current column-card field says (a 200-level column whose entropy
  equals a 12-level uniform column is concentrated, whatever its
  count of distinct values).
- **Categorical targets.** Pearson/Spearman do not apply and ξ is
  numeric-oriented; per-predictor information gain / Theil's U
  against a categorical target is a genuine gap in the current
  measure set, and there U *is* asymmetric (skewed level counts
  give different marginal entropies).

## Q2. Binning policy and ranking stability

Normalized MI's ranking of the *strong* structures flips with bin
count: linear ranks first at 8 bins (0.54) and fourth at 16 and 32
(0.46, 0.40); many-to-one moves from fifth (0.39) to first (0.57).
All real structures stay far above the null at every bin count, so
bin count does not change *detection*; it changes the *ordering*
among detected pairs, because finer bins reward structures
differently. A report that ranks pairs by NMI would give different
answers at different budgets. Any future MI reporting must fix the
bin count by rule (below), never by budget, and should not rank
across pairs by the raw value.

## Q3. Bias and null behaviour

Raw binned MI is badly biased upward when the table is sparse
(60 independent pairs per cell; mean / max in nats):

| n | bins | raw MI mean | raw MI max | Miller–Madow mean | Miller–Madow max |
|---|---|---|---|---|---|
| 200 | 16 | 0.654 | 0.757 | 0.369 | 0.507 |
| 200 | 32 | 1.726 | 1.774 | 1.418 | 1.484 |
| 2000 | 16 | 0.058 | 0.071 | 0.003 | 0.015 |
| 2000 | 32 | 0.276 | 0.303 | 0.070 | 0.102 |
| 2000 | 64 | 1.008 | 1.046 | 0.639 | 0.689 |
| 20000 | 32 | 0.024 | 0.027 | 0.001 | 0.003 |
| 20000 | 64 | 0.105 | 0.110 | 0.006 | 0.012 |

Miller–Madow's occupied-cells correction works while cells are
mostly occupied and fails in the sparse regime (bins² ≳ n), where
no closed-form correction rescues the estimate. The library's
existing `information()` (32 bins, up to 2048 rows) sits at the
edge: fine as an internal pre-screen ranking, not reportable as a
value.

A policy that keeps the null at zero within stated error, measured:
quantile bins k = clamp(√n / 2, 4, 32), Miller–Madow correction,
n ≥ 500. Null MI then has mean ≤ 0.012 and max ≤ 0.053 nats across
n ∈ {500, 1000, 2000, 5000, 20000} (k = 11, 16, 22, 32, 32). A
reporting floor of 0.1 nats sits above every null max measured
under the policy.

## Q4. Cost

The expensive step is shared, as the issue expected. In the
prototype at n = 20000, 32 bins: binning one column 3.06 ms; one
pair's contingency table plus every measure above (MI raw and
corrected, both entropies, joint entropy, NMI, both U directions)
1.64 ms. The measures are O(cells) reads off the table; the family
costs one table, not one pass per measure. onceover already builds
the encoded bins once and already builds pair tables in
`information()`; a Rust implementation inherits both.

## Q5. Where the results live

- Entropies: column cards (H in nats, effective levels exp(H)) —
  descriptive statistics of one column, no finding, no null
  problem.
- Pairwise MI for numeric pairs: not a finding (Q1, Q2).
- Categorical-target information gain: a candidate finding under
  `dependencies`/`predictive`-adjacent screening, or an enrichment
  of the existing categorical-target screen evidence — the
  follow-up issue should decide against a planted fixture with a
  skewed categorical target, where U's asymmetry is real.

## Recommendation

1. **Do not implement** MI/NMI/Theil's U as association findings
   for numeric pairs. ξ dominates them on the planted structures,
   carries direction already, and NMI's cross-pair ranking is
   bin-count-sensitive.
2. **Do implement** (follow-up issue): per-column entropy and
   effective-level count on column cards, computed from the
   existing encoded bins. No detector, no bars, no null.
3. **Do scope** (follow-up issue): per-predictor information gain
   and Theil's U for categorical targets, with the binning policy
   of Q3 (levels capped by the same √n rule) and the 0.1-nat floor,
   validated on planted skewed-target fixtures before any finding
   form is chosen.
