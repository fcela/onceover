# Discontinuities axis-data cache — 2026-11-04

Issue #118. Base: main `657123061d7e8ae6dd209774060ffddedf45c8ae`
(PR #142 merged). One file changes:
`crates/onceover-mines/src/disc.rs`.

## Problem 1: the axis path rebuilds per-target data that depends only on the axis

The 2026-11-04 code review measured the discontinuities pass at
42,287 ms across the corpus after its Problem 1 fix, with
`unit_data` — the per-unit extraction and standardization — at
5,034 thread-ms over 8,930 calls on taiwanese-bankruptcy. An
instrumented build in this session (temporary counters, reverted)
reproduces the call count exactly and adds the sort split on the
same dataset: `unit_data` 8,930 calls; `sort_order` inside
`scan_unit_with_base` 28,434 calls; the diagonal path's target
extraction 1,425 calls. (Thread-ms totals from the instrumented
run are load-dependent and are not quoted as the baseline; the
call counts are the stable part.)

The waste is structural. For one target, the axis loop calls
`unit_data` per axis column, and `unit_data` recomputes the kept
rows, the target vector, the axis standardization, the parity
fit mask and the [1, z, z²] background. The standardization,
fit mask, background and the sort order of z depend only on the
axis column and the kept-row set — not on the target — yet they
were rebuilt for every (target, axis) pair: on
taiwanese-bankruptcy, 95 targets × 94 axes. The diagonal path
already shares its per-pair data through `PairCache`; the axis
path had no equivalent.

## The change

- `DiscCaches` gains an axis cache next to the pair cache. Per
  axis column it stores `AxisData { rows, z, stats, fit, bg,
  order }`: the kept rows, the standardized column with its
  (mean, sd), the parity fit mask, the flat [1, z, z²]
  background, and the sort order of z. An entry is used only
  when its kept rows match the caller's exactly — the kept rows
  are the complete-case selection for the caller's target, so
  the missingness mask is part of the key in effect, and a
  column-only key would be a correctness bug. A cache miss
  builds the entry with the existing `standardize()` (the same
  arithmetic `unit_data` inlined) and stores it.
- The budget is declared in code as `AXIS_CACHE_BYTES =
  64 MiB`, the same figure and the same policy as the pair
  cache: byte-accounted inserts, an insert that would exceed
  the budget clears the cache first, and an entry larger than
  the budget is used but not stored. One entry costs
  41n + 16 bytes for n kept rows. Eviction can only cost time,
  never results: every entry is a pure function of (column,
  kept rows), rebuilt by the same deterministic computation.
  On taiwanese-bankruptcy's geometry the whole cache is
  95 × (41 × 6,819 + 16) ≈ 26.6 MB, inside the budget with no
  clearing expected.
- `scan_unit_with_base` splits into a wrapper that computes
  `sort_order(u)` and `scan_unit_with_base_order`, which takes
  the order from the caller. The axis path passes the cached
  order; the diagonal path, whose projection differs per
  direction, uses the wrapper as before. No sort algorithm
  changes.
- A per-call target-vector memo in `screen_target`: the target
  values over one kept-row set are extracted once and shared
  by the axis and diagonal loops (previously re-extracted per
  pair on the diagonal path, and per axis inside `unit_data`).
- `unit_data` is removed; `scan_unit` remains as the
  `#[cfg(test)]` reference implementation the tests check the
  cached paths against.

## Identity

Full report JSON dumps from the base build and the changed
build over every runnable corpus dataset (23 profiled, 11
skipped per the registry), compared recursively with only
`elapsed_ms` dropped: **23/23 identical**. The dumps were
produced with the corpus runner's own protocol (registry
budget, detectors and hints per dataset), so the comparison
covers different missingness masks across the corpus, and a
second pair of timing-enabled dumps cross-checks identically.

Four tests are added in `disc.rs`:

