# Congressional Voting Records — attribution

- **Source**: UCI Machine Learning Repository, *Congressional Voting
  Records* (dataset 105), https://archive.ics.uci.edu/static/public/105/data.csv
- **Creator**: Congressional Quarterly Almanac, 98th Congress, 2nd
  session (1984), via UCI.
- **License**: CC BY 4.0 (UCI Machine Learning Repository terms).
- **Citation requested by the source**: DOI 10.24432/C5C01P
  (UCI API record for dataset 105, fetched 2026-10-28). Dataset page:
  https://archive.ics.uci.edu/dataset/105/congressional+voting+records
- **Retrieved**: 2026-10-28. 435 members x 16 votes + party.
- One of the CrossCat paper's evaluation tables (voting records).

## Preprocessing log (decode/rename only)

1. `?` -> empty (votes not cast / not recorded).
2. `Class` -> `party`; vote-column hyphens become underscores.
   y/n values are the documented labels and are kept.
