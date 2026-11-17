# Heart Disease (Cleveland): 13 clinical attributes and the diagnosis grade — attribution

- **Source**: UCI Machine Learning Repository, *Heart Disease* (dataset 45),
  http://archive.ics.uci.edu/ml/datasets/Heart+Disease
- **Creators**: Andras Janosi, William Steinbrunn, Matthias Pfisterer, Robert Detrano.
- **License**: CC BY 4.0 (UCI Machine Learning Repository terms).
- **Citation requested by the source**: DOI 10.24432/C52P4X.
- **Retrieved**: pending. This pack's first fetch has not run yet;
  `prepare.py` fetches the dataset with the repository's own
  `ucimlrepo` package, prints the metadata it received, and
  refuses to write `data.csv` unless the license the metadata
  reports is CC BY 4.0. See
  `docs/benchmarks/2026-11-17-uci-expansion.md`.

## Preprocessing log (decode/rename only)

1. Columns renamed to the documentation's plain meanings.
2. Coded categoricals decoded to the documentation's labels (sex, chest_pain_type, resting_ecg, st_slope, thalassemia); flags and grades stay numeric.
3. '?' decoded to null.
