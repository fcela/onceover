# Air Quality: a year of hourly multisensor readings with a documented -200 missing code — attribution

- **Source**: UCI Machine Learning Repository, *Air Quality* (dataset 360)
- **Creators**: Saverio De Vito, Marco Massera, Raffaele Piga, Luisa Martinoto, Domenico Di Francia.
- **License**: CC BY 4.0 (UCI Machine Learning Repository terms). The license is re-verified from the fetched metadata on the first prepare run.
- **Citation requested by the source**: the dataset DOI is recorded here from the fetched metadata after the first prepare run (it was not read from the repository page when this file was written).
- **Retrieved**: pending. This pack's first fetch has not run yet;
  `prepare.py` fetches the dataset with the repository's own
  `ucimlrepo` package, prints the metadata it received, and
  refuses to write `data.csv` unless the license the metadata
  reports is CC BY 4.0. See
  `docs/benchmarks/2026-11-17-uci-expansion.md`.

## Preprocessing log (decode/rename only)

1. Sensor and reference columns renamed to plain snake_case names.
2. Date and Time rendered ISO (source formats DD/MM/YYYY and HH.MM.SS).
3. -200 values left untouched: the documented missing-value code is test material, not a preparation defect.
