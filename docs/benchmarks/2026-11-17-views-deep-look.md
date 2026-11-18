# Views deep look: grouping recall (#360) and the search-sample defaults (#359)

This record measures the views pass (`row_clusters`) on the current
main, `34718c60`. It covers the one question the views integrity work
(PR #369, `docs/benchmarks/2026-11-17-views-integrity.md`) explicitly
left open — general grouping recall, issue #360 — and verifies what
the new search-sample defaults rest on, issue #359.

The pass partitions columns into views (columns in one view share one
row clustering) by a deterministic greedy search: singleton
clusterings, a pairwise pre-screen, then merges in pre-screen order,
each merge accepted when a marginal-likelihood score delta is
positive. The findings it emits carry `basis="estimated"` with exact
`verify()` recounts.

## What #369 already fixed (not re-measured here)

The fixed 2,000-row search sample is gone: `sample_rows=` now
controls both search and scoring (20,000 rows at standard, 50,000 at
deep), the column screen spans the search range instead of its first
256 rows, and coverage reports actual search rows, completed pair
screens and merge attempts, with `capped` states for column, merge
and work truncation. The exact-claims defects (false modal values
from float projection, pooled categories reported as literals, wrong
denominators) are fixed. None of that is re-litigated here.

## Method

Planted-partition tables, generated in the lab module
(`crates/onceover-mines/src/views/views_lab.rs`, test-only code that
ships with this record's pull request; every run below is
reproducible from it):

- `bin_aligned_bool` / `bin_aligned_i64`: three blocks of four binary
  features in the Animals shape; each block is one latent class plus
  per-feature flips, so dependence is obvious pair by pair. The same
  table encoded as booleans (categorical path) and as 0/1 integers
  (continuous path). 15 planted pairs.
- `bin_weak_joint`: two blocks of five binary features whose
  dependence is spread across each block (pairwise rates 0.15/0.85),
  plus one stronger block. 30 planted pairs.
- `xor_bool`: two blocks of three features related by parity; no
  single pair carries the dependence. 6 planted pairs.
- `cont_factor`: three blocks of three continuous columns from a
  shared latent factor at loadings 0.9, 0.7, 0.5. 18 planted pairs.
- `cont_mixture`: two blocks of four continuous columns drawn from a
  shared two-component mixture. 12 planted pairs.

Each table also carries four noise columns, so the battery holds 96
planted pairs and 466 cross pairs that must stay split. Runs use the
full table as both the scoring sample and the search sample, so the
search sample cannot confound the recall measurement. The lab drives
the production search with a measurement trace that records, for
every screened pair, its rank in the merge order and what the merge
loop did with it; the trace is inert on the production path.

The Lace reference was recomputed with the Rust `lace` crate (0.9.1)
directly — the same CrossCat engine the Python bindings wrap — in a
standalone harness crate under `benchmarks/lace-depprob/`. The crate
is a benchmark dependency only; it is detached from the workspace
and is not a dependency of the shipped library. Protocol as in the
2026-10-30 head-to-head record: 16 posterior states, 5,000 update
iterations, seed 1337, depprob ≥ 0.5 counts as dependent.

## #359: what the new search defaults rest on

Verified from the merged code and PR #369's record. The 20,000 /
50,000 defaults are the pre-existing public sample values, extended
to govern search as well as scoring. The integrity record states
this itself: the change "makes the existing public sample contract
consistent; it is not a measurement that these limits are
statistically optimal". No recovery or runtime sweep behind the
chosen values exists in the record or the code.

The sweep that would settle the defaults is a search-sample sweep on
planted tables at scale under the current pass: planted families at
20,000 and 100,000 rows, search sample 2,000 / 8,000 / 32,000 / full,
recording recovered planted pairs, wrong joins, runtime, and whether
the 300-million-cell work budget stops the search. The 20,000-row
cells of that sweep, run on the current pass (debug build; two
planted tables of 28 columns — a binary family with 41 planted pairs
and a continuous family with 32):

| Table | Search rows | Recovered | Wrong joins | Runtime | Search stopped by |
|---|---|---|---|---|---|
| big_binary_20k | 2,000 | 0/41 | 0 | 5.8 s | merge-attempt cap |
| big_binary_20k | 8,000 | 0/41 | 0 | 22.4 s | merge-attempt cap |
| big_binary_20k | 20,000 (full) | 0/41 | 0 | 53.0 s | merge-attempt cap |
| big_cont_20k | 2,000 | 0/32 | 0 | 29.0 s | merge-attempt cap |
| big_cont_20k | 8,000 | 0/32 | 0 | 125.9 s | merge-attempt cap |
| big_cont_20k | 20,000 (full) | 0/32 | 0 | 338.9 s | **work budget** |

Recovery is 0 at every sample size, for the criterion reason
measured in the next section: a larger search sample cannot fix a
merge rule that refuses the planted merges at full rows. What the
sample size governs today is cost — runtime grows roughly linearly
in the search sample, and at the full-row setting on the continuous
family the unchanged 300-million-cell work budget stops the search
outright, the interaction PR #369's record already flags ("larger
samples can reach the existing work budget sooner"). Setting the
shipped defaults from a curve therefore waits on the criterion
question in #360: until merges can be accepted, the recovery axis of
the sweep is flat at zero and only the cost axis is measurable. The
100,000-row cells remain unrun on the current pass; on the pre-#369
code they showed the same flat recovery and an unfinished pair
screen at full rows.

## Attribution: every planted split at 500 rows is a score refusal

Planted battery, 500 rows, full rows, default configuration
(gamma 2.0, merge-attempt cap 256):

| Table | Planted pairs | Recovered | Split by pre-screen drop | Split: merge never attempted | Split: refused by the score |
|---|---|---|---|---|---|
| bin_aligned_bool | 15 | 0 | 0 | 0 | 15 |
| bin_aligned_i64 | 15 | 0 | 0 | 0 | 15 |
| bin_weak_joint | 30 | 0 | 0 | 0 | 30 |
| xor_bool | 6 | 0 | 0 | 0 | 6 |
| cont_factor | 18 | 0 | 0 | 0 | 18 |
| cont_mixture | 12 | 0 | 0 | 0 | 12 |
| **Total** | **96** | **0** | **0** | **0** | **96** |

No wrong joins at this size. Raising the merge-attempt cap from 256
to unlimited changes nothing (the loop ends by exhaustion, not by
the cap). At 50 rows the same configuration recovers 26 of 96 pairs
(bool 5, i64 1, weak_joint 3, xor 0, factor 10, mixture 7) at the
cost of 60 wrong joins — recovery appears exactly where the sample
is small enough that the score's per-row charges shrink, and it
arrives with false structure attached.

## The criterion refuses the true partition itself

A merge is accepted when

    delta = joint score − singleton scores + gamma · ln n  >  0,

where each score is a sum of per-column marginal likelihoods under
that column's own best clustering, minus gamma · ln n per cluster.
Three measured charges make delta negative for true pairs:

1. **Categorical refinement charge.** For an aligned binary pair
   (rates 0.85/0.15 within each latent class), each singleton scores
   −35.91 with the exact two-class partition. The joint clustering
   is the exact four-cell cross of the two singleton partitions and
   scores −87.49: the Dirichlet-multinomial marginal charges for
   refining each column's levels inside four cells instead of two,
   ≈ 15.7 nats, against a merge bonus of gamma · ln 500 = 12.43.
   Delta = −3.25.
2. **Binary data on the continuous path.** The same pair encoded as
   0/1 integers is standardized and scored as Gaussian. The
   singletons score +433.30 and +435.47 — a two-point distribution
   sliced at its gap fits the normal-inverse-gamma marginal far
   better than it should — and the joint scores +598.16.
   Delta = −258.18.
3. **Continuous shredding.** A factor pair at loading 0.9: each
   singleton's best clustering slices its unimodal column into six
   clusters (scores −89.21 and −123.18); the joint's best clustering
   has seven. Delta = −252.00. The slicing is the score working as
   written: under the NIG prior (0, 1, 2, 1), a homogeneous
   standardized Gaussian column gains ≈ +224 nats from one median
   split at n = 500 (≈ 0.45 nats per row), verified by direct
   numerical evaluation of `nig_log_marginal`. A unimodal column is
   worth more in slices than whole, so a shared clustering — which
   must serve two columns with one partition — starts hundreds of
   nats behind the two private slicings it has to beat.

The overlay test removes the search from the question: score each
pair's merge under the cross of its two singleton partitions (the
partition a merge would use if both columns kept their own
clustering), with no cluster cap involved. At 500 rows, gamma 2.0,
**all 96 planted pairs and all 466 cross pairs have delta ≤ 0**
(largest planted delta −3.02; factor −493.69, mixture −690.73). The
criterion does not merely fail to find the true partition; scored
directly, it ranks the true partition below the split one for every
planted pair in the battery.

