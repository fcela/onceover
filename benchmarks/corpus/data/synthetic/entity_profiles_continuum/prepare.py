"""Continuum fixture (issue #448): customers whose category mix
lies between two profiles, with no discrete groups.

Each of 400 customers has a mixing weight w, drawn uniformly from
[0, 1) and rounded to three decimals. The customer's category
distribution is w * A + (1 - w) * B, where A and B are the profiles
of entity_profiles: A puts 0.55 on fresh, and B puts 0.45 on
household and personal_care combined. A customer with w near 1
looks like A, a customer with w near 0 looks like B, and most
customers sit between them. Row counts are uniform over 60 to 120
per customer. Locations are uniform over the five levels for every
row. There is no drift, no misfit and no location specialist.

The truth is a continuum. Any partition into a fixed number of
groups cuts through it, so a detector that reports k clean groups
on this table is splitting a gradient. The weights are stored in
truth.json so that a fingerprint score can be checked against the
generating mix.

Each row's month is drawn uniformly over the twelve months of
2025. Cost is uniform over 2.00 to 60.00, rounded to two decimals.
Generated to spec; no external source. The seed is pinned:
rerunning this script reproduces data.csv and truth.json
byte-for-byte.
"""

import json
import random
from pathlib import Path

import polars as pl

OUT = Path(__file__).parent

rng = random.Random(20261123)

N_CUST = 400
CATEGORIES = [
    "fresh", "bakery", "dairy", "frozen",
    "household", "personal_care", "beverages", "snacks",
]
LOCATIONS = ["north", "south", "east", "west", "central"]

PROFILE_A = {
    "fresh": 0.55, "dairy": 0.10, "bakery": 0.08, "frozen": 0.06,
    "beverages": 0.07, "snacks": 0.05, "household": 0.05, "personal_care": 0.04,
}
PROFILE_B = {
    "household": 0.25, "personal_care": 0.20, "fresh": 0.08, "bakery": 0.08,
    "dairy": 0.12, "frozen": 0.12, "beverages": 0.08, "snacks": 0.07,
}
VEC_A = [PROFILE_A[c] for c in CATEGORIES]
VEC_B = [PROFILE_B[c] for c in CATEGORIES]
assert abs(sum(VEC_A) - 1.0) < 1e-9 and abs(sum(VEC_B) - 1.0) < 1e-9

ids = [f"C{i:04d}" for i in range(1, N_CUST + 1)]

rows = {"txn_id": [], "cust_id": [], "month": [], "category": [], "location": [], "cost": []}
weights = {}
txn = 0
for cid in ids:
    w = round(rng.random(), 3)
    weights[cid] = w
    vec = [w * a + (1 - w) * b for a, b in zip(VEC_A, VEC_B)]
    n = rng.randint(60, 120)
    for _ in range(n):
        txn += 1
        m = rng.randrange(12)
        cat = rng.choices(CATEGORIES, weights=vec)[0]
        rows["txn_id"].append(txn)
        rows["cust_id"].append(cid)
        rows["month"].append(f"2025-{m + 1:02d}-01")
        rows["category"].append(cat)
        rows["location"].append(rng.choice(LOCATIONS))
        rows["cost"].append(round(rng.uniform(2, 60), 2))

df = pl.DataFrame(rows)
df.write_csv(OUT / "data.csv")

truth = {
    "expect": "a continuum between profile A and profile B, not k groups",
    "entity": "cust_id",
    "dimensions": ["category", "location"],
    "measure": "cost",
    "axis": "month",
    "weights": weights,
}
(OUT / "truth.json").write_text(json.dumps(truth, indent=1, sort_keys=True) + "\n")
print(df.shape)
