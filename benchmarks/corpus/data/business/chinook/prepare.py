"""Regenerate data.csv for the Chinook benchmark table.

Source: the Chinook sample database (lerocha/chinook-database,
MIT), file Chinook_Sqlite.sqlite. The corpus table is the database's
Invoice table, extracted whole (412 rows) with columns renamed to
snake_case. Format conversion only: no rows dropped, no values
altered (InvoiceDate is already ISO text in the source).

The downloaded database file is verified against the pinned sha256
below on every run.
"""
import hashlib
import sqlite3
import sys
import urllib.request
from pathlib import Path

import polars as pl

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = ("https://raw.githubusercontent.com/lerocha/chinook-database/"
       "master/ChinookDatabase/DataSources/Chinook_Sqlite.sqlite")
# Pinned sha256 of the source database file (verified 2026-10-30).
SQLITE_SHA256 = "7651ba378ac2fcd0dfc3c66fb101f7a7eed3ba39a612ec642b96e20702061f15"
RENAME = {
    "InvoiceId": "invoice_id",
    "CustomerId": "customer_id",
    "InvoiceDate": "invoice_date",
    "BillingAddress": "billing_address",
    "BillingCity": "billing_city",
    "BillingState": "billing_state",
    "BillingCountry": "billing_country",
    "BillingPostalCode": "billing_postal_code",
    "Total": "total",
}


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    db = CACHE / "Chinook_Sqlite.sqlite"
    if not db.exists():
        urllib.request.urlretrieve(URL, db)
    digest = hashlib.sha256(db.read_bytes()).hexdigest()
    if digest != SQLITE_SHA256:
        sys.exit(f"source sqlite sha256 mismatch: got {digest}, pinned {SQLITE_SHA256}")
    con = sqlite3.connect(db)
    cur = con.execute("SELECT * FROM Invoice ORDER BY InvoiceId")
    cols = [d[0] for d in cur.description]
    rows = cur.fetchall()
    con.close()
    frame = pl.DataFrame(rows, schema=cols, orient="row").rename(RENAME)
    frame.write_csv(HERE / "data.csv")
    print(f"wrote {frame.height} rows, {frame.width} columns")


if __name__ == "__main__":
    main()
