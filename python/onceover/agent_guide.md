# Agent guide: profiling a table with onceover

Ships in the package, version-matched to it.
`onceover.capabilities()` is its machine-readable companion.

## 1. Hand over what you know before the call

What you know about the table decides which passes run, so
declare it in the call.

**The outcome.** `target="<column>"` designates the response
column. A designated target engages three passes otherwise
skipped: `target_shapes` (each numeric predictor's relationship
shape to the target), `leakage` (a column that determines the
target or dwarfs every other relationship to it), and
`model_failures` (where a cross-fitted model of the target
fails). An unknown target goes to coverage, not an error.

**Meanings and structure.** `roles` overrides a column's
semantic role; the vocabulary is `identifier`, `row_number`,
`timestamp`, `timestamp_part`, `dimension`, `flag`, `measure`,
`text`. A role outside the vocabulary is ignored; the frame section's
evidence shows whether one took. `labels` gives a column a
display label for prose; evidence keeps raw names.

Hint keys: `time`, `ordered_axis`, `level_order`, `order_by`, `no_gaps`, `cadence`, `dimensions`, `measures`, `roles`, `hierarchies`, `exclude_columns`, `labels`, `contrast`.

The expectations pass checks `order_by`, `no_gaps`, `cadence`,
`hierarchies`, and `dimensions`/`measures` exactly; checked
declarations tag their evidence `reference_source="declared"`,
and unusable entries go to coverage, not errors. `cadence`
values are `"daily"`, `"business_day"`, `"weekly"`, `"monthly"`,
or a step count; `hierarchies` are level columns, finest to
coarsest. Instructions to the run: `time`, `ordered_axis` (a non-date
ordering column), `level_order` ({column: [levels in order]},
never inferred), `exclude_columns` (dropped pre-run),
`contrast` (`{"column": name, "value": scalar}`; a boolean
column needs no `value`) profiles one subset against the rest. Other unknown keys raise an error.

**Units, grain, and your questions.** No hint key carries a
column's unit, the grain (what one row is), or the questions
the data serves. Hold them on your side: read magnitudes
against your units; pull each question's findings with
`report.select(...)`. A question no finding answers is
information when coverage shows the pass ran.

```python
report = onceover.profile(df, target="is_canceled",
    hints={"time": "arrival_date", "labels": {"adr": "average daily rate"}})
```

## 2. The call and the budgets

`profile()` also accepts a LazyFrame or a file path, sampled
above 200,000 rows unless `exact=True`: the `source` section
states the basis.

Budgets: `quick`, `standard`, `deep`.

`quick`: summaries and integrity checks. `standard`
(default): the default set, capped. `deep`: wider caps,
deeper search. `deadline_seconds=` bounds the run. Bad
budgets, detectors, hint keys, or hint values raise errors
listing the valid values.

## 3. Reading the answer

The digest, `report.to_digest()`, leads with the frame account
(grain, time axis), then counts and one line per
group of related findings; `report.finding(id)` expands one.
`report.coverage` records each pass as run, skipped, capped,
or ineligible; an empty findings list with skipped or capped
coverage means the table was not fully examined, not clean data.

A finding's key is its (`category`, `type`) pair;
`report.select(category=..., type=..., column=..., target=...)`
also filters on facet fields. The `claim` is a statement with
evidence attached: read its `metrics` payload and `basis`
(`measured`, `estimated`, `inferred`) before quoting it.
`interestingness` ranks attention within a category; it is
not a probability. Verify before acting:
`report.verify(df)` recounts the statements; `report.rows(id,
df)` returns one statement's rows.

## 4. Detector selection

The default set runs 32 of the 43 registered detectors;
`detectors=[...]` restricts a run. `capabilities()` lists each
detector's membership, trigger, and purpose. Skip
selection at first.
