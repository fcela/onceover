# Structural correctness audit of v0.1.6

Audited main at `7d27ac8` after a fast-forward pull on 2026-11-17.
This is an audit, not a detector change. Production code is unchanged.
The local pre-existing `experiments/` directory was left alone.

## Method and validation

Built the current native extension in release mode and ran the current
Python source against it. Python 3.13, macOS arm64. Reproduce the 30
structural probes with:

```sh
source .venv/bin/activate
maturin develop --release
PYTHONPATH=python python scripts/audit_structure.py > /tmp/onceover-structure-audit.json
PYTHONPATH=python python -m pytest tests/ -q
cargo test --workspace
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --check
PYTHONPATH=python python benchmarks/corpus/run_benchmarks.py --json-out /tmp/onceover-corpus-audit.json
```

The first build succeeded; maturin's uv-based installation then hit a local
macOS system-configuration panic. Probes, pytest and corpus runs explicitly
used `PYTHONPATH=python` with the freshly built `python/onceover/_native.abi3.so`,
not the previously installed package. This installation failure is an
environment observation, not a onceover correctness finding. A wheel
was subsequently built and installed with `python -m pip`, replacing
the environment's old 0.1.0 installation with 0.1.6. All 30 probes
were repeated against the installed wheel: findings, column cards,
coverage and grouped findings match the source-build results exactly.

Results: **775 Python tests and 576 Rust tests pass**. Strict Clippy,
formatting and release metadata checks pass. The corpus runs 32 datasets,
skips 25 absent datasets, and passes **220/220 machine expectations**.
Its reviewed cases separately record 17 found, 11 missed and 4 partial;
the harness correctly exits 2 for those documented misses.

Compact observations and corpus summary are committed in
`benchmarks/results/structure-audit-2026-11-17.json`. The reproduction
script emits full findings, column cards and coverage. Timings there are
single-run observations under concurrent test/build load, not performance
comparisons. No before/after detector benchmark applies: no detector changed.

## Confirmed findings, in repair priority order

### 1. P1 — Decimal conversion corrupts exact integrity statements

Source: `crates/onceover-core/src/typed.rs:567–583` converts Decimal to
`TypedCol::F64` before downstream exact-value operations.

Probe `decimal_exact_integrity`: 100 Decimal rows cycling through
`2**60 + {0,1,2,3}`. Polars counts four distinct values, each appearing
25 times. Quick profiling instead reports one distinct value, labels
the column constant, recommends `drop_candidate`, reports **99 redundant
rows instead of 96**, and says the repeating sequence is non-decreasing.
`report.verify(df)` returns mismatches for the duplicate count and ordering;
the constant finding is `not_checkable`.

Preserve Decimal's exact scaled integer representation for equality,
distinctness, duplicate detection and ordering. A floating projection is
appropriate only for approximate numeric modeling. Regression checks must
compare these exact quantities with native Polars, including high-scale
Decimals near ordinary magnitudes, not only large scale-zero values.

### 2. P1 — Views turns distinct integers into false 100% modal claims

Sources: `crates/onceover-mines/src/views.rs:213–227` casts integers to
f64; modal counting at `1530–1551` uses floating standardized values.

Probe `integer_precision`: two identical columns cycle through four values
over 6,000 rows. At offset zero, views finds their dependence and four
clusters. Adding `2**60` to each value loses the dependence and produces
two claims that **6,000/6,000 rows hold one value**, with facet
`support=exact`. The actual modal count is 1,500 and share 25%.

Preserve raw integer identity for counts and emitted values. Center integer
differences before approximate conversion where possible, and disclose or
reject model inputs whose meaningful variation cannot be represented.

### 3. P2 — Pooled categorical tails are stated as literal values

Sources: `views.rs:525–566` pools excess levels into `other`;
`1553–1566` renders that internal label as an observed modal value.

Probe `pooled_category_not_real_value`: 6,400 rows equally distributed over
64 strings, `level_00` through `level_63`. Output says **3,200/3,200 rows
hold x at "other"**. There are zero rows with that string. The bucket
contains 32 distinct original values. The finding again has exact support.

Represent a pooled set explicitly in evidence and claim text; never use a
synthetic bucket label as an exact observed value. Preserve the distinction
even when a genuine `other` value also exists.

### 4. P2 — Modal percentages use a different denominator from the claim