- `axis_cache_matches_standardize_across_row_sets` — two
  targets with different missingness produce two kept-row sets
  for one axis column; the cache serves each set bit-for-bit
  the `standardize()` result (z, stats, background, fit mask,
  sort order), including after the second set displaces the
  first and the first is rebuilt.
- `axis_scan_via_cache_matches_scan_unit` — the cached-data
  scan returns the identical candidate (threshold, jump, z,
  t_fit, gain, n_above, all compared as bits) as the reference
  scan of the same planted step.
- `axis_cache_enforces_its_byte_budget` — with a budget below
  one entry, data is served and nothing is stored; with a
  one-entry budget, a second column's insert clears the first
  and the byte accounting never exceeds the budget.
- `planted_steps_survive_missingness_with_caches` — a planted
  axis step and a planted diagonal step, with a different null
  pattern per column, are both reported through
  `profile_discontinuities` at the default config and at a
  deep config (row cap 50,000), and a repeated run produces
  byte-identical findings.

## Timings

Protocol: matched release builds (`maturin develop --release`
with `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=8` on both sides —
the stock profile's link step does not fit this machine's
memory; the override is identical for both builds and is not
the repo profile the gate uses), `RAYON_NUM_THREADS=4`, the
pass's own `elapsed_ms` from a full profile with `timing=True`,
median of 3 fresh processes per dataset.

A first comparison across sessions was confounded by other
builds sharing this machine (the baseline bankruptcy median
measured 32,906 ms under load against 13,055 ms in a quiet
window), so the headline numbers are a back-to-back A/B: the
baseline and changed builds were each rebuilt and measured
consecutively in the same quiet window, same harness, same
order.

| dataset | base median (range) | changed median (range) | change |
|---|---|---|---|
| taiwanese-bankruptcy | 13,055 (12,739–13,180) | 8,770 (8,567–9,084) | −32.8% |
| bike-sharing-hourly | 17,665 (17,521–22,062) | 17,263 (15,957–29,713) | −2.3% |
| adventureworks | 2,960 (2,836–2,988) | 2,754 (2,690–2,821) | −7.0% |
| bank-marketing (control) | 2,345 (2,153–2,403) | 1,958 (1,916–2,136) | −16.5% |

Complete corpus, single timed dumps in the same window:
discontinuities total over the 18 datasets with a measured
pass falls from 39,906 ms to 36,342 ms (−8.9%). Per-dataset
single runs at the sub-second end move by tens of milliseconds
in both directions; the one dataset whose single run suggested
a regression, bank-marketing (+18.0%), was re-measured under
the median-of-3 protocol and improves by 16.5% (the control
row above).

Peak memory, taiwanese-bankruptcy profile (child-process peak
RSS): 317 MiB base, 359 MiB changed — a 42 MiB increase,
inside the cache's declared 64 MiB budget.

The issue's aim — at least 10% off the discontinuities pass on
taiwanese-bankruptcy, no regression above 5% elsewhere, no
peak-memory growth beyond the declared cache budget — holds:
−32.8% on bankruptcy, −2.3% / −7.0% / −16.5% on the other
protocol datasets, +42 MiB against a 64 MiB budget.

## Negative results and scope limits

- Bike-sharing-hourly does not measurably improve (−2.3%,
  inside its repetition spread). With roughly a dozen numeric
  columns its pass time sits in the diagonal direction scans —
  a fresh projection per direction, which no cache can share —
  not in axis rebuilds. The cache does not hurt it, but the
  result bounds where this optimization pays: wide frames with
  many targets per axis column.
- The target-vector memo's ceiling measured small in the
  instrumented run (the diagonal extraction it removes was
  1,425 calls against 8,930 axis units on bankruptcy). It is
  kept because it costs one slice comparison per unit, but it
  is not the source of the improvement; the axis cache is.
- No sort variant was tried. The win is not sorting faster but
  not re-sorting (and not re-standardizing); this follows the
  code review's Problem 3 measurement, where a 16-bit radix
  variant was slower at every size.
