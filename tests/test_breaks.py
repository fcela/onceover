"""Breaks findings carry a located range for the changepoint (#210).

The point estimate alone states a level shift as if its location
were known exactly. Each breaks finding now also carries the range
of periods the detection scan's own cost curve cannot separate
from the best split: splits whose two-segment squared error lies
within 3.84 residual variances of the best split's.
"""
from datetime import date, timedelta

import polars as pl
import pytest

import onceover

np = pytest.importorskip("numpy", reason="planted frames need numpy")

N = 400
DAYS = [date(2020, 1, 1) + timedelta(days=i) for i in range(N)]
PLANTED = 200
PLANTED_DAY = (date(2020, 7, 19) - date(1970, 1, 1)).days  # DAYS[200], epoch days


def _of_type(report, type_):
    return [f for f in report.findings if f.get("type") == type_]


def _metrics(finding):
    return finding["evidence"][0]["metrics"]


def test_level_shift_range_contains_the_planted_date():
    # Three rows per day, so the training partition keeps rows on
    # the planted day itself: the split the scan can choose from
    # exists at day granularity. The shift is moderate (about 1.7
    # noise standard deviations), so the location is estimated
    # tightly but the located range still spans several days.
    rng = np.random.default_rng(5)
    days = [d for d in DAYS for _ in range(3)]
    y = rng.normal(scale=1.2, size=len(days))
    y[3 * PLANTED:] += 2.0
    report = onceover.profile(pl.DataFrame({"t": days, "y": y}), detectors=["breaks"])
    shifts = _of_type(report, "level_shift")
    assert len(shifts) == 1
    m = _metrics(shifts[0])
    assert m["breakpoint_range_start_raw"] <= PLANTED_DAY <= m["breakpoint_range_end_raw"]
    assert m["breakpoint_range_start"] <= "2020-07-19" <= m["breakpoint_range_end"]
    claim = shifts[0]["claim"]
    assert "between" in claim and "best estimate" in claim


def test_weakly_located_shift_reports_a_wide_range():
    # One row per day and a weak shift: the evidence locates the
    # shift only loosely, and the finding must say so with a range
    # spanning several days rather than a single confident date.
    rng = np.random.default_rng(5)
    y = rng.normal(scale=1.2, size=N)
    y[PLANTED:] += 1.5
    report = onceover.profile(pl.DataFrame({"t": DAYS, "y": y}), detectors=["breaks"])
    shifts = _of_type(report, "level_shift")
    assert len(shifts) == 1
    m = _metrics(shifts[0])
    assert m["breakpoint_range_end_raw"] - m["breakpoint_range_start_raw"] >= 2


def test_every_breaks_finding_carries_a_range_around_its_estimate():
    rng = np.random.default_rng(5)
    y = rng.normal(scale=1.2, size=N)
    y[PLANTED:] += 4.0
    report = onceover.profile(pl.DataFrame({"t": DAYS, "y": y}), detectors=["breaks"])
    breaks = [
        f
        for f in report.findings
        if f.get("type") in ("level_shift", "trend_change", "variance_change")
    ]
    assert breaks
    for f in breaks:
        m = _metrics(f)
        assert m["breakpoint_range_start_raw"] <= m["breakpoint_raw"]
        assert m["breakpoint_raw"] <= m["breakpoint_range_end_raw"]
