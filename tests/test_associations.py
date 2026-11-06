"""Tested associations (issue #134): per-pair tests with
Benjamini-Hochberg correction, the strong/weak reporting tiers,
the report's associations table, and the Simpson reversal
qualification attached to strong statements (item 5)."""
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


# --- Simpson reversal qualification (issue #134, item 5) ---


def _pearson(xs, ys):
    n = len(xs)
    mx = sum(xs) / n
    my = sum(ys) / n
    cov = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
    vx = sum((x - mx) ** 2 for x in xs)
    vy = sum((y - my) ** 2 for y in ys)
    return cov / math.sqrt(vx * vy)


def _strong_finding(report, columns):
    for f in report.findings:
        if f["type"] != "statistical":
            continue
        if set(f["subject"]["columns"]) != set(columns):
            continue
        for ev in f.get("evidence") or []:
            m = ev.get("metrics") or {}
            if m.get("tier") == "strong":
                return f, m
    return None, None


def _any_simpson(report):
    out = []
    for f in report.findings:
        for ev in f.get("evidence") or []:
            m = ev.get("metrics") or {}
            if "simpson" in m:
                out.append((f, m["simpson"]))
    return out


def _simpson_numeric(seed=7, slope=-1.5):
    rng = random.Random(seed)
    x, y, g = [], [], []
    for grp, (lo, base) in enumerate([(0.0, 0.0), (3.0, 8.0)]):
        for _ in range(2000):
            xi = lo + 2.0 * rng.random()
            x.append(xi)
            y.append(base + slope * xi + rng.gauss(0.0, 0.3))
            g.append("a" if grp == 0 else "b")
    return pl.DataFrame({"x": x, "y": y, "g": g})


def test_simpson_qualification_on_numeric_pair():
    df = _simpson_numeric()
    report = onceover.profile(df)
    f, m = _strong_finding(report, {"x", "y"})
    assert f is not None, "the planted pair must be a strong statement"
    assert "direction reverses within every level of g" in f["claim"]
    s = m["simpson"]
    assert s["stratifier"] == "g"
    assert s["statistic"] in ("pearson", "spearman")
    # Independent recomputation: pooled positive, both strata negative.
    xs = df["x"].to_list()
    ys = df["y"].to_list()
    gs = df["g"].to_list()
    pooled = _pearson(xs, ys)
    assert pooled > 0.3
    by_level = {lv: _pearson([x for x, g in zip(xs, gs) if g == lv],
                              [y for y, g in zip(ys, gs) if g == lv])
                for lv in ("a", "b")}
    assert all(v < 0 for v in by_level.values())
    strata = {row["level"]: row for row in s["strata"]}
    assert set(strata) == {"a", "b"}
    for lv, row in strata.items():
        assert abs(row["effect"] - by_level[lv]) < 0.05
        assert row["n"] == 2000
    assert s["pooled_effect"] * s["within_effect"] < 0
    assert abs(s["within_z"]) >= 3.84


def test_simpson_homogeneous_pair_gets_no_qualification():
    df = _simpson_numeric(seed=8, slope=1.5)
    report = onceover.profile(df)
    f, m = _strong_finding(report, {"x", "y"})
    assert f is not None, "the homogeneous pair is still a strong statement"
    assert "direction reverses" not in f["claim"]
    assert _any_simpson(report) == []


def _group_mean_pair(seed):
    """A pooled correlation produced by the group means alone:
    within each level of g, x and y are independent."""
    rng = random.Random(seed)
    x, y, g = [], [], []
    for _ in range(2000):
        grp = rng.random() < 0.5
        x.append(2.0 * grp + rng.gauss(0.0, 1.0))
        y.append(2.0 * grp + rng.gauss(0.0, 1.0))
        g.append("B" if grp else "A")
    return pl.DataFrame({"x": x, "y": y, "g": g})


def test_simpson_needs_a_measured_within_effect():
    # Within-level effects of x and y are zero by construction, so
    # their signs are coin flips: about a quarter of seeds give both
    # levels the sign opposite to the pooled effect. None of them
    # may be stated as a reversal.
    for seed in range(20):
        report = onceover.profile(_group_mean_pair(seed))
        f, _ = _strong_finding(report, {"x", "y"})
        assert f is not None, "the pooled pair is a strong statement"
        assert "direction reverses" not in f["claim"], (seed, f["claim"])
        assert _any_simpson(report) == []


def _simpson_mixed(seed=13):
    rng = random.Random(seed)
    t, y, s = [], [], []
    for base, p_treat, n in [(10.0, 0.95, 3800), (0.0, 0.05, 200)]:
        for _ in range(n):
            treated = rng.random() < p_treat
            t.append("trt" if treated else "ctrl")
            y.append(base + (-1.0 if treated else 0.0) + rng.gauss(0.0, 0.1))
            s.append("A" if base > 0 else "B")
    return pl.DataFrame({"t": t, "y": y, "s": s})


def test_simpson_qualification_on_mixed_pair():
    report = onceover.profile(_simpson_mixed())
    f, m = _strong_finding(report, {"t", "y"})
    assert f is not None, "the planted mixed pair must be a strong statement"
    assert "direction reverses within every level of s" in f["claim"]
    s = m["simpson"]
    assert s["statistic"] == "point_biserial"
    assert s["coded_level"] in ("ctrl", "trt")
    assert s["pooled_effect"] * s["within_effect"] < 0
    assert abs(s["within_z"]) >= 3.84
    assert all(row["effect"] * s["pooled_effect"] < 0 for row in s["strata"])
    assert {row["level"] for row in s["strata"]} == {"A", "B"}


def test_simpson_qualification_on_nominal_2x2_pair():
    cells = [
        ("A", "trt", "yes", 4410), ("A", "trt", "no", 490),
        ("A", "ctrl", "yes", 198), ("A", "ctrl", "no", 2),
        ("B", "trt", "yes", 1), ("B", "trt", "no", 99),
        ("B", "ctrl", "yes", 480), ("B", "ctrl", "no", 4320),
    ]
    s, t, o = [], [], []
    for sv, tv, ov, c in cells:
        s.extend([sv] * c)
        t.extend([tv] * c)
        o.extend([ov] * c)
    report = onceover.profile(pl.DataFrame({"s": s, "t": t, "o": o}))
    f, m = _strong_finding(report, {"t", "o"})
    assert f is not None, "the planted 2x2 pair must be a strong statement"
    assert "direction reverses within every level of s" in f["claim"]
    q = m["simpson"]
    assert q["statistic"] == "log_odds_ratio"
    assert q["coded_level_a"] in ("ctrl", "trt")
    assert q["coded_level_b"] in ("yes", "no")
    assert q["pooled_effect"] * q["within_effect"] < 0
    assert all(row["effect"] * q["pooled_effect"] < 0 for row in q["strata"])
    # No other strong statement in this frame reverses.
    assert len(_any_simpson(report)) == 1
