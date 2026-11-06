# Tested associations: per-pair tests, two statement tiers, and the pairs table

## What changed

The association screens used to state a pair only when an effect
cleared a fixed bar — max(|Pearson|, |Spearman|, ξ) ≥ 0.5 for
numeric pairs, and confidence/lift bars for nominal rules — with
no significance test anywhere in the path. Every screened pair
below the bar was computed and discarded. A probe pair at sample
r = 0.212, n = 5,000 produced no statement, and the planted-signal
battery's standing recall at ρ = 0.15 and ρ = 0.4 was 0.00.

Each screened pair now carries a p-value, adjusted across the
screened family with Benjamini–Hochberg, and statements split
into two tiers:

- **Strong tier** — adjusted q ≤ 0.05 and the previous effect
  bars. These statements read exactly as before.
- **Weak tier** — adjusted q ≤ 0.05, below the effect bars. Full
  findings in the report JSON with the same evidence shape; the
  digest collapses them into a single count line naming the
  strongest few, so the default digest budget is unchanged.

`report.associations()` returns every screened pair as a Polars
DataFrame: both columns, kind, n, the effect sizes the screen
computed, the raw and adjusted p-values, and the tier. Pairs the
screen computed but did not state are rows in the table with
tier `none`; nothing the screen measured is discarded anymore.

## The tests, per pair kind

- **Numeric** — a p-value per component: Pearson and Spearman
  through the standard t approximation, and Chatterjee's ξ in
  each direction through the permutation distribution of its
  rank-sum statistic. The permutation mean and variance of the
  statistic have closed forms in the observed rank multiset, so
  the ξ p-value costs one linear pass over quantities the
  statistic itself already computes — no permutations are
  simulated, and the per-pair cost stays the cost of computing ξ
  (an O(n log n) sort) plus O(n). The four component p-values
  combine by Bonferroni (four times the minimum): conservative,
  and valid under any dependence between components.
- **Nominal** — a G-test on the pair-complete contingency table,
  with Cramér's V from the Pearson statistic as the effect size.
- **Mixed** (nominal × numeric) — a one-way F-test across the
  nominal levels, with the correlation ratio η as the effect
  size. Mixed pairs previously had no pair statement of their
  own at all; only the GBM screen could speak about them.

Benjamini–Hochberg runs over the pairs actually tested in the
run, one family per screen: the numeric screen's tested pairs
(in the capped wide-table path, the survivors of the binned-MI
stage that receive detailed statistics — the coverage string
states the tested count), and the nominal/mixed screen's tested
pairs together. A pair is tested only when its inputs support a
test (n ≥ 30, no constant side, minimum expected cell counts for
the G-test); degenerate pairs still appear in the table,
untested, with their effect sizes where those are defined.

The ξ route was the one open design question: a permutation test
per pair is not affordable at wide-table scale (the standard
tier tests up to 496 pairs over up to 20,000 sampled rows). The
closed-form permutation moments make the test exact in its first
two moments and normal in the tail. The moment formulas are
validated in the unit tests against 40,000 simulated
permutations at n = 60, tied and untied ranks, and the resulting
test flags a quadratic relation and clears independent draws.

## Acceptance probes

Two planted probes from the issue, 12 seeds each, n = 5,000:

| probe | stated | tier | measured effect | worst q |
|---|---|---|---|---|
| numeric ρ = 0.2 | 12/12 | weak, all seeds | r = 0.169–0.224 | 2.2e-32 |
| nominal V ≈ 0.3 | 12/12 | weak, all seeds | V = 0.285–0.324 | 8.9e-91 |

Under the fixed bar both probes stated 0 of 3 seeds at the
issue's measurement (r = 0.5 stated 1 of 3, r = 0.9 stated 3 of
3); the strong tier preserves that behavior for large effects,
because a pair above the effect bar at these sample sizes has
an astronomically small p-value and always passes the test.

## Null behaviour: the false-discovery rate against its nominal level

The planted-signal battery's null frame (8 independent noise
columns, 4 numeric and 4 categorical) was profiled on 12 seeds
and every tested pair classified by the associations table:

