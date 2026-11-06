"""Planted tests for profile_many: candidate foreign keys across tables.

All fixtures are small literal frames with known answers.  No numpy:
the module under test is pure Polars, and this file must import
cleanly in wheel environments that install neither numpy nor scipy.
"""
import json

import polars as pl
import pytest

import onceover
from onceover import MultiTableReport, profile_many


def _customers_orders():
    customers = pl.DataFrame({
        "customer_id": [1, 2, 3],
        "name": ["ana", "bo", "cy"],
    })
    orders = pl.DataFrame({
        "order_id": [10, 11, 12, 13],
        "customer_id": [1, 2, 2, 3],
    })
    return {"customers": customers, "orders": orders}


def _find(report, child_table, child_column, parent_table, parent_column, pool):
    for record in pool:
        if (
            record["child_table"] == child_table
            and record["child_column"] == child_column
            and record["parent_table"] == parent_table
            and record["parent_column"] == parent_column
        ):
            return record
    return None


def candidate(report, *key):
    return _find(report, *key, report.candidates)


def overlap(report, *key):
    return _find(report, *key, report.overlaps)


def test_true_key_with_matching_names_is_strong():
    report = profile_many(_customers_orders())
    record = candidate(report, "orders", "customer_id", "customers", "customer_id")
    assert record is not None
    assert record["tier"] == "strong"
    assert record["name_agreement"] is True
    assert record["containment"] == 1.0
    assert record["child_distinct"] == 3
    assert record["parent_distinct"] == 3
    assert record["orphan_rows"] == 0
    # The name column is a string parent but names differ after
    # normalisation and values do not contain: not a candidate.
    assert candidate(report, "orders", "customer_id", "customers", "name") is None


def test_true_key_with_differing_names_is_weak_not_absent():
    employees = pl.DataFrame({"employee_id": [1, 2, 3], "name": ["a", "b", "c"]})
    customers = pl.DataFrame({"customer_id": [7, 8], "support_rep": [1, 3]})
    report = profile_many({"employees": employees, "customers": customers})
    record = candidate(report, "customers", "support_rep", "employees", "employee_id")
    assert record is not None
    assert record["tier"] == "weak"
    assert record["name_agreement"] is False
    assert 0.0 <= record["chance_score"] <= 1.0


def test_chance_containment_of_small_integers_scores_high():
    grades = pl.DataFrame({"grade": [1, 2, 3, 4, 5]})
    events = pl.DataFrame({"tier_code": [1, 2, 3, 2, 1], "label": list("abcde")})
    report = profile_many({"grades": grades, "events": events})
    record = candidate(report, "events", "tier_code", "grades", "grade")
    assert record is not None
    assert record["tier"] == "weak"
    assert record["chance_basis"] == "integer_span"
    # The child's whole span {1,2,3} is covered by the parent: a dense
    # range inside a dense range is exactly what chance produces.
    assert record["chance_score"] >= 0.99


def test_orphan_rows_are_counted_and_reported():
    parent = pl.DataFrame({"id": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]})
    child = pl.DataFrame({
        "ref": [1, 2, 3, 4, 5, 6, 7, 8, 9, 99, 99, 99, None],
    })
    report = profile_many({"parent": parent, "child": child})
    assert candidate(report, "child", "ref", "parent", "id") is None
    record = overlap(report, "child", "ref", "parent", "id")
    assert record is not None
    assert record["containment"] == pytest.approx(0.9)
    assert record["shared_distinct"] == 9
    assert record["orphan_distinct"] == 1
    assert record["orphan_rows"] == 3
    assert record["orphan_examples"] == [99]
    # The null row is counted as null, never as an orphan.
    assert record["child_null_rows"] == 1
    assert record["child_nonnull_rows"] == 12


def test_overlap_threshold_is_the_callers():
    parent = pl.DataFrame({"id": [1, 2, 3]})
    child = pl.DataFrame({"ref": [1, 2, 9, 9, None]})
    default = profile_many({"parent": parent, "child": child})
    assert overlap(default, "child", "ref", "parent", "id") is None
    lowered = profile_many({"parent": parent, "child": child}, overlap_threshold=0.5)
    record = overlap(lowered, "child", "ref", "parent", "id")
    assert record is not None
    assert record["orphan_rows"] == 2
    assert record["orphan_examples"] == [9]


def test_nonunique_parent_is_never_proposed():
    tags = pl.DataFrame({"code": [1, 2, 2, 3]})
    items = pl.DataFrame({"tag_code": [1, 2]})
    report = profile_many({"tags": tags, "items": items})
    assert candidate(report, "items", "tag_code", "tags", "code") is None
    assert overlap(report, "items", "tag_code", "tags", "code") is None
    assert report.candidates == []


def test_nulls_are_not_orphans_and_not_values():
    parent = pl.DataFrame({"id": [1, 2, 3]})
    child = pl.DataFrame({"ref": [1, None, 2, None]})
    report = profile_many({"parent": parent, "child": child})
    record = candidate(report, "child", "ref", "parent", "id")
    assert record is not None
    assert record["child_distinct"] == 2
    assert record["child_null_rows"] == 2
    assert record["orphan_rows"] == 0


