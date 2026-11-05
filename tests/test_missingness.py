"""Missing values: rates, the patterns behind them, co-missing blocks (#100).

Every count asserted here is recomputed with Polars from the same frame.
"""
from datetime import date, timedelta

import polars as pl
import pytest

import onceover

# The wheel-test environment installs only the library and pytest.
np = pytest.importorskip("numpy", reason="planted frames need numpy")


def _missing(report):
    return [f for f in report.findings if f["category"] == "missingness"]


def _for(report, column):
    return [f for f in _missing(report) if f["claim"].split(" ")[0] == column]


def _metrics(finding):
    return finding["evidence"][0]["metrics"]


def _frame(seed=0, n=5000):
    rng = np.random.default_rng(seed)
    return rng, {
        "a": rng.normal(size=n),
        "b": rng.lognormal(size=n),
        "group": [f"G{v}" for v in rng.integers(0, 4, size=n)],
        "other": [f"H{v}" for v in rng.integers(0, 3, size=n)],
    }


def _null_where(values, mask):
    return [None if m else float(v) for v, m in zip(values, mask)]


def test_rate_is_stated_with_exact_counts_at_moderate_rates():
    rng, d = _frame()
    n = 5000
    for rate in (0.05, 0.2, 0.5, 0.9):
        d["y"] = _null_where(rng.normal(size=n), rng.random(n) < rate)
        frame = pl.DataFrame(d)
        hits = _for(onceover.profile(frame), "y")
        assert len(hits) == 1
        expected = frame["y"].null_count()
        assert _metrics(hits[0])["missing"] == expected
        assert f"{expected:,} of 5,000" in hits[0]["claim"]


def test_trace_rate_is_quiet():
    rng, d = _frame()
    d["y"] = _null_where(rng.normal(size=5000), rng.random(5000) < 0.002)
    assert not _for(onceover.profile(pl.DataFrame(d)), "y")


def test_column_missing_at_random_gets_only_its_rate():
    for seed in range(6):
        rng, d = _frame(seed)
        d["y"] = _null_where(rng.normal(size=5000), rng.random(5000) < 0.1)
        hit = _for(onceover.profile(pl.DataFrame(d)), "y")[0]
        assert "pattern" not in _metrics(hit), hit["claim"]


def test_group_concentration_matches_polars():
    # 5% overall, 9% at the worst station: the shape in #100.
    rng = np.random.default_rng(3)
    n = 40000
    station = rng.integers(0, 5, size=n)
    rate = np.where(station == 2, 0.09, 0.04)
    frame = pl.DataFrame({
        "station": [f"S{v}" for v in station],
        "co": _null_where(rng.normal(size=n), rng.random(n) < rate),
        "temp": rng.normal(size=n),
    })
    hit = _for(onceover.profile(frame), "co")[0]
    m = _metrics(hit)
    assert m["pattern"] == "level"
    assert m["pattern_column"] == "station"
    assert m["pattern_level"] == "S2"
    subset = frame.filter(pl.col("station") == "S2")
    assert m["pattern_rows"] == subset.height
    assert m["pattern_missing_rows"] == subset["co"].null_count()
    assert "station = S2" in hit["claim"]
    assert m["level_rates"][0]["level"] == "S2"


def test_threshold_in_a_numeric_column():
    rng, d = _frame()
    x = rng.normal(size=5000)
    d["x"] = x
    d["y"] = _null_where(rng.normal(size=5000), x > 1.0)
    frame = pl.DataFrame(d)
    m = _metrics(_for(onceover.profile(frame), "y")[0])
    assert m["pattern"] == "threshold"
    assert m["pattern_column"] == "x"
    assert m["pattern_missing_above"] is True
    assert abs(m["pattern_cut"] - 1.0) < 0.05
    above = frame.filter(pl.col("x") > m["pattern_cut"])
    assert m["pattern_rows"] == above.height
    assert m["pattern_missing_rows"] == above["y"].null_count()


def test_onset_along_the_time_axis():
    rng, d = _frame()
    n = 5000
    d["when"] = [date(2010, 1, 1) + timedelta(days=i) for i in range(n)]
    onset = int(n * 0.7)
    d["y"] = _null_where(rng.normal(size=n), np.arange(n) > onset)
    frame = pl.DataFrame(d)
    hit = _for(onceover.profile(frame), "y")[0]
    m = _metrics(hit)
    assert m["pattern"] == "onset"
    assert m["pattern_missing_after"] is True
    assert m["pattern_cut"] == (date(2010, 1, 1) + timedelta(days=onset)).isoformat()
    assert m["pattern_missing_rows"] == frame["y"].null_count()


def test_co_missing_block_is_one_statement():
    rng, d = _frame()
    n = 5000
    block = rng.random(n) < 0.15
    d["p"] = _null_where(rng.normal(size=n), block)
    d["q"] = _null_where(rng.normal(size=n), block)
    frame = pl.DataFrame(d)
    hits = [f for f in _missing(onceover.profile(frame)) if "p, q" in f["claim"]]
    assert len(hits) == 1
    m = _metrics(hits[0])
    assert m["block"] is True
    assert m["missing_together"] == frame.filter(pl.col("p").is_null()).height
    assert not _for(onceover.profile(frame), "p")


def test_independent_gaps_are_not_a_block():
    rng, d = _frame()
    n = 5000
    d["p"] = _null_where(rng.normal(size=n), rng.random(n) < 0.9)
    d["q"] = _null_where(rng.normal(size=n), rng.random(n) < 0.9)
    report = onceover.profile(pl.DataFrame(d))
    assert not [f for f in _missing(report) if "p, q" in f["claim"]]


def test_position_in_the_frame_does_not_matter():
    rng, d = _frame()
    n = 5000
    d["y"] = _null_where(rng.normal(size=n), rng.random(n) < 0.3)
    last = pl.DataFrame(d)
    first = last.select(["y"] + [c for c in last.columns if c != "y"])
    assert _for(onceover.profile(last), "y")[0]["claim"] == (
        _for(onceover.profile(first), "y")[0]["claim"]
    )


def test_appears_in_the_digest():
    rng, d = _frame()
    d["y"] = _null_where(rng.normal(size=5000), rng.random(5000) < 0.3)
    assert "y is missing on" in onceover.profile(pl.DataFrame(d)).to_digest(4000)
