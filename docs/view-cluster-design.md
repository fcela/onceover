# Design: the `views` detector

The `views` detector fits groups of columns that can share a clustering of
rows. It is implemented in
[`onceover-mines/src/views.rs`](../crates/onceover-mines/src/views.rs) and runs
only when explicitly selected at the `standard` or `deep` budget.

The [tutorial](book/methodology/view_clusters.ipynb) contains runnable examples.
The [method reference](book/methodology/view_clusters_reference.ipynb) specifies
the component formulas, score, search, configuration, and report fields.

## Origin and scope

The column-view and row-cluster structure is inspired by
[CrossCat (Mansinghka et al., 2016)](https://www.jmlr.org/papers/v17/11-392.html).
Onceover uses a greedy search with fixed work limits to select one partition.
It does not implement CrossCat's Bayesian inference over partitions or return
posterior dependence probabilities.

A shared partition can describe columns whose distributions vary together
across groups. This is useful for exploring mixed numeric and categorical
tables. The current merge criterion is not a general dependence test, and
clustering alone does not establish distinct populations or temporal regimes.

## Integration

- Input is the extracted `TypedTable`. The pass builds its own fitting sample;
  it does not use the quantile-binned matrix used by the predictive passes.
- Standard and deep fitting samples contain at most 20,000 and 50,000 rows,
  respectively. Python's `sample_rows` overrides this cap.
- `detectors=["views"]` selects the pass. Default profiling excludes it, and
  the quick budget skips it even when selected.
- Coverage records sample size, excluded columns, and work-budget exhaustion.
  Model findings use `basis="estimated"` and carry no independent validation.
- Findings enter the shared selector. The selector recalculates priorities;
  the detector's initial scores do not directly set report order.

## Model and score

Continuous values use Gaussian components with a Normal-Inverse-Gamma prior
on median/IQR-standardized values. Strings and booleans use categorical
components with a symmetric Dirichlet prior. The 32 most frequent categorical
levels retain their identity; remaining levels are pooled. Nulls are excluded
from component scores, and null counts are recorded over the fitting sample.

The objective sums component log marginal likelihoods and subtracts
`gamma * log(n)` for each cluster and each view, with `gamma=2` by default.
Search uses the search-sample row count; final scores use the fitting-sample
row count. Merge gains therefore refer to the search sample even when final
view scores cover more rows.

Saving cluster penalties can make a shared view preferable to separate views
without improving their component marginals. Merges into a single-cluster view
are rejected because they can save penalties without adding partition structure.
This guard does not calibrate the remaining merges as statistical tests.

## Search and limits

The search fits singleton views, ranks candidate column pairs, and greedily
merges views with a positive score gain. Within a view, it splits clusters at
distinct-value boundaries near each column's median position. All tie-breaks
use fixed indices. The limits are:

| Work | Limit |
|---|---:|
| Search subsample | 2,000 rows |
| Modeled width | First 64 eligible columns in table order |
| Clusters per view | 8 |
| Merge attempts | 64 |
| Counted split-evaluation cell visits | 300 million |

After search, rows from the fitting sample are assigned using predictive
scores, statistics are recomputed, and assignment runs once more. Each row is
removed from a candidate cluster's component statistics when it contributed
to those statistics. This limits direct self-influence, but the partition
was still selected using the dataset.

The profiler checks its optional wall-clock deadline before this pass.
The pass itself stops on count-based work limits, so a running pass can exceed
the remaining wall-clock time.

## Reporting

| Finding | Current interpretation |
|---|---|
| `associations/statistical` | Columns share a fitted row partition; evidence includes search-sample merge gain. |
| `changes/residual_level` | Fitted clusters differ in means or categorical distributions. The pass does not test temporal order. |
| `concentration/modal_share` | A modeled numeric value or categorical level occupies at least 90% of a cluster's non-null values, with at least 30 such observations. All qualifying clusters for a column share one statement. |
| `concentration/spike_cells` | Cells score below their column's median predictive log density minus 10 nats, capped at 20 cells per column. |

Spike scoring removes a cell from its own component statistics. For clusters
with fewer than 30 rows, it uses the view's largest cluster as the reference
instead, unless the small cluster is itself the largest. This can expose small
groups of repeated extremes, but it can also flag valid rare groups. Crossing
the 30-row threshold changes the scoring reference abruptly.

A dominant pooled category can represent several raw labels. Such a modal
finding describes the encoded level, not a repeated single raw value.

The concentration types keep modal findings and spike findings separate during
statement consolidation. The reference chapter describes a remaining ranking
mismatch: the shared selector reads correlation and concentration metrics that
the views pass does not supply under the same names.

## Evaluation needed

Unit tests cover component formulas, predictive/marginal consistency,
determinism, specific partition fixtures, pooling, leave-one-out component
scoring, and statement separation. These checks do not establish general
statistical or performance guarantees.

Further evaluation should measure:

- False findings on independent columns and stationary noise. The current
  criterion splits noise into clusters, so cluster differences alone do not
  demonstrate regimes.
- Sensitivity to `gamma`, samples, rare groups, missing values, and column order.
- Anomaly precision and recall on held-out observations, including valid rare
  groups and pooled categorical levels.
- Runtime and memory at large row counts and widths. The current 64-column
  selection limit excludes later columns from this pass, even though other
  onceover detectors screen them.

The [scale benchmark](../benchmarks/README.md) covers the profiler's existing
large-table behavior. Its late-column checks do not validate discovery by this
pass beyond the pass's own width cap.

## References

- Mansinghka, Shafto, Jonas, Petschulat, Gasner, and Tenenbaum (2016).
  [CrossCat: A Fully Bayesian Nonparametric Method for Analyzing Heterogeneous,
  High Dimensional Data](https://www.jmlr.org/papers/v17/11-392.html).
  *Journal of Machine Learning Research*, 17(138), 1–49.
- Saad and Mansinghka (2017).
  [Detecting Dependencies in Sparse, Multivariate Databases Using Probabilistic
  Programming and Non-parametric Bayes](https://proceedings.mlr.press/v54/saad17a.html).
  *Proceedings of AISTATS*, PMLR 54, 632–641. This describes Bayesian dependence
  queries; its method and uncertainty estimates are not implemented by `views`.
