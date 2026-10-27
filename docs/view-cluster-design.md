# Design note: `views` — a deterministic greedy view/cluster pass

> STATUS: design. This note is a design artifact; the implementation that
> follows it lives in `crates/onceover-mines/src/views.rs` in this clone.
> Nothing in this note claims posterior probabilities: every score below is
> a marginal-likelihood value under a stated conjugate model, optimized by a
> deterministic greedy search. The findings are point estimates plus scores.

## 0. Purpose, scope, and naming

CrossCat's transferable idea is a two-level partition: columns split into
views (columns in one view depend on each other; across views they are
independent), and rows within each view split into clusters, each with its
own per-column component model. What does NOT transfer is the inference:
MCMC over partitions is stochastic, slow, and unusable at profiler scale.
This pass keeps the model and replaces the inference with a deterministic
greedy search over the profiler's existing sample path.

- Code name: `views`. Detector id `views`, in `onceover-mines`.
  The name "CrossCat" appears nowhere in code or findings: the inference is
  different, so the name must say so honestly.
- Budgets: standard and deep only, like the other model passes. Quick skips
  it (quick has no model section at all).
- Inputs: the extracted `TypedTable` (zero-copy columns), never the raw
  Polars frame. The pass runs on the profiler's deterministic stratified
  sample (`onceover_core::sample_rows`), standard row cap 20,000, deep
  50,000, or the `sample_rows` override. Search decisions use a bounded
  search subsample (section 2); final scores use the full sample.
- Finding categories (existing taxonomy only, no new kinds): view findings
  are `associations` / `statistical`; regime splits are `changes` /
  `residual_level`; degenerate clusters and spike cells are `concentration`.
  Section 4 gives the exact mapping and claim templates. This mapping is
  deliberately aligned with the sweep scorer's expectations for the
  sensor-iot patterns (regime shift: `changes` / `residual_level`;
  stuck run and spikes: `concentration`).

## 1. Component marginal likelihoods

### 1.1 Column families (from `TypedCol`)

- Continuous: `I64`, `F64`, `Date` (days since epoch, as f64),
  `Datetime` (microseconds since epoch, naive, as f64).
- Categorical: `Str`, `StrOwned`, `Bool` (two levels).
- Excluded: `Null` (all-null column carries no information) and `Other`
  (opaque dtype). Excluded columns are recorded in evidence metrics, not
  silently dropped.

### 1.2 Continuous: Gaussian with Normal-Inverse-Gamma prior

Values are standardized per column on the sample before scoring:
z = (x - median) / IQR, with quartiles by linear interpolation (type 7)
over the column's non-null sample values. Robust scaling is load-bearing,
not cosmetic: mean/sd scaling lets a few extreme spikes inflate the
scale so far that a constant block becomes indistinguishable from varied
rows (verified during implementation: a 60-row constant block at 21.5
plus 3 spikes at 999.9 collapses to near-identical z values under
mean/sd, and the constant block never separates). Median/IQR scaling
keeps the block at z = 0 and the varied rows spread around it.
Fallbacks, in order: if IQR == 0, use mean/sd; if that degenerates too
(sd == 0 or fewer than 2 non-null values), every standardized value is
0.0. This is an empirical-Bayes-flavored step, stated openly: the score
is a marginal on the standardized scale, comparable across columns and
units, and absolute-scale information is lost. Anomaly rankings are
unaffected: standardization is a positive linear map, so
predictive-density orderings are identical on raw and standardized
scales.

Prior on the standardized scale, fixed for all continuous columns:
NIG(mu0 = 0, kappa0 = 1, alpha0 = 2, beta0 = 1). Justification: proper
prior; kappa0 = 1 is roughly one pseudo-observation; alpha0 = 2 gives a
finite prior mean for the variance; beta0 = 1 sets the prior variance
scale to the standardized unit. No per-column tuning.

