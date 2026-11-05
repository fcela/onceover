"""Column content: what values are, and which do not fit (issue #133)."""
from datetime import date, timedelta

import polars as pl
import pytest

import onceover

np = pytest.importorskip("numpy", reason="planted frames need numpy")


def _kinds(report, column=None):
    out = {}
    for f in report.findings:
        if f.get("type") != "content_anomaly":
            continue
        if column and f"column {column} " not in f["claim"]:
            continue
        out.setdefault(f["evidence"][0]["metrics"]["kind"], f)
    return out


def _profile(**columns):
    n = len(next(iter(columns.values())))
    rng = np.random.default_rng(0)
    frame = pl.DataFrame({"filler": rng.normal(size=n), **columns})
    return onceover.profile(frame, detectors=["content"])


def test_numbers_stored_as_text_with_exceptions():
    values = [f"{i * 1.5:,.2f}" for i in range(2000)]
    for i in (5, 50, 500):
        values[i] = "N/A"
    k = _kinds(_profile(amount=values), "amount")["numbers_as_text"]
    m = k["evidence"][0]["metrics"]
    assert m["exceptions"] == 3
    assert m["exception_values"] == ["N/A"]
    assert "'N/A' (3)" in k["claim"]


def test_dates_stored_as_text():
    base = date(2020, 1, 1)
    values = [(base + timedelta(days=i % 900)).isoformat() for i in range(1000)]
    assert "dates_as_text" in _kinds(_profile(when=values), "when")


def test_email_exceptions_listed_in_full():
    values = [f"user{i}@example.com" for i in range(1000)]
    values[10] = "not-an-email"
    k = _kinds(_profile(contact=values), "contact")["email"]
    assert k["evidence"][0]["metrics"]["exceptions"] == 1
    assert "'not-an-email'" in k["claim"]


def test_fully_conforming_format_is_quiet():
    values = [f"user{i}@example.com" for i in range(1000)]
    assert "email" not in _kinds(_profile(contact=values), "contact")


def test_label_case_and_spacing_variants():
    labels = ["New York", "Chicago", "Boston"]
    values = [labels[i % 3] for i in range(3000)]
    for i in range(0, 300, 3):
        values[i] = "NEW YORK"
    for i in range(300, 600, 3):
        values[i] = " New York "
    k = _kinds(_profile(city=values), "city")["label_variants"]
    assert k["evidence"][0]["metrics"]["variant_groups"] == 1
    assert "'NEW YORK' (100)" in k["claim"]


def test_misspelled_rare_label():
    labels = ["California", "Texas", "Florida", "New York"]
    values = [labels[i % 4] for i in range(4000)]
    for i in (3, 7, 11, 15, 19, 23):
        values[i] = "Califronia"
    k = _kinds(_profile(state=values), "state")["possible_misspelling"]
    assert "'Califronia' (6) resembles 'California'" in k["claim"]


def test_distinct_labels_are_not_misspellings():
    labels = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"]
    values = [labels[i % 6] for i in range(3000)]
    kinds = _kinds(_profile(code=values), "code")
    assert "possible_misspelling" not in kinds
    assert "label_variants" not in kinds


def test_long_text():
    values = [f"note {i}" for i in range(2000)]
    for i in (9, 99, 999):
        values[i] = "x" * 5000
    m = _kinds(_profile(note=values), "note")["long_text"]["evidence"][0]["metrics"]
    assert m["long_values"] == 3
    assert m["longest"] == 5000


def test_repeated_extreme_counts_match():
    rng = np.random.default_rng(1)
    y = rng.normal(size=5000)
    y[rng.choice(5000, 5, replace=False)] = 50.0
    k = _kinds(_profile(y=y), "y")["repeated_extreme"]
    m = k["evidence"][0]["metrics"]
    assert m["value"] == 50.0
    assert m["rows"] == int((y == 50.0).sum())


