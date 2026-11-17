# Yeast: 8 protein-sequence scores and the localization site (10 classes) — attribution

- **Source**: UCI Machine Learning Repository, *Yeast* (dataset 110),
  http://archive.ics.uci.edu/ml/datasets/yeast
- **Creators**: Paul Horton, Kenta Nakai.
- **License**: CC BY 4.0 (UCI Machine Learning Repository terms). The license is re-verified from the fetched metadata on the first prepare run.
- **Citation requested by the source**: the dataset DOI is recorded here from the fetched metadata after the first prepare run (it was not read from the repository page when this file was written).
- **Retrieved**: pending. This pack's first fetch has not run yet;
  `prepare.py` fetches the dataset with the repository's own
  `ucimlrepo` package, prints the metadata it received, and
  refuses to write `data.csv` unless the license the metadata
  reports is CC BY 4.0. See
  `docs/benchmarks/2026-11-17-uci-expansion.md`.

## Preprocessing log (decode/rename only)

1. Score columns renamed to what each score measures, per the dataset documentation.
2. sequence_name and the class site codes keep their names and values.
