"""Panel dynamics: entry and exit blocks, and first activity
(issue #204).

The shapes the passes used to miss, reproduced on a person × week
panel: a cohort of 30 people first appearing in a single week
(the reported case had 180 in one week), a block of 30 people
last seen in a single week, and activity that starts three
periods after each person's first row — the first-activity week
the user computed by hand. Controls: steady one-or-two-per-period
churn is not a block, and a frame with no entity × time grain
records the check as skipped.
"""

import datetime

import polars as pl

import onceover

START = datetime.date(2025, 1, 6)  # a Monday


def _panel(with_measure=True):
    persons, weeks = 120, 26
    person, week, tokens = [], [], []
    for p in range(persons):
        if p < 60:
            first, last = 0, weeks - 1          # founding cohort, stays
        elif p < 90:
            first, last = 8, weeks - 1          # entry block at week 8
        else:
            first, last = 0, 15                 # exit block after week 15
        for w in range(first, last + 1):
            person.append(f"p{p:03d}")
            week.append(START + datetime.timedelta(weeks=w))
            active = w >= first + 3
            tokens.append(10 + ((p * 13 + w * 7) % 40) if active else 0)
    cols = {"person": person, "week": week}
    if with_measure:
        cols["tokens"] = tokens
    return pl.DataFrame(cols)


def _hits(report, type_, entity="person"):
    return [
        f
        for f in report.select(type=type_)
        if f["evidence"][0]["metrics"].get("entity_column") == entity
    ]


def test_entry_block_is_reported():
    report = onceover.profile(_panel())
    hits = _hits(report, "entry_block")
    assert len(hits) == 1, [f["claim"] for f in report.findings]
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["entering_entities"] == 30
    assert metrics["entities"] == 120
    assert metrics["period"] == "2025-03-03"
    assert abs(metrics["entity_share"] - 0.25) < 1e-9
    assert "30 of 120" in hits[0]["claim"]


def test_exit_block_is_reported():
    report = onceover.profile(_panel())
    hits = _hits(report, "exit_block")
    assert len(hits) == 1, [f["claim"] for f in report.findings]
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["exiting_entities"] == 30
    assert metrics["period"] == "2025-04-21"
    # The entities still present in the last week are not exits:
    # 90 of the 120 are last seen in the final period, and no exit
    # block is stated there.
    assert all(
        f["evidence"][0]["metrics"]["period_index"] < 25
        for f in _hits(report, "exit_block")
    )


def test_first_activity_is_reported_with_lag_and_spans():
    report = onceover.profile(_panel())
    hits = _hits(report, "first_activity")
    assert len(hits) == 1, [f["claim"] for f in report.findings]
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["activity_column"] == "tokens"
    assert metrics["entities_active"] == 120
    assert metrics["entities_starting_on_entry"] == 0
    assert metrics["entities_starting_late"] == 120
    assert metrics["entities_never_active"] == 0
    assert metrics["lag_periods_median"] == 3.0
    spans = metrics["entity_spans"]
    assert len(spans) == 120
    by_entity = {s["entity"]: s for s in spans}
    assert by_entity["p000"]["first_period"] == "2025-01-06"
    assert by_entity["p000"]["last_period"] == "2025-06-30"
    assert by_entity["p000"]["first_activity_period"] == "2025-01-27"
    assert by_entity["p075"]["first_period"] == "2025-03-03"
    assert by_entity["p100"]["last_period"] == "2025-04-21"


def test_spans_ride_on_a_block_finding_without_a_measure():
    report = onceover.profile(_panel(with_measure=False))
    assert _hits(report, "first_activity") == []
    blocks = _hits(report, "entry_block") + _hits(report, "exit_block")
    assert len(blocks) == 2
    carriers = [
        f for f in blocks if "entity_spans" in f["evidence"][0]["metrics"]
    ]
    assert len(carriers) == 1
    spans = carriers[0]["evidence"][0]["metrics"]["entity_spans"]
    assert len(spans) == 120
    assert "first_activity_period" not in spans[0]


def test_uniform_churn_stays_quiet():
    persons, weeks = 48, 26
    person, week = [], []
    for p in range(persons):
        first = 0 if p < 24 else (p - 24) // 2
        for w in range(first, weeks):
            person.append(f"p{p:02d}")
            week.append(START + datetime.timedelta(weeks=w))
    report = onceover.profile(pl.DataFrame({"person": person, "week": week}))
    assert report.select(type="entry_block") == []
    assert report.select(type="exit_block") == []
    assert report.select(type="first_activity") == []
    entry = report.coverage["panel_dynamics"]
    assert entry["status"] == "ran_exact"
    assert "0 entry blocks, 0 exit blocks" in entry["detail"]


def test_panel_dynamics_coverage_without_a_grain():
    report = onceover.profile(pl.DataFrame({
        "a": [1.0, 2.0, 3.0] * 100,
        "b": [True, False, True] * 100,
    }))
    entry = report.coverage["panel_dynamics"]
    assert entry["status"] == "skipped"
    assert "no entity" in entry["detail"]
