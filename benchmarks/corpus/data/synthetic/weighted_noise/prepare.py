"""Negative control for measure-weighted concentration
(issue #329): segment x region cells over independent
categoricals, a gamma-distributed measure, and a `noise_weight`
column drawn lognormally, independent of every other column.

With `noise_weight` declared through the `weights` hint, no
cell may be stated as a weighted concentration: the weight is
independent of the cell variables by construction. The heavy
lognormal spread makes the effective row count bind (about
e^-1 of the rows), so the control also discriminates the
significance discipline — a z computed on raw row counts
fires where the effective-count z must not.

Generated to spec; no external source. The seed is pinned:
rerunning this script reproduces data.csv byte-for-byte (the
registry pins its sha256).
"""

import random
from pathlib import Path

import polars as pl

OUT = Path(__file__).parent

rng = random.Random(20261117)
n = 8000
df = pl.DataFrame(
    {
        "segment": rng.choices(["alpha", "beta", "gamma", "delta"], k=n),
        "region": rng.choices(
            ["north", "south", "east", "west", "central"], k=n
        ),
        "amount": [round(rng.gammavariate(2.0, 30.0), 2) for _ in range(n)],
        "noise_weight": [round(rng.lognormvariate(0.0, 1.0), 2) for _ in range(n)],
    }
)
df.write_csv(OUT / "data.csv")
print(df.shape)
