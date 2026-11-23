# onceover

Give a Polars DataFrame a first look: find data-quality problems, discover
relationships between columns, and locate changes in how the data behaves.
`onceover.profile(df)` returns ranked findings with their evidence, a
summary of every column, and coverage showing what was examined.
Computation runs in Rust, with Python and Rust APIs.
`onceover.profile_many({"orders": df_a, "customers": df_b})` reads a
set of related tables together and reports candidate foreign keys
between them: a strong tier (fully contained, column names agree), a
lower tier of contained-only candidates with chance scores, and
partial overlaps with exact orphan counts.

[Read the documentation](https://fcela.github.io/onceover/) ·
[Explore the capabilities](docs/book/usage/capabilities.ipynb) ·
[Work through the analysis notebook](notebooks/onceover_analysis.ipynb)

**For agents:** Start with `onceover.agent_guide()` for the packaged
quickstart and `onceover.capabilities()` for the detector inventory and
activation rules. Both describe the installed version.

## Start with a table

```bash
pip install onceover
```

Requires CPython 3.10+ and Polars 1.28–2.x. Binary wheels are available for
Linux x86_64 and aarch64, macOS Intel and Apple silicon, and Windows x86_64.
Building from source requires Rust and a C compiler.

This example contains a region lookup and a derived quantity. It has enough
rows for the relationship detectors to run:

```python
import polars as pl
import onceover

n = 600
quantity = [1 + i % 20 for i in range(n)]
region = [i % 3 for i in range(n)]
df = pl.DataFrame({
    "order_id": range(n),
    "quantity": quantity,
    "double_quantity": [2 * q for q in quantity],
    "region_code": region,
    "region_name": [["north", "south", "west"][r] for r in region],
})

report = onceover.profile(df)
print(report.to_digest(4_000))

for finding in report.select(category="dependencies", target="double_quantity"):
    print(finding["claim"])
```

The report can identify `order_id` as a key, recover the region mapping,
and verify the quantity relationship. Inspect a finding's `evidence` for
its evaluated rows, missing-value policy, validation, and measured effect.
A report's rank indicates which findings to inspect first; it is not a
probability that a claim is correct.

## What can onceover find?

| Your question | What to look for | Methods and examples |
|---|---|---|
| Can I join or deduplicate this table safely? | Candidate and nearly unique keys, composite keys, repeated rows | [Keys and duplicates](docs/book/methodology/keys_duplicates.ipynb) |
| Are values missing, suspicious, or inconsistently recorded? | Nulls, sentinel codes, isolated extremes, rare categorical exceptions, mixed formats, rounding changes, digit patterns | [Column summaries](docs/book/usage/columns.ipynb), [extreme values](docs/book/methodology/extreme_values.ipynb), [digit fingerprints](docs/book/methodology/digit_fingerprints.ipynb) |
| Which columns contain the same information? | Functional mappings, arithmetic identities, duplicate encodings, rollups, predictable targets | [Dependencies and identities](docs/book/methodology/functional_dependencies.ipynb), [predictability](docs/book/methodology/predictability_screen.ipynb) |
| What happens within a group? | Conditional rules, decision tables, unexpectedly concentrated combinations, local linear equations | [Rules](docs/book/methodology/rules_tables.ipynb), [hotspots](docs/book/methodology/hotspots.ipynb), [local regimes](docs/book/methodology/local_linear_regimes.ipynb) |
| How do entities differ in their activity mix? | Groups, continuums, specialists, and changes by category or pair, using row counts or declared weights with the on-request `entity_fingerprints` detector | [Entity fingerprints](docs/book/methodology/entity_fingerprints.ipynb), [library craft-kit case study](docs/book/case_studies/library_craft_kits.ipynb) |
| How are variables related? | Linear, monotone, and nonlinear dependence; changing associations by period; target shapes and predictor interactions | [Dependence measures](docs/book/methodology/dependence_measures.ipynb), [target shapes](docs/book/methodology/target_shapes.ipynb), [interactions](docs/book/methodology/interactions.ipynb) |
| Where does behavior change? | Calendar gaps, appended backfills, sustained level/trend/variance changes, temporary excursions, threshold jumps, model-routing drift | [Temporal analysis](docs/book/methodology/temporal_analysis.ipynb), [discontinuities](docs/book/methodology/step_discontinuities.ipynb), [drift](docs/book/methodology/drift.ipynb) |
| Does the layout hide structure? | Verified long-format layouts and relationships in their wide form; shared row states across column groups | [Long-format reports](docs/book/usage/findings.ipynb), [view and cluster structure](docs/book/methodology/view_clusters.ipynb) |

The [capability guide](docs/book/usage/capabilities.ipynb) connects these
questions to detector IDs, examples, and the methodological chapters.
Each method documents the statement it supports, its assumptions, and what
it can miss. Association and predictive findings describe observed
structure; they do not establish causation.

## Find scatterplot structure within groups

Select `partition_scagnostics` when a pooled scatterplot may hide
different relationships in different populations:

```python
report = onceover.profile(df, detectors=["partition_scagnostics"])
print(report.to_digest(4_000))
```

Call `onceover.partition_scagnostics()` directly to choose pairs,
candidate columns, or metrics:

```python
result = onceover.partition_scagnostics(
    df, pairs=[("x", "y")], partition_by=["protocol"], metrics=["monotonic"],
)
for finding in result["findings"]:
    print(finding["rule"], finding["validation"])
```

`partition_by` gives the search candidate columns. It chooses the rule
on discovery rows and scores it on held-out rows. Read `result["coverage"]`
to see how much of the search ran. This experimental standalone call
accepts a Polars DataFrame. [Usage and runnable example](docs/book/usage/find_partitioned_shapes.ipynb)

## Read and use the report

```python
report.to_brief(2_500)                 # Token-compact LLM brief: schema strip + [f1] handles
report.to_digest(4_000)                # Section-budgeted plain-text briefing
report.finding("f1")                   # One finding by short handle ("f1"/"g1") or full ID
report.select(column="region_name")    # Complete findings about one column
report.columns                         # All-row summaries, keyed by column name
report.findings_frame()                # Searchable index of the selected findings
report.coverage                        # Completed, sampled, capped, or skipped work
report.data                            # Complete JSON-compatible report
report.save("profile.json")

report.verify(df)                      # Recount statements through Polars
report.rows("f1", df)                  # The rows behind a statement (short handle or full ID)
report.to_checks()                     # Exact statements as constraints for new data
```

For agent/LLM context windows, `report.to_brief(2_500)` (or
`report.to_digest(4_000, compact=True, include_schema=True)`) combines a
role-grouped schema+range strip with short `[f1]` handles resolvable via
`report.finding("f1")` and `report.rows("f1", df)`. For structured JSON,
`report.to_json(compact=True)` trims boilerplate coverage and oversized
repeated payloads, and `report.to_json(max_chars=24_000)` returns a
size-budgeted view (raising `ValueError` if the requested size cannot hold
its minimal index). Both keep the full report available. `report.diff(other)`
compares two reports by finding ID, records coverage-status changes, and
compares the column cards — columns added, removed, or retyped, and the
card quantities that moved; it is separate from the statistical drift
detector. All digest/brief rendering, JSON compacting/budgeting, lookup,
filtering, diffing, `infer_spec`, and `profile_many` execute in Rust.

Start with the [quickstart](docs/book/usage/quickstart.ipynb), then follow
[reading findings](docs/book/usage/findings.ipynb) and
[checking evidence](docs/book/methodology/evidence.ipynb). The
[API reference](docs/book/usage/api.ipynb) and
[JSON contract](docs/report-schema.md) provide lookup details.

## Choose the work and declare what you know

`onceover.RECOMMENDED_DETECTORS` lists the recommended detector IDs and
appears in the `profile()` signature. Pass a list of registered IDs to
choose a different set for one call.

| Budget | Work |
|---|---|
| `quick` | All-row column summaries and integrity checks; no model-based relationship search |
| `standard` | The default detectors with bounded relationship samples and search caps |
| `deep` | Larger samples and wider searches for those detectors |

Use `detectors=[...]` to replace the default selection with named methods.
Required resource passes may also run; coverage explains that work.
The recommended set includes 31 of the 47 registered detectors. To
request the whole registry, pass `detectors=onceover.ALL_DETECTORS`.
Budget, required hints, target, and data eligibility still apply.
Designating `target="response"` also engages the five target-gated
passes with the default selection. With an explicit list, include any
target passes you need. [Capability guide](docs/book/usage/capabilities.ipynb)

Hints declare intended structure, including ordering, complete sequences,
calendar cadence, and panel dimensions. For example:

```python
import datetime as dt

monthly = pl.DataFrame({
    "month": [dt.date(2024, m, 1) for m in (1, 2, 4, 5, 6)],
    "revenue": [10.0, 11.0, 13.0, 12.0, 14.0],
})
hinted = onceover.profile(monthly, hints={
    "time": "month",
    "no_gaps": ["month"],
    "cadence": {"month": "monthly"},
})
for finding in hinted.select(category="gaps"):
    print(finding["claim"])
```

This checks for the missing March against a declared monthly calendar.
Unusable hints are explained in coverage. See
[declared expectations](docs/book/methodology/expectations.ipynb) for
ordering, integer sequences, and panel completeness.

Results are deterministic for the same package version, data, and settings
when neither a deadline nor timing is enabled. `deadline_seconds` is a
soft limit checked between passes and at work quanta inside five
searches — the dependence screen, the hotspots scan, the discontinuities
pass, the surprises scan, and the duplicate-encodings scan — which can
stop mid-pass; their coverage entries state how much of the search was
examined. The on-request entity fingerprints scan checks the deadline
between views and drift boundaries. The surprises scan also stops at a deterministic work budget
over row visits and bitset words; a budget stop marks only that pass
`capped`, with the contexts and targets examined, and does not suppress
later passes.
`timing=True` records per-pass elapsed time without changing the findings.

## Large and wide tables

Column summaries inspect every row. Dense and nullable Int64, UInt64, and
Float64 columns borrow Arrow buffers; other dtypes may require normalized
copies. Exact counts preserve unsigned integer values and timestamp
resolution. Numeric summary statistics and model features use
floating-point arithmetic.

Relationship discovery screens broadly, then limits follow-up work.
`sample_rows`, `max_predictors`, and `max_pairs` adjust specific search
budgets; they do not reduce full-data integrity checks. Coverage identifies
the rows and limits behind each result. Rare patterns and relationships
visible only through interactions can escape a shortlist.

The [large-table guide](docs/book/usage/large_tables.ipynb) explains those
tradeoffs. The [scale benchmark](docs/benchmarks/large-table-performance.md)
reports runtime, memory, and recovery together on 2,000,000 rows × 200
columns, with planted relationships beyond column 64; the
[2-core / 8 GiB overhaul record](docs/benchmarks/2026-11-22-architecture-performance-precision.md)
documents 200,000 × 200 profiling in 17.2s at 1.05 GiB peak RSS on 2 threads
alongside the precision gate reductions.

## Rust and development

All core profiling (`profile`, `profile_many`, `infer_spec`,
`partition_scagnostics`) and report operations (`to_brief`, `to_digest`,
`to_json`, `to_json_compact`, `to_json_budgeted`, `finding`, `diff`) live
in Rust (Polars 0.55), keeping Python a thin wrapper and making other
language bindings straightforward:

```rust
use onceover::{on_request_detector_ids, profile, ProfileConfig};
use polars::prelude::*;

fn main() -> PolarsResult<()> {
    let df = df!("id" => [1i64, 2, 3], "value" => [4.0, 5.0, 6.0])?;
    let report = profile(&df, &ProfileConfig::default());
    let on_req = on_request_detector_ids();
    let on_req_refs: Vec<&str> = on_req.iter().map(String::as_str).collect();
    println!("{}", report.to_brief(2_500, &on_req_refs));
    println!("{}", report.to_json_compact());
    Ok(())
}
```

Install with `cargo add onceover polars@0.55`. For a Python development
checkout, activate a virtual environment and run:

```bash
pip install 'maturin>=1.15,<2' pytest scipy
maturin develop --release
cargo test --workspace --locked
python -m pytest tests/
```

The [book](https://fcela.github.io/onceover/) includes case studies,
methodology, implementation notes, and research references.
[RELEASING.md](RELEASING.md) describes publication. The package is licensed
under [MIT](LICENSE).
