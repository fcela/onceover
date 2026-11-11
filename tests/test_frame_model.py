"""Frame model, step 1: semantic column roles and time-axis assembly.

The pipeline computes a frame model before the detectors run. Roles
are disclosed in the report's `frame` section and on each column
card; the assembled axis is handed to the temporal passes; columns
the model marks as scaffolding (index, identifier, timestamp, and
the parts of an assembled axis) are excluded as detector targets
and as discontinuity axes, with the exclusion recorded in coverage.
"""
import calendar
from datetime import date, datetime, timedelta

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


def test_year_week_parts_assemble_a_weekly_axis():
    # Year and week columns with no month part: the axis is
    # assembled from the Monday of each ISO week, the parts are
    # timestamp parts, and the account calls the axis weekly.
    rows = {"year": [], "week": [], "person": [], "tokens": []}
    for p in range(12):
        for yr in (2023, 2024):
            for wk in range(1, 27):
                rows["year"].append(yr)
                rows["week"].append(wk)
                rows["person"].append(f"p{p:02}")
                rows["tokens"].append(20.0 + 0.1 * wk)
    report = onceover.profile(pl.DataFrame(rows))
    axis = report.data["frame"]["axis"]
    assert axis["source"] == "timestamp_parts"
    assert axis["name"] == "year/week"
    assert axis["parts"] == ["year", "week"]
    assert axis["step"] == "1 week"
    assert "weekly" in axis["description"]
    assert axis["min"] == "2023-01-02"  # the Monday of 2023 week 1
    roles = _roles(report)
    assert roles["year"] == "timestamp_part"
    assert roles["week"] == "timestamp_part"
    # The temporal passes run on the assembled axis.
    assert report.coverage["series"]["status"] != "skipped"


def test_lone_week_column_is_a_low_confidence_candidate():
    rows = {"week": [], "person": [], "tokens": []}
    for p in range(12):
        for wk in range(1, 27):
            rows["week"].append(wk)
            rows["person"].append(f"p{p:02}")
            rows["tokens"].append(20.0 + 0.1 * wk)
    report = onceover.profile(pl.DataFrame(rows))
    axis = report.data["frame"]["axis"]
    assert axis["source"] == "week_candidate"
    assert axis["confidence"] == "low"
    assert axis["min"] == "1"
    assert axis["max"] == "26"


def test_weekly_date_axis_is_described_as_weekly():
    mondays = [date(2023, 1, 2) + timedelta(weeks=i) for i in range(80)]
    df = pl.DataFrame({"t": mondays, "y": [float(i) for i in range(80)]})
    report = onceover.profile(df)
    axis = report.data["frame"]["axis"]
    assert axis["step"] == "1 week"
    assert "weekly" in axis["description"]


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


def test_datetime_axis_span_is_correct_in_every_time_unit():
    # Polars defaults to microseconds. Normalizing those ticks must not
    # overflow, or the account states a 1969-1970 span for modern data.
    start = datetime(2022, 5, 30)
    # Build the timestamp column as epoch microseconds and cast, as
    # in tests/test_events.py: every polars path that constructs a
    # Datetime series from Python datetime objects goes through
    # sequence_to_pyseries, whose temporal branch gets None back
    # (from Series.dt.cast_time_unit) on polars 1.x under Python
    # 3.15, so list-of-datetime construction raises AttributeError
    # there — the CI Polars-1.x leg. Integer construction and the
    # cast stay in working code paths on every supported polars.
    base_us = calendar.timegm(start.timetuple()) * 1_000_000 + start.microsecond
    day_us = 86_400 * 1_000_000
    base = pl.DataFrame({
        "ts": pl.Series(
            "ts", [base_us + day_us * i for i in range(400)]
        ).cast(pl.Datetime("us")),
        "y": [float(i % 7) for i in range(400)],
    })
    for unit in ("ms", "us", "ns"):
        frame = base.with_columns(pl.col("ts").cast(pl.Datetime(unit)))
        axis = onceover.profile(frame).data["frame"]["axis"]
        assert axis["min"].startswith("2022-05-30"), unit
        assert axis["max"].startswith("2023-07-03"), unit
        assert axis["step"] == "1 day", unit


def test_axis_alone_is_the_grain_when_it_is_unique():
    # Crossing a dimension with an already unique axis is a non-minimal key.
    n = 200
    frame = pl.DataFrame({
        "when": [date(2020, 1, 1) + timedelta(days=i) for i in range(n)],
        "region": [f"R{i % 7}" for i in range(n)],
        "value": [float((i * 37) % 101) for i in range(n)],
    })
    grain = onceover.profile(frame).data["frame"]["grain"]
    assert [f["column"] for f in grain["factorization"]] == ["when"]


