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
    # The annual mix of a planted drifter can sit between the two
    # profiles. None of the customers who did not drift is a misfit.
    assert len(stated) >= 15
    assert stated <= set(truth["drift"]["ids"])


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


def _run_account_mix(df, *, weights=None, ordered=False):
    roles = {name: "dimension" for name in ["plan", "channel", "copy"] if name in df.columns}
    roles.update({name: "measure" for name in ["spend", "period"] if name in df.columns})
    hints = {"entity": "account", "roles": roles}
    if weights:
        hints["weights"] = weights
    if ordered:
        hints["ordered_axis"] = "period"
    return onceover.profile(df, hints=hints, detectors=["entity_fingerprints"])


def test_weighted_groups_use_measure_shares_and_keep_the_pooled_baseline():
    rows = []
    for account in range(40):
        favored = "basic" if account < 20 else "plus"
        size = 3 if favored == "basic" else 1
        for label in ["basic", "plus"]:
            rows.extend((account, label, size * (9 if label == favored else 1)) for _ in range(60))
    df = pl.DataFrame(rows, schema=["account", "plan", "spend"], orient="row")
    assert _run_account_mix(df).findings == []

    spec = onceover.infer_spec(df, hints={
        "entity": "account", "weights": ["spend"], "roles": {"plan": "dimension", "spend": "measure"},
    })
    result = onceover.profile(df, spec=spec, detectors=["entity_fingerprints"])
    [group] = result.select(type="entity_groups")
    metrics = group["evidence"][0]["metrics"]
    assert metrics["weight"] == "spend"
    assert metrics["effective_rows"] < metrics["n_rows"] == df.height
    levels = {level["level"]: level for g in metrics["groups"] for level in g["levels"]}
    assert levels["basic"]["share"] == pytest.approx(0.90)
    assert levels["basic"]["overall"] == pytest.approx(0.70)
    assert levels["plus"]["overall"] == pytest.approx(0.30)
    assert sorted(g["entities"] for g in metrics["groups"]) == [20, 20]
    scaled = _run_account_mix(df.with_columns(pl.col("spend") * 1000), weights=["spend"])
    assert scaled.select(type="entity_groups")[0]["claim"] == group["claim"]


def test_large_weights_do_not_turn_one_effective_row_into_a_group():
    rows = []
    for account in range(40):
        favored = "basic" if account < 20 else "plus"
        for label in ["basic", "plus"]:
            for event in range(60):
                weight = 10**9 if label == favored and event == 0 else 1
                rows.append((account, label, weight))
    df = pl.DataFrame(rows, schema=["account", "plan", "spend"], orient="row")
    result = _run_account_mix(df, weights=["spend"])
    assert result.findings == []
    assert "40 of 40 entities have fewer than 10 effective rows" in result.coverage["entity_fingerprints"]["detail"]


def test_weighted_drift_detects_a_change_that_is_absent_in_row_shares():
    rows = []
    for account in range(30):
        for period in [1, 2]:
            favored = "plus" if account < 5 and period == 2 else "basic"
            for label in ["basic", "plus"]:
                rows.extend((account, period, label, 9 if label == favored else 1) for _ in range(100))
    df = pl.DataFrame(rows, schema=["account", "period", "plan", "spend"], orient="row")
    assert _changes(_run_account_mix(df, ordered=True)) == []
    [finding] = _changes(_run_account_mix(df, weights=["spend"], ordered=True))
    m = finding["evidence"][0]["metrics"]
    assert m["population_shift"] is None
    assert m["weight"] == "spend"
    assert {shift["entity"] for shift in m["entity_shifts"]} == set(map(str, range(5)))
    for shift in m["entity_shifts"]:
        assert shift["period"] == "2"
        assert sorted([shift["before_share"], shift["after_share"]]) == pytest.approx([0.1, 0.9])
        assert shift["before_effective_rows"] < shift["before_rows"] == 200


def _pair_frame(*, switching=False, weight=False):
    rows = []
    for account in range(40):
        for period in [1, 2]:
            aligned = (account < 20) != (switching and period == 2)
            for plan, channel in [("basic", "web"), ("basic", "store"), ("plus", "web"), ("plus", "store")]:
                favored = ((plan == "basic") == (channel == "web")) == aligned
                if weight:
                    rows.extend((account, period, plan, channel, 9 if favored else 1) for _ in range(60))
                elif favored:
                    rows.extend((account, period, plan, channel, 1) for _ in range(100))
    return pl.DataFrame(rows, schema=["account", "period", "plan", "channel", "spend"], orient="row")


