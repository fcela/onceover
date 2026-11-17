# Exact Decimal integrity (#365)

Base: main `7d27ac8`, v0.1.6. Python 3.13, macOS arm64; release build.

Decimal ingestion previously discarded the exact scaled integer and
kept only f64. Distinctness, duplicate-row verification, canonical
dependency encoding and sortedness then operated on rounded values.
The new Decimal representation retains its i128 values and scale.
Statistical models explicitly request the numeric projection; column
roles, numeric summaries and existing affine detection remain available.
Sentinel and boundary counting retain exact i128 keys as well.

## Counterexample protocol

Compare profiler output with native Polars on a 100-row Decimal column
cycling through `2**60 + {0,1,2,3}`, then on four values separated by
1e-18 near 1. Both have four distinct values, 25 occurrences per value,
96 redundant rows, and descending seams between repetitions.

Before: one distinct value, a constant/drop-candidate recommendation,
99 redundant rows, and a false non-decreasing claim. `verify()` reported
duplicate-count and sortedness mismatches.

After: four distinct values, exact top-value strings and counts, 96
redundant rows, no constant or sortedness claim, and no verification
mismatch. Parameterized regressions also cover negatives, nulls, a
strictly increasing unique Decimal key, exact dependency verification,
and a Decimal near 1 that must not round into a binary flag.

## Validation

- Dtype regressions: 40 passed, including seven new tests.
- Full Python suite: 782 passed.
- Rust workspace: 576 passed; strict Clippy and formatting pass.
- Corpus before/after: 32 datasets pass, 25 absent datasets skipped;
  **220/220 machine expectations pass on both builds**. Reviewed
  cases remain 17 found, 11 missed and 4 partial (harness exit 2).
- Strict executed book: 61 pages built with static HTML produced.

Commands: activated-venv `maturin develop --release`,
`PYTHONPATH=python python -m pytest tests/ -q`, `cargo test --workspace`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo fmt --check`, corpus `run_benchmarks.py --json-out PATH`, and
`bash docs/book/build-html.sh`.

Maturin compiled successfully but its uv installer hit the environment's
macOS system-configuration panic. Checks used the freshly built extension
in `python/onceover` through explicit `PYTHONPATH`. Book execution used
the same source. MyST 1.10.1 needed its recommended compatible theme 1.4.1
selected temporarily; the unpinned latest theme is incompatible with
that engine. The old generated cache was preserved outside the repository
and a clean book build was used after a stale favicon route failed.
The tracked theme configuration was restored after building.

The extraction chapter now describes exact Decimal identity separately
from its approximate numeric projection. No schema version or field is
added by this change. This record measures correctness, not performance.
