"""Transient events (issue #32): short excursions from a local
baseline in time-ordered series, reported as changes findings whose
evidence carries the event window."""
import datetime

import polars as pl

import onceover


def events(report):
    return [f for f in report.findings if f.get("type") == "transient_excursion"]


def frame(values, start=datetime.datetime(2024, 1, 1), step_minutes=5):
    # Build the timestamp column with datetime_range (native
    # construction). A list of Python datetimes goes through polars'
    # sequence constructor, which fails on polars 1.x under Python
    # 3.15 ('NoneType' object has no attribute '_s' in
    # polars._utils.construction.series) — the CI Polars-1.x leg.
    end = start + datetime.timedelta(minutes=step_minutes * (len(values) - 1))
    ts = pl.datetime_range(start, end, f"{step_minutes}m", eager=True)
    return pl.DataFrame({"ts": ts, "value": [float(v) for v in values]})


def test_planted_dip_is_reported_with_window():
    vals = [100.0] * 600
    for i in range(300, 306):
        vals[i] = 40.0
    hits = events(onceover.profile(frame(vals)))
    assert len(hits) == 1
    hit = hits[0]
    assert hit["category"] == "changes"
    metrics = hit["evidence"][0]["metrics"]
    assert metrics["direction"] == "dip"
    assert metrics["peak_z"] < 0
    # 300 * 5min = 25h after 2024-01-01 00:00
    assert metrics["peak_time"].startswith("2024-01-02 01:")


def test_level_shift_is_not_an_event():
    vals = [100.0] * 300 + [160.0] * 300
    assert events(onceover.profile(frame(vals))) == []


def test_flat_series_has_no_events():
    assert events(onceover.profile(frame([7.0] * 600))) == []


def test_no_time_column_no_events():
    df = pl.DataFrame({"value": [100.0] * 300 + [1.0] * 6 + [100.0] * 294})
    assert events(onceover.profile(df)) == []
