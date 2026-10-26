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



def test_profile_returns_report():
    report = onceover.profile(planted_df(3000))
    assert isinstance(report, onceover.ProfileReport)
    assert report.overview["headline"]
    assert report.findings and report.groups
    assert report.coverage["predictability_screen"]["status"] == "ran_sampled"


def test_planted_structure_recovered():
    from _findings import select
    report=onceover.profile(planted_df(6000))
    assert select(report,"uniqueness",column="order_id")
    assert select(report,"dependencies",subtype="functional",form="value_mapping",inputs=["region_id"],target="region_name")
    assert select(report,"gaps",column="event_date")
    assert select(report,"column_roles",column="region_id")
    assert select(report,"dependencies",subtype="predictive",target="y")
    assert select(report,"dependencies",subtype="functional",form="affine",column="double_quantity")


def test_json_roundtrip_and_determinism():
    df=planted_df(1500)
    a=onceover.profile(df)
    assert a.to_json()==onceover.profile(df).to_json()
    assert a.data["schema_version"]=="0.2"
    assert a.columns["region_id"]["role"]=="categorical"
    for f in a.findings:
        assert "kind" not in f and "confidence" not in f
        assert isinstance(f['evidence'],list) and f['evidence']
        assert f['subject']['columns'] == sorted(set(f['subject']['columns']))


def test_quick_budget_skips_models():
    from _findings import select
    report=onceover.profile(planted_df(1500),budget="quick")
    assert not select(report,"dependencies")
    assert not select(report,"conditional_rules")
    assert report.coverage['predictability_screen']['status']=='skipped'


def test_bad_budget_raises():
    with pytest.raises(ValueError):
        onceover.profile(planted_df(100),budget="yolo")


def test_chatterjee_xi_matches_scipy():
    from _findings import select,metrics
    scipy=pytest.importorskip('scipy.stats')
    rng=__import__('random').Random(99)
    xs=[i/2000 for i in range(2000)]
    ys=[x*x+0.001*rng.random() for x in xs]
    report=onceover.profile(pl.DataFrame({'x':xs,'y':ys}))
    finding=select(report,'associations')[0]
    m=metrics(finding,contains='xi_ab')
    assert m['variables']==['x','y']
    assert abs(m['xi_ab']-scipy.chatterjeexi(xs,ys).statistic)<1e-9


def test_perf_standard_10k():
    df=planted_df(10000)
    t=time.perf_counter()
    assert onceover.profile(df).findings
    assert time.perf_counter()-t<5


def test_markdown_and_save(tmp_path):
    import json
    report=onceover.profile(planted_df(800))
    assert '# onceover report' in report.to_markdown()
    p=tmp_path/'report.json'
    report.save(str(p))
    assert json.loads(p.read_text())['schema_version']=='0.2'
