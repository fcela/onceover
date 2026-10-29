"""Subset-conditional surprise (issue #31): individual values that are
extreme relative to the bulk of their column, or relative to the bulk
of a subset defined by one categorical column."""
import random

import polars as pl

import onceover


def surprises(report):
    return [f for f in report.findings if f.get("type") == "extreme_value"]


def test_detector_registered():
    ids = {d["id"] for d in onceover.available_detectors()}
    assert "surprises" in ids


def test_global_extreme_isolated():
    vals = [20.0 + (i % 50) for i in range(300)]
    vals[7] = 455.0
    df = pl.DataFrame({"age": vals})
    hits = surprises(onceover.profile(df))
    assert len(hits) == 1
    hit = hits[0]
    assert hit["category"] == "intervals"
    assert "age" in hit["subject"]["columns"]
    metrics = hit["evidence"][0]["metrics"]
    assert metrics["extreme_values"] == [455.0]
    assert metrics["extreme_rows"] == [7]
    assert metrics["side"] == "high"


def test_subset_extreme_not_globally_extreme():
    # Cohort a spans 17..80 smoothly, so 73/75 are unremarkable
    # globally. Within cohort b every other value is <= 42.
    a = [17.0 + (i % 64) for i in range(200)]
    b = [17.0 + (i % 26) for i in range(51)] + [73.0, 75.0]
    df = pl.DataFrame(
        {
            "cohort": ["a"] * len(a) + ["b"] * len(b),
            "age": a + b,
        }
    )
    hits = surprises(onceover.profile(df))
    assert len(hits) == 1
    hit = hits[0]
    spec = hit["specification"]["definition"]
    assert spec["condition_column"] == "cohort"
    assert spec["condition_value"] == "b"
    metrics = hit["evidence"][0]["metrics"]
    assert sorted(metrics["extreme_values"]) == [73.0, 75.0]
    assert metrics["bulk_edge"] == 42.0


def test_low_side_extreme():
    vals = [30.0 + (i % 51) for i in range(300)]
    vals[3] = -500.0
    df = pl.DataFrame({"delta": vals})
    hits = surprises(onceover.profile(df))
    assert len(hits) == 1
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["extreme_values"] == [-500.0]
    assert metrics["side"] == "low"


def test_smooth_heavy_tail_is_not_a_surprise():
    import math

    vals = [math.log1p(i) * 10.0 for i in range(300)]
    df = pl.DataFrame({"x": vals})
    assert surprises(onceover.profile(df)) == []


def test_plain_column_has_no_surprises():
    vals = [float(20 + (i % 40)) for i in range(200)]
    df = pl.DataFrame({"x": vals, "g": ["p" if i % 2 else "q" for i in range(200)]})
    assert surprises(onceover.profile(df)) == []


def test_near_unique_measurement_column_is_scanned():
    # Regression: a near-unique continuous column is labeled an
    # identifier by cardinality in the column cards, and the
    # surprises invocation used to exclude every identifier-role
    # column — so the pass scanned 0 numeric columns on frames
    # whose measurements are near-unique. The exclusion now keeps
    # real-valued measurements eligible, as the regimes, breaks,
    # discontinuities, and drift invocations already did.
    rng = random.Random(34)
    vals = [rng.gauss(100, 5) for _ in range(2000)]
    vals[1000] = 400.0
    df = pl.DataFrame({"reading": vals})
    hits = surprises(onceover.profile(df))
    assert len(hits) == 1
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["extreme_values"] == [400.0]
