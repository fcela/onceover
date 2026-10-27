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
| Profiling time | 5.68 s |
| Peak process RSS during profiling | 3.22 GiB |
| Increase from pre-profile RSS | 0.15 GiB |

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

## Views sensor benchmark

The opt-in views pass has a separate, self-contained sensor fixture:

```bash
python benchmarks/views.py --output /tmp/views-sensor.json \
  --report-dir /tmp/views-sensor-reports
```

The default `sensor-v1` fixture uses seed 20261027 and 50,000 rows. It contains
a temperature shift at row 25,000, 600 repeated temperature readings, 15
planted extremes, a device change, pressure measurements, and independent
humidity noise. It runs views with a 20,000-row fitting sample and with all
50,000 rows, repeating both runs to check report equality. It also profiles
2,000 independent Gaussian values as a control.

The [recorded result](results/views-sensor.json) includes the generator hash,
input hash, package versions, thread count, complete coverage, flagged row
indices, and per-cluster summaries. In that release-build run on macOS arm64:

| Views-only measurement | Standard sample | Full sample |
|---|---:|---:|
| Evaluated rows | 20,000 | 50,000 |
| Planted extremes in evaluated rows | 8 | 15 |
| Planted extremes reported | 8 | 15 |
| Other reported temperature cells | 1 | 0 |
| Profile time | 0.060 s | 0.121 s |
| Repeated report JSON identical | Yes | Yes |

The fitted grouping changes with the sample: the standard run joins `device_id`
and `temperature`, while the full-sample run has no shared views. The independent
Gaussian control forms eight clusters. Both observations matter when
interpreting a partition as meaningful structure.

To check the earlier boundary claim, the script also calculates a diagnostic
from the reported temperature clusters: sort their means, split at the largest
adjacent mean gap, and take the largest row index among the lower groups. This
gives 49,997 for the standard sample and 24,999 for the full sample, against a
shift planted at 25,000. The standard result fails to locate the shift. This
calculation is not part of the profiler; unordered cluster ranges do not supply
a temporal change-point estimate.

The script's pass/fail checks cover repeatability, default exclusion, and sample
accounting. Its recovery counts and cluster diagnostics describe this fixture;
they do not establish general detection accuracy. Views models only its first
64 eligible columns, so the profiler-wide late-column benchmark above does not
validate wide-table discovery by this pass.
