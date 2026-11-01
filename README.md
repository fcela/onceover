# onceover

Give a Polars DataFrame a first look: find data-quality problems, discover
relationships between columns, and locate changes in how the data behaves.
`onceover.profile(df)` returns ranked findings with their evidence, a
summary of every column, and coverage showing what was examined.
Computation runs in Rust, with Python and Rust APIs.

[Read the documentation](https://fcela.github.io/onceover/) ·
[Explore the capabilities](docs/book/usage/capabilities.ipynb) ·
[Work through the analysis notebook](notebooks/onceover_analysis.ipynb)

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
| How are variables related? | Linear, monotone, and nonlinear dependence; changing associations by period; target shapes and predictor interactions | [Dependence measures](docs/book/methodology/dependence_measures.ipynb), [target shapes](docs/book/methodology/target_shapes.ipynb), [interactions](docs/book/methodology/interactions.ipynb) |
| Where does behavior change? | Calendar gaps, appended backfills, sustained level/trend/variance changes, temporary excursions, threshold jumps, model-routing drift | [Temporal analysis](docs/book/methodology/temporal_analysis.ipynb), [discontinuities](docs/book/methodology/step_discontinuities.ipynb), [drift](docs/book/methodology/drift.ipynb) |
| Does the layout hide structure? | Verified long-format layouts and relationships in their wide form; shared row states across column groups | [Long-format reports](docs/book/usage/findings.ipynb), [experimental views](docs/book/methodology/view_clusters.ipynb) |

The [capability guide](docs/book/usage/capabilities.ipynb) connects these
questions to detector IDs, examples, and the methodological chapters.
Each method documents the statement it supports, its assumptions, and what
it can miss. Association and predictive findings describe observed
structure; they do not establish causation.

## Read and use the report

```python
report.columns                         # All-row summaries, keyed by column name
report.findings_frame()                 # Searchable index of the selected findings
report.select(column="region_name")    # Complete findings about one column
report.finding(report.findings[0]["id"])  # One finding, including all its evidence
report.coverage                        # Completed, sampled, capped, or skipped work
report.data                            # Complete JSON-compatible report
report.save("profile.json")
```

Use `report.to_digest(4_000)` for a short reading budget, or
`report.to_json(max_chars=24_000)` for a budgeted JSON view. The latter
raises `ValueError` if the requested size cannot hold its minimal index.
Both keep the full report available. `report.diff(other)` compares two
reports by finding ID and records coverage-status changes; it is separate
from the statistical drift detector.

Start with the [quickstart](docs/book/usage/quickstart.ipynb), then follow
[reading findings](docs/book/usage/findings.ipynb) and
[checking evidence](docs/book/methodology/evidence.ipynb). The
[API reference](docs/book/usage/api.ipynb) and
[JSON contract](docs/report-schema.md) provide lookup details.

## Choose the work and declare what you know

| Budget | Work |
|---|---|
| `quick` | All-row column summaries and integrity checks; no model-based relationship search |
| `standard` | The default detectors with bounded relationship samples and search caps |
| `deep` | Larger samples and wider searches for those detectors |

Use `detectors=[...]` to replace the default selection with named methods.
Required resource passes may also run; coverage explains that work. The
experimental `views` detector is opt-in. Designating `target="response"`
adds the opt-in target-shape pass when using the default detector selection;
with an explicit list, include `target_shapes` yourself.

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
soft limit checked between passes; a running pass completes.
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
columns, with planted relationships beyond column 64.

## Rust and development

The Rust API uses Polars 0.55:

```rust
use onceover::{profile, ProfileConfig};
use polars::prelude::*;

fn main() -> PolarsResult<()> {
    let df = df!("id" => [1i64, 2, 3], "value" => [4.0, 5.0, 6.0])?;
    let report = profile(&df, &ProfileConfig::default());
    println!("{}", report.to_json());
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
