# Weighted concentration for the hotspots pass (issue #329)

This record holds the design analysis for measure-weighted concentration
in the `hotspots` pass and, in its second half, the measurements from
the implementation. The problem: the pass finds cells (conjunctions of
categorical levels) holding an unexpected share of rows. The
concentrations that matter most are often in a quantity, not in row
counts, and a cell whose rows carry an outsized share of a measure sits
at its row-count expectation, so the pass cannot see it.

The two cases on record in issue #329, both verified against the data:

- Bike sharing (hourly, 17,379 rows). Working-day rush hours
  (workingday = 1, hr in {7, 8, 9, 17, 18, 19}) hold 2,983 rows
  (17.2%) and 1,181,285 of 3,292,679 rentals (35.9%). The fullest
  single cell, (workingday = 1, hr = 17), holds 499 rows against 496
  expected under independence.
- planted_mix (6,000 synthetic rows). The cell channel = kiosk,
  applicant_age 18-24 holds 207 rows (3.5%) and 24.3% of total
  payout.

## What the code does today

`crates/onceover-mines/src/hotspots.rs` scans pairs of encoded features
(and, at the deep budget, triples chosen by a pairwise pre-score). A
feature is eligible when its encoded bin count is between 2 and
`HotspotConfig::max_levels` (20 at every budget). For each combination
the pass walks the encoded rows once, counts rows per cell and per
feature margin over the combination's complete rows, and computes the
independence expectation from the margins. A cell is a candidate when
its count clears absolute bars (at least 20 observed rows, expected at
least 10, ratio at least 2.0, share of scanned rows at least 0.5%); a
Bonferroni z bar over all examined cells (family alpha 0.01) is applied
after the scan. Findings are `Pattern::Hotspot`, category
Concentration, claim giving observed rows, expected rows, and the
ratio against the independence baseline.

The miss has two layers, and only one of them is the row-count
baseline:

1. Measure blindness. The pass accumulates counts only. A cell at its
   independence expectation for rows is invisible no matter how much
   of a measure its rows carry.
2. The bike cell is not scanned at all. The encoder quantile-bins
   numeric columns (`encode.rs::bin_numeric`, at most
   `MAX_BINS = 32` bins). The `hr` column has 24 distinct values and
   encodes to 25 bins — one per hour plus an empty top bin — which is
   above the eligibility cap of 20. Replicating the encoder's binning
   on the corpus file reproduces the pass's own coverage numbers
   exactly (28 combinations, 1,144 cells), and `hr` is not among the
   eligible features. The same applies to `applicant_age` in
   planted_mix (32 bins). Any design that judges "each scanned cell"
   under a weight therefore cannot state either case: the cells are
   outside the scanned set before weights enter the question.

## Design decisions

### 1. Declaration: a new hint key, `weights`

The declaration lives in a new hint key `weights`, a list of column
names, alongside `measures` in `ProfileHints`. The alternative — a
`weight` value in the `roles` hint — does not fit how roles work:
`roles` maps each column to exactly one role string, and a column can
be both a measure and a weight (a revenue column is the table's
measure and the natural weight for concentration). The list form is
the `measures` precedent: an independent list of column names the run
treats as a set. Several weights can be declared; each is judged
independently.

