# Communities and Crime: 127 census/LEMAS attributes and violent crimes per capita — attribution

- **Source**: UCI Machine Learning Repository, *Communities and Crime* (dataset 183),
  http://archive.ics.uci.edu/ml/datasets/communities+and+crime
- **Creators**: Michael Redmond (La Salle University).
- **License**: CC BY 4.0 (UCI Machine Learning Repository terms). The license is re-verified from the fetched metadata on the first prepare run.
- **Citation requested by the source**: the dataset DOI is recorded here from the fetched metadata after the first prepare run (it was not read from the repository page when this file was written).
- **Retrieved**: pending. This pack's first fetch has not run yet;
  `prepare.py` fetches the dataset with the repository's own
  `ucimlrepo` package, prints the metadata it received, and
  refuses to write `data.csv` unless the license the metadata
  reports is CC BY 4.0. See
  `docs/benchmarks/2026-11-17-uci-expansion.md`.

## Preprocessing log (decode/rename only)

1. '?' decoded to null.
2. Target renamed to violent_crimes_per_pop; feature columns keep the repository's names verbatim.
