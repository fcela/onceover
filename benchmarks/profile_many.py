"""Evaluation for profile_many (issue #136 item 2): the ruled criterion
on the real Chinook database, planted orphans, a planted chance
containment, and three scale cases.

Run:  python benchmarks/profile_many.py [--scale-only]

The Chinook database file is the corpus's pinned source (same URL and
sha256 as benchmarks/corpus/data/business/chinook/prepare.py; MIT).
It is downloaded to the corpus raw cache and verified on every run.
The script exits nonzero if the Chinook shape assertions fail.
"""
import hashlib
import sqlite3
import sys
import time
import urllib.request
from pathlib import Path

import polars as pl

HERE = Path(__file__).parent
CACHE = HERE / "corpus" / ".raw-cache"
URL = ("https://raw.githubusercontent.com/lerocha/chinook-database/"
       "master/ChinookDatabase/DataSources/Chinook_Sqlite.sqlite")
SQLITE_SHA256 = "7651ba378ac2fcd0dfc3c66fb101f7a7eed3ba39a612ec642b96e20702061f15"

TRUE_FK = {
    ("Album", "ArtistId", "Artist", "ArtistId"),
    ("Customer", "SupportRepId", "Employee", "EmployeeId"),
    ("Invoice", "CustomerId", "Customer", "CustomerId"),
    ("InvoiceLine", "InvoiceId", "Invoice", "InvoiceId"),
    ("InvoiceLine", "TrackId", "Track", "TrackId"),
    ("PlaylistTrack", "PlaylistId", "Playlist", "PlaylistId"),
    ("PlaylistTrack", "TrackId", "Track", "TrackId"),
    ("Track", "AlbumId", "Album", "AlbumId"),
    ("Track", "GenreId", "Genre", "GenreId"),
    ("Track", "MediaTypeId", "MediaType", "MediaTypeId"),
}


def load_chinook():
    CACHE.mkdir(parents=True, exist_ok=True)
    db = CACHE / "Chinook_Sqlite.sqlite"
    if not db.exists():
        urllib.request.urlretrieve(URL, db)
    digest = hashlib.sha256(db.read_bytes()).hexdigest()
    if digest != SQLITE_SHA256:
        sys.exit(f"source sqlite sha256 mismatch: got {digest}")
    con = sqlite3.connect(db)
    tables = [r[0] for r in con.execute(
        "SELECT name FROM sqlite_master WHERE type='table' "
        "AND name NOT LIKE 'sqlite_%' ORDER BY name")]
    frames = {}
    for t in tables:
        cur = con.execute(f'SELECT * FROM "{t}"')
        cols = [d[0] for d in cur.description]
        rows = cur.fetchall()
        frames[t] = pl.DataFrame(
            {c: [r[i] for r in rows] for i, c in enumerate(cols)},
            infer_schema_length=None)
    con.close()
    return frames


def key(record):
    return (record["child_table"], record["child_column"],
            record["parent_table"], record["parent_column"])


def chinook_eval(frames):
    import onceover

    t0 = time.perf_counter()
    report = onceover.profile_many(frames)
    seconds = time.perf_counter() - t0
    strong = [c for c in report.candidates if c["tier"] == "strong"]
    weak = [c for c in report.candidates if c["tier"] == "weak"]
    found = {key(c) for c in report.candidates}
    strong_keys = {key(c) for c in strong}
    support = [c for c in report.candidates
               if key(c) == ("Customer", "SupportRepId", "Employee", "EmployeeId")]
    print(f"chinook: {seconds:.2f}s candidates={len(report.candidates)} "
          f"strong={len(strong)} weak={len(weak)} overlaps={len(report.overlaps)}")
    print(f"  true FKs found: {len(found & TRUE_FK)}/10; "
          f"strong precision: {len(strong_keys & TRUE_FK)}/{len(strong)}")
    print(f"  SupportRepId -> EmployeeId: "
          f"{support[0]['tier'] if support else 'ABSENT'} "
          f"(chance {support[0]['chance_score']:.3f})" if support else
          "  SupportRepId -> EmployeeId: ABSENT")
    weak_chance1 = sum(1 for c in weak if c["chance_score"] >= 0.99)
    print(f"  weak candidates with chance >= 0.99: {weak_chance1}/{len(weak)}")
    low = sorted(weak, key=lambda c: c["chance_score"])[:2]
    for c in low:
        print(f"  lowest-chance weak: {c['child_table']}.{c['child_column']} -> "
              f"{c['parent_table']}.{c['parent_column']} chance={c['chance_score']:.3g}")
    pairs = report.coverage["pairs"]
    print(f"  pairs: {pairs}")
    ok = (
        len(report.candidates) == 112
        and len(strong) == 9
        and strong_keys <= TRUE_FK
        and len(found & TRUE_FK) == 10
        and support and support[0]["tier"] == "weak"
    )
    print(f"  shape assertions: {'PASS' if ok else 'FAIL'}")
    return ok