def test_string_family_uses_the_value_pool():
    authors = pl.DataFrame({"name": ["ana", "bo", "cy"]})
    books = pl.DataFrame({
        "author_name": ["ana", "bo"] * 11,
        "title": [f"title {i}" for i in range(20)] + ["ana", "bo"],
    })
    report = profile_many({"authors": authors, "books": books})
    record = candidate(report, "books", "author_name", "authors", "name")
    assert record is not None
    assert record["tier"] == "weak"
    assert record["chance_basis"] == "value_pool"
    # Pool: 3 author names + 20 titles = 23 values; a random 2-subset
    # landing inside the parent's 3 is C(3,2)/C(23,2), about 0.012.
    assert record["chance_score"] == pytest.approx(3 / 253, rel=0.02)


def test_float_family_and_nan_is_missing():
    calibrations = pl.DataFrame({"value": [0.5, 1.5, 2.5]})
    readings = pl.DataFrame({
        "reading": pl.Series([0.5, 1.5, float("nan")], dtype=pl.Float32),
    })
    report = profile_many({"calibrations": calibrations, "readings": readings})
    record = candidate(report, "readings", "reading", "calibrations", "value")
    assert record is not None
    assert record["tier"] == "weak"
    assert record["child_distinct"] == 2
    assert record["child_nonnull_rows"] == 2


def test_temporal_family_dates():
    calendar = pl.DataFrame({
        "day": pl.Series(["2024-01-01", "2024-01-02", "2024-01-03"]).str.to_date(),
    })
    visits = pl.DataFrame({
        "day": pl.Series(["2024-01-01", "2024-01-03", "2024-01-03"]).str.to_date(),
    })
    report = profile_many({"calendar": calendar, "visits": visits})
    record = candidate(report, "visits", "day", "calendar", "day")
    assert record is not None
    assert record["tier"] == "strong"


def test_same_table_pairs_are_not_examined():
    employees = pl.DataFrame({
        "employee_id": [1, 2, 3],
        "manager_id": [1, 1, 2],
    })
    report = profile_many({"employees": employees})
    assert report.candidates == []
    assert report.overlaps == []
    assert report.coverage["pairs"]["ordered_cross_table_pairs"] == 0
    assert "Same-table pairs were not examined" in report.coverage["statement"]


def test_boolean_columns_are_disclosed_not_examined():
    a = pl.DataFrame({"id": [1, 2], "flag": [True, False]})
    b = pl.DataFrame({"id": [1, 2, 3], "ref": [1, 2, 2]})
    report = profile_many({"a": a, "b": b})
    skipped = report.coverage["unexamined_columns"]
    assert any(
        entry["table"] == "a" and entry["column"] == "flag" for entry in skipped
    )
    assert report.coverage["columns_examined"] == 3


def test_input_validation():
    with pytest.raises(ValueError):
        profile_many({})
    with pytest.raises(TypeError):
        profile_many([pl.DataFrame({"a": [1]})])
    with pytest.raises(TypeError):
        profile_many({"t": 42})
    with pytest.raises(ValueError):
        profile_many({"t": pl.DataFrame({"a": [1]})}, overlap_threshold=0)
    with pytest.raises(ValueError):
        profile_many({"t": pl.DataFrame({"a": [1]})}, overlap_threshold=1.5)


def test_lazyframe_input_is_collected():
    tables = _customers_orders()
    lazy = {name: frame.lazy() for name, frame in tables.items()}
    report = profile_many(lazy)
    assert candidate(report, "orders", "customer_id", "customers", "customer_id")


def test_report_surface():
    report = profile_many(_customers_orders())
    assert isinstance(report, MultiTableReport)
    assert onceover.profile_many is profile_many
    assert onceover.MultiTableReport is MultiTableReport
    assert report.data["kind"] == "onceover.profile_many"
    assert report.data["version"] == "1"
    assert {t["name"] for t in report.tables} == {"customers", "orders"}
    payload = json.loads(report.to_json())
    assert payload["candidates"] == report.candidates
    digest = report.to_digest()
    assert "1 candidate foreign key (1 strong" in digest
    assert "orders.customer_id -> customers.customer_id" in digest
    frame = report.candidates_frame()
    assert frame.height == len(report.candidates)
    assert "MultiTableReport" in repr(report)
    pairs = report.coverage["pairs"]
    assert pairs["ordered_cross_table_pairs"] == 8  # 4 columns x 2 partners
    assert pairs["same_family_pairs"] == 4  # the name column has no partner
    assert pairs["unique_parent_pairs"] == 3
    assert pairs["size_pruned_pairs"] == 1
    assert pairs["probe_checked_pairs"] == 2
    assert pairs["exact_checked_pairs"] == 0  # small frames: the probe is exact


def test_child_larger_than_parent_is_pruned():
    parent = pl.DataFrame({"id": [1, 2]})
    child = pl.DataFrame({"ref": [3, 4, 5, 6]})
    report = profile_many({"parent": parent, "child": child})
    assert report.candidates == []
    assert report.coverage["pairs"]["size_pruned_pairs"] >= 1


def test_all_null_child_column_yields_nothing():
    parent = pl.DataFrame({"id": [1, 2, 3]})
    child = pl.DataFrame({"ref": pl.Series([None, None], dtype=pl.Int64)})
    report = profile_many({"parent": parent, "child": child})
    assert report.candidates == []
    assert report.overlaps == []


def test_uint64_past_signed_range_uses_text_keys():
    big = 2**63 + 5
    parent = pl.DataFrame({"id": pl.Series([big, big + 1], dtype=pl.UInt64)})
    child = pl.DataFrame({"ref": pl.Series([big], dtype=pl.UInt64)})
    report = profile_many({"parent": parent, "child": child})
    record = candidate(report, "child", "ref", "parent", "id")
    assert record is not None
    assert record["chance_basis"] == "value_pool"