| screen family | pairs tested | discoveries (q ≤ 0.05) | frames with a discovery |
|---|---|---|---|
| numeric | 72 | 1 | 1/12 |
| nominal | 72 | 0 | 0/12 |
| mixed | 192 | 2 | 2/12 |

Three discoveries in 336 tested null pairs is 0.9% of pairs
against the 5% nominal level. Per family, the frame counts are
consistent with the nominal rate at this sample size: at an
exact 5% per-family rate, the probability of seeing two or more
discovery frames in 12 is about 0.12, and of seeing one or more
is about 0.46. The three discoveries are single weak-tier
findings with small effects — a numeric pair driven by the ξ
component (effect 0.029, raw component p ≈ 0.0017, q = 0.042;
its Pearson and Spearman p-values are 0.94 and 0.97) and two
mixed pairs (η = 0.057, q = 0.036; η = 0.066, q = 0.003). No
strong-tier statement appears on any null frame. This is the
behaviour the correction budgets: the screen spends its stated
error rate on weak statements with full evidence attached, and
the digest shows them as one collapsed line.

## Battery before and after

Full battery, 12 seeds, base variant. Recorded before:
`benchmarks/quality/results/2026-11-04-full.json` (commit
613303d4). Recorded after:
`benchmarks/quality/results/2026-11-05-full.json` (main 9dd66df9
plus this change).

| plant | detector before → after | stated before → after |
|---|---|---|
| weak_association (ρ=0.15) | 0.00 → 1.00 | 0.00 → 1.00 |
| association_moderate (ρ=0.4) | 0.00 → 1.00 | 0.00 → 1.00 |

Every other plant family is unchanged in detector and stated
recall (affine_identity, nonlinear_only, interaction_only, and
level_shift at 1.00; conditional_extremes and rare_slice_outlier
at 0.00, their standing known misses). categorical_fd is stated
on all 12 seeds in both runs; in the new run its statement is
grouped under the relationship family's lead (schema 0.6), so
its selected recall reads 0.00 while stated recall stays 1.00.
The battery document carries the baseline update and the revised
null tolerance in its 2026-11-05 addendum.

## Independent verification of the p-values

Three adjusted p-values were recomputed independently with
scipy (analysis tooling only; the library takes no scipy
dependency):

- Nominal G-test on a planted 2×2 pair: report p =
  4.3885913782324113e-110; scipy `chi2_contingency` with
  `lambda_="log-likelihood"` and no continuity correction gives
  4.3885913782325366e-110 — agreement to 13 significant figures.
  (scipy's default applies the Yates correction to 2×2 tables,
  which moves the tail to 1.03e-109; the uncorrected statistic is
  the standard G-test.) A second, near-null table agreed to five
  significant figures (0.21981 against 0.21981).
- Mixed F-test on a planted pair with η = 0.713: both the report
  and scipy's `f_oneway` underflow to 0.0.
- Numeric pair: the report's Bonferroni-min combination is
  8.913120233198795e-67 against 8.913120233190453e-67 recomputed
  from scipy's component p-values — agreement to 11 significant
  figures. The ξ statistics themselves match scipy's
  `chatterjeexi` to all printed digits (0.05488756165417352).

## Limits of this measurement

- The ξ p-value is a normal approximation to the permutation
  distribution, exact in mean and variance. In the far tail it
  can be mildly anti-conservative; the Bonferroni combination
  over four components and the BH step both push the other way,
  and the null measurement above is the empirical check — at
  these seeds it shows no excess. Wider-scale calibration (more
  null frames, heavier ties) remains open.
- In the capped wide-table path, BH adjusts over the pairs that
  received detailed statistics, not over every pair the first
  stage scored. The coverage string states the tested count, and
  the table contains exactly the tested family plus untested
  degenerate pairs — but a pair the first stage dropped cannot
  be discovered, at any tier. That is the pre-existing cap
  behaviour; testing does not widen the funnel.
- Strong statements remain unconditional. No Simpson-type
  reversal check over stratifying dimensions is included, so a
  strong pair statement can still reverse inside a subgroup;
  the statement's evidence is the pooled sample, as before.
  (Follow-up: the reversal check was added as item 5 of the
  same issue — see docs/benchmarks/2026-11-05-simpson-check.md.)
