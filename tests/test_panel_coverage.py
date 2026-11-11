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


def _valued_panel(dip_week=None, dip_count=None):
    # Each person's measure value varies (90 + their index), so
    # the column carries the frame model's measure role. In the
    # dip week the values collapse to an eighth.
    rows = []
    for wi, w in enumerate(WEEKS):
        present = PERSONS[:dip_count] if wi == dip_week and dip_count else PERSONS
        for pi, p in enumerate(present):
            v = (90 + pi) / 8 if wi == dip_week else 90 + pi
            rows.append({"person": p, "week": w, "tokens": v})
    return pl.DataFrame(
        rows,
        schema={"person": pl.String, "week": pl.Date, "tokens": pl.Float64},
    )


def test_volume_collapse_with_steady_counts_is_flagged():
    # Every week holds all 40 people; in week 5 the measure's
    # values collapse to an eighth, so the week's total falls to
    # 12.5% of typical while rows and entities are unchanged. The
    # row and entity counts cannot see this period; the volume
    # rule can. A full week's total is sum(90..129) = 4380; the
    # collapsed week's is 547.5.
    report = onceover.profile(_valued_panel(dip_week=5))
    hits = _partial(report)
    assert len(hits) == 1
    m = hits[0]["evidence"][0]["metrics"]
    assert m["position"] == "interior"
    assert m["period"] == "2024-02-05"
    assert m["rows"] == 40
    assert m["entities"] == 40
    assert "total tokens 547.5" in hits[0]["claim"]
    measures = m["measures"]
    assert len(measures) == 1
    assert measures[0]["column"] == "tokens"
    assert measures[0]["total"] == 547.5
    assert measures[0]["typical_total"] == 4380
    assert measures[0]["below_typical"] is True
    # Per-period totals in the evidence, against the hand-computed
    # sums above.
    totals = {p["period"]: p["totals"]["tokens"] for p in m["periods"]}
    assert totals["2024-01-01"] == 4380
    assert totals["2024-02-05"] == 547.5
    entry = report.coverage["panel_coverage"]
    assert "principal measures: total tokens typically 4380 per period" in entry["detail"]
    assert "1 period(s) below 75%" in entry["detail"]


def test_row_and_volume_shortfall_in_one_period_is_one_statement():
    # Week 4 holds 16 of the 40 people and the measure collapses
    # there too: rows, entities, and volume all fall short in the
    # same period, stated once with every shortfall as a clause.
    # The week's total is sum(90..105) / 8 = 195.
    report = onceover.profile(_valued_panel(dip_week=4, dip_count=16))
    hits = _partial(report)
    assert len(hits) == 1
    assert hits[0]["evidence"][0]["metrics"]["period"] == "2024-01-29"
    assert "holds 16 rows" in hits[0]["claim"]
    assert "total tokens 195" in hits[0]["claim"]


def test_volume_spread_does_not_silence_count_shortfalls():
    # The measure's totals sit at half volume in weeks 7-9, and
    # week 2 holds 16 of 40 people, so four of ten periods fall
    # below 75% of the median total (4380) — past the quarter
    # share, so the volume names no period. The counts' own
    # spread is narrow, so the count statement still stands and
    # carries no volume clause. Weekly totals: 4380 in full
    # weeks, 2190 in weeks 7-9, 1560 in week 2.
    rows = []
    for wi, w in enumerate(WEEKS):
        present = PERSONS[:16] if wi == 2 else PERSONS
        for pi, p in enumerate(present):
            v = (90 + pi) / 2 if wi >= 7 else 90 + pi
            rows.append({"person": p, "week": w, "tokens": v})
    df = pl.DataFrame(
        rows,
        schema={"person": pl.String, "week": pl.Date, "tokens": pl.Float64},
    )
    report = onceover.profile(df)
    hits = _partial(report)
    assert len(hits) == 1
    assert hits[0]["evidence"][0]["metrics"]["period"] == "2024-01-15"
    assert "holds 16 rows" in hits[0]["claim"]
    assert "total tokens" not in hits[0]["claim"]
    entry = report.coverage["panel_coverage"]
    assert "too wide a spread for the measure volume" in entry["detail"]


def test_signed_measure_is_totalled_but_never_flags():
    # A signed measure's per-period sum is a level, not a volume:
    # its total swings negative in week 5 with rows and entities
    # steady, and no statement is made. The coverage entry still
    # reports its typical total, sum(40..79) = 2380.
    rows = []
    for wi, w in enumerate(WEEKS):
        for pi, p in enumerate(PERSONS):
            rows.append(
                {"person": p, "week": w, "delta": -(40 + pi) if wi == 5 else 40 + pi}
            )
    report = onceover.profile(pl.DataFrame(rows))
    assert _partial(report) == []
    entry = report.coverage["panel_coverage"]
    assert "principal measures: total delta typically 2380 per period" in entry["detail"]
