# Reading a report

A finding is useful when you can connect its claim to the rows and
comparison that produced it. This walkthrough uses the bundled thyroid
fixture: 3,772 rows and 32 columns, profiled with onceover 0.1.3 at the
standard budget. It illustrates mappings, conditional rules, and unusual
observations without treating any of them as an automatic correction.

Run the example from the repository root:

```python
import polars as pl
import onceover

df = pl.read_csv(
    "benchmarks/corpus/data/gritbot/thyroid/data.csv",
    infer_schema_length=None,
)
report = onceover.profile(df)
print(report.to_digest(4_000))
print(report.data["selection"])
```

The recorded report selects 100 findings from 117 candidates and 102
distinct statements. Those counts describe this package version and
fixture. The code prints the counts for the installed version.

## Read coverage alongside the headline

`overview.headline` gives the table's shape and a short result summary.
`coverage` records completed, sampled, capped, and skipped passes. The
experimental views detector is disabled by default; the drift detector
requires a declared time axis. A method absent from the findings may have
found no eligible pattern, reached a search limit, or not run at all.

Use the detail and each evidence record's `scope.rows_evaluated` and
`scope.sampling` to determine which rows support a particular result.
Pass-level status is not a substitute for that finding-specific scope.

## Read a conditional rule's evaluated rate

One selected rule says that `t4u_measured` is false when `fti_measured` is
false. Its diagnostic recount succeeds in 85 of 85 matching rows, against
a baseline outcome rate of 11.24%. Retrieve its evaluator record rather
than taking whichever evidence entry happens to appear first:

```python
rule = next(
    f for f in report.select(category="conditional_rules")
    if f["specification"]["outcome"]["column"] == "t4u_measured"
    and any(p["column"] == "fti_measured"
            for p in f["specification"]["conditions"])
)
evaluated = next(
    e for e in rule["evidence"]
    if e["method"]["detector"] == "rule_evaluator"
)
print(rule["claim"])
print(evaluated["scope"])
print(evaluated["validation"])
print(evaluated["metrics"])
```

Association mining and PRISM contributed discovery evidence for this rule.
Their numbers describe different stages. The reported rate uses the common
`rule_evaluator` counts: `joint_count`, `antecedent_count`,
`conditional_rate`, `baseline_rate`, and `lift`. The diagnostic partition
may have participated in discovery and selection; its rate is not a final
independent estimate for future records.

## Inspect the unusual rows

The numeric surprise detector identifies an age of 455, separated from a
remaining maximum of 94. It also finds an age of 75 within the
`pregnant = true` subset, whose other evaluated ages stop at 42. The
subset result would be easy to overlook in the full column.

```python
for finding in report.select(type="extreme_value", column="age"):
    print(finding["claim"])
    for evidence in finding["evidence"]:
        print(evidence["metrics"])
```

`extreme_rows` identifies the original row indices. `extreme_values`,
`bulk_edge`, and `central_spread_p90_p10` show the values and reference
spread behind the isolation test. These are statistical discrepancies to
inspect in the source, not clinical conclusions or permission to delete
records. The detector also reports categorical values that break a nearly
universal pattern within a context; see the
[extreme-value chapter](book/methodology/extreme_values.ipynb).

## Separate mappings from predictions

A functional dependency checks whether equal inputs map to equal outputs
on the evaluated complete rows. Its violation count has a different
meaning from a model's holdout accuracy. A value can be measured on fewer
than 3,772 rows because the comparison excludes missing inputs or targets.

For prediction, read the inputs, baseline, score, train/evaluation counts,
and whether validation helped select the result. For a mapping, read its
comparison rule, sample scope, and violations. The
[evidence chapter](book/methodology/evidence.ipynb) works through those fields.

## Use rank to navigate

`interestingness` orders the report; it is not a probability of correctness.
This report reaches the default 100-finding limit, so some discovered
statements are omitted. `selection` distinguishes candidate generation,
verification, consolidation, and final selection. A missing result alone
cannot establish independence or absence of a data-quality problem.

The [capability guide](book/usage/capabilities.ipynb) helps choose a method;
[coverage](book/usage/coverage.ipynb) explains its work limits; the
[API reference](book/usage/api.ipynb) covers selection, lookup, comparison,
and serialization.
