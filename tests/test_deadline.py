"""In-pass deadline enforcement (issue #117, stage 2).

Stage 1 bounded the memory of the wide searches; the deadline,
however, used to be consulted only between passes, so one
expensive pass could overrun the caller's budget by its whole
runtime. These tests assert that a stop is disclosed in the
pass's coverage entry with the examined share of its search
space.

The three stop tests use an already-expired deadline
(`deadline_seconds=0.0`). The passes that poll the deadline
inside their own loops (dependence screen, hotspots,
discontinuities) are dispatched even then, stop at their first
work quantum, and record the stop with 0 examined — so the
exact disclosure strings are deterministic at any machine
speed and are pinned verbatim. (They originally used a 2 s
deadline and needed each pass to still be running when it
expired; on an idle machine the fixtures finish inside 2 s and
the stop never engaged.) A stop that lands mid-pass remains
timing-dependent in *where* it lands; the examined set is
always a prefix of the pass's deterministic enumeration order,
and that property is pinned by the Rust unit tests in
onceover-mines with a counting stop predicate.
"""

import time

import polars as pl
import pytest

import onceover

np = pytest.importorskip("numpy", reason="planted frames need numpy")

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


def test_dependence_screen_stops_at_first_quantum_and_discloses():
    df = _int_frame(1200, 100)
    t0 = time.time()
    report = onceover.profile(
        df, detectors=["numeric_dependence"], deadline_seconds=0.0
    )
    wall = time.time() - t0
    entry = report.data["coverage"]["dependence_screen"]
    assert entry["status"] == "capped", entry
    assert entry["detail"] == (
        "0 of 719400 numeric pairs (deadline); stage-1 screening "
        "stopped at the deadline in its first pass after 0 of 719400 "
        "pairs; survivor selection needs the complete screen, so no "
        "pairs were selected and no detailed statistics were "
        "computed; 0 strong pairwise, 0 redundancy links"
    ), entry
    assert wall < 12.0, wall
    # The between-pass machinery also records the deadline.
    assert "deadline" in report.data["coverage"]


def test_hotspots_scan_stops_at_first_combination_and_discloses():
    df = _int_frame(600, 2000)
    t0 = time.time()
    report = onceover.profile(df, detectors=["hotspots"], deadline_seconds=0.0)
    wall = time.time() - t0
    entry = report.data["coverage"]["hotspots"]
    assert entry["status"] == "capped", entry
    assert entry["detail"] == (
        "600 eligible columns, 0 of 179700 pair combinations "
        "examined, 0 of 14555700 pair cells examined before the "
        "deadline stopped the scan; triples were not selected or "
        "examined — selection needs the complete pair screen; "
        "counts over the encoded sample of 2000 rows"
    ), entry
    assert wall < 12.0, wall


def test_discontinuities_pass_stops_before_first_target_and_discloses():
    df = _float_frame(80, 8000)
    t0 = time.time()
    report = onceover.profile(
        df, detectors=["discontinuities"], deadline_seconds=0.0
    )
    wall = time.time() - t0
    entry = report.data["coverage"]["discontinuities"]
    assert entry["status"] == "capped", entry
    assert entry["detail"] == (
        "0 numeric targets screened over 0 axis units and 0 "
        "diagonal units (16 directions x pairs); 0 candidates "
        "flagged on the evaluation half (|z| >= 2, held-out gain "
        "> 0), 0 of the typed leaders rejected as steep smooth "
        "transitions; 0 jumps reported; deadline reached mid-pass: "
        "0 of 80 targets fully screened, 0 stopped mid-screen "
        "(findings withheld), 80 not started"
    ), entry
    assert wall < 12.0, wall


def test_generous_deadline_changes_nothing():
    # A deadline that never binds must leave the report
    # byte-identical to the no-deadline run: no disclosure, no
    # status change, no finding change.
    df = _float_frame(24, 600)
    plain = onceover.profile(df)
    timed = onceover.profile(df, deadline_seconds=3600.0)
    assert timed.data == plain.data
    for name in ("dependence_screen", "hotspots", "discontinuities",
                 "surprises", "duplicate_encodings"):
        assert "deadline" not in plain.data["coverage"][name]["detail"]


_PASS_OF = {
    "column_profile": "column_cards",
    "gbm": "predictability_screen",
    "prism": "prism_rules",
    "numeric_dependence": "dependence_screen",
    "identities": "arithmetic_identities",
}


@pytest.mark.parametrize("kwargs", [
    {},
    {"budget": "quick"},
    {"budget": "deep"},
    {"deadline_seconds": 0.0},
    {"budget": "quick", "deadline_seconds": 0.0},
    {"detectors": ["keys"]},
    {"detectors": ["surprises"]},
])
def test_every_registered_detector_has_a_coverage_entry(kwargs):
    # A pass that did not run says why; none is silently absent.
    df = _float_frame(6, 400).with_columns(
        pl.Series("g", ["a", "b", "c", "d"] * 100))
    coverage = onceover.profile(df, **kwargs).data["coverage"]
    missing = [d["id"] for d in onceover.available_detectors()
               if _PASS_OF.get(d["id"], d["id"]) not in coverage]
    assert not missing, (kwargs, missing)
