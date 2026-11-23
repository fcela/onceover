# onceover — agent brief

onceover profiles a Polars DataFrame in one call and returns a
report of ranked, evidence-backed findings: what is in the data,
what looks structurally notable, and what was checked and found
nothing. It is built for an agent that needs a good picture of an
unfamiliar dataset in a single turn.

## The packaged guide

```python
import polars as pl
import onceover

df = pl.read_csv("data.csv")
report = onceover.profile(df)          # budget="standard" by default
print(report.to_digest(4_000))         # sized for a context window
```

The brief for using the library ships inside the package,
version-matched to the build: `onceover.agent_guide()` returns
it — the handover (a designated target, column roles, display
labels, and every hint key with its semantics), the call and the
budgets, reading the report, and detector selection.
`onceover.capabilities()` returns the compact capability
manifest derived from the registry on every call. Read the guide
first; what follows here is only what it does not carry.

## Inspecting the interpretation before the run

`onceover.infer_spec(df)` returns the frame's resolved interpretation without profiling it: every column's role, the time axis, the entity, the target and the weights. Each entry is marked `declared` or `inferred` and carries its evidence. Edit it and pass it back as `profile(df, spec=...)`: an unedited spec reproduces the plain run, and every edited entry is held exactly.

`hierarchy_candidates` lists observed finest-to-coarsest paths
for review. Pass the paths you accept in
`hints={"hierarchies": [...]}` to use them for peer comparisons.

## What the guide states only in brief

- **Deadline reach.** A `deadline_seconds=` bound also applies
  inside the dependence screen, the hotspots scan, the
  discontinuities pass, the surprises scan, and the
  duplicate-encodings scan: they stop at internal work quanta
  and their coverage entries state how much of the search space
  was examined (pairs screened of total, combinations examined
  of total, targets fully screened of total), so a pass stopped
  early reads as `capped`, not as a clean negative.
- **Work bounds.** The surprises scan also has a deterministic
  work budget over row visits and bitset words. When that budget
  binds without a deadline, its coverage entry reads `capped`,
  states the work spent and the contexts and categorical targets
  examined, and later passes still run; a truncated search is
  never a clean negative.
- **Evidence payloads.** Several detectors attach a bespoke
  evidence payload under a named metric key (for example
  `mapping` on functional dependencies, `segments` on breaks,
  `duplicate_groups` on duplicate rows, `periods` on drift).
  Read the payload before paraphrasing the finding.
- **Sampled sources.** On a sampled basis every finding's
  evidence scope records the sample and the source population;
  read sampled numbers as statements about the sample, not the
  population.
- **Report contract.** The report's JSON shape is
  `docs/report-schema.json`, field by field in
  `docs/report-schema.md`.
- **Hint syntax.** Passing a bare string where a list is
  expected raises `ValueError`. The per-key reference beyond
  the guide is "Declared expectations" in `docs/findings.md`.

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
columns were not examined. `MultiTableReport.to_json(max_chars=...)`
collapses candidate and overlap records into a truncation summary;
below the reduced skeleton's size it raises `ValueError` rather than
emitting invalid JSON.

## Partitioned scatterplot diagnostics

`onceover.partition_scagnostics(df, pairs=[("x", "y")],
partition_by=["protocol"])` searches observed context columns for rules
that sharpen 2D scatterplot geometry on held-out rows using content-keyed
sampling and Bonferroni-adjusted label permutations. `profile(df,
detectors=["partition_scagnostics"])` runs the same search integrated with
the frame model (excluding identifiers, row numbers, timestamp parts, and
exact identities), budget tiers, and cooperative deadlines. Omit `pairs` or
`partition_by` to search eligible columns under the caps. `metrics` selects any non-empty subset of `monotonic`, `functional`,
`binned_striated`, `binned_stringy`, and `binned_clumpy` (all five by
default). Both whole-partition gain and exceptional single-subgroup
(`standout_subgroup`, including minority sign reversals down to 5% share)
rules are supported, guarded against range-restriction and flat-axis-collapse
artifacts; `report.rows(id, df)` extracts the annotated partition rows. Read `findings`
for the rules and labeled per-group effects and `coverage` for sampling,
caps, and validation outcomes.

