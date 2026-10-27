# onceover

Profile a Polars DataFrame with one call. `onceover` returns column summaries,
ranked findings with evidence, and coverage describing what each pass examined.
The computation runs in Rust; Python bindings release the GIL while profiling.

**Documentation:** the full book — tutorials, case studies, per-pass
methodology, implementation notes, and the report contract — is built from
this repository and published at
[https://fcela.github.io/onceover/](https://fcela.github.io/onceover/).
The [analysis notebook](notebooks/onceover_analysis.ipynb) is a complete
worked example with charts and controls.

## Install

```bash
pip install onceover
```

Requires CPython 3.10+ and Polars 2.x. Binary wheels cover Linux x86_64 and
aarch64, macOS Intel and Apple silicon, and Windows x86_64. Building the
source distribution requires Rust and a C compiler.

## Python

```python
import polars as pl
import onceover

df = pl.DataFrame({
    "order_id": [1, 2, 3, 4],
    "quantity": [2, 1, 4, 3],
    "double_quantity": [4, 2, 8, 6],
})

report = onceover.profile(df)
print(report.overview["headline"])
print(report.to_markdown())

report.columns     # Per-column types, roles, counts, and statistics
report.findings    # Selected statements with supporting evidence
report.coverage    # Pass status, samples, caps, and skipped work
report.groups      # Connected findings, without a redundancy claim
report.data        # Complete JSON-compatible report
report.save("profile.json")
```

`profile(df, budget="standard", deadline_seconds=None)` accepts three budgets:

| Budget | Work |
|---|---|
| `quick` | Column summaries, single-column keys, duplicates, dates, and sortedness |
| `standard` | Default passes with bounded search and model samples |
| `deep` | Default passes with larger search caps and model samples |

Results are deterministic for the same data and configuration. An optional
wall-clock deadline can change which passes finish; coverage records skipped work.

Use `detectors=[...]` to select passes explicitly. The experimental `views`
pass is disabled by default and can be selected at the standard or deep budget:

```python
onceover.available_detectors()
report = onceover.profile(df, detectors=["keys", "numeric_dependence", "views"])
```

An explicit list replaces the default selection. Required passes may also run
to supply resources; coverage identifies that work.

## Findings

- Primary, composite, and nearly unique keys; duplicated rows.
- Exact and approximate functional dependencies, with violation counts.
- Predictable columns, cross-validated decision tables, and PRISM rules.
- Pearson, Spearman, and Chatterjee's ξ dependence; redundant column groups.
- Arithmetic identities, M5-style local linear regimes, and dated residual shifts.
- Nominal associations with training and diagnostic holdout evidence.
- Date gaps, irregular intervals, sortedness, constants, missing values,
  concentration, numeric category codes, and inconsistent string formats.

Finding categories describe the discovered pattern, while contributing methods
are recorded in `evidence`. Scope, measurement basis, and validation are separate
fields. Equivalent statements combine evidence; shared selection limits apply
across detectors. See [the report schema](https://github.com/fcela/onceover/blob/main/docs/findings.md).
Model explanations and associations use diagnostic validation; they do not
establish causal effects or multiple-testing guarantees.

## Rust

```bash
cargo add onceover polars@0.55
```

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

The Rust API uses Polars 0.55. The `onceover` crate re-exports report types.

## Large and wide tables

The profiler accepts in-memory DataFrames with millions of rows and hundreds of
columns. Column summaries examine every row and retain exact cardinality,
null counts, distributions, and quantiles. Dense and nullable Int64/Float64
columns borrow Arrow buffers; other dtypes can require normalized copies.
This is an in-memory profiler, so input size and exact statistics still affect
memory and runtime.

Relationship discovery uses broad screening followed by bounded follow-up work:

| Work | `standard` | `deep` |
|---|---|---|
| Encoded relationship sample | 20,000 rows, all usable columns | 50,000 rows, all usable columns |
| Predictability | Every eligible target; up to 48 shortlisted predictors and 5,000 fitting rows per target | Up to 64 predictors and 10,000 fitting rows |
| Numeric dependence | All pairs screened; up to 496 pairs examined over 20,000 rows | Up to 2,016 pairs over 50,000 rows |
| Functional dependencies | Every eligible determinant/target screened; up to 16 determinants per target, verified over 200,000 rows | Up to 32 determinants, verified over 500,000 rows |
| Decision tables / PRISM | Up to six promising targets per pass | Up to ten targets per pass |

Predictor selection uses training rows. Nominal associations consider all eligible
nominal columns. Arithmetic identities are fitted on a sample and their residuals
are checked over all pair-complete input rows. Single-column key checks use exact
full-column cardinality. Higher-order searches, category levels, and explanation
output have additional budgets, described in `report.coverage`.

Samples use deterministic stratified row selection. They can miss rare patterns;
shortlisting can miss weak or interaction-only signals. Coverage records samples,
shortlists, and skipped work. Absence of a finding is not proof of independence.

Adjust the main relationship costs when needed:

```python
report = onceover.profile(
    df,
    sample_rows=50_000,
    max_predictors=96,
    max_pairs=2_000,
)
```

These overrides keep exact column summaries and integrity checks on the full
input. A deadline is soft: it skips later passes without interrupting a pass
already running.

See the [analysis notebook](https://github.com/fcela/onceover/blob/main/notebooks/onceover_analysis.ipynb) for planted examples,
Altair charts, evidence, coverage, and a wide-table demonstration. Install its
optional dependencies with `pip install 'onceover[notebook]'`.
The [scale benchmark](https://github.com/fcela/onceover/blob/main/benchmarks/scale.py) measures runtime, peak resident memory,
and recovery of relationships planted in the final columns:

```bash
pip install 'onceover[benchmark]'
python benchmarks/scale.py --rows 2000000 --columns 200 --output /tmp/onceover-scale.json
```

## Development

```bash
python -m venv .venv
source .venv/bin/activate
pip install 'maturin>=1.15,<2' pytest scipy
maturin develop --release
cargo test --workspace --locked
pytest tests/
```

Publishing setup is described in [RELEASING.md](https://github.com/fcela/onceover/blob/main/RELEASING.md).

## License

MIT. See [LICENSE](https://github.com/fcela/onceover/blob/main/LICENSE).