def test_numeric_grids_are_measures_and_key_named_integers_are_identifiers():
    n = 5000
    frame = pl.DataFrame({
        "age": [18 + (i * 13) % 74 for i in range(n)],
        "reading": [((i * 7) % 50) / 50 for i in range(n)],
        "hour": [i % 24 for i in range(n)],
        "customer_id": [(i * 31) % 900 for i in range(n)],
        "currencyRateId": [i // 3 for i in range(n)],
        "sales": [float((i * 97) % 10007) / 3 for i in range(n)],
    })
    roles = _roles(onceover.profile(frame))
    assert roles["age"] == "measure"
    assert roles["reading"] == "measure"
    assert roles["hour"] == "dimension"
    assert roles["customer_id"] == "identifier"
    assert roles["currencyRateId"] == "identifier"
    assert roles["sales"] == "measure"


def test_unique_widely_spaced_integers_are_measures():
    n = 300
    frame = pl.DataFrame({
        "total_spend": [30_000 + (i * 7919) % 30_011 for i in range(n)],
        "row_label": [100 + (i * 7) % n for i in range(n)],
        "group": [f"G{i % 3}" for i in range(n)],
    })
    roles = _roles(onceover.profile(frame))
    assert frame["total_spend"].n_unique() == n
    assert roles["total_spend"] == "measure"
    assert roles["row_label"] == "identifier"


def test_flag_columns_get_the_flag_role_not_dimension():
    # Boolean and 0/1 columns are flags: they state a condition on
    # the row rather than partitioning the frame. A two-state
    # string column keeps the dimension role (its levels are
    # labels, not states); a single-state boolean is a constant
    # and takes neither role.
    n = 400
    frame = pl.DataFrame({
        "active": [i % 3 != 0 for i in range(n)],
        "converted": [1 if i % 5 == 0 else 0 for i in range(n)],
        "fraction_flag": [1.0 if i % 7 == 0 else 0.0 for i in range(n)],
        "channel": ["web" if i % 2 == 0 else "store" for i in range(n)],
        "region": [["north", "south", "east"][i % 3] for i in range(n)],
        "always_on": [True] * n,
        "amount": [10.0 + (i % 50) for i in range(n)],
    })
    report = onceover.profile(frame)
    roles = _roles(report)
    assert roles["active"] == "flag"
    assert roles["converted"] == "flag"
    assert roles["fraction_flag"] == "flag"
    assert roles["channel"] == "dimension"
    assert roles["region"] == "dimension"
    assert roles["amount"] == "measure"
    assert roles["always_on"] not in ("flag", "dimension")
    card = report.data["columns"]["active"]
    assert card["semantic_role"] == "flag"
    assert card["semantic_role_evidence"]


def test_flag_role_hint_overrides_both_ways():
    frame = pl.DataFrame({"active": [i % 2 == 0 for i in range(100)]})
    report = onceover.profile(frame, hints={"roles": {"active": "dimension"}})
    assert _roles(report)["active"] == "dimension"
    report = onceover.profile(frame, hints={"roles": {"active": "flag"}})
    assert _roles(report)["active"] == "flag"


def test_a_flag_crossed_with_time_is_not_a_grain():
    # Every (flag state, period) pair occurs exactly once, so the
    # flag would form an exact grain factor with the axis if it
    # carried the dimension role. It must not: a flag is not a
    # grain component, and no dominant grouping is measured on it.
    periods = 15
    rows = {"date": [], "active": [], "volume": []}
    for state in (False, True):
        for p in range(periods):
            rows["date"].append(f"202{3 + p // 12}-{1 + p % 12:02d}-01")
            rows["active"].append(state)
            rows["volume"].append(50.0 + ((p * 13 + int(state) * 7) % 40))
    report = onceover.profile(pl.DataFrame(rows))
    assert _roles(report)["active"] == "flag"
    grain = report.data["frame"].get("grain")
    if grain:
        # Neither the factorized grain nor the dependency pass's
        # exact key, promoted to the grain, may name the flag.
        assert "active" not in grain["key_columns"]
        assert "active" not in [
            f["column"] for f in grain.get("factorization") or []
        ]
    dominant = report.data["frame"].get("dominant") or []
    assert all(d["grouping"] != "active" for d in dominant)


def test_one_way_flag_scan_still_finds_its_flag():
    # The panel flag scan selects flags by the same definition the
    # frame model now uses for the role: on this person x week
    # panel the licence flag carries the flag role, stays out of
    # the grain, and is still reported as a one-way flag.
    persons, weeks = 30, 20
    rows = {"person": [], "week": [], "licence_flag": [], "activity": []}
    start = date(2025, 1, 6)
    for p in range(persons):
        onset = 5 + (p % 10)
        for w in range(weeks):
            rows["person"].append(f"p{p:02d}")
            rows["week"].append(start + timedelta(weeks=w))
            rows["licence_flag"].append(w >= onset)
            rows["activity"].append(10.0 if w > onset else 0.0)
    report = onceover.profile(pl.DataFrame(rows))
    roles = _roles(report)
    assert roles["licence_flag"] == "flag"
    assert roles["person"] == "dimension"
    grain = report.data["frame"]["grain"]
    assert "licence_flag" not in [f["column"] for f in grain["factorization"]]
    hits = report.select(type="one_way_flag")
    assert len(hits) == 1, [f["claim"] for f in report.findings]
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["flag"] == "licence_flag"
    assert metrics["entity_column"] == "person"
    assert metrics["entities_with_onset"] == persons
