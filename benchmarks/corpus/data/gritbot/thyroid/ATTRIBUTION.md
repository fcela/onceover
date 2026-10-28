# Thyroid (allhypo) — attribution

- **Source**: UCI Machine Learning Repository, *Thyroid Disease* (dataset 102),
  files `allhypo.data` + `allhypo.test` from
  https://archive.ics.uci.edu/static/public/102/thyroid+disease.zip
- **Creators**: Thyroid disease records supplied by the Garavan Institute and
  J. Ross Quinlan, New South Wales Institute, Sydney, Australia (1987), as
  stated in the dataset's own `.names` file.
- **License**: CC BY 4.0 (UCI Machine Learning Repository terms).
- **Citation requested by the source**: DOI 10.24432/C5D010 (dataset 102).
  Dataset page: https://archive.ics.uci.edu/dataset/102/thyroid+disease
- **Retrieved**: 2026-10-28.

## Why these two files

RuleQuest's GritBot sample results (https://rulequest.com/gritbot-examples.html)
analyze a thyroid screening application's data and test files, 3,772 cases in
total, concerning hypothyroid diagnosis. `allhypo.data` (2,800) + `allhypo.test`
(972) = 3,772, and line 1365 of `allhypo.data` is GritBot's published
"data case 1365" (record 861, age 455) — the mapping was verified against the
published cases before adoption, including the two pregnant patients in their
seventies at data lines 2224 and 1610.

## Preprocessing log (decode/rename only; no rows dropped, no values altered)

1. Column names taken from `allhypo.names`, lowercased, spaces to underscores.
2. The trailing `|<id>` suffix on each record becomes `record_id`.
3. `?` (the files' missing-value marker) becomes an empty field.
4. Boolean attributes stored as `t`/`f` become `true`/`false`.
5. Diagnosis values lose the trailing period used in the `.names` class list.
6. `case_ref` added (`data:<line>` / `test:<line>`, 1-indexed within each
   source file) to preserve the numbering GritBot's output uses.
