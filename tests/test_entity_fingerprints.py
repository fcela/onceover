"""Entity fingerprints (issue #448) on the planted fixtures and the
null controls in benchmarks/corpus/data/synthetic."""
import json
from pathlib import Path

import polars as pl
import pytest

import onceover

ROOT = Path(__file__).resolve().parents[1] / "benchmarks/corpus/data/synthetic"


def _run(name):
    df = pl.read_csv(ROOT / name / "data.csv")
    return onceover.profile(df, detectors=["entity_fingerprints"], hints={"entity": "cust_id"})


def _types(report, dim):
    return {f["type"] for f in report.findings if dim in f["subject"]["columns"]}


def test_planted_groups_and_specialists():
    r = _run("entity_profiles")
    assert {"entity_groups", "entity_misfits"} <= _types(r, "category")
    assert {"entity_groups", "entity_specialists"} <= _types(r, "location")
    cat = next(f for f in r.findings if f["type"] == "entity_groups" and "category" in f["subject"]["columns"])
    assert "fall into 4 groups" in cat["claim"]
    truth = json.loads((ROOT / "entity_profiles/truth.json").read_text())
    misfits = next(f for f in r.findings if f["type"] == "entity_misfits")
    ev = misfits["evidence"]
    ev = ev[0] if isinstance(ev, list) else ev
    stated = {m["entity"] for m in ev["metrics"]["misfits"]}
    # The stated misfits are planted drifters, whose annual mix sits
    # between two profiles; at the Bonferroni bar at most one other
    # customer is stated (11 of 12 are drifters on this seed).
    assert len(stated & set(truth["drift"]["ids"])) >= 10
    assert len(stated - set(truth["drift"]["ids"])) <= 1


@pytest.mark.parametrize("name", ["entity_profiles_null_shuffled", "entity_profiles_null_one_profile"])
def test_null_controls_state_nothing(name):
    r = _run(name)
    assert r.findings == []
    assert "do not differ in mix beyond sampling noise" in r.coverage["entity_fingerprints"]["detail"]


def test_continuum_is_not_stated_as_groups():
    r = _run("entity_profiles_continuum")
    assert _types(r, "category") == {"entity_continuum"}


def test_without_an_entity_the_pass_is_skipped():
    df = pl.read_csv(ROOT / "entity_profiles/data.csv")
    r = onceover.profile(df, detectors=["entity_fingerprints"])
    cov = r.coverage["entity_fingerprints"]
    assert cov["status"].startswith("skipped") and "no entity" in cov["detail"]