def test_joint_groups_reveal_pairings_with_identical_marginals():
    df = _pair_frame()
    [group] = _run_account_mix(df).select(type="entity_groups")
    assert set(group["subject"]["columns"]) == {"account", "channel", "plan"}
    metrics = group["evidence"][0]["metrics"]
    assert metrics["dimensions"] == ["channel", "plan"]
    assert sorted(g["entities"] for g in metrics["groups"]) == [20, 20]
    for g in metrics["groups"]:
        assert [level["share"] for level in g["levels"]] == pytest.approx([0.5, 0.5])
        assert [level["overall"] for level in g["levels"]] == pytest.approx([0.25, 0.25])

    weighted = _pair_frame(weight=True)
    assert _run_account_mix(weighted).findings == []
    [group] = _run_account_mix(weighted, weights=["spend"]).select(type="entity_groups")
    assert set(group["subject"]["columns"]) == {"account", "channel", "plan", "spend"}


def test_joint_drift_can_cancel_in_full_period_mix():
    df = _pair_frame(switching=True)
    result = _run_account_mix(df, ordered=True)
    assert result.select(type="entity_groups") == []
    [finding] = _changes(result)
    m = finding["evidence"][0]["metrics"]
    assert m["dimensions"] == ["channel", "plan"]
    assert m["population_shift"] is None
    assert m["n_entity_shifts"] == 40
    assert {shift["period"] for shift in m["entity_shifts"]} == {"2"}


def test_redundant_dimension_pair_stays_silent():
    rows = [(account, "basic" if account < 20 else "plus") for account in range(40) for _ in range(100)]
    df = pl.DataFrame(rows, schema=["account", "plan"], orient="row").with_columns(pl.col("plan").alias("copy"))
    result = _run_account_mix(df)
    assert result.select(type="entity_groups")
    assert all(len(f["subject"]["columns"]) == 2 for f in result.findings)
    assert "copy × plan: no additional joint signal" in result.coverage["entity_fingerprints"]["detail"]


def test_misfit_matches_no_supported_group():
    rows = []
    for account in range(41):
        if account < 20:
            counts = {"basic": 90, "plus": 8, "legacy": 2}
        elif account < 40:
            counts = {"basic": 8, "plus": 90, "legacy": 2}
        else:
            counts = {"basic": 5, "plus": 5, "legacy": 90}
        for plan, count in counts.items():
            rows.extend([(account, plan)] * count)
    df = pl.DataFrame(rows, schema=["account", "plan"], orient="row")
    result = _run_account_mix(df)
    [group] = result.select(type="entity_groups")
    assert sorted(g["entities"] for g in group["evidence"][0]["metrics"]["groups"]) == [20, 20]
    [finding] = result.select(type="entity_misfits")
    metrics = finding["evidence"][0]["metrics"]
    assert metrics["groups_tested"] == 2
    [misfit] = metrics["misfits"]
    assert misfit["entity"] == "40"
    assert misfit["p_value"] < metrics["bar"]
    assert misfit["level"] == "legacy"
    assert misfit["share"] == pytest.approx(0.90)
    assert misfit["reference_share"] == pytest.approx(0.02)


def _hierarchy_report(df, hierarchies):
    return onceover.profile(
        df, detectors=["entity_fingerprints"],
        hints={"entity": "host", "hierarchies": hierarchies,
               "ordered_axis": "period", "roles": {"kit": "dimension"}},
    )


def test_hierarchy_pooling_uses_local_peers_and_falls_back_for_small_groups():
    rows = []
    for team, n, normal in [("A", 12, 20), ("B", 12, 120), ("C", 3, 20)]:
        for i in range(n):
            host = f"{team}{i:02}"
            featured = 195 if host in {"A00", "C00"} else normal
            rows.extend(
                [(host, team, "North", "featured")] * featured
                + [(host, team, "North", "simple")] * (200 - featured)
            )
    df = pl.DataFrame(rows, schema=["host", "team", "region", "kit"], orient="row")
    report = _hierarchy_report(df, [["host", "team", "region"]])
    [finding] = report.select(type="entity_misfits")
    ev = finding["evidence"][0]
    assert ev["method"]["algorithm"] == "hierarchy_peer_mix"
    hits = {m["entity"]: m["peer_group"] for m in ev["metrics"]["misfits"]}
    assert hits == {
        "A00": {"column": "team", "value": "A", "entities": 11},
        "C00": {"column": "region", "value": "North", "entities": 26},
    }
    assert "hierarchy peers: 27 of 27 entities" in report.coverage["entity_fingerprints"]["detail"]


