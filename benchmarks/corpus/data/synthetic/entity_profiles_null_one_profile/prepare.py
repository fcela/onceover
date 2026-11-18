"""Null control with one category profile and strongly varying
supports (issue #448).

All 400 customers draw categories from one profile: the mixture
0.5 * A + 0.3 * B + 0.2 * C, using the A, B and C profiles of
entity_profiles. Locations are uniform over the five levels for
every row. There is no drift, no misfit and no location
specialist.

Supports vary strongly. Exactly half of the customers (200) get
3 to 8 rows, and the other half get 50 to 150 rows. A detector
that fingerprints customers by their category mix must not state
the small customers as misfits: with 3 to 8 rows, their category
proportions are noisy, and that noise is not a plant.

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

rng = random.Random(20261122)

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
PROFILE_C = {
    "beverages": 0.28, "snacks": 0.22, "fresh": 0.10, "bakery": 0.08,
    "dairy": 0.10, "frozen": 0.08, "household": 0.07, "personal_care": 0.07,
}
MIX = {c: 0.5 * PROFILE_A[c] + 0.3 * PROFILE_B[c] + 0.2 * PROFILE_C[c] for c in CATEGORIES}
assert abs(sum(MIX.values()) - 1.0) < 1e-9
MIX_VEC = [MIX[c] for c in CATEGORIES]

ids = [f"C{i:04d}" for i in range(1, N_CUST + 1)]
small_flags = [True] * (N_CUST // 2) + [False] * (N_CUST - N_CUST // 2)
rng.shuffle(small_flags)
small_ids = sorted(cid for cid, s in zip(ids, small_flags) if s)

rows = {"txn_id": [], "cust_id": [], "month": [], "category": [], "location": [], "cost": []}
txn = 0
for cid, small in zip(ids, small_flags):
    n = rng.randint(3, 8) if small else rng.randint(50, 150)
    for _ in range(n):
        txn += 1
        m = rng.randrange(12)
        cat = rng.choices(CATEGORIES, weights=MIX_VEC)[0]
        rows["txn_id"].append(txn)
        rows["cust_id"].append(cid)
        rows["month"].append(f"2025-{m + 1:02d}-01")
        rows["category"].append(cat)
        rows["location"].append(rng.choice(LOCATIONS))
        rows["cost"].append(round(rng.uniform(2, 60), 2))

df = pl.DataFrame(rows)
df.write_csv(OUT / "data.csv")

truth = {
    "expect": "no groups and no misfits; small customers must not be stated as misfits",
    "entity": "cust_id",
    "dimensions": ["category", "location"],
    "measure": "cost",
    "axis": "month",
    "profile": "0.5 * A + 0.3 * B + 0.2 * C, one profile for every customer",
    "groups": {},
    "misfits": [],
    "drift": None,
    "location_specialists": [],
    "small_customers": small_ids,
}
(OUT / "truth.json").write_text(json.dumps(truth, indent=1, sort_keys=True) + "\n")
print(df.shape)
