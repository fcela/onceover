# Benchmarks

The benchmarks exercise profiling cost and result quality together. The
synthetic scale fixture tests exact counts and recovery of known
relationships on large, wide tables. The corpus checks documented behavior
on real datasets.

| Benchmark | What it measures | Documentation |
|---|---|---|
| Large-table fixture | Runtime, resident memory, exact cardinalities, and relationships beyond column 64 | [Protocol and recorded results](../docs/benchmarks/large-table-performance.md) |
| Known-answer corpus | Findings and coverage against per-dataset expectations | [Corpus guide](corpus/README.md) |
| CI timing gate | Large runtime regressions on two smaller synthetic tables | [Timing gate](timing_gate.py) |

## Large tables

Run from the repository root against the installed package:

```bash
python -m pip install '.[benchmark]'
python benchmarks/scale.py --rows 2000000 --columns 200 --threads 4 \
  --output /tmp/onceover-scale.json
```

The fixed seed plants an affine identity, a nonlinear response, and a
category-to-label dependency in the final columns. The script fails if
recovery or exact cardinality checks fail. Its JSON output records package
versions, search coverage, timing, and sampled process RSS. See the
[benchmark documentation](../docs/benchmarks/large-table-performance.md)
for the workload, measurement boundaries, comparison caveats, and results.

## Real datasets

```bash
python benchmarks/corpus/run_benchmarks.py --pack gritbot
```

The [registry](corpus/registry.json) records each dataset's source, license,
preprocessing, and expectations. Bundled data is hash-verified before
profiling. Test-only datasets run when their prepared CSV exists locally;
otherwise the runner reports why they were skipped.

Every dataset has attribution and a preparation script. Preparation decodes
or renames source fields; it does not drop rows, impute values, or remove
outliers. The [corpus guide](corpus/README.md) explains the redistribution
policy, expectation types, and known-miss checks.
