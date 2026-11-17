"""Gate challenge table (issue #317): many numeric columns built from
three latent factors, interleaved so pairwise and axis statements
have many near-equivalent forms whose selection can depend on
column order. Generated to spec; no external source. The seed is
pinned: rerunning this script reproduces data.csv byte-for-byte
(the registry pins its sha256).
"""

import random
from datetime import date, timedelta
from pathlib import Path

import polars as pl

OUT = Path(__file__).parent

rng = random.Random(20261117)
n = 4000
f1 = [rng.gauss(0, 1) for _ in range(n)]
f2 = [rng.gauss(0, 1) for _ in range(n)]
f3 = [rng.gauss(0, 1) for _ in range(n)]
noise = lambda sd: [rng.gauss(0, sd) for _ in range(n)]
add = lambda a, b: [x + y for x, y in zip(a, b)]
scale = lambda a, k: [k * x for x in a]
df = pl.DataFrame({
    "row_id": list(range(1, n + 1)),
    "a1": add(f1, noise(0.05)),
    "b1": add(f2, noise(0.05)),
    "n1": noise(1.0),
    "a2": add(scale(f1, 2.0), noise(0.05)),
    "c1": add(f3, noise(0.05)),
    "mix1": add(add(f1, f2), noise(0.10)),
    "n2": noise(1.0),
    "b2": add(scale(f2, 0.5), noise(0.05)),
    "a3": add(scale(f1, -1.0), noise(0.10)),
    "n3": noise(1.0),
    "mix2": add([x - y for x, y in zip(f1, f2)], noise(0.10)),
    "n4": noise(1.0),
    "n5": noise(1.0),
    "n6": noise(1.0),
})
df.write_csv(OUT / "data.csv")
print(df.shape)
