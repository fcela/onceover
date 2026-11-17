# UCI expansion: corpus scaffolding for 14 datasets, and the scagnostics scoping

This record documents the first step of the UCI expansion (issue #348):
the selection of 14 datasets from the UCI Machine Learning Repository,
a prepare script and attribution file for each, and corpus registry
entries in test-only mode. It also carries the scoping analysis for
bivariate shape diagnostics (issue #346), which is posted on that issue
as well.

One step of the expansion did not happen in this pass, and everything
below is written so the remaining step is mechanical: **the data fetch
itself**. The fetch is a network operation out of this sandbox that
requires an approval which was not granted in this session, so no
`data.csv` was fetched, no hash could be pinned, no corpus expectations
could be measured, and no gate reference statements could be verified
against data. Nothing was substituted from another source and no
dataset content was copied from web pages. Each registry entry is
therefore `test-only` with a skip reason, exactly like the existing
test-only entries whose files are absent: the corpus runner reports
them skipped and moves on. The conversion step at the end of this
record turns each entry into a bundled, hash-pinned entry with
measured expectations, and adds the gate subset, once the fetch is
approved.

## The problem

The corpus and the digest gate rest on a limited set of tables: 41
corpus datasets and 25 gate datasets, most of them a handful of
well-known business tables, a panel, and generated packs. The gate's
reference statements — the facts a correct briefing must state — can
only be written for tables whose properties are documented well enough
to state the facts before the library runs. UCI is the largest source
of such tables: hundreds of datasets with declared targets, variable
descriptions, and documented structure, fetchable through the
repository's own `ucimlrepo` package, which returns the metadata
(roles, declared targets, DOI, license) alongside the data. Most UCI
datasets are CC BY 4.0, which the corpus license gate admits with
attribution; anything else stays test-only or out.

## Selection

Criteria, applied in order: (a) documented properties from which
reference statements can be written before seeing library output;
(b) structural variety the current corpus and gate lack; (c) size
compatible with the corpus's runtime discipline (the largest current
bundled table is ~120k rows; the gate's per-PR run is ~18 minutes for
25 datasets, so gate candidates must be small or fast).

The pool is a new corpus pack, `uci`, with 14 datasets:

| Dataset (UCI id) | Structure the current set lacks |
| --- | --- |
| Iris (53) | The canonical clustered numeric pair (petal length × width, three documented clusters); tiny and fast |
| Wine Quality (186) | Ordinal sensory target over physicochemical measures, with a documented skewed quality distribution |
| Air Quality (360) | A long hourly sensor series with a documented missing-value code (−200) and documented sensor drift |
| Mushroom (73) | An all-categorical table (22 features) with documented exact rule structure |
| Dry Bean (602) | Geometric features with near-deterministic relations between them, by construction |
| Electrical Grid Stability (471) | Simulated dynamics with documented nonlinear structure, and a label documented as an exact function of a numeric column |
| Gas Sensor Array Drift (224) | A genuinely wide table (128 features) with batch structure and documented drift over 36 months |
| Heart Disease (45) | Small, messy clinical intake: mixed types, coded categoricals, `?` missing values |
| Spambase (94) | Text-intake shape: 57 heavy-tailed word/character frequency features |
| MAGIC Gamma Telescope (159) | Continuous physical measurements, ~19k rows, documented 65/35 class split |
| Letter Recognition (59) | A many-class target (26 letters) over 16 integer features, 20,000 rows |
| Concrete (165) | A documented saturating relation (age → strength) on a small table |
| Communities and Crime (183) | Wide (127 features) with heavy documented missingness concentrated in whole attribute groups |
| Yeast (110) | Strong class imbalance (10 localization sites) over 8 score features, with an identifier column |

The panel gap is only partly filled: Air Quality is a single long
series and Gas Sensor Drift is batch-structured, but the corpus still
has exactly one true long-form panel (state unemployment). A genuine
multi-entity panel from UCI was considered and deferred (below).

### Rejected, with reasons

- **Covertype (581k rows), YearPredictionMSD (515k rows), Individual
  Household Electric Power Consumption (~2M rows)** — size; the
  corpus's runtime discipline admits none of them.
- **Beijing Multi-Site Air Quality (501)** — the one genuine panel
  found (12 stations); distributed as 12 separate station files, so
  the fetch and preparation are multi-file. Deferred on that ground,
  not on merit.
- **Statlog Shuttle** — its role (continuous features, imbalanced
  classes, ~58k rows) duplicates MAGIC Gamma's at worse size and
  interest.
- **Online News Popularity (39.7k × 61)** — size and runtime for the
  pool; its skewed-target role is covered by Wine Quality and
  Spambase.
- **Superconductivity (21k × 81)** — the wide-continuous role is
  covered by Gas Sensor Drift and Communities and Crime.
- **Waveform** — repository-hosted generated data; the corpus already
  carries a generated pack, and the pool's purpose is documented real
  tables.
- **Ionosphere** — the continuous moderate-width role is covered by
  Electrical Grid Stability and MAGIC Gamma.
- **Parkinsons (195 rows), Glass (214), Wholesale Customers (440)** —
  too small to exercise screening multiplicity or selection.
- **Steel Plates Faults** — multi-label targets do not fit the
  corpus's single-target designation pattern.
- **Human Activity Recognition (561 features)** — the wide role is
  covered by Gas Sensor Drift, with a heavier fetch and preparation.
- **Beijing PM2.5 (single station)** — Air Quality covers the
  real-sensor-series role and adds the documented sentinel code.

## Provenance

License discipline: a dataset is bundled only when the license the
repository metadata reports is CC BY 4.0. Each `prepare.py` enforces
this at preparation time — it prints the fetched metadata and refuses
to write `data.csv` unless the reported license is CC BY 4.0. Where
the DOI and license below were read from the repository dataset page
in this session (2026-11-17), the entry says so; where they were not,
the first prepare run confirms them from the fetched metadata and the
attribution file is updated then. Hash pins exist only after a fetch,
so every pin is deferred to the conversion step.

| Dataset | UCI id | DOI | License | Status |
| --- | --- | --- | --- | --- |
| Iris | 53 | 10.24432/C56C76 | CC BY 4.0 | DOI and license read from the dataset page |
| Wine Quality | 186 | — | CC BY 4.0 (to confirm) | DOI and license confirmed at first fetch |
| Air Quality | 360 | — | CC BY 4.0 (to confirm) | DOI and license confirmed at first fetch |
| Mushroom | 73 | — | CC BY 4.0 (to confirm) | DOI and license confirmed at first fetch |
| Dry Bean | 602 | — | CC BY 4.0 (to confirm) | DOI and license confirmed at first fetch |
| Electrical Grid Stability | 471 | 10.24432/C5PG66 | CC BY 4.0 | DOI and license read from the dataset page |
| Gas Sensor Array Drift | 224 | 10.24432/C5RP6W | CC BY 4.0 | DOI and license read from the dataset page |
| Heart Disease | 45 | 10.24432/C52P4X | CC BY 4.0 | DOI and license read from the dataset page |
| Spambase | 94 | — | CC BY 4.0 (to confirm) | DOI and license confirmed at first fetch |
| MAGIC Gamma Telescope | 159 | 10.24432/C52C8B | CC BY 4.0 | DOI and license read from the dataset page |
| Letter Recognition | 59 | 10.24432/C5ZP40 | CC BY 4.0 | DOI and license read from the dataset page |
| Concrete | 165 | 10.24432/C5PK67 | CC BY 4.0 (to confirm) | DOI read from the dataset page; license confirmed at first fetch |
| Communities and Crime | 183 | — | CC BY 4.0 (to confirm) | DOI and license confirmed at first fetch |
| Yeast | 110 | — | CC BY 4.0 (to confirm) | DOI and license confirmed at first fetch |

Targets are designated from the UCI metadata in every entry (the
registry's top-level `target` key, the #315 pattern), and each entry's
notes record the designation. One designation is not
repository-declared: Air Quality declares no single target, so
`benzene_gt` is designated on a documented-purpose basis — the source
study frames the device as estimating benzene, the reference-analyser
column — and the entry's notes say exactly that.

Preparation follows the corpus standard (decode and rename only; no
rows dropped, no values imputed, no outliers touched). Each
`prepare.py` fetches with `fetch_ucirepo`, applies an explicit rename
map (a fetched column the map does not know, or a mapped column that
is absent, fails loudly rather than drifting the schema), decodes the
documented codes to the documentation's labels, decodes `?` to null
where the dataset documents it as the missing symbol, and prints the
prepared shape and SHA-256 — the value the registry pin takes at
conversion. Two preparations deserve a note: Air Quality's −200 code
is kept as data (it is the documented missing encoding, and stating it
is the profiler's job), and Mushroom's 22 feature letter-codes are
kept as fetched in this first preparation, because their decode
tables live in the fetched variables metadata and decoding them is a
separate reviewed step.

## The intended gate subset

Five of the 14 are intended for the gate, split deliberately. No gate
entries are added by this change — reference statements must be
verified against the data, which requires the fetch — so the subset,
its statements and its split are recorded here and take effect at
conversion. Every statement below is written from the documentation,
is tiered by the standing rule (must for
encoding/driver/missingness/impossible-value/sentinel kinds, should
otherwise; all provisional), and is marked for what it is: its check
expression is **to be evaluated at first fetch**, before any library
run on the table.

**Gated: Iris** (tiny, fast, and its facts are canonical).
- `setosa-separation` (should, relates): every setosa petal length is
  below every other class's petal length — the documentation's linear
  separability of setosa, operationalised on petal length. Check:
  max petal_length over class = setosa < min petal_length over the
  rest.
- `class-balance` (should, relates): the three classes hold 50 rows
  each, as documented. Check: every group count of class equals 50.

**Gated: Wine Quality** (fast; an ordinal target the gate lacks).
- `quality-scale` (should, sentinel/boundary): quality stays inside
  the documented 0–10 scale. Check: min(quality) ≥ 0 and
  max(quality) ≤ 10.
- `density-alcohol` (should, relates): density and alcohol are
  strongly negatively related (ethanol lowers density; the relation
  is a documented property of the table). Check: Spearman correlation
  of density and alcohol < −0.8.

**Gated: Mushroom** (fast; the gate has no all-categorical table).
- `stalk-root-missing` (must, missingness): stalk_root carries the
  documented `?` missing symbol, decoded to null by preparation, and
  no other column has nulls. Check: null_count(stalk_root) > 0 and
  the sum of null counts over all other columns = 0.
- `class-two-levels` (should, relates): the class takes exactly the
  two documented values, edible and poisonous. Check: n_unique(class)
  = 2.

**Held out: Dry Bean.** Its headline documented structure (the
near-deterministic geometric relations) is the shape battery's
material (see the scoping below), and that battery does not exist
yet; its identity statement (equivalent diameter is by construction
sqrt(4·area/π), to within the source's rounding) is recorded here for
the conversion. At 13,611 rows it also earns its gate place only
once the shape statements exist to justify the time.
**Held out: Air Quality.** Its drift and sentinel statements need
the ordered-axis behaviour measured on a 15-column hourly series
first; the sentinel statement (the −200 code is present in the
sensor columns; kind sentinel, tier must) is recorded here for the
conversion.

Gate-time delta of this change: zero — no gate datasets are added,
and the gate run on the final tree (below) confirms the previously
gated datasets' totals do not move. The intended subset is three
small tables; its delta is measured at conversion and recorded then.

## Measurements

Corpus on the unmodified tree (the registry without the new pack):
exit 2 — machine checks clean, documented misses on record. 30
datasets profiled, 11 skipped; machine-verified expectations
218/218 passed; documented cases found 17, missed 11, partial 4.

Corpus on the final tree: exit 2. 30 datasets profiled, 25 skipped
over 55 registry entries; machine-verified expectations 218/218
passed; documented cases found 17, missed 11, partial 4 — identical
on every shared dataset. Each of the 14 new entries is reported
skipped with its recorded reason ("data.csv is fetched by
prepare.py (ucimlrepo) and is not committed yet").

Digest gate on the final tree (`benchmarks/gate/run.py`, 441.9s):
exit 0; check_failed 0. Gated reference totals: must matched
16/16/17 of 19 (4k/8k/full, default mode), should 29/35/37 of 45;
held-out reference totals: must of 9, should of 16, check_failed 0.
This change adds no gate datasets and touches no gated input (the
gate registry, the reference files and the gated tables are all
unchanged), so these are the base tree's totals, reproduced.

pytest: 768 passed. test_release: 6 passed. release.py check:
passes. cargo fmt --check: clean. Clippy with the Rust 1.100
toolchain, `-D warnings`: clean. cargo test --workspace --locked
(release profile): 472 passed, 0 failed. (A first attempt's
compilation was interrupted by a VM reboot; the rerun completed
from the surviving build cache.)

No book page enumerates the corpus or gate datasets — the book's
dataset mentions are worked examples inside methodology notebooks —
so this change touches no book page and no strict book build is
required.

## The conversion step (what the first approved fetch run does)

1. Run each pack's `prepare.py` (requires `ucimlrepo` and network
   access to the UCI repository). Each run prints the fetched
   metadata; if any dataset's reported license is not CC BY 4.0, that
   dataset stays test-only and its registry entry and attribution
   file are corrected to the stated license.
2. Record each fetched DOI in the dataset's `ATTRIBUTION.md`, with
   the fetch date, and finalise any preparation detail the fetched
   frame settled (Wine Quality's variant column, Gas Sensor Drift's
   batch column).
3. In each registry entry: set `mode` to `bundled`, add `sha256`
   and `rows` from the prepare run's output, and remove
   `skip_reason`.
4. Run the corpus; pin each new dataset's expectations from the
   measured run, with mechanisms in the notes for anything recorded
   as missed.
5. Evaluate the draft reference checks above against the data
   (each must be True), write the gate reference files for Iris,
   Wine Quality and Mushroom, add the five gate registry entries
   (Dry Bean and Air Quality held out), run the gate, and record the
   gate-time delta in a follow-up record.

## Scoping: bivariate shape diagnostics (issue #346)

Analysis only — no implementation in this pass. The inventory below is
from the code; comments were not treated as evidence.

### (a) What the library already states about shape

- **`target_shapes`** (on request, target-gated; `crates/onceover-mines/src/shapes.rs`):
  for a designated numeric target against each predictor, it compares a
  line (linear R²), a binned curve (up to 20 equal-count bins, df-adjusted
  R²) and the best single step, and emits `step_shape` / `curved_shape`
  under the type `predictive` only when a shape is decisive over the line.
  It is one-predictor-at-a-time and reachable only with a designated
  numeric target.
- **`ordered_axis`** (default set; `crates/onceover-mines/src/axisbreaks.rs`):
  a battery over series aggregated to an axis grain. Its shape content is
  the shape of binned means along an ordered axis — monotonicity licensed
  at |Spearman| ≥ 0.5 with an isotonic-improvement bar, reversal blocks
  larger than 0.5 IQR reclassifying a pair as non-monotone, a strong
  located reversal (≥ 0.75 IQR) stated on its own, and saturation via a
  two-segment fit on 20 bins with a bootstrap licence. The taxonomy's
  `axis_shape` forms (`monotone`, `monotone_with_reversal_blocks`,
  `non_monotone_reversal`, `saturating`, `rises_then_flattens`) are all
  defined over "the binned target along the ordered axis".
- **`numeric_dependence`** (default set; `crates/onceover-mines/src/dep.rs`):
  Pearson, Spearman (minimum 30 pairs) and both directions of Chatterjee's
  xi over eligible numeric pairs under the shared pair budget. It states
  the *strength* of a dependence. Xi detects general dependence, but the
  finding reports a number, not a shape.
- Adjacent, not pairwise shape: `row_consistency` includes rows far from
  the line two tight columns follow (outlying rows against a fitted line,
  not the cloud's shape); the Simpson check states that an association's
  direction reverses or vanishes within strata (pooled vs within-stratum
  direction, not cloud shape); `row_clusters` states multivariate cluster
  views; `content_anomalies` states univariate separated modes.

**Conclusion: the issue's premise holds.** For an arbitrary pair of
numeric columns — neither the designated target, neither serving as an
ordered axis — the library states dependence strength and nothing about
the cloud's shape. No pattern, type or form in
`crates/onceover-report/src/finding.rs` describes a two-column scatter
cloud. Clumped, striated, convex, skinny or stringy clouds are stated
nowhere.

### (b) Candidate measure set, against Wilkinson's scagnostics

Wilkinson's nine measures (outlying, skewed, clumpy, sparse, striated,
convex, skinny, stringy, monotonic) are computed from the minimum
spanning tree of the point cloud after binning and outlier trimming —
the binning is part of the published method, not an approximation added
here. At screening scale the battery would run on the dependence
screen's sampled rows, binned to at most ~1,000 occupied cells with a
weighted MST over cell centres.

Recommended first battery:

- **Clumpy** — the highest-value measure for this library. Clusters in a
  pair are exactly the structure a dependence number hides (two clusters
  can sit at any correlation). MST-based; computable at screening scale.
  Binning loses clusters smaller than a bin width.
- **Striated** — parallel line structure (quantised or lattice data,
  instrument artefacts). MST angle coherence; computable. Binning loses
  striations finer than the bin width.
- **Outlying (joint)** — points outlying in the plane while ordinary on
  both margins. Only this joint form adds anything: marginal outliers
  are already stated univariately. Binned outliers are cell-level, so
  exact counts are lost; the statement carries the bin count.
- **Convex** — cloud convexity (a bent or boomerang cloud scores low).
  Hull-based; cheap once the cloud is binned.

Deferred, with reasons:

- **Monotonic** — computable exactly and cheaply, but it restates what
  the pair's Spearman statement already says. It is never emitted
  standalone (see the redundancy rule); at most it colours a compound
  cloud description.
- **Skewed** (MST edge-length skew) — overlaps the per-column skew
  statements the profiler already makes; the pairwise remainder is thin.
- **Skinny, stringy, sparse** — computable from the same MST/hull
  statistics, but each alone rarely changes what a reader does with the
  data; they return if the first battery's gate results argue for them.

### (c) Screening discipline

Pair growth is quadratic, so the battery does not run on all pairs.

- **Candidate pairs.** The pair universe is the dependence screen's:
  numeric pairs with enough complete rows, non-constant, not
  identifier-like, under the same sampled-pair budget and coverage
  accounting. No second pair universe.
- **Two-armed filter.** A strength-only prescreen would miss the
  diagnostic cases — clumped and striated clouds can sit at
  near-zero correlation (the axis battery's correlation prescreen is
  the cautionary precedent). So a pair pays for MST measures when
  either (i) the dependence screen flags it (Pearson, Spearman or xi
  beyond the screen's bars) — shape then explains a stated dependence —
  or (ii) a cheap grid screen flags it: on a coarse 8×8 occupancy grid
  over the sampled cloud, separated occupied components or empty
  diagonal bands indicate clumps or striations regardless of
  correlation. The grid screen is linear in the sample with tiny
  constants.
- **Multiplicity.** Benjamini–Hochberg across the shape family per run,
  as the axis battery does for its family; the family size (pairs ×
  measures) is recorded in coverage.
- **Redundancy rule.** A shape statement is emitted only if it is not
  implied by statements already made about the pair. Monotonic is
  suppressed standalone (Spearman covers it). A clumpy statement must
  carry what the dependence number does not (the separation itself).
  If `row_clusters` already states the same partition on the same
  columns, the pairwise statement defers, and the deference is recorded
  in coverage. Test of the rule: on a clean linear Gaussian pair the
  battery stays silent even though monotonicity is high.

### (d) Gate material, from the UCI expansion pool

- **Iris** — petal_length × petal_width: three documented clusters
  (setosa separated; versicolor and virginica overlapping) — the clumpy
  positive. sepal_length × sepal_width in the same table: weak
  structure, the within-table near-control.
- **Dry Bean** — area × perimeter and major_axis_length ×
  minor_axis_length: near-functional geometric relations by
  construction — the striated/stringy positives.
- **Wine Quality** — density × alcohol: a documented strong negative
  near-linear relation — the redundancy rule's test case (a dependence
  statement exists; the shape battery must add something or stay
  silent).
- **Electrical Grid Stability** — tau/g pairs from a simulated swing
  system with documented nonlinear structure — curved/clumpy
  candidates.
- **Negative controls.** The gate's permuted component is the
  structural control: permuting a column destroys joint shape while
  keeping both margins, so every shape statement must vanish there.
  The named weak pairs above are the within-table controls.

### (e) Implementation sketch

- New module `crates/onceover-mines/src/scagnostics.rs`: the grid
  screen, the binned weighted MST, and the four measures as pure
  functions, unit-tested on synthetic clouds (two clusters, parallel
  lines, a ring, a bent cloud, and silence on linear Gaussian and
  independent pairs). Size comparable to `axisbreaks.rs`.
- Registration: a new detector in `crates/onceover/src/detectors.rs`,
  on request first (the `target_shapes` precedent); promotion to the
  default set only on gate evidence. A new pattern/type in
  `crates/onceover-report/src/finding.rs` under Associations (type
  `pair_shape`, one form per measure), then taxonomy regeneration and
  the findings-document re-render through the existing generated-block
  machinery, and capability-manifest regeneration.
- Gate: a new reference kind (`shape`) in
  `benchmarks/gate/components/reference.py` mapping to the new type,
  and reference statements on the pairs named in (d) in the iris,
  dry-bean and wine-quality gate files. The permuted component supplies
  the negative control without new machinery.
- No new dependencies: Prim's algorithm on a binned distance matrix of
  at most ~1,000 cells is well inside the runtime discipline.

