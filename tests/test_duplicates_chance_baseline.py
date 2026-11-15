"""Duplicate statements against their chance/support baselines.

The duplicate passes compute how much repetition chance or a
small support already explains. The claim must state that
baseline, the score must use it, and a table whose compared
columns can only form a few distinct rows must not be reported
as duplicated records.
"""
from pathlib import Path

import polars as pl
import pytest

import onceover

np = pytest.importorskip("numpy", reason="planted frames need numpy")

CORPUS = Path(__file__).resolve().parent.parent / "benchmarks" / "corpus"


def _near(report):
    return [f for f in report.findings if f.get("type") == "near_duplicate_rows"]


def _detector_score(f):
    return f.get("detector_interestingness", f["interestingness"])


def test_noise_control_produces_no_duplicates_claim():
    # Three independent 5-level columns: 125 possible rows in
    # 5,000 draws. Every repeat is forced by the support.
    rng = np.random.default_rng(1)
    df = pl.DataFrame({c: rng.integers(0, 5, size=5000) for c in "abc"})
    report = onceover.profile(df)
    assert not [f for f in report.findings if f.get("category") == "duplicates"], [
        f["claim"] for f in report.findings if f.get("category") == "duplicates"
    ]
    assert "repeat earlier values" not in report.to_digest(4000)
    # The saturation is stated as a cardinality fact instead.
    conc = [
        f for f in report.findings
        if f.get("category") == "concentration" and "possible combinations" in f["claim"]
    ]
    assert len(conc) == 1, [f["claim"] for f in report.findings]
    m = conc[0]["evidence"][0]["metrics"]
    assert m["support_size"] == 125
    assert m["distinct_rows"] == 125


def test_near_duplicate_claim_states_the_chance_baseline():
    # Adult: the near-duplicate count sits against a non-trivial
    # chance baseline (hundreds of coincident pairs expected from
    # the compared columns' own value distributions).
    csv = CORPUS / "data" / "business" / "adult-census-income" / (
        "adult-census-income_uci-2.csv")
    df = pl.read_csv(csv, infer_schema_length=None)
    report = onceover.profile(df, detectors=["row_consistency", "numeric_dependence"])
    hits = [
        f for f in _near(report)
        if f["evidence"][0]["metrics"]["varying_column"] in ("fnlwgt", "final_weight")
    ]
    assert len(hits) == 1, [f["claim"] for f in _near(report)]
    hit = hits[0]
    m = hit["evidence"][0]["metrics"]
    assert m["expected_by_chance"] > 100
    assert "expected by chance" in hit["claim"]
    assert f"{m['over_chance']:.1f}x" in hit["claim"]
    # Chance already accounts for more than 1% of the table, so
    # the detector score is capped however large the share is.
    assert _detector_score(hit) <= 0.70 + 1e-9


def test_same_count_scores_lower_against_a_high_baseline():
    # A planted block of near duplicates on high-cardinality
    # columns (chance baseline near zero) must score above the
    # Adult statement, whose baseline is non-trivial.
    rng = np.random.default_rng(0)
    n = 4000
    base = pl.DataFrame({
        "a": rng.normal(size=n),
        "b": rng.lognormal(size=n),
        "c": rng.integers(0, 1000, size=n),
        "d": [f"L{v}" for v in rng.integers(0, 40, size=n)],
    })
    extra = base.sample(400, seed=1).with_columns(pl.col("a") + 100.0)
    planted = _near(onceover.profile(
        pl.concat([base, extra]), detectors=["row_consistency", "numeric_dependence"]))
    assert len(planted) == 1
    pm = planted[0]["evidence"][0]["metrics"]
    assert pm["expected_by_chance"] < 1.0
    assert "expected by chance" not in planted[0]["claim"]
    assert _detector_score(planted[0]) > 0.80


def test_planted_exact_duplicates_still_fire():
    rng = np.random.default_rng(7)
    n = 3000
    base = pl.DataFrame({
        "a": rng.normal(size=n),
        "b": rng.normal(size=n),
        "c": rng.integers(0, 1_000_000, size=n),
        "d": [f"K{v}" for v in rng.integers(0, 1_000_000, size=n)],
    })
    frame = pl.concat([base, base.head(300)])
    report = onceover.profile(frame)
    hits = [f for f in report.findings if f.get("type") == "duplicate_rows"]
    assert len(hits) == 1, [f["claim"] for f in report.findings]
    m = hits[0]["evidence"][0]["metrics"]
    assert m["n_redundant"] == 300
    assert m["support_size"] > frame.height
    assert _detector_score(hits[0]) >= 0.70
    assert "repeat earlier values" in hits[0]["claim"]
