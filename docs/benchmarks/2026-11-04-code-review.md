# Code review — 2026-11-04

Base: main `996f144c` (frame-model step 2 merged; schema 0.5). Scope, in
priority order: the step-2 code (`frame.rs`, `account.rs`, `pipeline.rs`,
report `ledger.rs`), cross-crate duplication of helpers, dead code proven
by search, and measured performance of the two passes the detector
battery flagged (`docs/benchmarks/2026-11-04-detector-battery-step2.md`:
discontinuities at 56.8% of corpus pass time; hotspots at 11.2 s on
gse15258 against ≤44 ms elsewhere).

Every timing below comes from runs in the review session on the base
build and on the changed build, same harness both times: the full
default detector set with `timing=True`, median of 3 repetitions, over
the 22 bundled corpus datasets. Changes marked IMPLEMENTED are in this
change set; everything behavior-touching is a recommendation only. The
identity proof for the implemented set is at the end: all 22 report
JSONs are byte-identical before and after (sha256), so nothing in this
change set alters any report.

## Problem 1: discontinuities recomputes identical scan totals (measured; IMPLEMENTED)

The battery's headline reproduces on the base build: discontinuities
takes 56,275 ms of 100,692 ms total pass time (55.9%). Per dataset:
bike-sharing-hourly 20,595 ms (90.7% of its total), adventureworks
15,090 ms (77.7%), taiwanese-bankruptcy 13,608 ms (73.2%),
bank-marketing 2,171 ms (56.9%).

An instrumented build (temporary counters, reverted) splits the scan
thread time on the three heaviest datasets. `scan_totals`
(`crates/onceover-mines/src/disc.rs`) — the per-half totals, base-model
Grams and rhs of a scan — took 28,363 / 14,359 / 15,566 thread-ms over
34,976 / 46,153 / 24,817 calls (bike-sharing / taiwanese-bankruptcy /
adventureworks). But `scan_totals` is a pure function of (background,
target, fit): it never reads the projection `u` or the cuts. The distinct triples number ≈1,575 on bike-sharing (210 axis units
plus 1,365 target-pair combinations) against 34,976 calls, ≈1,274 on
adventureworks against 24,817 calls — about 95% recompute on both —
and 13,205 on taiwanese-bankruptcy (8,930 axis units plus 4,275
target-pair combinations) against 46,153 calls, about 71%. The
recompute happens because:

- each diagonal direction of a pair (14 of the 16 configured
  directions execute; the two axis-aligned ones are skipped as
  duplicates of the axis scans) rescans the same background;
- a flagged unit rescans the identical base for its refinement scan
  (`scan_unit` called `disc_scan_ordered` twice, and each call
  recomputed the base).

Fix: `disc_scan_ordered` splits into a base-taking core
(`disc_scan_with_base`); `scan_unit` computes the base once for its
coarse and refinement scans; `screen_target` computes it once per
(target, pair) and shares it across directions. The arithmetic and
the accumulation order of the one computation are unchanged.

Result (same harness, median of 3): discontinuities 56,275 → 42,287 ms
(−24.9%); bike-sharing 20,595 → 14,700 ms; adventureworks 15,090 →
10,982 ms; taiwanese-bankruptcy 13,608 → 10,349 ms. Corpus total pass
time 100,692 → 81,874 ms (−18.7%). One small dataset (abalone) showed
771 → 1,632 ms between the two timing sessions; solo re-probes of the
changed build measure 494–537 ms over 3 repetitions, and no code path
in the change adds work for small frames, so the session number is
recorded as contention noise, not a regression.

## Problem 2: hotspots counts combinations that cannot report (measured; IMPLEMENTED)

Reproduced: hotspots takes 11,108 ms (timing median) to 11,269 ms
(probe) on gse15258-expression. Its coverage text states the
mechanism: 5,001 eligible columns, 12,502,500 combinations examined,
200,040,000 cells examined. `evaluate_combo`
(`crates/onceover-mines/src/hotspots.rs`) counts every pair over the
encoded rows and only then discards the combination when its complete
rows number under 200. The frame has 86 rows, so all 12,502,500 pairs
are counted — about 0.89 µs per pair over 86 rows — and none of them
can ever report.

Fix: a combination's complete-row count is bounded by any one
feature's non-missing count. The pass now computes per-feature present
counts once (one pass over the encoded matrix) and skips combinations
whose bound is under the 200-row reporting floor (extracted as
`MIN_COMPLETE_ROWS`, shared by both sites). Skipped combinations
provably produce no hits; the coverage text, which is computed from
the combination list rather than the evaluated set, is unchanged.

Result: hotspots on gse15258 11,108 → 1,120 ms (timing median);
1,238–1,276 ms in probe runs. The residual ≈1.1 s is the combination
enumeration itself: 12.5 million `Vec<usize>` allocations to build
the list whose only uses are the cells-examined sum and the
examination loop. Streaming that enumeration instead of materializing
it is a further exact saving, left as a recommendation (Problem 5)
because it also reworks the triples path.

