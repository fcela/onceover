"""Ranking by informativeness (issue: fewer trivial findings).

The selection score is the mean of the detector's own score (preserved
on every finding as `detector_interestingness`) and the ledger's
evidence score, with a demotion for dependencies a unique key already
implies. These tests pin the three cases from the issue on real frames.
"""
import math
import random

import polars as pl

import onceover


def panel():
    """The wide panel from the long-format experiment (seed 7)."""
    random.seed(7)
    regions = ["North", "South", "East", "West"]
    rows = []
    for i in range(40):
        ent, region = f"E{i:02d}", regions[i % 4]
        base = math.exp(random.gauss(math.log(12000), 0.7))
        for k, year in enumerate(range(2000, 2015)):
            gdp = base * (1.03**k) * math.exp(random.gauss(0, 0.05))
            life = 55 + 0.00045 * gdp + {"North": 2, "South": -2, "East": 0, "West": 1}[region]
            if region == "South" and year >= 2008:
                life += 4.0
            life += random.gauss(0, 1.5)
            co2 = 0.0002 * gdp + random.gauss(0, 0.5)
            if ent == "E05" and year == 2010:
                co2 *= 8
            rows.append(
                {
                    "entity": ent,
                    "year": year,
                    "region": region,
                    "gdp_pc": round(gdp, 1),
                    "life_exp": round(life, 2),
                    "co2_pc": round(max(co2, 0.01), 2),
                }
            )
    return pl.DataFrame(rows)


def find(report, category, **kw):
    out = []
    for f in report.findings:
        if f.get("category") != category:
            continue
        subj = f.get("subject") or {}
        if all(subj.get(k) == v for k, v in kw.items()):
            out.append(f)
    return out


def test_lookup_fd_no_longer_scores_near_the_top():
    report = onceover.profile(panel(), budget="standard")
    fds = [
        f
        for f in find(report, "dependencies", target="region")
        if (f.get("subject") or {}).get("inputs") == ["entity"]
    ]
    assert len(fds) == 1
    fd = fds[0]
    # Was 0.9889 under the old ledger. The detector now scores an exact
    # FD by the breadth of its determinant: 40 distinct entities over
    # 600 rows -> 0.5 + 0.35 * (40/600) = 0.5233, and the selection
    # score blends that with the ledger's evidence score.
    assert abs(fd["detector_interestingness"] - 0.5233) < 0.01
    assert fd["interestingness"] < 0.80


def test_float_measurement_is_a_weak_key_candidate():
    report = onceover.profile(panel(), budget="standard")
    keys = [
        f
        for f in report.findings
        if f.get("category") == "uniqueness"
        and "gdp_pc" in (f.get("subject") or {}).get("columns", [])
    ]
    assert len(keys) == 1
    key = keys[0]
    # Plausibility for an unnamed float column is 0.48 (type affinity
    # 0.2), so the detector score is 0.3 + 0.65 * 0.48 = 0.612, not the
    # flat 0.95 every key candidate used to get.
    assert abs(key["detector_interestingness"] - 0.612) < 0.01
    assert "do not look like an identifier" in (key.get("interpretation") or "")


def test_identity_on_a_unique_key_is_demoted_and_tagged():
    # The FD lattice screens near-unique determinants during discovery,
    # so a plain FD from a key never reaches selection. Arithmetic
    # identities do not screen: an exact 2 x key column is a functional
    # statement whose input is the key, and the ledger demotes it.
    keyed = pl.DataFrame(
        {
            "order_id": list(range(400)),
            "double_id": [2 * i for i in range(400)],
            "amount": [float((i * 37) % 91) for i in range(400)],
        }
    )
    report = onceover.profile(keyed, budget="standard")
    hits = [
        f
        for f in find(report, "dependencies", target="double_id")
        if (f.get("subject") or {}).get("inputs") == ["order_id"]
    ]
    assert len(hits) == 1
    hit = hits[0]
    tagged = [
        e["metrics"]
        for e in hit["evidence"]
        if e["metrics"].get("triviality") == "implied_by_unique_key"
    ]
    assert tagged, "key-implied identity must carry the triviality tag"
    assert hit["interestingness"] < 0.65

    # Control: the same affine shape on a non-key column is not demoted.
    plain = pl.DataFrame(
        {
            "quantity": [i % 40 for i in range(400)],
            "double_quantity": [2 * (i % 40) for i in range(400)],
            "amount": [float((i * 37) % 91) for i in range(400)],
        }
    )
    report2 = onceover.profile(plain, budget="standard")
    hits2 = [
        f
        for f in find(report2, "dependencies", target="double_quantity")
        if (f.get("subject") or {}).get("inputs") == ["quantity"]
    ]
    assert len(hits2) >= 1  # the affine identity and the plain FD
    for h in hits2:
        assert all(
            e["metrics"].get("triviality") is None for e in h["evidence"]
        )


def test_format_artifact_keeps_its_low_detector_score():
    rows = [
        {"region": ("North" if i % 3 else "north"), "x": float(i % 50)}
        for i in range(300)
    ]
    report = onceover.profile(pl.DataFrame(rows), budget="standard")
    formats = [f for f in report.findings if f.get("category") == "formats"]
    assert formats
    for f in formats:
        # The format detector assigns 0.42; selection used to overwrite
        # it with 0.8625. The blend keeps it below substantive findings.
        assert abs(f["detector_interestingness"] - 0.42) < 0.01
        assert f["interestingness"] < 0.7
