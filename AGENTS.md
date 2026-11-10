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
time is reported in coverage, not silently dropped. The bound
also applies inside the dependence screen, the hotspots scan,
the discontinuities pass, the surprises scan, and the
duplicate-encodings scan: they stop at internal work quanta
and their coverage entries state how much of the search space
was examined (pairs screened of total, combinations examined
of total, targets fully screened of total), so a pass stopped
early reads as `capped`, not as a clean negative.

## Reading a report

A report is a JSON-shaped object (contract:
`docs/report-schema.json`, field-by-field in
`docs/report-schema.md`). Its parts:

- **Findings** — ranked statements. Every finding carries an
  `id`, a `category`, and a `type` (always present; the pair is
  the reliable key for filtering and grouping). Every finding also
  carries `facets` — its properties as fields (`scope`, `object`,
  `evidence`, `conditionality`, `temporality`, `status`, and
  `quality_dimension` on data-quality findings); the category is
  derived from the facets by a fixed table, so filtering on either
  agrees. `claim` is the
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
- **Source** — `profile()` also accepts a Polars LazyFrame or a
  file path (`.parquet`, `.csv`, `.ipc`). Those reports carry a
  `source` section: the source's full row count and the basis —
  `full`, or `sampled` with the sample's method, seed, and cap.
  On a sampled basis every finding's evidence scope records the
  sample and the source population; read sampled numbers as
  statements about the sample, not the population.

## Multiple tables

`onceover.profile_many({"orders": orders, "customers": customers})`
reads a set of tables together and reports candidate foreign keys:
column pairs from different tables whose distinct non-null values
are fully contained in a unique column on the other side.
Candidates whose column names agree (case folded, underscores and
spaces removed) form the strong tier; the rest are reported too,
each with a chance score — how easily the containment happens by
accident. Dense integer ranges contain each other by construction,
so a lower-tier integer candidate with a score near 1 says the
values alone carry no evidence either way; a score near 0 says
chance cannot explain the containment. Partial containment into a
unique parent is reported as an overlap with exact orphan-row
counts (nulls are never orphans). The result is a
`MultiTableReport` — `candidates`, `overlaps`, `coverage`,
`to_digest()`, `to_json()`, `candidates_frame()` — separate from
the single-table report: `profile_many` does not profile the
individual tables, and same-table pairs are not examined. Coverage
states how many pairs were considered at each stage and which
columns were not examined.

## Getting findings back out

```python
report.finding("<id>")                       # one finding or None
report.select(category="dependencies")       # filters compose:
report.select(type="functional", column="x") # category, type,
                                             # column, target,
                                             # and facet fields
                                             # (object=, status=,
                                             # conditionality=, ...)
report.to_digest(budget_chars=8_000)                # rollup for triage
report.to_json()                             # full report
report.to_html()                             # self-contained HTML
        # rendering of the same report (needs the `html` extra:
        # pip install 'onceover[html]'); charts are Altair with
        # the Vega bundle inlined, so the file opens offline
report.to_json(max_chars=12_000)             # budgeted view; a
        # `budget` record in the output states what was cut
report.findings_frame()                      # Polars DataFrame
                                             # index of findings
report.associations()                        # Polars DataFrame of
                                             # every pair the
                                             # association screens
                                             # tested: effect sizes,
                                             # raw and BH-adjusted
                                             # p-values, tier
report.diff(other)                           # findings added /
        # removed / changed between two profiles of a table,
        # matched by stable id, plus coverage status changes
report.verify(df)                            # recount statements
        # through Polars: ok, mismatch, or not_checkable
report.rows("<id>", df)                      # the rows behind a
        # statement, with positions in `__row`
report.to_checks()                           # exact statements as
        # Polars expressions; "great_expectations" gives a suite
```

`findings_frame()` is an index (one row per finding), not the
evidence; fetch the full finding by `id` when a row matters.

## Command line

Installing the package also installs an `onceover` command:

```sh
onceover data.parquet                  # digest of the file's profile
onceover data.parquet --digest 4000    # digest with a character budget
onceover data.parquet --format json    # the full report as JSON
onceover data.parquet --format html -o report.html
onceover orders.parquet customers.parquet   # several files: the
        # multi-table report (candidate foreign keys), digest or JSON
```

The flags mirror the renderings plus the file-input controls
(`--budget`, `--deadline-seconds`, `--source-row-cap`, `--exact`,
`--sample-seed`, `--target`). The HTML format needs the `html`
extra, like `report.to_html()`.

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
    "roles": {"row_no": "index"},   # semantic role override:
                                    # identifier | index | timestamp |
                                    # timestamp_part | dimension |
                                    # measure | text
    "hierarchies": [["unit", "team", "division"]],
                                    # known hierarchy, levels finest
                                    # to coarsest; verified exactly,
                                    # stated once (or once as a
                                    # deviation naming broken mappings)
    "exclude_columns": ["note"],    # columns not profiled at all;
                                    # stated in coverage as user-declared
})
```

Passing a bare string where a list is expected raises
`ValueError`.

## Detectors and targeting

`onceover.available_detectors()` lists the detector set with the
categories each produces and each detector's activation (also
frozen in `docs/capability-manifest.json`). `detectors=[...]`
restricts a run to named detectors. The default set has 28 of
the 35 registered detectors: `views` (experimental), `dates`,
`pivot`, `period_associations`, `drift`, and `granularity` are
opt-in and run only when named — a default run records each as
skipped in coverage, naming it opt-in. `expectations` is in the
default set but engages only on declared hints. Passing
`target="y"` additionally enables the target-gated shape pass.
The full category and type reference is `docs/findings.md`.

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
