# MAGIC Gamma Telescope: 10 shower-image parameters, gamma vs hadron — attribution

- **Source**: UCI Machine Learning Repository, *MAGIC Gamma Telescope* (dataset 159),
  https://archive.ics.uci.edu/ml/datasets/magic+gamma+telescope
- **Creators**: R. K. Bock (MAGIC project).
- **License**: CC BY 4.0 (UCI Machine Learning Repository terms).
- **Citation requested by the source**: DOI 10.24432/C52C8B.
- **Retrieved**: pending. This pack's first fetch has not run yet;
  `prepare.py` fetches the dataset with the repository's own
  `ucimlrepo` package, prints the metadata it received, and
  refuses to write `data.csv` unless the license the metadata
  reports is CC BY 4.0. See
  `docs/benchmarks/2026-11-17-uci-expansion.md`.

## Preprocessing log (decode/rename only)

1. Column names snake_cased.
2. Class decoded: g -> gamma, h -> hadron.
