"""Negative control for measure-weighted concentration
(issue #329): one cell — channel = web, plan = basic — holds
about 40% of the rows, and `revenue` is drawn from the same
gamma distribution in every cell, so the large cell's share
of revenue matches its share of rows.

With `revenue` declared through the `weights` hint, the cell
must not be stated as a weighted concentration: merely large
is not concentration. The cell is not a row-count
concentration either (its count ratio against independence
is about 1.3, below the pass's ratio bar), so the control
isolates the weighted judgment.

Generated to spec; no external source. The seed is pinned:
rerunning this script reproduces data.csv byte-for-byte (the
registry pins its sha256).
"""

import random
from pathlib import Path

import polars as pl

OUT = Path(__file__).parent

rng = random.Random(20261118)
n = 8000
channels = rng.choices(["web", "store", "phone"], weights=[0.45, 0.30, 0.25], k=n)
plans = [
    "basic" if rng.random() < (0.89 if ch == "web" else 0.50) else "premium"
    for ch in channels
]
df = pl.DataFrame(
    {
        "channel": channels,
        "plan": plans,
        "revenue": [round(rng.gammavariate(2.0, 30.0), 2) for _ in range(n)],
    }
)
df.write_csv(OUT / "data.csv")
print(df.shape)