Two structural limits sit behind the charges. First, the merge
order is seeded by pairwise gains only, so parity structure (the
xor family) is unreachable by construction: no pair ever looks
mergeable. Second, gamma cannot fix the balance: sweeping gamma
over {0, 0.5, 1, 2, 4} at 500 rows recovers 0 pairs at every setting
up to 2, and 11 of 96 at gamma 4 (bool 5, weak_joint 6) with 8 wrong
joins — while at 50 rows gamma 4 recovers 94 of 96 with 402 wrong
joins. The penalty is one knob on a sum whose other terms are
mis-scaled per row; `max_clusters` ∈ {2, 3, 4, 8} changes nothing
at any gamma.

## Animals on the current pass

The recomputed depprob matrix reproduces the recorded aggregates:
605 of 3,570 pairs at depprob ≥ 0.5 (recorded: 605), 145 at ≥ 0.9,
and every named pair from the record at 1.000. (The record's "351
both-viewed" figure belongs to the superseded pre-#73 column
selection; under the current selection the both-viewed count is
408.)

Default configuration (gamma 2.0, cap 256, 50 search rows = the full
table): the screen keeps 64 of 86 columns (one is the row-id column
the corpus table carries); the search ends at the 256-attempt cap
with 37 views. Of the 605 dependent pairs:

