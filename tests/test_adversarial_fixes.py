"""Regression tests for fixes from the 2026-11-22 adversarial shootout (#466-#470, #473)."""

from __future__ import annotations

import re
import time

import numpy as np
import polars as pl
import pytest

import onceover


# ---------------------------------------------------------------------------
# #466: Preserve integer precision in order verification, rows(), and evidence
# ---------------------------------------------------------------------------


def test_order_constraints_large_signed_int64_precision():
    i = np.arange(2000, dtype=np.int64)
    a = np.int64(2**60) + (i % 200) * 16
    b = a - (1 + i % 5)
    b[9:19] = a[9:19] + 1
    df = pl.DataFrame({"a": a, "b": b})

    report = onceover.profile(df, detectors=["order_constraints"])
    hits = report.select(type="order_constraint")
    assert len(hits) == 1
    f = hits[0]
    m = f["evidence"][0]["metrics"]

    assert m["violations"] == 10
    assert m["equal_rows"] == 0
    assert m["worst_lower_value"] == int(b[9])
    assert m["worst_upper_value"] == int(a[9])
    assert m["worst_lower_value"] != m["worst_upper_value"]
    assert str(int(b[9])) in f["claim"]
    assert str(int(a[9])) in f["claim"]

    ver = report.verify(df).filter(pl.col("id") == f["id"])
    assert set(ver["status"].to_list()) == {"ok"}
    assert ver.filter(pl.col("quantity") == "violations")["actual"].item() == "10"
    assert ver.filter(pl.col("quantity") == "equal rows")["actual"].item() == "0"

    extracted = report.rows(f["id"], df)
    assert extracted["__row"].to_list() == list(range(9, 19))


def test_order_constraints_large_unsigned_uint64_precision():
    i = np.arange(2000, dtype=np.uint64)
    a = np.uint64(2**62) + (i % 200) * np.uint64(16)
    b = a - (np.uint64(1) + i % np.uint64(5))
    b[15:25] = a[15:25] + np.uint64(1)
    df = pl.DataFrame({"a": a, "b": b})

    report = onceover.profile(df, detectors=["order_constraints"])
    f = report.select(type="order_constraint")[0]
    ver = report.verify(df).filter(pl.col("id") == f["id"])
    assert set(ver["status"].to_list()) == {"ok"}
    assert report.rows(f["id"], df)["__row"].to_list() == list(range(15, 25))


def test_order_constraints_float_null_and_nonfinite_handling():
    i = np.arange(1000, dtype=np.float64)
    a = (i % 100) * 10.0 + 100.0
    b = a - (1.0 + i % 4)
    b[5:10] = a[5:10] + 2.0
    b[20] = np.nan
    a[21] = np.inf
    df = pl.DataFrame({"a": a, "b": b}).with_columns(
        pl.when(pl.int_range(pl.len()) == 22).then(None).otherwise(pl.col("a")).alias("a")
    )

    report = onceover.profile(df, detectors=["order_constraints"])
    f = report.select(type="order_constraint")[0]
    ver = report.verify(df).filter(pl.col("id") == f["id"])
    assert set(ver["status"].to_list()) == {"ok"}
    assert report.rows(f["id"], df)["__row"].to_list() == [5, 6, 7, 8, 9]


# ---------------------------------------------------------------------------
# #467: Keep sampled findings' row positions tied to the source
# ---------------------------------------------------------------------------


def test_sampled_lazyframe_and_parquet_rows_and_verify(tmp_path):
    x = np.random.default_rng(1).normal(size=4000)
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
    rng = np.random.default_rng(42)
    n = 25_000
    ts = pl.Series("ts", np.arange(1_700_000_000, 1_700_000_000 + n, dtype=np.int64))
    df = pl.DataFrame({
        "entity": [f"e{k}" for k in rng.integers(0, 400, size=n)],
        "ts": ts,
        "metric": rng.normal(size=n),
    }).with_columns(pl.from_epoch("ts", time_unit="s"))

    t0 = time.perf_counter()
    report = onceover.profile(df, detectors=["series"])
    elapsed = time.perf_counter() - t0
    assert elapsed < 2.0, f"high-res panel dynamics too slow: {elapsed:.2f}s"
    assert report.select(type="entry_block") == []
    assert report.select(type="exit_block") == []


