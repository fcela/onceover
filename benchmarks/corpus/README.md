# Benchmark corpus

A corpus of real tables with documented behavior, profiled on every run
so that detector changes are measured against known answers rather than
impressions. Issues #20 (harness), #21 (business pack), #23 (GritBot pack).

## Layout

- `registry.json` — every dataset: pack, mode, license, source, shape,
  content hash (bundled, and for reference datasets), expectations, notes.
- `data/<pack>/<dataset>/` — `data.csv` (bundled datasets only),
  `ATTRIBUTION.md` (source, license, citation, preprocessing log),
  `prepare.py` (regenerates `data.csv` from the raw source).
- `run_benchmarks.py` — the runner (below).
- Results are written up in `docs/benchmarks/` (project convention:
  a benchmark that is not in the docs does not exist).

## License gate

The package is MIT. A dataset is bundled only when its terms are
compatible with redistribution inside an MIT-licensed package (public
domain, CC BY 4.0). Non-commercial, share-alike, no-derivatives, or
unclear terms make a dataset **test-only**: its `prepare.py` fetches
and prepares it locally, the registry records attribution and
expectations, and its CSV is never committed.

## Preprocessing standard

Decode and rename only: coded categoricals become the labels in the
dataset's own documentation, cryptic columns get plain names, dates
become ISO. No rows dropped, no values imputed, no outliers touched.
Every transformation is listed in the dataset's `ATTRIBUTION.md`, and
`prepare.py` implements exactly that list.

## Running

    python benchmarks/corpus/run_benchmarks.py [--pack gritbot|business]
                                               [--only <dataset-id>]

Bundled datasets are hash-verified against the registry before
profiling. Test-only datasets run when their `data.csv` exists locally
and are otherwise reported as skipped with the registry's reason.
Each run writes a JSON record under `runs/` (gitignored) and exits
nonzero if any "found" expectation misses, any known-miss tripwire
matches, or any coverage entry errors.

Expectations come in three kinds: `finding` predicates over the
report (category / type / form / target / columns), `documented_case`
records (published anomalies with a found / partial / missed
disposition, reviewed by hand each run), and `max_findings` guards.
A `finding` expectation with `"expect": "missed"` is a known-miss
tripwire: it passes while nothing matches, and fails loudly when a
future detector starts matching — the signal to promote the case.
