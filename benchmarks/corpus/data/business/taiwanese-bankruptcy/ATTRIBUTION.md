# Taiwanese Bankruptcy Prediction — attribution

- **Source**: UCI Machine Learning Repository, *Taiwanese Bankruptcy
  Prediction* (dataset 572),
  https://archive.ics.uci.edu/static/public/572/data.csv
- **License**: CC BY 4.0 (per the dataset page).
- **Citation requested by the source**: DOI 10.24432/C5004D.
  Dataset page: https://archive.ics.uci.edu/dataset/572/taiwanese+bankruptcy+prediction
- **Retrieved**: 2026-11-01. The data were collected from the Taiwan
  Economic Journal for the years 1999 to 2009; bankruptcy is defined
  based on the business regulations of the Taiwan Stock Exchange
  (dataset page). 6,819 rows, 95 ratio features plus the target;
  220 bankrupt (3.2%). The page reports no missing values.

## Preprocessing log (decode + rename only)

1. The served file separates fields with `, ` and prefixes column
   names with a space. It is decoded as CSV with skipinitialspace
   and written as a plain comma-separated file with trimmed column
   names. No values altered.
2. `Bankrupt?` -> `bankrupt` (rename only).
