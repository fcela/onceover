# The digest as a library surface: report.to_digest(chars) (issue #35)

Date: 2026-10-28. Library change: `ProfileReport.to_digest(chars)`
in the Python package, promoting the corpus prototype
(`benchmarks/corpus/digest.py`, issue #27) without changing its
format: same header lines, same rollup key (category, type, form,
target), same strongest-claim-per-group ordering, same degradation
— rolled lines are dropped from the end and the digest closes by
stating how many groups were omitted. One guarantee the prototype
did not make: the result never exceeds the budget. At very small
budgets the header itself degrades (category counts, then coverage
line dropped) before any truncation.

## Measured, corpus reports at a 4,000-character budget

Reports produced with each dataset's registered detector
configuration, current library (including the #31 surprises pass
and the #32 excursion scan, so full-JSON sizes differ from the
prototype run):

| report | full JSON | digest | omissions stated |
|---|---|---|---|
| thyroid | 124,588 chars | 3,865 chars | yes |
| hospital-dha | 285,474 chars | 3,892 chars | yes |
| gse15258-expression | 9,649,216 chars | 1,514 chars | no — every group fits |

Prototype run for comparison (2026-10-28-context-digest.md):
thyroid 115,008 → 3,979; hospital 255,294 → 3,771; GSE
4,342,755 → 673. The GSE full JSON is larger now because the
registered configuration adds the views pass over 5,006 columns;
its digest stays small because rollups collapse the findings into
a handful of groups.

## Reading the digest

The digest is an index into the report, not a summary of record:
each line names a group and its strongest claim; the evidence,
the weaker group members, and any omitted groups live in the JSON
(`report.data`). Findings sharing a rollup key can differ in
columns and conditions — the (xN) marker is a prompt to look,
not a claim of equivalence.
