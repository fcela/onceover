# Scagnostics across candidate partitions

Issue [#346](https://github.com/fcela/onceover/issues/346) and the [earlier scoping study](2026-11-17-uci-expansion.md) cover the geometry of a whole scatterplot. A pooled plot can also hide structure that becomes clear once rows are grouped. The existing Simpson check starts from a strong pooled association. This search starts from eligible numeric pairs regardless of their pooled score and searches for observed context that makes their geometry clearer. Select `detectors=["partition_scagnostics"]` in a profile or call `onceover.partition_scagnostics(df)` to scope the search. The historical benchmark used the same Rust search through a Cargo example.

## Prior art and implementation choices

| Tool or method | What it provides |
| --- | --- |
| [Wilkinson and Wills](https://www.cs.uic.edu/~wilkinson/Publications/scagdist.pdf), [R `scagnostics`](https://github.com/cran/scagnostics), and [`cassowaryr`](https://github.com/numbats/cassowaryr) | Scoring for individual clouds or supplied subsets. These are reference implementations for validating additional metrics and measuring the effect of binning. |
| [Exceptional Model Mining](https://dspace.library.uu.nl/bitstream/handle/1874/314520/mining.pdf?sequence=1) and [`pysubgroup`](https://pysubgroup.readthedocs.io/en/stable/api/pysubgroup.html) | Search for predicate-defined subgroups with exceptional relationships. The experiment borrows the method of scoring a relationship over rows selected by other attributes and ranks a full partition by its weighted score gain. |
| [`delaunator`](https://github.com/mourner/delaunator-rs) and [`spade`](https://github.com/Stoeoef/spade) | Rust Delaunay triangulation primitives for a sparse Euclidean MST when more resolution is needed. The experiment uses dense Prim over at most 64 vertices. Profile both paths before taking a dependency. |
| [`flexmix`](https://cran.r-project.org/web/packages/flexmix/index.html) | A reference for finite mixtures of regressions when no observed context identifies the regimes. |

## Search and evidence

For each pair and requested metric, the discovery gain is `Σ(n_group / n_pair × score(group)) − score(pooled)`. The search maximizes this gain minus 0.02 for each leaf beyond two. The penalty gives simpler rules preference when extra leaves yield a small improvement. For each candidate, the groups cover precisely the same rows scored by the pooled cloud. Pairs with missing or non-finite coordinates use only their finite complete cases. All leaves must have at least 24 rows and 10% of the eligible rows in each half. The experiment requires a raw gain of at least 0.08 in both halves. These are pilot thresholds. With nearly balanced halves a table needs at least 96 complete rows for a two-way split to have support in both halves.

The observed partition rules are:

- Categorical columns with at most four discovered non-null levels. The search considers individual levels against the remainder for three- and four-level columns, and the full level partition. For two levels it considers the first versus the remainder. A single observed level can separate missing values. A `levels` rule assigns a row to the index of its matching listed value and assigns every other row to the final group.
- Numeric context cutoffs at the discovery quantiles 0.20, 0.35, 0.50, 0.65 and 0.80, deduplicated. A `threshold` rule assigns finite values at or below the cutoff to group zero and other rows to group one.
- Four-way crossings of two binary categorical rules. In a `cross`, each equality contributes bit zero for a match and bit one otherwise. The first test contributes the high bit. Enumerating crosses captures interactions that a greedy tree can miss when neither first split improves the score.

The two scores are `monotonic` (squared Spearman, with signed rho in the evidence) and `binned_striated` (fraction of centroid MST vertices with degree two and an incident-edge cosine below −0.75). The latter applies the published striation statistic to an 8×8 grid of occupied cell centroids. The scaling is fixed across all groups and both halves using the discovery pair's ranges. Values beyond the discovery envelope are clipped to its edges. This variant uses a fixed rectangular grid and retains outliers. Shapes finer than the grid and changes in scale driven by extreme values can affect the result. A constant rank axis makes the partition ineligible for a monotonic claim. Random permutations score a constant-axis leaf as zero so the randomized statistic is defined on every assignment. Fewer than three occupied cells yield zero striation.

Only the best discovery rule per pair and metric moves to holdout. A one-sided randomization test shuffles labels within that pair's complete holdout rows, preserving both the intact scatterplot and the leaf counts. The null is exchangeability of the joint `(x, y)` distribution across leaf labels. Changes in marginal range or location can affect the scores under this test, even when a regression mechanism is shared. The runner uses a fixed permutation count per run, `max(199, 40 × shortlist size) − 1`, and `(1 + exceedances) / (1 + permutations)`. It stops a test early when its best possible final p-value already exceeds the Bonferroni cutoff. The shortlist size includes candidates that later fail holdout support or the minimum gain. The statistical interpretation assumes independent, exchangeable rows under the null. Repeated entities or serially dependent observations need unit-aware splitting and a compatible null. [Athey and Imbens on sample splitting](https://doi.org/10.1073/pnas.1510489113) · [Phipson and Smyth on random permutations](https://arxiv.org/abs/1603.05766)

Rust presorts each coordinate once per half and restricts its order to each leaf to obtain exact within-leaf midranks. Each subsequent rank evaluation is linear in rows, including under label permutations. Geometric evaluation takes linear time in rows plus `O(k²)` per leaf for at most 64 occupied cells. Candidate membership bytes are prepared once across all pairs, and Rayon parallelizes pairs. The runner caps work at 4,096 sampled input rows, 16 pairs, 96 descriptor columns, and 128 rules. It tries one main rule per descriptor before variants and reserves up to a quarter of the candidate slots for interactions when the main rules fill the budget. Without `pairs`, it visits pairs in column-name order. Explicit pairs use caller order. Coverage records the caps, candidate counts, pairs with zero supported partitions, scored partitions, shortlist size, holdout outcomes, and permutations executed. The sample is index-based and depends on input row order. A production pass should use the shared content-keyed profiling basis and existing deadline accounting.

## Reproduction

```sh
RAYON_NUM_THREADS=8 cargo run --release -p onceover-mines \
  --example partition_scagnostics -- --benchmark
```

The benchmark uses seeded synthetic data. Each small run has 1,024 rows, the two plotted measures, eight random four-level descriptors, and two nominal and one continuous context where applicable. The wide fixture adds 35 four-level distractors to the planted pair, six independent numeric measures, and grows the input to 12,000 rows. It times five warmed runs with both objectives and the default caps. The rank microbenchmark evaluates the same 512 four-group assignments on a 2,048-row cloud against `onceover_core::stats::spearman` applied independently to the groups. Reported times exclude compilation, fixture generation, and JSON I/O.

The stripes fixture plants thin diagonal bands in one protocol and a uniform cloud in the other. Wrapping the bands at the boundaries gives both protocols the same uniform marginal distributions. Shuffling the true context in the opposite-slopes fixture preserves both coordinates of every scatter point. The latent fixture omits the true context and supplies the independent distractors.

For a Polars DataFrame `iris` with the two sepal measures and species, use the Python API. The notebook reproduces the data and call. [Case study](../book/case_studies/partition_scagnostics_cases.ipynb)

```python
import onceover

result = onceover.partition_scagnostics(
    iris, pairs=[("sepal_length", "sepal_width")],
    partition_by=["species"], metrics=["monotonic"],
)
print(result["findings"][0]["validation"])
```

## Results

Measured on arm64 macOS 26.3 with Rust 1.100.0, eight Rayon threads, and the release profile at base `a0527ef` with this experiment applied. Each count is the number of seeded runs with a surviving finding for the metric.

| Fixture | Runs | Monotonic | Binned striation | Median time |
| --- | ---: | ---: | ---: | ---: |
| Opposite slopes, observed protocol | 16 | 16 | 0 | 3.9 ms |
| Opposite slopes, numeric cutoff | 16 | 16 | 0 | 4.1 ms |
| Opposite slopes, two-column interaction | 16 | 16 | 1 | 3.8 ms |
| Wrapped stripes, matched marginals | 16 | 0 | 15 | 17.1 ms |
| Independent coordinates | 64 | 0 | 2 | 4.8 ms |
| Homogeneous association | 64 | 0 | 0 | 2.0 ms |
| Opposite slopes, true labels shuffled | 64 | 0 | 4 | 2.7 ms |
| Opposite slopes, true labels omitted | 64 | 0 | 1 | 2.6 ms |

In the seed-zero protocol mixture, holdout Spearman rho was +0.072 pooled and +0.996 and −0.997 in the two leaves, for a squared-score gain of 0.988. In the wrapped-stripe fixture the seed-zero rule also selected the planted protocol. Its holdout striation was 0.094 pooled, 0.524 in the striped protocol, and 0.079 in the uniform protocol, for a weighted gain of 0.211. The seven striation reports across the four control fixtures show that the geometry setting can produce false leads. The sample of synthetic fixtures is small and covers one family of stripe shapes.

The wide fixture took **98.5 ms** median and scored **2,048** pair/partition combinations with both metrics. Coverage marked the run capped at 4,096 of 12,000 rows, 16 of 28 pairs, and 128 of 183 candidate rules. The rank microbenchmark took **8.65 ms** with presorting, including presort time, versus **71.31 ms** with separate sorts for each group and assignment. That's **8.2×** faster in this workload, with no observed difference across the 2,048 correlation values compared.

On Anderson's 150 flowers, pooled sepal Spearman rho is −0.167 over the full data, and is +0.755, +0.518 and +0.427 within the three species. With seed zero, discovery selected setosa versus the remaining species. The 75 held-out flowers gave rho −0.171 pooled, +0.758 for 26 setosa, and +0.572 for the other 49. The weighted squared-score gain was 0.384 and the adjusted permutation p-value was 0.0050. [R iris documentation](https://stat.ethz.ch/R-manual/R-devel/library/datasets/html/iris.html)

## Scope of a production design

The observed-context method can surface a pattern when a field such as protocol or site identifies the regimes. A pair can also contain genuine mixtures without any useful context in the table. Choosing memberships from the plotted coordinates and then measuring the same objective on held-out rows can manufacture structure under a single population. For example, grouping two independent uniform coordinates on `[-1, 1]` by the sign of their product creates within-group Spearman correlations of +0.75 and −0.75 by construction, including on fresh data. A latent-regime experiment should compare held-out likelihood integrating over memberships against flexible single-population candidates such as curved or heteroskedastic regression, and measure the uncertainty of the memberships. The observed-context experiment supplies a benchmark for how much of the practical problem can be handled with replayable rules. [Tarpey and Petkova on discrete and continuous latent models](https://doi.org/10.1177/1471082X0801000202)
