# Runtime attribution in the report, CI timing gate, hotspot list

Date: 2026-10-30. Issue #26, second stage. The first stage (scaling
grid, single-detector attribution) is
`docs/benchmarks/2026-10-28-runtime-grid.md`. This stage adds
attribution to the report itself, puts the fast grid tier in CI,
and dispositions the hotspot list.

## Per-pass timing in coverage

`profile(df, timing=True)` (Rust: `ProfileConfig.timing`) records
`elapsed_ms` on each coverage entry: the interval between the
previous entry being recorded and this one. Passes run
sequentially and record their entry when they finish, so the delta
attributes the pass's own work; the first entry also carries table
extraction and the column cards, and work after the last pass
(selection, grouping) is unattributed. Timing is opt-in because it
makes the coverage section nondeterministic — default reports stay
byte-reproducible, and findings are identical either way (both
properties are under test). The field is in the report schema
(`docs/report-schema.json`, optional) and documented in
`docs/report-schema.md`.

## CI timing gate

`benchmarks/timing_gate.py` runs in CI (the `timing` job in
`.github/workflows/ci.yml`): two synthetic cases from the grid
(10,000 x 50 and 1,000 x 10, standard budget, timing on) with
thresholds ~30x the 2026-10-28 build-host references. The gate's
only job is to catch order-of-magnitude regressions; it also fails
if any pass that ran lacks `elapsed_ms`. Measured on this machine
for reference: 41.8s and 0.59s (thresholds 180s and 60s).

## Attribution measurements

Release build, this machine, standard budget, `timing=True`.
Numbers are relative, not absolute (shared VM).

| table | wall | top passes |
|---|---|---|
| synthetic 10,000 x 50 | 41.8s | discontinuities 33.7s, predictability_screen 1.6s, surprises 1.0s, interactions 0.7s, multiterm_identities 0.2s |
| synthetic 1,000 x 10 | 0.59s | discontinuities 0.30s, interactions 0.21s |
| thyroid 3,772 x 32 | 1.6s | discontinuities 0.4s, functional_dependencies 0.4s, interactions 0.3s, predictability_screen 0.3s |
| state-unemployment 609 x 52 | 5.9s | discontinuities 5.0s, functional_dependencies 0.4s, multiterm_identities 0.3s |
| GSE15258 86 x 5,006 | 20.7s | dependence_screen 17.9s, functional_dependencies 2.5s |
| drift fixture 2,000 x 14 (12 junk columns, declared time axis) | 2.9s | discontinuities 2.6s; drift itself under 0.1s on this fixture |

## Hotspot list and dispositions

1. **Discontinuities screen — filed as #65.** The dominant pass on
   every measured table with 30+ numeric columns (81% of the
   10k x 50 wall time, 85% of state-unemployment). It was added
   after the first grid, so it never went through the runtime
   program. Not accepted: the issue carries the measurements and
   asks for a cost model plus cheaper screening or tighter
   standard-tier caps, with the detection battery as the behavior
   contract.
2. **Dependence screen at width — filed as #66.** 17.9s of 20.7s
   on GSE15258 (86 x 5,006): pairwise screening is quadratic in
   columns before the detailed-work caps bind — the first pressure
   point #26 named, now confirmed by in-report attribution on a
   real corpus table.
3. **Functional dependencies — accepted, recheck next grid.**
   Second at width (2.5s on GSE15258) and tied-first on thyroid
   (0.4s), but an order of magnitude below the screen it feeds;
   exact verification is the workload. No action until the next
   full grid.
4. **Predictability screen, interactions, surprises — accepted.**
   Each at or below 1.6s on the 10k x 50 grid case, bounded by
   their caps, small shares of wall time on every real table
   measured.
5. **Drift permutation gate — accepted with a re-measurement
   note.** The 2026-10-28 grid recorded 22s on a wide panel; on
   this round's 2,000 x 14 drift fixture the pass itself is under
   0.1s (the discontinuities screen dominates instead). The gate's
   cost is shape-dependent (periods x predictors x permutations);
   the next full grid run should re-measure the wide-panel shape
   before any disposition changes.
6. **Multi-term identity enumeration at the deep cap — accepted.**
   0.2–0.3s at standard on the measured tables; the deep-tier cap
   (8000 candidates) is the bound the first grid already recorded.
   No action.

## What remains on #26

Nothing structural: the grid and published curves (first stage),
in-report per-detector attribution (this stage), the CI fast-tier
gate (live with this change), and the hotspot list with
dispositions (above, with #65 and #66 filed) cover the acceptance
items. The next full grid run replaces the 2026-10-28 numbers and
re-measures item 5.
