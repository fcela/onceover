# Mushroom: 22 categorical cap/stalk attributes and the edible/poisonous class — attribution

- **Source**: UCI Machine Learning Repository, *Mushroom* (dataset 73)
- **Creators**: J. S. Schlimmer (data from the Audubon Society Field Guide to North American Mushrooms).
- **License**: CC BY 4.0 (UCI Machine Learning Repository terms). The license is re-verified from the fetched metadata on the first prepare run.
- **Citation requested by the source**: the dataset DOI is recorded here from the fetched metadata after the first prepare run (it was not read from the repository page when this file was written).
- **Retrieved**: pending. This pack's first fetch has not run yet;
  `prepare.py` fetches the dataset with the repository's own
  `ucimlrepo` package, prints the metadata it received, and
  refuses to write `data.csv` unless the license the metadata
  reports is CC BY 4.0. See
  `docs/benchmarks/2026-11-17-uci-expansion.md`.

## Preprocessing log (decode/rename only)

1. Column names snake_cased.
2. Class decoded: e -> edible, p -> poisonous (the dataset documentation's codes).
3. '?' decoded to null (the documented missing symbol, on stalk-root).
4. The 22 features' letter codes are kept as fetched; their decode tables are in the fetched variables metadata and are applied in a later reviewed step if wanted.
