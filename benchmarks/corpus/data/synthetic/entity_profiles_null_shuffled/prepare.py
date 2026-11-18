"""Null control for the entity-profile fixture (issue #448).

Reads ../entity_profiles/data.csv and permutes the category column
and the location column independently across all rows. Every other
column (txn_id, cust_id, month, cost) is copied unchanged. The
permutation breaks the link between a customer and the categories
and locations it bought, so no customer has a profile, no misfits,
no drift and no location specialists. A detector that reports
groups on this table is reporting structure that the shuffle
removed.

The source file must be generated first by
../entity_profiles/prepare.py. Values are read as text so that
every unpermuted column is written back exactly as it appears in
the source. The seed is pinned: rerunning this script reproduces
data.csv and truth.json byte-for-byte.
"""

import json
import random
from pathlib import Path

import polars as pl

OUT = Path(__file__).parent
SOURCE = OUT.parent / "entity_profiles" / "data.csv"

rng = random.Random(20261121)

df = pl.read_csv(SOURCE, infer_schema_length=0)
categories = df["category"].to_list()
locations = df["location"].to_list()
rng.shuffle(categories)
rng.shuffle(locations)
out = df.with_columns(
    pl.Series("category", categories),
    pl.Series("location", locations),
)
out.write_csv(OUT / "data.csv")

truth = {
    "expect": "no groups, no misfits, no drift, no specialists",
    "source": "entity_profiles with category and location permuted independently across rows",
}
(OUT / "truth.json").write_text(json.dumps(truth, indent=1, sort_keys=True) + "\n")
print(out.shape)
