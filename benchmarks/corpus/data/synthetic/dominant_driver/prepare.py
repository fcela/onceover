"""Gate challenge table (issue #317): listings whose price is
driven by size_sqft, with park_distance a plausible-looking but
weak alternative, rooms near-collinear with size, a weak
negative age effect, hoa_fee missing on about an eighth of
rows, and a concentration of listings in one suburb. Generated
to spec; no external source. The seed is pinned: rerunning this
script reproduces data.csv byte-for-byte (the registry pins its
sha256).
"""

import random
from datetime import date, timedelta
from pathlib import Path

import polars as pl

OUT = Path(__file__).parent

rng = random.Random(20261121)
n = 5000
suburbs = rng.choices(
    ["harbor", "hillcrest", "downtown", "airport", "university", "suburbia"],
    weights=[0.30, 0.22, 0.18, 0.12, 0.10, 0.08], k=n)
size = [max(600.0, rng.gauss(1800, 450)) for _ in range(n)]
rooms = [max(1, round(s / 350 + rng.gauss(0, 0.4))) for s in size]
park = [rng.uniform(2, 20) for _ in range(n)]
age_years = [rng.randint(0, 40) for _ in range(n)]
price = [
    420 * s + 800 * p - 1500 * a + rng.gauss(0, 90000)
    for s, p, a in zip(size, park, age_years)
]
df = pl.DataFrame({
    "listing_id": list(range(1, n + 1)),
    "suburb": suburbs,
    "size_sqft": [round(s, 1) for s in size],
    "rooms": rooms,
    "park_distance": [round(p, 2) for p in park],
    "age_years": age_years,
    "hoa_fee": [None if rng.random() < 0.12 else rng.randint(50, 600) for _ in range(n)],
    "price": [round(p, 2) for p in price],
})
df.write_csv(OUT / "data.csv")
print(df.shape)
