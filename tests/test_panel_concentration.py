"""Concentration over time and outlier entities (issue #205).

The whole-column concentration check pools the periods and never
aggregates to the entity, so two panel shapes passed silently:

1. a measure quietly concentrating into a few accounts — here one
   account's share of each period's total rises from about 4% to
   about 45% while every other account is flat;
2. an entity whose level sits far outside the other entities —
   here one account at 100x the typical level, where no single
   cell is extreme because every one of the account's rows is.

The controls are a balanced panel (activity spread evenly, stable
over time), which must produce neither statement, and a flat
frame, where the coverage entry records the skip.
"""

import datetime

import polars as pl

import onceover

START = datetime.date(2025, 1, 6)


def _panel(values, measure="tokens"):
    person, week, vals = [], [], []
    for e, series in enumerate(values):
        for w, v in enumerate(series):
            person.append(f"p{e:03d}")
            week.append(START + datetime.timedelta(weeks=w))
            vals.append(float(v))
    return pl.DataFrame({"person": person, "week": week, measure: vals})


def _rising_panel():
    periods = 12
    values = [[100.0] * periods for _ in range(60)]
    values[0] = [260.0 + (4900.0 - 260.0) * w / (periods - 1)
                 for w in range(periods)]
    return _panel(values)


def _outlier_panel():
    # Ordinary accounts wiggle between 90 and 118 per period;
    # account 7 runs at about 100x that level.
    values = [[90.0 + ((e * 37 + w * 13) % 29) for w in range(12)]
              for e in range(60)]
    values[7] = [10_400.0 + ((w * 7) % 29) for w in range(12)]
    return _panel(values)


def _balanced_panel():
    return _panel([[100.0 + ((e * 31 + w * 17) % 23) for w in range(12)]
                   for e in range(60)])


def test_rising_top_share_is_reported():
    report = onceover.profile(_rising_panel())
    hits = report.select(type="concentration_over_time")
    assert len(hits) == 1, [f["claim"] for f in report.findings]
    hit = hits[0]
    assert hit["category"] == "concentration"
    metrics = hit["evidence"][0]["metrics"]
    assert metrics["measure"] == "tokens"
    assert metrics["entity_column"] == "person"
    assert metrics["entities"] == 60
    assert metrics["top_group_size"] == 1
    assert metrics["late_top_share"] - metrics["early_top_share"] >= 0.10
    assert metrics["late_top_share"] >= 0.40
    top = metrics["top_entities_last_period"]
    assert top[0]["entity"] == "p000"
    assert top[0]["share"] >= 0.40
    assert "p000" in hit["claim"]


def test_hundredfold_entity_is_named_as_outlier():
    report = onceover.profile(_outlier_panel())
    hits = report.select(type="outlier_entity")
    assert len(hits) == 1, [f["claim"] for f in report.findings]
    hit = hits[0]
    assert hit["category"] == "concentration"
    metrics = hit["evidence"][0]["metrics"]
    assert metrics["measure"] == "tokens"
    assert metrics["entity_column"] == "person"
    assert metrics["outlier_entities"] == 1
    named = metrics["outliers"]
    assert named[0]["entity"] == "p007"
    assert abs(named[0]["ratio_to_typical"] - 100.0) < 5.0
    assert 0.60 < named[0]["share"] < 0.65
    assert "p007" in hit["claim"]


def test_balanced_panel_reports_neither():
    report = onceover.profile(_balanced_panel())
    assert report.select(type="concentration_over_time") == []
    assert report.select(type="outlier_entity") == []
    entry = report.coverage["panel_concentration"]
    assert entry["status"] == "ran_exact"
    assert "0 concentration and 0 outlier" in entry["detail"]


def test_panel_concentration_coverage_without_a_grain():
    report = onceover.profile(pl.DataFrame({
        "a": [1.0, 2.0, 3.0] * 100,
        "b": [True, False, True] * 100,
    }))
    entry = report.coverage["panel_concentration"]
    assert entry["status"] == "skipped"
    assert "no entity" in entry["detail"]