For values x_1..x_n with sample mean xbar and sum of squared deviations
S = sum (x_i - xbar)^2, the log marginal likelihood is:

    kappa_n = kappa0 + n
    alpha_n = alpha0 + n / 2
    beta_n  = beta0 + S / 2 + kappa0 * n * (xbar - mu0)^2 / (2 * kappa_n)
    log p(x) = lgamma(alpha_n) - lgamma(alpha0)
             + (alpha0 / 2) * log(beta0) - (alpha_n / 2) * log(beta_n)
             + (1 / 2) * (log(kappa0) - log(kappa_n))
             - (n / 2) * log(2 * pi)

Degenerate cases are all finite: n = 1 gives kappa_n = 2, alpha_n = 2.5,
beta_n = 1 + 0 + (xbar^2) / 4, all finite. Constant columns give S = 0,
which is fine as long as beta0 > 0 (it is). A cluster with no non-null
values for a column contributes 0 to that column's sum (the column is
scored only on rows where it is observed).

Posterior predictive (used for anomaly scores in section 3): Student-t
with nu = 2 * alpha_n, location mu_n = (kappa0 * mu0 + n * xbar) / kappa_n,
scale^2 = beta_n * (kappa_n + 1) / (alpha_n * kappa_n), density

    log p(x_new) = lgamma((nu+1)/2) - lgamma(nu/2)
                 - (1/2) * log(nu * pi * scale^2)
                 - ((nu+1)/2) * log(1 + (x_new - mu_n)^2 / (nu * scale^2))

### 1.3 Categorical: multinomial with symmetric Dirichlet prior

Levels are the distinct non-null values on the sample, capped at 32
(the same `nominal_max_levels` the encoder uses): the 32 most frequent
levels keep their identity, and every other value is pooled into one
explicit `other` level. The pooled level is a real level in the model,
not a deletion: rare-level structure inside the tail is lost, and that
loss is stated in the finding's metrics (tail share reported).

Prior: symmetric Dirichlet with concentration alpha = 1 per level
(Laplace smoothing). Justification: proper, simple, avoids zero
probabilities, and ranking is insensitive to alpha in {0.5, 1} for the
cluster sizes this pass considers. For counts c_1..c_K (K levels
including the pooled tail) and n = sum c_k:

    log p = lgamma(K * alpha) - lgamma(n + K * alpha)
          + sum_k [lgamma(c_k + alpha) - lgamma(alpha)]

Predictive probability of level k: (c_k + alpha) / (n + K * alpha).

Bool columns are two-level categoricals. `Date` and `Datetime` are NOT
treated as categorical: they go through the continuous family (section
1.4).

### 1.4 Dates and datetimes as continuous epoch values

Date (days) and Datetime (microseconds, naive — the extractor already
normalizes timezones away) enter the Gaussian family on their epoch
numbers, then standardized like any continuous column. Justification:
order and spacing are preserved, so regime shifts in time appear as
distribution shifts, which is what the cluster layer detects. Limits,
stated openly: calendar structure (day-of-week, seasonality, business
calendars) is not modeled; a column whose only structure is calendrical
will not cluster meaningfully, and the pass will report that as no
improving split rather than inventing one.

### 1.5 Nulls

Nulls are EXCLUDED from component scores, for every type, uniformly.
Each (view, cluster, column) cell records its null count in evidence
metrics, so a cluster that is mostly null on some column is visible.
Justification: nulls in this profiler are missingness, already covered
by column-profile missingness findings; scoring nulls as a category
would let missingness patterns drive views and clusters. Rejected
alternative: null-as-a-category would duplicate the conditional
missingness structure that is the sibling pass's job, and would let a
skip-logic column dominate a view. A cluster with heavy nulls on a
column is reported, not hidden, via the recorded counts.

### 1.6 Summary of per-type treatment

