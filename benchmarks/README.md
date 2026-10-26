# Large-table validation

Run the benchmark in a fresh process from the repository root:

```bash
python -m pip install '.[benchmark]'
python benchmarks/scale.py --rows 2000000 --columns 200 --threads 4 \
  --output /tmp/onceover-scale.json
```

The fixed seed generates independent numeric noise, an integer primary key,
a key with one repeated value, and relationships in the final columns:
an affine identity, a nonlinear response, and a category-to-label dependency.
The script exits unsuccessfully if any recovery or cardinality check fails.

The [recorded run](results/2m-200-standard.json) uses 2,000,000 rows and 200
columns on macOS arm64 with 24 GiB system memory and four Rayon threads.
It records the package versions, sample sizes, pass coverage, wall time, and
process resident memory observed every 20 ms. Peak RSS includes the input
DataFrame and the Python process; the change from pre-profile RSS also includes
pages becoming resident and should not be interpreted as allocated profiler
scratch space. Compare complete configurations when comparing results.

| Recorded measurement | Value |
|---|---:|
| Profiling time | 5.78 s |
| Peak process RSS during profiling | 3.23 GiB |
| Increase from pre-profile RSS | 0.16 GiB |

All eight checks passed, including recovery of the late-column relationships.
No independent noise target was flagged as predictable in this fixture.
These are reproducible synthetic checks, not guarantees for every data
distribution. Exact column statistics still grow with input size; rare patterns
and interactions without strong individual predictors can escape sampled
screening and follow-up shortlists.

Use `--budget quick` to measure full-column summaries and integrity checks,
or `--budget deep` to exercise the larger relationship budgets. Regular CI
includes the smaller regression cases in `tests/test_scale.py`; this larger
memory test is run separately before publishing.
