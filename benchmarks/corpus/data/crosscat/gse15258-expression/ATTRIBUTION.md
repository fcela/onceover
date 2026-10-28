# GSE15258 gene expression — attribution

- **Source**: NCBI Gene Expression Omnibus, series GSE15258,
  series matrix at
  https://ftp.ncbi.nlm.nih.gov/geo/series/GSE15nnn/GSE15258/matrix/GSE15258_series_matrix.txt.gz
- **License**: public data (NCBI GEO).
- **Retrieved**: 2026-10-28.
- **Contents**: 86 arrays (the series matrix's sample count; the
  CrossCat paper describes the study as 87 patients) x the first
  5,000 probes in file order, with sample annotations (disease state,
  anti-TNF response, DAS28 at two visits). The paper evaluated
  1,000- and 5,000-column subsets of this series; the subset rule
  here is deterministic (first 5,000 in file order) and is stated in
  prepare.py.

## Preprocessing log (transpose + subset only)

1. Matrix transposed: one row per array.
2. Sample characteristics parsed into named columns.
3. First 5,000 probes kept; no values altered.
