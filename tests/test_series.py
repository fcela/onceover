"""Series along the time axis: axis gaps, trend, cycle, persistence, levels (#131)."""
import calendar
from datetime import date, datetime, timedelta

import polars as pl
import pytest

import onceover

np = pytest.importorskip("numpy", reason="planted frames need numpy")

N = 3000


def _days(n=N, start=date(2010, 1, 1)):
    return [start + timedelta(days=i) for i in range(n)]


def _series(report, column=None):
    out = {}
    for f in report.findings:
        if f.get("type") == "series_structure" and (
            column is None or f["claim"].startswith(column)
            or f"of {column} =" in f["claim"]
        ):
            out.setdefault(f["evidence"][0]["metrics"]["kind"], f)
    return out


def _run(**columns):
    return onceover.profile(pl.DataFrame(columns), detectors=["series"])


def test_axis_gap_counts_match_the_calendar():
    days = _days()
    kept = days[:1000] + days[1120:]
    rng = np.random.default_rng(0)
    report = _run(t=kept, y=rng.normal(size=len(kept)))
    gaps = [f for f in report.findings if f["category"] == "gaps"]
    assert len(gaps) == 1
    m = gaps[0]["evidence"][0]["metrics"]
    assert m["absent_steps"] == 120
    assert m["expected_steps"] == N
    assert m["longest_gap_steps"] == 120


def test_weekday_series_has_no_gaps():
    days = [d for d in _days() if d.weekday() < 5]
    rng = np.random.default_rng(1)
    report = _run(t=days, y=rng.normal(size=len(days)))
    assert not [f for f in report.findings if f["category"] == "gaps"]


