# Architecture, Performance, Precision, and Token Economy Overhaul (2026-11-22)

## Provenance

- Version: `onceover 0.1.7`, report schema `0.8`
- Platform: macOS arm64, Python 3.13.7, Polars 2.0.0
- Scale protocol: `python benchmarks/scale.py --rows 200000 --columns 200 --threads 2` (targeting constrained 2-core / 8 GiB code executors)
- Gate protocol: `python benchmarks/gate/run.py` across all 27 registry datasets (20 primary + 7 held out)

## 1. Scale Benchmark (200,000 rows × 200 columns)

Key optimizations:
- Cached `RowKeys` (`OnceLock<RowKeys>` on `TypedTable`) with Rayon-parallel streaming column hashing and compact `Const`/`U8`/`U16`/`U32` ranks (drops all-distinct sort buffers immediately).
- Direct Polars `downcast_iter()` extraction, chunk-walking `NumericValuesIter`, O(N) `select_nth_unstable_by` quartiles/medians with Kahan summation, and scratch-buffer reuse in `card.rs`.
- Zero-allocation pre-scan in `boundaries.rs`, Rayon-parallel pair evaluation + `pooled_dev_sorted` merge-difference in `levels.rs`, borrowed `&str` encoding in `encode.rs`, and eager release of intermediate frames/encodings in `pipeline/`.

| Metric | Prior baseline (v0.1.7, 4 threads) | Current build (2 threads) |
|---|---:|---:|
| Wall time (`profile_seconds`) | 78.93 s | **17.17 s** (4.6× faster on half the cores) |
| Peak RSS during profile | — | **1.05 GiB** total (`0.680 GiB` additional) |
| Scale correctness checks | 9 / 9 | **9 / 9** |
| Noise targets flagged | 0 | **0** |

## 2. Precision Gate (`benchmarks/gate/run.py`)

Five statistical calibrations were added without regressing planted recall or reference statements:
1. `breaks.rs` (`excursion_scan`): leave-one-run-out background rate (`other_flagged`) so heavy-tailed series do not understate their marginal excursion rate, plus global robust-z gating for lone-point runs already owned by `outliers`.
2. `panels.rs` (`entry_block` / `exit_block`): exact hypergeometric row-permutation null expectation per period to suppress boundary-adjacent order-statistic decay.
3. `panels.rs` (`outlier_entity`): leave-one-row-out entity mean plus Bonferroni-calibrated permutation z-test to guard against row-count skew and single heavy-tailed cells.
4. `surprises.rs`: global-column Bonferroni binomial tail calibration for conditional numeric `extreme_value` findings.
5. `axisbreaks.rs` (`window_contrast`): empirical shuffle tail floor plus Mann-Whitney rank-sum check to prevent zero-inflated/heavy-tailed single spikes from firing `regime_contrast`.

Additionally, `frame.rs` rule `(h2)` (`looks_like_structured_code`) classifies high-cardinality (>500 distinct) whitespace-free structured string codes as `identifier` so panel passes follow the frame's real entity axis.

### Primary Gate Totals (20 datasets)

| Metric | Previous Ceiling | Measured Head | Delta |
|---|---:|---:|---:|
| `permuted.false_full` | 39 | **7** | −82% |
| `permuted.false_4k` | 22 | **4** | −82% |
| `permuted.false_full_target` | 17 | **1** | −94% |
| `permuted.false_4k_target` | 10 | **1** | −90% |
| `reference.must_not_*` (all 6 scopes) | 0 | **0** | 0 |
| `planted.found_full` / `found_4k` | 61 / 59 (of 76) | **61 / 59** | 0 |
| `reference.must_full_default` / `target` | 17 / 17 (of 19) | **17 / 17** | 0 |
| `truth.mismatch` / `error` | 0 / 0 | **0 / 0** (846 ok) | 0 |
| `redundancy.repeated_default` / `target` | 0 / 0 | **0 / 0** | 0 |
| `cost.profile_seconds` | — | **5.09 s** | — |

Held-out totals (7 datasets): `permuted.false_full = 9` (down from 19), `permuted.false_full_target = 2` (down from 12), `planted = 9/12` full and `9/12` in 4k, `truth.mismatch = 0`, `truth.error = 0` (242 ok).

Ceilings in `benchmarks/gate/ceilings.json` have been lowered to `7 / 4 / 1 / 1`.

## 3. Rust-First Core & Token Economy

