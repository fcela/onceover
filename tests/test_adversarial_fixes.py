"""Regression tests for fixes from the 2026-11-22 adversarial shootout (#466-#470, #473)."""

from __future__ import annotations

import math
import random
import re
import time

import polars as pl
import pytest

import onceover


# ---------------------------------------------------------------------------
# #466: Preserve integer precision in order verification, rows(), and evidence
# ---------------------------------------------------------------------------


def test_order_constraints_large_signed_int64_precision():
    base = 1 << 60
    a = [base + (i % 200) * 16 for i in range(2000)]
    b = [a[i] - (1 + i % 5) for i in range(2000)]
    for i in range(9, 19):
        b[i] = a[i] + 1
    df = pl.DataFrame(
        {"a": pl.Series("a", a, dtype=pl.Int64), "b": pl.Series("b", b, dtype=pl.Int64)}
    )

    report = onceover.profile(df, detectors=["order_constraints"])
    hits = report.select(type="order_constraint")
    assert len(hits) == 1
    f = hits[0]
    m = f["evidence"][0]["metrics"]

    assert m["violations"] == 10
    assert m["equal_rows"] == 0
    assert m["worst_lower_value"] == b[9]
    assert m["worst_upper_value"] == a[9]
    assert m["worst_lower_value"] != m["worst_upper_value"]
    assert str(b[9]) in f["claim"]
    assert str(a[9]) in f["claim"]

    ver = report.verify(df).filter(pl.col("id") == f["id"])
    assert set(ver["status"].to_list()) == {"ok"}
    assert ver.filter(pl.col("quantity") == "violations")["actual"].item() == "10"
    assert ver.filter(pl.col("quantity") == "equal rows")["actual"].item() == "0"

    extracted = report.rows(f["id"], df)
    assert extracted["__row"].to_list() == list(range(9, 19))


def test_order_constraints_large_unsigned_uint64_precision():
    base = 1 << 62
    a = [base + (i % 200) * 16 for i in range(2000)]
    b = [a[i] - (1 + i % 5) for i in range(2000)]
    for i in range(15, 25):
        b[i] = a[i] + 1
    df = pl.DataFrame(
        {"a": pl.Series("a", a, dtype=pl.UInt64), "b": pl.Series("b", b, dtype=pl.UInt64)}
    )

    report = onceover.profile(df, detectors=["order_constraints"])
    f = report.select(type="order_constraint")[0]
    ver = report.verify(df).filter(pl.col("id") == f["id"])
    assert set(ver["status"].to_list()) == {"ok"}
    assert report.rows(f["id"], df)["__row"].to_list() == list(range(15, 25))


def test_order_constraints_float_null_and_nonfinite_handling():
    a: list[float | None] = [(i % 100) * 10.0 + 100.0 for i in range(1000)]
    b: list[float | None] = [a[i] - (1.0 + i % 4) for i in range(1000)]
    for i in range(5, 10):
        b[i] = a[i] + 2.0
    b[20] = math.nan
    a[21] = math.inf
    a[22] = None
    df = pl.DataFrame({"a": a, "b": b})

    report = onceover.profile(df, detectors=["order_constraints"])
    f = report.select(type="order_constraint")[0]
    ver = report.verify(df).filter(pl.col("id") == f["id"])
    assert set(ver["status"].to_list()) == {"ok"}
    assert report.rows(f["id"], df)["__row"].to_list() == [5, 6, 7, 8, 9]


# ---------------------------------------------------------------------------
# #467: Keep sampled findings' row positions tied to the source
# ---------------------------------------------------------------------------


