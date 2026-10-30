# AdventureWorks (SalesOrderHeader) — attribution

- **Source**: Microsoft's sql-server-samples repository,
  `samples/databases/adventure-works/oltp-install-script/SalesOrderHeader.csv`
  — the per-table install file for AdventureWorks'
  Sales.SalesOrderHeader table.
  https://github.com/microsoft/sql-server-samples
- **License**: MIT (the repository's `license.txt`; copyright
  Microsoft Corporation). Bundle-compatible; the prepared table is
  committed.
- **Retrieved**: 2026-10-30. `prepare.py` verifies the downloaded
  install file against a pinned sha256.

## Preprocessing log (format only; no rows dropped, no values altered)

1. The install file is tab-separated with no header row; the
   table's documented 26 column names are supplied in snake_case
   (SalesOrderID → sales_order_id, SubTotal → subtotal,
   TaxAmt → tax_amt, TotalDue → total_due, and so on).
2. Written as CSV: 31,465 rows, 26 columns.

Known answers in this table, verified on the source file before
preparation (2026-10-30): `total_due = subtotal + tax_amt +
freight` holds to the cent on all 31,465 rows; `comment` is empty
on every row; `credit_card_id` is empty on 1,131 rows (orders not
paid by card). Which of these the current detector set reports is
recorded in the registry expectations and the pack write-up.
