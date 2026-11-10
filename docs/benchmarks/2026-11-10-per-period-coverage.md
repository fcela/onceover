# Per-period coverage: corpus behaviour (2026-11-10)

## Question

Issue #203: the profiler states declared-panel missing cells but
never computes how much data each observed period carries, so a
period holding a fraction of the usual rows — collection started
mid-period, an outage, a truncated final period — is invisible in
the report. The change adds a `panel_coverage` detector (default
set) that counts rows per period on the frame's time axis and,
where an entity axis exists, distinct entities per period, and
reports `partial_period` findings for periods far below the
typical period. This record covers the corpus behaviour of the
new check: the corpus contract before and after, and the datasets
whose finding sets changed.

## Protocol

- Builds: the unmodified tree at the current main head and the
  issue-#203 branch build rebased onto that head, both
  `maturin develop --release` into separate environments;
  nothing else differed.
- Corpus: `python benchmarks/corpus/run_benchmarks.py --json-out
  <path>` over all 34 registered datasets, default budget, on each
  build.
- Synthetic panels: the cases in `tests/test_panel_coverage.py`
  (40 entities × 10 weekly periods with an interior dip and a
  truncated final period; the same panel uniform; a frame with no
  time axis; an integer week axis; a declared-panel case).
- The rule, as shipped: the typical period is the median period;
  a period is partial when its row count is below 75% of the
  typical row count and short by at least 4 rows, or its entity
  count is below 75% of the typical entity count and short by at
  least 2 entities; the check needs at least 4 observed periods;
  at most 10 periods are stated, worst shortfall first; periods
  are singled out only when they make up at most a quarter of all
  periods — past that share the coverage entry reports the spread
  instead of naming periods.

## Results

Corpus contract, both builds: 23 datasets profiled, 11 skipped;
machine-verified expectations 93/93 passed; documented cases
found 18, missed 10, partial 4; the documented misses sit in the
same five datasets (thyroid, adventureworks,
metro-interstate-traffic, bike-sharing-hourly,
adult-census-income); runner exit code 2 on both builds (machine
checks clean; documented misses on record). No corpus expectation
changed.

Finding sets changed on one dataset:

- bike-sharing-hourly (17,379 rows): 6 `partial_period` findings
  appear. The frame's grain is day × hour, so each day is a
  period with a typical 24 rows and 24 hour-entities. The flagged
  days hold 1 (2012-10-29), 8 (2011-01-27), 11 (2012-10-30), 12
  (2011-01-18), 16 (2011-01-26), and 17 (2011-08-28) of the 24
  hourly records — the table's known short days. The dataset's
  report is capped at 100 selected findings, so the 6 new
  findings displace others in the selection: dependencies
  14 → 13, changes 76 → 74, column_roles 3 → 0; every other
  category count is unchanged.

One dataset motivated the quarter-share condition during the run:

- sec-edgar-company-facts (11,543 rows): the axis is
  `period_end`, a fiscal date attribute of each filing fact.
  141 of its 365 distinct dates (39%) hold fewer than 75% of the
  median 14 rows — per-date volume varies by construction, so no
  date is a coverage break. With the condition, the dataset's
  finding set is identical to the unmodified build's, and its
  coverage entry states the spread: "141 of 365 periods (39%)
  fall below 75% of the typical volume — too wide a spread for a
  typical period to name, so no partial-period statements".

Synthetic panels, after the change (the unmodified build has no
`panel_coverage` coverage entry and states nothing):

- The dipped panel produces two findings: the interior period
  2024-01-29 with 16 rows and 16 entities against a typical 40
  (40% of typical), and the last period 2024-03-04 with 24 rows
  and 24 entities (60% of typical), each stated with its
  position.
- The uniform panel produces no finding; the coverage entry
  records that no period fell below 75% of the typical volume.
- A frame with no time axis records `panel_coverage` as skipped,
  with the frame model's reason.

## Reading

The check fires where a collection cadence exists — a panel
whose periods normally carry the same volume — and the flagged
periods are the table's real short periods. Where per-period
volume is heterogeneous by construction, the quarter-share
condition keeps the report from restating the frame's shape as
dozens of coverage breaks; the coverage entry still records how
wide the spread is. The corpus contract is untouched: no
expectation was edited, and 22 of the 23 profiled datasets
produce identical category counts (the 11 skipped datasets skip
on both builds).
