# Iris flower measurements (Fisher, 1936) — attribution

- **Source**: UCI Machine Learning Repository, *Iris* (dataset 53),
  https://archive.ics.uci.edu/dataset/53/iris
- **Creators**: R. A. Fisher (data collected by Edgar Anderson).
- **License**: CC BY 4.0 (UCI Machine Learning Repository terms).
- **Citation requested by the source**: DOI 10.24432/C56C76.
- **Retrieved**: pending. This pack's first fetch has not run yet;
  `prepare.py` fetches the dataset with the repository's own
  `ucimlrepo` package, prints the metadata it received, and
  refuses to write `data.csv` unless the license the metadata
  reports is CC BY 4.0. See
  `docs/benchmarks/2026-11-17-uci-expansion.md`.

## Preprocessing log (decode/rename only)

1. Column names snake_cased.
2. Class labels decoded: Iris-setosa -> setosa, Iris-versicolor -> versicolor, Iris-virginica -> virginica.
