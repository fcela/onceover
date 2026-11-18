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


def _changes(report):
    return report.select(type="joint_distribution_change")


def test_planted_drifts_have_the_right_entities_and_boundary():
    report = _run("entity_profiles")
    [change] = _changes(report)
    metrics = change["evidence"][0]["metrics"]
    truth = json.loads((ROOT / "entity_profiles/truth.json").read_text())["drift"]
    shifts = metrics["entity_shifts"]
    assert metrics["population_shift"] is None
    assert len(shifts) >= 10
    assert {shift["entity"] for shift in shifts} <= set(truth["ids"])
    assert {shift["period"] for shift in shifts} == {truth["period"]}
    assert {shift["level"] for shift in shifts} == {"fresh"}
    assert "changed their category mix beyond the change among peers" in report.to_digest(4000)

    df = pl.read_csv(ROOT / "entity_profiles/data.csv")
    spec = onceover.infer_spec(df, hints={"entity": "cust_id"})
    shuffled = onceover.profile(
        df.sample(fraction=1.0, shuffle=True, seed=448),
        detectors=["entity_fingerprints"],
        spec=spec,
    )
    assert _changes(shuffled)[0]["evidence"][0]["metrics"] == metrics


def test_discontinued_level_is_a_shared_change_on_a_declared_ordered_axis():
    # The discontinued level varied in importance across entities.
    # The type mix of the new entities is deliberately different too.
    rows = []
    for entity in range(30):
        for period in ["early", "middle", "late"]:
            discontinued = 15 + 2 * (entity % 10)
            levels = ["old"] * discontinued + ["other"] * (40 - discontinued)
            if period == "late":
                levels = ["new" if level == "old" else level for level in levels]
            rows.extend((entity, period, level) for level in levels)
    rows.extend((entity, "late", "new") for entity in range(30, 40) for _ in range(40))
    df = pl.DataFrame(rows, schema=["entity", "stage", "level"], orient="row")
    r = onceover.profile(df, detectors=["entity_fingerprints"], hints={
        "entity": "entity",
        "ordered_axis": "stage",
        "level_order": {"stage": ["early", "middle", "late"]},
        "roles": {"level": "dimension"},
    })
    [change] = _changes(r)
    metrics = change["evidence"][0]["metrics"]
    assert metrics["entity_shifts"] == []
    assert metrics["n_entity_shifts"] == 0
    population = metrics["population_shift"]
    assert population["period"] == "late"
    assert population["matched_entities"] == 30
    assert sorted((population["before_share"], population["after_share"])) == pytest.approx([0.0, 0.6])
    assert "A population-wide change in level around late" in change["claim"]


def test_no_axis_keeps_static_fingerprints():
    df = pl.read_csv(ROOT / "entity_profiles/data.csv").drop("month")
    r = onceover.profile(df, detectors=["entity_fingerprints"], hints={"entity": "cust_id"})
    assert "entity_groups" in _types(r, "category")
    assert _changes(r) == []
    assert "drift skipped" in r.coverage["entity_fingerprints"]["detail"]
