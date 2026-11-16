"""Gate challenge table (issues #317, #332): a complete daily
series, 2022-01-01 to 2024-12-31, with a weekly cycle and a
PLANTED sustained level collapse: on 2023-03-15 the level drops
to 0.45 of its prior value and stays there for 150 days (to
2023-08-11), then recovers to 0.98 of the prior level from
2023-08-12. The series is synthetic; the collapse and recovery
are planted at known dates. Generated to spec; no external
source. The seed is pinned: rerunning this script reproduces
data.csv byte-for-byte (the registry pins its sha256).
"""

import random
from datetime import date, timedelta
from pathlib import Path

import polars as pl

OUT = Path(__file__).parent

rng = random.Random(20261123)
start = date(2022, 1, 1)
days = (date(2024, 12, 31) - start).days + 1
collapse_start = date(2023, 3, 15)
collapse_end = date(2023, 8, 11)  # inclusive: 150 days at the low level
dates, sessions, conversions = [], [], []
for i in range(days):
    d = start + timedelta(days=i)
    wd = d.weekday()  # Mon=0 .. Sun=6
    mult = 1.0 if wd < 5 else (0.78 if wd == 5 else 0.72)
    level = 1200 + 0.05 * i
    if collapse_start <= d <= collapse_end:
        level *= 0.45
    elif d > collapse_end:
        level *= 0.98
    s = max(10.0, level * mult + rng.gauss(0, 55))
    dates.append(d.isoformat())
    sessions.append(round(s))
    conversions.append(max(0, round(0.09 * s + rng.gauss(0, 6))))
df = pl.DataFrame({"date": dates, "sessions": sessions, "conversions": conversions})
df.write_csv(OUT / "data.csv")
print(df.shape)