def test_sentinel_code_among_ordinary_integers():
    rng = np.random.default_rng(2)
    age = rng.integers(18, 90, size=5000)
    age[rng.choice(5000, 50, replace=False)] = 9999
    k = _kinds(_profile(age=age), "age")["repeated_extreme"]
    assert k["evidence"][0]["metrics"]["value"] == 9999


def test_stray_negative_values():
    rng = np.random.default_rng(3)
    y = rng.lognormal(3, 1, size=5000)
    idx = rng.choice(5000, 6, replace=False)
    y[idx] *= -1
    k = _kinds(_profile(price=y), "price")["minority_sign"]
    m = k["evidence"][0]["metrics"]
    assert m["minority_rows"] == 6
    assert m["furthest"] == float(y.min())


def test_zero_inflation():
    rng = np.random.default_rng(4)
    y = np.where(rng.random(5000) < 0.6, 0.0, rng.lognormal(size=5000))
    m = _kinds(_profile(y=y), "y")["zero_inflation"]["evidence"][0]["metrics"]
    assert m["zeros"] == int((y == 0).sum())


def test_two_modes():
    rng = np.random.default_rng(5)
    y = np.where(rng.random(5000) < 0.5, rng.normal(-5, 1, size=5000), rng.normal(5, 1, size=5000))
    m = _kinds(_profile(y=y), "y")["two_modes"]["evidence"][0]["metrics"]
    assert abs(m["lower_mode"] + 5) < 1 and abs(m["upper_mode"] - 5) < 1


def test_far_future_dates():
    base = date(2019, 1, 1)
    values = [base + timedelta(days=i % 1500) for i in range(3000)]
    for i in (1, 2, 3, 4, 5):
        values[i] = date(2099, 12, 31)
    k = _kinds(_profile(when=values), "when")["temporal_outlier"]
    m = k["evidence"][0]["metrics"]
    assert m["rows"] == 5 and m["extreme"] == "2099-12-31"


def test_noise_is_quiet():
    bad = []
    for seed in range(5):
        rng = np.random.default_rng(seed)
        n = 5000
        frame = pl.DataFrame({
            "normal": rng.normal(size=n),
            "lognormal": rng.lognormal(size=n),
            "student": rng.standard_t(2, size=n),
            "exponential": rng.exponential(size=n),
            "uniform": rng.uniform(size=n),
            "counts": rng.poisson(30, size=n),
            "level": [f"L{v}" for v in rng.integers(0, 12, size=n)],
            "when": [date(2015, 1, 1) + timedelta(days=int(v)) for v in rng.integers(0, 2000, size=n)],
        })
        report = onceover.profile(frame, detectors=["content"])
        bad += [f["claim"] for f in report.findings]
    assert not bad, bad


def test_clean_yes_no_column_is_quiet_and_exceptions_are_not():
    clean = ["yes" if i % 3 else "no" for i in range(1000)]
    dirty = list(clean)
    dirty[4] = "NaN"
    report = _profile(clean=clean, dirty=dirty)
    assert "booleans_as_text" not in _kinds(report, "clean")
    assert "'NaN' (1)" in _kinds(report, "dirty")["booleans_as_text"]["claim"]


def test_codes_and_model_names_are_not_misspellings():
    values = ["Atlas 5" if i % 2 else "Proton M" for i in range(2000)]
    values[1] = "Atlas 3"
    values[3] = "Proton D"
    assert "possible_misspelling" not in _kinds(_profile(model=values), "model")


def test_small_discrete_grids_do_not_read_as_modes():
    rng = np.random.default_rng(6)
    grid = np.array([20, 25, 26, 27, 28, 30, 33, 37, 38, 39, 40, 41, 45, 50, 55, 58,
                     60, 64, 70, 80, 90, 95])
    y = rng.choice(grid, size=2000)
    assert "two_modes" not in _kinds(_profile(y=y), "y")
