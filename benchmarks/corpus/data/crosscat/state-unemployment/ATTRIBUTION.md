# State unemployment panel — attribution

- **Source**: FRED (Federal Reserve Bank of St. Louis) series
  downloads of BLS Local Area Unemployment Statistics: monthly
  unemployment rate for each state and DC (series CAUR, NYUR, ...),
  https://fred.stlouisfed.org/graph/fredgraph.csv?id=<SERIES>
- **License**: public data (BLS / US government; FRED distribution).
- **Retrieved**: 2026-10-28. 609 months (1976-01 onward) x 51 series.
- The CrossCat paper's unemployment evaluation used a state-level
  unemployment panel; this is the same series family in wide form.

## Preprocessing log (merge only)

1. Series merged wide on observation date; FRED's `.` missing marker
   becomes empty. No values altered.
