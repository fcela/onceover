# FAISS-style similarity machinery: audit, Rust k-means / e-means, and an integration recommendation (2026-11-11)

## Question

Several parts of a profile are similarity questions: the row
clusters behind the views findings, near-duplicate rows, and
duplicate encodings of a column. The library never computes a
distance between rows. The views pass clusters by predictive
density (a deterministic divisive search, conjugate per-column
models, leave-one-out scoring, a complexity penalty), not by
distance, and its findings (`modal_share`, `spike_cells`) are
stated in density currency. This study asks what FAISS-style
machinery — Lloyd's k-means and the mini-batch e-means update
over dense float vectors under Euclidean distance, plus
nearest-neighbour search — could add, and in what form: an
implementation in Rust inside the core, an opt-in external pass
on Python FAISS (the second-pass screener pattern), or no
adoption.

Non-goals: replacing the views pass's density clustering, and
GPU support.

## Audit: what is similarity-shaped today, where it is approximate, and what breaks first

No detector computes a row-to-row Euclidean distance. The
similarity-shaped sites, with their bounds as shipped at the
base commit of this study (version 0.1.4), are:

| Site | What it computes instead of distance | Bound / sample | First failure as rows grow |
|---|---|---|---|
| `views` (`crates/onceover-mines/src/views.rs`) | Divisive median-split clustering scored by conjugate marginal likelihood; rows assigned by leave-one-out log predictive density | `row_cap` 20,000 rows (standard) / 50,000 (deep); search decisions on `search_rows` 2,000; at most 64 eligible columns (`max_columns`), at most 8 clusters per view, 256 tried merges, a 300,000,000 cell-visit work budget | The sample fractions shrink: on the largest bundled corpus frame (48,842 rows) the pass scores 41% of rows and searches on 4.1%; at 1,000,000 rows those shares are 2% and 0.2%. Columns past 64 are not clustered at all. |
| Near-duplicate rows (`crates/onceover-mines/src/rows.rs`) | Exact hash equality on all columns but one (leave-one-column-out hashing), with a chance-coincidence correction | All rows in scope, but the scan is skipped above 50,000,000 cells (`MAX_NEAR_DUPLICATE_CELLS`); at most 3 statements | The cell cap: a 1,000,000-row frame is scanned at 32 columns (32,000,000 cells) and skipped at 51 columns. Rows that differ a little in many columns — the usual near-duplicate shape for numeric data — are invisible at any size, because equality is exact. The swapped-values check in the same file samples 20,000 rows. |
| Exact duplicate rows (`crates/onceover-core/src/dup.rs`) | 64-bit row hashing with full verification of hash buckets | No row cap: one hash entry per row, all rows | Memory and time grow linearly with no stated bound; this is the one duplicate check that still runs at 1,000,000 rows, and it answers only the exact-equality question. |
| Duplicate encodings (`crates/onceover-deps/src/dupe.rs`, `fd.rs`) | Hash-based candidate discovery over column pairs, verified on exact canonical encodings | Deterministic row sample per pair: `row_cap` 100,000 for duplicate encodings, 200,000 (standard) for functional dependencies, with a 2,048-row screen inside the FD pass | Sampled discovery: a mapping that only appears past the sample can be missed. The question itself is exact (two encodings of one grouping), so a distance version would be strictly weaker; see the evaluation below. |
| Encoded matrix (`crates/onceover-mines/src/encode.rs`) shared by the predictability screen, decision tables, and rule mining | Quantile binning / level coding of every usable column | `row_cap` 20,000 rows (standard), `feature_cap` 48 features | Every consumer of the matrix inherits the 20,000-row sample and the 48-feature cap. |
| One-dimensional neighbour gaps (`crates/onceover-mines/src/surprises.rs`, `crates/onceover-deps/src/boundaries.rs`, `crates/onceover-mines/src/disc.rs`) | Sorted-order gaps: an extreme value's distance to its nearest distinct neighbour on one axis; boundary piles against the median count of nearest distinct values; discontinuity scores from nearest evaluation points | Surprises and boundaries scan all values on one column; discontinuities stride-sample to `row_cap` 20,000 | These are one-dimensional by construction. No multivariate version exists: a row that is ordinary on every column and unusual only in combination has no distance-based statement. |
| Pair closeness for order constraints (`crates/onceover-deps/src/order.rs`) | A per-pair scale statistic, median \|a − b\| / max(\|a\|, \|b\|) over sampled complete rows, used as a gate (at most 0.2) | Sampled complete rows per pair | A gate, not a clustering: it decides whether two columns may be compared, and computes no row geometry. |
| String near-matches (`crates/onceover-deps/src/content.rs`) | Bounded edit distance between distinct string values | A length-difference window and an edit-distance limit per pair of distinct values | Cost grows with the number of distinct values, not rows; it is the only true distance in the library, and it is over strings, not rows. |

Two conclusions from the audit. First, the binding limits are
not raw speed: they are the row samples (2,000–20,000 rows for
the model-based passes), the 64-column views cap, and the
50,000,000-cell near-duplicate cap. Second, two of the three
similarity questions in the issue are exact questions (exact
duplicates, duplicate encodings) that hashing already answers
exactly; only near-duplicates beyond exact equality, and row
cluster structure as a statement in its own right, are genuinely
distance questions.