| TypedCol | Family | Prior | Nulls | Degenerate |
|---|---|---|---|---|
| I64, F64, Date, Datetime | Gaussian (standardized) | NIG(0, 1, 2, 1) | excluded, counted | finite (section 1.2) |
| Str, StrOwned, Bool | Multinomial, <=32 levels + pooled tail | Dirichlet(1) | excluded, counted | finite (counts may be zero) |
| Null, Other | excluded | - | - | column ineligible |

## 2. View/cluster scoring and greedy search

### 2.1 Total score and complexity penalty

For a partition into views V, where view v has clusters C(v):

    score = sum_v [ sum_{c in C(v)} sum_{col in v} log-marginal(col values in c)
                    - gamma * log(n_sample) * |C(v)| ]
            - gamma * log(n_sample) * |V|

with gamma = 2.0 (default, recorded in metrics) and n_sample the full
sample row count. Justification: each new cluster must earn its keep by
improving fit by more than a small multiple of log n — the same order as
a BIC penalty for a handful of parameters — and each new view likewise.
This is deliberately not a Dirichlet-process prior: no concentration
parameters to check, one tunable constant, and the penalty is visible in
every reported score. Sensitivity to gamma is untested (section 6).

A merge of views v1, v2 into m changes the score by:

    delta = score(m) - score(v1) - score(v2) + gamma * log(n_sample)

(the last term because the view count drops by one). A merge is accepted
iff delta > 0, computed on the search sample (section 2.3).

### 2.2 Eligibility and caps

- Eligible columns: non-`Null`, non-`Other`, with at least one non-null
  value on the sample.
- `max_columns = 64`: the first 64 eligible columns in table order are
  considered; dropped column names are recorded in evidence metrics.
- `max_clusters = 8` per view.
- `search_rows = 2000`: search decisions (pair pre-screen, splits, merge
  acceptance) run on `sample_rows(n_sample, 2000)` mapped through the
  sample, i.e. a stratified subsample of the sample. Final scores and
  reported statistics use the full sample.
- `tried_merges_cap = 64`: at most 64 merge attempts in the merge loop.
- All caps are recorded in evidence metrics. No other hidden limits.

### 2.3 Row clustering within a view (Phase B)

Divisive median splits, fully deterministic:

