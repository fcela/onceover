"""End-to-end tests: Polars DataFrame in → ProfileReport out."""

import time

import polars as pl
import pytest

import onceover


def planted_df(n: int = 12_000) -> pl.DataFrame:
    """Table with known structure (mirror of the Rust integration fixture)."""
    import random

    rng = random.Random(2024)
    regions = ["emea", "amer", "apac"]
    categories = ["electronics", "clothing", "food", "tools"]

    ids = list(range(n))
    region_id = [rng.choice([0, 1, 2]) for _ in range(n)]
    region_name = [regions[r] for r in region_id]
    category = [rng.choice(categories) for _ in range(n)]
    quantity = [1 + rng.randrange(10) for _ in range(n)]
    unit_price = [round(5.0 + rng.random() * 95.0, 2) for _ in range(n)]
    total = [round(q * p, 2) for q, p in zip(quantity, unit_price)]
    # Planted pairwise affine identity.
    double_quantity = [2 * q for q in quantity]

    # Business-daily dates with two planted missing business days
    # (kept at n rows: the sequence jumps over 2 business days).
    import datetime as dt

    start = dt.date(2024, 1, 1)
    days = []
    d = start
    while len(days) < n + 4:
        if d.weekday() < 5:
            days.append(d)
        d += dt.timedelta(days=1)
    skip_at = n // 3
    dates = [days[i] if i < skip_at else days[i + 2] for i in range(n)]

    # y = strongly nonlinear function of x (for xi / predictability).
    xs = [rng.random() for _ in range(n)]
    ys = [x * x + 0.01 * rng.random() for x in xs]

    return pl.DataFrame(
        {
            "order_id": ids,
            "region_id": region_id,
            "region_name": region_name,
            "category": category,
            "quantity": quantity,
            "double_quantity": double_quantity,
            "unit_price": unit_price,
            "total": total,
            "event_date": dates,
            "x": xs,
            "y": ys,
        }
    ).with_columns(pl.col("event_date").cast(pl.Date))


def kinds(report):
    return [f["kind"] for f in report.findings]


def test_profile_returns_report():
    report = onceover.profile(planted_df(3_000))
    assert isinstance(report, onceover.ProfileReport)
    assert not report.overview["headline"] == ""
    assert len(report.findings) > 0
    assert isinstance(report.coverage, dict)
    assert report.coverage["predictability_screen"]["status"] == "ran_sampled"


def test_planted_structure_recovered():
    report = onceover.profile(planted_df(6_000))
    k = kinds(report)
    claims = " | ".join(f["claim"] for f in report.findings)
    assert "likely_primary_key" in k
    assert "order_id" in claims
    assert "functional_dependency" in k
    assert "region_id → region_name" in claims
    assert "date_gap" in k
    assert "likely_categorical" in k  # region_id: numeric-but-categorical
    # y is a planted function of x: the model-based screen must flag it.
    assert "predictable_from" in k
    assert any("y is predictable" in c for c in claims.split(" | "))
    assert "arithmetic_identity" in k  # double_quantity = 2 · quantity
    assert any("double_quantity" in c for c in claims.split(" | "))


def test_json_roundtrip_and_determinism():
    df = planted_df(1_500)
    a = onceover.profile(df)
    b = onceover.profile(df)
    assert a.to_json() == b.to_json()
    data = a.data
    assert data["schema_version"] == "0.1"
    assert isinstance(data["columns"], dict)
    card = data["columns"]["region_id"]
    assert card["role"] == "categorical"


def test_quick_budget_skips_models():
    report = onceover.profile(planted_df(1_500), budget="quick")
    k = kinds(report)
    assert "predictable_from" not in k
    assert "functional_dependency" not in k
    assert report.coverage["predictability_screen"]["status"] == "skipped"


def test_bad_budget_raises():
    with pytest.raises(ValueError):
        onceover.profile(planted_df(100), budget="yolo")


def test_chatterjee_xi_matches_scipy():
    scipy = pytest.importorskip("scipy.stats")
    rng = __import__("random").Random(99)
    xs = [i / 2000.0 for i in range(2000)]
    ys = [x * x + 0.001 * rng.random() for x in xs]
    df = pl.DataFrame({"x": xs, "y": ys})
    report = onceover.profile(df)
    dep = next(
        (f for f in report.findings if f["kind"] == "dependence"), None
    )
    assert dep is not None, "expected a dependence finding for y ~ x^2"
    ours = dep["evidence"]["xi_ab"]
    theirs = scipy.chatterjeexi(xs, ys).statistic
    assert abs(ours - theirs) < 1e-9, f"ours={ours} scipy={theirs}"


def test_perf_standard_10k():
    df = planted_df(10_000)
    t0 = time.perf_counter()
    report = onceover.profile(df)
    dt = time.perf_counter() - t0
    assert len(report.findings) > 0
    assert dt < 5.0, f"standard tier on 10k×10 took {dt:.2f}s"


def test_markdown_and_save(tmp_path):
    report = onceover.profile(planted_df(800))
    md = report.to_markdown()
    assert "# onceover report" in md
    p = tmp_path / "report.json"
    report.save(str(p))
    import json

    loaded = json.loads(p.read_text())
    assert loaded["schema_version"] == "0.1"
