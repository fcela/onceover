"""Gate challenge table (issue #317): a complete daily series,
2021-01-01 to 2025-12-31, with a weekly cycle and two PLANTED
temporal breaks at known dates -- a variance change on
2022-07-01 (generating noise sd 55 -> 150) and a level shift on
2024-03-01 (level x1.38) -- plus one planted spike day
(2023-06-15, x3). The series is synthetic; it has no natural
break. Generated to spec; no external source. The seed is
pinned: rerunning this script reproduces data.csv byte-for-byte
(the registry pins its sha256).
"""

import random
from datetime import date, timedelta
from pathlib import Path

import polars as pl

OUT = Path(__file__).parent

rng = random.Random(20261120)
start = date(2021, 1, 1)
days = (date(2025, 12, 31) - start).days + 1
dates, visits, signups = [], [], []
for i in range(days):
    d = start + timedelta(days=i)
    wd = d.weekday()  # Mon=0 .. Sun=6
    mult = 1.0 if wd < 5 else (0.75 if wd == 5 else 0.70)
    sd = 55 if d < date(2022, 7, 1) else 150
    v = (900 + 0.06 * i) * mult + rng.gauss(0, sd)
    if d >= date(2024, 3, 1):
        v *= 1.38
    if d == date(2023, 6, 15):
        v *= 3
    v = max(1.0, v)
    dates.append(d.isoformat())
    visits.append(round(v, 1))
    signups.append(max(0, round(0.11 * v + rng.gauss(0, 8))))
df = pl.DataFrame({"date": dates, "visits": visits, "signups": signups})
df.write_csv(OUT / "data.csv")
print(df.shape)
