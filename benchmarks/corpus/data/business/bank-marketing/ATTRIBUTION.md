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

## Preprocessing log (format-standard preparation)

1. Columns renamed to fully descriptive names: `marital` ->
   `marital_status`, `default` -> `credit_in_default`, `balance` ->
   `average_yearly_balance_euros`, `housing` -> `housing_loan`,
   `loan` -> `personal_loan`, `day` -> `last_contact_day_of_month`,
   `duration` -> `last_contact_duration_seconds`, `campaign` ->
   `contacts_during_this_campaign`, `pdays` ->
   `days_since_previous_campaign_contact`, `previous` ->
   `contacts_before_this_campaign`, `poutcome` ->
   `previous_campaign_outcome`, `y` -> `subscribed_term_deposit`.
   The output file is named `bank-marketing_uci-222_bank-full.csv`.
2. The literal string `NaN` in categorical columns is this file's
   missing-value marker (36,959 rows in `poutcome`); decoded to
   empty. History: the marker was initially kept as served because
   a conditional rule reported the never-contacted structure
   exactly on the string form; the 2026-11-01 restandardization
   applies the decode and records the rule as a known-miss
   tripwire in the registry instead.
3. Kept as documented values: `pdays` = -1 means "never previously
   contacted" in this variant (the bank-additional variant uses 999).
