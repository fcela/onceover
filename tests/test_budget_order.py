"""Budget-order selection (issue #126): when a pass's
candidate cap binds, what gets examined must be chosen by a
cheap whole-population score, not by column position — and
the truncation must be disclosed in coverage.

Two passes are pinned here:

- ordered_partitions: 141 numeric responses against the
  standard budget's 128-response cap. A level-break response
  planted in the last position is screened and stated; the
  same data with the response planted first yields the
  identical statement.
- hotspots: 20 nominal columns give 1,140 candidate triples
  against the deep budget's 400-triple cap. A piled triple
  cell planted on the last three columns is stated; the same
  data with the triple on the first three columns yields the
  identical statement.

The planted frames are built with numpy, which is not a
library dependency, so this module skips where numpy is
unavailable (the wheel-test environment installs only the
wheel and pytest).
"""

import pytest

pytest.importorskip(
    "numpy",
    reason="planted frames need numpy (not a library dependency)",
)

import numpy as np
import polars as pl

import onceover


def _partitions_frame(planted_last: bool) -> pl.DataFrame:
    rng = np.random.default_rng(20261104)
    n = 600
    dates = pl.date_range(
        pl.date(2021, 1, 1), pl.date(2022, 8, 23), interval="1d", eager=True
    )
    fillers = {f"f{i:03d}": rng.standard_normal(n) for i in range(140)}
    planted = np.concatenate(
        [rng.normal(0.0, 0.5, n // 2), rng.normal(5.0, 0.5, n - n // 2)]
    )
    cols = {"date": dates}
    if planted_last:
        cols.update(fillers)
        cols["planted_response"] = planted
    else:
        cols["planted_response"] = planted
        cols.update(fillers)
    return pl.DataFrame(cols)


def _partition_hits(report) -> list:
    return [
        f
        for f in report.findings
        if f.get("type") == "ordered_partition"
        and f["evidence"][0]["metrics"]["response"] == "planted_response"
    ]


def test_partitions_planted_response_last_is_screened():
    report = onceover.profile(
        _partitions_frame(True), budget="standard", detectors=["ordered_partitions"]
    )
    hits = _partition_hits(report)
    assert hits, "planted response in last position was not screened"


def test_partitions_position_invariance():
    last = onceover.profile(
        _partitions_frame(True), budget="standard", detectors=["ordered_partitions"]
    )
    first = onceover.profile(
        _partitions_frame(False), budget="standard", detectors=["ordered_partitions"]
    )
    last_hits = _partition_hits(last)
    first_hits = _partition_hits(first)
    assert last_hits and first_hits
    assert last_hits[0]["claim"] == first_hits[0]["claim"]


def test_partitions_cap_disclosed_in_coverage():
    report = onceover.profile(
        _partitions_frame(True), budget="standard", detectors=["ordered_partitions"]
    )
    cov = report.data["coverage"]["ordered_partitions"]
    assert cov["status"].startswith("capped")
    assert "128 of 141 eligible responses screened" in cov["detail"]
    assert "prescore" in cov["detail"]
    assert "not column order" in cov["detail"]


def _hotspots_frame(planted_last: bool) -> tuple[pl.DataFrame, list[str]]:
    # One planted frame; the twin is a pure column permutation
    # of it, so the planted triple (always c17, c18, c19) sits
    # in the last three positions or the first three and every
    # value — hence the claim — is otherwise identical.
    rng = np.random.default_rng(20261104)
    n = 5000
    levels = np.array(["a", "b", "c", "d"])
    data = {f"c{i:02d}": levels[rng.integers(0, 4, n)] for i in range(20)}
    triple = (17, 18, 19)
    rows = rng.choice(n, size=800, replace=False)
    for c in triple:
        data[f"c{c:02d}"][rows] = "a"
    frame = pl.DataFrame(data)
    triple_cols = [f"c{c:02d}" for c in triple]
    if planted_last:
        return frame, triple_cols
    others = [c for c in frame.columns if c not in triple_cols]
    return frame.select(triple_cols + others), triple_cols


def _hotspot_hits(report, triple_cols: list[str]) -> list:
    return [
        f
        for f in report.findings
        if f.get("type") == "hotspot"
        and sorted(f["evidence"][0]["metrics"]["columns"]) == sorted(triple_cols)
    ]


def test_hotspots_planted_triple_last_is_examined():
    frame, triple_cols = _hotspots_frame(True)
    report = onceover.profile(frame, budget="deep", detectors=["hotspots"])
    hits = _hotspot_hits(report, triple_cols)
    assert hits, "planted triple in last position was not examined"
    payload = hits[0]["evidence"][0]["metrics"]
    assert payload["cell"] == {c: "a" for c in triple_cols}


def test_hotspots_position_invariance():
    last_frame, last_cols = _hotspots_frame(True)
    first_frame, first_cols = _hotspots_frame(False)
    last = onceover.profile(last_frame, budget="deep", detectors=["hotspots"])
    first = onceover.profile(first_frame, budget="deep", detectors=["hotspots"])
    last_hits = _hotspot_hits(last, last_cols)
    first_hits = _hotspot_hits(first, first_cols)
    assert last_hits and first_hits
    assert last_hits[0]["claim"] == first_hits[0]["claim"]


def test_hotspots_cap_disclosed_in_coverage():
    frame, _ = _hotspots_frame(True)
    report = onceover.profile(frame, budget="deep", detectors=["hotspots"])
    cov = report.data["coverage"]["hotspots"]
    assert "400 of 1140 candidate triples examined" in cov["detail"]
    assert "pre-score" in cov["detail"]
    assert "not column order" in cov["detail"]