The corpus's largest frames by row count in the registry
(`benchmarks/corpus/registry.json`) are test-only datasets
whose data is not committed (online-retail-ii, 1,067,371 rows;
credit-card-fraud, 284,807; incident-event-log, 141,712), so
they cannot be measured here. The largest bundled frames,
which this study measures, are adult-census-income (48,842
rows), metro-interstate-traffic (48,204 rows), and
bank-marketing (45,211 rows).

## Frozen protocol

Stated before any benchmark result was seen, and held constant
across datasets:

- Implementations: (a) Lloyd's k-means in Rust, (b) e-means in
  Rust, (c) FAISS Lloyd's via `faiss-cpu` in Python, (d) FAISS
  e-means via `faiss.contrib` if importable in this
  environment; if (d) is unavailable it is reported as
  unavailable, with no substitute variant.
- The Rust implementations are a research harness
  (`crates/onceover-core/examples/kmeans_research.rs`), not
  library code: they change no library behaviour, no finding,
  and no part of the default detector set. Data enters through
  the library's existing numeric extraction
  (`onceover_core::typed::extract`): numeric columns only
  (integer, unsigned, and float columns, with decimal and
  duration columns arriving as floats through that extraction),
  rows with a null in any numeric column dropped (complete
  cases), constant columns dropped, and each remaining column
  standardized to mean 0 and standard deviation 1 before any
  distance is computed. The standardized matrix is written once
  as float32 and shared by all four implementations, so every
  implementation clusters identical bytes.
- Datasets: the three largest bundled corpus frames above;
  one mid-size corpus frame, bike-sharing-hourly (17,379 rows);
  taiwanese-bankruptcy (6,819 rows, 95 numeric columns) as a
  wide-frame case for the column-cap question; and one
  synthetic frame of 1,000,000 rows × 32 numeric columns,
  generated by the harness with seed 259001 from a stated
  distribution (a mixture of 8 spherical Gaussians in 32
  dimensions: centres drawn from a normal distribution with
  standard deviation 3 per coordinate, rows assigned to a
  component uniformly, values centre plus standard normal
  noise).
- Parameters: k = 16 for every headline comparison. Lloyd's
  runs 25 epochs (the e-means paper's reference run), full
  batch, no early stop. E-means runs 3 epochs with mini-batch
  size 1,024; the update is the sum-and-count exponential
  moving average (per-cluster mass N and centroid c: N is
  decayed by 1 − α and increased by α × batch count, and c
  moves toward the batch mean by α × batch count / new N),
  with initial α₀ = 0.1 cosine-annealed toward 0.1 × α₀ over
  the run, initial mass 2⁻¹⁶, dead-cluster revival every 5
  batches (a cluster whose mass falls below 0.1 × the mean
  mass is revived by splitting the largest-mass cluster, with
  a deterministic ±10⁻⁴ relative perturbation), and a final
  pass that replaces each centroid by the unweighted mean of
  the rows assigned to it during the last epoch. The FAISS
  e-means run uses the same batch size, epoch count, and α₀
  where its API accepts them.
- Initialisation: the same k initial centroids for every
  implementation and dataset — k distinct data rows chosen by
  a splitmix64 generator with seed 25916 (partial
  Fisher–Yates), written to a shared file that both harnesses
  read. FAISS Lloyd's accepts initial centroids through its
  Python API; FAISS e-means accepts them through `train`.
- FAISS Lloyd's is run with its per-iteration training cap
  raised so all rows train every iteration
  (`max_points_per_centroid` set above the largest dataset's
  rows per centroid). At its default of 256 points per
  centroid, FAISS would train on at most 4,096 rows per
  iteration at k = 16, which is a different, sampled
  computation and is not compared here.
- Reporting: the median of 3 runs for wall time, and final
  inertia for every implementation × variant × dataset.
  Final inertia is computed the same way for every run: a
  separate full assignment pass with the final centroids,
  summing squared Euclidean distance to the nearest centroid
  over all rows. A trainer's internal objective is not used.
- Success bar for the in-core option, fixed in advance: the
  Rust implementation is competitive if its wall time is
  within 3× of FAISS's on the same dataset at a final inertia
  within 1% of FAISS's. The factor actually measured is
  reported either way.
- Environment: recorded in the results section with the
  numbers (machine, core count, Rust / Python / faiss-cpu
  versions). This is a shared-CPU virtual machine; absolute
  times carry that caveat, ratios between implementations
  measured back-to-back on the same data are the load-bearing
  numbers.

## Results

Results are added to this record as the benchmark legs
complete. The audit and protocol above are the published
starting point of the study.

## What it would buy

Evaluation of the candidate statements, each with the baseline
it would be judged against, is added with the results.

## Recommendation

The recommendation among in-core Rust, an opt-in external pass
on Python FAISS, and no adoption is stated at the end of this
record, tied to the measured numbers and the audit.

## Provenance

- Base commit: a804dbf606673a83aba9d44445bca99c925a8351
  (version 0.1.4).
- Corpus registry: `benchmarks/corpus/registry.json` in this
  repository; row counts above are the registry's counts,
  cross-checked against the bundled CSV files.
- FAISS: https://github.com/facebookresearch/faiss (MIT
  license). E-means: the mini-batch exponential-moving-average
  variant shipped in `faiss.contrib`.
