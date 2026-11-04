# Adult (Census Income) — attribution

- **Source**: UCI Machine Learning Repository, *Adult* (dataset
  2), https://archive.ics.uci.edu/static/public/2/adult.zip
- **Creators**: Barry Becker and Ronny Kohavi (1996). Extraction
  from the 1994 Census database under the conditions
  ((AAGE>16) && (AGI>100) && (AFNLWGT>1) && (HRSWK>0)), per the
  dataset page.
- **License**: CC BY 4.0 (see the UCI dataset page).
- **Citation requested by the source**: DOI 10.24432/C5XW20.
  Dataset page: https://archive.ics.uci.edu/dataset/2/adult
- **Retrieved**: 2026-11-03. The source zip's sha256,
  `7537312dd56c2b98035880805ce99e68183a30ee468aa5329d6df0fbb3cc21bb`,
  is pinned in `prepare.py` and verified on every run.

## Content

48,842 census respondents (the published train and test splits
concatenated) with fourteen demographic and employment
attributes and the binary income label (<=50K on 37,155 rows,
76.07%; >50K on 11,687). Per the source's own documentation:
`education` takes 16 levels, from Preschool to Doctorate, and
`education_num` is its numeric encoding — Preschool 1, 1st-4th
2, 5th-6th 3, 7th-8th 4, 9th 5, 10th 6, 11th 7, 12th 8, HS-grad
9, Some-college 10, Assoc-voc 11, Assoc-acdm 12, Bachelors 13,
Masters 14, Prof-school 15, Doctorate 16. `final_weight` (the
source's `fnlwgt`, "final weight") is the CPS sampling weight:
per `adult.names`, the weights are controlled to independent
estimates of the civilian noninstitutional population of the US,
and people with similar demographic characteristics should have
similar weights. It is not an attribute of the respondent.

## Preprocessing log

Implemented by `prepare.py`; nothing beyond this list is done.

1. **Concatenation.** `adult.data` (32,561 rows) then
   `adult.test` (16,281 rows). The test file's first line
   (`|1x3 Cross validator`) is a header artefact of the source
   distribution and is skipped; its labels carry a trailing
   period (`<=50K.`), which is stripped so both files share the
   label set. No rows are dropped from either file.
2. **Whitespace.** Every field is stripped (the source format
   is `, `-separated).
3. **Missing-value marker decoded.** The source's `?` marker
   (the UCI page records "Has Missing Values? Yes") becomes an
   empty field. It occurs in exactly three columns:
   work_class 2,799 rows (5.73%), occupation 2,809 (5.75%),
   native_country 857 (1.75%); counts measured with Polars on
   the raw files, 2026-11-03.
4. **Renames.** fnlwgt → final_weight, education-num →
   education_num, marital-status → marital_status, capital-gain
   → capital_gain, capital-loss → capital_loss, hours-per-week
   → hours_per_week, native-country → native_country,
   workclass → work_class, the label → income.

## The dataset's character, kept as served

1. **The education encoding is exact both ways.** education →
   education_num and education_num → education hold with 0
   violations in 48,842 rows (verified by group counting). The
   pair is one piece of information carried twice.
2. **Top-coded capital gain.** capital_gain is 0 on 44,807 rows
   (91.74%) and takes the value 99,999 on 244 rows; 123 distinct
   values, maximum 99,999. capital_loss is 0 on 46,560 rows
   (95.33%); after 0 its largest mass points are 1902 (304
   rows), 1977 (253) and 1887 (233); maximum 4,356.
3. **Duplicates.** 48,790 distinct full rows: 52 excess exact
   duplicate rows (49 groups, largest 3). Ignoring the label,
   57 excess; ignoring final_weight, only 42,468 distinct rows —
   6,374 rows repeat a row that differs only in the sampling
   weight, the behaviour `adult.names` predicts for similar
   people in different state samples.
4. **No key.** No single column is unique, and the full
   15-column tuple repeats, so no subset of columns identifies
   rows. No two columns are identical vectors either.
5. **Age mass at the maximum.** age runs 17–90 with 55 rows at
   exactly 90. Recorded as a count; the source documentation
   does not state an age top-code.