# ---------------------------------------------------------------------------
# #469: Cooperative deadline reports requested vs elapsed time and bounds overshoot
# ---------------------------------------------------------------------------


def test_deadline_reports_requested_limit_and_elapsed_time():
    df = pl.DataFrame(
        np.random.default_rng(772).normal(size=(15_000, 48)),
        schema=[f"v{k:02d}" for k in range(48)],
        orient="row",
    )
    report = onceover.profile(df, deadline_seconds=0.0)
    detail = report.coverage["deadline"]["detail"]
    assert "wall-clock deadline of 0.000s reached" in detail
    assert "elapsed " in detail


# ---------------------------------------------------------------------------
# #470: Scale-aware affine identity tolerance under translation and rescaling
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("origin", [0, 2**52, 2**60])
def test_affine_rejects_noisy_relation_under_large_offset(origin):
    i = np.arange(2000, dtype=np.int64)
    a = (i % 200) * 16 + origin
    b = a - (1 + i % 5)
    df = pl.DataFrame({"a": a, "b": b})
    report = onceover.profile(df, detectors=["numeric_dependence", "identities"])
    assert report.select(type="affine") == []


@pytest.mark.parametrize("origin", [0, 2**52, 2**60])
def test_affine_accepts_and_verifies_exact_relation_under_large_offset(origin):
    i = np.arange(2000, dtype=np.int64)
    a = (i % 200) * 16 + origin
    b = a - 3
    df = pl.DataFrame({"a": a, "b": b})
    report = onceover.profile(df, detectors=["numeric_dependence", "identities"])
    hits = report.select(type="affine")
    assert len(hits) == 1
    ver = report.verify(df).filter(pl.col("id") == hits[0]["id"])
    assert set(ver["status"].to_list()) == {"ok"}
    checks = [c for c in report.to_checks() if c["kind"] == "identity"]
    assert checks
    assert df.select(eval(checks[0]["polars"], {"pl": pl})).item() is True


def test_affine_accepts_rescaled_exact_relation():
    i = np.arange(1500, dtype=np.float64)
    x = (i % 150) * 1e6 + 5e8
    y = 2.5 * x + 1e9
    df = pl.DataFrame({"x": x, "y": y})
    report = onceover.profile(df, detectors=["numeric_dependence", "identities"])
    hits = report.select(type="affine")
    assert len(hits) == 1
    assert set(report.verify(df).filter(pl.col("id") == hits[0]["id"])["status"].to_list()) == {"ok"}


# ---------------------------------------------------------------------------
# #473: Distinguish declared entity effects from post-outcome leakage
# ---------------------------------------------------------------------------


def test_declared_entity_is_distinguished_from_post_outcome_leakage():
    rng = np.random.default_rng(10)
    entities = np.repeat([f"e{k:02d}" for k in range(80)], 60)
    means = np.repeat(rng.normal(0, 20, size=80), 60)
    score = means + rng.normal(0, 1, size=4800)
    perm = rng.permutation(4800)
    df = pl.DataFrame({
        "cohort": entities[perm],
        "decoy": rng.normal(size=4800),
        "score": score[perm],
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
    brief = report.to_brief()
    grain_lines = [line for line in brief.splitlines() if "jointly identify rows" in line]
    assert len(grain_lines) == 1
    match = re.search(r"\[(f\d+)\]", grain_lines[0])
    assert match is not None, f"expected handle on grain line: {grain_lines[0]!r}"
    resolved = report.finding(match.group(1))
    assert resolved is not None
    assert resolved["category"] == "uniqueness"
