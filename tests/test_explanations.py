"""Public Python API checks for M5 regimes, date shifts, and associations."""

import datetime as dt
import random

import polars as pl
import pytest

import onceover
from _findings import select, metrics


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
    finding = select(report,"dependencies",subtype="predictive",form="piecewise_linear",target="response")[0]
    evidence = metrics(finding,contains="leaves")
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
        assert set(leaf["coefficients"]) == set(finding["subject"]["inputs"])
    assert "local linear regimes" in report.to_markdown()
    assert report.to_json() == onceover.profile(df, budget=budget).to_json()
    quick = onceover.profile(df, budget="quick")
    assert quick.coverage["regimes"]["status"] == "skipped"
    assert not select(quick,"dependencies",form="piecewise_linear")

    controls = pl.DataFrame({"x": x, "linear": [4 + 1.5*v for v in x],
                             "noise": [rng.random() for _ in x]})
    assert not select(onceover.profile(controls,budget=budget),"dependencies",form="piecewise_linear")


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
    finding = select(report,"changes",target="response")[0]
    evidence = metrics(finding,contains="breakpoint")
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
    finding = next(f for f in select(report,"conditional_rules",target="shipping")
                   if f["specification"]["outcome"]["value"]=="expedite"
                   and any(p["column"]=="segment" and p["operator"]=="eq" and p["value"]=="premium"
                           for p in f["specification"]["conditions"]))
    evidence=metrics(finding,contains="conditional_rate")
    assert evidence["conditional_rate"]>0.9 and evidence["lift"]>2
    assert evidence["conditional_rate"]==pytest.approx(evidence["joint_count"]/evidence["antecedent_count"])
    raw=metrics(finding,contains="holdout_pair_n")
    assert raw["train_pair_n"]<raw["train_n"]
    assert raw["holdout_pair_n"]<raw["holdout_n"]
    assert not select(onceover.profile(df,budget="quick"),"conditional_rules")


def test_wide_dependence_coverage_reports_the_full_pair_count():
    df = pl.DataFrame({f"x{i}": list(range(100)) for i in range(33)})
    coverage = onceover.profile(df).coverage["dependence_screen"]
    assert coverage["status"] == "capped"
    assert coverage["detail"].startswith("496 of 528 numeric pairs (capped)")
