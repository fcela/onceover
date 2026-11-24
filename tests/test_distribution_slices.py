"""Numeric subgroup discovery and its relationship to the legacy slice pass."""

import random

import polars as pl

import onceover


DETECTOR = "distribution_slices"
FORM = "marginal_distribution_shift"


def _findings(df, **kwargs):
    report = onceover.profile(df, detectors=[DETECTOR], **kwargs)
    findings = [
        f for f in report.select(type="slice_shift", column="response")
        if f["specification"]["form"] == FORM
        and f["specification"]["definition"]["numeric_target"] == "response"
    ]
    return report, findings


def test_same_median_changes_in_a_tied_target():
    # Both groups have the same median and 4% above the pooled 95th percentile.
    # Empirical distributions nevertheless differ by 43 percentage points.
    rows = [
        (group, y)
        for group, scale in [("wide", 8.), ("narrow", 2.)]
        for y in ([-scale] * 43 + [0.] * 10 + [scale] * 43 + [20.] * 4) * 30
    ]
    random.Random(52).shuffle(rows)
    df = pl.DataFrame(rows, schema=["group", "response"], orient="row")
    report, findings = _findings(df)
    assert len(findings) == 1  # The complementary rules describe one comparison.
    finding = findings[0]
    evidence = finding["evidence"][0]
    m = evidence["metrics"]
    assert evidence["validation"]["method"] == "diagnostic_holdout"
    assert evidence["scope"]["sampling"] == "full"
    assert m["holdout"]["cdf_gap"] > 0.38
    assert m["holdout"]["slice_quantiles"][2] == m["holdout"]["rest_quantiles"][2] == 0
    assert m["p_adjusted"] < 0.05
    assert report.coverage[DETECTOR]["status"] == "ran_exact"
    assert report.rows(finding["id"], df).height == 3000
    assert set(report.verify(df)["status"]) == {"not_checkable"}
    legacy = onceover.profile(df, detectors=["slice_shifts"])
    assert all(f["specification"]["form"] == "level_share"
               for f in legacy.select(type="slice_shift"))


def test_two_numeric_conditions_find_an_interaction_with_no_single_axis_signal():
    rng = random.Random(4273)
    x, z, y = [], [], []
    for _ in range(8000):
        a, b = rng.random(), rng.random()
        x.append(a)
        z.append(b)
        y.append(rng.gauss(0, 0.14 if (a <= 0.5) != (b <= 0.5) else 1.0))
    for i in range(0, len(x), 31):
        x[i] = float("inf")
    for i in range(0, len(y), 29):
        y[i] = None
    df = pl.DataFrame({"x": x, "z": z, "response": y})
    report, findings = _findings(df, target="response")
    assert findings
    for f in findings:
        conds = f["specification"]["conditions"]
        assert {c["column"] for c in conds} == {"x", "z"}
        assert len(conds) == 2
        rows = report.rows(f["id"], df)
        assert rows["response"].is_finite().all()
        assert rows["x"].is_finite().all()
        assert rows.height == f["evidence"][0]["metrics"]["slice_rows"]
        for p in conds:
            xs = rows[p["column"]]
            if p["operator"] == "le":
                assert xs.max() <= p["value"]
            else:
                assert p["operator"] == "gt"
                assert xs.min() > p["value"]
    reversed_y = list(df["response"])
    rng.shuffle(reversed_y)
    _, null = _findings(df.with_columns(pl.Series("response", reversed_y)))
    assert not null


def test_target_cap_is_reported_and_designated_target_gets_a_slot():
    rng = random.Random(3927)
    n = 450
    groups = [rng.choice(["a", "b"]) for _ in range(n)]
    data = {f"noise_{i}": [rng.gauss(0, 1) for _ in range(n)] for i in range(25)}
    data["group"] = groups
    data["response"] = [rng.gauss(2 if g == "a" else 0, 1) for g in groups]
    report, findings = _findings(pl.DataFrame(data), target="response")
    assert findings
    coverage = report.coverage[DETECTOR]
    assert coverage["status"] == "capped"
    assert "2 numeric target columns capped" in coverage["detail"]
    assert "rule × target comparisons" in coverage["detail"]


def test_categorical_shift_combines_moderate_level_changes():
    rng = random.Random(135)
    levels = list("abcdefgh")
    rows = [
        (
            group,
            rng.choices(levels, weights=[18] * 4 + [7] * 4 if group else [12.5] * 8)[0],
        )
        for group in (False, True)
        for _ in range(4000)
    ]
    for i in range(0, len(rows), 137):
        rows[i] = (rows[i][0], None)
    rng.shuffle(rows)
    df = pl.DataFrame(rows, schema=["region", "response"], orient="row").with_columns(
        pl.col("response").cast(pl.Categorical)
    )
    report = onceover.profile(df, detectors=[DETECTOR], target="response")
    findings = [
        f for f in report.select(type="slice_shift")
        if f["specification"]["definition"].get("categorical_target") == "response"
    ]
    assert len(findings) == 1
    finding = findings[0]
    assert finding["specification"]["form"] == "categorical_distribution_shift"
    m = finding["evidence"][0]["metrics"]
    selected = set(m["holdout"]["selected_levels"])
    # The selected complement is equally informative if discovery keeps region=False.
    assert selected in (set("abcd"), set("efgh"))
    assert m["search"]["total_variation"] > 0.18
    assert m["holdout"]["share_gap"] > 0.18
    counts = m["holdout"]["levels"]
    assert sum(level["slice_count"] for level in counts) == m["holdout"]["slice_n"]
    assert sum(level["rest_count"] for level in counts) == m["holdout"]["rest_n"]
    assert m["p_adjusted"] <= 0.05
    rows = report.rows(finding["id"], df)
    assert rows["response"].null_count() == 0
    assert rows.height == m["slice_rows"]
    assert rows["response"].n_unique() == 8  # The region includes all observed target levels.

    shuffled = df["response"].cast(pl.String).to_list()
    rng.shuffle(shuffled)
    null = onceover.profile(
        df.with_columns(pl.Series("response", shuffled)), detectors=[DETECTOR], target="response"
    )
    assert null.select(type="slice_shift") == []


