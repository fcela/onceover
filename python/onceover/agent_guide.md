# Agent guide: profiling a table with onceover

Ships in the package, version-matched;
`onceover.capabilities()` is the machine-readable companion.

## 1. Hand over what you know before the call

What you know decides what runs: declare it in the call.

**The outcome.** `target="<column>"` designates the response
and engages five passes otherwise skipped: `outcome_account`
(the outcome's distribution), `outcome_contrasts`
(per-level event rates, or medians, against the pooled
outcome; absences stated as nulls), `target_shapes`,
`leakage`, `model_failures`. Unknown targets go to coverage.

**Meaning and purpose.** These ride the frame account;
`declarations` coverage states what was checked and what was
carried unchecked. `gloss` ({column: text, or {level: text}}): a column's
meaning. `units`
({column: unit}): magnitudes, via the findings' `unit`
field. `event`: the target level counting as the event —
contrasts and nulls are stated against it, else in a weaker
per-level form. `entity`: the panel entity column.
`recorded_after` ([columns]): recorded after the outcome;
leakage checks them first, as declared suspects; model
screens exclude them as predictors. `purpose` and
`questions` ([strings]): what the data is for — a finding
answering a declared question (matched by the columns it
names) is admitted ahead of an equal-score unmatched
finding, and `questions` coverage counts the answers per
question, including zero.

**Structure.** `roles` overrides a column's semantic role;
unusable roles are refused in `declarations` coverage.
`labels`: display labels for prose; evidence keeps raw names.
The `expectations` pass checks `order_by`, `no_gaps`, `cadence`
(named or step count), `hierarchies` (finest first), and
`dimensions`/`measures` exactly; refusals go to coverage.
`time`, `ordered_axis`, and `level_order` ({column: [levels
in order]}) are applied or refused in
`declarations` coverage. `exclude_columns`, `contrast`
(`{"column": name, "value": scalar}`; boolean needs no `value`)
profiles one subset against the rest; `weights`
([columns]): hotspots judge cells by weight shares too.
Unknown keys raise.

Hint keys: `time`, `ordered_axis`, `level_order`, `order_by`, `no_gaps`, `cadence`, `dimensions`, `measures`, `weights`, `roles`, `hierarchies`, `exclude_columns`, `labels`, `contrast`, `gloss`, `units`, `event`, `entity`, `recorded_after`, `purpose`, `questions`.

```python
report = onceover.profile(df, target="is_canceled",
    hints={"event": 1, "recorded_after": ["reservation_status"],
           "questions": ["what drives cancellations?"]})
```

## 2. The call and the budgets

`profile()` accepts a LazyFrame or a file path too, sampled
above 200,000 rows unless `exact=True`; `source` states the basis.

Budgets: `quick`, `standard`, `deep`.

`quick`: summaries and integrity checks. `standard`
(default): the default set, capped. `deep`: wider caps,
deeper search. `deadline_seconds=` bounds the run. Bad
budgets, detectors, or hint keys raise errors listing the
valid values.

## 3. Reading the answer

The digest, `report.to_digest()`, is a briefing in sections
by reader question, one line per fact; `report.finding(id)`
expands one.
`report.coverage` records each pass as run, skipped, capped,
or ineligible; empty findings with skipped or capped coverage
mean the table was not fully examined, not clean data.

`report.select(category=..., type=..., column=..., target=...)`
filters the findings. The `claim` is a statement with evidence
attached: read its `metrics` payload and `basis` (`measured`,
`estimated`, `inferred`) before quoting it. `interestingness`
is the selection score that orders the report, not a
probability. Verify before acting: `report.verify(df)`
recounts the statements; `report.rows(id, df)` returns one
statement's rows.

## 4. Detector selection

The default set runs 31 of the 46 registered detectors;
`detectors=[...]` restricts a run; `capabilities()` lists
membership, trigger, and purpose per detector.