def test_multiple_peer_hierarchies_can_supply_the_smallest_supported_pool():
    rows = []
    for i in range(32):
        featured = 195 if i == 0 else 20
        rows.extend(
            [(f"H{i:02}", f"T{i // 16}", "North", f"S{i % 4}", "Metro", "featured")] * featured
            + [(f"H{i:02}", f"T{i // 16}", "North", f"S{i % 4}", "Metro", "simple")] * (200 - featured)
        )
    df = pl.DataFrame(rows, schema=["host", "team", "region", "site", "area", "kit"], orient="row")
    report = _hierarchy_report(df, [["host", "team", "region"], ["host", "site", "area"]])
    [finding] = report.select(type="entity_misfits")
    [hit] = finding["evidence"][0]["metrics"]["misfits"]
    assert hit["entity"] == "H00"
    assert hit["peer_group"] == {"column": "site", "value": "S0", "entities": 7}


def test_hierarchy_drift_distinguishes_shared_local_change_from_individual_change():
    rows = []
    for i in range(48):
        team = "Gallery" if i < 24 else "Workshop"
        for period in (1, 2):
            featured = 108 if period == 2 and (i < 24 or i == 24) else 12
            rows.extend(
                [(f"H{i:02}", team, "North", period, "featured")] * featured
                + [(f"H{i:02}", team, "North", period, "simple")] * (120 - featured)
            )
    df = pl.DataFrame(rows, schema=["host", "team", "region", "period", "kit"], orient="row")
    report = _hierarchy_report(df, [["team", "region"]])
    [change] = _changes(report)
    metrics = change["evidence"][0]["metrics"]
    [shared] = metrics["population_shifts"]
    assert shared["peer_group"]["value"] == "Gallery"
    assert shared["matched_entities"] == 24
    [individual] = metrics["entity_shifts"]
    assert individual["entity"] == "H24"
    assert individual["peer_group"] == {"column": "team", "value": "Workshop", "entities": 23}


def test_broken_peer_path_is_refused_without_unscoped_misfits():
    rows = [(f"H{i:02}", "T", f"R{i % 2}", "special" if i == 0 else "usual")
            for i in range(24) for _ in range(80)]
    df = pl.DataFrame(rows, schema=["host", "team", "region", "kit"], orient="row")
    report = _hierarchy_report(df, [["team", "region"]])
    assert report.select(type="entity_misfits") == []
    assert "a mapping is broken" in report.coverage["entity_fingerprints"]["detail"]


def test_static_peers_can_run_on_a_small_but_supported_pool():
    rows = [(f"H{i}", "A", "North", "featured" if i == 0 else "simple")
            for i in range(6) for _ in range(100)]
    df = pl.DataFrame(rows, schema=["host", "team", "region", "kit"], orient="row")
    report = _hierarchy_report(df, [["team", "region"]])
    [finding] = report.select(type="entity_misfits")
    [hit] = finding["evidence"][0]["metrics"]["misfits"]
    assert hit["entity"] == "H0"
    assert hit["peer_group"] == {"column": "team", "value": "A", "entities": 5}


def test_weighted_hierarchy_peer_outlier_with_equal_order_counts():
    rows = [
        (f"H{i:02}", "Workshop", "North", kit, 9 if i == 0 and kit == "featured" else 1)
        for i in range(24) for kit in ("featured", "simple") for _ in range(80)
    ]
    df = pl.DataFrame(rows, schema=["host", "team", "region", "kit", "cost"], orient="row")
    hints = {"entity": "host", "hierarchies": [["team", "region"]],
             "roles": {"kit": "dimension", "cost": "measure"}}
    assert onceover.profile(df, hints=hints, detectors=["entity_fingerprints"]).select(type="entity_misfits") == []
    weighted = onceover.profile(df, hints={**hints, "weights": ["cost"]}, detectors=["entity_fingerprints"])
    [finding] = weighted.select(type="entity_misfits")
    metrics = finding["evidence"][0]["metrics"]
    assert metrics["weight"] == "cost"
    [hit] = metrics["misfits"]
    assert hit["entity"] == "H00"
    assert hit["peer_group"]["entities"] == 23
    assert hit["reference_share"] == pytest.approx(0.5)
