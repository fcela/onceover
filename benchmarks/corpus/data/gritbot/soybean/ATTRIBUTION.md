# Soybean — attribution

- **Source**: OpenML dataset 42 ("soybean"),
  https://www.openml.org/data/v1/download/42/soybean.arff
- **Creators**: R.S. Michalski, R.L. Chilausky (as recorded by OpenML and UCI).
- **License**: Public (OpenML's recorded licence for this dataset).
- **Cross-reference**: the same dataset is UCI *Soybean (Large)* (id 90),
  DOI 10.24432/C5JG6Z,
  https://archive.ics.uci.edu/dataset/90/soybean+%28large%29
- **Retrieved**: 2026-10-28.
- **Why OpenML is the bundled source**: this file has all 683 cases with
  the documented text labels — the version GritBot was demonstrated on.
  UCI's current data.csv for id 90 carries 307 rows with integer-coded
  attributes; it is recorded in the registry as an alternate source, not
  bundled.

## Preprocessing log (rename/format only)

1. ARFF converted to CSV; `?` (missing marker) becomes an empty field.
2. Column-name hyphens become underscores. Values are the dataset's own
   documented labels (`dna` = "does not apply" is one of them).