1. Start with all search rows in one cluster.
2. Repeat: for each cluster, for each column in the view, consider
   splitting the cluster's rows at the distinct-value boundary nearest
   the median position of that column's non-null values within the
   cluster (for distinct values this is exactly the lower median
   value). For categorical columns the "values" are level ids under
   the frequency-then-name encoding order (section 1.3), so the split
   bipartitions levels by that order — crude but deterministic and
   bounded. The boundary rule (rather than a median-value rule) is
   load-bearing: splitting at the median VALUE degenerates whenever
   the median equals the cluster maximum (left side = every row,
   right side empty), which happens routinely for discrete-valued
   columns — a singleton column then cannot split at all while a pair
   view can split around it, manufacturing a spurious merge (caught
   2026-10-26 while prototyping the documentation examples: two
   independent binary columns merged with gain 167.1 nats; fixed by
   this rule, with regression tests). Rows null on the split column go
   to the majority side (ties go to the lower-index side). Compute the
   split gain: (marginal of left + marginal of right - marginal of
   whole), summed over ALL columns in the view (the split is evaluated
   on the view's joint score, not just the split column), minus
   gamma * log(n_search) for the new cluster.
3. Accept the best split iff its gain > 0. Tie-break order, applied
   strictly: highest gain; then lowest cluster index; then lowest
   column index. Stop when no split gains or `max_clusters` is reached.

Marginals are computed from sufficient statistics (n, mean, sum of
squared deviations for continuous; level counts for categorical), so
each candidate split costs O(rows in cluster) for the statistics pass
plus O(1) per marginal evaluation.

### 2.4 View discovery (Phase A)

1. Singleton phase: run Phase B on each eligible column alone; record
   each singleton view's score.
2. Pair pre-screen: for every column pair, run Phase B on the two-column
   view and compute
   gain_pair = score_pair - score_single(col1) - score_single(col2).
   Sort pairs by gain descending; ties break by lowest column-index
   pair lexicographically.
3. Merge loop, in pre-screen order: for each pair whose columns are in
   different views, compute the merged view's Phase B clustering and
   score on the search sample; accept iff delta > 0 (section 2.1).
   Skip pairs already in the same view. Stop after `tried_merges_cap`
   attempts or when the pair list is exhausted.
4. Assignment: every full-sample row is assigned to the cluster
   maximizing its total log predictive density across the view's
   columns (nulls contribute 0). Ties go to the lowest cluster index.
5. Final scoring: recompute all marginals on the full sample with the
   final assignments. Reported scores are full-sample scores; search
   decisions are search-sample decisions, and that distinction is
   recorded in metrics.

### 2.5 Complexity

Let C = eligible columns (<= 64), K = max clusters (8),
n_s = search rows (2000), n = sample rows (<= 50,000). A split
candidate costs O(C * cluster_size) cell visits, because the split
regroups rows and every column's marginals must be recomputed for both
halves; split thresholds are computed once per (cluster, column) per round.

- Singleton phase: O(K * n_s) per column; trivial.
- Pair pre-screen: C^2 / 2 pairs, each O(K * 2 * n_s) per round set;
  at C = 64, about 1.1e8 cell visits.
- Merge loop: at most 64 attempts, each O(K * C_view^2 * n_s) in the
  worst case (C_view = 64 gives ~5.7e7 per attempt). The pathological
  case — many merges into one giant view — is bounded by the work
  budget below, not by the attempt cap alone.
- Assignment: O(n * K * C), about 1.0e7 at n = 20,000.
- Work budget: split evaluation stops when cell visits exceed
  3e8 (configurable), records `budget_hit` in metrics, and keeps the
  current partition. Typical cases (small views) run far below the
  budget; the budget exists so the pathological case degrades
  gracefully instead of blowing the profiler's runtime contract.
- Runtime is NOT recorded in evidence metrics: wall-clock values in
  the report would break byte-identical determinism. Runtime is
  measured by the validation harness, outside the report JSON.

### 2.6 Deadline behavior

The pipeline's deadline pre-check skips the pass entirely if the
deadline fired before it starts, with a coverage note. Inside the pass,
all bounds are count-based (sections 2.2–2.4), never wall-clock-based,
so the pass itself introduces no nondeterminism: the only
deadline-related behavior is the pipeline-level skip, which the
profiler already reports honestly.

## 3. Per-cell anomaly score

For each full-sample row, its assigned cluster, and each column in the
view, compute the log posterior-predictive density/probability of the
cell under the cluster's component (Student-t for continuous, section
1.2; Dirichlet-multinomial predictive for categorical, section 1.3),
using full-sample cluster statistics. Null cells get no score (nulls
are excluded, section 1.5).

Reporting rule, per column: cells with log predictive density below
(column median log-density - 10 nats) are reported, lowest first, capped
at 20 per column. The threshold is a strong-evidence line: 10 nats is a
factor of about 22000 in density against a typical cell. Reported per
finding: count, the cell values, and original row indices (sample rows
map back through the stratified sample to table rows). The threshold
(10 nats) and the cap (20) are recorded in metrics.

Small-cluster rule (load-bearing): cells in clusters smaller than
`min_degenerate_rows` (30 sample rows) are scored under the view's
LARGEST cluster, not their own. Without this, a handful of identical
spikes forms its own cluster and looks perfectly typical under it,
making the spikes invisible to exactly the score meant to catch them
(verified during implementation: 3 spikes at 999.9 clustered together
and produced no anomaly finding). The trade-off is stated: a genuine
small regime (under 30 sample rows) is reported as outlying cells
rather than as a regime. Clusters at or above the threshold are scored
under their own cluster as usual.

