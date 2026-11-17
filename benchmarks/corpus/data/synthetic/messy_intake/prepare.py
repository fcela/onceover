"""Gate challenge table (issue #317): intake records generated to
a messy spec -- intake_date_raw mixes three date formats,
amount_raw and fee_raw are space-padded money strings with
thousands separators holding a -999 sentinel and blanks, notes
is blank on about 30% of rows, and status_code's distribution
flips at 2024-01-01 because its meaning changed there. Labelled
generated; not a real dataset. The seed is pinned: rerunning
this script reproduces data.csv byte-for-byte (the registry
pins its sha256).
"""

import random
from datetime import date, timedelta
from pathlib import Path

import polars as pl

OUT = Path(__file__).parent

rng = random.Random(20261122)
n = 2500
first = date(2023, 1, 1)
span = (date(2025, 6, 30) - first).days
rows = {"record_id": [], "intake_date_raw": [], "amount_raw": [],
        "fee_raw": [], "status_code": [], "source_form": [], "notes": []}
for i in range(n):
    d = first + timedelta(days=rng.randint(0, span))
    fmt = rng.random()
    if fmt < 0.60:
        raw_date = d.isoformat()
    elif fmt < 0.85:
        raw_date = d.strftime("%m/%d/%Y")
    else:
        raw_date = d.strftime("%d %b %Y")
    r = rng.random()
    if r < 0.04:
        amount_raw, fee_raw = " -999 ", None
    elif r < 0.07:
        amount_raw, fee_raw = None, None
    else:
        amount = rng.uniform(1000, 90000)
        amount_raw = f" {amount:,.2f} "
        fee_raw = f" {0.02 * amount:,.2f} "
    p_one = 0.70 if d < date(2024, 1, 1) else 0.30
    rows["record_id"].append(i + 1)
    rows["intake_date_raw"].append(raw_date)
    rows["amount_raw"].append(amount_raw)
    rows["fee_raw"].append(fee_raw)
    rows["status_code"].append(1 if rng.random() < p_one else 2)
    rows["source_form"].append(
        rng.choices(["web", "paper", "phone"], weights=[0.55, 0.30, 0.15])[0])
    rows["notes"].append(None if rng.random() < 0.30 else "intake complete")
df = pl.DataFrame(rows)
df.write_csv(OUT / "data.csv")
print(df.shape)
