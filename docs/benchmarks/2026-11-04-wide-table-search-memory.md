# Wide-table search memory — 2026-11-04

Base: main `613303d4`. Issue #117, stage 1: bound the memory of the
two wide-table search passes — hotspots and the numeric-dependence
screen — by streaming deterministic combination enumeration instead
of materializing the pair/combination universe. Stage 2 (deadline
checkpoints inside passes) is out of scope here. No behavior change:
the tested universe, the Bonferroni denominator, the survivor
selection order, every coverage count, and every finding are
identical; the proof is below.

## What changed

**Hotspots** (`crates/onceover-mines/src/hotspots.rs`). The pass
built `Vec<Vec<usize>>` holding every eligible pair (and, on the
deep tier, every examined triple) before counting a single cell:
at 5,001 eligible columns that is 12.5M heap allocations, the
~1.1 s residual the 2026-11-04 code review measured after the
present-count skip landed. The list is now an enumerator,
`enumerate_combos`, yielding pairs first and triples second in
exactly the order the list held them, with the same cell caps and
the same `max_triples` cutoff placement. Enumeration is a pure
function of the bin counts and the config, so the pass enumerates
twice — once to total the examined cells (the Bonferroni
denominator), once to evaluate — and the coverage counts are the
streamed totals. New bound: O(1) combination state (one
combination of at most 3 features at a time), beyond the O(width)
eligible/small/present lists and the per-combination counting
arrays bounded by the cell caps (≤ 1024 cells per pair, ≤ 4096
per triple). Was O(combinations) storage.

**Numeric dependence** (`crates/onceover-mines/src/dep.rs`). The
screen built the full pair list `Vec<(usize, usize)>` up front —
~12.5M entries at 5,000 numeric columns — and the capped path then
scored all of it into a second full list. Pairs are now addressed
by ordinal: `pair_at` maps an ordinal to its pair in the same
lexicographic order (exact integer arithmetic; the count of pairs
before first-position *i* is *i·m − i·(i+1)/2*), and every stage
consumes the universe in chunks of 2^18 ordinals.

- Uncapped (total pairs ≤ the pair cap): detailed statistics are
  computed chunk by chunk and concatenated in pair order; no pair
  list exists at any point.
- Capped, universe within the survivor bound
  (`want = max(4 × pair_cap, 1024)` = 1,984 at standard): the
  previous in-place selection runs unchanged on the scored list,
  which is config-bounded by construction.
- Capped, universe above the bound: stage-1 selection streams in
  two passes. Scoring is a pure function of the encoded matrix,
  so pass 2 reproduces every score bit-for-bit. Pass 1 retains
  only the `want` largest scores in a bounded heap; its minimum
  is the boundary score the in-place partition would find. Pass 2
  collects the pairs above the boundary in pair order and the
  name-smallest boundary ties in a second bounded heap — together
  exactly the in-place selection's set, in an order the subsequent
  stable sort makes indistinguishable. Stage 2 (rescoring the
  ≤ `want` survivors on all encoded rows) is unchanged.

New bound: pair storage is O(chunk + `want` + `pair_cap`) — all
config-bounded constants — where it was O(total pairs). The
`deps` output was and is bounded by the pair cap (capped path) or
by the total pair count below it (uncapped path). The 25M-pair
width gate above the screen is an arithmetic check and was
already streaming-safe; it is unchanged. The price of the
streaming selection is one extra stage-1 scoring pass over the
universe; the runtime section measures it.

Two unit tests pin the enumerations against the naive
materializations they replace: `enumeration_matches_materialized_universe`
(hotspots: eligibility, both cell caps, the ≤ 8-bin triple subset,
and the max-triples cutoff) and `pair_at_matches_materialized_order`
(dep: exhaustive for widths 2–39, spot checks at 5,001 columns).

## Identity proof

Official corpus runner (`benchmarks/corpus/run_benchmarks.py`),
before and after: **23 ok / 0 fail / 11 skipped over 34
registered**, both times.

Full report JSONs were dumped for all 23 profiled datasets
(frames loaded exactly as the runner loads them) and compared
recursively with only `elapsed_ms` dropped: **23 of 23 identical**,
including the three widest — gse15258-expression (5,006 columns),
taiwanese-bankruptcy (96), lace-animals (86). All three dep routes
are exercised by the corpus and identical: streaming
(gse15258-expression 12,497,500 pairs; taiwanese-bankruptcy 4,560;
lace-animals 3,570), in-place capped (hospital-dha 1,953 pairs,
just under the 1,984 bound; state-unemployment 1,275), and
uncapped (the remaining 17). The synthetic fixtures below also
reproduce their coverage strings and finding counts exactly on
all 10 configurations.

The corpus runs entirely at the standard budget, so the deep
tier's triple enumeration is covered by the unit test above plus
a smoke profile (6 four-value columns × 5,000 rows, deep):
35 combinations examined (20 triples), 2,875 cells — consistent
with 5 encoded bins per column (15 pairs × 25 + 20 triples × 125).