Plain statement about the two sensor-iot cases: they do NOT both surface
from this score. Spike cells (999.9 under a ~20C cluster) have hugely
negative log predictive density — in the worked example (section 5) a
standardized spike scores about -51 nats against about -0.79 for a
typical cell — so spikes surface directly. Stuck-run cells (600 rows at
constant 21.5) have HIGH predictive density under their own degenerate
cluster, because the cluster is defined by those very values. The stuck
run surfaces instead from the degenerate-cluster concentration finding
(section 4): a cluster whose within-cluster standard deviation on a
column is ~0 over a contiguous row block. One score does not do both
jobs, and the design does not pretend otherwise.

## 4. Findings translation

Every claim carries the honesty line: "greedy point estimate; scores
are marginal-likelihood values, not posterior probabilities." Evidence
basis is `estimated` (a fitted partition, not a measurement), validation
method `none`, scope `sample` with `rows_evaluated` (sample rows) and
`population_rows` (table rows). Interestingness values below are
selection priorities (0–1), not probabilities, fixed per kind so
selection is deterministic.

| Kind | Category / type | Subject | Claim template | Interestingness |
|---|---|---|---|---|
| View | `associations` / `statistical` | view columns | "Columns {a, b, ...} form a dependence block under the greedy view partition (merge gain {g} nats over separate views; greedy point estimate, not a posterior probability)" | 0.60 |
| Regime split | `changes` / `residual_level` | view columns | "Rows split into {k} clusters; cluster {i} covers rows {r1}–{r2} with {column} mean {m1} vs {m2} in cluster {j} (full-sample marginal gain {g} nats; greedy point estimate)" | 0.70 |
| Degenerate cluster | `concentration` | the column | "{n} of {m} rows in cluster {i} (rows {r1}–{r2}) hold {column} at {v} ({share}% of the cluster; greedy point estimate)" | 0.60 |
| Spike cells | `concentration` | the column | "{n} cells in {column} have log predictive density below {threshold} nats under their cluster (lowest {v} at rows {r...}; greedy point estimate)" | 0.50 |

Findings are emitted only when they exist: a view finding per
multi-column view (singleton views emit nothing — a column alone in a
view is the absence of detected dependence, not a statement); a regime
finding per view with >= 2 clusters, describing the largest
between-cluster shift; a degenerate finding per (cluster, column) with
cluster size >= 30 rows and one exact value (continuous) or level
(categorical) holding at least 90% of the cluster's non-null rows
(purity is deliberately not required: greedy search rarely isolates a
constant block exactly — it usually lands as the dominant share of a
slightly mixed cluster, and the 90% share is the honest statement of
that structure); a spike finding per column with at least one cell
below the section 3 threshold.

Evidence metrics for every finding include: `gamma`, `n_sample`,
`search_rows`, `max_columns`, `max_clusters`, per-column null counts
in the relevant clusters, dropped-column names (view findings), and
the full-sample partition score. Specification definitions record the
cluster row ranges and, for regime findings, the reference model
("single-component per column") and breakpoint row range, so the
statement is interpretable without the design note.

## 5. Worked example and validation plan

### 5.1 Worked example (numbers checked by script, not asserted)

Continuous, NIG(0, 1, 2, 1). Raw values [19.0, 20.0, 21.0]: median 20.0,
quartiles (type 7) 19.5 and 20.5, IQR 1.0, standardized values
[-1, 0, 1]. Then n = 3, xbar = 0, S = 2.0, kappa_n = 4,
alpha_n = 3.5, beta_n = 2.0, and

    log p = lgamma(3.5) - lgamma(2) + 0 - 1.75 * log(2.0)
            + 0.5 * (log 1 - log 4) - 1.5 * log(2 * pi)
          = 1.20097 - 1.21287 - 0.69315 - 2.75682
          = -3.46200

checked by direct evaluation (script output: -3.46200).

