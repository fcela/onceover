"""Public Python API checks for M5 regimes, date shifts, and associations."""

import datetime as dt
import random

import polars as pl
import pytest

import onceover


@pytest.mark.parametrize("budget", ["standard", "deep"])
def test_m5_equations_holdout_and_linear_control(budget):
    rng = random.Random(1234567)
    gate = [rng.uniform(-1, 1) for _ in range(2400)]
    x = [rng.uniform(-1, 1) for _ in gate]
    df = pl.DataFrame({
        "gate": gate,
        "x": x,
        "response": [1 + 2*v if g < 0 else -2 - 3*v for g, v in zip(gate, x)],
    })
    report = onceover.profile(df, budget=budget)
    finding = next(f for f in report.findings
                   if f["kind"] == "piecewise_relationship"
                   and f["subject"]["target"] == "response")
    evidence = finding["evidence"]
    assert evidence["holdout_r2"] > 0.95
    assert evidence["relative_error_reduction"] > 0.8
    assert len(evidence["leaves"]) >= 2
    assert any(c["column"] == "gate" and abs(c["threshold"]) < 0.1
               for leaf in evidence["leaves"] for c in leaf["conditions"])
    for intercept, slope in [(1, 2), (-2, -3)]:
        assert any(abs(leaf["intercept"] - intercept) < 0.15
                   and abs(leaf["coefficients"]["x"] - slope) < 0.15
                   for leaf in evidence["leaves"])
    for leaf in evidence["leaves"]:
        assert leaf["n_holdout"] >= 10
        assert set(leaf["coefficients"]) == set(finding["subject"]["predictors"])
    assert "local linear regimes" in report.to_markdown()
    assert report.to_json() == onceover.profile(df, budget=budget).to_json()
    quick = onceover.profile(df, budget="quick")
    assert quick.coverage["regimes"]["status"] == "skipped"
    assert not any(f["kind"] == "piecewise_relationship" for f in quick.findings)

    controls = pl.DataFrame({"x": x, "linear": [4 + 1.5*v for v in x],
                             "noise": [rng.random() for _ in x]})
    assert not any(f["kind"] == "piecewise_relationship"
                   for f in onceover.profile(controls, budget=budget).findings)


@pytest.mark.parametrize("budget", ["standard", "deep"])
def test_date_residual_shift_reaches_python_report(budget):
    rng = random.Random(1234)
    n = 2000
    x = [rng.random() for _ in range(n)]
    start = dt.date(2024, 1, 1)
    df = pl.DataFrame({
        "x": x,
        "response": [3*v + (5 if i >= n//2 else 0) for i, v in enumerate(x)],
        "observed_at": [start + dt.timedelta(days=i) for i in range(n)],
    })
    report = onceover.profile(df, budget=budget)
    finding = next(f for f in report.findings if f["kind"] == "residual_shift"
                   and f["subject"]["target"] == "response")
    evidence = finding["evidence"]
    expected_break = start + dt.timedelta(days=n//2)
    assert abs((dt.date.fromisoformat(evidence["breakpoint"]) - expected_break).days) < 10
    assert evidence["holdout_error_reduction"] > 0.9
    assert evidence["holdout_before"] >= 10 and evidence["holdout_after"] >= 10
    assert report.coverage["regimes"]["status"] == "ran_sampled"


@pytest.mark.parametrize("budget", ["standard", "deep"])
def test_nominal_association_uses_complete_pair_holdout(budget):
    rng = random.Random(4321)
    segment, shipping = [], []
    for i in range(3000):
        premium = rng.random() < 0.3
        expedite = rng.random() < (0.95 if premium else 0.1)
        segment.append(None if i % 29 == 0 else ("premium" if premium else "standard"))
        shipping.append(None if i % 31 == 0 else ("expedite" if expedite else "normal"))
    df = pl.DataFrame({"segment": segment, "shipping": shipping})
    report = onceover.profile(df, budget=budget)
    finding = next(f for f in report.findings if f["kind"] == "association_rule"
                   and f["subject"]["antecedent_feature"] == "segment"
                   and f["subject"]["antecedent_value"] == "premium"
                   and f["subject"]["consequent_value"] == "expedite")
    evidence = finding["evidence"]
    assert evidence["holdout_confidence"] > 0.9 and evidence["holdout_lift"] > 2
    assert evidence["train_pair_n"] < evidence["train_n"]
    assert evidence["holdout_pair_n"] < evidence["holdout_n"]
    assert report.coverage["association_rules"]["status"] == "ran_sampled"
    assert onceover.profile(df, budget="quick").coverage["association_rules"]["status"] == "skipped"


def test_wide_dependence_coverage_reports_the_full_pair_count():
    df = pl.DataFrame({f"x{i}": list(range(100)) for i in range(33)})
    coverage = onceover.profile(df).coverage["dependence_screen"]
    assert coverage["status"] == "capped"
    assert coverage["detail"].startswith("496 of 528 numeric pairs (capped)")
