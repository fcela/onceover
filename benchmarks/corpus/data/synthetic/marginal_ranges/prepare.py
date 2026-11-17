"""Gate challenge table (issue #317): independent columns whose
ranges are disjoint or nested by construction, so order facts
follow from the marginals alone and any stated row-level
relationship is a false positive. Generated to spec; no external
source. The seed is pinned: rerunning this script reproduces
data.csv byte-for-byte (the registry pins its sha256).
"""

import random
from datetime import date, timedelta
from pathlib import Path

import polars as pl

OUT = Path(__file__).parent

rng = random.Random(20261118)
n = 3000
df = pl.DataFrame({
    "row_id": list(range(1, n + 1)),
    "low_band": [rng.uniform(0, 10) for _ in range(n)],
    "mid_band": [rng.uniform(20, 30) for _ in range(n)],
    "high_band": [rng.uniform(100, 110) for _ in range(n)],
    "wide_band": [rng.uniform(0, 200) for _ in range(n)],
    "inner_band": [rng.uniform(40, 60) for _ in range(n)],
    "level_code": [rng.randint(1, 5) for _ in range(n)],
})
df.write_csv(OUT / "data.csv")
print(df.shape)
