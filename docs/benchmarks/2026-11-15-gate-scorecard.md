# Gate scorecard: scoring a build's output against its base

Date: 2026-11-15. Change: issue #317 (measurement only; no detector,
renderer or library code changed). Code: `benchmarks/gate/`.

## Question

The test suites check that each detector still produces its finding on
a synthetic frame. They do not measure whether the output of one build
is better or worse than the output of another, and the 4,000-character
digest, which is what a reader sees first, is not scored. The gate
scores a build on seven components and compares two scorecards.

## Method

`benchmarks/gate/run.py` profiles 18 corpus tables (13 gated, 5 held
out) with the installed build and writes a JSON scorecard with
per-dataset metrics, totals over the gated datasets, and provenance
(library version, commit, package versions, seeds).
`benchmarks/gate/compare.py base.json head.json` prints the metrics of
both with their deltas. Count metrics have zero tolerance because the
runs are seeded and deterministic (two runs of the baseline build
produced identical counts). Total time may grow by 20% or one second,
whichever is larger. Per-dataset values are compared as well as totals.

Components: planted recovery (six structures injected into real
tables, each verified with Polars/NumPy before profiling), false
relationship statements on tables with every column shuffled
independently, facts lost or gained under five layout transforms,
hand-written reference statements scored at 4k, 8k and in the full
report, repeated facts and budget use in the 4k digest, `report.verify`
statuses, and cost. The definitions, the exclusions in the invariance
comparison, and how to add datasets, plants, components and reference
statements are in `benchmarks/gate/README.md`.

## Baseline

Build: onceover 0.1.5 at commit 1618886 (release build, macOS arm64,
Polars 2.1.0, NumPy 2.5.5, seed 0). Scorecard:
`benchmarks/gate/results/main-1618886.json`. The run takes about 100
seconds. Totals over the 13 gated datasets:

| Component | Metric | Total |
|---|---|---:|
| planted | plants applied / found in full report / found in 4k digest | 46 / 34 / 20 |
| permuted | false relationship statements, full report / 4k digest | 144 / 136 |
| permuted | same, with a target declared | 75 / 67 |
| invariance | facts lost plus gained: reverse / row shuffle / rename / noise / scale | 269 / 104 / 0 / 30 / 5 |
| reference | `must` statements found (of 11): 4k / 8k / full, default mode | 7 / 11 / 11 |
| reference | `should` statements found (of 12): 4k / 8k / full, default mode | 7 / 9 / 11 |
| reference | `must` found, target mode: 4k / 8k / full | 7 / 11 / 11 |
| reference | `should` found, target mode: 4k / 8k / full | 8 / 8 / 10 |
| reference | statements whose own check fails | 0 |
| redundancy | repeated facts in the 4k digest, default / target | 4 / 3 |
| redundancy | numbers with six or more decimals in the 4k digest, default / target | 53 / 41 |
| truth | `verify` mismatch / error / ok / not_checkable | 0 / 0 / 858 / 1423 |
| cost | median profile seconds (sum over datasets) / findings | 6.9 / 1089 |

`finding_permille` (characters on finding lines per thousand characters
of the digest) is summed over datasets in the totals; per-dataset
values are in the scorecard.

The reference statements are provisional: they were written from
reading seven tables (adult-census-income, bank-marketing, abalone,
bike-sharing-hourly, state-unemployment-long, sec-edgar-company-facts,
gse15258-expression) and are marked `"provisional": true` in
`benchmarks/gate/reference/`. Their scores are a starting point until
the statements are reviewed.

## Observations on the baseline

- The 4k digest states 20 of 46 planted structures; the full report
  states 34. A structure present in the report and absent from the
  digest is a selection result, not a detection result.
- Shuffling every column leaves 144 relationship statements in the full
  report. Some follow from the marginals alone (an order `a <= b`
  between columns whose ranges do not overlap); they are counted.
- Reversing the column order changes 269 facts. On abalone the changed
  facts are `axis_shape` statements for different column pairs.
- Renaming columns with a prefix changes no fact. A suffix such as
  `__x` changes the role the library assigns to a column named like
  `employee_id`, so the rename transform uses a prefix.

## Held-out datasets

south-german-credit, soybean, lace-satellites, sec-edgar-company-facts,
gse15258-expression, marginal_ranges and messy_intake are scored and
written to the scorecard (`held_out_totals`) and are reported apart by
`compare.py`. They are excluded from the pass/fail comparison unless
`--include-held-out` is given.

## Not included

- The `family`-level and per-detector views of the existing quality
  battery (`benchmarks/quality/`) are unchanged and not folded in.
- Titanic and diamonds, used in an earlier prototype, are not in the
  corpus and are not added.
- Seeds: the default run uses seed 0. More seeds are available with
  `--seeds`; the committed baseline uses one.

## Reference coverage extended (2026-11-16)

The reference component now also scores state-unemployment-long (the
panel; `unemployment_rate` is designated as its target in the
registry), sec-edgar-company-facts (an event log, held out) and
gse15258-expression (a wide table, held out): 16 more provisional
statements, 39 in total over seven datasets. The statements were
transcribed from the briefing statement set prepared for this issue.
Each carries its measured note and its original verification
expression (`verify`); its `check` states the same fact as a single
Polars expression, and every check was re-run against the data on the
current tree — `check_failed` is 0 for the three datasets.
gse15258-expression is registered for the reference component only:
its two profiles are the most expensive in the gate — the reference
component took 326 s for this dataset on the sandbox, against 4 s for
the next slowest of the three added datasets.

