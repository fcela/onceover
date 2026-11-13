"""Predictable missingness (#274): a column's gaps follow a rule a
model can find, stated only when held-out data certifies it.

The planted frames use plain Python randomness so the fixtures run
in the wheel-test environment as well.
"""
import random

import polars as pl

import onceover


def _hits(report, column=None):
    out = [
        f
        for f in report.findings
        if f["type"] == "predictable_missingness"
    ]
    if column is not None:
        out = [f for f in out if f["subject"]["columns"] == [column]]
    return out


def _structural(n=6000, seed=11):
    rng = random.Random(seed)
    channel, agent, x = [], [], []
    for _ in range(n):
        c = "direct" if rng.random() < 0.4 else "agency"
        channel.append(c)
        agent.append(None if c == "direct" else float(rng.randint(1, 500)))
        x.append(rng.gauss(0.0, 1.0))
    return pl.DataFrame({"channel": channel, "agent": agent, "x": x})


def test_structural_missingness_is_stated_with_its_driver():
    report = onceover.profile(_structural())
    hits = _hits(report, "agent")
    assert len(hits) == 1
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["auc"] >= 0.99
    assert metrics["drivers"] == ["channel"]
    assert metrics["missing"] == 2474
    assert "holdout AUC" in hits[0]["claim"]


def test_missing_at_random_is_not_stated():
    rng = random.Random(11)
    n = 6000
    frame = pl.DataFrame(
        {
            "x": [rng.gauss(0.0, 1.0) for _ in range(n)],
            "z": [rng.gauss(0.0, 1.0) for _ in range(n)],
            "y": [
                None if rng.random() < 0.3 else rng.gauss(0.0, 1.0)
                for _ in range(n)
            ],
        }
    )
    report = onceover.profile(frame)
    assert _hits(report) == []


def test_conjunction_of_levels_is_stated():
    rng = random.Random(11)
    n = 6000
    region, tier, w, x = [], [], [], []
    for _ in range(n):
        r = rng.choice(["north", "south"])
        t = rng.choice(["gold", "basic"])
        region.append(r)
        tier.append(t)
        w.append(None if (r == "south" and t == "gold") else rng.gauss(5.0, 1.0))
        x.append(rng.gauss(0.0, 1.0))
    frame = pl.DataFrame({"region": region, "tier": tier, "w": w, "x": x})
    report = onceover.profile(frame)
    hits = _hits(report, "w")
    assert len(hits) == 1
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["auc"] >= 0.99
    assert set(metrics["drivers"]) == {"region", "tier"}


def test_quick_budget_does_not_run_the_screen():
    report = onceover.profile(_structural(), budget="quick")
    assert _hits(report) == []