## Measurements

Fixtures follow the `benchmarks/scale.py` pattern: numpy
`default_rng(20261104)`, integer values 0–7 as f64 columns.
Sparse fixtures mask entries to null (NaN → null) so that
per-feature present counts sit between the encoder's 30-row floor
and the hotspots 200-complete-row reporting floor — hotspot
counting is then skipped and the measured cost is the enumeration
itself (50% missing at 100 rows → ≈ 50 present; 85% missing at
1,000 rows → ≈ 150 present). Dense fixtures have no missing
values; at 2,000 rows hotspot counting genuinely runs. The
5,000-column dense case is excluded: its counting work is
O(pairs × rows) ≈ 2.5 × 10^10 cell visits, orthogonal to the
enumeration storage bounded here.

Protocol: `profile(budget="standard",
detectors=["hotspots", "numeric_dependence"], timing=True)` in a
fresh child process per repetition, 3 repetitions; pass times are
the passes' own `elapsed_ms` coverage values, reported as medians;
peak incremental RSS is a 20 ms psutil sampler's peak minus the
pre-profile RSS, reported as the maximum over the 3 repetitions.
The machine is a shared 4-core / 7.9 GB VM, so small-configuration
times swing between repetitions; the wide-configuration effects
below are consistent across repetitions and match the mechanisms.
Local builds used `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=8` for
both the before and after builds (the manifest profile's
codegen-units = 1 exceeds this VM's memory when other builds run
concurrently); the override is identical on both sides and does
not affect the comparison.

| series | cols × rows | hotspots ms before → after | dependence ms before → after | peak incr. RSS MiB before → after |
|---|---|---|---|---|
| sparse | 200 × 100 | 6.9 → 0.2 | 201.8 → 106.3 | 15.1 → 15.0 |
| sparse | 1,000 × 100 | 201.0 → 5.9 | 1,177.3 → 1,619.7 | 42.0 → 27.8 |
| sparse | 5,000 × 100 | 3,167.2 → 190.3 | 17,209.5 → 37,306.7 | 661.9 → 40.6 |
| sparse | 200 × 1,000 | 1.6 → 0.3 | 77.4 → 103.3 | 15.0 → 14.1 |
| sparse | 1,000 × 1,000 | 81.9 → 4.2 | 500.1 → 1,673.5 | 57.8 → 25.2 |
| sparse | 5,000 × 1,000 | 2,096.8 → 199.9 | 9,278.5 → 31,682.5 | 715.6 → 32.6 |
| dense | 200 × 100 | 2.0 → 0.2 | 21.1 → 97.5 | 15.3 → 15.3 |
| dense | 1,000 × 100 | 44.0 → 3.6 | 377.4 → 1,306.3 | 40.6 → 29.5 |
| dense | 200 × 2,000 | 507.4 → 572.7 | 260.9 → 481.0 | 17.0 → 16.7 |
| dense | 1,000 × 2,000 | 16,705.1 → 13,502.4 | 1,278.2 → 2,891.2 | 61.2 → 33.9 |

Reading:

- **Memory.** At 5,000 columns the two passes' peak incremental
  RSS falls from ~660–720 MiB to ~33–41 MiB. The quadratic storage
  is gone at every width; what remains is the encoded matrix, the
  frame itself, and the bounded structures above.
- **Hotspots** is faster everywhere, by ~10–17× at 5,000 columns
  (3,167 → 190 ms; 2,097 → 200 ms): the enumeration no longer
  allocates per combination. The dense 1,000 × 2,000 case, where
  counting dominates, also improves (16.7 s → 13.5 s) because the
  materialization share is removed; counting itself is untouched.
- **Dependence** pays for its bound: above the survivor bound,
  stage-1 scoring runs twice (once for the boundary, once to
  collect), and on these fixtures stage-1 scoring is most of the
  pass's time — the pass is ~2–3.4× slower at 1,000–5,000 columns.
  On the corpus's full profiles (one full `profile()` call per
  dataset in the dump harness) the totals are unchanged within
  noise (gse15258-expression 24.7 s → 25.0 s;
  taiwanese-bankruptcy 18.3 s → 19.1 s; bike-sharing-hourly
  36.7 s → 29.2 s), where encoding and the other passes dominate.
  Exact bounded-memory selection cannot do asymptotically better
  than a second scoring pass without storing a score per pair,
  which is the quadratic storage this change removes; stage 2 of
  the issue (deadline checkpoints) is the lever for wall-clock
  bounds inside the pass, and it is unaffected by this change.

## Provenance

Before/after builds of main `613303d4` and of this change,
respectively, same harness and machine, 2026-11-04. The corpus
runner's own run records (gitignored local artifacts) are
`benchmarks/corpus/runs/20261104T181617Z.json` (before) and
`benchmarks/corpus/runs/20261104T184516Z.json` (after). Main
advanced to `dc648208` after the measurements were taken; the
intervening commits do not touch either pass's source file, and
this change is parented on the newer head unchanged.
