"""Issue #282: leakage signatures against a designated target.

Fixtures: a planted derived column fires with the column named;
a column that determines the target exactly fires on the
determination path; a strong but legitimate field stays silent;
a shuffled target stays silent; a numeric target with a
near-copy column fires; without a designated target the screen
does not run.
"""

import polars as pl
import pytest

import onceover


def _leakage(report):
    return [f for f in report.findings if f["type"] == "leakage_signature"]


def _coverage(report, pass_id):
    return report.coverage[pass_id]


def _rng(seed):
    state = [seed & 0xFFFFFFFFFFFFFFFF]

    def nxt():
        x = state[0]
        x ^= (x << 13) & 0xFFFFFFFFFFFFFFFF
        x ^= x >> 7
        x ^= (x << 17) & 0xFFFFFFFFFFFFFFFF
        state[0] = x
        return x

    return nxt


def test_derived_column_fires_and_names_it():
    nxt = _rng(2_820_001)
    n = 4000
    base = [nxt() % 5 for _ in range(n)]
    target = [["low", "mid", "high", "top", "peak"][v % 3] for v in base]
    # The derived column restates the target with 2% of labels flipped.
    derived = [
        t if nxt() % 100 >= 2 else ["low", "mid", "high"][nxt() % 3] for t in target
    ]
    noise = [nxt() % 7 for _ in range(n)]
    other = [["a", "b", "c"][nxt() % 3] for _ in range(n)]
    df = pl.DataFrame(
        {"y": target, "restated": derived, "noise": noise, "other": other}
    )
    report = onceover.profile(df, target="y", budget="standard")
    hits = _leakage(report)
    assert len(hits) == 1, [f["claim"] for f in report.findings]
    assert "'restated'" in hits[0]["claim"] and "'y'" in hits[0]["claim"]
    ev = hits[0]["evidence"][0]
    assert ev["metrics"]["strength"] >= 0.8
    assert ev["metrics"]["strength_measure"] == "entropy_fraction_corrected"
    assert ev["metrics"]["determination"] is False


def test_exact_determination_fires():
    nxt = _rng(2_820_002)
    n = 3000
    canceled = [nxt() % 100 < 37 for _ in range(n)]
    # A post-outcome status column: a deterministic refinement of
    # the outcome, recorded after it.
    status = [
        ("Canceled" if c else ("Check-Out" if nxt() % 2 else "Checked-In"))
        for c in canceled
    ]
    segment = [["Online TA", "Direct", "Groups"][nxt() % 3] for _ in range(n)]
    df = pl.DataFrame(
        {"is_canceled": canceled, "status": status, "segment": segment}
    )
    report = onceover.profile(df, target="is_canceled", budget="standard")
    hits = _leakage(report)
    assert len(hits) == 1, [f["claim"] for f in report.findings]
    assert "'status'" in hits[0]["claim"]
    assert "exactly" in hits[0]["claim"]
    assert hits[0]["evidence"][0]["metrics"]["determination"] is True


def test_strong_legitimate_field_stays_silent():
    nxt = _rng(2_820_003)
    n = 6000
    signals = [[nxt() % 2 for _ in range(n)] for _ in range(5)]
    target = [
        "yes" if sum(signals[k][i] for k in range(5)) >= 3 else "no"
        for i in range(n)
    ]
    data = {f"s{k}": [("p" if v else "q") for v in signals[k]] for k in range(5)}
    data["y"] = target
    df = pl.DataFrame(data)
    report = onceover.profile(df, target="y", budget="standard")
    assert _leakage(report) == []
    cov = _coverage(report, "leakage")
    assert "no leakage signature" in cov["detail"]


def test_shuffled_target_stays_silent():
    nxt = _rng(2_820_004)
    n = 3000
    target = [["a", "b", "c"][nxt() % 3] for _ in range(n)]
    # A permutation of the target as a column: same margins, no
    # row-level relation.
    perm = target[::2] + target[1::2]
    df = pl.DataFrame({"y": target, "shuffled_copy": perm})
    report = onceover.profile(df, target="y", budget="standard")
    assert _leakage(report) == []


def test_numeric_target_near_copy_fires():
    nxt = _rng(2_820_005)
    n = 4000
    y = [((nxt() % 100_000) / 100.0) for _ in range(n)]
    near_copy = [v + ((nxt() % 21) - 10) / 100.0 for v in y]
    unrelated = [((nxt() % 100_000) / 100.0) for _ in range(n)]
    df = pl.DataFrame({"y": y, "recorded_later": near_copy, "unrelated": unrelated})
    report = onceover.profile(df, target="y", budget="standard")
    hits = _leakage(report)
    assert len(hits) == 1, [f["claim"] for f in report.findings]
    assert "'recorded_later'" in hits[0]["claim"]
    assert hits[0]["evidence"][0]["metrics"]["strength_measure"] == "chatterjee_xi"


def test_no_target_no_screen():
    df = pl.DataFrame({"y": [1, 2, 3] * 100, "x": [1, 2, 3] * 100})
    report = onceover.profile(df, budget="standard")
    assert _leakage(report) == []
    cov = _coverage(report, "leakage")
    assert cov["status"] == "skipped"


def test_near_unique_column_does_not_determine_vacuously():
    # An imbalanced target and a near-unique continuous column:
    # every singleton group pairs with a single target value, but
    # that is vacuous, not determination (the Taiwanese Bankruptcy
    # shape, where 27 ratio columns "determined" the target before
    # the group-repetition guard).
    nxt = _rng(2_820_006)
    n = 6000
    target = ["yes" if nxt() % 100 < 3 else "no" for _ in range(n)]
    ratio = [((nxt() % 1_000_000) / 1000.0) for _ in range(n)]
    df = pl.DataFrame({"bankrupt": target, "ratio_x": ratio})
    report = onceover.profile(df, target="bankrupt", budget="standard")
    assert _leakage(report) == []
    cov = _coverage(report, "leakage")
    assert "no leakage signature" in cov["detail"]
