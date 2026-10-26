"""Regression checks for large cardinalities and relationships in late columns."""

import math
import random

import polars as pl
import pytest

import onceover
from _findings import select, metrics


@pytest.mark.parametrize("budget", ["quick", "standard"])
def test_unique_and_dirty_keys_above_one_million_values(budget):
    n = 1_100_003
    ids = pl.int_range(0, n, eager=True)
    df = pl.DataFrame({"id": ids}).with_columns(
        pl.when(pl.col("id") == n - 1).then(0).otherwise(pl.col("id")).alias("dirty_id"))
    report = onceover.profile(df, budget=budget)
    assert report.columns["id"]["distinct"] == n
    assert report.columns["id"]["distinct_ratio"] == 1.0
    assert report.columns["dirty_id"]["distinct"] == n - 1
    assert report.columns["dirty_id"]["distinct_ratio"] == (n - 1) / n
    assert select(report,"uniqueness",column="id")
    dirty=select(report,"uniqueness",column="dirty_id")[0]
    assert metrics(dirty,contains="duplicates")["duplicates"]==1
    if budget=="standard":
        identity=select(report,"dependencies",subtype="functional",form="affine")[0]
        ev=metrics(identity,contains="matching_rows")
        assert ev["n"]==n and ev["matching_rows"]==n-1 and ev["violations"]==1
        assert "1 violation" in identity["claim"] and "100.00%" not in identity["claim"]


@pytest.mark.parametrize("reverse", [False, True])
def test_late_columns_reach_models_dependencies_and_pair_screen(reverse):
    rng = random.Random(24024)
    n = 1500
    data = {f"noise_{i:03}": [rng.random() for _ in range(n)] for i in range(72)}
    x = [rng.uniform(-1, 1) for _ in range(n)]
    codes = [rng.randrange(4) for _ in range(n)]
    data.update({"late_x": x, "late_affine": [3 * v + 2 for v in x],
                 "late_curve": [v * v for v in x], "late_code": codes,
                 "late_label": ["abcd"[v] for v in codes]})
    df = pl.DataFrame(data)
    if reverse:
        df = df.select(reversed(df.columns))
    report = onceover.profile(df, sample_rows=n, max_predictors=8, max_pairs=12)
    assert len(report.columns) == df.width
    assert "all usable columns" in report.coverage["encoding"]["detail"]
    assert select(report,"dependencies",subtype="predictive",target="late_curve")
    assert select(report,"dependencies",subtype="functional",form="value_mapping",inputs=["late_code"],target="late_label")
    assert select(report,"dependencies",subtype="functional",form="affine",column="late_affine")
    assert "all 2850 pairs screened" in report.coverage["dependence_screen"]["detail"]


def test_concentration_uses_the_entire_distribution():
    # A nonuniform tail beyond the 64 reported values used to be approximated.
    values = [v for v in range(100) for _ in range(v + 1)]
    counts = [v + 1 for v in range(100)]
    total = sum(counts)
    card = onceover.profile(pl.DataFrame({"value": values}), budget="quick").columns["value"]
    expected = sum((c / total) ** 2 for c in counts)
    entropy = -sum(c / total * math.log(c / total) for c in counts)
    assert card["concentration"]["hhi"] == pytest.approx(expected)
    assert card["concentration"]["entropy_nats"] == pytest.approx(entropy)


@pytest.mark.parametrize("option", ["sample_rows", "max_predictors", "max_pairs"])
def test_zero_analysis_limits_are_rejected(option):
    with pytest.raises(ValueError, match=option):
        onceover.profile(pl.DataFrame({"x": [1, 2, 3]}), **{option: 0})


def test_nullable_chunked_numeric_buffers_keep_normalization():
    df = pl.concat([
        pl.DataFrame({"x": [None, float("nan"), -0.0], "id": [None, 1, 2]}),
        pl.DataFrame({"x": [float("inf"), 0.0, 1.0], "id": [3, 4, 5]}),
    ], rechunk=False)
    card = onceover.profile(df, budget="quick").columns["x"]
    assert card["n"] == 6
    assert card["null_count"] == 3
    assert card["distinct"] == 2
    assert card["distinct_ratio"] == 2 / 3
    assert card["numeric_stats"]["zeros"] == 2


def test_composite_key_search_considers_late_columns():
    rng = random.Random(832)
    data = {f"noise_{i}": [rng.randrange(16) for _ in range(128)] for i in range(64)}
    data.update({"late_site": [i % 8 for i in range(128)],
                 "late_batch": [i // 8 for i in range(128)]})
    report = onceover.profile(pl.DataFrame(data), max_pairs=4)
    assert any(set(f["subject"]["columns"])=={"late_site","late_batch"} for f in select(report,"uniqueness"))


def test_nominal_associations_consider_columns_beyond_old_feature_caps():
    rng = random.Random(843)
    n = 2000
    data = {f"noise_{i}": [rng.choice(["a", "b"]) for _ in range(n)] for i in range(60)}
    premium = [rng.random() < 0.3 for _ in range(n)]
    data["late_segment"] = ["premium" if value else "standard" for value in premium]
    data["late_shipping"] = ["expedite" if rng.random() < (0.95 if value else 0.1)
                             else "normal" for value in premium]
    report = onceover.profile(pl.DataFrame(data), max_predictors=4)
    assert any(f["specification"]["outcome"]["value"]=="expedite"
               and any(p["column"]=="late_segment" and p["value"]=="premium" for p in f["specification"]["conditions"])
               for f in select(report,"conditional_rules",target="late_shipping"))
