"""Frame model, step 1: semantic column roles and time-axis assembly.

The pipeline computes a frame model before the detectors run. Roles
are disclosed in the report's `frame` section and on each column
card; the assembled axis is handed to the temporal passes; columns
the model marks as scaffolding (index, identifier, timestamp, and
the parts of an assembled axis) are excluded as detector targets
and as discontinuity axes, with the exclusion recorded in coverage.
"""
import polars as pl

import onceover

SCAFFOLD_BEIJING = {"No", "year", "month", "day", "hour"}


def _beijing_mini():
    # Two stations x 96 hours across two months; `No` restarts per
    # station, the calendar is split into part columns.
    rows = {"No": [], "year": [], "month": [], "day": [], "hour": [],
            "station": [], "PM2.5": [], "TEMP": []}
    k = 0
    for station in ("A", "B"):
        n = 0
        for month in (1, 2):
            for day in range(1, 3):
                for hour in range(24):
                    n += 1
                    k += 1
                    rows["No"].append(n)
                    rows["year"].append(2024)
                    rows["month"].append(month)
                    rows["day"].append(day)
                    rows["hour"].append(hour)
                    rows["station"].append(station)
                    rows["PM2.5"].append(20.0 + (k % 41) * 1.7)
                    rows["TEMP"].append(-5.0 + (k % 31) * 0.9)
    return pl.DataFrame(rows)


def _roles(report):
    return {r["column"]: r["role"] for r in report.data["frame"]["roles"]}


def _targets(report):
    return {f["subject"].get("target") for f in report.findings}


def test_split_calendar_roles_and_assembled_axis():
    report = onceover.profile(_beijing_mini())
    roles = _roles(report)
    assert roles["No"] == "index"
    for part in ("year", "month", "day", "hour"):
        assert roles[part] == "timestamp_part", part
    assert roles["station"] == "dimension"
    assert roles["PM2.5"] == "measure"
    axis = report.data["frame"]["axis"]
    assert axis["source"] == "timestamp_parts"
    assert axis["name"] == "year/month/day/hour"
    assert axis["parts"] == ["year", "month", "day", "hour"]
    # The card carries the same assignment, with its evidence.
    card = report.data["columns"]["No"]
    assert card["semantic_role"] == "index"
    assert card["semantic_role_evidence"]


def test_temporal_passes_run_on_assembled_axis():
    report = onceover.profile(_beijing_mini())
    for name in ("breaks", "ordered_partitions"):
        cov = report.coverage[name]
        assert cov["status"] not in ("not_applicable", "error"), cov
        assert "no date, datetime, or year-like integer column" not in cov["detail"]


def test_scaffolding_is_never_a_target_or_discontinuity_axis():
    report = onceover.profile(_beijing_mini())
    assert not (_targets(report) & SCAFFOLD_BEIJING)
    for f in report.findings:
        if f["type"] == "step_discontinuity":
            assert not (set(f["subject"]["columns"]) & SCAFFOLD_BEIJING), f["claim"]


def test_string_timestamps_become_the_axis():
    n = 240
    df = pl.DataFrame(
        {
            "date_time": [
                f"2021-{1 + (i // 24) % 9:02d}-{(i % 27) + 1:02d} {i % 24:02d}:00:00"
                for i in range(n)
            ],
            "traffic_volume": [1000.0 + (i % 53) * 37.0 + (900.0 if i >= 180 else 0.0)
                               for i in range(n)],
            "temp": [40.0 + (i % 29) * 1.3 for i in range(n)],
        }
    )
    report = onceover.profile(df)
    axis = report.data["frame"]["axis"]
    assert axis["source"] == "string_parse"
    assert axis["source_column"] == "date_time"
    assert _roles(report)["date_time"] == "timestamp"
    for name in ("breaks", "ordered_partitions"):
        cov = report.coverage[name]
        assert cov["status"] not in ("not_applicable", "error"), cov


def test_panel_with_string_dates_partitions_on_the_measure():
    months = []
    y, m = 2005, 1
    for _ in range(72):
        months.append(f"{y:04d}-{m:02d}-01")
        m += 1
        if m == 13:
            y, m = y + 1, 1
    rows = {"date": [], "state": [], "unemployment_rate": []}
    for state, base in (("AA", 4.0), ("BB", 5.0), ("CC", 6.0)):
        for j, d in enumerate(months):
            rows["date"].append(d)
            rows["state"].append(state)
            rows["unemployment_rate"].append(base + (j % 5) * 0.1 + (4.0 if j >= 48 else 0.0))
    report = onceover.profile(pl.DataFrame(rows))
    axis = report.data["frame"]["axis"]
    assert axis["source"] == "string_parse"
    cov = report.coverage["ordered_partitions"]
    assert cov["status"] not in ("not_applicable", "error"), cov
    hits = [
        f for f in report.findings
        if f["type"] == "ordered_partition"
        and "unemployment_rate" in f["subject"]["columns"]
    ]
    assert hits, "expected ordered partitions of unemployment_rate"


def test_year_like_integer_is_a_low_confidence_candidate():
    years = [2015 + (i * 7) % 6 for i in range(120)]
    df = pl.DataFrame(
        {"year": years, "value": [10.0 + (i % 37) * 0.8 for i in range(120)]}
    )
    report = onceover.profile(df)
    axis = report.data["frame"]["axis"]
    assert axis["source"] == "year_candidate"
    assert axis["confidence"] == "low"


def test_role_hint_overrides_the_assignment():
    report = onceover.profile(_beijing_mini(), hints={"roles": {"No": "measure"}})
    roles = _roles(report)
    assert roles["No"] == "measure"
    entry = next(r for r in report.data["frame"]["roles"] if r["column"] == "No")
    assert "hints.roles" in entry["evidence"]


def test_identifier_column_is_excluded_as_a_target():
    n = 200
    df = pl.DataFrame(
        {
            "label": [f"row-{i:04d}" for i in range(n)],
            "x": [float(i % 17) for i in range(n)],
            "y": [float((i % 17) * 2) for i in range(n)],
        }
    )
    report = onceover.profile(df)
    assert _roles(report)["label"] == "identifier"
    assert "label" not in _targets(report)
