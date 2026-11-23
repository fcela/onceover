# Agent guide: profiling a table with onceover

Reviewed for onceover 0.1.8.
`onceover.capabilities()` lists every detector, its purpose,
requirements, and activation rules.

## 1. Hand over what you know before the call

`target="<column>"` names the response and engages five passes:
`outcome_account`, `outcome_contrasts`, `target_shapes`, `leakage`,
`model_failures`. `event` names the positive target level.
`entity` names the unit that can repeat across rows.

**Meaning.** `gloss` maps columns or levels to their meanings.
`units` names the units, and `labels` changes display names.
`recorded_after` lists columns recorded after the outcome for the
leakage screen. `purpose` and `questions` describe the task.
Coverage accounts for the declarations and identifies refusals.

**Structure.** `roles` overrides semantic roles. The `expectations`
pass checks `order_by`, `no_gaps`, `cadence`, `hierarchies` (finest
first), and `dimensions`/`measures`. `time`, `ordered_axis`, and
`level_order` select order. `exclude_columns` drops fields.
`contrast` (column and value, or a boolean column) compares a
subset with the rest. `weights` lists measures for hotspots and
entity fingerprints.

Hint keys: `time`, `ordered_axis`, `level_order`, `order_by`, `no_gaps`, `cadence`, `dimensions`, `measures`, `weights`, `roles`, `hierarchies`, `exclude_columns`, `labels`, `contrast`, `gloss`, `units`, `event`, `entity`, `recorded_after`, `purpose`, `questions`.

**Peer hierarchies.** `infer_spec()` shows how the frame will be read.
Review its `hierarchy_candidates`: multiple paths may hold. Choose
paths relevant to the comparison, then pass them as hints. For
example, when the observed path is host → program_session → program:

```python
spec = onceover.infer_spec(df, hints={"entity": "host"})
print(spec["hierarchy_candidates"])
report = onceover.profile(
    df, spec=spec, detectors=["entity_fingerprints"],
    hints={"hierarchies": [["host", "program_session", "program"]]},
)
```

Discovery alone leaves the profile unchanged. With accepted hints,
fingerprints chooses the smallest supported pool and can use a
broader ancestor. Read `peer_group` in the `misfits` or
`entity_shifts` evidence and `population_shifts` for shared changes.
See the [craft-kit case study](https://fcela.github.io/onceover/library-craft-kits)
for a worked audit.

## 2. The call and the budgets

`profile(df)` accepts a Polars frame, LazyFrame, or file path.
Lazy or file inputs above 200,000 rows are sampled unless
`exact=True`; `source` states the basis. Edit a spec and pass it
as `spec=...` to hold the interpretation.

Budgets: `quick`, `standard`, `deep`.

`quick`: summaries and integrity checks. `standard` (default):
the default set, capped. `deep`: wider search.
`deadline_seconds=` bounds the run.

## 3. Reading the answer

Start with `report.to_brief(2_500)` (schema strip + `[f1]` handles)
or `report.to_digest(4_000)`. Expand via `report.finding(id)`
(`"f1"`, `"g1"`, or full id) or filter via
`report.select(category=..., type=..., column=..., target=...)`.
Findings carry `claim`, `subject`, `evidence[0]` (`metrics`, `basis`).
`report.coverage` (dict by pass) states what ran, skipped, or capped;
silence only speaks for work that ran.

`report.verify(df)` returns a DataFrame recounting checkable findings;
`report.rows(id, df)` extracts supporting rows.
`report.to_json(max_chars=..., compact=True)` gives compact JSON.
`profile_many({...})` checks foreign keys across tables.
`detectors=["partition_scagnostics"]` adds group shapes; standalone
`partition_scagnostics(df)` returns a `dict`.
`report.diff(other)` compares runs; `report.to_checks()` exports
constraints; `report.to_html()` (`html` extra) renders HTML.

## 4. Detector selection

The recommended set runs 31 of the 47 registered detectors.
Pass `detectors=[...]` (or `onceover.ALL_DETECTORS`) to focus a
wide table if `selection` caps findings. `entity_fingerprints`
compares entity mixes by rows or weights along an axis.
