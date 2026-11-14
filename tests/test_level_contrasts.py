"""Per-level contrasts (issue #275): one level of a categorical
against the pooled rest on a numeric measure.

The mixed-pair screen tests level means; these fixtures plant
the three shapes it cannot state — a location-shifted group, a
scale-shifted group with equal means, and a constant-multiple
group — plus the controls that must stay silent: clean data, a
shuffled-level control, and a level below the row floor.
"""

import datetime as dt

import polars as pl

import onceover


def _noise(i: int) -> float:
    """Deterministic pseudo-noise in [0, 1): a hashed walk."""
    x = (i * 0x9E3779B97F4A7C15) & 0xFFFFFFFFFFFFFFFF
    x ^= x >> 29
    x = (x * 0xBF58476D1CE4E5B9) & 0xFFFFFFFFFFFFFFFF
    x ^= x >> 32
    return (x % 10_000) / 10_000.0


def _frame(sites, per_site, n_per_site=400):
    groups, values = [], []
    for s in sites:
        for i in range(n_per_site):
            groups.append(s)
            values.append(per_site(s, i))
    return pl.DataFrame({"site": groups, "reading": values})


def _contrasts(report):
    return [
        f
        for f in report.findings
        if f["type"] == "level_contrast"
        and f["evidence"][0]["method"]["detector"] == "level_contrasts"
    ]


def _metrics(finding):
    return finding["evidence"][0]["metrics"]


def test_location_shifted_level_fires():
    df = _frame(
        ["A", "B", "C", "K7"],
        lambda s, i: 3.5 + _noise(i) + _noise(i + 5000) + (1.2 if s == "K7" else 0.0),
    )
    report = onceover.profile(df)
    hits = [f for f in _contrasts(report) if _metrics(f)["level"] == "K7"]
    assert len(hits) == 1
    m = _metrics(hits[0])
    assert m["form"] in ("location", "location_scale")
    assert m["grouping_column"] == "site"
    assert m["measure_column"] == "reading"
    assert m["level_median"] > m["rest_median"]
    assert "no other level" in hits[0]["claim"]


def test_scale_shifted_level_with_equal_means_fires():
    def value(s, i):
        u = _noise(i) - 0.5 + (_noise(i + 9000) - 0.5)
        return 10.0 + (3.0 * u if s == "WIDE" else u)

    df = _frame(["A", "B", "WIDE"], value, n_per_site=500)
    report = onceover.profile(df)
    hits = [f for f in _contrasts(report) if _metrics(f)["level"] == "WIDE"]
    assert len(hits) == 1
    m = _metrics(hits[0])
    assert m["form"] == "scale"
    assert m["iqr_ratio"] > 1.5
    # The centres agree; only the spread moved.
    assert abs(m["level_median"] - m["rest_median"]) < 0.2


def test_constant_multiple_level_gets_the_multiple_form():
    def value(s, i):
        base = 20.0 + 10.0 * _noise(i) + 5.0 * _noise(i + 3000)
        return base * 2.54 if s == "CM" else base

    df = _frame(["A", "B", "CM"], value)
    report = onceover.profile(df)
    # Only the multiple is stated: A and B look narrow against
    # a typical level only because CM is wide.
    assert len(_contrasts(report)) == 1
    hits = [f for f in _contrasts(report) if _metrics(f)["level"] == "CM"]
    assert len(hits) == 1
    m = _metrics(hits[0])
    assert m["form"] == "constant_multiple"
    assert abs(m["quotient"] - 2.54) < 0.05
    assert "near-constant multiple" in hits[0]["claim"]


def test_clean_control_is_silent():
    df = _frame(["A", "B", "C", "D"], lambda s, i: 3.5 + _noise(i) + _noise(i + 5000))
    report = onceover.profile(df)
    assert _contrasts(report) == []
    assert report.coverage["level_contrasts"]["status"] == "ran_exact"


def test_shuffled_level_control_is_silent():
    df = _frame(["A", "B", "C", "D"], lambda s, i: 3.5 + _noise(i) + _noise(i + 5000))
    sites = df["site"].to_list()
    n = len(sites)
    shuffled = sites[:]
    for i in range(n):
        j = (i * 7919 + 13) % n
        shuffled[i], shuffled[j] = shuffled[j], shuffled[i]
    df = df.with_columns(pl.Series("site", shuffled))
    report = onceover.profile(df)
    assert _contrasts(report) == []


def test_level_below_the_row_floor_is_not_stated():
    # A tiny level shifted enormously: below the 30-row floor,
    # so the scan must not state it.
    groups = ["A"] * 500 + ["B"] * 500 + ["TINY"] * 10
    values = (
        [3.5 + _noise(i) + _noise(i + 5000) for i in range(500)]
        + [3.5 + _noise(i) + _noise(i + 5000) for i in range(500)]
        + [50.0] * 10
    )
    df = pl.DataFrame({"site": groups, "reading": values})
    report = onceover.profile(df)
    assert _contrasts(report) == []


def test_calendar_day_of_week_contrast_fires():
    start = dt.date(2023, 1, 1)  # a Sunday
    days = [start + dt.timedelta(days=d) for d in range(731)]
    values = []
    for d, day in enumerate(days):
        v = 10.0 + _noise(d) + _noise(d + 4000)
        if day.weekday() == 5:  # Saturday
            v += 3.0
        values.append(v)
    df = pl.DataFrame({"day": days, "reading": values})
    report = onceover.profile(df)
    hits = [
        f
        for f in _contrasts(report)
        if _metrics(f).get("calendar_part") == "day_of_week"
    ]
    assert len(hits) == 1
    assert _metrics(hits[0])["level"] == "Saturday"


def test_detector_registered_and_in_default_set():
    registry = {d["id"]: d for d in onceover.available_detectors()}
    assert registry["level_contrasts"]["activation"] == "default"
    df = _frame(["A", "B"], lambda s, i: 3.5 + _noise(i), n_per_site=100)
    report = onceover.profile(df, detectors=["level_contrasts"])
    assert report.coverage["level_contrasts"]["status"] == "ran_exact"
