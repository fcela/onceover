"""Frame-model step 2: the account block (grain, axis span,
dominant structure), digest order, banded ranking, and
demotion-with-disclosure.

The fixtures are miniature versions of the evaluation frames: a
balanced state x month panel (the unemployment panel's shape), an
hourly series whose timestamp repeats (the traffic shape), and a
calendar-derived frame (the bike-sharing shape).
"""
import polars as pl

import onceover


def panel_df():
    states = ["CA", "MI", "NV"]
    dates = [f"2024-{m:02d}-01" for m in range(1, 13)]
    rows = {"state": [], "date": [], "rate": []}
    for s in states:
        for j, d in enumerate(dates):
            rows["state"].append(s)
            rows["date"].append(d)
            rows["rate"].append(4.0 + j + (1.0 if s == "NV" else 0.0))
    return pl.DataFrame(rows)


def test_account_states_the_factorized_grain():
    report = onceover.profile(panel_df())
    frame = report.data["frame"]
    grain = frame["grain"]
    assert grain["balanced"] is True
    assert grain["combinations"] == 36
    assert set(grain["key_columns"]) == {"state", "date"}
    # The grain finding leads the report (band 0, frame scope).
    first = report.data["findings"][0]
    assert first["facets"]["scope"] == "frame"
    assert first["claim"] == grain["statement"]
    # The bare composite-key finding for the same columns is gone:
    # the grain statement replaces it, it does not duplicate it.
    # (Other genuine keys of the fixture, like (rate, state), keep
    # their own findings.)
    assert not any(
        f["claim"] == "(date, state) jointly identify rows (composite key)"
        for f in report.data["findings"]
    )


def test_axis_span_and_step_are_stated():
    report = onceover.profile(panel_df())
    axis = report.data["frame"]["axis"]
    assert axis["min"] == "2024-01-01"
    assert axis["max"] == "2024-12-01"
    assert axis["distinct_values"] == 12
    assert axis["step"] == "1 month"


def test_digest_leads_with_the_account():
    report = onceover.profile(panel_df())
    digest = report.to_digest(100_000)
    lines = digest.splitlines()
    assert lines[0].startswith("profile digest:")
    assert lines[1].startswith("account: the grain is 3 state")
    assert any(
        line.startswith("account: time axis") and "2024-01-01 to 2024-12-01" in line
        for line in lines[:6]
    )
    account_at = next(i for i, line in enumerate(lines) if line.startswith("account:"))
    headline_at = next(i for i, line in enumerate(lines) if line.startswith("headline:"))
    assert account_at < headline_at


def test_schema_version_is_0_5():
    report = onceover.profile(panel_df())
    assert report.data["schema_version"] == "0.5"


def repeating_axis_df():
    # 48 rows over 36 distinct hours: 12 rows repeat an hour, with
    # the same reading — the axis nearly identifies rows, and no
    # other column rescues uniqueness.
    ts = [f"2024-03-0{1 + (h // 24)} {h % 24:02d}:00:00" for h in range(36)]
    volume = [100.0 + (h % 5) for h in range(36)]
    return pl.DataFrame(
        {"date_time": ts + ts[:12], "volume": volume + volume[:12]}
    )


def test_repeating_axis_is_stated_as_a_near_key():
    report = onceover.profile(repeating_axis_df())
    grain = report.data["frame"]["grain"]
    assert grain["near_key"]["distinct"] == 36
    assert grain["near_key"]["duplicate_rows"] == 12
    near = [
        f
        for f in report.data["findings"]
        if (f["facets"] or {}).get("scope") == "frame"
    ]
    assert len(near) == 1
    assert "does not identify rows" in near[0]["claim"]


def calendar_df():
    # dteday determines mnth, which determines season: findings
    # among the derived columns restate the calendar.
    import datetime as dt

    start = dt.date(2011, 1, 1)
    days = [start + dt.timedelta(days=k) for k in range(731)]
    return pl.DataFrame(
        {
            "dteday": [d.isoformat() for d in days],
            "mnth": [d.month for d in days],
            "season": [(d.month - 1) // 3 + 1 for d in days],
            "cnt": [100.0 + (k % 9) for k in range(731)],
        }
    )


def test_calendar_restatements_are_demoted_not_removed():
    report = onceover.profile(calendar_df())
    fds = [
        f
        for f in report.data["findings"]
        if f["category"] == "dependencies" and f.get("type") == "functional"
    ]
    assert fds, "the calendar dependencies are still reported"
    demoted = [
        f
        for f in fds
        if any(
            (e.get("metrics") or {}).get("triviality") == "calendar_derivation"
            for e in f["evidence"]
        )
    ]
    assert demoted, "calendar-derived dependencies carry the demotion reason"
    selection = report.data["coverage"].get("selection", {})
    frame_model = report.data["coverage"].get("frame_model", {})
    assert "frame_model" in report.data["coverage"]
    assert "grain" in frame_model["detail"]
    assert (
        "calendar_derivation" in selection.get("detail", "")
        or "calendar_derivation" in frame_model.get("detail", "")
        or demoted
    )
