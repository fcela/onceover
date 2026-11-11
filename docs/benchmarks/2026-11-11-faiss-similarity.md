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

Environment: AMD EPYC 9D25, 4 vCPUs visible, on a shared-CPU
virtual machine that also ran other work during the measurement
window — absolute times carry that caveat, and the ratios
between implementations measured back-to-back on the same data
are the load-bearing numbers, as the protocol states. Rust
1.99.0 (release build, workspace profile), Python 3.12.3,
faiss-cpu 1.16.0, numpy 2.5.4. FAISS ran with its default 4
OpenMP threads; the Rust assignment step uses rayon with its
default thread count (4). Wall time is the median of 3 runs and
covers training only (not matrix loading, not the final inertia
pass). Final inertia is the uniform recomputation of the
protocol — one full assignment pass in float64 accumulation
over float32 distances — applied to every implementation's
centroids; each implementation's inertia was identical across
its 3 runs on every dataset.

### Lloyd's k-means: Rust against FAISS

| Dataset | n | d | Rust time (s) | FAISS time (s) | Rust / FAISS | Final inertia (Rust) | Final inertia (FAISS) |
|---|---|---|---|---|---|---|---|
| adult-census-income | 48,842 | 6 | 0.409 | 2.188 | 0.19× | 71,217.40 | 71,217.40 |
| bank-marketing | 45,211 | 7 | 0.438 | 2.172 | 0.20× | 111,802.20 | 111,802.32 |
| taiwanese-bankruptcy | 6,819 | 95 | 1.014 | 4.128 | 0.25× | 453,083.95 | 453,083.95 |
| metro-interstate-traffic | 48,204 | 5 | 0.251 | 2.419 | 0.10× | 116,013.74 | 116,013.74 |
| bike-sharing-hourly | 17,379 | 16 | 0.425 | 2.517 | 0.17× | 123,313.11 | 123,313.10 |
| synthetic (seed 259001) | 1,000,000 | 32 | 13.227 | 10.787 | 1.23× | 3,724,725.20 | 3,724,725.74 |

Starting from the shared initial centroids, the two Lloyd's
implementations reach the same optimum on every dataset: final
inertias agree to within 0.001% (the residual differences are
float32 summation order). No cluster was empty at any epoch on
any dataset. Against the frozen success bar — Rust within 3× of
FAISS's wall time at a final inertia within 1% — the Rust
implementation clears the bar on all six datasets. On the five
corpus frames it is 4–10× faster than FAISS: FAISS's run time is
nearly flat at 2–4 s across frames from 6,819 to 48,842 rows, a
fixed-cost floor that dominates at profiling scale. At
1,000,000 × 32 the picture inverts and FAISS's kernels are
faster, by 1.23× — throughput, not overhead, is what binds
there. Only `max_points_per_centroid` was changed on the FAISS
side, as the protocol specifies; its other defaults (including
`min_points_per_centroid` = 39) are as shipped.

### E-means: Rust against FAISS contrib

| Dataset | Rust time (s) | Rust inertia vs Lloyd | FAISS time (s) | FAISS inertia vs Lloyd |
|---|---|---|---|---|
| adult-census-income | 0.041 | 85,085.84 (+19.5%) | 4.483 | 74,483.16 (+4.6%) |
| bank-marketing | 0.080 | 122,954.82 (+10.0%) | 5.663 | 112,267.91 (+0.4%) |
| taiwanese-bankruptcy | 0.105 | 515,562.77 (+13.8%) | 0.631 | 508,345.79 (+12.2%) |
| metro-interstate-traffic | 0.045 | 115,079.83 (−0.8%) | 6.370 | 116,503.62 (+0.4%) |
| bike-sharing-hourly | 0.073 | 122,842.53 (−0.4%) | 2.069 | 123,424.09 (+0.1%) |
| synthetic (seed 259001) | 2.439 | 3,724,721.45 (−0.0%) | 91.120 | 3,724,725.84 (+0.0%) |

The two e-means runs implement the same update and differ in
details the protocol leaves to each side: the FAISS reference
implementation (`faiss.contrib.e_means.NumpyEmeans`, run with
`nepochs=3`, `batch_size=1024`, `alpha0=0.1` and the shared
initial centroids passed to `train`) shuffles the rows each
epoch under its seed, samples revival donors in proportion to
their mass, and keeps its state in float32; the Rust run takes
the batches in row order, revives deterministically by
splitting the largest-mass cluster with the ±10⁻⁴ perturbation,
and keeps its state in float64. Dead-cluster revivals in the
Rust runs: adult 1, bank 6, taiwanese 5, metro 4, bike 3,
synthetic 0.

On quality, FAISS e-means lands closer to the Lloyd optimum on
adult and bank-marketing, about equally far on
taiwanese-bankruptcy, and slightly behind the Rust run on metro
and bike-sharing, where the Rust run matches or beats Lloyd
itself; on the synthetic mixture both are at the Lloyd optimum.
Neither e-means is uniformly at Lloyd quality after 3 epochs,
and the gap is dataset-dependent (−0.8% to +19.5% across the
two implementations). On time, the Rust e-means is 6–142×
faster than the FAISS reference implementation, which is a
NumPy reference with FAISS assignment, not an optimised kernel:
on the synthetic frame it takes 91.1 s against the Rust run's
2.4 s.

