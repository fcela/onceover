# Frame model step 1: semantic roles and time-axis assembly (2026-11-03)

Protocol: three frames chosen because their time axis is not a
native datetime column — split calendar parts (Beijing), timestamps
stored as strings (Metro Interstate Traffic), and a string-dated
panel (state unemployment, long form). Each was profiled with the
baseline build (main at the schema 0.3 merge) and with the step-1
build, pass 1, no hints, standard budget, on the same machine.
Timings are indicative; finding counts and coverage statuses are
the record. The corpus runner result is at the end.

## Beijing Multi-Site Air Quality (UCI 501 file, 420,768 rows × 18)

Before: 59.0 s, 96 findings. Breaks recorded "no date, datetime,
or year-like integer column; break scan skipped";
ordered_partitions was not_applicable with the same reason.
Findings targeted the frame's scaffolding: a conditional rule and
predictive dependencies on `year`, and step discontinuities along
axes including `month`, `day`, and `[No, month]`.

After: 19.2 s, 100 findings (the report selection cap; the
temporal passes now contribute most of the ledger). The frame
section assigns `No` = index, `year`/`month`/`day`/`hour` =
timestamp_part, `station` and `wd` = dimension, and the eleven
pollutant/weather columns = measure; the axis is the timestamp
assembled from `year/month/day/hour`. Breaks ran_sampled: 11 of 11
numeric series scanned along the assembled axis, 58 findings.
Ordered_partitions ran_exact: 9 findings. No finding in the report
has `No` or a calendar part as its target or as a discontinuity
axis.

## Metro Interstate Traffic Volume (UCI 492 file, 48,204 rows × 9)

Before: 1.6 s, 24 findings; breaks and ordered_partitions declined
for a missing date axis (the `date_time` column is a string).

After: 1.6 s, 37 findings. `date_time` is assigned the timestamp
role and the axis source is `string_parse`. Breaks ran_sampled
with 11 findings along `date_time`; ordered_partitions ran_exact
with 2 findings. `traffic_volume` remains a measure and a target.

## State unemployment, long form (bundled corpus entry, 31,059 rows × 3)

Before: under a second, 1 finding (the composite key); breaks and
ordered_partitions declined for a missing date axis (the `date`
column is a string).

After: 0.1 s, 7 findings. `date` is the axis (string parse),
`state` a dimension, `unemployment_rate` a measure. Breaks
ran_sampled with 5 findings on `unemployment_rate`;
ordered_partitions ran_exact with 1 finding: `unemployment_rate`
follows 8 level+trend regimes along `date`, first split at
2008-10-01 — the panel's known break structure, recovered with no
hints.

## Corpus

`python benchmarks/corpus/run_benchmarks.py`: 20 ok / 0 fail /
11 skipped (the standing test-only and unwired entries), every
registry expectation passing. No registry expectation changed:
where scaffolding-target findings disappeared from a dataset's
report, no expectation had pinned one.
