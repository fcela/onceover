"""Functional-dependency recall and coverage status.

A determinant that looks near-unique on the 2,048-row screen sample
but cannot be near-unique on the full rows (its columns hold fewer
combinations than rows) is verified, not dismissed as a key."""
import time

import polars as pl
import pytest

import onceover

np = pytest.importorskip("numpy", reason="planted frames need numpy")


def _price_table(n, k, seed=1):
    rng = np.random.default_rng(seed)
    store = rng.integers(0, k, size=n)
    product = rng.integers(0, k, size=n)
    price = rng.normal(size=(k, k))[store, product]
    return pl.DataFrame({"store": store, "product": product, "price": price,
                         "qty": rng.integers(1, 9, size=n)})


def _fd(report, inputs, target):
    for f in report.findings + report.grouped_findings:
        if (f["type"] == "functional" and f["subject"].get("target") == target
                and set(f["subject"].get("inputs") or []) == set(inputs)):
            return f
    return None


@pytest.mark.parametrize("n,k", [(50_000, 200), (200_000, 400)])
def test_composite_determinant_beyond_the_screen_sample_is_found(n, k):
    # 200 x 200 = 40,000 cells over 50,000 rows: near-unique on 2,048
    # sampled rows, a real determinant on all of them.
    report = onceover.profile(_price_table(n, k))
    f = _fd(report, {"store", "product"}, "price")
    assert f is not None, [g["claim"] for g in report.findings]
    assert f["evidence"][0]["metrics"]["violations"] == 0


def test_fd_coverage_is_capped_only_when_a_cap_bound():
    small = onceover.profile(_price_table(5_000, 60))
    entry = small.coverage["functional_dependencies"]
    assert entry["status"] == "ran_exact", entry
    assert "caps bound" not in entry["detail"]
    rng = np.random.default_rng(3)
    wide = pl.DataFrame({f"c{i}": rng.integers(0, 6, size=2_000) for i in range(30)})
    entry = onceover.profile(wide).coverage["functional_dependencies"]
    assert entry["status"] == "capped", entry
    assert "determinant shortlist cap 16 bound" in entry["detail"], entry


def test_chance_term_cost_does_not_grow_with_the_square_of_the_rows():
    # This determinant (114,226 cells over 200,000 rows) is verified
    # only since the screen stopped dismissing it. The chance term
    # summed per (group, target value) cell would take about 1.3e10
    # hypergeometric terms here; summed over distinct margins it takes
    # milliseconds.
    df = _price_table(200_000, 400)
    t0 = time.time()
    report = onceover.profile(df, detectors=["functional_dependencies"], timing=True)
    assert time.time() - t0 < 30
    assert report.coverage["functional_dependencies"]["elapsed_ms"] < 10_000
