"""Statements that noise must not produce (issues #123, #124).

Every column here is independent, so a dated event, a key, or a format mix
stated about these columns is false by construction.
"""
from datetime import date, timedelta

import polars as pl
import pytest

import onceover

# The wheel-test environment installs only the library and pytest.
np = pytest.importorskip("numpy", reason="planted frames need numpy")


def _claims(report, fragment):
    return [f["claim"] for f in report.findings if fragment in f["claim"]]


def _dated_noise(seed, n=3000):
    rng = np.random.default_rng(seed)
    return pl.DataFrame({
        "when": [date(2010, 1, 1) + timedelta(days=i) for i in range(n)],
        "normal": rng.normal(size=n),
        "lognormal": rng.lognormal(size=n),
        "student": rng.standard_t(2, size=n),
        "uniform": rng.uniform(size=n),
    })


def test_heavy_tailed_noise_has_no_transient_events():
    events = 0
    for seed in range(5):
        report = onceover.profile(_dated_noise(seed), detectors=["breaks"])
        events += sum(f.get("type") == "transient_excursion" for f in report.findings)
    # Before: up to 10 per heavy-tailed series, 100 across these frames.
    assert events <= 2


def test_planted_event_survives_in_heavy_tailed_noise():
    frame = _dated_noise(0)
    values = frame["lognormal"].to_numpy().copy()
    values[1500:1510] += 200.0
    report = onceover.profile(
        frame.with_columns(pl.Series("lognormal", values)), detectors=["breaks"]
    )
    hits = [
        f for f in report.findings
        if f.get("type") == "transient_excursion" and f["claim"].startswith("lognormal")
    ]
    assert any("2014-02" in f["claim"] for f in hits), [f["claim"] for f in hits]


def test_fractional_floats_are_not_identifiers_or_keys():
    rng = np.random.default_rng(1)
    report = onceover.profile(pl.DataFrame({"m": rng.normal(size=1000)}))
    assert not _claims(report, "unique identifier")
    assert not _claims(report, "primary key candidate")
    assert "key candidate" not in report.overview["headline"]


def test_whole_number_floats_keep_the_identifier_statement():
    report = onceover.profile(pl.DataFrame({"n": [float(i) for i in range(1000)]}))
    assert _claims(report, "unique identifier")


def test_digit_count_and_word_shape_are_not_format_mixes():
    n = 600
    frame = pl.DataFrame({
        "code": [f"H{i % 40}" for i in range(n)],
        "email": [f"user{i}@example.com" for i in range(n)],
        "state": [("New York" if i % 2 else "California") for i in range(n)],
    })
    report = onceover.profile(frame, detectors=["column_profile"])
    assert not _claims(report, "mixes value formats")


def test_real_format_mixes_still_fire():
    n = 600
    frame = pl.DataFrame({
        "when": [
            (f"2021-03-{i % 28 + 1:02d}" if i % 3 else f"03/{i % 28 + 1:02d}/2021")
            for i in range(n)
        ],
        "label": [("alpha" if i % 3 else "ALPHA") for i in range(n)],
    })
    report = onceover.profile(frame, detectors=["column_profile"])
    assert _claims(report, "column when mixes value formats")
    assert _claims(report, "column label mixes value formats")


def test_subset_extreme_needs_an_ordinary_global_value():
    # 9999 sits far outside the column's fence: its subset says nothing.
    rng = np.random.default_rng(2)
    n = 4000
    values = rng.normal(50, 5, size=n)
    values[rng.choice(n, 40, replace=False)] = 9999.0
    frame = pl.DataFrame({
        "group": [f"G{v}" for v in rng.integers(0, 8, size=n)],
        "value": values,
    })
    report = onceover.profile(frame, detectors=["surprises"])
    assert not [c for c in _claims(report, "9999") if c.startswith("within")]


def test_later_columns_keep_their_extreme_scan():
    # Early columns' subset statements must not use up the whole budget.
    rng = np.random.default_rng(3)
    n = 2000
    data = {f"g{i}": [f"L{v}" for v in rng.integers(0, 6, size=n)] for i in range(3)}
    for i in range(30):
        data[f"m{i}"] = rng.standard_t(2, size=n)
    data["late"] = rng.normal(size=n)
    data["late"][7] = 500.0
    report = onceover.profile(pl.DataFrame(data), detectors=["surprises"])
    assert any(c.startswith("late ") for c in _claims(report, "500"))
