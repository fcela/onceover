# onceover — agent brief

onceover profiles a Polars DataFrame in one call and returns a
report of ranked, evidence-backed findings: what is in the data,
what looks structurally notable, and what was checked and found
nothing. It is built for an agent that needs a good picture of an
unfamiliar dataset in a single turn.

## Quickstart

```python
import polars as pl
import onceover

df = pl.read_csv("data.csv")
report = onceover.profile(df)          # budget="standard" by default
print(report.to_digest(4_000))         # sized for a context window
```

## Budgets

`budget` is `"quick"`, `"standard"` (default), or `"deep"`.
Quick runs column summaries and integrity checks only. Standard
runs the default detector set with caps. Deep widens the caps and
searches further (more pairs, more periods, more rules). A
`deadline_seconds=` argument bounds wall time; work skipped for
time is reported in coverage, not silently dropped.

## Reading a report

A report is a JSON-shaped object (contract:
`docs/report-schema.json`, field-by-field in
`docs/report-schema.md`). Its parts:

- **Findings** — ranked statements. Every finding carries an
  `id`, a `category`, and a `type` (always present; the pair is
  the reliable key for filtering and grouping). `claim` is the
  human-readable statement; treat it as a hypothesis with
  evidence attached, not a conclusion.
- **Evidence** — each finding carries evidence blocks with a
  `scope` (rows evaluated), a `validation` status, a `basis`
  (`measured`, `estimated`, or `inferred`), and `metrics`. Metric
  keys are detector-specific and stay open by design: several
  detectors attach a bespoke evidence payload under a named key
  (for example `mapping` on functional dependencies, `segments`
  on breaks, `duplicate_groups` on duplicate rows, `periods` on
  drift). Read the payload before paraphrasing the finding.
- **Interestingness** — a ranking score for attention, roughly
  0–1. It is not a probability that the finding is correct, and
  scores are not comparable across categories. Demotion for
  triviality (a true finding that follows from something simpler,
  such as a key implied by a unique column) lowers the score; it
  never removes the finding.
- **Coverage** — per-pass status: what ran, what was skipped,
  capped, or ineligible, and why. **An empty findings list does
  not mean clean data.** Check coverage first: passes may have
  been skipped (budget, deadline, column-count caps) rather than
  run and found nothing.

## Getting findings back out

```python
report.finding("<id>")                       # one finding or None
report.select(category="dependencies")       # filters compose:
report.select(type="functional", column="x") # category, type,
                                             # column, target
report.to_digest(budget_chars=8_000)                # rollup for triage
report.to_json()                             # full report
report.to_json(max_chars=12_000)             # budgeted view; a
        # `budget` record in the output states what was cut
report.findings_frame()                      # Polars DataFrame
                                             # index of findings
report.diff(other)                           # findings added /
        # removed / changed between two profiles of a table,
        # matched by stable id, plus coverage status changes
```

`findings_frame()` is an index (one row per finding), not the
evidence; fetch the full finding by `id` when a row matters.

## Hints

Declare what you know about the data; detectors and the
expectations pass use it, and unusable hints are reported in
coverage rather than raised:

```python
report = onceover.profile(df, hints={
    "time": "date",                 # time-axis column
    "order_by": ["date"],           # list of column names
    "no_gaps": ["date"],            # list of column names
    "cadence": {"date": "daily"},   # daily | business_day |
                                    # weekly | monthly | day count
    "dimensions": ["region"],
    "measures": ["revenue"],
})
```

Passing a bare string where a list is expected raises
`ValueError`.

## Detectors and targeting

`onceover.available_detectors()` lists the detector set with the
categories each produces (also frozen in
`docs/capability-manifest.json`). `detectors=[...]` restricts a
run to named detectors; the default set excludes the
experimental `views` pass. Passing `target="y"` additionally
enables the opt-in target-aware shape pass. The full category
and type reference is `docs/findings.md`.

## Empty and partial results

- No findings + full coverage: the passes ran and found nothing
  above their reporting bars. That is a result, not an error.
- No findings + skipped/capped coverage: the dataset was not
  fully examined. Raise the budget, raise the caps
  (`max_pairs=`, `max_predictors=`, `sample_rows=`), or narrow
  with `detectors=` before concluding anything.
- Findings are leads. Verify a finding against the raw data
  before acting on it; the evidence metrics state exactly what
  was measured, on how many rows.

## Working on this repository

Build, test, benchmark, and contribution rules live in
`docs/agents.md`. The book (usage, methodology, case studies)
is under `docs/book/`.
