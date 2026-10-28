"""Period-split associations (issue: relationships by period).

A pooled correlation can hide a sign flip or a large change in
strength between periods. The `period_associations` detector recomputes
pairwise Pearson correlations within the periods of a date (or `year`)
column and reports the pairs that change materially.
"""
import datetime

import polars as pl

import onceover


def flip_frame():
    rows = []
    for year, sign in ((2020, 1.0), (2021, -1.0)):
        for i in range(200):
            rows.append(
                {
                    "when": datetime.date(year, 1, 1)
                    + datetime.timedelta(days=i % 200),
                    "x": float(i),
                    "y": sign * float(i),
                }
            )
    return pl.DataFrame(rows)


def period_findings(report):
    return [f for f in report.findings if f.get("type") == "period_association"]


def test_sign_flip_across_years_is_reported():
    df = flip_frame()
    report = onceover.profile(df, budget="standard")
    hits = period_findings(report)
    assert len(hits) == 1
    f = hits[0]
    definition = f["specification"]["definition"]
    assert definition["period_column"] == "when"
    assert definition["periods"] == ["2020", "2021"]
    ev = [
        e for e in f["evidence"] if e["method"]["detector"] == "period_associations"
    ][0]
    m = ev["metrics"]
    assert abs(m["delta_r"] - 2.0) < 0.01
    assert {p["label"] for p in m["periods"]} == {"2020", "2021"}
    # The reported per-period coefficients are checkable from the frame.
    for p in m["periods"]:
        sub = df.filter(pl.col("when").dt.year() == int(p["label"]))
        r = sub.select(pl.corr("x", "y")).item()
        assert abs(r - p["pearson"]) < 0.01


def test_stable_relationship_is_not_flagged():
    rows = []
    for year in (2020, 2021):
        for i in range(200):
            rows.append(
                {
                    "when": datetime.date(year, 1, 1)
                    + datetime.timedelta(days=i % 200),
                    "x": float(i),
                    "y": 2.0 * i + (i % 3),
                }
            )
    report = onceover.profile(pl.DataFrame(rows), budget="standard")
    assert period_findings(report) == []


def test_integer_year_column_defines_periods():
    rows = []
    for year, sign in ((2019, 1.0), (2020, -1.0)):
        for i in range(200):
            rows.append({"year": year, "x": float(i), "y": sign * float(i)})
    report = onceover.profile(pl.DataFrame(rows), budget="standard")
    hits = period_findings(report)
    assert len(hits) == 1
    definition = hits[0]["specification"]["definition"]
    assert definition["period_column"] == "year"
    assert definition["periods"] == ["2019", "2020"]


def test_detector_is_selectable_alone():
    report = onceover.profile(
        flip_frame(), budget="standard", detectors=["period_associations"]
    )
    assert len(period_findings(report)) == 1
    assert report.coverage["period_associations"]["status"] == "ran_exact"
