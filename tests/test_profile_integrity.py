"""Exact value identity, relationship recovery, and bounded profiling checks."""

import random

import polars as pl
import pytest

import onceover


@pytest.mark.parametrize("values", [
    [2**63, 2**63 + 1, 2**63 + 2],
    [2**53, 2**53 + 1, 2**63, 2**63 + 1],
    [2**64 - 3, 2**64 - 2, 2**64 - 1],
])
def test_unsigned_integer_identity_is_exact(values):
    report = onceover.profile(pl.DataFrame({"id": pl.Series(values, dtype=pl.UInt64)}), budget="quick")
    assert report.columns["id"]["distinct"] == len(values)
    assert not report.select(category="duplicates")
    assert report.select(category="uniqueness", column="id")
    assert {v["value"] for v in report.columns["id"]["top_values"]} == set(map(str, values))


def test_unsigned_dependencies_and_ordering_use_exact_values():
    # Both adjacent values round to the same float. They must remain
    # different determinant groups and retain their chronological order.
    df = pl.DataFrame({
        "key": pl.Series([2**63, 2**63 + 1] * 60, dtype=pl.UInt64),
        "label": ["a", "b"] * 60,
    })
    report = onceover.profile(df, detectors=["functional_dependencies"])
    hits = report.select(category="dependencies", type="functional", target="label")
    assert any(f["subject"]["inputs"] == ["key"]
               and any(e["metrics"].get("violations") == 0 for e in f["evidence"])
               for f in hits)
    ordered = pl.DataFrame({"key": pl.Series([2**63 + 1, 2**63] * 60, dtype=pl.UInt64)})
    r = onceover.profile(ordered, detectors=["expectations"], hints={"order_by": ["key"]})
    assert r.select(category="ordering")[0]["evidence"][0]["metrics"]["inversions"] == 60


def test_nullable_chunked_unsigned_buffers_preserve_values():
    df = pl.concat([
        pl.DataFrame({"x": pl.Series([None, 2**63, 2**63 + 1], dtype=pl.UInt64)}),
        pl.DataFrame({"x": pl.Series([2**63 + 1, 2**63 + 2, None], dtype=pl.UInt64)}),
    ], rechunk=False)
    report = onceover.profile(df, budget="quick")
    assert report.columns["x"]["distinct"] == 3
    assert report.columns["x"]["null_count"] == 2
    dup = report.select(category="duplicates")[0]
    assert dup["evidence"][0]["metrics"]["n_redundant"] == 2


def test_unsigned_sequence_gaps_near_u64_max_are_exact():
    maximum = 2**64 - 1
    values = [v for v in range(maximum - 11, maximum + 1) if v != maximum - 1]
    df = pl.DataFrame({"id": pl.Series(values, dtype=pl.UInt64)})
    report = onceover.profile(df, detectors=["expectations"],
                              hints={"no_gaps": ["id"], "cadence": {"id": 1}})
    hits = report.select(category="gaps")
    assert hits[0]["evidence"][0]["metrics"]["absent_values"] == [maximum - 1]


@pytest.mark.parametrize("unit", ["ms", "us", "ns"])
def test_timestamp_identity_keeps_source_resolution(unit):
    frame = pl.DataFrame({"t": pl.Series([-1, 0, 1, 2], dtype=pl.Int64).cast(pl.Datetime(unit))})
    report = onceover.profile(frame, budget="quick")
    assert report.columns["t"]["distinct"] == 4
    assert not report.select(category="duplicates")
    assert len({v["value"] for v in report.columns["t"]["top_values"]}) == 4
    assert report.columns["t"]["temporal_stats"]["min"].startswith("1969-12-31 23:59:59.")


def test_extreme_millisecond_epochs_do_not_overflow():
    frame = pl.DataFrame({"t": pl.Series([-(2**63), 2**63 - 1], dtype=pl.Int64).cast(pl.Datetime("ms"))})
    report = onceover.profile(frame, budget="quick")
    assert report.columns["t"]["distinct"] == 2
    assert report.columns["t"]["temporal_stats"]["span_days"] > 200_000_000_000


@pytest.mark.parametrize("unit,day", [("ms", 86_400_000), ("us", 86_400_000_000), ("ns", 86_400_000_000_000)])
def test_calendar_projection_uses_source_unit_and_floor(unit, day):
    df = pl.DataFrame({"t": pl.Series([-1, 0, 2 * day], dtype=pl.Int64).cast(pl.Datetime(unit))})
    r = onceover.profile(df, detectors=["expectations"], hints={"no_gaps": ["t"], "cadence": {"t": "daily"}})
    hits = r.select(category="gaps")
    assert len(hits) == 1
    assert hits[0]["evidence"][0]["metrics"]["missing"] == 1


