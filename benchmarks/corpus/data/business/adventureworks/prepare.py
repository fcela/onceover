"""Regenerate data.csv for the AdventureWorks benchmark table.

Source: Microsoft's sql-server-samples repository (MIT),
samples/databases/adventure-works/oltp-install-script/
SalesOrderHeader.csv — the per-table install file for the
AdventureWorks Sales.SalesOrderHeader table. The file is
tab-separated with no header row; this script supplies the table's
documented column names in snake_case and writes CSV. Format
conversion only: no rows dropped, no values altered.

The downloaded file is verified against the pinned sha256 below on
every run.

Known answer in this table (verified on the source file,
2026-10-30): TotalDue = SubTotal + TaxAmt + Freight holds to the
cent on all 31,465 rows.
"""
import hashlib
import sys
import urllib.request
from pathlib import Path

import polars as pl

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = ("https://raw.githubusercontent.com/microsoft/sql-server-samples/"
       "master/samples/databases/adventure-works/oltp-install-script/"
       "SalesOrderHeader.csv")
# Pinned sha256 of the source install file (verified 2026-10-30).
SOURCE_SHA256 = "cf637c7d58006ebbc13daf0b8ed6cf7f738165d298ae0b5479a59237429c1882"
COLUMNS = [
    "sales_order_id", "revision_number", "order_date", "due_date",
    "ship_date", "status", "online_order_flag", "sales_order_number",
    "purchase_order_number", "account_number", "customer_id",
    "sales_person_id", "territory_id", "bill_to_address_id",
    "ship_to_address_id", "ship_method_id", "credit_card_id",
    "credit_card_approval_code", "currency_rate_id", "subtotal",
    "tax_amt", "freight", "total_due", "comment", "rowguid",
    "modified_date",
]


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    src = CACHE / "aw-SalesOrderHeader.csv"
    if not src.exists():
        urllib.request.urlretrieve(URL, src)
    digest = hashlib.sha256(src.read_bytes()).hexdigest()
    if digest != SOURCE_SHA256:
        sys.exit(f"source csv sha256 mismatch: got {digest}, pinned {SOURCE_SHA256}")
    frame = pl.read_csv(src, separator="\t", has_header=False,
                        new_columns=COLUMNS, infer_schema_length=10_000)
    frame.write_csv(HERE / "data.csv")
    print(f"wrote {frame.height} rows, {frame.width} columns")


if __name__ == "__main__":
    main()