Recommendation (behavior-touching, not implemented): on frames with
200+ rows the pair enumeration is still unbounded. `HotspotConfig`
has no combination cap, and `ProfileConfig.max_pairs` is plumbed only
to the dependence screen (`pipeline.rs` maps it to `dep_pair_cap`).
A cap changes `cells_examined`, hence the Bonferroni z bar and the
coverage text, so it needs an explicit decision — for example a
`max_combinations` knob in feature order, mirroring `max_triples`, or
routing `max_pairs` to this pass as well.

## Problem 3: the discontinuity radix sort (measured; no change)

The same instrumentation puts `sort_order` at 36,603 / 16,167 /
21,793 thread-ms — 39–42% of scan-unit thread time on the three heavy
datasets. A 16-bit-digit variant of the radix was micro-benchmarked
standalone at n = 600 / 6,819 / 17,379 / 20,000: slower at every size
(0.175 ms vs 0.014 ms at n = 600; 0.535 ms vs 0.424 ms at n = 17,379),
because the 65,536-entry counts array costs more than the four saved
passes; both variants were verified to produce the identical
permutation. The standalone 8-bit sort runs 0.42 ms at n = 17,379
against ≈1.9 ms per call in situ under 4-way rayon, which points at
memory-system contention rather than the algorithm. No change.

## Problem 4: typing numbers computed twice for reported findings (measured; IMPLEMENTED)

`typing_numbers` is a pure function of a flagged candidate.
`screen_target` computes it for every typed leader, and
`make_finding` computed it again for every reported finding
(measured make_finding share: 383 / 172 / 332 thread-ms of typing
totals 2,425 / 4,529 / 1,993 on the three heavy datasets). The
leader's typing result now travels with the candidate through the
reporting filters into `make_finding`. Same values, computed once.

## Problem 5: further measured redundancies (recommendations only)

- Axis-unit data is rebuilt per (target, axis): `unit_data` measured
  5,034 thread-ms over 8,930 calls on taiwanese-bankruptcy (12% of its
  scan thread time). For a fixed target, each axis unit re-extracts
  the target vector, and the axis column's standardization over the
  unit's kept rows recurs across targets whenever the kept rows
  coincide (on frames without missing values, every target sees the
  shared sample). The diagonal path already caches exactly this
  (`PairCache`, keyed by column pair and validated by the kept-row
  vector); the axis path has no counterpart. An axis cache mirroring
  `PairCache` is the fix; not implemented here because it adds cache
  machinery to a correctness-critical screen for a smaller share.
- Hotspots combination list materialization: ≈1.1 s on gse15258 after
  Problem 2's fix (see there).
- Adjacent wide-frame costs observed on gse15258, outside the two
  named passes: dependence_screen 6,129 ms (it screens all 12,497,500
  pairs; its pair cap limits only the detailed statistics) and
  functional_dependencies 2,956–3,675 ms across runs (275,110 directed
  pairs over 86 rows). Not profiled further in this pass.

## Problem 6: duplicated helpers across crates (search-proven; IMPLEMENTED)

Each item below was verified by reading both bodies in the base tree.

- `value_is_null`: identical 14-line bodies at
  `crates/onceover-deps/src/fd.rs:74` and
  `crates/onceover/src/account.rs:97`. Now one method,
  `TypedCol::is_null_at` in onceover-core; all 8 call sites converted.
- `numeric_at`: identical bodies at
  `crates/onceover-mines/src/encode.rs:177` (`pub(crate)`),
  `crates/onceover/src/account.rs:112`, and as an inline closure in
  `dep.rs` `pairs_of`. Now one method, `TypedCol::numeric_at` in
  onceover-core; 26 call sites converted. The fourth copy, in
  `breaks.rs:278`, differs (bounds-checked access, non-finite values
  filtered) and stays.
- FNV-1a: `hashutil.rs:17` (`fnv64`, private) and `fd.rs:871`
  (`fnv64_local`) are byte-identical functions in the same crate.
  `hashutil::fnv64` is now `pub(crate)` and the only copy. The
  parallel per-value hash mapping in `fd.rs` `value_hash64` and
  `hashutil` `build_hashes` implements the same mapping twice by
  construction (one folds nulls to 0, the other keeps a null mask);
  unifying those two is left as a recommendation, since
  `build_hashes`' per-column loops are specialized.
- The round-to-4-decimals idiom `(x * 10_000.0).round() / 10_000.0`
  appeared four times: three inline in
  `crates/onceover-report/src/ledger.rs` (evidence score, triviality
  factor, selection score) and once as `account.rs`'s local `round4`.
  Now `onceover_report::round4`, used at all four sites.
