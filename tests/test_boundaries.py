"""Boundary mass (issue #119): values piled at the edge of a
numeric column's observed support, and unusual interior mass
points in zero-inflated columns. The finding states the pile-up
as observed — count, share, local baseline, the shape of the
rest of the column — and names top-coding only as one possible
cause in the interpretation. Controls: legitimate bounded
scores reaching their bounds, count columns, constant columns,
and ordinary zero inflation must stay quiet.
"""

import random

import polars as pl

import onceover


def _hits(report, column=None) -> list:
    return [
        f
        for f in report.findings
        if f.get("type") == "boundary_mass"
        and (column is None or column in f["subject"]["columns"])
    ]


def _payload(finding) -> dict:
    return finding["evidence"][0]["metrics"]


def test_top_coded_income_fires_with_exact_count_and_baseline():
    # Adult-shaped: 0 on ~92% of rows, a thin spread of nonzero
    # amounts, exactly 244 values piled at the 99,999 maximum.
    rng = random.Random(20261104)
    vals = [0.0] * 4480
    amounts = [float(rng.randrange(1, 45000)) for _ in range(340)]
    vals.extend(amounts)
    vals.extend([99999.0] * 244)
    rng.shuffle(vals)
    report = onceover.profile(pl.DataFrame({"capital_gain": vals}))
    hits = _hits(report, "capital_gain")
    assert len(hits) == 1
    claim = hits[0]["claim"]
    assert "piles 244 values" in claim
    assert "at its maximum, 99999" in claim
    payload = _payload(hits[0])
    assert payload["count"] == 244
    assert payload["value"] == 99999.0
    assert payload["boundary"] == "maximum"
    assert payload["n"] == len(vals)
    assert payload["population_rows"] == len(vals)
    assert payload["zero_count"] == 4480
    # The implicated scope is retrievable: the example rows hold
    # the piled value.
    rows = payload["example_rows"]
    assert rows and all(vals[r] == 99999.0 for r in rows)
    assert vals[payload["first_row"]] == 99999.0
    assert vals[payload["last_row"]] == 99999.0
    # Scope: full scan, missingness stated.
    scope = hits[0]["evidence"][0]["scope"]
    assert scope["sampling"] == "full"
    assert scope["rows_evaluated"] == len(vals)
    # Language discipline: the claim states the observation;
    # top-coding appears only in the interpretation, as one
    # possible cause.
    assert "top-cod" not in claim
    assert "top-coding" in hits[0]["interpretation"]
    assert "genuine upper bound" in hits[0]["interpretation"]


def test_isolated_minimum_pile_fires_without_impossibility_claim():
    # Metro-shaped: a continuous measure whose minimum sits far
    # below the bulk, with a handful of rows piled on it.
    rng = random.Random(11)
    vals = [243.0 + 70.0 * rng.random() for _ in range(5000)]
    vals.extend([rng.choice([270.5, 280.25, 291.75]) for _ in range(200)])
    vals.extend([0.0] * 10)
    rng.shuffle(vals)
    report = onceover.profile(pl.DataFrame({"temp": vals}))
    hits = _hits(report, "temp")
    assert len(hits) == 1
    payload = _payload(hits[0])
    assert payload["boundary"] == "minimum"
    assert payload["value"] == 0.0
    assert payload["count"] == 10
    assert payload["bulk_min"] > 240.0
    # No physical-impossibility language anywhere: without a
    # declared domain constraint the pile is reported as boundary
    # mass at the observed minimum, with distribution evidence.
    assert "impossible" not in hits[0]["claim"]
    assert "impossible" not in hits[0]["interpretation"]


def test_interior_mass_points_fire_under_zero_inflation():
    # capital_loss-shaped: 0 on ~95% of rows; the nonzero amounts
    # are thin except for repeated exact amounts.
    rng = random.Random(5)
    vals = [0.0] * 4760
    vals.extend(float(1700 + rng.randrange(0, 2600)) for _ in range(90))
    vals.extend([1902.0] * 60)
    vals.extend([1977.0] * 50)
    rng.shuffle(vals)
    report = onceover.profile(pl.DataFrame({"capital_loss": vals}))
    hits = _hits(report, "capital_loss")
    by_value = {p["value"]: p for p in (_payload(f) for f in hits)}
    assert by_value[1902.0]["count"] == 60
    assert by_value[1902.0]["boundary"] == "interior"
    assert by_value[1977.0]["count"] == 50


def test_bounded_scores_with_genuine_mass_at_bounds_stay_quiet():
    # Graded exam scores 0-100, clipped at the bounds: mass at
    # 100 is the scale working as designed, and neighbouring
    # scores carry comparable counts, so the local baseline is
    # not dominated. Several (mean, sd) shapes, including an easy
    # exam (85, 8) whose mass crowds the top of the scale.
    for mu, sd in ((72, 14), (85, 8), (60, 10)):
        rng = random.Random(21)
        vals = [
            float(min(100, max(0, round(rng.gauss(mu, sd)))))
            for _ in range(6000)
        ]
        report = onceover.profile(pl.DataFrame({"exam_score": vals}))
        assert _hits(report) == [], (mu, sd)


