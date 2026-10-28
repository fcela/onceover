# Reading a report

A worked pass over one real report: the corpus's thyroid table
(3,772 patients x 32 columns, `benchmarks/corpus/data/gritbot/thyroid/`),
profiled at the standard budget. 68 findings, selected from 82
candidates (the report's `selection` block keeps the counts).

## Start with the overview and coverage

`overview.headline` gives the shape and the biggest categories.
`coverage` says what actually ran: on this table every pass ran,
some sampled. If a category you expected is absent, coverage tells
you whether the pass ran and found nothing, was capped, or was
skipped — three different conclusions. Do not read an absent
category as "nothing there" until coverage says the pass ran.

## Findings carry their evidence

Two examples from this report:

- A functional dependency:
  "tsh → tsh_measured (tsh_measured is a function of tsh;
  0 violations in 3403 rows)". The claim states the relation and
  its support; the evidence entry names the detector and whether
  the count is over the full table or a sample. 3,403 is not
  3,772: the relation is evaluated where both columns are present,
  and the scope block says so.
- A conditional rule:
  "When fti_measured = false, t4u_measured = false holds in 85 of
  85 matching diagnostic rows (100.00%; baseline 11.24%)".
  Rules are validated on a diagnostic holdout, so the denominator
  is the holdout's matching rows, and the baseline is the target's
  overall rate — the lift over baseline is the point.

## Rank is a selection device, not a verdict

Findings are ranked by `interestingness` and the report is capped
(100 at the standard budget in these runs). Three corpus tables
saturate the cap (see the CrossCat write-up), so a missing finding
can mean "ranked below the cap" as well as "not detected". The
`selection` counts (candidates vs selected) tell you which regime
you are in.

## What absence looked like here

This table's documented anomalies (a 455-year-old patient; two
pregnant patients in their seventies) produce *no* finding: the
profiler found the table's structures (assay missingness FDs,
concentration, rules) but does not isolate surprising cases inside
them. That gap is measured, with tripwire expectations, in the
benchmark corpus — see `docs/benchmarks/2026-10-28-initial-corpus.md`.
Reading a report well includes knowing which kinds of things the
current detectors cannot yet say.
