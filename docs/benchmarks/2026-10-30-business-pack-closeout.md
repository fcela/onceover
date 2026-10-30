# Business pack close-out — fraud reference and the sample databases

Date: 2026-10-30. Issue #21's open remainders were the
credit-card-fraud entry's sourcing and the sample databases'
unresolved license check. Both are settled here; every dataset the
issue lists is now either bundled and running, a pinned reference
that profiles locally, or registered with a recorded disposition.

## Credit-card fraud: Kaggle credentials replaced by an OpenML reference

The entry was skipped ("Kaggle credentials required"), and its
attribution cited OpenML 1597 — a different dataset (Taiwan
credit-card clients). The ULB fraud data on OpenML is dataset
43627. Its record gives the licence as ODbL (database) / DbCL
(contents), so the dataset stays test-only under the license gate,
but it now ships as a pinned reference like Online Retail II:

- `prepare.py` downloads the ARFF from OpenML and verifies it
  against the md5 checksum OpenML publishes in the dataset record
  (matched on the 2026-10-30 download) plus a pinned sha256.
- The registry pins the prepared CSV's sha256 (284,807 rows); the
  runner verifies it whenever the file is present (the runner
  change from the Retail II reference: any pinned entry is
  verified, not only bundled ones).
- First local run: ok, 70 findings, expectations 3/3. Found, as
  recorded: the defining imbalance (class 99.83% '0', 492 fraud)
  as a concentration finding; class recognized as categorical
  despite numeric storage; 1,081 duplicate rows across the
  compared columns (not predicted; recorded as found). Runtime
  note: 92.9 s at standard budget — the discontinuities screen's
  cost on wide numeric tables is issue #65.

## Sample databases: license check resolved, two tables bundled

The check the remainder audit left open is resolved from the
canonical repositories' own license files: Chinook is MIT
(LICENSE.md, Luis Rocha) and microsoft/sql-server-samples —
which distributes both Northwind and AdventureWorks — is MIT
(license.txt). GitHub's license API reports both as unrecognized;
the texts are the standard MIT grant.

- **Chinook** (bundled): the Invoice table extracted whole from
  the canonical SQLite file (412 rows). First run ok, 43 findings,
  expectations 3/3: the address FD structure (19 FD findings),
  invoice_id uniqueness, and the customer/address duplicate
  encoding are all reported.
- **AdventureWorks** (bundled): SalesOrderHeader from the
  install-script CSV (31,465 rows). First run ok, 100 findings,
  expectations 5/5. Two documented cases are recorded as known
  misses: the exact identity total_due = subtotal + tax_amt +
  freight (verified on the source file; the nearest signal is a
  99.98% near-FD whose 6 violations are cent-rounding) and the
  fully empty `comment` column, which no finding reports.
- **Northwind** (registered, never fetched): the canonical
  distribution is a T-SQL install script with no flat table file
  to pin; the registry records the disposition and the resolved
  license.

Corpus state after this change: 26 registry entries; full run
exits clean with 15 datasets ok (12 before, plus these three) and
every other dataset ok or skipped with its recorded reason.
Python suite 125 passed, 1 skipped.
