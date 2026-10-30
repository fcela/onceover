"""Hotspots (issue #58): a cell of a column combination holding
far more rows than the marginals predict is reported with its
independence baseline; an independent frame with the same
marginals stays quiet. Triples are a deep-budget scan."""

import random

import polars as pl

import onceover


def _hits(report) -> list:
    return [f for f in report.findings if f.get("type") == "hotspot"]


def _planted_pair(seed: int = 11) -> pl.DataFrame:
    rng = random.Random(seed)
    rows = [("a1", "b1")] * 1000
    rest_a = ["a2", "a3", "a4", "a5"]
    rest_b = ["b2", "b3", "b4", "b5"]
    rows += [(rng.choice(rest_a), rng.choice(rest_b)) for _ in range(9000)]
    rng.shuffle(rows)
    return pl.DataFrame({"city": [r[0] for r in rows], "channel": [r[1] for r in rows]})


def _independent_same_marginals(seed: int = 12) -> pl.DataFrame:
    rng = random.Random(seed)
    a = ["a1"] * 1000 + ["a2", "a3", "a4", "a5"] * 2250
    b = ["b1"] * 1000 + ["b2", "b3", "b4", "b5"] * 2250
    rng.shuffle(a)
    rng.shuffle(b)
    return pl.DataFrame({"city": a, "channel": b})


def test_planted_cell_is_named_with_its_baseline():
    report = onceover.profile(_planted_pair())
    hits = _hits(report)
    assert hits, "expected a hotspot finding for the planted cell"
    top = hits[0]
    payload = top["evidence"][0]["metrics"]
    assert payload["cell"] == {"channel": "b1", "city": "a1"}
    assert payload["observed_rows"] == 1000
    assert 50.0 < payload["expected_rows"] < 200.0
    assert payload["ratio"] > 5.0
    assert payload["cells_examined"] >= 25


def test_independent_frame_with_same_marginals_is_quiet():
    report = onceover.profile(_independent_same_marginals())
    assert _hits(report) == []


def _planted_triple(seed: int = 13, n: int = 9000, plant: int = 1800) -> pl.DataFrame:
    rng = random.Random(seed)
    levels = ["x1", "x2", "x3"]
    rows = [(rng.choice(levels), rng.choice(levels), rng.choice(levels)) for _ in range(n)]
    for i in rng.sample(range(n), plant):
        rows[i] = ("x1", "x1", "x1")
    return pl.DataFrame(
        {"a": [r[0] for r in rows], "b": [r[1] for r in rows], "c": [r[2] for r in rows]}
    )


def test_triples_run_at_deep_budget_only():
    frame = _planted_triple()
    deep = onceover.profile(frame, budget="deep")
    triple_hits = [
        f for f in _hits(deep) if len(f["evidence"][0]["metrics"]["columns"]) == 3
    ]
    assert triple_hits, "expected a 3-column hotspot at deep budget"
    standard = onceover.profile(frame)
    assert all(
        len(f["evidence"][0]["metrics"]["columns"]) == 2 for f in _hits(standard)
    )
