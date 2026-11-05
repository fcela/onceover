"""Tested associations (issue #134): per-pair tests with
Benjamini-Hochberg correction, the strong/weak reporting tiers,
and the report's associations table."""
import math
import random

import polars as pl

import onceover


def _correlated_pair(n=5000, rho=0.2, seed=11):
    rng = random.Random(seed)
    x = [rng.gauss(0.0, 1.0) for _ in range(n)]
    e = [rng.gauss(0.0, 1.0) for _ in range(n)]
    y = [rho * xi + math.sqrt(1 - rho * rho) * ei for xi, ei in zip(x, e)]
    noise = [rng.gauss(0.0, 1.0) for _ in range(n)]
    return pl.DataFrame({"x": x, "y": y, "z": noise})


def _weak_tier_metrics(report):
    out = []
    for f in report.findings:
        for ev in f.get("evidence") or []:
            m = ev.get("metrics") or {}
            if m.get("tier") == "weak":
                out.append((f, m))
    return out


def test_rho_0_2_pair_is_stated_as_a_weak_finding():
    report = onceover.profile(_correlated_pair())
    weak = [
        (f, m)
        for f, m in _weak_tier_metrics(report)
        if set(f["subject"]["columns"]) == {"x", "y"}
    ]
    assert weak, "the rho = 0.2 pair must be stated (weak tier)"
    f, m = weak[0]
    assert f["type"] == "statistical"
    assert m["q_value"] <= 0.05
    assert "weak numeric association" in f["claim"]


def test_associations_table_carries_every_screened_pair():
    report = onceover.profile(_correlated_pair())
    frame = report.associations()
    assert frame.columns == [
        "a", "b", "kind", "n", "pearson", "spearman", "xi_ab", "xi_ba",
        "cramers_v", "eta", "p_value", "q_value", "tier",
    ]
    # Three numeric columns -> three screened pairs.
    assert frame.height == 3
    xy = frame.filter(
        ((pl.col("a") == "x") & (pl.col("b") == "y"))
        | ((pl.col("a") == "y") & (pl.col("b") == "x"))
    )
    assert xy.height == 1
    row = xy.row(0, named=True)
    assert row["kind"] == "numeric"
    assert row["tier"] == "weak"
    assert abs(row["pearson"]) < 0.5
    assert row["q_value"] <= 0.05
    # Both directions of the asymmetric statistic are carried.
    assert row["xi_ab"] is not None and row["xi_ba"] is not None
    # The independent pair earned no statement.
    xz = frame.filter(pl.col("b") == "z")
    assert xz.height == 2
    assert set(xz["tier"].to_list()) == {"none"}


def test_digest_collapses_weak_associations_into_one_line():
    report = onceover.profile(_correlated_pair())
    digest = report.to_digest(4000)
    weak_lines = [
        ln for ln in digest.splitlines() if "statistical/weak" in ln
    ]
    assert len(weak_lines) == 1, digest
    assert "(x1)" in weak_lines[0]
    assert "report.associations()" in weak_lines[0]
    # The weak pair's claim is not also rolled up as its own group.
    rolled = [
        ln for ln in digest.splitlines()
        if ln.startswith("[associations] statistical:")
    ]
    assert rolled == [], digest


def test_nominal_weak_and_mixed_pairs_are_tested():
    rng = random.Random(99)
    n = 5000
    a, b, v = [], [], []
    for _ in range(n):
        x = rng.random() < 0.5
        y = x if rng.random() < 0.65 else not x
        a.append("p" if x else "q")
        b.append("r" if y else "s")
        v.append((3.0 if x else 0.0) + rng.random())
    df = pl.DataFrame({"a": a, "b": b, "v": v})
    report = onceover.profile(df)
    frame = report.associations()
    kinds = {}
    for row in frame.iter_rows(named=True):
        kinds.setdefault(row["kind"], []).append(row)
    nominal = kinds["nominal"][0]
    assert 0.2 < nominal["cramers_v"] < 0.4
    assert nominal["tier"] == "weak"
    mixed = [r for r in kinds["mixed"] if r["a"] == "a" and r["b"] == "v"][0]
    assert mixed["eta"] > 0.5
    assert mixed["tier"] == "strong"
    forms = {
        (f.get("specification") or {}).get("form")
        for f in report.findings
        if f["type"] == "statistical"
    }
    assert "nominal_pair" in forms and "mixed_pair" in forms


def test_associations_empty_frame_has_the_schema():
    df = pl.DataFrame({"only": [1.0, 2.0, 3.0, 4.0]})
    report = onceover.profile(df)
    frame = report.associations()
    assert frame.height == 0
    assert "tier" in frame.columns and "q_value" in frame.columns


def test_budgeted_json_keeps_the_associations_table():
    import json

    report = onceover.profile(_correlated_pair())
    view = json.loads(report.to_json(max_chars=6000))
    assert "associations" in view
    assert any(r["tier"] == "weak" for r in view["associations"])
