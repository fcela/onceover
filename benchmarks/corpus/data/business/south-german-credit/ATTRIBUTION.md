# South German Credit — attribution

- **Source**: UCI Machine Learning Repository, *South German
  Credit* (dataset 522),
  https://archive.ics.uci.edu/static/public/522/south+german+credit.zip
- **License**: CC BY 4.0 (per the dataset page).
- **Citation requested by the source**: DOI 10.24432/C5X89F.
  Dataset page: https://archive.ics.uci.edu/dataset/522/south+german+credit
- **Retrieved**: 2026-11-01. 1,000 credits from 1973 to 1975:
  700 good and 300 bad, bad credits heavily oversampled in a
  stratified sample (dataset page). This dataset is the corrected
  successor to the Statlog German credit data: the page states
  that Statlog's coding information suffers from severe errors,
  and this version (Groemping 2019, based on the Open Data LMU
  2010 representation) corrects it.

## Preprocessing log (format-standard preparation)

The download holds a whitespace-separated `.asc` file whose
columns are abbreviated German names, plus the dataset's own
`codetable.txt`. This preparation:

1. Converts the `.asc` file to CSV.
2. Renames every column to the descriptive variable name the
   dataset page gives for it (`laufkont` -> `status`, `moral` ->
   `credit_history`, `verw` -> `purpose`, `hoehe` -> `amount`,
   `famges` -> `personal_status_sex`, `gastarb` ->
   `foreign_worker`, `kredit` -> `credit_risk`, and likewise for
   the rest).
3. Maps every categorical code to its codetable label, lightly
   regularized (ranges spelled out, e.g. savings code 3 ->
   "100 to below 500 DM"; credit_risk 0 -> "bad", 1 -> "good").

Documented traps, recorded rather than repaired: `amount` is the
result of an unknown monotonic transformation, not observed DM
values; `personal_status_sex` conflates sex and marital status
(code 2 covers both female non-single and male single debtors, so
sex cannot be recovered); the 30% bad rate is the oversampled
sample composition, not a population rate. No rows are dropped
and no numeric value is altered.