@pytest.mark.parametrize("values", [
    [None, -7046029254386353131, -7046029254386353131],
    [-7046029254386353131, None, -7046029254386353131],
    [-7046029254386353131, -7046029254386353131, None],
])
def test_duplicate_classes_survive_hash_collisions(values):
    report = onceover.profile(pl.DataFrame({"x": values}), budget="quick")
    hits = report.select(category="duplicates")
    assert len(hits) == 1
    assert hits[0]["evidence"][0]["metrics"]["n_redundant"] == 1


def test_views_constants_do_not_hide_late_dependent_columns():
    rng = random.Random(9)
    n = 600
    x = [rng.gauss(0, 1) for _ in range(n)]
    data = {f"constant_{i}": [float(i)] * n for i in range(64)}
    data.update({"late_x": x, "late_y": x})
    report = onceover.profile(pl.DataFrame(data), detectors=["views"], max_findings=500)
    assert any(set(f["subject"]["columns"]) == {"late_x", "late_y"}
               for f in report.select(category="associations"))
    assert "late_x, late_y" not in report.coverage["views"]["detail"]


def test_pivot_rules_keep_their_wide_population_and_validation():
    n = 800
    wide = pl.DataFrame({
        "entity": [f"E{i}" for i in range(n)],
        "a": [float(i % 2) for i in range(n)],
        "b": [float(i % 2) for i in range(n)],
        "c": [100.0 + (i * 37 % 251) * 0.31 for i in range(n)],
    })
    long = wide.unpivot(index="entity", variable_name="indicator", value_name="value")
    direct = onceover.profile(wide, max_findings=500, max_rules=100)
    pivoted = onceover.profile(long, max_findings=500, max_rules=100)
    expected = {f["id"] for f in direct.select(category="conditional_rules")}
    rules = pivoted.select(category="conditional_rules")
    assert len(expected) == 4
    assert {f["id"] for f in rules} == expected
    for rule in rules:
        evaluated = [e for e in rule["evidence"] if e["method"]["detector"] == "rule_evaluator"]
        assert len(evaluated) == 1
        assert evaluated[0]["scope"]["population_rows"] == n


def test_pivot_preserves_unsigned_measure_identity():
    n = 600
    wide = pl.DataFrame({
        "entity": [f"E{i}" for i in range(n)],
        "a": pl.Series([2**63 + i % 300 for i in range(n)], dtype=pl.UInt64),
        "b": pl.Series([2**63 + 600 + i % 300 for i in range(n)], dtype=pl.UInt64),
    })
    long = wide.unpivot(index="entity", variable_name="indicator", value_name="value")
    report = onceover.profile(long, max_findings=500)
    assert any(f["subject"]["inputs"] == ["a"]
               and any(e["metrics"].get("source_form") == "pivoted wide form"
                       and e["metrics"].get("lhs_distinct") == 300 for e in f["evidence"])
               for f in report.select(category="dependencies", type="functional", target="b"))


def test_extreme_date_range_is_rejected_before_full_enumeration():
    df = pl.DataFrame({"d": pl.Series([-(2**31), 2**31 - 1], dtype=pl.Int32).cast(pl.Date)})
    r = onceover.profile(df, detectors=["expectations"], hints={"no_gaps": ["d"], "cadence": {"d": "daily"}})
    assert "exceeds" in r.coverage["expectations"]["detail"]
    assert not r.select(category="gaps")


def test_low_frequency_sentinel_outside_top_64_values_is_retained():
    values = [float(i) for i in range(1, 81) for _ in range(30)]
    values[600:620] = [-999.0] * 20
    r = onceover.profile(pl.DataFrame({"x": values}), detectors=["sentinels"])
    hits = r.select(type="sentinel_values")
    assert any(e["metrics"].get("value") == -999.0 for f in hits for e in f["evidence"])


def test_surprise_scan_retains_rare_unsampled_rows():
    rng = random.Random(44)
    n = 100_003
    values = [rng.gauss(50, 1) for _ in range(n)]
    values[-1] = 9999.0
    r = onceover.profile(pl.DataFrame({"value": values}), detectors=["surprises"])
    assert any(n - 1 in e["metrics"].get("extreme_rows", [])
               for f in r.findings for e in f["evidence"])