| Fate | Pairs |
|---|---|
| Recovered (same view) | 29 |
| A column dropped by the screen | 197 |
| Both kept, merge never attempted (attempt cap) | 320 |
| Both kept, attempted, refused by the score | 59 |

Wrong joins: 11 co-viewed pairs sit below depprob 0.5. At depprob
≥ 0.9, 25 of 144 both-viewed pairs are recovered. The refused
deltas run from −63.3 to −0.47 (median −13.0). The screen drops
include `muscle` (strongest depprob 0.94), `hibernate`, `plains` and
`fields` (0.88); an oracle top-64 screen ranked by true depprob
would keep 478 dependent pairs both-viewed against the actual 408,
so roughly a third of the screen's loss is its 256-row sample noise
and the rest is the cap itself. Lifting the attempt cap to
unlimited recovers 38 of 408 (the loop then attempts all 1,998
cross-view pairs); gamma 4 recovers 132 of 408 with 27 wrong joins.

The named pairs, one by one (depprob 1.000 unless noted):

- `paws`~`claws`: **merged** (rank 225, delta +3.21). The design
  document's statement that the pre-screen drops `paws` is stale:
  the screen keeps `paws` (score 0.4688, best partner `toughskin`)
  and the pair merges. The sentence is corrected in this change.
- `pads`~`paws`, `pads`~`claws`: never attempted — ranks 297 and
  304, past the 256-attempt cap.
- `big`~`small`: merged (rank 44, delta +8.94).
- `flippers`~`swims`: recovered, already in one view when visited
  (rank 21).
- `flippers`~`plankton`: refused by the score (rank 28,
  delta −8.36).
- `flippers`~`strainteeth`: never attempted (rank 552).
- `meatteeth`~`hunter`: refused by the score (rank 250,
  delta −11.49).

## Satellites on the current pass

20 columns, all 190 pairs dependent in the recomputed matrix (min
0.500, median 0.9375, 73 at ≥ 0.99 — the recorded aggregates). The
greedy pass returns 18 views with two multi-column views
({Class_of_Orbit, Perigee_km, Period_minutes} and
{Inclination_radians, longitude_radians_of_geo}); the attempt loop
exhausts (209 attempts, no cap hit). The lace states place the 20
columns in 1–3 views. Runtime, debug build: 6.1 s for the greedy
search and assignment; the lace fit took 211.7 s.

## Lace as the engine: recovery, runtime, determinism

The same harness fit the planted tables (500 rows) and both corpus
tables at the record protocol. Pairwise recovery against planted
truth, counting a pair as joined when its depprob reaches the
threshold:

| Table | Greedy, gamma 2 (planted / wrong) | Greedy, gamma 4 | Lace ≥ 0.5 | Lace ≥ 0.9 |
|---|---|---|---|---|
| bin_aligned_bool | 0/15, 0 wrong | 5/15, 2 | 15/15, 73 cross | 15/15, 0 |
| bin_aligned_i64 | 0/15, 0 | 0/15, 0 | 15/15, 11 | 15/15, 3 |
| bin_weak_joint | 0/30, 0 | 6/30, 2 | 30/30, 34 | 16/30, 6 |
| xor_bool | 0/6, 0 | 0/6, 4 | 6/6, 0 | 6/6, 0 |
| cont_factor | 0/18, 0 | 0/18, 0 | 18/18, 5 | 18/18, 0 |
| cont_mixture | 0/12, 0 | 0/12, 0 | 12/12, 7 | 12/12, 1 |

At the record's 0.5 threshold the posterior over-groups the small
binary tables (individual states place the 15 planted columns into
1–5 views); at 0.9 it is sharp: 86 of 96 planted pairs with 10
cross pairs joined. The weak-joint family is the exception at 0.9
(16 of 30): its planted depprobs spread across 0.5–1.0, the honest
posterior answer for dependence that is only joint.

