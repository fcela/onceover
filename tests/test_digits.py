"""Recording-process fingerprints (issue #59): heaping on round
units, terminal-digit preference, and Benford deviation with a
conservative eligibility rule."""

import random

import polars as pl

import onceover


def _of_type(report, type_: str) -> list:
    return [f for f in report.findings if f.get("type") == type_]


def _measured(seed: int, n: int = 4000, round_share: float = 0.0) -> list[int]:
    rng = random.Random(seed)
    vals = []
    for _ in range(n):
        v = round(rng.gauss(500, 80))
        if rng.random() < round_share:
            v = round(v / 10) * 10
        vals.append(v)
    return vals


def test_heaping_fires_on_rounded_column():
    report = onceover.profile(pl.DataFrame({"value": _measured(21, round_share=0.3)}))
    hits = _of_type(report, "heaping")
    assert len(hits) == 1
    payload = hits[0]["evidence"][0]["metrics"]
    assert payload["unit"] == 10
    assert payload["share_divisible"] > 0.25
    # One phenomenon, one finding: no separate digit finding.
    assert _of_type(report, "digit_preference") == []


def test_unrounded_column_is_quiet():
    report = onceover.profile(pl.DataFrame({"value": _measured(22)}))
    assert _of_type(report, "heaping") == []
    assert _of_type(report, "digit_preference") == []


def test_terminal_digit_preference_fires_without_heaping():
    rng = random.Random(23)
    vals = []
    for _ in range(4000):
        v = round(rng.gauss(500, 80))
        if rng.random() < 0.3:
            v = v - (v % 10) + 7
        vals.append(v)
    report = onceover.profile(pl.DataFrame({"value": vals}))
    hits = _of_type(report, "digit_preference")
    assert len(hits) == 1
    payload = hits[0]["evidence"][0]["metrics"]
    assert payload["top_digit"] == 7
    assert payload["top_digit_share"] > 0.25


def _benford_conforming(seed: int, n: int = 4000) -> list[float]:
    rng = random.Random(seed)
    vals = []
    for _ in range(n):
        v = 1.0
        for _ in range(4):
            v *= rng.uniform(1, 10)
        vals.append(v)
    return vals


def _uniform_first_digit(seed: int, n: int = 4000) -> list[float]:
    rng = random.Random(seed)
    return [rng.uniform(1, 10) * 10 ** rng.randrange(0, 4) for _ in range(n)]


def test_benford_conforming_column_is_quiet():
    report = onceover.profile(pl.DataFrame({"amount": _benford_conforming(24)}))
    assert _of_type(report, "benford_deviation") == []


def test_uniform_first_digit_column_fires():
    report = onceover.profile(pl.DataFrame({"amount": _uniform_first_digit(25)}))
    hits = _of_type(report, "benford_deviation")
    assert len(hits) == 1
    payload = hits[0]["evidence"][0]["metrics"]
    assert payload["mad"] > 0.012


def test_bounded_column_is_benford_ineligible():
    # Uniform first digits, but the span is two orders of
    # magnitude: eligibility, not conformity, keeps it quiet.
    rng = random.Random(26)
    vals = [rng.uniform(1, 100) for _ in range(4000)]
    report = onceover.profile(pl.DataFrame({"amount": vals}))
    assert _of_type(report, "benford_deviation") == []
    cov = report.coverage["digits"]["detail"]
    assert "0 Benford-eligible" in cov