def test_monthly_axis_counts_months():
    months = [date(2000 + i // 12, i % 12 + 1, 1) for i in range(120)]
    kept = months[:50] + months[56:]
    rng = np.random.default_rng(2)
    report = _run(t=kept, y=rng.normal(size=len(kept)))
    gaps = [f for f in report.findings if f["category"] == "gaps"]
    assert gaps[0]["evidence"][0]["metrics"]["absent_steps"] == 6
    assert "1 month" in gaps[0]["claim"]


def test_duplicate_timestamps_stated_when_a_float_is_unique():
    k = np.arange(N)
    rng = np.random.default_rng(3)
    k[rng.choice(N, N // 50, replace=False)] -= 1
    days = [date(2010, 1, 1) + timedelta(days=int(v)) for v in k]
    report = onceover.profile(pl.DataFrame({"t": days, "y": rng.normal(size=N)}))
    near = report.data["frame"]["grain"].get("near_key")
    assert near and near["duplicate_rows"] == N - len(set(days))


def test_trend():
    rng = np.random.default_rng(4)
    k = _series(_run(t=_days(), y=np.arange(N) * 0.01 + rng.normal(size=N)), "y")
    m = k["trend"]["evidence"][0]["metrics"]
    assert m["slope_per_year"] == pytest.approx(0.01 * 365.25, rel=0.05)
    assert "rises" in k["trend"]["claim"]


def test_level_shift_is_not_a_trend():
    rng = np.random.default_rng(5)
    y = rng.normal(size=N)
    y[N // 2:] += 5
    assert "trend" not in _series(_run(t=_days(), y=y), "y")


def test_weekly_cycle():
    rng = np.random.default_rng(6)
    y = np.sin(np.arange(N) * 2 * np.pi / 7) * 2 + rng.normal(size=N) * 0.5
    k = _series(_run(t=_days(), y=y), "y")
    assert k["cycle"]["evidence"][0]["metrics"]["period"] == "7 days"


def test_hourly_cycle():
    rng = np.random.default_rng(7)
    # Epoch microseconds and a cast: building a Datetime series from Python
    # datetimes raises on Polars 1.x under Python 3.15.
    start = calendar.timegm(datetime(2020, 1, 1).timetuple()) * 1_000_000
    t = pl.Series("t", [start + i * 3_600_000_000 for i in range(N)]).cast(pl.Datetime("us"))
    y = np.sin(np.arange(N) * 2 * np.pi / 24) * 2 + rng.normal(size=N) * 0.5
    assert _series(_run(t=t, y=y), "y")["cycle"]["evidence"][0]["metrics"]["period"] == "1 day"


def test_annual_cycle_beyond_the_lag_scan():
    rng = np.random.default_rng(8)
    y = np.sin(np.arange(N) * 2 * np.pi / 365.25) * 3 + rng.normal(size=N) * 0.5
    assert _series(_run(t=_days(), y=y), "y")["cycle"]["evidence"][0]["metrics"]["period"] == "1 year"


def test_persistence_without_a_false_cycle():
    rng = np.random.default_rng(9)
    y = np.zeros(N)
    e = rng.normal(size=N)
    for i in range(1, N):
        y[i] = 0.9 * y[i - 1] + e[i]
    k = _series(_run(t=_days(), y=y), "y")
    assert "cycle" not in k
    assert k["persistence"]["evidence"][0]["metrics"]["autocorrelation"] == pytest.approx(0.9, abs=0.05)


def test_level_shift_does_not_read_as_persistence():
    rng = np.random.default_rng(10)
    y = rng.normal(size=N)
    y[N // 2:] += 5
    assert "persistence" not in _series(_run(t=_days(), y=y), "y")


def test_new_level_with_exact_date():
    rng = np.random.default_rng(11)
    onset = int(N * 0.6)
    g = [("C" if i > onset and u < 0.3 else ("A" if u < 0.6 else "B"))
         for i, u in enumerate(rng.random(N))]
    k = _series(_run(t=_days(), g=g, y=rng.normal(size=N)), "g")
    m = k["new_level"]["evidence"][0]["metrics"]
    assert m["value"] == "C"
    first = g.index("C")
    assert m["first_seen"] == _days()[first].isoformat()
    assert m["rows_before"] == first


def test_share_shift():
    rng = np.random.default_rng(12)
    g = ["A" if u < i / N else "B" for i, u in enumerate(rng.random(N))]
    m = _series(_run(t=_days(), g=g, y=rng.normal(size=N)), "g")["share_shift"]["evidence"][0]["metrics"]
    assert abs(m["late_share"] - m["early_share"]) > 0.5


def test_calendar_columns_are_not_new_levels():
    days = _days()
    rng = np.random.default_rng(13)
    report = _run(t=days, year=[str(d.year) for d in days], y=rng.normal(size=N))
    assert not _series(report, "year")


def test_no_axis_is_disclosed():
    report = onceover.profile(pl.DataFrame({"a": [float(i) for i in range(200)]}), detectors=["series"])
    assert report.coverage["series"]["status"] == "skipped"


def test_noise_is_quiet():
    bad = []
    for seed in range(8):
        rng = np.random.default_rng(100 + seed)
        report = _run(
            t=_days(),
            a=rng.normal(size=N),
            b=rng.lognormal(size=N),
            c=rng.standard_t(2, size=N),
            d=rng.exponential(size=N),
            g=[f"L{v}" for v in rng.integers(0, 5, size=N)],
        )
        bad += [f["claim"] for f in report.findings]
    assert not bad, bad


def test_recurring_level_is_not_new_and_a_seasonal_share_is_not_a_shift():
    # Levels cycling with the year: absent at the start, but also absent for
    # as long later on.
    days = _days()
    season = [("s" + str(((d.timetuple().tm_yday + 45) // 91) % 4)) for d in days]
    rng = np.random.default_rng(14)
    # Not fixed by a calendar month, since boundaries fall inside months.
    k = _series(_run(t=days, season=season, y=rng.normal(size=N)), "season")
    assert "new_level" not in k
    assert "ended_level" not in k
    assert "share_shift" not in k


def test_level_seen_near_the_start_is_not_new():
    rng = np.random.default_rng(15)
    g = ["A" if u < 0.5 else "B" for u in rng.random(N)]
    g[0:100] = ["A"] * 100
    assert "new_level" not in _series(_run(t=_days(), g=g, y=rng.normal(size=N)), "g")


def test_irregular_axis_gets_no_cycle():
    rng = np.random.default_rng(16)
    offsets = np.cumsum(rng.integers(1, 200, size=N))
    t = [date(1900, 1, 1) + timedelta(days=int(v) // 40) for v in offsets]
    y = np.sin(np.arange(N) * 2 * np.pi / 7) * 2 + rng.normal(size=N) * 0.2
    k = _series(_run(t=t, y=y), "y")
    assert "cycle" not in k


# --- #210: significance-tested trend ------------------------------------

MONDAYS = [date(2023, 1, 2) + timedelta(weeks=i) for i in range(200)]


def test_trend_carries_its_significance_test():
    rng = np.random.default_rng(4)
    k = _series(_run(t=_days(), y=np.arange(N) * 0.01 + rng.normal(size=N)), "y")
    m = k["trend"]["evidence"][0]["metrics"]
    assert m["slope_p_value"] < 0.001
    assert m["residual_autocorrelation"] >= 0.0
    assert 0 < m["effective_points"] <= N


def test_random_walk_is_not_a_trend():
    # A random walk's line fit explains most of its variance (seeds
    # 100 and 103 read as trends on the descriptive gates alone),
    # but the movement is indistinguishable from noise: the deflated
    # slope test must keep it from being stated as a trend.
    n = 156
    for seed in (100, 102, 103):
        rng = np.random.default_rng(seed)
        y = np.cumsum(rng.normal(size=n))
        assert "trend" not in _series(_run(t=MONDAYS[:n], y=y), "y"), seed


def test_weekly_axis_annual_cycle_at_two_years():
    # 115 weekly points span about 2.2 years — too short for the
    # general yearly probe — but a weekly axis carries its annual
    # cycle at lag 52, and the pass tests that lag directly.
    n = 115
    rng = np.random.default_rng(8)
    y = 3 * np.sin(np.arange(n) * 2 * np.pi / 52) + rng.normal(scale=0.5, size=n)
    k = _series(_run(t=MONDAYS[:n], y=y), "y")
    assert k["cycle"]["evidence"][0]["metrics"]["period"] == "1 year"
    assert k["cycle"]["evidence"][0]["metrics"]["lag_steps"] == 52


# --- #210: per-entity series on a panel grain ----------------------------


def _panel(persons=40, weeks=80, risers=8, seed=21):
    rng = np.random.default_rng(seed)
    rows = {"t": [], "person": [], "tokens": []}
    for i in range(persons):
        for wk in range(weeks):
            rows["t"].append(MONDAYS[wk])
            rows["person"].append(f"p{i:02}")
            base = 20.0 + (0.3 * wk if i < risers else 0.0)
            rows["tokens"].append(base + rng.normal(scale=2.0))
    return pl.DataFrame(rows)


def _kinds(report, column="tokens"):
    return _series(report, column)


def test_panel_trend_names_the_minority_that_carries_it():
    report = onceover.profile(_panel(), detectors=["series"])
    k = _kinds(report)
    ent = k["entity_trend"]["evidence"][0]["metrics"]
    assert ent["entity_column"] == "person"
    assert ent["direction"] == "rise"
    assert ent["entities_with_pattern"] == 8
    assert ent["entities_tested"] == 40
    claim = k["entity_trend"]["claim"]
    assert "8 of the 40" in claim and "p00" in claim and "p04" in claim
    # The aggregate trend must say it is carried by a minority.
    agg = k["trend"]
    assert "minority of entities: 8 of 40" in agg["claim"]
    am = agg["evidence"][0]["metrics"]
    assert am["entities_with_trend"] == 8
    assert am["entities_tested"] == 40


def test_panel_trend_no_entity_carries_is_qualified():
    # Every entity rises slightly — too slightly for any single
    # series to clear the trend gates — while the aggregate rise
    # is strong. The aggregate statement must say that no
    # individual series shows the rise on its own.
    rng = np.random.default_rng(43)
    rows = {"t": [], "person": [], "tokens": []}
    for i in range(40):
        for wk in range(80):
            rows["t"].append(MONDAYS[wk])
            rows["person"].append(f"p{i:02}")
            rows["tokens"].append(20.0 + 0.05 * wk + rng.normal(scale=3.0))
    report = onceover.profile(pl.DataFrame(rows), detectors=["series"])
    k = _kinds(report)
    assert "entity_trend" not in k
    agg = k["trend"]
    assert "no individual person series shows a significant rise" in agg["claim"]
    am = agg["evidence"][0]["metrics"]
    assert am["entities_with_trend"] == 0
    assert am["entities_tested"] == 40


def test_panel_break_names_the_entities_that_share_it():
    rng = np.random.default_rng(22)
    rows = {"t": [], "person": [], "tokens": []}
    for i in range(30):
        for wk in range(40):
            rows["t"].append(MONDAYS[wk])
            rows["person"].append(f"p{i:02}")
            base = 20.0 + (8.0 if i < 6 and wk >= 20 else 0.0)
            rows["tokens"].append(base + rng.normal(scale=1.5))
    report = onceover.profile(pl.DataFrame(rows), detectors=["series"])
    k = _kinds(report)
    ent = k["entity_break"]["evidence"][0]["metrics"]
    assert ent["entity_column"] == "person"
    assert ent["direction"] == "up"
    assert ent["entities_with_pattern"] == 6
    assert ent["entities_tested"] == 30
    assert ent["breakpoint"] == "2023-05-22"  # MONDAYS[20]
    assert "6 of the 30" in k["entity_break"]["claim"]


# --- #239: per-entity series beyond the principal measure ----------------


def _entity_findings(report, kind=None):
    out = []
    for f in report.findings:
        if f.get("type") != "series_structure":
            continue
        m = f["evidence"][0]["metrics"]
        if m.get("kind") in ("entity_trend", "entity_break") and (
            kind is None or m["kind"] == kind
        ):
            out.append(f)
    return out


def _two_measure_panel(risers, seed):
    rng = np.random.default_rng(seed)
    rows = {"t": [], "person": [], "tokens": [], "clicks": []}
    for i in range(40):
        for wk in range(80):
            rows["t"].append(MONDAYS[wk])
            rows["person"].append(f"p{i:02}")
            rows["tokens"].append(20.0 + rng.normal(scale=2.0))
            rows["clicks"].append(
                20.0 + (0.3 * wk if i < risers else 0.0) + rng.normal(scale=2.0)
            )
    return pl.DataFrame(rows)


def test_panel_second_measure_trend_is_stated():
    # The principal measure (tokens: the observation counts tie, so
    # column order puts it first) is flat; the pattern lives in the
    # second measure. The per-entity pass states it for clicks, on
    # clicks' own tested and carrier counts.
    report = onceover.profile(_two_measure_panel(8, 31), detectors=["series"])
    ent = [
        f
        for f in _entity_findings(report, "entity_trend")
        if f["claim"].startswith("clicks")
    ]
    assert len(ent) == 1
    m = ent[0]["evidence"][0]["metrics"]
    assert m["entity_column"] == "person"
    assert m["direction"] == "rise"
    assert m["entities_with_pattern"] == 8
    assert m["entities_tested"] == 40
    assert "8 of the 40" in ent[0]["claim"]
    assert not [
        f for f in _entity_findings(report) if f["claim"].startswith("tokens")
    ]
    # The aggregate trend on clicks is qualified against clicks'
    # own per-entity account, not the principal measure's.
    agg = _series(report, "clicks")["trend"]
    assert "minority of entities: 8 of 40" in agg["claim"]
    am = agg["evidence"][0]["metrics"]
    assert am["entities_with_trend"] == 8
    assert am["entities_tested"] == 40


def test_panel_second_measure_too_few_carriers_stays_silent():
    # Two carriers are below the carrier bar of three: the second
    # measure states nothing, exactly as the principal would not.
    report = onceover.profile(_two_measure_panel(2, 32), detectors=["series"])
    assert not [
        f for f in _entity_findings(report) if f["claim"].startswith("clicks")
    ]


def test_panel_second_entity_column_is_examined():
    # Every person rises too slightly to carry a trend alone, but
    # the persons of a store rise together: grouping by the second
    # entity column (store) shows the pattern the first (person)
    # cannot state.
    rng = np.random.default_rng(33)
    rows = {"t": [], "person": [], "store": [], "tokens": []}
    for i in range(120):
        for wk in range(80):
            rows["t"].append(MONDAYS[wk])
            rows["person"].append(f"p{i:03}")
            rows["store"].append(f"s{i % 12:02}")
            rows["tokens"].append(20.0 + 0.05 * wk + rng.normal(scale=3.0))
    report = onceover.profile(pl.DataFrame(rows), detectors=["series"])
    by_store = [
        f
        for f in _entity_findings(report, "entity_trend")
        if f["evidence"][0]["metrics"]["entity_column"] == "store"
    ]
    assert len(by_store) == 1
    m = by_store[0]["evidence"][0]["metrics"]
    assert m["entities_tested"] == 12
    assert m["entities_with_pattern"] == 12
    assert not [
        f
        for f in _entity_findings(report, "entity_trend")
        if f["evidence"][0]["metrics"]["entity_column"] == "person"
    ]


def test_panel_measure_cap_is_stated_in_coverage():
    # Ten measures, all fully observed; the pattern sits in the
    # last by the observation ranking (ties keep column order),
    # past the eight-measure examination bound. It states nothing,
    # and coverage states the bound.
    rng = np.random.default_rng(34)
    rows = {"t": [], "person": []}
    measures = {f"m{j}": [] for j in range(10)}
    for i in range(40):
        for wk in range(80):
            rows["t"].append(MONDAYS[wk])
            rows["person"].append(f"p{i:02}")
            for j in range(10):
                v = 20.0 + rng.normal(scale=2.0)
                if j == 9 and i < 8:
                    v += 0.3 * wk
                measures[f"m{j}"].append(v)
    rows.update(measures)
    report = onceover.profile(pl.DataFrame(rows), detectors=["series"])
    assert not [
        f for f in _entity_findings(report) if f["claim"].startswith("m9")
    ]
    detail = report.coverage["series"]["detail"]
    assert "first 8 of 10 measures" in detail
