"""Series along the time axis: axis gaps, trend, cycle, persistence, levels (#131)."""
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
    t = [datetime(2020, 1, 1) + timedelta(hours=i) for i in range(N)]
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
