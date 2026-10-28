# Context-budgeted reporting: prototype and measurements (2026-10-28)

Issue #27. Prototype: `benchmarks/corpus/digest.py` — takes a
profile report and emits a plain-text digest under a character
budget, with a stated degradation order: full rolled-up claims,
then as many as fit with an explicit omission line, then category
counts, then the header alone. Near-duplicate findings (same
category, type, form, and target) roll up into one line with a
count, keeping the highest-interestingness claim as the example.

## Measured on corpus reports (default detectors, standard budget)

| table | full report JSON | digest @ 4,000 chars |
|---|---|---|
| gse15258-expression | 4,342,755 chars | 673 |
| hospital-dha | 255,294 | 3,771 |
| thyroid | 115,008 | 3,979 |

The GSE number deserves a sentence of its own: the report holds 25
findings, so the 4.3 MB is almost entirely *coverage detail* — the
views and dependence passes enumerate thousands of column names in
their detail strings (the views detail alone was measured at 63 KB
in the CrossCat entry, naming 4,942 dropped columns). A consumer
with a context budget pays for those strings whether or not it
reads findings. The digest's coverage line reduces each pass to
its status, which loses the dropped-column list; that list is
exactly what the retrieval mode below is for.

## What the issue's full design still needs (recorded, not built here)

- Retrieval by finding id / category / column from a stored report,
  so a digest reader can pull the one finding (or the one coverage
  detail) it needs instead of the whole JSON.
- A diff mode between two runs' reports (the corpus runner's run
  records are the input shape for this).
- The digest as a library surface (`report.to_digest(chars)` next
  to `to_json` / `to_markdown`), with the schema version in the
  header — the prototype prints it; the library version should be
  specified in the report-schema doc (issue #28).