- Moved digest & brief rendering (`crates/onceover-report/src/digest.rs`), compact & budgeted JSON (`compact.rs`), finding lookup/select/diff (`diff.rs`), multi-table foreign-key profiling (`crates/onceover/src/many.rs`), and frame spec + hierarchy discovery (`spec.rs`) into Rust so Python and future host-language bindings (e.g. R) share one implementation.
- Modularized `crates/onceover/src/pipeline/` into 8 stage modules and reduced the non-UI Python layer by ~1,700 lines (`_hierarchies.py` deleted).
- Added short report-local handles (`"f1"`, `"g1"`), `report.to_brief(2_500)` (dense schema strip + short handles), `report.to_digest(..., compact=True, include_schema=True)`, `report.to_json(compact=True)`, and `--format brief` / `--compact` CLI flags.

## 4. Partitioned Scagnostics v2

Upgraded `partition_scagnostics` (`crates/onceover-mines/src/partition_scagnostics/{metrics.rs,search.rs}`, `crates/onceover/src/partition_scagnostics.rs`, `python/onceover/_checks.py`):

- **5-metric engine (shared-MST + nonparametric curve)**: Added `functional` (bias-adjusted nonparametric rank-space $R^2$ over 12 discovery-fitted quantile bins, capturing multi-peak/non-monotonic curves), `binned_stringy` (MST diameter / total length), and `binned_clumpy` (O(V) rerooting-DP MST cluster gap with point-weight guard) alongside `monotonic` and `binned_striated` on a robust `[q01, q99]`-scaled 12×12 grid, with all three MST metrics computed in one Prim pass per candidate.
- **Pipeline & frame-model integration**: Content-keyed sampling (`TypedTable::row_keys()`), exclusion of `identifier` / `row_number` / `timestamp_part` / exact-identity duplicates, integer-coded `Flag`/`Dimension` promotion to exact `Levels` rules, salience-ordered target-first pairs, budget-scaled caps, and cooperative in-pass deadlines.
- **Richer rules & statistical guards**: Top-12 one-vs-rest splits on categoricals up to 32 levels, dual `partition_gain` + `standout_subgroup` objectives (checking both branches down to 5% share, including minority sign reversals), an all-metric rank-curve range-restriction guard, a 2D-span flat-axis-collapse guard, a minimum-4-distinct-values Spearman guard, labeled per-group claims, and `report.rows(id, df)` support.
- **Case-study gaps closed**: Capital Bikeshare `hr × registered` across `workingday` now recovers **12 / 12 seeds** (`functional` gain `+0.123`, `p_adj = 0.005`, up from `0 / 12`), the 7%-minority opposite-slopes trial recovers **15 / 16 seeds** (up from `0 / 16`, with **0 / 32** false positives on homogeneous 7% controls), and `functional` produced **0 / 288** false positives across all null and range-restriction controls.

### 12-Scenario Release Benchmark (`cargo run --release -p onceover-mines --example partition_scagnostics -- --benchmark`)

Wide 12,000-row × 43-column fixture (all 4 metrics): **118.8 ms** median.

| Scenario | Runs | `monotonic` | `binned_striated` | `binned_stringy` | `binned_clumpy` | Any | Median ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `opposite_slopes` | 16 | 16 | 0 | 0 | 0 | **16 / 16** | 8.51 |
| `numeric_split` | 16 | 16 | 0 | 0 | 0 | **16 / 16** | 8.34 |
| `two_column_interaction` | 16 | 16 | 0 | 0 | 0 | **16 / 16** | 10.69 |
| `stripes` | 16 | 0 | 16 | 0 | 12 | **16 / 16** | 45.36 |
| `curved_arc` | 16 | 0 | 16 | 16 | 0 | **16 / 16** | 40.44 |
| `clumpy_regimes` | 16 | 0 | 0 | 8 | 8 | **16 / 16** | 28.90 |
| `standout_subgroup` (10% of 10 levels) | 16 | 16 | 0 | 0 | 1 | **16 / 16** | 44.49 |
| `range_restriction` (sliced parabola) | 32 | 0 | 0 | 0 | 0 | **0 / 32** | 1.26 |
| `homogeneous` (null) | 64 | 0 | 0 | 0 | 2 | **2 / 64** | 1.41 |
| `independent` (null) | 64 | 0 | 1 | 1 | 0 | **2 / 64** | 7.22 |
| `latent` (null) | 64 | 0 | 1 | 0 | 0 | **1 / 64** | 2.45 |
| `shuffled_labels` (null) | 64 | 0 | 1 | 0 | 1 | **2 / 64** | 2.78 |

Across the 20 gated real datasets: **0 column-order diffs (`revΔ = 0`) across all 20 tables**, **0 row-shuffle diffs (`shufΔ = 0`) across all 19 tables with repeated-value columns**, and complete elimination of calendar/row-counter noise and constant-axis geometric collapses.
