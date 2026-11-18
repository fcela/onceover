"""Raw-value truth and honest bounded-search coverage for issue #365."""
import random

import polars as pl
import pytest

import onceover


def _metrics(finding):
    return finding["evidence"][0]["metrics"]


@pytest.mark.parametrize("offset,dtype", [(0, pl.Int64), (2**60, pl.Int64), (-(2**60), pl.Int64), (2**64 - 4, pl.UInt64)])
def test_integer_translation_preserves_views_and_exact_modal_values(offset, dtype):
    values = [offset + i % 4 for i in range(6000)]
    df = pl.DataFrame({"x": pl.Series(values, dtype=dtype), "y": pl.Series(values, dtype=dtype)})
    report = onceover.profile(df, detectors=["row_clusters"])
    assert any(set(f["subject"]["columns"]) == {"x", "y"} for f in report.select(category="associations"))
    modal = report.select(type="modal_share", column="x")
    assert len(modal) == 1
    clusters = _metrics(modal[0])["modal_clusters"]
    assert {c["modal_value"] for c in clusters} == set(values)
    assert all(c["modal_n"] == c["nonnull_n"] == c["cluster_n"] == 1500 for c in clusters)


def test_pooled_tail_is_a_set_of_levels_not_a_literal_value():
    df = pl.DataFrame({"x": [f"level_{i % 64:02}" for i in range(6400)]})
    report = onceover.profile(df, detectors=["row_clusters"])
    findings = report.select(type="modal_share")
    assert findings
    entry = _metrics(findings[0])
    assert entry["modal_value"]["pooled"] is True
    assert entry["modal_value"]["level_count"] == 32
    excluded = entry["modal_value"]["excluding"]
    assert len(excluded) == 32
    assert df.filter(~pl.col("x").is_in(excluded)).height == entry["modal_n"] == 3200
    assert 'hold x at "other"' not in findings[0]["claim"]
    assert "pooled set of 32 other levels" in findings[0]["claim"]


def test_modal_share_uses_nonnull_denominator_in_single_cluster():
    df = pl.DataFrame({"x": [1] * 100 + [None] * 900})
    finding, = onceover.profile(df, detectors=["row_clusters"]).select(type="modal_share")
    metrics = _metrics(finding)
    assert metrics["cluster_n"] == 1000
    assert metrics["nonnull_n"] == metrics["modal_n"] == 100
    assert metrics["modal_share"] == 1
    assert "100 of 100 non-null rows" in finding["claim"]
    assert "1000 total rows" in finding["claim"]
    assert "100% of the non-null rows" in finding["claim"]


def test_modal_share_uses_nonnull_denominator_in_multiple_clusters():
    df = pl.DataFrame({"x": [0] * 100 + [1] * 100 + [None] * 800})
    finding, = onceover.profile(df, detectors=["row_clusters"]).select(type="modal_share")
    clusters = _metrics(finding)["modal_clusters"]
    assert sum(c["cluster_n"] for c in clusters) == 1000
    assert sum(c["nonnull_n"] for c in clusters) == 200
    assert all(c["modal_n"] == c["nonnull_n"] == 100 for c in clusters)
    assert "100 of 100 non-null rows" in finding["claim"]
    assert "100% of the non-null rows" in finding["claim"]


def test_column_screen_spans_leading_nulls_and_prefers_signal_to_constants():
    rng = random.Random(1729)
    values = [None] * 1000 + [rng.gauss(0, 1) for _ in range(5000)]
    data = {f"constant_{i}": [float(i)] * 6000 for i in range(64)}
    data.update({"late_x": values, "late_y": values})
    report = onceover.profile(pl.DataFrame(data), detectors=["row_clusters"], max_findings=1000)
    assert any(set(f["subject"]["columns"]) == {"late_x", "late_y"} for f in report.select(category="associations"))
    coverage = report.coverage["row_clusters"]
    assert coverage["status"] == "capped"
    dropped = coverage["detail"].split("past the 64-column cap: ", 1)[1].split(";", 1)[0]
    assert "late_x" not in dropped and "late_y" not in dropped
    assert "256 rows spanning the search sample" in coverage["detail"]


@pytest.mark.parametrize("budget", ["standard", "deep"])
@pytest.mark.parametrize("cap", [50, 6000])
def test_search_and_scoring_both_follow_public_sample_override(budget, cap):
    df = pl.DataFrame({"x": [i % 4 for i in range(6000)], "y": [i % 4 for i in range(6000)]})
    report = onceover.profile(df, detectors=["row_clusters"], budget=budget, sample_rows=cap)
    assert report.findings
    for finding in report.findings:
        metrics = _metrics(finding)
        assert metrics["search_rows"] == metrics["n"] == cap
    assert f"search {cap} of {cap} scoring rows" in report.coverage["row_clusters"]["detail"]


@pytest.mark.parametrize("budget,rows", [("standard", 20000), ("deep", 50000)])
def test_default_search_uses_budget_sample_instead_of_fixed_2000(budget, rows):
    df = pl.DataFrame({"x": [i % 2 for i in range(60000)]})
    report = onceover.profile(df, detectors=["row_clusters"], budget=budget)
    assert _metrics(report.findings[0])["search_rows"] == rows
    assert report.coverage["row_clusters"]["status"] == "ran_sampled"


def test_merge_exhaustion_is_capped_even_when_all_rows_are_scored():
    rng = random.Random(1729)
    df = pl.DataFrame({f"x{k}": [rng.gauss(0, 1) for _ in range(600)] for k in range(30)})
    report = onceover.profile(df, detectors=["row_clusters"], max_findings=1000)
    coverage = report.coverage["row_clusters"]
    assert coverage["status"] == "capped"
    assert "pair screen 435 of 435 pairs" in coverage["detail"]
    assert "256 merge attempts" in coverage["detail"]
    assert "merge-attempt cap reached (256)" in coverage["detail"]
