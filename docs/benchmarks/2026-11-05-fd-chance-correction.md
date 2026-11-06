# FD chance correction: reliable fraction of information

Date: 2026-11-05 (implementation), measured 2026-11-06
Issue: #128 (chance-corrected FD scoring and selection; standard-tier
size-3 determinant and key step).

## Problem

Two defects in the functional-dependency pass, both measured on the
unmodified build:

1. **Selection by violation rate alone.** For each target, the
   reported approximate FD was the candidate with the lowest g1
   (violation rate). A candidate that adds an unrelated column to
   the determinant splits determinant groups into smaller cells;
   smaller cells are purer by chance, so the enlarged determinant
   often shows the lower violation rate and displaces the true one.
2. **Breadth rewards near-unique determinants.** An exact FD scored
   `0.5 + 0.35 * (distinct determinant values / rows)`. A
   determinant with a distinct value for nearly every row agrees
   with any target almost by construction, yet scored up to 0.85.

Probe (planted, deterministic): 5,000 rows, 48 zips over 8 states,
one zip ambiguous so `p_zip -> p_state` holds with about 1%
violations concentrated in that zip, plus an unrelated 8-level
column. Splitting the ambiguous zip by the unrelated column cuts
the violation count in every seed by chance alone. Measured on
the unmodified build (violation counts recomputed independently
in Polars):

| seed | violations, zip alone | violations, zip + noise | statement reported before |
| ---- | --------------------- | ----------------------- | ------------------------- |
| 0    | 49                    | 42                      | zip + noise (99.16%)      |
| 1    | 46                    | 37                      | zip + noise (99.26%)      |
| 2    | 39                    | 35                      | zip + noise (99.30%)      |
| 3    | 48                    | 42                      | zip + noise (99.16%)      |
| 4    | 41                    | 37                      | zip + noise (99.26%)      |

Each seed ran with the noise column in both positions: the pair
is the reported statement in all 10 runs, and the exact
single-column determinant is reported in none.