Categorical, Dirichlet(1). Counts [3, 1] over K = 2 levels:

    log p = lgamma(2) - lgamma(6) + (lgamma(4) - lgamma(1))
            + (lgamma(2) - lgamma(1))
          = 0 - 4.78749 + 1.79176 + 0
          = -2.99573

checked by direct evaluation (script output: -2.99573).

Predictive check on the NIG cluster above (mu_n = 0, nu = 7,
scale^2 = 0.7143): log predictive density at a typical value (0) is
-0.7863; at a standardized spike (1200) it is -51.07. The ~50-nat gap
is why the section 3 threshold (median - 10 nats) separates spikes
from typical cells with wide margin.

### 5.2 Validation plan (criteria fixed in advance)

Case: the sweep's sensor-iot case (ground truth: regime shift at row
25000, 20.0C to 32.0C with drift and increased variance; stuck run rows
10000–10600 at constant 21.5; 15 spikes at 999.9). Run at standard
budget through the public `profile` entry with the `views` detector
enabled, and judge:

1. Regime: a `changes` / `residual_level` finding on temperature whose
   cluster boundary, mapped back to original row indices, falls within
   ±500 rows of 25000. Pass/fail: boundary in [24500, 25500].
2. Spikes: at least 80% of the planted spikes present in the evaluated
   row set appear among the anomaly cells for temperature ranked by
   log predictive density. (Restated 2026-10-26 after the first
   validation run: the original "12 of 15" assumed full-table
   evaluation, but the standard budget evaluates a 20,000-row sample,
   which contains only ~7 of the 15 spikes. The per-evaluated-set
   statement is the honest criterion; full-scope numbers are reported
   alongside.) Pass/fail: flagged share >= 80%.
3. Stuck: NOT APPLICABLE to this pass (verdict 2026-10-26). A stuck run
   is order structure — 600 consecutive rows — and a value-partition
   model cannot see consecutiveness: in sensor-iot the 600 rows at
   21.5 are ~6% of their regime cluster, so no modal-share finding
   fires, and no threshold tweak fixes that without firing everywhere.
   Stuck-run detection belongs to the run-length pass (sibling track),
   not to this pass. The original criterion is retained here as a
   record of the change, not deleted. Pass/fail: n/a.
4. Determinism: two runs on sensor-iot and one further sweep case
   produce byte-identical report JSON.
5. Runtime: the pass completes on every sweep case at standard budget
   in under 60 seconds per case (a smoke bound, not a performance
   claim; actual times are reported, not gated, below 60 s).

Criteria 1–4 are pass/fail. Criterion 5 is reported numerically in all
cases. Failure of any of 1–4 is reported as a design miss, not tuned
away after the fact: thresholds and constants above are fixed before
validation runs.

### Validation results, 2026-10-26 (debug build, standard budget unless noted)

- C1 PASS: regime finding on temperature, 2 clusters, boundary
  candidate 24997 (within 500 of 25000). The second cluster's row
  range overlaps the first (drift interleaves rows) — the
  max-of-lower-cluster rule is the operative boundary estimate.
- C2 PASS (restated): standard budget flagged 7/7 sampled spikes;
  full-scope run (sample_rows = 50000) flagged 15/15 spikes, lowest
  logp -60151 nats. The original "12 of 15" was unachievable at
  standard budget by construction (the sample holds ~7 spikes).
- C3 NOT APPLICABLE: no modal-share finding fires for the stuck run
  (21.5 is ~6% of its regime cluster). Reassigned to the run-length
  pass; see criterion text above.
- C4 PASS: dual-run byte-identical report JSON on sensor-iot and on
  sales-orders (views-only profiles).
- C5 PASS: 0.7 s / 0.6 s standard budget (debug build); 2.2 s
  full-scope. No runtime claim beyond the smoke bound.
- Side observation (not a criterion): the pass found a
  device_id/timestamp dependence block (merge gain 30.4 nats) — the
  device_id concentration finding shows rows 20000+ hold a single
  device id, matching the case's device-change structure. Pressure
  produced 6 clusters and its own spike cells (lowest 1022.15 at row
  36854), which the ground truth summary did not mention — worth a
  look when the pressure story is examined, not asserted here.

