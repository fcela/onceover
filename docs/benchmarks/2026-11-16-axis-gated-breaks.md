# Along-axis statements gated to the time axis or a declared axis: gate measurements (2026-11-16)

Issue #317, phase 3, second statement family. In a default run the
along-axis passes (`breaks`, `ordered_partitions`, `ordered_axis`,
`discontinuities`) chose their own axes: the ordered-axis battery
evaluated shapes along every numeric column and breaks along extra
numeric axes, and the discontinuities screen treated every numeric
column as an axis for every other one. A table with no time axis
received level shifts and shapes along measurement columns, stated
as if they were events along an order the reader never declared.
This record holds the before/after measurements. Base is main at
064132d8 (0.1.5); the head build is this change. All runs use the
repository's own instruments: the gate in `benchmarks/gate/` (seed
0, full runs) and the corpus in `benchmarks/corpus/`.

## What changed

- `crates/onceover/src/pipeline.rs`: an axis gate computed once per
  run. In a default run the four passes scan along one axis — a
  caller-declared `ordered_axis` when the pass can use it, else the
  frame model's time axis. Where neither exists, the pass does not
  run and its coverage entry records the skip and the reason.
  Naming a detector explicitly lifts the gate for that detector.
  The registry's membership/trigger vocabulary has no condition
  for "the frame model found a time axis", so the gate lives in
  the pipeline and the coverage record carries it; the detectors'
  registry entries are unchanged, and the capability manifest
  regenerates identically.
- The passes take the named axis: `profile_breaks`,
  `ordered_partitions`, and `profile_ordered_axis` take a
  `hinted_only` flag that confines their axis selection to the
  hinted axis (for the battery: the primary axis only, no added
  scan axes, shapes along the primary only); a pass that cannot
  use the named column states why in its coverage text.
  `profile_discontinuities` takes an axis list that confines its
  screen to the declared axis. With the flag off, axis selection
  is what it was.
- The discontinuities screen is the one pass whose axis pool never
  contained the time axis (date and datetime columns are not
  numeric to it), so for that pass only a declared axis opens the
  default-run gate.
- The `regimes` detector is not gated: its along-axis residual
  scan already engages only when a date or datetime column
  exists, and its remaining findings are not along-axis
  statements.
- One edge is recorded: on a frame whose time axis is assembled
  from timestamp parts, the battery's named axis may not resolve
  to a single column it can key on; it then records the axis as
  not usable, and explicit selection restores the unrestricted
  scan.

## Gate totals (gated set, seed 0)

| Metric | Before | After |
|---|---:|---:|
| reference must at 4k, default | 10/12 | 11/12 |
| reference must at 4k, target | 11/12 | 11/12 |
| reference should at 4k, default | 6/16 | 7/16 |
| reference should at 4k, target | 10/16 | 9/16 |
| reference at 8k and full, both modes | — | unchanged in every cell |
| planted found, full reports | 33/46 | 34/46 |
| planted found at 4k | 29/46 | 29/46 |
| permuted false lines at 4k, default | 78 | 52 |
| permuted false lines at 4k, target | 44 | 32 |
| permuted false lines, full | 144 | 117 |
| invariance violations, total | 430 | 315 |
| repeated facts | 0 | 0 |
| truth mismatches | 0 | 0 |
| findings, all gate reports | 1089 | 1033 |
| report JSON, total chars | 3,366,352 | 3,281,478 |

Held-out totals are unchanged in every cell (reference must at 4k
3/7 in both modes; planted 7 in full reports, 6 at 4k).

Two named losses, both displacement effects of removing the gated
statements, not detection losses:

- Bike sharing, reference statement `cnt-temperature` (should
  tier): its default-mode carrier was an `axis_shape` statement
  along the temperature axis, a member of the family this change
  removes. The association is still stated in target mode at 8k
  and in the full report; at 4k in target mode it falls one notch
  short, which is the should-at-4k target cell above.
