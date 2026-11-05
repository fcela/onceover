# Breaks series selection by prescore, not column order

Issue #126, breaks leg. Question: when more numeric series are
eligible for the breaks scan than the series cap admits (16 at
the standard budget), does the choice of which series are
scanned depend on column position, and what changes when the
scanned set is chosen by a cheap whole-population prescore
instead?

## Protocol

Plants. Frames of 5,000 daily rows (2012-01-01 to 2025-09-08),
40 Gaussian noise fillers, and one planted series, seed
20261105. Two plant families from the issue's measurement: a
variance break (standard normal, second half scaled by 3) and
a transient spike (10 consecutive rows raised by 12). Each
plant sits either in the last numeric position (after all 40
fillers) or in the first; the two shapes are pure column
permutations of the same data. A plant counts as stated when
the report carries a finding of the planted type whose subject
is the planted column.

Corpus. Full report JSON for every runnable registry dataset
(23 of 34; 11 skipped by the registry) at registry budgets,
base tree vs patched tree, compared field by field with
`elapsed_ms` dropped.

Overhead. Median of repeated profiles, base and patched builds
interleaved rep by rep so machine contention hits both builds
alike, on this contended VM, debug builds: the breaks pass
alone on the plant frame (3 reps) and on the binding corpus
dataset (5 reps), and the full registry-settings profile of
the binding corpus dataset (3 reps).

## The prescore

Every eligible series is scored once, on its time-ordered
values, before the cap binds. The score is the largest of
three components, one per break family the scan reports:

- the share of total variance between the means of 16
  contiguous time blocks (eta-squared) of the values — ranks
  level and trend breaks;
- the same share computed on the squared deviations from the
  mean — a variance break is a level break in the squared
  series;
- the share of the total absolute deviation from the median
  held by the 10 largest deviations — ranks transient
  excursions.

The top `series_cap` series by score are scanned, ties broken
by column index; the scanned set is then iterated in column
order, so a run whose cap does not bind is unchanged. A series
too short to scan (fewer points than the scan's own minimum)
scores 0. When the cap binds, coverage names the rule, the
counts, and the series that were not scanned (up to 40 names,
then a count of the rest).

## Plant results

| plant | position | base | patched |
|---|---|---|---|
| variance break | last | missed | stated |
| variance break | first | stated | stated |
| transient spike | last | missed | stated |
| transient spike | first | stated | stated |

The patched claims are byte-identical across positions and
equal to the base first-position claims:

- variance: "planted variance changes after 2018-11-05 along
  date: variance 1.003314 before, 9.036785 after; the
  training-selected split reduces holdout squared-deviation
  error by 15.8%"
- spike: "planted spikes between 2018-11-05 and 2018-11-14:
  peak value 13.504693 at 2018-11-05, 13.5 robust z from the
  local baseline"

Base coverage on the plant frames reads "16 of 41 numeric
series scanned along date over 5000 time-ordered rows, 0
skipped, N findings" with no statement of how the 16 were
chosen. Patched coverage appends: "series cap 16 bound: 16 of
41 eligible series scanned, chosen by prescore (between-block
variance shares of the series and of its squared deviations,
and largest-point deviation share), not column order; not
scanned: " followed by the 25 filler columns not chosen.

## Corpus results

22 of 23 runnable datasets produce byte-identical reports.
The one difference is state-unemployment, the only registry
dataset where the cap binds at registry budgets (16 of 51
eligible series):

- breaks findings rise from 68 to 83 (the prescore-chosen set
  carries more detectable structure; the entered findings are
  dominated by the April 2020 level shifts and spikes in state
  series the column-order set never reached);
- at report level, 14 breaks findings enter the 100-finding
  selection and 9 leave; 12 association findings leave the
  selection, displaced in ranking (the associations pass itself
  is unchanged); no finding present on both sides changes its
  claim;
- coverage gains the disclosure suffix, naming all 35 series
  not scanned.

Every other dataset scans all of its eligible series, so its
scanned set cannot change, and none does.

## Overhead

Breaks pass alone, binding corpus dataset (51 series, 609
rows): median 3.00s base, 2.69s patched — no measurable
overhead; the prescore reads each unscanned series once
(35 series × 609 values here) against a scan that fits
segmentations per scanned series.

Breaks pass alone, plant frame (41 series, 5,000 rows):
median 23.29s base (reps 18.5–25.5), 25.09s patched (reps
16.0–27.7). Full registry-settings profile of
state-unemployment: median 141.4s base, 154.7s patched, with
the patched build's fastest rep (140.2s) at the base median.
At both sizes the median shift is smaller than the spread
between repetitions of the same build on this machine.

## Regression tests

`tests/test_budget_order.py` (breaks section): the four plant
shapes above, claim equality across the position twins, and
the coverage disclosure contents. Unit tests in
`crates/onceover-mines` pin that each family's prescore ranks
above a noise series' score.
