# Abalone — attribution

- **Source**: UCI Machine Learning Repository, *Abalone* (dataset 1),
  https://archive.ics.uci.edu/static/public/1/data.csv
- **Creators**: Warwick Nash, Tracy Sellers, Simon Talbot, Andrew Cawthorn,
  Wes Ford. Data from the Marine Resources Division, Tasmanian Department of
  Primary Industry and Fisheries.
- **License**: CC BY 4.0 (UCI Machine Learning Repository terms).
- **Citation requested by the source**: DOI 10.24432/C55C7W.
  Dataset page: https://archive.ics.uci.edu/dataset/1/abalone
- **Retrieved**: 2026-10-28.
- **GritBot reference**: RuleQuest's sample results report 23 possible
  anomalies in this table, most violating physical constraints the data
  imply (a part's weight exceeding the whole weight; length below diameter).

## Preprocessing log (decode/rename only)

1. `Sex` decoded: M -> male, F -> female, I -> infant (UCI documentation).
2. Column names lowercased.
