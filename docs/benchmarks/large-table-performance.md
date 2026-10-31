# Large-table performance and relationship recovery

On the two-million-row benchmark, onceover 0.1.3 profiled 200 columns in
79.67 seconds with a peak process RSS of 3.47 GiB. It recovered all planted
relationships, including those beyond column 64, and passed the exact
cardinality and key checks.

The benchmark measures two things together: the cost of profiling a large
table and whether bounded relationship searches still reach useful signals
at the end of a wide table. Runtime alone would not establish that the
profiler examined the intended data.

## Workload

`benchmarks/scale.py` generates a fixed-seed table with 2,000,000 rows and
200 columns. Most columns are independent numeric noise. The remaining
columns include an integer primary key, a key with one repeated value, and
three planted relationships in the final columns:

- an affine identity, `late_affine = 3 * late_x + 2`;
- a nonlinear response, `late_curve = late_x ** 2`;
- a categorical lookup, `late_code → late_label`.

The script exits unsuccessfully if any of its eight checks fails. These
cover the column-card count, both exact cardinalities, primary and dirty
key recovery, and all three late-column relationships. It also records how
many independent noise columns are reported as predictable targets.

## Recorded measurements

Both runs used release wheels, the standard budget, Python 3.13.7,
Polars 2.0.0, four Rayon threads, and the same macOS arm64 host with 24 GiB
of system memory. The fixture seed is `20241024`. Measurements were
recorded on 2026-10-31.

| Measurement | 0.1.2 | 0.1.3 |
|---|---:|---:|
| Profiling wall time | 351.08 s | 79.67 s |
| Peak process RSS during profiling | 5.84 GiB | 3.47 GiB |
| Increase from pre-profile RSS | 2.77 GiB | 0.40 GiB |
| Recovery and cardinality checks passed | 8 / 8 | 8 / 8 |
| Independent noise targets reported predictable | 0 | 0 |

Timing covers the `onceover.profile` call after the DataFrame has been
constructed. RSS is sampled every 20 ms during profiling and includes the
input DataFrame and Python process. The increase from pre-profile RSS is
a change in resident process memory, not an allocation count for profiler
scratch space. Data generation and subsequent Python report materialization
are outside the timed phase.

The 0.1.2 run briefly shared the host with other validation tasks; the
0.1.3 run started after those tasks completed. These are single-run
observations, so the comparison does not establish a universal speedup or
an estimate of run-to-run variance. Both versions ran their full default
detector set. The older 0.1.1 record in this repository predates several
default detectors and is not a comparable runtime baseline.

The complete machine-readable records retain versions, platform, sampling
settings, coverage, selection counts, and individual check outcomes:
[0.1.2](../../benchmarks/results/2m-200-0.1.2.json) and
[0.1.3](../../benchmarks/results/2m-200-0.1.3.json).

## Reproducing the benchmark

From a checkout of the version to measure, install its package and benchmark
dependencies, then run the fixture in a fresh process:

```bash
python -m pip install '.[benchmark]'
python benchmarks/scale.py --rows 2000000 --columns 200 --threads 4 \
  --output /tmp/onceover-scale.json
```

Use `--budget quick` for full-column summaries and integrity checks, or
`--budget deep` for larger relationship-search budgets. Compare runs with
the same budget, fixture dimensions, thread count, and package versions;
different budgets perform different work.

## Interpreting the result

In 0.1.3, discontinuity detection borrows numeric values and selects its
complete-case samples using validity bitmaps. Its pair-data cache is
bounded independently of the sort-order cache. Sentinel detection reuses
exact frequencies to exclude values below its occurrence floor, while
surprise detection reuses category encodings and sorted value orders.
The anomaly scans retain all input rows, and the discontinuity sample is
the same deterministic complete-case sample used previously.

Passing this fixture demonstrates recovery of these particular strong
signals. It does not measure recall for rare relationships, interactions
without useful individual predictors, or every distribution encountered
in practice. Zero predictable noise targets is an outcome for this fixture,
not a false-positive guarantee for every detector. Consult each report's
coverage and evidence scope when interpreting an actual table.
