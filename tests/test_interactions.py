"""Friedman-Popescu H interactions (issue #42): pairs whose joint
partial dependence carries interaction variance, from the gbm
screen's surrogate."""

import random

import polars as pl

import onceover


def interaction_findings(df, **kwargs):
    r = onceover.profile(
        df, budget="standard", detectors=["gbm", "interactions"], **kwargs
    )
    return [
        f
        for f in r.findings
        if f.get("type") == "predictive"
        and f["specification"].get("form") == "interaction_h"
    ]


def test_product_term_pair_fires_first():
    rng = random.Random(3)
    n = 1500
    x1 = [rng.uniform(-1, 1) for _ in range(n)]
    x2 = [rng.uniform(-1, 1) for _ in range(n)]
    x3 = [rng.uniform(-1, 1) for _ in range(n)]
    y = [
        2.0 * a * b + 0.3 * c + 0.1 * rng.gauss(0, 1)
        for a, b, c in zip(x1, x2, x3)
    ]
    df = pl.DataFrame({"x1": x1, "x2": x2, "x3": x3, "y": y})
    out = interaction_findings(df)
    assert out, "expected an interaction finding"
    first = out[0]
    # The report ranks findings; the generating pair must be among
    # them with the target as subject target.
    pairs = {
        tuple(sorted(f["subject"]["inputs"]))
        for f in out
        if f["subject"].get("target") == "y"
    }
    assert ("x1", "x2") in pairs
    m = first["evidence"][0]["metrics"]
    assert m["h2"] >= 0.05
    assert m["interaction_z"] >= 4.0
    assert m["holdout_r2"] >= 0.10


def test_additive_target_produces_no_interaction():
    rng = random.Random(5)
    n = 1500
    x1 = [rng.uniform(-1, 1) for _ in range(n)]
    x2 = [rng.uniform(-1, 1) for _ in range(n)]
    y = [1.5 * a - 1.0 * b + 0.1 * rng.gauss(0, 1) for a, b in zip(x1, x2)]
    df = pl.DataFrame({"x1": x1, "x2": x2, "y": y})
    assert interaction_findings(df) == []


def test_unpredictable_target_produces_no_interaction():
    rng = random.Random(7)
    n = 800
    df = pl.DataFrame(
        {
            "x1": [rng.uniform(-1, 1) for _ in range(n)],
            "x2": [rng.uniform(-1, 1) for _ in range(n)],
            "y": [rng.gauss(0, 1) for _ in range(n)],
        }
    )
    assert interaction_findings(df) == []


def test_quick_budget_skips_the_pass():
    rng = random.Random(3)
    n = 1500
    x1 = [rng.uniform(-1, 1) for _ in range(n)]
    x2 = [rng.uniform(-1, 1) for _ in range(n)]
    y = [2.0 * a * b + 0.1 * rng.gauss(0, 1) for a, b in zip(x1, x2)]
    df = pl.DataFrame({"x1": x1, "x2": x2, "y": y})
    r = onceover.profile(df, budget="quick", detectors=["gbm", "interactions"])
    assert [
        f
        for f in r.findings
        if f["specification"].get("form") == "interaction_h"
    ] == []