def test_sampled_lazyframe_and_parquet_rows_and_verify(tmp_path):
    rng = random.Random(1)
    x = [rng.gauss(0.0, 1.0) for _ in range(4000)]
    x[1205] = 1_000_000.0
    df = pl.DataFrame({"reading": x})

    # LazyFrame source
    report = onceover.profile(
        df.lazy(), source_row_cap=1000, sample_seed=5, detectors=["outliers"]
    )
    f = report.select(type="extreme_value")[0]
    m = f["evidence"][0]["metrics"]
    assert m["row_index_basis"] == "sample"
    assert m["extreme_rows"] == [301]

    # Original DataFrame resolves to true source row 1205
    rows_df = report.rows(f["id"], df)
    assert rows_df["__row"].to_list() == [1205]
    assert rows_df["reading"].to_list() == [1_000_000.0]

    # Original LazyFrame resolves to true source row 1205
    rows_lf = report.rows(f["id"], df.lazy())
    assert rows_lf["__row"].to_list() == [1205]
    assert rows_lf["reading"].to_list() == [1_000_000.0]

    # Incompatible frame height is rejected instead of silently indexing wrong rows
    with pytest.raises(ValueError, match="does not match"):
        report.rows(f["id"], df.head(2500))

    # verify() against original source uses deterministic stratified sample
    ver_ok = report.verify(df)
    assert set(ver_ok["status"].to_list()) == {"ok"}
    assert "stratified sample" in ver_ok["note"][0]

    # verify() against incompatible frame height reports not_checkable
    ver_bad = report.verify(df.head(2500))
    assert set(ver_bad["status"].to_list()) == {"not_checkable"}

    # Parquet file input behaves identically
    pq = tmp_path / "sampled.parquet"
    df.write_parquet(pq)
    pq_report = onceover.profile(
        pq, source_row_cap=1000, sample_seed=5, detectors=["outliers"]
    )
    pq_f = pq_report.select(type="extreme_value")[0]
    assert pq_report.rows(pq_f["id"], df)["__row"].to_list() == [1205]
    assert set(pq_report.verify(df)["status"].to_list()) == {"ok"}


