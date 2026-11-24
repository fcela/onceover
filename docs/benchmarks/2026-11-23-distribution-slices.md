# Numeric distribution slices

The on-request `distribution_slices` pass uses a support-weighted mean
squared empirical CDF gap on a pooled quantile grid. It searches bounded
one- and two-predicate rules and validates the rule, threshold and direction
on held-out rows using a one-sided Fisher exact test with Holm correction.

## Protocol

Run from the repository root with the release build installed.
The benchmark uses NumPy and SciPy alongside onceover and Polars.

```sh
OPENBLAS_NUM_THREADS=1 VECLIB_MAXIMUM_THREADS=1 python benchmarks/distribution_slices.py --stage criteria
OPENBLAS_NUM_THREADS=1 VECLIB_MAXIMUM_THREADS=1 python benchmarks/distribution_slices.py --stage detectors
```

Each positive case uses 16 seeds and 3,200 rows. The controls use 64 seeds
per configuration and independent responses. The wider null has 600 rows,
six independent numeric nuisance columns, and the same categorical context
columns as the other fixtures. Recovery requires a reported region to have
row-set Jaccard similarity of at least 0.75 with the planted region. The
complement also qualifies for a numeric contrast, which is symmetric.
For the categorical control the reported region must match the planted
region and the stated level must be `yes`. Null counts cover all findings
of type `slice_shift`, including those for nuisance targets.

The criteria study gives each score the same candidate rules, discovery
rows, minimum CDF effect and held-out test. Only the ranking of the eight
proposals changes. It compares an equally weighted quantile grid, exact
plug-in CRPS improvement on the target's original scale, and a Gaussian
MMD approximation with 128 random Fourier features. These are split
criteria implemented in the benchmark. DisTreebution's leave-one-out tree
learner, trained forests, and Hessboost's fitted exceedance model are
outside this comparison. [DisTreebution paper](https://arxiv.org/abs/2502.05157) ·
[drforest paper](https://arxiv.org/abs/2607.23721) ·
[Hessboost objectives](https://docs.rs/crate/hessboost/0.4.8)

## Split criterion results

| Criterion | Correct rule confirmed, 144 ordinary numeric trials | Confirmed with six values scaled by 1,000,000, out of 16 | Null runs with any confirmation, out of 64 |
| --- | ---: | ---: | ---: |
| Quantile-grid CDF | 143 | 16 | 0 |
| Exact CRPS gain | 143 | 6 | 0 |
| Gaussian MMD (RFF) | 143 | 16 | 0 |

Most small synthetic cases took roughly 0.3–0.4 ms for grid scoring,
2.3–2.4 ms for exact CRPS and 1.5–1.7 ms for MMD in the NumPy harness.
For the numeric checkerboard, the corresponding medians were 1.21,
7.66 and 3.16 ms. These are comparisons of the benchmark kernels on
arm64 macOS. The measurements exclude candidate enumeration and
held-out confirmation and say nothing about the runtime of the
upstream tree and forest implementations.

The quantile grid tied MMD on recovery, was faster in this experiment,
and supports a search using additive bin counts. Exact CRPS on the
target's original units let extreme gaps dominate the rank ordering in
the outlier fixture. The shipped detector uses the quantile grid.

## Detector comparison

| Plant, 16 trials each | `slice_shifts` | `distribution_slices` |
| --- | ---: | ---: |
| Location | 13 | 16 |
| Concentrated upper tail | 16 | 16 |
| Rare upper tail | 13 | 15 |
| Spread | 1 | 16 |
| Bimodality | 0 | 16 |
| Skew | 0 | 16 |
| Tied values with a mass shift | 0 | 16 |
| Categorical interaction | 0 | 16 |
| Numeric interaction | 0 | 16 |
| Bimodality with outliers | 0 | 16 |
| **Numeric total** | **43/160** | **159/160** |
| Categorical level share | 16 | 0 |

In the 128 end-to-end null runs, `slice_shifts` returned a finding on
one wider null table and `distribution_slices` returned one finding on
each of two ordinary null tables. These are observed false-alarm counts
for this protocol. The criteria sketch uses a different deterministic
split from the shipped detector.

The median of the ten numeric case medians was 14.5 ms for
`distribution_slices` and 51.9 ms for `slice_shifts` in the release
build. This measures the full `profile()` call. On the wider null
the medians were 11.3 ms and 8.5 ms, respectively.

The numeric detector is the stronger choice for distribution-shape
discovery on these controls. Keep `slice_shifts` available for
categorical level-share findings, specialized tail and median reports,
and its depth-three search at the deep budget. The evidence supports
adding the new opt-in detector alongside it.

The corpus run produced 223 passing machine-checked expectations out of
228 before and after registration. The same four datasets account for
the existing failures.