Not carried over from the source set: the daily aggregation of
metro-interstate-traffic (a derived table with no corpus entry), the
two generated no-structure controls (the gate measures that property
with the permuted component on real tables), and titanic and diamonds
(not in the corpus, as noted above). The source set also marks
bank-marketing held out, by a mechanical rule; the gate's registry
does not, and that designation was left as the gate has it.

## Gate challenge tables added (2026-11-16)

Round 1 could not fold the generated expansion tables in: the gate
loads only corpus datasets. The tables are now registered in the
corpus first, under a new pack, `synthetic`
(`benchmarks/corpus/data/synthetic/`): seven bundled datasets, each
with its seeded generator as `prepare.py` (the corpus's prepare
convention; the seed is pinned in the registry entry's source line),
an `ATTRIBUTION.md`, and sha256 and row-count pins. Rerunning a
generator reproduces its `data.csv` byte-for-byte, verified for all
seven. Corpus expectations are pinned from measured runs: 45
machine-verified expectations over the seven entries, all passing,
including known-miss tripwires for the facts the current build does
not state (below).

This change was first published stacked on round 1 and was closed
unmerged when the base branch merged. It is re-published here on
current main, and every number in this section was re-measured on
the post-digest-rebuild build (main at 064132d8 plus this change);
the corpus expectations pass as pinned under the new build, with no
re-pinning.

Five are gated (column_order_stress, planted_mix,
daily_regime_break, dominant_driver, daily_level_collapse) and two
held out (marginal_ranges, messy_intake). The gate reference files
carry 46 provisional statements — the package's 39, transcribed as in
round 1, plus 7 written for the collapse series — and every check
was re-run against the data on the current tree: `check_failed` is
0, gated and held out. No new dataset is reference-only; the most
expensive took 20 s of gate wall time (planted_mix).

daily_level_collapse is new in this change, for issue #332: a daily
series whose level drops to 0.45 of its prior value on 2023-03-15
(60-day means 510 after against 1,122 before), stays there for 150
days (the window's last 30 days still measure 0.45 of the prior
level), and recovers on 2023-08-12 (ratio 1.00). Its corpus
anomaly-coverage expectation records both dates covered by typed
events at the exact dates.

Reference results for the new datasets on the current build (4k
digest, default mode, then the full report): column_order_stress
should 5/6 and 5/6; planted_mix must 2/3 at 4k and 3/3 at full,
should 3/4; daily_regime_break should 5/6 at 4k and 6/6 at full;
dominant_driver must 2/2, should 3/4; daily_level_collapse should
5/6 at 4k and 6/6 at full; marginal_ranges should 1/7; messy_intake
must 1/2, should 2/4. Against the pre-rebuild measurement of these
tables, the digest rebuild moved two planted_mix must statements
into the 4k digest (the sentinel subpopulation and the tied
follow-up missingness; the channel contrast remains in the full
report only), and moved the two daily series' complete-series
statement out of the 4k digest — it is still stated in the full
report, displaced by the briefing's section allowances. The
collapse, its duration, and the recovery statements for
daily_level_collapse all still match at 4k. The unmatched
statements are the known gaps: the noise columns' independence (no
independence vocabulary), the planted hotspot cell
(measure-weighted, over a numeric age band), the suburb
concentration, the range facts that follow from the marginals, the
sentinel inside the money strings, the status-code flip behind an
unparsed date column, and the two no-missing absence statements.
The gated reference totals are now 19 must and 42 should over nine
datasets — at 4k, 14 must and 27 should in default mode, 15 and 16
with a target designated; the held-out totals are 9 must and 15
should over five — at 4k, 4 must and 5 should in default mode, 3
and 2 with a target.

`compare.py` against the committed baseline reads FAIL (35 worse,
103 improved, 15 unchanged). Two mechanical causes: the totals now
sum over 18 gated datasets instead of 13 — planted structures
applied rise 46 → 65 and findings 1,089 → 1,293 by construction —
and the head carries the merged digest rebuild, which moves the
pre-existing datasets (repeated facts 4 → 0 and long decimals
53 → 0 in total, planted facts found at 4k up, the invariance total
up 408 → 446 as more lines sit near a taking boundary). The
remaining worse rows are the invariance lines and profile time
(the baseline was recorded on a laptop; this run is the sandbox).
A fresh baseline record belongs with the merge.

## History, report and accepted regressions (added 2026-11-15)

- `benchmarks/gate/report.py` stores one history entry per change and
  renders the history as a markdown report: headline metrics per change
  with their movement against the change before, the latest change in
  detail, every total metric over recent changes, the latest scorecard
  per dataset, and the held-out datasets.
- The CI job `Gate history` runs on each push to main and keeps the
  history and the rendered report on the `gate-history` branch.
- `benchmarks/gate/accepted.json` lists regressions accepted on
  purpose, each with a metric, a limit and a reason. `compare.py` passes
  a worse metric that an entry covers and prints the reason.

## Reproducing

    python benchmarks/gate/run.py --out scorecard.json --commit <sha>
    python benchmarks/gate/compare.py benchmarks/gate/results/main-1618886.json scorecard.json