def planted_orphans(frames):
    import onceover

    inv = frames["Invoice"]
    ids = inv["CustomerId"].to_list()
    ids[:3] = [9999, 9999, 9999]
    invoices = inv.with_columns(pl.Series("CustomerId", ids))
    report = onceover.profile_many(
        {"Invoice": invoices, "Customer": frames["Customer"]})
    record = next(
        (o for o in report.overlaps
         if key(o) == ("Invoice", "CustomerId", "Customer", "CustomerId")), None)
    cand = next(
        (c for c in report.candidates
         if key(c) == ("Invoice", "CustomerId", "Customer", "CustomerId")), None)
    print(f"planted orphans: candidate={cand is not None} overlap="
          f"{None if record is None else (record['orphan_rows'], record['orphan_distinct'], record['orphan_examples'])}")
    ok = (cand is None and record is not None and record["orphan_rows"] == 3
          and record["orphan_distinct"] == 1 and record["orphan_examples"] == [9999])
    print(f"  shape assertions: {'PASS' if ok else 'FAIL'}")
    return ok


def planted_chance():
    import onceover

    grades = pl.DataFrame({"grade": list(range(1, 51))})
    events = pl.DataFrame({"tier_code": [i % 40 + 1 for i in range(500)]})
    report = onceover.profile_many({"grades": grades, "events": events})
    record = next(
        (c for c in report.candidates
         if key(c) == ("events", "tier_code", "grades", "grade")), None)
    print(f"planted chance containment: "
          f"{None if record is None else (record['tier'], round(record['chance_score'], 4))}")
    ok = record is not None and record["tier"] == "weak" and record["chance_score"] >= 0.99
    print(f"  shape assertions: {'PASS' if ok else 'FAIL'}")
    return ok


def scale_cases():
    import numpy as np

    import onceover

    rng = np.random.default_rng(7)
    tables = {}
    for ti, n in enumerate([1_000_000, 400_000, 250_000, 120_000, 60_000, 30_000]):
        tables[f"t{ti}"] = pl.DataFrame(
            {f"c{ci}": rng.integers(1, 500_001, size=n) for ci in range(10)})
    t0 = time.perf_counter()
    report = onceover.profile_many(tables)
    print(f"scale random-values (6 tables x 10 int cols, largest 1M rows): "
          f"{time.perf_counter() - t0:.1f}s candidates={len(report.candidates)} "
          f"pairs={report.coverage['pairs']}")

    rng = np.random.default_rng(11)
    tables = {}
    for ti in range(6):
        tables[f"s{ti}"] = pl.DataFrame(
            {f"c{ci}": rng.choice(1_000_000, size=800_000, replace=False) + 1
             for ci in range(10)})
    t0 = time.perf_counter()
    report = onceover.profile_many(tables)
    print(f"scale shared-space (6 x 10 unique cols, 800k of 1M shared space): "
          f"{time.perf_counter() - t0:.1f}s candidates={len(report.candidates)} "
          f"pairs={report.coverage['pairs']}")

    rng = np.random.default_rng(13)
    tables = {"parent": pl.DataFrame(
        {"id": np.arange(1, 1_000_001), "x": rng.random(1_000_000)})}
    for ti in range(5):
        tables[f"child{ti}"] = pl.DataFrame({
            "id": rng.choice(1_000_000, size=900_000, replace=True) + 1,
            "y": rng.random(900_000)})
    t0 = time.perf_counter()
    report = onceover.profile_many(tables)
    strong = sum(1 for c in report.candidates if c["tier"] == "strong")
    print(f"scale true-keys (1M-key parent, 5 children of 900k rows): "
          f"{time.perf_counter() - t0:.1f}s candidates={len(report.candidates)} "
          f"strong={strong} pairs={report.coverage['pairs']}")


def main():
    frames = load_chinook()
    ok = chinook_eval(frames)
    ok = planted_orphans(frames) and ok
    ok = planted_chance() and ok
    if "--scale-only" not in sys.argv:
        scale_cases()
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
