# Gas Sensor Array Drift: 128 sensor features in 10 batches over 36 months — attribution

- **Source**: UCI Machine Learning Repository, *Gas Sensor Array Drift Dataset* (dataset 224),
  https://archive.ics.uci.edu/dataset/224/gas+sensor+array+drift+dataset
- **Creators**: Alexander Vergara (with the ChemoSignals Laboratory, UC San Diego).
- **License**: CC BY 4.0 (UCI Machine Learning Repository terms).
- **Citation requested by the source**: DOI 10.24432/C5RP6W.
- **Retrieved**: pending. This pack's first fetch has not run yet;
  `prepare.py` fetches the dataset with the repository's own
  `ucimlrepo` package, prints the metadata it received, and
  refuses to write `data.csv` unless the license the metadata
  reports is CC BY 4.0. See
  `docs/benchmarks/2026-11-17-uci-expansion.md`.

## Preprocessing log (decode/rename only)

1. Class codes decoded to the documented gas names when the fetched values are the codes.
2. Feature columns keep the repository's names; a fetched batch column is renamed `batch`. Whether the fetched frame carries the batch column is recorded here after the first run.
