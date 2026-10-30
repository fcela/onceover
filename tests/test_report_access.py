"""Addressable findings and size-budgeted JSON (issue #27):
report.select / report.finding pull one evidence block instead of
the whole report; to_json(max_chars) degrades in a stated order and
says what it cut."""
import json

import polars as pl

import onceover


def _panel_report():
    regions = ["North", "South", "East", "West"]
    entity, region, year, life = [], [], [], []
    i = 0
    for e in range(40):
        for yi in range(15):
            entity.append(f"E{e:02d}")
            region.append(regions[e // 10])
            year.append(2000 + yi)
            shift = 4.0 if regions[e // 10] == "South" and yi >= 8 else 0.0
            life.append(70.0 + (((i * 37) % 11) - 5) * 0.3 + shift)
            i += 1
    df = pl.DataFrame(
        {"entity": entity, "region": region, "year": year, "life_exp": life}
    )
    return onceover.profile(df, hints={"time": "year"})


def test_select_filters():
    report = _panel_report()
    assert report.select() == report.findings
    deps = report.select(category="dependencies")
    assert deps and all(f["category"] == "dependencies" for f in deps)
    shifts = report.select(category="changes", type="level_shift")
    assert len(shifts) == 1 and shifts[0]["type"] == "level_shift"
    by_column = report.select(column="life_exp")
    assert by_column and all(
        "life_exp" in f["subject"]["columns"] for f in by_column
    )
    by_target = report.select(target="life_exp")
    assert by_target and all(
        f["subject"].get("target") == "life_exp" for f in by_target
    )
    assert report.select(category="dependencies", column="year") == [
        f
        for f in report.findings
        if f["category"] == "dependencies" and "year" in f["subject"]["columns"]
    ]


def test_finding_lookup():
    report = _panel_report()
    first = report.findings[0]
    assert report.finding(first["id"]) is first
    assert report.finding("no-such-id") is None


def test_to_json_unbounded_is_the_native_report():
    report = _panel_report()
    assert json.loads(report.to_json()) == report.data


def test_to_json_budgeted():
    report = _panel_report()
    total = len(report.findings)
    assert total > 3
    # Full JSON for this report is ~19k chars; 9000 forces findings
    # to collapse into the index and column cards to names.
    out = report.to_json(max_chars=9000)
    assert len(out) <= 9000
    view = json.loads(out)
    budget = view["budget"]
    assert budget["max_chars"] == 9000
    assert budget["columns"] == "names_only"
    assert view["columns"] == {}
    assert view["column_names"] == list(report.columns.keys())
    assert budget["full_findings"] + budget["indexed_findings"] == total
    assert budget["indexed_findings"] > 0
    # Full findings are the top of the ranking, in order; index
    # entries carry the rest, also in report order.
    full_ids = [f["id"] for f in view["findings"]]
    assert full_ids == [f["id"] for f in report.findings[: len(full_ids)]]
    index_ids = [e["id"] for e in view["finding_index"]]
    assert index_ids == [f["id"] for f in report.findings[len(full_ids):]]
    for entry in view["finding_index"]:
        assert set(entry) == {
            "id",
            "category",
            "type",
            "columns",
            "interestingness",
            "claim",
        }


def test_to_json_budget_too_small():
    report = _panel_report()
    try:
        report.to_json(max_chars=50)
    except ValueError:
        pass
    else:
        raise AssertionError("expected ValueError for a 50-char budget")