## Getting findings back out

The guide covers `report.finding(id)` (accepts full ids or short
report-local handles `"f1"`, `"g1"`), `report.select(...)`,
`report.to_brief(2_500)`, `report.to_digest()`, `report.to_json()`,
`report.verify(df)`, and `report.rows(id, df)`. All digest/brief
rendering, JSON compacting/budgeting, lookup, filtering, diffing,
`infer_spec`, and `profile_many` execute in Rust. The rest of the
surface:

```python
report.to_brief(2_500)                       # token-compact LLM brief:
        # dense role-grouped schema+range strip plus section-budgeted
        # findings tagged with short [f1] handles
report.to_digest(4_000, compact=True, include_schema=True)
report.to_json(compact=True)                 # compact JSON: short handles,
        # collapsed skipped on-request coverage, trimmed bulky arrays
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
        # and a column-card comparison (columns added, removed,
        # retyped, and the card quantities that moved)
report.to_checks()                           # exact statements as
        # Polars expressions; "great_expectations" gives a suite
```

`findings_frame()` is an index (one row per finding), not the
evidence; fetch the full finding by `id` when a row matters.

## Command line

Installing the package also installs an `onceover` command:

```sh
onceover data.parquet                  # digest of the file's profile
onceover data.parquet --format brief   # compact LLM brief with schema strip
onceover data.parquet --digest 4000 --compact
onceover data.parquet --format json --compact
onceover data.parquet --format html -o report.html
onceover orders.parquet customers.parquet   # several files: the
        # multi-table report (candidate foreign keys), digest or JSON
```

The flags mirror the renderings plus the file-input controls
(`--budget`, `--deadline-seconds`, `--source-row-cap`, `--exact`,
`--sample-seed`, `--target`). The HTML format needs the `html`
extra, like `report.to_html()`. With several files, the single-table
controls are rejected: `profile_many` examines every value and takes
no sampling controls, and `--target` applies to one file only.

## Detectors and targeting

`onceover.available_detectors()` lists the detector set with the
categories each produces and each detector's `membership`
(`default_set` or `on_request`) and `trigger` (`always`, `hints`,
or `target`) (also frozen in `docs/capability-manifest.json`).
`detectors=[...]` restricts a run to named detectors.
`onceover.RECOMMENDED_DETECTORS` is the default selection, with 31 of
the 47 registered detectors. Pass `detectors=onceover.ALL_DETECTORS`
to request every registered detector. Eligibility still depends on
the budget, hints, target, and data. With the default selection, a
designated target also activates target-triggered detectors. The full
category and type reference is `docs/findings.md`.

The `outliers` pass is in the default set, but a default run
states a `categorical_outlier` finding only when every level
its pattern-breaking rows carry is rare in the column overall
(at or below 1% of the column's values); statements withheld
by that requirement are counted in the pass's coverage entry,
and naming `outliers` in `detectors=[...]` states them.

`breaks`, `ordered_partitions`, and `ordered_axis` ordinarily scan
the frame's time axis or an axis declared with `ordered_axis`.
`discontinuities` requires a usable declared numeric axis in a
recommended run. Naming one of these detectors in `detectors=[...]`
allows it to scan any axis it supports. Coverage records skips.

Name `entity_fingerprints` to compare each entity's distribution
over single dimensions and pairs that add a joint signal. The pass
uses row counts or each usable `weights` column separately.
With a resolved time or ordered axis it also looks for changes
against matched peers. Coverage names the weight, periods, entities
tested, and pair cap. The book's entity fingerprints chapter has
worked examples.

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
