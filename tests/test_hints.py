"""Declared hints (issue #18): the expectations pass checks structure
the user declares — gap-free columns, cadences, ordering, panel shape —
and tags its evidence reference_source="declared"."""
import datetime

import polars as pl

import onceover


def declared(report, category=None):
    out = []
    for f in report.findings:
        if category and f.get("category") != category:
            continue
        if any(
            e["metrics"].get("reference_source") == "declared"
            for e in f.get("evidence") or []
        ):
            out.append(f)
    return out


def test_integer_sequence_gaps():
    ids = [i for i in range(1, 101) if i not in (4, 7, 18)]
    df = pl.DataFrame({"id": ids, "x": [float(i % 7) for i in ids]})
    report = onceover.profile(df, hints={"no_gaps": ["id"]})
    hits = declared(report, "gaps")
    assert len(hits) == 1
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["missing"] == 3
    assert metrics["absent_values"] == [4, 7, 18]
    assert metrics["step"] == 1


def test_monthly_cadence_below_inference_floor():
    months = [
        datetime.date(2024, 1, 1),
        datetime.date(2024, 2, 1),
        datetime.date(2024, 4, 1),
        datetime.date(2024, 5, 1),
        datetime.date(2024, 6, 1),
    ]
    df = pl.DataFrame({"month": months, "v": [1.0, 2.0, 3.0, 4.0, 5.0]})
    report = onceover.profile(
        df, hints={"no_gaps": ["month"], "cadence": {"month": "monthly"}}
    )
    hits = declared(report, "gaps")
    assert len(hits) == 1
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["missing"] == 1
    assert metrics["reference"] == "months"
    # The declared check replaces inference for this column: every gap
    # finding about `month` is the declared one.
    gaps = [
        f
        for f in report.findings
        if f.get("category") == "gaps"
        and "month" in (f.get("subject") or {}).get("columns", [])
    ]
    assert len(gaps) == 1


def test_ordering_violations_and_satisfaction():
    bad = pl.DataFrame({"x": [1, 2, 3, 5, 4, 6, 7, 8, 9, 10]})
    report = onceover.profile(bad, hints={"order_by": ["x"]})
    hits = declared(report, "ordering")
    assert len(hits) == 1
    assert hits[0]["evidence"][0]["metrics"]["inversions"] == 1
    assert "not ordered" in hits[0]["claim"]

    good = pl.DataFrame({"x": list(range(10))})
    report2 = onceover.profile(good, hints={"order_by": ["x"]})
    hits2 = declared(report2, "ordering")
    assert len(hits2) == 1
    assert hits2[0]["evidence"][0]["metrics"]["inversions"] == 0


def test_panel_completeness():
    rows = []
    for ent in ("E0", "E1"):
        for year in (2020, 2021, 2022):
            if (ent, year) == ("E1", 2021):
                continue
            rows.append({"entity": ent, "year": year, "gdp": 1.0})
    df = pl.DataFrame(rows)
    hints = {"dimensions": ["entity"], "time": "year", "measures": ["gdp"]}
    report = onceover.profile(df, hints=hints)
    hits = declared(report, "gaps")
    assert len(hits) == 1
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["missing_cells"] == 1
    assert metrics["expected_cells"] == 6
    absent = str(metrics["absent_values"])
    assert "E1" in absent and "2021" in absent

    full = pl.DataFrame(rows + [{"entity": "E1", "year": 2021, "gdp": 1.0}])
    report2 = onceover.profile(full, hints=hints)
    assert declared(report2, "gaps") == []


def test_invalid_hints_are_reported_not_raised():
    df = pl.DataFrame({"id": list(range(1, 16))})
    report = onceover.profile(df, hints={"no_gaps": ["id", "dtae"], "time": "when"})
    detail = report.coverage["expectations"]["detail"]
    assert "dtae" in detail and "when" in detail


def test_no_hints_no_expectations():
    df = pl.DataFrame({"id": list(range(1, 16))})
    report = onceover.profile(df)
    assert report.coverage["expectations"]["status"] == "skipped"
    assert declared(report) == []
