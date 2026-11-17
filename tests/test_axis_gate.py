"""The default-set axis gate (issue #317, phase 3).

In a default run the along-axis passes — breaks, ordered_partitions,
ordered_axis, discontinuities — scan along the frame's time axis or
an axis declared with the `ordered_axis` hint. Along any other
numeric axis they do not run by default; naming a detector
explicitly, or declaring its axis, runs it. A gated pass records the
skip in its coverage entry.
"""

import datetime as dt
import random

import polars as pl

import onceover

FAMILY = ("breaks", "ordered_partitions", "ordered_axis", "discontinuities")


def _by_detector(report, detector):
    return [
        f
        for f in report.findings
        if f["evidence"][0]["method"]["detector"] == detector
    ]


def _time_frame() -> pl.DataFrame:
    rng = random.Random(7)
    days = [dt.date(2024, 1, 1) + dt.timedelta(days=i) for i in range(400)]
    y = [(3.0 if i >= 200 else 0.0) + rng.gauss(0, 0.5) for i in range(400)]
    return pl.DataFrame({"day": days, "y": y})


def _dose_frame() -> pl.DataFrame:
    rng = random.Random(0)
    n = 900
    x = [rng.random() for _ in range(n)]
    y = [(2.0 if xi > 0.6 else 0.0) + 1.5 * xi + 0.3 * rng.gauss(0, 1) for xi in x]
    return pl.DataFrame(
        {"dose": x, "y": y, "w": [rng.gauss(0, 1) for _ in range(n)]}
    )


def test_time_axis_frame_states_its_breaks_by_default():
    report = onceover.profile(_time_frame())
    breaks = _by_detector(report, "breaks")
    assert any(f["type"] == "level_shift" for f in breaks)
    assert report.coverage["breaks"]["status"] != "skipped"


def test_numeric_axis_only_frame_is_gated_by_default():
    report = onceover.profile(_dose_frame())
    for detector in FAMILY:
        assert _by_detector(report, detector) == [], detector
        entry = report.coverage[detector]
        assert entry["status"] == "skipped", (detector, entry)
        assert "ordered_axis" in entry["detail"], (detector, entry)


def test_explicit_selection_runs_the_gated_passes():
    df = _dose_frame()
    report = onceover.profile(df, detectors=["discontinuities"])
    steps = [
        f for f in _by_detector(report, "discontinuities")
        if f["type"] == "step_discontinuity"
    ]
    assert steps and steps[0]["subject"]["columns"] == ["dose", "y"]
    report = onceover.profile(df, detectors=["ordered_axis"])
    assert _by_detector(report, "ordered_axis")
    assert report.coverage["ordered_axis"]["status"] != "skipped"


def test_explicit_breaks_scan_a_sequence_axis():
    rng = random.Random(3)
    n = 600
    y = [
        (0.02 * i if i < 300 else 6.0 + 0.002 * (i - 300)) + rng.gauss(0, 0.4)
        for i in range(n)
    ]
    df = pl.DataFrame({"t": list(range(n)), "y": y})
    default = onceover.profile(df)
    assert _by_detector(default, "breaks") == []
    assert default.coverage["breaks"]["status"] == "skipped"
    report = onceover.profile(df, detectors=["breaks"])
    assert _by_detector(report, "breaks")


def test_declared_axis_opens_the_gate():
    df = _dose_frame()
    report = onceover.profile(df, hints={"ordered_axis": "dose"})
    steps = [
        f for f in _by_detector(report, "discontinuities")
        if f["type"] == "step_discontinuity"
    ]
    assert steps and steps[0]["subject"]["columns"] == ["dose", "y"]
    assert report.coverage["discontinuities"]["status"] != "skipped"
    assert report.coverage["ordered_axis"]["status"] != "skipped"


def test_declared_integer_axis_carries_the_break_scan():
    rng = random.Random(11)
    steps = [s for s in range(1, 61) for _ in range(20)]
    y = [(2.5 if s >= 30 else 0.0) + rng.gauss(0, 0.5) for s in steps]
    df = pl.DataFrame({"step": steps, "y": y})
    gated = onceover.profile(df)
    assert _by_detector(gated, "breaks") == []
    assert gated.coverage["breaks"]["status"] == "skipped"
    report = onceover.profile(df, hints={"ordered_axis": "step"})
    breaks = _by_detector(report, "breaks")
    assert any(
        f["type"] == "level_shift" and f["subject"]["columns"] == ["step", "y"]
        for f in breaks
    )
