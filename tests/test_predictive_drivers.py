"""Predictive findings name their drivers (issue #127).

The predictability screen's claim used to list every input the
model split on, so one real driver arrived with a tail of noise
columns that happened to split once. The claim now names the
driver shortlist derived from the model's own gain shares, and —
when the shortlist is a proper subset of the inputs — reports the
holdout score of a model refit on the drivers alone next to the
full model's score. These tests pin the three acceptance probes:
a single-driver plant names that driver alone, a multi-driver
plant names all of its drivers and no noise column, and a pure
noise target produces no predictive claim. A fourth test pins
that a column roled as scaffolding is not used as a predictor.
"""

import random

import polars as pl

import onceover


def _predictive(report, target):
    return [f for f in report.data["findings"]
            if f["type"] == "predictive"
            and f["subject"].get("target") == target]


def _metrics(finding):
    return finding["evidence"][0]["metrics"]


def _single_driver_frame(n=5000, seed=20261104):
    rng = random.Random(seed)
    cols = {f"f_num{i}": [rng.gauss(0.0, 1.0) for _ in range(n)]
            for i in range(17)}
    for i in range(7):
        cols[f"f_cat{i}"] = [rng.choice("abcd") for _ in range(n)]
    xs = [rng.random() for _ in range(n)]
    cols["p_x"] = xs
    cols["p_y"] = [x * x + rng.gauss(0.0, 0.05) for x in xs]
    return pl.DataFrame(cols)


def test_single_driver_claim_names_only_the_driver():
    report = onceover.profile(_single_driver_frame(), detectors=["gbm"])
    found = _predictive(report, "p_y")
    assert len(found) == 1
    m = _metrics(found[0])
    assert m["drivers"] == ["p_x"]
    # The full pool stays in evidence; the claim names the driver
    # and reports the refit score first.
    assert len(m["inputs"]) > 5
    assert found[0]["claim"].startswith(
        "p_y is predictable from p_x (")
    assert "using only these drivers" in found[0]["claim"]
    assert m["reduced_metric"] is not None
    # The refit on p_x alone keeps nearly all of the full score.
    assert m["reduced_metric"] > m["metric"] - 0.1


def _multi_driver_frame(n=5000, seed=7):
    rng = random.Random(seed)
    x1 = [rng.gauss(0.0, 1.0) for _ in range(n)]
    x2 = [rng.gauss(0.0, 1.0) for _ in range(n)]
    x3 = [rng.gauss(0.0, 1.0) for _ in range(n)]
    cols = {"x1": x1, "x2": x2, "x3": x3}
    for i in range(10):
        cols[f"noise{i}"] = [rng.gauss(0.0, 1.0) for _ in range(n)]
    cols["y"] = [a + 2.0 * b + c + rng.gauss(0.0, 0.1)
                 for a, b, c in zip(x1, x2, x3)]
    return pl.DataFrame(cols)


def test_multi_driver_claim_names_all_drivers_and_no_noise():
    report = onceover.profile(_multi_driver_frame(), detectors=["gbm"])
    found = _predictive(report, "y")
    assert len(found) == 1
    m = _metrics(found[0])
    assert set(m["drivers"]) == {"x1", "x2", "x3"}
    assert not any(name.startswith("noise")
                   for name in m["drivers"])
    assert m["reduced_metric"] is not None
    assert m["reduced_metric"] > 0.9 * m["metric"]


def test_pure_noise_target_produces_no_predictive_claim():
    rng = random.Random(99)
    n = 2000
    cols = {f"n{i}": [rng.gauss(0.0, 1.0) for _ in range(n)]
            for i in range(10)}
    cols["y"] = [rng.gauss(0.0, 1.0) for _ in range(n)]
    report = onceover.profile(pl.DataFrame(cols), detectors=["gbm"])
    assert _predictive(report, "y") == []


def test_scaffolding_column_is_not_a_predictor():
    rng = random.Random(5)
    n = 3000
    x = [rng.random() for _ in range(n)]
    df = pl.DataFrame({
        "row_id": list(range(n)),
        "x": x,
        "y": [v * v + rng.gauss(0.0, 0.05) for v in x],
    })
    report = onceover.profile(df, detectors=["gbm"],
                              hints={"roles": {"row_id": "index"}})
    found = _predictive(report, "y")
    assert len(found) == 1
    m = _metrics(found[0])
    assert "row_id" not in m["inputs"]
    assert m["drivers"] == ["x"]
