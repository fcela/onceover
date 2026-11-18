"""Entity-profile fixture (issue #448): customer transaction rows
drawn from three latent category profiles, with planted structure
for evaluating a future entity-fingerprints detector.

Plants:
- Three category profiles over eight categories. Profile A puts
  0.55 on fresh. Profile B puts 0.45 on household and
  personal_care combined. Profile C puts 0.50 on beverages and
  snacks combined. The remaining mass is spread over the other
  categories.
- Group membership: 12 customers are misfits (see below). The
  other 388 are assigned to A, B and C in proportions 0.5, 0.3
  and 0.2 (194, 116 and 78 customers).
- Location specialists: 70% of customers buy at one location
  only, chosen uniformly. The other 30% spread their rows
  uniformly over all five locations.
- Misfits: 12 customers whose categories are 0.45 frozen,
  0.45 dairy and 0.10 spread over the other six categories. No
  profile favours this pair.
- Drift: 40 customers from profile B switch to profile A from
  2025-07-01 onward. Their rows dated January to June follow B,
  and their rows dated July to December follow A.
- Support: each customer has a row count drawn uniformly from
  20 to 120, so supports vary.

Each row's month is drawn uniformly over the twelve months of
2025 (first of the month). Cost is uniform over 2.00 to 60.00,
rounded to two decimals. Generated to spec; no external source.
The seed is pinned: rerunning this script reproduces data.csv
and truth.json byte-for-byte.
"""

import json
import random
from pathlib import Path

import polars as pl

OUT = Path(__file__).parent

rng = random.Random(20261120)

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
MISFIT = {"frozen": 0.45, "dairy": 0.45}
for c in CATEGORIES:
    MISFIT.setdefault(c, 0.10 / 6)

PROFILES = {"A": PROFILE_A, "B": PROFILE_B, "C": PROFILE_C, "M": MISFIT}
for name, prof in PROFILES.items():
    assert abs(sum(prof.values()) - 1.0) < 1e-9, name
PROFILE_VEC = {name: [prof[c] for c in CATEGORIES] for name, prof in PROFILES.items()}

ids = [f"C{i:04d}" for i in range(1, N_CUST + 1)]

shuffled = ids[:]
rng.shuffle(shuffled)
misfits = sorted(shuffled[:12])
rest = shuffled[12:]
n_a = round(0.5 * len(rest))
n_b = round(0.3 * len(rest))
group_ids = {
    "A": sorted(rest[:n_a]),
    "B": sorted(rest[n_a:n_a + n_b]),
    "C": sorted(rest[n_a + n_b:]),
}
profile_of = {cid: "M" for cid in misfits}
for name, members in group_ids.items():
    for cid in members:
        profile_of[cid] = name

drifters = sorted(rng.sample(group_ids["B"], 40))
drift_set = set(drifters)

specialist_loc = {}
for cid in ids:
    if rng.random() < 0.7:
        specialist_loc[cid] = rng.choice(LOCATIONS)
    else:
        specialist_loc[cid] = None
specialists = sorted(cid for cid in ids if specialist_loc[cid] is not None)

rows = {"txn_id": [], "cust_id": [], "month": [], "category": [], "location": [], "cost": []}
txn = 0
for cid in ids:
    n = rng.randint(20, 120)
    for _ in range(n):
        txn += 1
        m = rng.randrange(12)
        prof = profile_of[cid]
        if cid in drift_set and m >= 6:
            prof = "A"
        cat = rng.choices(CATEGORIES, weights=PROFILE_VEC[prof])[0]
        loc = specialist_loc[cid] if specialist_loc[cid] is not None else rng.choice(LOCATIONS)
        rows["txn_id"].append(txn)
        rows["cust_id"].append(cid)
        rows["month"].append(f"2025-{m + 1:02d}-01")
        rows["category"].append(cat)
        rows["location"].append(loc)
        rows["cost"].append(round(rng.uniform(2, 60), 2))

df = pl.DataFrame(rows)
df.write_csv(OUT / "data.csv")

truth = {
    "entity": "cust_id",
    "dimensions": ["category", "location"],
    "measure": "cost",
    "axis": "month",
    "groups": group_ids,
    "misfits": misfits,
    "drift": {"ids": drifters, "period": "2025-07-01", "from": "B", "to": "A"},
    "location_specialists": specialists,
}
(OUT / "truth.json").write_text(json.dumps(truth, indent=1, sort_keys=True) + "\n")
print(df.shape)
