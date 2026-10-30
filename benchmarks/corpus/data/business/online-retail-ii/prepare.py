"""Regenerate data.csv for the Online Retail II benchmark table.

Source: UCI Online Retail II (id 502), file online_retail_II.xlsx (two
sheets, 2009-2010 and 2010-2011) from
https://archive.ics.uci.edu/static/public/502/online+retail+ii.zip
Transformations (rename/format only; no rows dropped):
- Sheets concatenated; columns renamed to snake_case:
  Invoice -> invoice_no, StockCode -> stock_code, Description ->
  description, Quantity -> quantity, InvoiceDate -> invoice_date,
  Price -> unit_price, Customer ID -> customer_id, Country -> country.
- invoice_date rendered as ISO 8601 (YYYY-MM-DD HH:MM:SS).
Cancellation invoices keep their C-prefixed invoice numbers and negative
quantities -- they are part of the dataset's documented behavior.
Invoice and StockCode are forced to string at read time: the xlsx stores
most invoice numbers as numbers, and a numeric-typed read silently nulls
the C-prefixed cancellation numbers (19,500 rows lost that way in an
early version of this script -- caught by counting, fixed here).

Requires: polars and fastexcel (the xlsx reader). The dataset ships as a
reference (registry decision, 2026-10-30): the downloaded zip is verified
against the pinned sha256 below on every run, cached or fresh.
"""
import hashlib
import io
import sys
import urllib.request
import zipfile
from pathlib import Path

import polars as pl

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://archive.ics.uci.edu/static/public/502/online+retail+ii.zip"
# Pinned sha256 of the source zip as served at the URL above
# (verified 2026-10-30). If UCI re-serves the archive, update this pin
# and the registry's prepared-CSV pin together, with the reason recorded.
ZIP_SHA256 = "572e36277c2390fbfde10664750731e0a86f55e33470d91919085f0408e67bfb"


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()
RENAME = {
    "Invoice": "invoice_no",
    "StockCode": "stock_code",
    "Description": "description",
    "Quantity": "quantity",
    "InvoiceDate": "invoice_date",
    "Price": "unit_price",
    "Customer ID": "customer_id",
    "Country": "country",
}


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    zpath = CACHE / "online+retail+ii.zip"
    if not zpath.exists():
        urllib.request.urlretrieve(URL, zpath)
    digest = _sha256(zpath)
    if digest != ZIP_SHA256:
        sys.exit(f"source zip sha256 mismatch: got {digest}, pinned {ZIP_SHA256}")
    xlsx = CACHE / "online_retail_II.xlsx"
    if not xlsx.exists():
        with zipfile.ZipFile(zpath) as z:
            z.extract("online_retail_II.xlsx", CACHE)
    frames = []
    for sheet in ("Year 2009-2010", "Year 2010-2011"):
        df = pl.read_excel(xlsx, sheet_name=sheet,
                           schema_overrides={"Invoice": pl.String,
                                             "StockCode": pl.String})
        frames.append(df.rename(RENAME))
    out = pl.concat(frames).with_columns(
        pl.col("invoice_date").dt.strftime("%Y-%m-%d %H:%M:%S")
    )
    buf = io.BytesIO()
    out.write_csv(buf)
    payload = buf.getvalue()
    (HERE / "data.csv").write_bytes(payload)
    print(f"wrote {out.height} rows, {out.width} columns")
    print(f"data.csv sha256: {hashlib.sha256(payload).hexdigest()} "
          f"(registry pins this value; a mismatch means the preparation changed)")


if __name__ == "__main__":
    main()