def test_boolean_target_and_high_cardinality_bound():
    rng = random.Random(64)
    groups = [rng.choice(["a", "b", "c"]) for _ in range(2400)]
    frame = pl.DataFrame({
        "group": groups,
        "flag": [rng.random() < (0.6 if g == "a" else 0.1) for g in groups],
        "label": [f"id-{i % 40}" for i in range(len(groups))],
    })
    report = onceover.profile(frame, detectors=[DETECTOR], target="flag")
    flags = [
        f["evidence"][0]["metrics"]["holdout"]["selected_levels"]
        for f in report.select(type="slice_shift")
        if f["specification"]["definition"].get("categorical_target") == "flag"
    ]
    assert [True] in flags
    cov = report.coverage[DETECTOR]
    assert cov["status"] == "capped"
    assert "1 categorical targets above 25 levels" in cov["detail"]


def test_numeric_and_categorical_proposals_share_confirmation_family():
    rng = random.Random(251)
    n = 3200
    group = [str(rng.randrange(4)) for _ in range(n)]
    frame = pl.DataFrame({
        "region": group,
        "score": [rng.gauss(5 if g == "1" else 0, 1) for g in group],
        "status": [rng.random() < (0.65 if g == "1" else 0.12) for g in group],
    })
    report = onceover.profile(frame, detectors=[DETECTOR])
    findings = report.select(type="slice_shift")
    forms = {f["specification"]["form"] for f in findings}
    assert forms == {FORM, "categorical_distribution_shift"}
    families = {f["evidence"][0]["metrics"]["confirmation_family"] for f in findings}
    assert len(families) == 1
    assert next(iter(families)) >= len(findings)


def _rare_findings(report):
    return [
        f for f in report.select(type="slice_shift")
        if f["specification"]["form"] == "rare_level_shift"
        and f["specification"]["definition"]["categorical_target"] == "response"
    ]


def test_rare_level_depletion_is_confirmed_without_a_finite_ratio():
    rng = random.Random(817)
    rows = (
        [("a", False)] * 4000 + [("b", False)] * 1940 + [("b", True)] * 60
        + [("c", False)] * 1940 + [("c", True)] * 60
    )
    rng.shuffle(rows)
    df = pl.DataFrame(rows, schema=["region", "response"], orient="row")
    report = onceover.profile(df, detectors=[DETECTOR], target="response")
    findings = [
        f for f in _rare_findings(report)
        if f["specification"]["conditions"][0]["value"] == "a"
    ]
    assert len(findings) == 1
    f = findings[0]
    assert f["specification"]["definition"]["target_level"] is True
    m = f["evidence"][0]["metrics"]
    assert m["holdout"]["slice_count"] == 0
    assert m["holdout"]["rest_count"] >= 8
    assert m["holdout"]["share_ratio"] is None
    assert 0 < m["holdout"]["share_gap"] < 0.12
    assert m["p_adjusted"] < 0.05
    assert report.rows(f["id"], df).height == 4000

    shuffled = df["response"].to_list()
    rng.shuffle(shuffled)
    control = df.with_columns(pl.Series("response", shuffled))
    assert not _rare_findings(onceover.profile(control, detectors=[DETECTOR]))


def test_rare_level_enrichment_needs_observations_in_both_halves():
    rng = random.Random(918)
    for count in (4, 280):
        rows = (
            [("a", "yes")] * count + [("a", "no")] * (4000 - count)
            + [("b", "yes")] * (48 if count > 4 else 0)
            + [("b", "no")] * (3952 if count > 4 else 4000)
        )
        rng.shuffle(rows)
        df = pl.DataFrame(rows, schema=["region", "response"], orient="row")
        findings = _rare_findings(onceover.profile(df, detectors=[DETECTOR]))
        if count == 4:
            assert not findings
        else:
            assert len(findings) == 1
            m = findings[0]["evidence"][0]["metrics"]
            inside_a = findings[0]["specification"]["conditions"][0]["value"] == "a"
            assert (m["holdout"]["slice_share"] > m["holdout"]["rest_share"]) == inside_a
            assert m["holdout"]["share_ratio"] > 2
            assert m["holdout"]["pooled_share"] < 0.05
