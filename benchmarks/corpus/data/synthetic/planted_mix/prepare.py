"""Gate challenge table (issue #317): application records carrying
plants of kinds the digest under-recovers -- a -999 sentinel on
the legacy-import rows marking a real subpopulation, an exact
affine identity (total_cost = 2*parts_cost + 3*labor_cost + 7),
a per-level approval contrast by channel, followup_days missing
on exactly the legacy rows, and a hotspot cell (channel = kiosk,
age 18-24) holding a disproportionate share of payout mass.
Generated to spec; no external source. The seed is pinned:
rerunning this script reproduces data.csv byte-for-byte (the
registry pins its sha256).
"""

import random
from datetime import date, timedelta
from pathlib import Path

import polars as pl

OUT = Path(__file__).parent

rng = random.Random(20261119)
n = 6000
channels = rng.choices(["branch", "web", "kiosk"], weights=[0.40, 0.35, 0.25], k=n)
legacy = [rng.random() < 0.08 for _ in range(n)]
p_approve = {"branch": 0.62, "web": 0.45, "kiosk": 0.18}
approved = [
    1 if rng.random() < (0.25 if leg else p_approve[ch]) else 0
    for ch, leg in zip(channels, legacy)
]
parts = [rng.randint(20, 200) for _ in range(n)]
labor_hours = [rng.randint(1, 4) for _ in range(n)]
labor_cost = [45 * h for h in labor_hours]
total = [2 * p + 3 * lc + 7 for p, lc in zip(parts, labor_cost)]
age = [rng.randint(18, 70) for _ in range(n)]
payout = []
for ch, a in zip(channels, age):
    if ch == "kiosk" and 18 <= a <= 24:
        payout.append(round(rng.uniform(700, 1100), 2))
    else:
        payout.append(round(rng.uniform(40, 160), 2))
df = pl.DataFrame({
    "application_id": list(range(1, n + 1)),
    "channel": channels,
    "source_system": ["legacy" if leg else "current" for leg in legacy],
    "approved": approved,
    "applicant_age": age,
    "parts_cost": parts,
    "labor_cost": labor_cost,
    "total_cost": total,
    "income_reported": [-999 if leg else rng.randint(25000, 120000) for leg in legacy],
    "followup_days": [None if leg else rng.randint(1, 90) for leg in legacy],
    "payout": payout,
})
df.write_csv(OUT / "data.csv")
print(df.shape)
