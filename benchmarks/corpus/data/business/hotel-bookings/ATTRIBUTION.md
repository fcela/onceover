# Hotel Booking Demand — attribution

- **Source**: Nuno Antonio, Ana de Almeida, and Luis Nunes (2019),
  *Hotel booking demand datasets*, Data in Brief, 22, 41–49,
  https://doi.org/10.1016/j.dib.2018.11.126
- **Mirror**: TidyTuesday `2020-02-11` (`hotels.csv`),
  https://raw.githubusercontent.com/rfordatascience/tidytuesday/main/data/2020/2020-02-11/hotels.csv
- **License**: CC BY 4.0 (published open access in *Data in Brief*).
- **Source CSV sha256**: `7c2ae42a7353905ea136e5c2287f17c92c5435826598bfbb8491c6f0c7b1fc06`

## Content

119,390 hotel booking records across two Portuguese properties (`City Hotel`,
79,330 rows; `Resort Hotel`, 40,060 rows) with scheduled arrival dates from
`2015-07-01` through `2017-08-31` across 32 columns, including the binary
cancellation outcome `is_canceled` (`0` / `1`), lead time, guest counts,
meal plan, country, market segment, distribution channel, deposit type,
agent/company IDs, average daily rate (`adr`), special requests, and
post-outcome reservation status fields (`reservation_status`,
`reservation_status_date`).

## Preprocessing log

Stored as Zstandard-compressed Parquet (`data.parquet`, 917 KB) directly
from `pl.read_csv(..., infer_schema_length=None)` with **no value
cleaning**: literal `"NULL"`, `"NA"`, and `"Undefined"` strings, exact
duplicate group blocks (`31,994` redundant rows), `adr = 5400` and
`adr = -6.38`, zero-guest and zero-night rows, and post-outcome leakage
columns are preserved verbatim as published.
