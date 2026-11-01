# Ordered partitions (MOB): verification record

2026-11-01. New detector `ordered_partitions` (finding category
`changes`, type `ordered_partition`, form `recursive_partition`):
model-based recursive partitioning of a numeric response along a
date, datetime, or integer-year axis. Per node, OLS of the
response on a level and a linear trend in the ordering variable.
Candidates compete per node on two-segment SSE: every admissible
breakpoint along the axis, and every prefix bipartition of each
categorical dimension's levels ordered by their within-level
fitted level. Stopping is by deterministic bars — relative SSE
gain at least 2% and split F (2 and n−4 df) at least 15 — plus a
24-row segment floor, depth and leaf caps (standard: depth 3,
8 leaves, 128 responses, 25 findings, 20,000 rows per response
by systematic sample in t order; deep: depth 4, 12 leaves, 512
responses, 64 findings, 100,000 rows, 64 levels per dimension).
Quick tier: disabled. Design: the panel/MOB design note of
2026-10-31 (decisions D1–D5 taken as recommended there).

## The F floor is calibrated, not chosen

A relative-gain bar alone recurses into noise: on a 240-point
pure-noise series the best breakpoints carry relative gains to
4% and F to 6.4. Calibration of the floor, measured with a
temporary Rust harness over 2,000 pure-noise series at the
panel's own scale (n = 609, segment floor 24, gain bar 2%):
largest noise F observed 11.96; an F floor of 10 admitted 3 of
2,000 series, a floor of 15 admitted none. The shipped floor is
15. A permanent Rust test keeps 40 seeded 609-point noise series
silent at the shipped bars. For scale: a 15% SSE gain at n = 609
is F ≈ 52, and the panel's real root splits run to F = 3,659.

## State unemployment panel, wide form (design expectations)

Bundled corpus data, 609 months × 51 series (BLS via FRED).
Deep budget, detector isolated so the report can carry every
tree (protocol: `tests/test_partitions_panel.py`, which pins
these numbers):

- Trees fitted: 51 of 51 series.
- First (root) split in 2008-08..2008-12 for **34 of 51** series
  (design bar: at least 30; the numpy design prototype also
  counted 34).
- Median tree relative SSE gain **0.870**, minimum 0.702 (design
  bar: median at least 0.45, the prototype's two-break figure;
  the trees fit up to 7 splits, so they exceed it).
- Negative control, same panel with each series shuffled in
  time: **0 findings**.
- Same panel without the date column: coverage
  `not_applicable` ("no date, datetime, or year-like integer
  column to order by"), 0 findings.

At standard budget under the corpus entry's detector set, the
report carries all 25 trees the pass emits (its per-pass cap);
the registry pins at least 20 plus `ran_exact` coverage, and the
entry now loads with `parse_dates` so the axis exists (the
breaks pass in the same entry was previously in the same
position). Under the full default detector set, report
selection admits 6 of the 25: the discontinuities pass posts 91
findings at a blended 0.897 on this panel and the 100-finding
report cap binds. That is report-level selection behavior,
recorded here rather than tuned around.

## State unemployment panel, long form

New bundled corpus entry `state-unemployment-long` (31,059
rows; a logged unpivot of the wide panel, same public source).
One pooled tree for `unemployment_rate` with `state` as the
dimension (deep budget):

- Root split on `date` at **2008-11-01**, F = 3,659.
- 12 splits in all, 13 leaves, pooled relative gain **0.559**;
  time splits and state bipartitions alternate down the tree
  (state sides are level-ordered groups, e.g. {ND, SD, NE, VT,
  NH, IA, HI} against the higher-rate states post-2008).
- Registry expectations: one `ordered_partition` finding,
  `ran_exact` coverage.

Known miss, recorded as a tripwire test: the design's
preanalysis measured a 2014→2016 rise contrast topped by the
energy states (WY, OK, ND) — a localized slope pattern. The
shipped search orders dimension levels by fitted level and the
greedy tree prices the panel's large level contrasts first;
within the depth cap it never isolates a mid-2010s node where
the energy grouping is the best split, so no split has an
energy-dominated side. `test_energy_state_split_is_a_known_miss`
asserts the absence; if a future search (for example
slope-ordered dimension scans) finds it, the test fails and the
expectation is promoted, not deleted.

## Cost

Per-pass elapsed (coverage timing, this machine):

- Wide panel, standard: 6.8 ms (51 responses × 609 rows).
- Long panel, deep: 16.4 ms (31,059 rows, one response).
- Synthetic dated frame, 200,000 rows × 50 responses with a
  planted step, standard: 169 ms (each response systematically
  sampled to the 20,000-row cap).
- The 200,000-row scale fixture has no ordering column, so the
  pass is not applicable there and adds no time.

## Regression surface

- Corpus (31 entries, this build): 20 ok / 0 failed / 11
  skipped (run 20261101T184938Z).
- Detection battery: 57 pass / 2 fail / 0 gap / 9 observed —
  unchanged from the pre-change build; the 2 failures are the
  documented single-binary-predictor drift boundary cases.
- Python tests: 170 passed.
- Rust workspace tests: 187 passed; `cargo fmt --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` clean;
  release metadata check and its tests pass.
- One selection-layer fix was required to ship the new type at
  all: `Finding::validate` carries a hard-coded allowlist of
  `changes` types, and selection silently drops findings that
  fail validation; `ordered_partition` is now in the allowlist,
  with a regression test in the report crate
  (`ordered_partition_finding_validates_and_survives_selection`).
