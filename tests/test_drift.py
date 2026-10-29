"""Drift along a declared time axis (issue #50): a reference-period
model per numeric target, later periods scored by split-routing
chi-square. Engages only with hints={"time": ...}."""

import random

import polars as pl

import onceover


def _frame(shift: bool) -> pl.DataFrame:
    rng = random.Random(7)
    t, x, y = [], [], []
    for i in range(4000):
        xv = rng.uniform(-1, 1) + (2.0 if shift and i >= 2000 else 0.0)
        t.append(i)
        x.append(xv)
        y.append(xv + rng.uniform(-0.05, 0.05))
    return pl.DataFrame({"t": t, "x": x, "y": y})


def drift_findings(df, **kwargs):
    r = onceover.profile(df, budget="standard", detectors=["drift"], **kwargs)
    return [
        f
        for f in r.findings
        if f.get("category") == "changes"
        and f["specification"].get("form") == "drift"
    ]


def test_planted_shift_fires_with_time_hint():
    out = drift_findings(_frame(True), hints={"time": "t"})
    assert out, "expected a drift finding for the planted shift"
    metrics = out[0]["evidence"][0]["metrics"]
    assert metrics["time_column"] == "t"
    assert metrics["max_score"] >= 5.0
    assert metrics["periods"][-1]["drift"] is not None


def test_stable_series_is_quiet():
    assert drift_findings(_frame(False), hints={"time": "t"}) == []


def test_no_hint_no_drift():
    assert drift_findings(_frame(True)) == []


def test_unrelated_column_shift_is_quiet():
    # A shift in a column the target's model does not use must not
    # fire: the per-split chi-squares are weighted by each feature's
    # share of the reference model's total gain, and the maximum
    # must beat a label-permutation maximum.
    rng = random.Random(11)
    t, x, z, y = [], [], [], []
    for i in range(4000):
        xv = rng.uniform(-1, 1)
        t.append(i)
        x.append(xv)
        z.append(rng.uniform(0, 10) + (3.0 if i >= 2000 else 0.0))
        y.append(2 * xv + rng.gauss(0, 0.4))
    df = pl.DataFrame({"t": t, "x": x, "z": z, "y": y})
    out = drift_findings(df, hints={"time": "t"})
    targets = {f["subject"]["columns"][0] for f in out}
    assert "y" not in targets
    assert "x" not in targets
