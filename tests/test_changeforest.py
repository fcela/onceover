"""The opt-in changeforest detector: planted distribution changes.

The breaks detector fits two-segment models of one series at a
time, so it has no statement for a change that leaves every
single-series summary intact. The changeforest detector segments
the measures jointly with a random-forest classifier gain
(Londschien, Bühlmann & Kovács, JMLR 24 (2023)) and states a
`distribution_change` where the joint distribution changes, kept
when the permutation p-value is at most 0.01. The fixtures are
generated with the standard library only, so the tests also run
on the no-numpy wheel leg.
"""

import datetime as dt
import math
import random

import polars as pl

import onceover

START = dt.date(2024, 1, 1)
N = 600


def _days(n):
    return [START + dt.timedelta(days=i) for i in range(n)]


def _of_type(report, type_):
    return [f for f in report.findings if f.get("type") == type_]


def _metrics(finding):
    return finding["evidence"][0]["metrics"]


def _covariance_frame():
    # The paper's change-in-covariance setup: correlation 0, then
    # 0.7, then 0 across five coordinates; every coordinate keeps
    # mean 0 and variance 1 throughout, so no single-series level,
    # trend, or variance statement describes the change.
    rng = random.Random(260)
    measures = {f"m{j}": [] for j in range(5)}
    for r in range(N):
        rho = 0.7 if 200 <= r < 400 else 0.0
        common = rng.gauss(0.0, 1.0)
        for j in range(5):
            own = rng.gauss(0.0, 1.0)
            measures[f"m{j}"].append(
                math.sqrt(rho) * common + math.sqrt(1.0 - rho) * own
            )
    return pl.DataFrame({"date": _days(N), **measures})


def test_covariance_shift_states_distribution_changes():
    report = onceover.profile(_covariance_frame(), detectors=["changeforest"])
    assert report.coverage["changeforest"]["status"] == "ran_exact"
    changes = _of_type(report, "distribution_change")
    splits = sorted(_metrics(f)["split_row"] for f in changes)
    # Both planted splits must be stated, within tolerance. The
    # count is not asserted exactly: at a per-segment permutation
    # bar of 0.01 a further split can be stated on a remainder
    # segment (see the benchmark record).
    assert any(185 <= s <= 215 for s in splits), f"no split near 200: {splits}"
    assert any(385 <= s <= 415 for s in splits), f"no split near 400: {splits}"
    for f in changes:
        assert f["category"] == "changes"
        m = _metrics(f)
        assert m["p_value"] <= 0.01
        assert m["measures"] == ["m0", "m1", "m2", "m3", "m4"]
        assert f["facets"]["temporality"] == "change_across_segments"
    near = [f for f in changes if _metrics(f)["split_row"] <= 215]
    assert len(_metrics(near[0])["segments"]) >= 3


def test_shape_change_with_level_and_variance_held():
    # Fold the second half's noise to its absolute value and
    # re-centre and re-scale it to the same mean and variance: the
    # level and the variance do not move; the shape does. The
    # series runs to 1,200 rows: at 300 rows a side this contrast
    # is at the edge of the method's power at the 0.01 bar, and at
    # 600 a side it is stated on every probed draw (see the
    # benchmark record).
    n = 1200
    rng = random.Random(261)
    raw = [rng.gauss(0.0, 1.0) for _ in range(n)]
    folded = [abs(v) for v in raw]
    mean = sum(folded) / n
    var = sum((v - mean) ** 2 for v in folded) / n
    series = raw[:600] + [(v - mean) / math.sqrt(var) for v in folded[600:]]
    df = pl.DataFrame({"date": _days(n), "y": series})
    report = onceover.profile(df, detectors=["changeforest"])
    changes = _of_type(report, "distribution_change")
    splits = sorted(_metrics(f)["split_row"] for f in changes)
    assert any(570 <= s <= 630 for s in splits), f"no split near 600: {splits}"


def test_mean_shift_control_breaks_still_states_level_shift():
    rng = random.Random(262)
    y = [rng.gauss(0.0, 1.0) for _ in range(N)]
    y = [v + (3.0 if i >= 300 else 0.0) for i, v in enumerate(y)]
    df = pl.DataFrame({"date": _days(N), "y": y})
    report = onceover.profile(df, detectors=["breaks", "changeforest"])
    assert _of_type(report, "level_shift"), "the breaks control regressed"


def test_no_change_control_states_nothing():
    rng = random.Random(263)
    df = pl.DataFrame(
        {
            "date": _days(N),
            **{f"m{j}": [rng.gauss(0.0, 1.0) for _ in range(N)] for j in range(3)},
        }
    )
    report = onceover.profile(df, detectors=["changeforest"])
    assert _of_type(report, "distribution_change") == []


def test_two_profiles_produce_identical_findings():
    df = _covariance_frame()
    a = onceover.profile(df, detectors=["changeforest"])
    b = onceover.profile(df, detectors=["changeforest"])
    assert a.findings == b.findings


def test_default_run_records_changeforest_as_opt_in():
    report = onceover.profile(_covariance_frame())
    entry = report.coverage["changeforest"]
    assert entry["status"] == "skipped"
    assert "opt-in" in entry["detail"]
    assert _of_type(report, "distribution_change") == []


def test_verify_marks_distribution_change_not_checkable():
    report = onceover.profile(_covariance_frame(), detectors=["changeforest"])
    changes = _of_type(report, "distribution_change")
    assert changes
    checks = report.verify(_covariance_frame())
    rows = checks.filter(pl.col("id") == changes[0]["id"])
    assert rows.height == 1
    assert rows["status"][0] == "not_checkable"
