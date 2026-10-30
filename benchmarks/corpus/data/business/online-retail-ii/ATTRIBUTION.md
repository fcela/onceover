# Online Retail II — attribution

- **Source**: UCI Machine Learning Repository, *Online Retail II*
  (dataset 502), https://archive.ics.uci.edu/static/public/502/online+retail+ii.zip
- **Creator**: Daqing Chen.
- **License**: CC BY 4.0 (see the UCI dataset page).
- **Citation requested by the source**: DOI 10.24432/C5CG6D.
  Dataset page: https://archive.ics.uci.edu/dataset/502/online+retail+ii
- **Retrieved**: 2026-10-28.

## Storage: reference, not bundled (decided 2026-10-30)

The prepared CSV is 1,067,371 rows and ~93 MB, and it is not committed.
Git LFS was considered and declined; the dataset ships as a pinned
reference instead:

- The source is the UCI zip above; its sha256,
  `572e36277c2390fbfde10664750731e0a86f55e33470d91919085f0408e67bfb`,
  is pinned in `prepare.py`, which verifies the download against it on
  every run (a fresh download on 2026-10-30 matched, and the prepared
  CSV came out byte-identical to the 2026-10-28 preparation).
- The registry entry pins the prepared CSV's sha256
  (`f58108fb637bfae3e14a8adcd5535808478d0f03bfdd9927659044dd29718c19`)
  and row count; the corpus runner hash-verifies the prepared file
  whenever it is present locally, the same check bundled datasets get.
- `prepare.py` regenerates `data.csv` locally in about 20 seconds
  (requires polars and fastexcel). Without a local copy the runner
  reports the dataset as skipped, as for any test-only entry.

## Preprocessing log (rename/format only; no rows dropped)

1. The two yearly sheets are concatenated.
2. Columns renamed to snake_case (Price -> unit_price, Customer ID ->
   customer_id, Invoice -> invoice_no, and so on).
3. `invoice_date` rendered ISO 8601.
4. Documented behavior kept as-is: `C`-prefixed cancellation invoices
   (19,494 rows), negative quantities (22,950 rows), non-product stock
   codes (postage, fees, adjustments), and 243,007 rows with no
   customer id. All counts verified on the prepared file, 2026-10-28.
5. Invoice and StockCode are forced to string when the xlsx is read:
   a numeric-typed read silently nulls the C-prefixed invoice numbers.
