"""Target-aware shape decomposition (issue #43): profile(df,
target="y") adds the target_shapes pass; without a target the
profile is unchanged."""

import random

import polars as pl

import onceover


def shape_findings(df, **kwargs):
    r = onceover.profile(df, budget="standard", **kwargs)
    return r, [
        f
        for f in r.findings
        if f["specification"].get("form") in ("step_shape", "curved_shape")
    ]


def grid_frame(**cols):
    return pl.DataFrame(cols)


def _step_frame(rng, n=1200):
    x = [rng.uniform(0, 10) for _ in range(n)]
    y = [(1.0 if xi < 4.0 else 3.0) + 0.1 * rng.gauss(0, 1) for xi in x]
    return grid_frame(x=x, y=y)


def test_step_predictor_fires_with_located_cut():
    rng = random.Random(11)
    df = _step_frame(rng)
    _, out = shape_findings(df, target="y")
    steps = [f for f in out if f["specification"]["form"] == "step_shape"]
    assert steps, "expected a step_shape finding"
    f = steps[0]
    assert f["subject"]["inputs"] == ["x"]
    assert f["subject"]["target"] == "y"
    m = f["evidence"][0]["metrics"]
    assert abs(m["step_at"] - 4.0) < 0.5
    assert m["step_r2"] > 0.8


def test_monotone_curve_fires_curved():
    rng = random.Random(12)
    n = 1200
    x = [rng.uniform(-1, 1) for _ in range(n)]
    y = [xi**3 + 0.05 * rng.gauss(0, 1) for xi in x]
    df = grid_frame(x=x, y=y)
    _, out = shape_findings(df, target="y")
    curved = [f for f in out if f["specification"]["form"] == "curved_shape"]
    assert curved, "expected a curved_shape finding"
    m = curved[0]["evidence"][0]["metrics"]
    assert m["curvature"] >= 0.05
    assert m["spearman"] > 0.9


def test_linear_and_null_predictors_are_silent():
    rng = random.Random(13)
    n = 1200
    x = [rng.uniform(-1, 1) for _ in range(n)]
    z = [rng.uniform(-1, 1) for _ in range(n)]
    y = [2.0 * xi + 0.1 * rng.gauss(0, 1) for xi in x]
    df = grid_frame(x=x, z=z, y=y)
    _, out = shape_findings(df, target="y")
    assert out == []


def test_no_target_means_no_shapes_and_pass_skipped():
    rng = random.Random(11)
    df = _step_frame(rng)
    r, out = shape_findings(df)
    assert out == []
    assert r.coverage["target_shapes"]["status"] == "skipped"


def test_unknown_target_is_coverage_not_error():
    df = grid_frame(x=[1.0, 2.0, 3.0], y=[1.0, 2.0, 3.0])
    r = onceover.profile(df, target="nope")
    assert "not a column" in r.coverage["target_shapes"]["detail"]


def test_explicit_detector_list_without_target_shapes_stays_off():
    rng = random.Random(11)
    df = _step_frame(rng)
    _, out = shape_findings(df, target="y", detectors=["gbm"])
    assert out == []
