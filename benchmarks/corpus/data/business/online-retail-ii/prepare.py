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
"""
import io
import urllib.request
import zipfile
from pathlib import Path

import polars as pl

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://archive.ics.uci.edu/static/public/502/online+retail+ii.zip"
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
    (HERE / "data.csv").write_bytes(buf.getvalue())
    print(f"wrote {out.height} rows, {out.width} columns")


if __name__ == "__main__":
    main()
