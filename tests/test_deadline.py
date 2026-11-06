"""In-pass deadline enforcement (issue #117, stage 2).

Stage 1 bounded the memory of the wide searches; the deadline,
however, used to be consulted only between passes, so one
expensive pass could overrun the caller's budget by its whole
runtime. These tests run synthetic frames sized so the
dependence screen, the hotspots scan, and the discontinuities
pass each overrun a short deadline, and assert that the pass
stops near the budget and that its coverage entry discloses how
much of the search space was examined. The examined counts are
timing-dependent by nature (the deadline is the documented
opt-in source of nondeterminism), so the assertions pin the
disclosure's shape and the examined < total inequality, not
exact counts; the exact-prefix determinism of the examined set
under a deterministic stop predicate is pinned by the Rust
unit tests in onceover-mines.
"""

import time

import numpy as np
import polars as pl

import onceover

SEED = 20261104


def _int_frame(n_cols: int, n_rows: int) -> pl.DataFrame:
    rng = np.random.default_rng(SEED)
    return pl.DataFrame(
        {f"c{j}": rng.integers(0, 8, size=n_rows) for j in range(n_cols)}
    )


def _float_frame(n_cols: int, n_rows: int) -> pl.DataFrame:
    rng = np.random.default_rng(SEED)
    data = {f"c{j}": rng.standard_normal(n_rows) for j in range(n_cols)}
    # A planted level step in c0 along c1, so the pass has real
    # work (and a real candidate) in flight when the deadline hits.
    half = n_rows // 2
    data["c0"][:half] -= 3.0
    data["c0"][half:] += 3.0
    data["c1"] = np.arange(n_rows, dtype=float)
    return pl.DataFrame(data)


def test_dependence_screen_stops_inside_stage1_and_discloses():
    df = _int_frame(1200, 100)
    t0 = time.time()
    report = onceover.profile(
        df, detectors=["numeric_dependence"], deadline_seconds=2.0
    )
    wall = time.time() - t0
    entry = report.data["coverage"]["dependence_screen"]
    assert entry["status"] == "capped", entry
    assert "stage-1 screening stopped at the deadline in its" in entry["detail"]
    assert "of 719400 pairs" in entry["detail"]
    # The unbounded screen takes several times the deadline on
    # this fixture; the stopped run must land near the budget,
    # not near the unbounded runtime.
    assert wall < 12.0, wall
    # The between-pass machinery also records the deadline.
    assert "deadline" in report.data["coverage"]


def test_hotspots_scan_stops_mid_enumeration_and_discloses():
    df = _int_frame(600, 2000)
    t0 = time.time()
    report = onceover.profile(df, detectors=["hotspots"], deadline_seconds=2.0)
    wall = time.time() - t0
    entry = report.data["coverage"]["hotspots"]
    assert entry["status"] == "capped", entry
    assert "of 179700 pair combinations examined" in entry["detail"]
    assert "deadline stopped the scan" in entry["detail"]
    # The stop lands in pair evaluation on this fixture, and a
    # partial pair screen selects no triples: their selection
    # spends the completed screen's pre-scores.
    assert "triples were not selected or examined" in entry["detail"]
    assert wall < 12.0, wall


def test_discontinuities_pass_stops_mid_pass_and_discloses():
    df = _float_frame(80, 8000)
    t0 = time.time()
    report = onceover.profile(
        df, detectors=["discontinuities"], deadline_seconds=2.0
    )
    wall = time.time() - t0
    entry = report.data["coverage"]["discontinuities"]
    assert entry["status"] == "capped", entry
    assert "deadline reached mid-pass" in entry["detail"]
    assert "of 80 targets fully screened" in entry["detail"]
    assert wall < 12.0, wall


def test_generous_deadline_changes_nothing():
    # A deadline that never binds must leave the report
    # byte-identical to the no-deadline run: no disclosure, no
    # status change, no finding change.
    df = _float_frame(24, 600)
    plain = onceover.profile(df)
    timed = onceover.profile(df, deadline_seconds=3600.0)
    assert timed.data == plain.data
    for name in ("dependence_screen", "hotspots", "discontinuities"):
        assert "deadline" not in plain.data["coverage"][name]["detail"]