- The modal-step diff counting (count consecutive differences of the
  distinct ticks, take the modal difference, smallest on ties) was
  implemented in `frame.rs` `modal_step_label` and again inline in
  `account.rs` `dominant_structure`. Now one function,
  `frame::modal_diff` (`pub(crate)`), used by both.
- `FrameAxis` was constructed at six sites in `frame.rs`, each
  spelling out the same four not-yet-measured span fields
  (`min`/`max`/`distinct_values`/`step: None`) that
  `span_from_ticks` later overwrites — leftover scaffolding from the
  two-step landing. The literals now go through one constructor,
  `FrameAxis::unmeasured` in onceover-report.

Not unified, with reasons:

- Float formatters at three precisions — account `fmt_num` ({:.3}),
  sentinels `fmt_num` ({:.4}), mines `fmt_float` ({:.6}) — produce
  different strings by design; unification would change claim text.
- `quantile` exists three times (core `card.rs`, mines
  `disc.rs::quantile_sorted`, mines `surprises.rs` over value-row
  pairs) with the same linear-interpolation formula but different
  slice types and callers; consolidation is plausible but the
  per-site risk outweighs the ~15 lines.
- `dep.rs` `midranks` (1-based) and `shapes.rs` `ranks` (0-based) are
  the same midrank algorithm up to a constant offset; the offset is
  load-bearing at the call sites, so merging is not bit-safe without
  a per-site audit.

## Problem 7: dead code (search-proven; IMPLEMENTED)

Searches covered the Rust workspace, the Python package, tests,
benchmarks, docs, and notebooks.

- `onceover::stride_rows` (`crates/onceover/src/lib.rs`): a
  "legacy function name" wrapper delegating to
  `onceover_mines::stride_rows`. Zero callers anywhere in the
  searched tree. Removed. This narrows the onceover crate's public
  Rust API (the function was `pub`); nothing in the workspace, the
  Python bindings, or the test suite used it.
- `onceover_mines::dep::screen` plus its re-export
  `screen_dependence` (`crates/onceover-mines/src/lib.rs`): a wrapper
  over `screen_sampled` with fixed arguments. Zero callers; the
  pipeline calls `screen_sampled` directly. Removed.
- `frame::parse_timestamp`: called only inside `frame.rs` (and its
  unit tests) but declared `pub`, where its siblings are
  `pub(crate)`. Narrowed to `pub(crate)`. Also a public-API
  narrowing, flagged here for the same reason.

Checked and not dead: `finalize_ledger` and `select_findings`
(exercised by onceover-report tests); `DepsOutcome.n_exact` /
`n_afd` (read by fd.rs to build its coverage string); every
`DiscConfig`, `TierConfig`, and `ProfileConfig` field has at least
one reader.

## Problem 8: step-2 code computes some facts twice (read + measured reasoning; IMPLEMENTED)

- `compute_grain` (`account.rs`) collected the axis's distinct ticks
  into a `BTreeSet` twice on the no-key path: once for the exact-key
  factorization branch and once for the near-key branch. The count is
  now computed at most once, lazily, and shared by both branches; the
  exact-key path's cost profile is unchanged when the second branch
  never runs.
- `time_peak_label` (`account.rs`) was called once per distinct tick
  to build the time grouping's peak labels, and each call re-scanned
  all present ticks twice (day alignment, month starts) — quadratic
  on day-aligned axes: 609 distinct × 31k ticks ≈ 38M operations on
  state-unemployment-long, and a daily axis with 100k distinct values over
  1M rows would cost ~2×10^10. The two alignment booleans are now
  computed once per grouping.
- `pipeline.rs` extended the findings vector with
  `screen.findings.clone()` while `screen.results` and
  `screen.coverage` were still needed afterwards; the findings
  themselves are never read from `screen` again. Now moved with
  `std::mem::take`, the pattern the deps path already uses.

Read-only judgment, no change: `profile_inner` is a single ~1,500-line
function, and the axis ticks are materialized by `axis_ticks_micros`
in up to three places per profile (span measurement, the account,
the calendar-family demotion), each O(n). Both are worth a future
restructure; neither is a safe mechanical edit in a review pass.
`TargetProfile` is exported under two names (`targets::TargetProfile`
and `gbm::TargetResult`); cosmetic, left alone.

## Identity proof and gate

- Corpus runner (registry configuration): 22 ok / 0 fail / 11 skipped
  before and after.
- Report capture (registry configuration, timing off): per-dataset
  selected-findings counts identical (1,529 in total), and the full
  report JSON is sha256-identical on all 22 bundled datasets before
  and after. The JSON carries no timestamps or run ids, so no
  normalization was needed: the comparison is raw byte-for-byte on
  all 22, including the three largest (gse15258-expression,
  10,614,286 bytes; taiwanese-bankruptcy, 352,497 bytes;
  state-unemployment, 342,828 bytes).
- Coverage texts for discontinuities and hotspots are identical on
  every dataset in the timing runs.
- Full library gate in CI order on the final tree: see the pull
  request for the run record.