- Metro traffic, planted statement `sentinel_value` (clouds_all):
  still verified and present in the full report; it no longer
  holds a line in the 4k digest. The planted total at 4k holds
  because adult-census-income gains one.

The gate comparison also reports small per-dataset worse rows:
invariance on thyroid (32 to 36) and absenteeism reverse (39 to
41) and adventureworks noise (1 to 2), and finding-line permille
wobbles of 1 to 6 on five datasets. The totals those rows belong
to all improve or hold.

## Along-axis findings per gate dataset (default run, full report)

Counts are findings by the four detectors in the capped report.
The axis named is the one the statements run along after the
change.

| Dataset | Before | After | Axis after |
|---|---:|---:|---|
| abalone | battery 36 | 0 | none — no time axis |
| absenteeism-at-work | discontinuities 12 | 0 | none — no time axis |
| adult-census-income | battery 6 | 0 | none — no time axis |
| adventureworks | breaks 12, battery 4, discontinuities 1 | breaks 12, battery 4 | order date |
| bank-marketing | discontinuities 2, battery 3 | 0 | none — no time axis |
| bike-sharing-hourly | breaks 16, discontinuities 12, battery 21, partitions 5 | breaks 25, partitions 5, battery 0 | dteday |
| congressional-voting | 0 | 0 | — |
| gse15258-expression | 0 | 0 | — |
| hospital-dha | battery 25 | 0 | none — no time axis |
| lace-satellites | discontinuities 12 | 0 | none — no declared axis |
| metro-interstate-traffic | breaks 11, discontinuities 1, battery 4, partitions 2 | breaks 11, battery 4, partitions 2 | date_time |
| nab-nyc-taxi | breaks 10, battery 1 | breaks 10, battery 1 | timestamp |
| sec-edgar-company-facts | breaks 16, discontinuities 1, battery 7, partitions 2 | breaks 16, battery 7, partitions 3 | filed date |
| south-german-credit | battery 2 | 0 | none — no time axis |
| soybean | 0 | 0 | — |
| state-unemployment (wide) | breaks 15, discontinuities 12, battery 27, partitions 12 | breaks 25, battery 5, partitions 12 | date |
| state-unemployment-long | breaks 5, partitions 1 | breaks 5, partitions 1 | date |
| thyroid | battery 20 | 0 | none — no time axis |

On bike sharing the battery still runs along dteday and generates
4 shape statements; they are displaced at the 100-finding cap by
the breaks statements the cap now admits (25 selected, 56
generated). On the wide state-unemployment panel the battery's
remaining 5 statements are the ones along the date axis; the
other 22 ran along state columns. Breaks counts rise on the two
time-axis tables where the gated findings previously displaced
them at the cap. No time-axis table loses a breaks or partitions
statement.

## Corpus

173/173 machine-verified expectations before and after; exit code
2 in both runs (documented misses on record). Five expectations
changed, each with the mechanism in its note:

- Four `step_discontinuity` expectations (bank-marketing,
  taiwanese-bankruptcy, absenteeism-at-work,
  sec-edgar-company-facts) re-baselined from found to known-miss:
  in a default run the screen now requires a declared axis, and
  these frames declare none. The screen still reports the
  statements when named explicitly. A fifth, on lace-satellites,
  is unchanged: that corpus entry names the detector explicitly,
  so its run is unaffected.
- One bike-sharing expectation (predictive on `cnt`) promoted
  from missed to found: the gated findings no longer fill the
  capped report, so the statement is selected again.

## Regression tests

`tests/test_axis_gate.py` pins the contract: a time-axis frame
states its breaks by default; a frame whose only candidate axis
is an ordinary numeric column states no along-axis findings by
default, with the skip and its reason in coverage for all four
passes; explicit selection and a declared `ordered_axis` each
restore the passes. `tests/test_discontinuities.py` and the
discontinuities cap test in `tests/test_detector_defaults.py`
now name the detector explicitly, and the first discontinuities
test also pins the default-run gate.
