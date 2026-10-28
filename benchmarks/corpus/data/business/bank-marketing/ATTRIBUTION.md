# Bank Marketing — attribution

- **Source**: UCI Machine Learning Repository, *Bank Marketing* (dataset 222),
  https://archive.ics.uci.edu/static/public/222/data.csv
- **Creators**: S. Moro, P. Rita, P. Cortez.
- **License**: CC BY 4.0 (UCI Machine Learning Repository terms).
- **Citation requested by the source**: DOI 10.24432/C5K306.
  Dataset page: https://archive.ics.uci.edu/dataset/222/bank+marketing
- **Retrieved**: 2026-10-28. The file UCI serves here is the original
  bank-full variant: 45,211 rows. (ADBench's `campaign` derives from the
  same source at 41,188 rows — that is the later bank-additional-full
  variant, a different file.)

## Preprocessing log (rename + missing-marker decode only)

1. `y` -> `subscribed_term_deposit`.
2. The literal string `NaN` in categorical columns is this file's
   missing-value marker (36,959 rows in `poutcome`); decoded to empty.
3. Kept as documented values: `pdays` = -1 means "never previously
   contacted" in this variant (the bank-additional variant uses 999),
   and `unknown` is a genuine category level.
