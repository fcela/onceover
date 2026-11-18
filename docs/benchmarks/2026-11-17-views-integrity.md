# Views: exact claims and disclosed search limits (#365)

Base: `a8debce` (the Decimal fix, PR #368), on main `7d27ac8` / v0.1.6.
Python 3.13, macOS arm64; release extension, explicit source `PYTHONPATH`.
This addresses issue #365 items 2–7; foreign-key scoring is PR #366.

## Changes and counterexample results

`scripts/audit_structure.py` records full findings, cards and coverage for
30 probes. Run it on the base and changed builds with
`PYTHONPATH=python python scripts/audit_structure.py > result.json`.
Selected before/after records are in
`benchmarks/results/365-views-regressions.json`.

| Counterexample | Before | After |
|---|---|---|
| Two identical columns cycling over four integers near 2**60 | Dependence lost; false 6,000/6,000 modal count | Dependence recovered, four 1,500-row clusters, exact integer values |
| 64 equally frequent categorical levels | 3,200 rows said to hold a nonexistent literal `other` | A pooled set of 32 levels, identified by its count and retained-level exclusions |
| 100 ones and 900 nulls | 100/1,000 described as 100% of the cluster | 100/100 non-null observations, with 1,000 total rows stated separately |
| Identical columns after 1,000 leading nulls, preceded by 64 constants | Both signals dropped | Signals retained; two constants dropped and coverage capped |
| Public `sample_rows=50` | Search metric said 2,000 | Search and scoring both use 50; both actual counts stated |
| 6,000-row full sample | 2,000 search rows, hidden by `ran_exact` | 6,000 search/scoring rows |
| 30 independent columns, 600 rows | 256 of 435 possible merge attempts, `ran_exact` | 435 completed pair screens, 256 merge attempts, explicitly capped |

Integer, unsigned integer, Decimal and datetime models center values in
their exact integer domain before conversion. Modal counts always compare
original source values, including signed-zero canonicalization, and use
exact JSON integers or exact Decimal strings for values. Approximate
component means remain numeric projections, not exact-value evidence.

Pooled modal values now carry an object with `pooled`, `level_count` and
`excluding`; literal modal values retain scalars. Both single-cluster and
multi-cluster payloads state `modal_n`, `nonnull_n` and `cluster_n`.
These are detector-specific open metrics, with no schema-version change.

## Sampling and search accounting

The Python public sample control now governs search as well as scoring:
20,000 rows at standard, 50,000 at deep, overridden by `sample_rows`.
This removes the separate unchangeable 2,000-row bottleneck. It makes
the existing public sample contract consistent; it is not a measurement
that these limits are statistically optimal, or a resolution of the
general recall question in #360. Larger samples can consume more work
before hitting the unchanged 300-million split-cell budget.

The width screen samples 256 positions across the full search range,
prefers varying columns over constants on score ties, and uses a ceiling
stride to actually stay within its 250,000-pair bound. Coverage states its
row and visited-pair counts. The partition search counts fully completed
pair screens and merge attempts separately; merge-cap exhaustion, column
drops, work exhaustion and a smaller Rust-configured search sample all
produce capped coverage. A merge cap is reported only if an unattempted
cross-view pair remains, not when remaining pairs already share a view.

## Corpus comparison

Commands: activated-venv release build, then
`PYTHONPATH=python python benchmarks/corpus/run_benchmarks.py --json-out PATH`
before and after. Both runs cover 32 datasets and skip 25 absent datasets.
The first changed run passed 216/220 expectations; **all four failures
were old `ran_exact` coverage pins**, with no finding expectation failure:

| Dataset | Measured reason for capped coverage |
|---|---|
| hospital-dha | 2,016 pairs screened; 256 merge attempts |
| state-unemployment | 1,326 pairs screened; 256 merge attempts |
| gse15258-expression | 64/5,006 columns kept; 245,638/12,527,515 width-screen pairs visited; 256 merge attempts |
| lace-animals | 64/86 columns kept; 256 merge attempts |

Only those four status pins and their explanatory notes were updated.
The final run passes **220/220** machine expectations. Reviewed cases
remain 17 found, 11 missed and 4 partial; exit 2 continues to disclose the
documented misses. No known-miss expectation was removed or weakened.

## Other checks

- **797 Python tests passed**, including 15 new public-API regressions.
- **577 Rust tests passed**, including a new search/work accounting test.
- Strict Clippy and formatting passed.
- Both changed methodology notebooks were re-executed and saved
  (four code cells in the walkthrough, one in the reference).
- The strict executed book produced all 61 pages and static HTML.

Commands: `PYTHONPATH=python python -m pytest tests/ -q`,
`cargo test --workspace`, strict all-target Clippy, `cargo fmt --check`,
`scripts/check_notebook.py --notebook PATH --write`, and
`bash docs/book/build-html.sh`.

As in the Decimal validation, maturin's compile succeeded but its uv
installer hit the local macOS system-configuration panic. Source checks
used the freshly built extension through `PYTHONPATH`. Jupyter execution
needed local kernel-port access. MyST 1.10.1 needed its recommended
compatible theme 1.4.1 temporarily selected; the tracked theme config
was restored afterward. The record measures correctness and coverage,
not comparative runtime under the concurrent test/build load.