Runtime: lace fits took 51–151 s per planted table, 116.3 s on
Animals and 211.7 s on Satellites, against greedy totals of
0.1–2.3 s per planted table and 0.55 s on Animals (debug build;
release builds narrow but do not close a two-to-three-order gap).
The pass's budgets are bounded-work contracts — a 300-million-cell
search budget inside a profiling call that also runs 42 other
detectors — and a posterior fit does not fit inside them: a
20,000-row binary table at a reduced 500-iteration fit took
256.9 s and still under-mixed (95 of 337 cross pairs at ≥ 0.5);
the full 5,000-iteration protocol scales to ≈ 2,600 s for that one
table.

Determinism, measured rather than assumed. Same seed (1337) and
same input twice on Animals: the depprob matrices and the per-state
view assignments are **bitwise identical** (fit times 116.3 s and
147.8 s under contention). Across seeds the posterior is not
stable at 5,000 iterations on the wide table: pairs at ≥ 0.5 number
605 (seed 1337), 453 (seed 42) and 858 (seed 777); Jaccard overlap
with seed 1337 is 0.579 and 0.358 at the 0.5 threshold (0.604 and
0.433 at 0.9); mean absolute depprob difference 0.099 and 0.168,
maximum 0.562 and 0.750; all three seeds agree on 297 pairs. The
named pairs stay high across seeds (0.562–1.000) but are not pinned
at 1.000. A fixed seed makes the engine exactly reproducible; it
does not make the answer seed-insensitive.

## Decision: repair the greedy criterion, or adopt lace

| Axis | Repair the greedy search | Adopt the lace engine |
|---|---|---|
| Recovery (planted, 500 rows) | 0/96 at every gamma ≤ 2; 11/96 at gamma 4 with wrong joins. No measured setting works; a repair means redesigning the score, not tuning it | 86/96 at depprob ≥ 0.9, 10 cross joins; weak-joint structure stated honestly |
| Recovery (Animals) | 29/408 both-viewed dependent pairs; 25/144 at ≥ 0.9 | Reference itself: states use 3–7 views against the greedy 37 |
| Runtime | 0.1–6 s per table inside the existing work budget | 51–212 s per small table; ≈ 2,600 s projected at 20,000 rows; outside every current budget |
| Determinism | Exact by construction (fixed order, no randomness) | Exact at a fixed seed (bitwise-identical repeat); materially seed-sensitive across seeds at 5,000 iterations |
| Contract costs | None: findings stay `basis="estimated"` with exact `verify()` recounts; bounded work; dependency set unchanged | Findings would rest on a stochastic fit (the basis and verify story needs redesign); bounded-work budgets abandoned or the engine runs as an opt-in second pass; a large new shipped dependency (the `lace` crate and its tree) |

Recommendation the measurements support: adoption is the only
measured route to recovering the planted structure — no greedy
setting on the swept grid recovers it, and the overlay test shows
the failure is in the score, not the search. Adoption is
nevertheless not implemented here: it changes what a finding means
(estimated-from-a-fit becomes sampled-from-a-posterior), breaks the
bounded-work contract the profiler's budgets are built on, and its
runtime is two to three orders above the pass's current cost at
table sizes the profiler is expected to handle in one call. That is
a product decision that needs its own change. What this record
establishes for that decision: the greedy criterion as written
cannot be tuned into recovering obvious structure; a repair attempt
has to replace the score's per-column charging (the refinement
charge, the Gaussian misfit on discrete data, and the NIG slicing
gain above), not adjust gamma, caps, or sample sizes.

## What remains open

- The merge score itself: the three charges in the criterion
  section, each with its measured size. Any repair must re-run this
  battery and the overlay test.
- The #359 defaults: still set by contract consistency, not by a
  recovery curve. The settling sweep is specified in the #359
  section; its recovery axis stays flat until the criterion changes,
  so it should be run as part of a criterion repair, not before.
- Parity and other purely joint structure: unreachable while merges
  are seeded by pairwise gains alone.
- The Animals attempt cap: 320 dependent pairs are never attempted
  at cap 256. The cap's own sweep (issue #73) measured recovery
  against the same criterion, so its knee is a knee of a broken
  score; re-sweep it after any criterion repair.

## Reproduction

- Lab: `cargo test -p onceover-mines --locked -- --ignored lab_`
  (writes JSON under `VIEWS_LAB_OUT`, default
  `~/workspace/onceover-pkg/lace-lab/out`).
- Lace harness: `benchmarks/lace-depprob/` (standalone crate;
  `depprob` and `kinds` modes), scorer `score_depprob.py` logic as
  recorded in the working notes.
- Corpus tables: `benchmarks/corpus/data/crosscat/lace-animals` and
  `lace-satellites` (byte-identical to the lace crate's bundled
  resources; sha256 verified).