On the AdventureWorks corpus table, four of the twelve
highest-ranked statements are exact FDs on the single determinant
`credit_card_id` (distinct ratio 0.584 over the table's rows) —
to account_number, online_order_flag, ship_method_id, and
territory_id. Each scores 0.85, the maximum of the breadth
formula: the determinant contains nulls, and the breadth term
takes its fallback value 1.0 in that case.

## Score

The pass now computes, for every verified candidate, the fraction
of information and its chance expectation under the permutation
model, following Mandros, Boley & Vreeken, "Discovering Reliable
Approximate Functional Dependencies", Proc. KDD 2017
(arXiv:1705.09391), section 3:

- `FI(X -> Y) = I(X;Y) / H(Y)` on the evaluated rows (plug-in
  estimates, natural log; the ratio is base-invariant).
- `E[FI_chance]` is the exact expectation of the plug-in FI under
  a random permutation of Y, computed from the contingency table's
  margins (the paper's eq. 11–12): for each determinant group of
  size s and each target value with count c of n rows, the number
  of that value landing in the group is hypergeometric, and the
  expected cell term follows from the mode recurrence for the
  hypergeometric pmf.
- `RFI = FI - E[FI_chance]`, the reliable fraction of
  information. RFI is computed from the same contingency table as
  verification, at no extra pass over the data.

Selection and scoring use RFI throughout:

- The reported AFD per target is the candidate with the highest
  RFI (ties: smaller determinant, then lower g1), not the lowest
  violation rate.
- An exact FD scores `min(0.5 + 0.35 * breadth, clamp(RFI, 0, 1))`:
  the old breadth score survives only where the chance correction
  supports it. An AFD scores `min(0.72, clamp(RFI, 0, 1))`.
- FD evidence gains `fi`, `expected_fi_chance`, and `rfi`.

The standard budget also takes one step the deep budget already
had, for tables the smaller search leaves unexplained:

- **Determinants:** for a target with no exact FD at size 1 or 2,
  size-3 determinant combinations are screened and verified for
  that target only (pool capped by the existing lhs3_cap).
- **Keys:** when no 1- or 2-column key exists, size-3 key
  combinations are screened with the same tuple-space chance
  bound and sample pre-check the pair search uses, and verified
  exactly. The coverage string discloses when this step ran below
  the size-3 cap.

The two steps have different triggers. A single-column key skips
the key step, but it does not skip the determinant step: a unique
column is never an eligible determinant, so targets that only the
key would trivially explain still count as unexplained and get
the determinant step. Both triggers are pinned by tests
(tests/test_fd_chance.py).

## Results

All numbers below were measured on 2026-11-06 against the
unmodified build at f5a43b5b (the BEFORE column) and the same
tree with this change (AFTER). The BEFORE column was re-measured
on f5a43b5b for this write-up; it agrees exactly with the values
recorded during implementation on the earlier base (the noise
probe counts, the corpus census of 203 records, and the quality
battery are identical), so every difference below is attributable
to this change alone.

### Planted probes

- Noise probe (above): the pair is reported in 0 of 10 runs; the
  exact single-column determinant is reported in 10 of 10
  (FI ≈ 0.993, E[FI_chance] ≈ 0.016, RFI ≈ 0.977). The chance
  statement is gone; the true one survives.
- AdventureWorks probe: of the twelve highest-ranked statements,
  those with determinant ratio above 0.5 number 4 BEFORE and 0
  AFTER (acceptance bar: at most 2).
- Keys probe (planted 10×10×10 grid table): the 3-column key
  claim ("jointly identify rows") appears at standard budget only
  AFTER; the coverage disclosure for the size-3 key step appears
  exactly on the tables with no 1- or 2-column key.

### Corpus census

Every functional-dependency record in the report (selected
findings plus the grouped pool) over the 23 runnable corpus
datasets, BEFORE → AFTER: 203 → 167 records; exact FDs among
them 139 → 107; AFDs 64 → 60. The movements fall in four
classes:

1. **Chance-level exact FDs re-scored.** Exact FDs on near-unique
   determinants keep their verification but lose their score:
   e.g. the AdventureWorks `credit_card_id` family drops from
   0.85 to its RFI. Detection is unchanged — the pass still
   verifies and emits every exact FD (per-dataset pass counts
   verified identical on thyroid: 7 exact FDs, 12 AFDs on both
   builds).
2. **AFD reselection.** The per-target AFD winner changes where
   RFI disagrees with the violation rate. Absenteeism-at-work is
   the churn case: 27 records BEFORE and AFTER, with different
   winners (e.g. for social_drinker, (employee_id,
   reason_for_absence) is replaced by (employee_id,
   month_of_absence)). Soybean's leafspot_size winner changes
   from (class, leafspots_halo) to (class, leafspots_marg)
   between two candidates whose violation counts are identical
   (recomputed independently: both leave the same number of rows
   in impure groups). For absenteeism's
   distance_from_residence_to_work_km, the winner changes from
   employee_id (1 violation in 740 rows) to (body_mass_index,
   weight) (3 violations): both determinants are per-employee
   attributes, and RFI ranks the pair's information higher.
3. **Report-cap displacement.** On datasets whose report hits
   the 100-finding cap, a re-scored statement competes for slots
   at its corrected score and can fall out of the report even
   though the pass emits it. Taiwanese-bankruptcy goes 26 → 4
   records this way (its ratio columns are near-unique
   determinants); bike-sharing-hourly keeps one direction of the
   cnt = casual + registered identity ((casual, registered) ->
   cnt) and loses the other two at report level; thyroid keeps
   13 of 19.
4. **Size-3 step additions.** Statements the size-≤2 search
   could not reach appear on tables it left unexplained (new
   exact FDs and AFDs in the AFTER census on absenteeism-at-work,
   lace-satellites, and others, plus the planted key claims
   above).

Control case: chinook is unchanged (17 records, 16 exact, BEFORE
and AFTER) — small determinants with genuinely high RFI are
untouched.

### Corpus runner (registry expectations)

Control run on the unmodified build: 89/89 machine-verified
expectations pass (documented cases: 18 found, 10 missed,
4 partial; runner exit 2, the standing state). With this change:
84/89 machine-verified (exit 1); documented cases unchanged.
The five failures, by the classes above:

- thyroid, functional finding with target tsh_measured — class
  3: the assay-value → measured-flag singles score at their RFI
  (≈0.08) and fall out of the capped report.
- adventureworks, functional finding with target account_number
  — class 3: all four exact determinants of account_number are
  near-unique identifiers.
- bike-sharing-hourly, (cnt, casual) -> registered — class 3:
  one identity direction survives; this direction does not.
- soybean, class + leafspots_halo -> leafspot_size — class 2
  reselection between violation-tied candidates.
- absenteeism-at-work, employee_id ->
  distance_from_residence_to_work_km — class 2 reselection of
  the target's AFD winner.

The registry is unchanged in this change: the five expectations
stand as written, and this document is their record. Whether the
corrected scores should cost these statements their report slots
is the adoption decision the numbers above support either way:
the chance statements the issue targets are gone from every
planted probe, and the cost is report presence for low-RFI
statements — including some exact ones — on capped reports.

### Quality battery

benchmarks/quality/battery.py --full (9 planted families,
12 null frames, seeds 0–11): identical BEFORE and AFTER — all
72 plant metric cells equal, the null summary equal, and null
false positives unchanged (dependencies family: 0.0 mean per
frame on both builds). The change does not move any association,
dependence, interaction, surprise, or breaks measurement.

## Provenance

- BEFORE build: unmodified f5a43b5b. AFTER build: f5a43b5b plus
  this change (crates/onceover-deps/src/fd.rs,
  crates/onceover/src/account.rs, docs/findings.md,
  tests/test_fd_chance.py).
- Environment: Python 3.12, polars 2.1.0, numpy 2.5.4,
  pytest 9.2.0; release build (maturin develop --release).
- Corpus: benchmarks/corpus/registry.json, 34 entries, 23
  runnable (11 skipped for licensing or source reasons, same on
  both builds).
- The RFI expectation is exact (no sampling): the harness
  cross-check of the hypergeometric mode recurrence against the
  closed form passed during implementation.

## Amendment, 6 November 2026: verification cost and screen-sample near-uniqueness

**Cost of the chance term.** `fd_stats` summed the chance expectation over every
(determinant group, target value) cell: O(groups × target values), which grows with the
square of the rows when both sides are near-unique. The term depends on a cell only
through its margins, so it is now summed over the distinct group sizes and distinct
target counts, weighted by how many groups and values carry each (at most about
sqrt(2n) of each). The result is unchanged; a unit test compares it with the cell-by-cell
sum. On a (store, product) → price table with 20,000 rows and 10,816 cells, the FD pass
took 228 ms before and 11 ms after.

**Determinants dismissed on the screen sample.** The level search dropped any
determinant whose groups numbered at least 95% of the 2,048 screen rows, treating it as
an approximate key. A determinant can look near-unique on 2,048 rows and still be far
from unique on the full table: (store, product) with 200 levels each has at most 40,000
combinations, so over 50,000 rows it is a real determinant, and the exact FD
(store, product) → price was not reported at all. Such a determinant is now dismissed
only when the product of its columns' distinct counts reaches 95% of the verification
rows. Otherwise it goes to exact verification on the full rows, and its exactness on the
screen sample, which is trivial there, does not prune other candidates.

| Table | Before | After |
|---|---|---|
| (store, product) → price, 50,000 rows, 28,564 cells | not reported | exact, 0 violations |
| (store, product) → price, 200,000 rows, 114,226 cells | not reported | exact, 0 violations; FD pass 105 ms |

The second row would have cost about 1.3 × 10^10 cell terms under the old chance sum.

Corpus: the six new statements are on Metro traffic, on `date_time` (40,575 distinct
values in 48,204 rows). The hours recorded twice agree exactly on traffic_volume,
holiday and snow_1h, and nearly on temp (89 violations), rain_1h (13) and clouds_all
(34). The Metro findings-count guard moves from 64 to 70. AdventureWorks reselects
between two determinants tied at 6 violations: (subtotal, online_order_flag) replaces
(subtotal, ship_method_id) for freight and total_due. No other functional statement in
the corpus changes (167 before, 173 after).

**Coverage status.** The pass's coverage entry read `capped` on every run. It now reads
`capped` only when a cap removed something from the search (the per-target determinant
shortlist, the size-3 pool, or the composite-key verification budget), and the detail
names each cap that bound. Otherwise it reads `ran_sampled` when verification used a
row sample, and `ran_exact` when it used every row.
