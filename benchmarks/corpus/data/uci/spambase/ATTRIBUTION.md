# Spambase: word and character frequencies of 4,601 emails, spam or not — attribution

- **Source**: UCI Machine Learning Repository, *Spambase* (dataset 94),
  https://archive.ics.uci.edu/ml/datasets/spambase/
- **Creators**: Mark Hopkins, Erik Reeber, George Forman, Jaap Suermondt (Hewlett-Packard Labs).
- **License**: CC BY 4.0 (UCI Machine Learning Repository terms). The license is re-verified from the fetched metadata on the first prepare run.
- **Citation requested by the source**: the dataset DOI is recorded here from the fetched metadata after the first prepare run (it was not read from the repository page when this file was written).
- **Retrieved**: pending. This pack's first fetch has not run yet;
  `prepare.py` fetches the dataset with the repository's own
  `ucimlrepo` package, prints the metadata it received, and
  refuses to write `data.csv` unless the license the metadata
  reports is CC BY 4.0. See
  `docs/benchmarks/2026-11-17-uci-expansion.md`.

## Preprocessing log (decode/rename only)

1. Character-frequency columns with punctuation in their names renamed to words.
2. Class decoded: 1 -> spam, 0 -> not_spam.
3. Word-frequency and capital-run-length columns keep the repository's names.