def test_sampled_categorical_outlier_rows_resolve_to_source():
    n = 4000
    grp = ["A"] * (n // 2) + ["B"] * (n // 2)
    val = ["x"] * (n // 2) + ["y"] * (n // 2)
    # Plant a rare categorical pattern-breaker at a row included in the stratified sample.
    from onceover import _source

    sample_idx = _source._stratified_indices(n, 1000, 7)
    target_src_row = next(r for r in sample_idx if r < n // 2)
    val[target_src_row] = "RARE_BREAKER"

    df = pl.DataFrame({"grp": grp, "val": val})
    report = onceover.profile(
        df.lazy(), source_row_cap=1000, sample_seed=7, detectors=["outliers"]
    )
    hits = report.select(type="categorical_outlier")
    assert hits, "expected categorical_outlier finding on sampled source"
    f = hits[0]
    assert f["evidence"][0]["metrics"]["row_index_basis"] == "sample"
    extracted = report.rows(f["id"], df)
    assert target_src_row in extracted["__row"].to_list()
    assert "RARE_BREAKER" in extracted["val"].to_list()
    assert set(report.verify(df)["status"].to_list()) == {"ok"}


# ---------------------------------------------------------------------------
# #468: High-resolution panel dynamics stays fast when no ticks clear gate
# ---------------------------------------------------------------------------


def test_high_res_panel_dynamics_no_blocks_is_fast():
    rng = random.Random(42)
    n = 25_000
    df = pl.DataFrame({
        "entity": [f"e{rng.randrange(400)}" for _ in range(n)],
        "ts": pl.Series("ts", range(1_700_000_000, 1_700_000_000 + n), dtype=pl.Int64),
        "metric": [rng.gauss(0.0, 1.0) for _ in range(n)],
    }).with_columns(pl.from_epoch("ts", time_unit="s"))

    t0 = time.perf_counter()
    report = onceover.profile(df, detectors=["series"])
    elapsed = time.perf_counter() - t0
    assert elapsed < 5.0, f"high-res panel dynamics too slow: {elapsed:.2f}s"
    assert report.select(type="entry_block") == []
    assert report.select(type="exit_block") == []


# ---------------------------------------------------------------------------
# #469: Cooperative deadline reports requested vs elapsed time and bounds overshoot
# ---------------------------------------------------------------------------


def test_deadline_reports_requested_limit_and_elapsed_time():
    rng = random.Random(772)
    n = 4_000
    df = pl.DataFrame({
        f"v{k:02d}": [rng.gauss(0.0, 1.0) for _ in range(n)]
        for k in range(32)
    })
    report = onceover.profile(df, deadline_seconds=0.0)
    detail = report.coverage["deadline"]["detail"]
    assert "wall-clock deadline of 0.000s reached" in detail
    assert "elapsed " in detail


# ---------------------------------------------------------------------------
# #470: Scale-aware affine identity tolerance under translation and rescaling
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("origin", [0, 1 << 52, 1 << 60])
def test_affine_rejects_noisy_relation_under_large_offset(origin):
    a = [(i % 200) * 16 + origin for i in range(2000)]
    b = [a[i] - (1 + i % 5) for i in range(2000)]
    df = pl.DataFrame(
        {"a": pl.Series("a", a, dtype=pl.Int64), "b": pl.Series("b", b, dtype=pl.Int64)}
    )
    report = onceover.profile(df, detectors=["numeric_dependence", "identities"])
    assert report.select(type="affine") == []


@pytest.mark.parametrize("origin", [0, 1 << 52, 1 << 60])
def test_affine_accepts_and_verifies_exact_relation_under_large_offset(origin):
    a = [(i % 200) * 16 + origin for i in range(2000)]
    b = [a[i] - 3 for i in range(2000)]
    df = pl.DataFrame(
        {"a": pl.Series("a", a, dtype=pl.Int64), "b": pl.Series("b", b, dtype=pl.Int64)}
    )
    report = onceover.profile(df, detectors=["numeric_dependence", "identities"])
    hits = report.select(type="affine")
    assert len(hits) == 1
    ver = report.verify(df).filter(pl.col("id") == hits[0]["id"])
    assert set(ver["status"].to_list()) == {"ok"}
    checks = [c for c in report.to_checks() if c["kind"] == "identity"]
    assert checks
    assert df.select(eval(checks[0]["polars"], {"pl": pl})).item() is True


def test_affine_accepts_rescaled_exact_relation():
    x = [(i % 150) * 1e6 + 5e8 for i in range(1500)]
    y = [2.5 * v + 1e9 for v in x]
    df = pl.DataFrame({"x": x, "y": y})
    report = onceover.profile(df, detectors=["numeric_dependence", "identities"])
    hits = report.select(type="affine")
    assert len(hits) == 1
    assert (
        set(report.verify(df).filter(pl.col("id") == hits[0]["id"])["status"].to_list())
        == {"ok"}
    )


# ---------------------------------------------------------------------------
# #473: Distinguish declared entity effects from post-outcome leakage
# ---------------------------------------------------------------------------


def test_declared_entity_is_distinguished_from_post_outcome_leakage():
    rng = random.Random(10)
    entity_means = [rng.gauss(0.0, 20.0) for _ in range(80)]
    rows = []
    for eid, mean in enumerate(entity_means):
        label = f"e{eid:02d}"
        for _ in range(60):
            rows.append((label, rng.gauss(0.0, 1.0), mean + rng.gauss(0.0, 1.0)))
    rng.shuffle(rows)
    df = pl.DataFrame({
        "cohort": [r[0] for r in rows],
        "decoy": [r[1] for r in rows],
        "score": [r[2] for r in rows],
    })

    ent_report = onceover.profile(
        df, target="score", hints={"entity": "cohort"}, detectors=["leakage"]
    )
    ent_f = ent_report.select(type="leakage_signature")[0]
    assert "declared by the caller as an entity" in ent_f["claim"]
    assert "derived from the outcome or recorded after it" not in ent_f["claim"]
    assert "group-held-out" in ent_f["claim"]
    assert ent_f["evidence"][0]["metrics"]["declared_entity"] is True

    post_df = df.with_columns((pl.col("score") + 100.0).alias("after"))
    post_report = onceover.profile(
        post_df,
        target="score",
        hints={"entity": "cohort", "recorded_after": ["after"]},
        detectors=["leakage"],
    )
    post_f = post_report.select(type="leakage_signature")[0]
    assert "recorded after the outcome" in post_f["claim"]


# ---------------------------------------------------------------------------
# Composite key handle in brief / digest grain line
# ---------------------------------------------------------------------------


def test_composite_key_grain_line_carries_addressable_handle():
    df = pl.DataFrame({
        "userId": [k // 20 for k in range(2000)],
        "movieId": [k % 20 for k in range(2000)],
        "rating": [(k % 5) + 0.5 for k in range(2000)],
    })
    report = onceover.profile(df)
    for text, pattern in [(report.to_brief(), r"\[(f\d+)\]"), (report.to_digest(), r"\[(uniqueness-[0-9a-f]+)\]")]:
        grain_lines = [line for line in text.splitlines() if "jointly identify rows" in line]
        assert len(grain_lines) == 1
        match = re.search(pattern, grain_lines[0])
        assert match is not None, f"expected handle on grain line: {grain_lines[0]!r}"
        resolved = report.finding(match.group(1))
        assert resolved is not None
        assert resolved["category"] == "uniqueness"
