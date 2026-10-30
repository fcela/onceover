# Credit Card Fraud (ULB) — attribution (test-only)

- **Original source**: *Credit Card Fraud Detection* (Machine Learning
  Group, Université Libre de Bruxelles / Worldline),
  https://www.kaggle.com/datasets/mlg-ulb/creditcardfraud
- **Distribution used here**: OpenML dataset 43627
  ("credit-card-fraud-detection"), https://www.openml.org/search?type=data&id=43627
  — the same ULB data, served without account credentials. (An earlier
  version of this file cited OpenML 1597; that is a different dataset,
  the Taiwan credit-card clients table. Corrected 2026-10-30.)
  Also used in ADBench as `fraud`.
- **License**: the OpenML record gives ODbL (database) / DbCL
  (contents). That fails this repository's bundle gate, so the dataset
  is **test-only**: `data.csv` is never committed. It ships as a
  pinned reference — `prepare.py` verifies the downloaded ARFF
  against the md5 checksum OpenML publishes in the dataset record
  (plus a pinned sha256), and the registry pins the prepared CSV's
  sha256, which the corpus runner verifies whenever the file is
  present locally.
- **Content**: 284,807 transactions made by European cardholders in
  September 2013; 492 fraud (0.17%). Features V1–V28 are PCA
  components; `time` (seconds since the first transaction) and
  `amount` are untransformed; `class` is 1 for fraud. Counts verified
  on the prepared file, 2026-10-30.
- **Citation requested by the source**: Dal Pozzolo, Caelen, Johnson
  & Bontempi, "Calibrating Probability with Undersampling for
  Unbalanced Classification", IEEE CIDM 2015.

## Preprocessing log (format only; no rows dropped, no values altered)

1. The ARFF data section is copied verbatim under snake_case column
   names (Time → time, V1..V28 → v1..v28, Amount → amount,
   Class → class). The source serves Class as `1.0`/`0.0` text; it is
   kept as served.
