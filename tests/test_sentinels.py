"""Sentinel values as missing-data encodings (issue #55): a
repeated implausible value concentrated in a row window is
reported with its extent; genuine repeated values stay quiet."""

import random

import polars as pl

import onceover


def _hits(report) -> list:
    return [f for f in report.findings if f.get("type") == "sentinel_values"]


def _gaussian(seed: int, n: int = 2000) -> list[float]:
    rng = random.Random(seed)
    return [rng.gauss(50, 10) for _ in range(n)]


def test_sentinel_block_is_named_with_its_window():
    vals = _gaussian(44)
    rng = random.Random(7)
    for i in range(600, 760):
        if rng.random() < 0.5:
            vals[i] = -999.0
    report = onceover.profile(pl.DataFrame({"amount": vals}))
    hits = _hits(report)
    assert len(hits) == 1
    claim = hits[0]["claim"]
    assert "-999" in claim and "amount" in claim
    payload = hits[0]["evidence"][0]["metrics"]
    assert payload["value"] == -999.0
    assert payload["occurrences"] >= 20
    # The reported window is the shortest covering >= 80% of the
    # occurrences; it must lie inside the planted block (it need
    # not start at the block's edge).
    assert 600 <= payload["window_start_row"]
    assert payload["window_end_row"] <= 759
    assert payload["window_span_rows"] <= 160
    assert payload["window_occurrences"] * 5 >= payload["occurrences"] * 4
    assert payload["mean_excluding"] > 45.0 > payload["mean_with"]


def test_negative_code_in_nonnegative_counts_fires():
    rng = random.Random(3)
    vals = [rng.randrange(0, 6) for _ in range(1500)]
    for i in range(300, 420):
        vals[i] = -1
    report = onceover.profile(pl.DataFrame({"count": vals}))
    hits = _hits(report)
    assert len(hits) == 1
    assert hits[0]["evidence"][0]["metrics"]["value"] == -1.0


def test_clean_gaussian_is_quiet():
    report = onceover.profile(pl.DataFrame({"amount": _gaussian(45)}))
    assert _hits(report) == []


def test_zero_inflated_counts_are_quiet():
    rng = random.Random(5)
    vals = [0 if rng.random() < 0.4 else rng.randrange(1, 8) for _ in range(2000)]
    report = onceover.profile(pl.DataFrame({"count": vals}))
    assert _hits(report) == []


def test_scattered_far_value_is_quiet():
    vals = _gaussian(46)
    for i in range(0, 2000, 80):
        vals[i] = -999.0
    report = onceover.profile(pl.DataFrame({"amount": vals}))
    assert _hits(report) == []


def test_constant_column_is_quiet():
    report = onceover.profile(pl.DataFrame({"x": [7.5] * 500}))
    assert _hits(report) == []
