"""Per-period coverage on the frame's time axis (issue #203).

The report used to state declared-panel missing cells but never
how much data each observed period carries, so a partial final
week was invisible. The panel_coverage pass counts rows (and
distinct entities, where the frame model provides an entity axis)
per period on the assembled axis and flags periods far below the
typical period, with their position in the statement.
"""

import datetime as dt

import polars as pl

import onceover

PERSONS = [f"p{i:02d}" for i in range(40)]
WEEKS = [dt.date(2024, 1, 1) + dt.timedelta(days=7 * k) for k in range(10)]


def _panel(dip_week=None, final_count=None, entity="person"):
    rows = []
    for wi, w in enumerate(WEEKS):
        present = PERSONS
        if wi == dip_week:
            present = PERSONS[:16]
        if wi == len(WEEKS) - 1 and final_count is not None:
            present = PERSONS[:final_count]
        for p in present:
            rows.append({entity: p, "week": w, "tokens": 100})
    return pl.DataFrame(rows)


def _partial(report):
    return report.select(category="gaps", type="partial_period")


def test_truncated_final_and_interior_dip_are_flagged():
    report = onceover.profile(_panel(dip_week=4, final_count=24))
    hits = _partial(report)
    assert len(hits) == 2
    by_position = {f["evidence"][0]["metrics"]["position"]: f for f in hits}
    assert set(by_position) == {"interior", "last"}

    last = by_position["last"]
    m = last["evidence"][0]["metrics"]
    assert m["period"] == "2024-03-04"
    assert m["rows"] == 24
    assert m["entities"] == 24
    assert m["typical_rows"] == 40
    assert m["typical_entities"] == 40
    assert "final period" in last["claim"]
    assert "2024-03-04" in last["claim"]

    interior = by_position["interior"]
    m = interior["evidence"][0]["metrics"]
    assert m["period"] == "2024-01-29"
    assert m["rows"] == 16
    assert "interior period" in interior["claim"]
    # The per-period table is in the evidence.
    periods = m["periods"]
    assert len(periods) == 10
    assert {p["period"]: p["rows"] for p in periods}["2024-03-04"] == 24

    entry = report.coverage["panel_coverage"]
    assert entry["status"] == "ran_exact"
    assert "2 period(s) below 75%" in entry["detail"]


def test_uniform_panel_is_quiet():
    report = onceover.profile(_panel())
    assert _partial(report) == []
    entry = report.coverage["panel_coverage"]
    assert entry["status"] == "ran_exact"
    assert "no period below 75%" in entry["detail"]


def test_frame_without_time_axis_records_the_skip():
    df = pl.DataFrame({"x": [1.0, 2.0, 3.0, 4.0], "y": [5.0, 6.0, 7.0, 8.0]})
    report = onceover.profile(df)
    entry = report.coverage["panel_coverage"]
    assert entry["status"] == "skipped"
    assert "no time axis" in entry["detail"]


def test_integer_week_axis_via_hint():
    rows = []
    for w in range(1, 11):
        present = PERSONS if w < 10 else PERSONS[:20]
        for p in present:
            rows.append({"person": p, "week_no": w, "tokens": 3})
    df = pl.DataFrame(rows)
    report = onceover.profile(df, hints={"time": "week_no"})
    hits = _partial(report)
    assert len(hits) == 1
    m = hits[0]["evidence"][0]["metrics"]
    assert m["position"] == "last"
    assert m["period"] == "10"
    assert m["rows"] == 20


def test_declared_panel_owns_entity_completeness():
    # With the entity axis declared as dimensions, the declared
    # panel check owns entity completeness; per-period coverage
    # still reports row volume but does not restate entities.
    report = onceover.profile(
        _panel(dip_week=4, final_count=24),
        hints={"dimensions": ["person"], "time": "week"},
    )
    hits = _partial(report)
    assert hits, "row-volume shortfall is still reported"
    for f in hits:
        assert "entities" not in f["evidence"][0]["metrics"]
    entry = report.coverage["panel_coverage"]
    assert "declared panel check" in entry["detail"]


def test_too_few_periods_states_no_typical():
    rows = []
    for w in WEEKS[:3]:
        for p in PERSONS:
            rows.append({"person": p, "week": w, "tokens": 1})
    report = onceover.profile(pl.DataFrame(rows))
    assert _partial(report) == []
    entry = report.coverage["panel_coverage"]
    assert entry["status"] == "ran_exact"
    assert "at least 4 periods" in entry["detail"]