`weights` joins the instruction class of hint keys (the class the
`profile()` docstring describes as instructions to the run, with
`roles` and `exclude_columns`). It is added to `HINT_KEYS` and to
`ProfileHints::is_empty()` — a key missing from `is_empty()` is
reported as "no hints provided" when it is the only hint given (the
issue #319 trap), and the hints tests pin both.

Validation follows the pattern the expectations pass set for unusable
hint entries (issue #320): a declaration that cannot be applied is
reported in the hotspots pass coverage, never silently dropped, and
never raised. Per declared name, in declaration order:

- not a column of the table: not applied, noted;
- not a numeric column: not applied, noted;
- negative values present: not applied, noted with the count of
  negative rows (shares need a non-negative measure — the same rule
  the panel concentration scan applies);
- total over present values is zero: not applied, noted;
- constant over present values: not applied, noted — its shares
  restate row shares exactly, so weighting by it can only duplicate
  the row-count scan;
- declared twice: applied once, noted.

Rows with a missing weight value are excluded from that weight's
totals; each weight is judged over its own complete rows (rows
complete on the combination's columns and present for the weight).

### 2. Mechanism: generalize the accumulation, do not add a pass

Counting is weighting where every row weighs 1. The implementation
generalizes the per-combination row walk: one walk over the encoded
rows accumulates, together, the cell counts, the count margins, and
for each applied weight the cell sums, the weight margins per feature,
the weight total, and the sum of squared weights over the weight's
complete rows. There is no rescan per weight; the cost of the weighted
judgment is the same row walk with wider accumulators. With no weights
declared the walk, the enumeration order, the bars, and the findings
are exactly today's — the unhinted run is unchanged by construction,
not by a separate code path that could drift.

One exclusion is forced by the mechanism: a weight is never judged
over combinations that contain the weight column itself as a feature.
Bins of the weight mechanically concentrate the weight (the top bin of
`cnt` holds more `cnt` per row by construction), so those cells would
state a tautology. Other weights remain eligible as features.

### 3. Cell set: the weighted judgment uses a wider eligibility

The row-count scan keeps its eligibility (2 to 20 bins) and its cell
set, unchanged. The weighted judgment runs over pairs of features with
2 to 32 bins — the encoder's own `MAX_BINS`, the widest binning the
encoder produces — because the cells that motivated the feature sit
above 20 bins (`hr` at 25, `applicant_age` at 32), and a declared
weight is an explicit instruction to look at this quantity, which is
what justifies the wider scan. Triples ride the row-count scan's
selection (triples are chosen by their strongest constituent pair's
count z, over features with at most 8 bins); a triple strong only
under a weight is not selected, which is a named limitation, not a
hidden one. The wider set is paid for where it should be: the
weighted Bonferroni denominator counts every weighted cell examined.

This is the one point where the implementation departs from the design
sketch recorded on issue #329, which spoke of judging each scanned
cell. The evidence above (the encoder replication reproducing the
pass's 28 combinations and 1,144 cells, with `hr` ineligible) shows
the scanned set does not contain the cells the feature exists to
state. The settled requirements — both shares stated, the correction
over cells x weights, the two statements matching in a hinted gate
run — are all kept; only the cell set they quantify over is wider for
the weighted judgment than for counts.

Single-column weighted concentration (one level of one column holding
a large share of a weight) is not stated. The row-count scan leaves
marginals to the `value_concentration` detector for the same reason;
the weighted marginal is that detector's weighted analog and a
separate statement family. It is the known remaining gap, recorded
here rather than discovered later.

### 4. Baseline: weight share against row share

Two baselines were computed for both tables before choosing.

Q1, weight-margin independence: the cell's expected weight share is
the product of the features' weight shares (the direct analog of the
count scan's independence expectation). Q2, row share: the cell's
expected weight share is its share of rows — the statement "these
rows hold an outsized share of the total".

Measured on the corpus data with the encoder's bins:

Bike sharing, (workingday x hr), weight `cnt` (n_eff = 9,068 of
17,379 rows):

| cell | rows | row share | weight share | Q2 ratio | Q1 ratio |
|---|---|---|---|---|---|
| workingday=1, hr=17 | 499 | 2.87% | 8.02% | 2.77 | 1.12 |
| workingday=1, hr=18 | 498 | 2.87% | 7.42% | 2.60 | 1.14 |
| workingday=1, hr=8 | 496 | 2.85% | 7.19% | 2.52 | 1.30 |
| workingday=0, hr=17 | 231 | 1.33% | 2.27% | 1.71 | 0.73 |

planted_mix, (channel x applicant_age bin), weight `payout`
(n_eff = 2,495 of 6,000 rows): the planted 18-24 band spans four age
bins; each bin cell has Q2 between 6.88 and 7.12 and Q1 between 1.69
and 1.91.

Under Q1 neither plant clears the ratio bar of 2.0 that the count scan
uses: the bike rush hour is mostly a margins phenomenon (hour 17 rows
carry more rentals on every day of the week), and the planted payout
cell reaches only 1.91. Under Q2 both plants clear 2.0, and the
weekend hr=17 cell — the same hour without the working-day rush —
does not (1.71), so the bar separates the plant from its own margins'
sibling cell on this data.

Q2 is the baseline, because the statement being made is a
concentration statement — a share of a total held by a named set of
rows — not an interaction statement. The count scan's claim is about
departure from independence; the weighted claim is about where the
quantity sits. To keep the decomposition visible, the evidence also
carries the Q1 quantities (`expected_weight_share` under weight-margin
independence and its ratio, `margin_ratio`), so a reader can see how
much of a stated concentration exceeds what the features' own weight
margins predict.

### 5. Significance: a z on shares with an effective row count

Under a weight, the row count is not the sample size for a share: a
total dominated by a few heavy rows carries less information than the
same total spread evenly. The z uses the Kish effective sample size
of the weight over the combination's complete rows,
n_eff = (sum w)^2 / sum w^2, in the standard error of the cell's
weight share against its row share:

    z = (weight_share - row_share) / sqrt(row_share (1 - row_share) / n_eff)

For `cnt` on bike sharing n_eff is 9,068 of 17,379 rows; for `payout`
on planted_mix, 2,495 of 6,000. A noise weight with heavy spread is
discounted by the same formula, which is what the negative controls
exercise. The candidate bars reuse the count scan's config values,
which are scale-free under a weight (bars on raw weight sums would
depend on the weight's unit): at least 20 rows in the cell
(`min_observed`), weight share at least 0.5% (`min_share`), share
ratio at least 2.0 (`min_ratio`), and z at or above the weighted
Bonferroni bar — family alpha 0.01 over the total tests, cells
examined per weight summed over weights (the cells x weights
correction). The count scan's bar and family are untouched, so
hinted runs keep the same count findings as unhinted runs.

Caps mirror the count scan's, in a separate pool so weighted findings
never displace count findings inside the pass: at most
`max_per_combination` (3) per combination and weight, and at most
`max_findings` (15) weighted findings overall, strongest z first.

### 6. Statement form

The finding is a `hotspot` finding (category Concentration) whose
subject columns are the cell's columns plus the weight's name, so the
weight is part of the finding's identity: the same cell under two
weights is two findings, and a weighted finding never merges with the
row-count finding on the same cell. The claim template, with shares
at three significant figures:

    cell (workingday = '1', hr = '(16, 17]') concentrates 8.02% of
    cnt on 2.87% of scanned rows (499 of 17,379) — 2.77x its row
    share (z = 29.0, bar 5.02 over N cells x 1 weights examined)

Evidence keys: `columns`, `cell`, `weight`, `observed_rows`,
`rows_scanned`, `cell_weight`, `weight_total`, `row_share`,
`weight_share`, `share_ratio`, `expected_weight_share`,
`margin_ratio`, `effective_rows`, `z`, `z_bar`, `cells_examined`,
`weights_examined`, `method`. The method name is `weighted_share_scan`
(the count scan's is `independence_excess_scan`).

### 7. Gate design

The gate gains a `weighted` component. A gate registry entry declares
`weights`; the component profiles the dataset's own frame with those
weights added to the run's hints (a separate cached profile — the
unhinted profiles the other components use are untouched) and scores:

- statements carrying a `hinted_tier` in the dataset's reference file,
  at the 4k digest, 8k digest, and full report scopes of the hinted
  run, counted under must/should exactly as the reference component
  counts its tiers. The two issue #329 statements
  (bike-sharing-hourly `cnt-rush-hour-concentration`, planted_mix
  `hotspot-payout`) become must-tier in the hinted run; in the
  reference component they stay should-tier and stay recorded as
  misses in every unhinted run.
- negative controls, declared in the registry entry as
  `weighted_controls`: a control is satisfied when no weighted
  finding (a hotspot finding whose evidence names a weight) fits the
  control's columns and kind in the hinted full report. A satisfied
  control counts as a must match at every scope; a firing control is
  a must miss.

Declared weights per dataset:

- bike-sharing-hourly: `cnt` only. `casual` and `registered` are
  `cnt`'s additive components; declaring them would spend
  multiplicity on the same quantity three times.
- planted_mix: `payout` and `total_cost`. By construction
  `total_cost = 2*parts_cost + 3*labor_cost + 7` with parts and labor
  independent of channel and age, so `total_cost` is a true negative
  on the planted cell: the several-weights machinery runs with one
  weight firing and one silent.
- two new synthetic control datasets, generated by seeded prepare
  scripts like the rest of the synthetic pack:
  - `weighted_noise`: segment x region cells over independent
    categoricals, with `noise_weight` a lognormal column independent
    of every other column. The heavy spread makes the effective-n
    discount bind, so a z computed on raw row counts would fire
    where the discounted z must not.
  - `weighted_large_cell`: one cell (channel = web, plan = basic)
    holds about 40% of rows, and `revenue` is drawn from the same
    distribution in every cell, so the large cell's revenue share
    matches its row share. Merely large is not concentration; the
    control fails if the cell is stated under the weight.

Both controls also enter the corpus registry with the weight declared
in the entry's hints and a `missed` expectation on the weighted
hotspot, so the corpus harness tripwires if a weighted finding
appears there.

## Implementation

As designed above, with these particulars:

- `ProfileHints` gains `weights: Vec<String>` after `measures`;
  `HINT_KEYS` and `is_empty()` gain the key in the same change.
- The pipeline resolves the declared weights once per profile
  against the extracted table (`crates/onceover/src/pipeline.rs`):
  each applied weight becomes a `WeightSeries` (values indexed
  by original row); each refused declaration becomes a note
  appended to the hotspots coverage entry at both dispatch
  sites. The series are gathered over the full table; the pass
  reads them through the encoded matrix's row map, so a sampled
  encoding judges the sample's rows with their true weights.
- `hotspots` takes the resolved series. `evaluate_combo`'s row
  walk accumulates counts and, per weight, cell sums, cell row
  counts, margins, total, and squared total in the same walk.
  The count judgment is byte-identical when no weights are
  declared: same enumeration order, same bars, same coverage
  text (the coverage gains a weights clause only when weights
  were applied). One display guard came with the wider scan:
  when the count side examines no cells (weights declared on a
  table with fewer than two count-eligible features), the
  coverage states that there is no count z bar instead of
  printing the infinite bar.
- Weighted findings are rendered by a separate renderer with
  the claim form settled in the analysis, capped in their own
  pool (per combination-and-weight 3, overall 15), so they
  never displace count findings inside the pass.
- The detector registry's `hotspots` purpose now reads
  "Combinations concentrated beyond their marginal frequencies,
  in rows or in a declared weight"; `docs/findings.md` was
  regenerated for the registry-owned blocks and its
  hand-written concentration and declared-expectations prose
  updated; the capability manifest was regenerated. The
  packaged agent guide names the key and its semantics inside
  its 4,000-character cap (3,970 characters after the change);
  the book's hints material (usage/api, usage/talking to the
  profiler) and the hotspots methodology page carry the same
  account, the methodology page with a worked example.

## Measurements

(Filled from the runs on the final tree; see the pull request
for the scorecard comparison.)

- Hinted gate run (weighted component, head tree): the two
  #329 statements are must-tier in the hinted run. The
  planted_mix statement (payout over channel × applicant_age)
  matches at all three scopes (4k digest, 8k digest, full
  report). The bike statement (cnt over workingday × hr)
  matches at 8k and in the full report, and misses at 4k: the
  hinted profile's 4k briefing carries one hotspot line — the
  strongest weighted cell (holiday × casual top bin, z = 30.9)
  — and the rush-hour cell (z = 29.0) is the fifth weighted
  line, present from 8k up. The miss is briefing crowding
  inside the hinted profile, not detection: the finding is in
  the report the scopes are drawn from. Several of the cells
  outranking it are driven by cnt's own additive components
  (casual, registered), which concentrate cnt mechanically.
  Ranking among true statements is the selection problem
  (#343/#347 territory); this change does not tune it.
  Weighted findings in the hinted profiles: 8 (bike), 7
  (planted_mix, all under payout; total_cost states none).
  Weighted totals: must 3/4 at 4k, 4/4 at 8k, 4/4 at full.
- Negative controls: both satisfied at every scope.
  weighted_noise states no weighted finding (0 in the hinted
  profile; the control weight stays silent despite its heavy
  spread), and weighted_large_cell states none (the cell
  holding 40.04% of rows and 40.25% of revenue is large in
  both alike and is not stated). The corpus runs of both
  datasets carry the same silence as `missed` tripwire
  expectations, and both tripwires hold.
- Unhinted gate comparison (base 610812ef vs head, same
  scorecard machinery): the reference component is unchanged
  — must 16/16/17 of 19 and should 29/35/37 of 45 at
  4k/8k/full in default mode, check_failed 0 on both sides.
  compare.py's verdict is FAIL on totals alone: every
  per-dataset row in the comparison has delta 0, and
  recomputing the totals over the 25 shared datasets makes
  every metric identical (permuted false_full 125 = 125,
  invariance total 500 = 500, planted found_full 59 = 59,
  redundancy finding lines 361 = 361). The totals move only
  because the gated set grew: the two new datasets contribute
  their own rows (their planted expectations, found 4 of 5
  and 4 of 6; one permuted false positive on weighted_noise;
  one noise-sensitivity count each). No unhinted number on a
  shared dataset moved.
- Corpus: exit 2 (machine checks clean, documented misses on
  record), 220/220 machine-verified expectations over 43
  datasets — the 218 of the base plus the two new control
  tripwires, both holding.
- Verification legs on the final tree: `scripts/release.py
  check` passes; `scripts/test_release.py` 6 passed;
  `cargo fmt --all -- --check` clean; clippy on the Rust
  1.100 toolchain (`--workspace --all-targets --locked --
  -D warnings`) clean; `cargo test --workspace --locked` in
  the debug profile died at the link of an example binary
  (ld terminated with signal 7 — the disk-quota failure), so
  the suite was run in the release profile instead: 476
  passed, 0 failed; `python -m pytest tests/ -q`: 775
  passed; the strict book build (`docs/book/build-html.sh`,
  executed, strict) completes.