def test_percentages_saturating_at_bounds_stay_quiet():
    # Task completion percentages with genuine atoms at 0 and
    # 100 (not started / finished) on a rounded grid.
    rng = random.Random(31)
    vals = [0.0] * 900 + [100.0] * 1100
    vals.extend(round(rng.uniform(1, 99), 2) for _ in range(4000))
    report = onceover.profile(pl.DataFrame({"pct_complete": vals}))
    assert _hits(report) == []


def test_declining_count_distribution_stays_quiet():
    # Bank-shaped (evaluation audit): a zero-inflated count
    # column whose nonzero counts decline monotonically — every
    # low value beats the median of its thinner upper neighbours,
    # so a one-sided comparison fires all along the decline. An
    # interior mass point must dominate the local mass on each
    # side separately; a decline has no such point.
    counts = [2772, 2106, 1142, 714, 452, 301, 205, 129, 92, 67, 65]
    vals = [0.0] * 8000
    for k, c in enumerate(counts, start=1):
        vals.extend([float(k)] * c)
    # Thin tail out to 41 distinct nonzero values.
    vals.extend(float(k) for k in range(12, 42) for _ in range(3))
    report = onceover.profile(pl.DataFrame({"contacts_before": vals}))
    assert _hits(report) == []


def test_count_columns_stay_quiet():
    rng = random.Random(41)
    small = [float(rng.randrange(0, 7)) for _ in range(4000)]
    report = onceover.profile(pl.DataFrame({"children": small}))
    assert _hits(report) == []
    wide = [float(max(0, round(rng.gauss(300, 40)))) for _ in range(4000)]
    report = onceover.profile(pl.DataFrame({"hourly_count": wide}))
    assert _hits(report) == []


def test_constant_column_stays_quiet():
    report = onceover.profile(pl.DataFrame({"flag_level": [7.0] * 2000}))
    assert _hits(report) == []


def test_ordinary_zero_inflation_stays_quiet():
    # 60% zeros with continuous positives approaching zero: no
    # pile at either boundary, no interior mass point.
    rng = random.Random(51)
    vals = [0.0] * 3000
    vals.extend(0.05 + 240.0 * rng.random() ** 2 for _ in range(2000))
    report = onceover.profile(pl.DataFrame({"rain_mm": vals}))
    assert _hits(report) == []


def test_rounded_zero_inflation_edge_pile_stays_quiet():
    # Regression (evaluation control, seed 5002): zero-inflated
    # rain rounded to 2dp piles 14 rows on the smallest positive
    # grid value, 0.05 — a rounding artifact at the edge of the
    # nonzero support, 4.7x its local baseline. Interior mass
    # points carry a higher count floor than boundary piles, so
    # this stays quiet; the measured genuine mass points in the
    # evaluation clear the floor several times over.
    rng = random.Random(5002)
    vals = [0.0] * 3000
    vals.extend(round(0.05 + 240.0 * rng.random() ** 2, 2) for _ in range(2000))
    report = onceover.profile(pl.DataFrame({"rain_mm": vals}))
    assert _hits(report) == []


def test_small_frames_and_floors_stay_quiet():
    # Below the evaluated-rows floor nothing fires, however
    # extreme the pile.
    vals = [1.0 + (i % 50) * 0.31 for i in range(60)] + [999.0] * 30
    report = onceover.profile(pl.DataFrame({"x": vals}))
    assert _hits(report) == []


def test_detector_is_registered_and_ran():
    registry = {d["id"]: d for d in onceover.available_detectors()}
    assert "boundaries" in registry
    assert registry["boundaries"]["categories"] == ["formats"]
    rng = random.Random(61)
    vals = [rng.gauss(50, 10) for _ in range(1500)]
    report = onceover.profile(pl.DataFrame({"x": vals}))
    assert report.coverage["boundaries"]["status"] == "ran_exact"
    assert "numeric columns scanned for boundary mass" in (
        report.coverage["boundaries"]["detail"]
    )


def test_planted_caps_are_recalled_across_positions_shares_and_seeds():
    # A continuous sensor hard-capped at a known value: the cap
    # collects the whole upper tail. Recall over cap positions
    # (tail shares) and seeds.
    cases = 0
    found = 0
    for tail_share in (0.10, 0.025, 0.005):
        for seed in range(5):
            rng = random.Random(1000 + seed)
            n = 20000
            raw = [rng.gauss(100.0, 15.0) for _ in range(n)]
            cap = sorted(raw)[int(n * (1 - tail_share))]
            vals = [min(v, cap) for v in raw]
            report = onceover.profile(pl.DataFrame({"sensor": vals}))
            hits = _hits(report, "sensor")
            cases += 1
            if hits and abs(_payload(hits[0])["value"] - cap) < 1e-9:
                assert _payload(hits[0])["count"] == sum(
                    1 for v in raw if v >= cap
                )
                found += 1
    assert found == cases, f"recall {found}/{cases}"
