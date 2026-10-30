# Chinook (Invoice table) — attribution

- **Source**: the Chinook sample database, created by Luis Rocha —
  repository https://github.com/lerocha/chinook-database, file
  `ChinookDatabase/DataSources/Chinook_Sqlite.sqlite`.
- **License**: MIT (LICENSE.md in the repository; copyright
  (c) 2008-2024 Luis Rocha). Bundle-compatible; the prepared table
  is committed.
- **Retrieved**: 2026-10-30. `prepare.py` verifies the downloaded
  database file against a pinned sha256.

## Table and preprocessing log (format only)

The corpus table is the database's `Invoice` table, extracted whole:
412 rows, 9 columns. Columns renamed to snake_case (InvoiceId →
invoice_id, CustomerId → customer_id, InvoiceDate → invoice_date,
BillingAddress → billing_address, and so on). No rows dropped, no
values altered; invoice dates are ISO text in the source and are
kept as served.

Documented structure used as known answers: every invoice belongs
to a customer (`customer_id` references the Customer table, 59
customers); `invoice_id` is the primary key; totals are positive.
Whether the current detector set reports this table's structure is
recorded in the registry expectations and the pack write-up.
