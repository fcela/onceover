"""Granularity changes in float columns (issue #56): a series that
becomes integer-valued partway (a rounding/recording regime change)
is reported with its seam row."""

import random

import polars as pl

import onceover


def _series(round_from: int | None, n: int = 2000) -> pl.DataFrame:
    rng = random.Random(46)
    vals = []
    for i in range(n):
        v = rng.gauss(50, 10)
        vals.append(round(v) if round_from is not None and i >= round_from else v)
    return pl.DataFrame({"amount": vals})


def _granularity_hits(report) -> list:
    return [
        f
        for f in report.findings
        if f.get("type") == "format_variation" and "granularity" in f["claim"]
    ]


def test_rounding_change_is_reported_at_the_seam():
    report = onceover.profile(_series(round_from=1000))
    hits = _granularity_hits(report)
    assert len(hits) == 1
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["change_row"] == 1000
    assert metrics["integer_share_before"] < 0.05
    assert metrics["integer_share_after"] > 0.95
    assert report.data["coverage"]["granularity"]["status"] == "ran_exact"


def test_single_regime_columns_do_not_fire():
    assert _granularity_hits(onceover.profile(_series(round_from=None))) == []
    assert _granularity_hits(onceover.profile(_series(round_from=0))) == []


def test_short_columns_do_not_fire():
    assert _granularity_hits(onceover.profile(_series(round_from=100, n=300))) == []
