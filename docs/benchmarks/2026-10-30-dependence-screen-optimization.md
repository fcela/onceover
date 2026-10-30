# Dependence screen on wide frames — two-stage screening, sparse scoring, width gate

Date: 2026-10-30. Change (issue #66): the numeric dependence
screen's capped path no longer pays full price for the whole pair
universe. Four changes, all in `crates/onceover-mines`:

1. **Sparse MI sum** (`screen::information`): the mutual-information
   sum now iterates the nonzero joint cells, collected during the
   histogram pass and sorted, instead of the full 32×32 grid. The
   terms and their order are unchanged, so scores are
   bit-identical; the GBM screen, which calls the same function,
   benefits too.
2. **Two-stage pair selection** (`dep::screen_sampled`): stage 1
   scores every pair with `screen::information_fast` — the same
   normalized MI with logarithms decomposed over a precomputed
   integer log table — on a bounded row subsample (256 encoded
   rows, the deterministic stratified sample the codebase already
   uses; exact when the encoded sample is smaller). Stage 2
   rescores the survivors (4× the pair cap, at least 1,024) with
   the exact scorer on all encoded rows and takes the cap.
   Reported statistics are computed exactly as before, from the
   stage-2 selection onward. Stage-1 scores agree with the exact
   scorer to ~1e-12 relative (pinned by unit test at 1e-9),
   which can only matter for pairs tied at the survivor
   boundary; the survivor set is 4× oversampled against that.
3. **Selection instead of a full sort**: stage 1 partitions at
   the survivor boundary score (`select_nth_unstable_by`) and
   fills boundary ties by column name, which yields exactly the
   full sort's top entries without sorting 12.5M pairs. Marginal
   entropies are cached per column when the column has no
   missing value in the stage-1 rows (the only case where the
   entropy is pair-independent), and slot lookup is a direct
   index instead of a map.
4. **Width gate**: above 25,000,000 pairs (7,071+ numeric
   columns) pair screening is skipped and coverage says so,
   instead of truncating silently. The check is arithmetic and
   runs before the pair list is built.

## Before / after (standard budget, `timing=True`, 2-core VM)

| fixture | dependence pass before | after | wall before | after |
|---|---|---|---|---|
| GSE15258 (86 × 5,006; 12,497,500 pairs) | 16.6s | 6.1s | 19.5s | 8.9s |
| state-unemployment (609 × 52; 1,275 pairs) | 0.06s | 0.08s | 5.7s | 5.7s |
| synthetic (2,000 × 3,020; 4,558,690 pairs) | — | 7.0s | — | 8.1s |

"Before" is a clean build of main (65e80df) measured in the same
session; the issue reported 17.9s of 20.7s wall for GSE15258.
Stage attribution on an intermediate build (two-stage layout,
fast stage-1 scorer, full sort) measured stage-1 scoring at
7.7s and the full sort at 5.7s of the 13.6s pass; the selection
and scorer changes above address those two blocks. The synthetic frame is the
detector-restricted profile (`numeric_dependence` only); its
stage 1 engages the 256-row subsample (2,000 encoded rows).

## Behavior contract

- **Corpus fixtures unchanged**: findings for GSE15258 and
  state-unemployment are byte-identical between the clean main
  build and this change (full finding JSON compared, 45 and 100
  findings). Corpus run: every runnable dataset ok, all registry
  expectations met; skips are the standing ones.
- **Pre-filter recall, planted fixture**: 2,000 × 3,020 frame
  with 20 planted dependent pairs (10 linear at ρ ≈ 0.95, 5
  quadratic, 5 near-copies) among independent noise — all 20
  recovered in the reported findings.
- **Width gate, live check**: a 40 × 7,200 frame (25,916,400
  pairs) returns in 0.24s with no dependence findings and
  coverage "0 of 25916400 numeric pairs (width-gated); pair
  screening skipped: 25916400 pairs exceeds the 25000000-pair
  gate for the all-pairs stage".
- **Detection battery** (66 cases): dependence and
  discontinuity cases all pass. The 6 fails are the 2 documented
  single-binary-predictor drift boundary cases plus 4 cases for
  detectors that live on other branches (sentinel values,
  hotspots, digit fingerprints ×2) and are absent from this
  branch by construction.
- **Suites**: Python 125 passed / 1 skipped; Rust workspace
  168 passed / 0 failed, including 3 new unit tests pinning
  fast-vs-exact scorer agreement (with and without missing
  values, with entropy caching) and dependence recovery.

## What did not change

Caps are untouched (496 pairs detailed at standard), the
detailed statistics (Pearson / Spearman / Chatterjee ξ on the
disclosed row sample) are untouched, and the coverage string
format for screened tables is unchanged — it still reports all
pairs screened, because stage 1 does score every pair; the
approximation is confined to which pairs reach stage 2, inside
a 4× survivor margin. For tables whose encoded sample fits in
256 rows, stage 1 is the exact scorer up to the log-table
decomposition, and stage 2 is exact.