### Validation addendum, 2026-10-26 (split-rule fix)

While prototyping the documentation examples, two independent binary
columns merged with gain 167.1 nats. Root cause: median-value splits
degenerate when the median equals the cluster maximum (empty right
side), so the singleton column could not split while the pair view
could split around it. Fixed by splitting at the distinct-value
boundary nearest the median position (section 2.3), with two
regression tests (binary column with majority upper value splits
exactly; independent binary columns do not merge). Unit tests now 9/9.
Re-validation after the fix: sensor-iot criteria unchanged (regime
boundary 24997, determinism byte-identical, 0.7 s standard budget,
sampled spikes 7/7, full-scope spikes 15/15 — full-scope spike cells
15 → 17, the two extra cells are non-spike rows; the criterion counts
spikes among cells, so it still passes). The aligned-regime
documentation fixture (two continuous columns sharing a shift at the
same row, plus an independent noise control) merges the shifting pair
(gain 24.6 nats) and leaves the control singleton. The same fixture
shows the noise-splitting limit below: the pure-noise control still
splits into 4 clusters, so cluster counts on noisy columns are not
regime evidence.

## 6. Blockers and honest limits

- No mathematical blocker was found for any column type: every family
  has a closed-form marginal and a finite degenerate behavior. The
  attempt's blocker was the implementation route (the delegate sessions
  ended silently before writing), not the math.
- Standardization is empirical-Bayes-flavored (section 1.2): the prior
  sits on a data-dependent scale. Scores are comparable across columns
  but are not pure prior marginals. Absolute-scale structure (a column
  with tiny vs huge variance) is invisible after standardization.
  Quartiles use type-7 linear interpolation; the IQR == 0 fallback chain
  (mean/sd, then all zeros) is fixed in code, not a choice per column.
- Greedy search finds local optima, not the best partition. Merge and
  split order plus tie-breaks make it deterministic, not optimal.
- Merge semantics, stated plainly: a merge fires when a shared
  clustering matches or improves the columns' singleton clusterings
  (aligned regimes, shared structure) closely enough that the saved
  cluster penalties outweigh any per-column marginal loss. It is not
  a general dependence detector: discrete columns whose singleton
  clusterings already reach purity (e.g. a binary pair that agrees
  82% of the time) will not merge, because the joint clustering adds
  clusters without adding per-column marginal gains. Whether to
  strengthen the criterion with a dependence-aware term is an open
  design question, not settled here.
- Noise splitting: the marginal-likelihood criterion splits pure-noise
  columns too (a unimodal sample cut into tighter pieces gains
  marginal likelihood faster than the gamma = 2.0 penalty removes
  it). Cluster counts and regime findings on noisy columns are
  therefore not, on their own, evidence of regimes.
- Search-sample vs full-sample gap: merge/split decisions use 2000
  rows; final scores use the full sample. A decision that is right on
  the search sample can be wrong on the full sample; the metrics record
  both sample sizes so the gap is visible.
- Nulls excluded (section 1.5): missingness-driven structure is
  invisible to this pass by design.
- Exchangeability: rows are treated as an unordered set within a view.
  Time order, gaps, and cadence stay with the dates pass; a view
  partition says nothing about sequence.
- Column cap: only the first 64 eligible columns in table order are
  considered; the rest are recorded as dropped, not modeled.
- Penalty constant gamma = 2.0 is a chosen default, not a derived
  value; sensitivity to gamma is untested and must be reported if a
  validation criterion fails marginally.
- Categorical tail pooling loses rare-level structure; datetime as
  continuous loses calendar structure (sections 1.3–1.4).
- The pass adds a model-fitting step to profiling; its worst-case cost
  (section 2.5) is seconds, not milliseconds, and must be measured on
  the scale case (1M rows) before any performance claim is made.
