# onceover

Profile a Polars DataFrame with one call. `onceover` returns column summaries,
ranked findings with evidence, and coverage describing what each pass examined.
The computation runs in Rust; Python bindings release the GIL while profiling.

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
report.findings    # Ranked claims with evidence and confidence
report.coverage    # Pass status, samples, caps, and skipped work
report.data        # Complete JSON-compatible report
report.save("profile.json")
```

`profile(df, budget="standard", deadline_seconds=None)` accepts three budgets:

| Budget | Work |
|---|---|
| `quick` | Column summaries, single-column keys, duplicates, dates, and sortedness |
| `standard` | All passes with bounded search and model samples |
| `deep` | All passes with larger search caps and model samples |

Results are deterministic for the same data and configuration. An optional
wall-clock deadline can change which passes finish; coverage records skipped work.

## Findings

- Primary, composite, and nearly unique keys; duplicated rows.
- Exact and approximate functional dependencies, with violation counts.
- Predictable columns, cross-validated decision tables, and PRISM rules.
- Pearson, Spearman, and Chatterjee's ξ dependence; redundant column groups.
- Arithmetic identities, M5-style local linear regimes, and dated residual shifts.
- Nominal associations with training and diagnostic holdout evidence.
- Date gaps, irregular intervals, sortedness, constants, missing values,
  concentration, numeric category codes, and inconsistent string formats.

Findings distinguish exact, sampled, and heuristic evidence. Model explanations
and associations use diagnostic validation; they are not causal claims or
multiple-testing guarantees.

## Rust

```bash
cargo add onceover
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

## Limits

Standard model screening examines up to 20,000 rows and 48 encoded columns.
Decision tables and PRISM each select at most six eligible targets. Dependency,
association, and regime searches have separate caps, disclosed in coverage.
Column summaries and selected dependence pairs still examine full columns;
runtime and memory grow with table size even when models use samples.

## Development

```bash
python -m venv .venv
source .venv/bin/activate
pip install 'maturin>=1.15,<2' pytest scipy
maturin develop --release
cargo test --workspace --locked
pytest tests/
```

Publishing setup is described in [RELEASING.md](RELEASING.md).

## License

MIT. See [LICENSE](LICENSE).