Source: `views.rs:1542–1551, 1570–1595` divides by non-null observations,
but renders the percentage as a share of the whole cluster.

Probe `modal_share_missing_denominator`: 100 ones and 900 nulls. Output:
**"100 of 1000 rows ... hold x at 1.0 (100% of the cluster...)"**.
The whole-cluster share is 10%; 100% is the share of non-null rows only.

Emit both denominators and use the correct wording. The single-cluster
and multi-cluster claim paths both need this correction.

### 5. P2 — Prefix-only screening drops an obvious late-valued pair

Source: `views.rs:400` takes the first 256 search positions for column
screening, rather than a sample spanning the search rows. Ties at
`445–447` favor table order.

Probes `leading_null_dependent_pair_wide_False/True`: two identical
6,000-row Gaussian columns have nulls in their first 1,000 rows. Alone,
they form a dependence block (merge gain 136.8 nats). Add 64 constant
columns before them, and screening retains all constants and drops both
dependent columns. The prefix contains no observed values of the pair.

Screen across the full row range, avoid spending column capacity on
constants, and test invariance to irrelevant constant columns and leading
null runs. Coverage names the dropped columns, but still says `ran_exact`.

### 6. P2 — A fixed 2,000-row search is hidden behind full-sample coverage

Sources: `views.rs:60, 1139, 1226, 1345`; `config.rs:317`;
`pipeline.rs:121–127, 2545–2551`.

Both standard and deep use `search_rows=2000`. `sample_rows=` changes
the scoring row cap, not this search cap. With 6,000 rows, a report says
`ran_exact` and "sample 6000 of 6000 rows" while view selection and split
decisions used 2,000. Evidence records the configured search cap, not the
actual search count: a 50-row sample still says `search_rows: 2000`.

This is a search limit, not a 2,000-row limit on all scoring: the later
assignment/scoring stage really does visit the larger sample. Evidence
and coverage should distinguish those stages and their actual row counts.
Expose a search control or tie it to the chosen budget, with a work bound.

### 7. P2 — Merge-attempt exhaustion is not reported as capped

Sources: `views.rs:952–955` stops after `tried_merges_cap=256`;
`ViewsOutcome.budget_hit` only reflects the separate cell-work budget;
pipeline coverage inspects that flag and the scoring row cap.

Probe `merge_attempt_cap`: 30 independent Gaussian columns, 600 rows,
435 candidate pairs. No columns merge, so the merge loop hits 256
attempts and leaves 179 pairs unattempted. Coverage reports `ran_exact`,
30 singleton views, and no search-limit disclosure. This is different
from a clean exhaustive negative.

Track attempted/available merges and column truncation separately from
cell-budget exhaustion; report incomplete search as capped with counts.

### 8. P2 — Foreign-key chance scores lose all signal for wide integer spans

Source: `python/onceover/_many.py:87–96` subtracts nearly equal large
`lgamma` results to calculate log binomial coefficients.

For two tables with unique key values `{0, 2**60}`, renamed to make
the candidate weak-tier, `profile_many()` correctly finds containment
but reports **chance_score=1.0**. Under its own integer-span model the
probability is `1 / choose(2**60 + 1, 2)`, about **1.5046e-36**.
Directly, `_log_choose(2**60, 2)` returns 0 rather than about 82.48.

Use stable log-products for small `min(k,n-k)` and an accurate method
for the larger case. Compare with exact integer binomial coefficients
on a range of spans; test scores and ordering, not just containment.

## What the audit does and does not establish

The small clean-input controls do recover identical Gaussian columns,
affine copies and identical binary columns in views. Default profiling
also recovers the planted affine/arithmetic identities and binary-label
dependency. Independent Gaussian, binary and exact Cartesian-product
controls produce no multi-column dependence blocks in these probes.
The claim that all simple structure fails would be too broad.

The confirmed failures concern exact-value preservation, the meaning of
reported claims, and incomplete-search disclosure. Increasing the
2,000-row limit alone does not repair them. Existing green tests and
corpus expectations do not cover these counterexamples. Repair exact
claims first, then search/coverage behavior, and add these cases to the
regression and corpus gates before treating a green gate as evidence
that these specific problems are fixed.

This audit did not exhaustively validate all 43 detectors, download the
25 missing corpus datasets, reproduce the user's original dataset, or
measure statistical error rates over many seeds. The findings above are
reproducible counterexamples, not a completeness claim about the audit.