### Install cost of the external option

Measured in this environment: the faiss-cpu 1.16.0 wheel
(cp310-abi3, manylinux x86_64) is 19,485,185 bytes and installs
to 28 MB on disk. It also requires numpy where that is not
already present (numpy 2.5.4 wheel: 16,727,283 bytes). The
in-core option adds no dependency and no installed size.

## What it would buy

The audit reduced the similarity-shaped questions to two that
are genuinely distance questions. Each is evaluated against the
baseline it would have to beat, with what this study measured.

**Approximate near-duplicate groups over all rows.** Baseline:
the leave-one-column-out exact-hash scan, which is exact but
blind to rows that differ a little in many columns, and is
skipped above 50,000,000 cells. What the measurement adds is a
feasibility result: distance machinery is cheap at every size
the library profiles. One full assignment pass over the
1,000,000 × 32 synthetic frame costs about 0.5 s in-core (the
13.2 s Lloyd run is 25 such passes plus updates), so a
nearest-neighbour near-duplicate pass need not sample or skip
at these sizes. What the measurement does not settle is the
statement itself: the distance threshold, in standardised
units, that separates a near-duplicate group from ordinary
close rows, and what the report verifier would recount. That
design work is the remaining cost of this candidate, not
computation.

**Cluster structure as a statement, including past the views
column cap.** Baseline: the views pass, capped at 64 eligible
columns and reporting density clusters in density currency.
The measurement shows the computational cap is removable:
in-core Lloyd clusters the 95-column taiwanese-bankruptcy
frame in 1.0 s and the 1,000,000-row synthetic in 13.2 s. The
barrier that remains is not computational. K-means has no
model-selection score — the protocol's k = 16 is a study
parameter, not a profiling answer — and a cluster claim would
need a k-selection rule and a verifier recount that do not
exist yet. Distance clusters also answer a different question
than the views density clusters; nothing measured here says
the distance version serves a profiling reader better.

**Duplicate encodings.** No buy, as the audit concluded: the
question is exact, hashing already answers it exactly, and a
distance version would be strictly weaker.

**E-means specifically.** At profiling scale it buys nothing:
full Lloyd at 25 epochs costs about a second at most in-core
on every corpus frame (medians 0.25–1.01 s), so the
mini-batch variant's speed (0.04–0.11 s) has no consumer, and
its 3-epoch quality is inconsistent
(−0.8% to +19.5% against Lloyd across datasets). Its regime is
the 1,000,000-row frame, where the Rust e-means matches the
Lloyd optimum in 2.4 s against 13.2 s — which matters only if
profiling ever runs at that scale without sampling. The
current passes deliberately cap and sample instead.

## Recommendation

No adoption now, and the implementation route is settled if
that changes.

- **Not the external FAISS pass, in either role.** At
  profiling scale the in-core Rust Lloyd is 4–10× faster than
  FAISS Lloyd from the same initialisation, to the same
  optimum; FAISS leads only at 1,000,000 × 32, by 1.23×. The
  external option costs a 19.5 MB wheel (28 MB installed), and
  its e-means is a NumPy reference implementation that is
  slower than the in-core Rust e-means on every dataset
  measured (6–142×) while following a different batch order
  than the specification it references — an external dependency
  that is neither faster nor canonical.
- **Not a new detector yet.** The two genuinely distance-shaped
  candidates — near-duplicate groups beyond exact equality,
  and cluster structure as a statement — each still lack the
  statement design, the threshold or k-selection baseline, and
  the verifier recount. The audit's binding limits (row
  samples, the views column cap) turn out not to be
  computational: the machinery measured here removes them
  whenever a statement design justifies it.
- **If a distance-based statement is designed later, build it
  in-core in Rust.** Lloyd's clears the frozen bar on all six
  datasets — worst case 1.23× FAISS's wall time, final inertia
  within 0.001% — with no new dependency, in about 300 lines
  against the existing numeric extraction (the harness in
  `crates/onceover-core/examples/kmeans_research.rs` is that
  implementation, kept as research code). E-means should not
  be part of it at profiling scale; revisit it only for
  unsampled million-row inputs.

## Provenance

- Base commit: a804dbf606673a83aba9d44445bca99c925a8351
  (version 0.1.4).
- Corpus registry: `benchmarks/corpus/registry.json` in this
  repository; row counts above are the registry's counts,
  cross-checked against the bundled CSV files.
- FAISS: https://github.com/facebookresearch/faiss (MIT
  license). E-means: the mini-batch exponential-moving-average
  variant shipped in `faiss.contrib`.
- Measured with faiss-cpu 1.16.0 and numpy 2.5.4 (Python
  3.12.3) and the Rust harness in
  `crates/onceover-core/examples/kmeans_research.rs` (rustc
  1.99.0). The FAISS e-means leg is `NumpyEmeans` from
  `faiss.contrib.e_means`.
- Row counts from the extraction matched the registry counts
  for all five corpus frames, and every corpus frame's rows
  were complete cases (no row dropped). The
  taiwanese-bankruptcy frame yields 96 numeric columns through
  the extraction; one (`Net Income Flag`) is constant and
  drops out under the protocol's constant-column rule, leaving
  d = 95 as the protocol states.
