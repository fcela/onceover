# Runtime program: first grid and attribution (2026-10-28)

Issue #26. Script: `benchmarks/corpus/runtime_grid.py`. Run record:
`benchmarks/corpus/runs/runtime-20261028T120941Z.json`. Machine: this
project's Linux build host; treat numbers as relative, not absolute.
The repository's existing scale validation (2M x 200) is in
`benchmarks/results/`; this grid covers the corpus-relevant range.

## Grid (synthetic mixed tables, seconds)

| rows | cols | quick | standard | deep |
|---|---|---|---|---|
| 1,000 | 10 | 0.03 | 0.09 | |
| 1,000 | 50 | 0.01 | 0.88 | |
| 1,000 | 100 | 0.01 | 1.31 | |
| 10,000 | 10 | 0.01 | 0.42 | |
| 10,000 | 50 | 0.04 | 5.37 | 11.68 |
| 10,000 | 100 | 0.07 | 9.04 | |
| 100,000 | 10 | 0.08 | 1.34 | |
| 100,000 | 50 | 0.29 | 16.07 | |
| 100,000 | 100 | 0.96 | 25.77 | |

Standard-budget cost grows faster in columns than in rows across
this range (100k rows: 1.3s at 10 columns, 25.8s at 100). Deep is
about 2.2x standard at 10k x 50. Quick is under a second everywhere
in the grid — and reports 0 findings on the larger synthetic tables:
the planted structure in this generator is exactly what quick's
sampling skips, so quick's cost advantage and its blindness scale
together. (The corpus tables tell the same story from the other
side: GSE15258 at standard took 20.2s for 86 x 5,006, almost all of
it width-driven passes.)

## Per-detector attribution (single-detector runs)

Thyroid (3,772 x 32), full default run 0.83s. Most expensive alone:
views 0.61s, functional_dependencies 0.38s, gbm 0.31s,
decision_tables 0.31s. Voting (435 x 17), full 0.04s: views 0.05s,
gbm 0.03s, decision_tables 0.03s.

Single-detector timings are ordinal, not additive: detectors share
the column profile and resource passes, and the full run executes
passes concurrently, so the parts sum to more than the whole. The
named pressure points from issue #26 check out as the top of the
list — the views pass and the pairwise machinery (functional
dependencies, GBM screen) are where standard-budget time goes.
