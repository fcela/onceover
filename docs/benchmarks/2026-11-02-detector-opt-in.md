# Detector opt-in, discontinuity tiering, and report caps

The 2026-11-02 detector battery (40 runs over the 20 bundled corpus
datasets, two configurations per dataset) measured five detectors
producing zero findings in every run, one detector dominating runtime,
and two detectors producing about 41% of all report characters. This
change acts on those three measurements:

1. `dates`, `pivot`, `period_associations`, `drift`, and `granularity`
   become opt-in detectors. They stay registered and run when named in
   `detectors=[...]`; a default run records each as `skipped` in
   coverage with a detail line naming it opt-in. The default set goes
   from 27 detectors to 22 (the already opt-in `views` and the
   target-gated `target_shapes` were never in it). `expectations` is
   formalized as hint-gated: a default-set member that engages only on
   declared hints. Activation is a registry field
   (`default` / `opt_in` / `hint_gated` / `target_gated`) returned by
   `available_detectors()` and frozen in the capability manifest.
2. `discontinuities` stays default-on, but at the standard budget its
   diagonal scan considers the 6 strongest axes per target on wide
   tables instead of 10 (15 column pairs instead of 45). The deep
   budget keeps the full search. Tables with at most 20 numeric axes
   already scan every pair and are unaffected.
3. `functional_dependencies` and `gbm` gain per-detector caps at
   selection: at most 25 and 20 selected statements attributed to each
   (attribution is by the detector of a statement's first evidence
   record). A cap is never silent: when one binds, coverage gains a
   `selection` entry with status `capped` stating how many statements
   per detector were not selected.
4. The corpus registry gains the discontinuities detector's first
   expectations, on the datasets where the battery observed it firing
   (see the state-unemployment exception below).

## Evidence from the battery

- The five opt-in detectors produced zero findings in all 40 battery
  runs. The before/after harness below confirms zero findings for them
  on all 20 datasets in the default configuration, so the measured
  cost of the change is zero findings lost.
- `discontinuities`: 0.43 s median marginal cost (2.4x the next
  detector), +83.0 s of excess runtime across the battery, 208
  findings, and zero expectation hits — it had no corpus expectations
  at all.
- `functional_dependencies` produced 550,443 of 2,479,798 total report
  characters and `gbm` 467,098: together about 41%.

## Measured effect

The same harness (default configuration, standard budget, all 20
bundled datasets) ran before and after the change on the same host
(Python 3.12.3, Polars 1.44.2, 4 threads). The before run reproduces
the battery's totals exactly (2,479,798 report characters).

| Measurement | Before | After |
|---|---:|---:|
| Total profile time, 20 datasets | 105.9 s | 88.0 s |
| Total report characters | 2,479,798 | 2,323,840 |
| functional_dependencies findings / chars | 300 / 550,443 | 183 / 331,219 |
| gbm findings / chars | 174 / 467,098 | 155 / 386,201 |
| discontinuities findings / chars | 208 / 412,874 | 194 / 383,562 |
| taiwanese-bankruptcy profile time | 38.97 s | 20.76 s |
| taiwanese-bankruptcy diagonal units | 59,850 | 19,950 |

The caps bind on six datasets, each disclosed in coverage:
soybean (gbm, 6 statements past the cap), taiwanese-bankruptcy
(gbm, 5), absenteeism-at-work (functional_dependencies, 178),
adventureworks (functional_dependencies, 55), hospital-dha (gbm, 42),
and lace-satellites (functional_dependencies, 74). On saturated
reports the 100-finding selection stays full and the mix changes:
slots the two capped detectors no longer take go to the next-ranked
statements (numeric_dependence findings 71 -> 111, surprises
141 -> 167, ordered_partitions 10 -> 17 across the 20 datasets). On
the one unsaturated capped report, hospital-dha, the report falls
from 96 to 52 findings and from 232,582 to 89,559 characters; the
coverage entry states the 42 capped gbm statements.

The discontinuity tiering cuts the diagonal term exactly 3x on wide
tables (taiwanese-bankruptcy 59,850 -> 19,950 units, state-unemployment
32,130 -> 10,710, hospital-dha 38,430 -> 12,810) and leaves narrow
tables byte-identical: on thyroid, abalone, bank-marketing,
absenteeism-at-work, adventureworks, sec-edgar-company-facts, and
lace-satellites the coverage detail (units screened, candidates
flagged, jumps reported) is unchanged. Reported jumps on wide tables
fall where the dropped diagonals carried them: taiwanese-bankruptcy
37 -> 28, state-unemployment 91 -> 81, hospital-dha 3 -> 1. A
discontinuities-only profile of taiwanese-bankruptcy takes 16.31 s
after the change; the pass measured 33.05 s inside the full profile
before it.

## Corpus expectations for discontinuities

Nine datasets gain an expectation (kind `finding`, category `changes`,
type `step_discontinuity`, pinned to the column carrying the
top-ranked observed finding): thyroid, abalone, bank-marketing,
taiwanese-bankruptcy, absenteeism-at-work, adventureworks,
sec-edgar-company-facts, hospital-dha, and lace-satellites. The last
two, plus state-unemployment, pin their detector lists in the
registry; `discontinuities` was added to the hospital-dha and
lace-satellites lists.

state-unemployment is the exception. The detector fires there (81
selected findings in the default run), but adding it to that
dataset's pinned harness list floods the fixed 100-finding report:
associations findings fall from 20 to 0 and ordered_partition
findings from 25 to 13, failing two existing expectations that record
real structure. Its pinned list is unchanged and it carries no
discontinuities expectation; giving that dataset a discontinuities
expectation belongs with the selection-mix question, not this change.

The corpus run after the change: 20 ok / 0 failed / 11 skipped, with
all nine new discontinuities expectations passing.
