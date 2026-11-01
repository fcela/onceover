# State unemployment panel, long form — attribution

- **Source**: the bundled wide panel in `../state-unemployment`
  (FRED downloads of BLS Local Area Unemployment Statistics:
  monthly unemployment rate for each state and DC),
  https://fred.stlouisfed.org/graph/fredgraph.csv?id=<SERIES>
- **License**: public data (BLS / US government; FRED distribution).
- 609 months (1976-01 onward) x 51 series, reshaped to 31,059 rows.

## Preprocessing log (reshape only)

1. Unpivot of the wide panel: one row per (month, state); the
   state code is the wide column prefix uppercased
   (`al_unemployment_rate` -> `AL`). No values altered, filled,
   or dropped. See `prepare.py`.
