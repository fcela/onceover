"""Rows that disagree with the table's structure (issue #132)."""
import polars as pl
import pytest

import onceover

np = pytest.importorskip("numpy", reason="planted frames need numpy")


def _kind(report, kind):
    return [
        f for f in report.findings
        if f.get("type") == "row_inconsistency"
        and f["evidence"][0]["metrics"]["kind"] == kind
    ]


def _near(report):
    return [f for f in report.findings if f.get("type") == "near_duplicate_rows"]


def _run(df):
    return onceover.profile(df, detectors=["row_consistency", "numeric_dependence"])


def _base(seed=0, n=4000):
    rng = np.random.default_rng(seed)
    return rng, pl.DataFrame({
        "a": rng.normal(size=n),
        "b": rng.lognormal(size=n),
        "c": rng.integers(0, 1000, size=n),
        "d": [f"L{v}" for v in rng.integers(0, 40, size=n)],
    })


def test_near_duplicates_counted_against_polars():
    rng, df = _base()
    extra = df.sample(40, seed=1).with_columns(pl.col("a") + 100.0)
    frame = pl.concat([df, extra])
    hits = _near(_run(frame))
    assert len(hits) == 1
    m = hits[0]["evidence"][0]["metrics"]
    assert m["varying_column"] == "a"
    assert m["near_duplicates"] == 40
    assert "every column except a" in hits[0]["claim"]


def test_exact_duplicates_are_not_near_duplicates():
    _, df = _base()
    frame = pl.concat([df, df.sample(40, seed=1)])
    assert not _near(_run(frame))


def test_coarse_tables_are_quiet():
    rng = np.random.default_rng(2)
    n = 4000
    frame = pl.DataFrame({k: rng.integers(0, 3, size=n) for k in "abcde"})
    assert not _near(_run(frame))


def test_residual_outliers_found_when_marginals_look_fine():
    rng, df = _base()
    n = df.height
    x = rng.normal(size=n)
    y = x + 0.05 * rng.normal(size=n)
    idx = rng.choice(np.where(np.abs(x) > 1)[0], 8, replace=False)
    y[idx] = -x[idx]
    report = _run(df.with_columns(pl.Series("x", x), pl.Series("y", y)))
    hits = _kind(report, "residual_outlier")
    assert len(hits) == 1
    m = hits[0]["evidence"][0]["metrics"]
    assert set(m["rows"]) == set(int(i) for i in idx)
    assert m["outliers"] == 8


def test_tight_line_without_outliers_is_quiet():
    rng, df = _base()
    x = rng.normal(size=df.height)
    report = _run(df.with_columns(pl.Series("x", x), pl.Series("y", x + 0.05 * rng.normal(size=df.height))))
    assert not _kind(report, "residual_outlier")


def test_fat_tailed_residuals_are_not_outliers():
    rng, df = _base()
    x = rng.normal(size=df.height) * 20
    y = x + rng.standard_t(2, size=df.height)
    report = _run(df.with_columns(pl.Series("x", x), pl.Series("y", y)))
    assert not _kind(report, "residual_outlier")


def test_swapped_values_counted():
    rng, df = _base()
    n = df.height
    lat, lon = rng.normal(40, 3, size=n), rng.normal(-90, 5, size=n)
    mask = rng.random(n) < 0.01
    swapped_lat, swapped_lon = np.where(mask, lon, lat), np.where(mask, lat, lon)
    report = _run(df.with_columns(pl.Series("lat", swapped_lat), pl.Series("lon", swapped_lon)))
    hits = _kind(report, "swapped_values")
    assert len(hits) == 1
    assert abs(hits[0]["evidence"][0]["metrics"]["swapped_rows"] - int(mask.sum())) <= 1
    assert "look swapped" in hits[0]["claim"]


def test_three_column_key_found_with_a_unique_float_present():
    rng = np.random.default_rng(3)
    n = 5000
    idx = rng.permutation(30 * 30 * 30)[:n]
    frame = pl.DataFrame({
        "measure": rng.normal(size=n),
        "k1": idx // 900, "k2": (idx // 30) % 30, "k3": idx % 30,
    })
    keys = [f for f in onceover.profile(frame).findings if f["category"] == "uniqueness"]
    assert any({"k1", "k2", "k3"} <= set(f["subject"]["columns"]) for f in keys), [f["claim"] for f in keys]


def test_random_columns_are_not_a_three_column_key():
    rng = np.random.default_rng(4)
    n = 300
    # Enough combinations that the tuples are unique by chance alone.
    frame = pl.DataFrame({k: rng.integers(0, 500, size=n) for k in "abc"})
    report = onceover.profile(frame, detectors=["keys"])
    assert not [f for f in report.findings if f["evidence"][0]["method"]["algorithm"] == "composite_uniqueness"]


def test_noise_is_quiet():
    bad = []
    for seed in range(5):
        rng = np.random.default_rng(seed + 50)
        n = 4000
        report = _run(pl.DataFrame({
            "a": rng.normal(size=n), "b": rng.lognormal(size=n),
            "c": rng.standard_t(2, size=n), "d": rng.exponential(size=n),
            "e": rng.integers(0, 1000, size=n), "f": [f"L{v}" for v in rng.integers(0, 12, size=n)],
        }))
        bad += [f["claim"] for f in report.findings
                if f.get("type") in ("near_duplicate_rows", "row_inconsistency")]
    assert not bad, bad


def test_verifier_recounts_these_statements():
    rng, df = _base()
    extra = df.sample(40, seed=1).with_columns(pl.col("a") + 100.0)
    frame = pl.concat([df, extra])
    report = onceover.profile(frame)
    result = report.verify(frame)
    assert result.filter(pl.col("status").is_in(["mismatch", "error"])).is_empty()
    hit = _near(report)[0]
    assert report.rows(hit["id"], frame).height == 80


def test_heavy_tails_landing_in_another_range_are_not_swaps():
    rng = np.random.default_rng(7)
    n = 6000
    frame = pl.DataFrame({
        "ratio": rng.lognormal(0, 3, size=n),
        "level": rng.normal(5000, 100, size=n),
        "other": rng.lognormal(8, 3, size=n),
    })
    assert not _kind(_run(frame), "swapped_values")


def test_single_row_off_the_line_reads_as_one_row():
    rng = np.random.default_rng(3)
    x = rng.uniform(0, 100, size=2000)
    y = 2 * x + rng.normal(0, 0.1, size=2000)
    y[77] += 90
    hits = _kind(_run(pl.DataFrame({"x": x, "y": y})), "residual_outlier")
    assert hits[0]["claim"].startswith("1 row sits far from the line")
