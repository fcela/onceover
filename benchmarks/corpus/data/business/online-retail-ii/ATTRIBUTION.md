# Online Retail II — attribution

- **Source**: UCI Machine Learning Repository, *Online Retail II*
  (dataset 502), https://archive.ics.uci.edu/static/public/502/online+retail+ii.zip
- **Creator**: Daqing Chen.
- **License**: CC BY 4.0 (see the UCI dataset page).
- **Citation requested by the source**: DOI 10.24432/C5CG6D.
  Dataset page: https://archive.ics.uci.edu/dataset/502/online+retail+ii
- **Retrieved**: 2026-10-28.

## Bundling status: test-only in this repository (size)

The prepared CSV is 1,067,371 rows and ~93 MB. That exceeds what this
project's publishing path (GitHub Git Data API blobs) can carry, so the
CSV is not committed; `prepare.py` regenerates it locally in about
20 seconds and the registry entry is marked test-only with this reason.
Bundling it properly is a Git LFS decision, recorded in the registry.

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
