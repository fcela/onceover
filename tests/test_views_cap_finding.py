"""The views column cap must be stated as a finding (issue #214).

A reader of the findings alone — never opening coverage — has to be
able to tell that the views pass did not examine part of the table.
"""

import json
import random

import polars as pl

import onceover


def _wide_frame():
    # 68 eligible columns at the 64-column cap: 64 constants, two
    # noise columns, and a dependent pair the pre-screen must keep.
    # Four columns drop past the cap.
    rng = random.Random(9)
    n = 600
    x = [rng.gauss(0, 1) for _ in range(n)]
    data = {f"constant_{i}": [float(i)] * n for i in range(64)}
    data.update({
        "noise_a": [rng.gauss(0, 1) for _ in range(n)],
        "noise_b": [rng.gauss(0, 1) for _ in range(n)],
        "late_x": x,
        "late_y": x,
    })
    return pl.DataFrame(data)


def test_views_cap_drop_is_a_finding():
    report = onceover.profile(_wide_frame(), detectors=["views"], max_findings=500)
    hits = report.select(category="gaps", type="unexamined_columns")
    assert len(hits) == 1
    hit = hits[0]
    dropped = hit["subject"]["columns"]
    assert len(dropped) == 4
    # The pre-screen keeps the dependent pair; the dropped columns
    # are named in the claim itself.
    assert "late_x" not in dropped and "late_y" not in dropped
    for name in dropped:
        assert name in hit["claim"]
    metrics = hit["evidence"][0]["metrics"]
    assert metrics["pass"] == "views"
    assert metrics["reason"] == "column_cap"
    assert metrics["cap"] == 64
    assert metrics["n_eligible"] == 68
    assert metrics["n_examined"] == 64
    assert sorted(metrics["dropped_columns"]) == dropped
    # The pre-existing coverage text remains.
    detail = report.coverage["views"]["detail"]
    assert "dropped 4 column(s) past the 64-column cap" in detail


def test_views_cap_finding_reaches_json_and_digest():
    report = onceover.profile(_wide_frame(), detectors=["views"], max_findings=500)
    parsed = json.loads(report.to_json())
    hits = [f for f in parsed["findings"] if f.get("type") == "unexamined_columns"]
    assert len(hits) == 1
    assert hits[0]["category"] == "gaps"
    digest = report.to_digest(60_000)
    assert "did not examine" in digest


def test_views_at_cap_produces_no_unexamined_finding():
    rng = random.Random(4)
    n = 300
    data = {f"c{i}": [rng.gauss(0, 1) for _ in range(n)] for i in range(10)}
    report = onceover.profile(pl.DataFrame(data), detectors=["views"], max_findings=500)
    assert not report.select(category="gaps", type="unexamined_columns")


def test_default_run_has_no_views_cap_finding():
    # Views is opt-in: a default run must not gain the finding.
    report = onceover.profile(_wide_frame(), max_findings=500)
    assert not report.select(category="gaps", type="unexamined_columns")
