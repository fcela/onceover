"""Stated basis for order statements (issue #293).

An order between two columns is stated only with a stated
basis: the pair functionally related in the run's exact
functional dependencies, both columns timestamps of one unit,
one column a component of the other, or a predictive
dependency between the columns (Spearman rank correlation at
least 0.5 on the screening sample, recorded as
``basis_spearman``). A direction that passes a reporting bar
without a basis is counted in the pass's coverage entry, not
stated. The plants: the issue #132 pair (predictive), a
derived pair (functional), a start/end pair (temporal), the
hotel-bookings shape from issue #293 (an id code against a
count — no basis), and two unrelated zero-inflated counts
crossing systematically (no basis).
"""

import random

import polars as pl

import onceover


def _hits(report, lower=None, upper=None):
    out = [
        f
        for f in report.findings
        if f.get("type") in ("order_constraint", "order_violation")
    ]
    if lower is not None:
        out = [
            f
            for f in out
            if f["evidence"][0]["metrics"]["lower"] == lower
            and f["evidence"][0]["metrics"]["upper"] == upper
        ]
    return out


def _payload(f):
    return f["evidence"][0]["metrics"]


def test_issue_132_plant_states_a_predictive_basis():
    rng = random.Random(1)
    n = 5000
    lo = [rng.uniform(0.0, 50.0) for _ in range(n)]
    hi = [x + rng.uniform(0.0, 50.0) for x in lo]
    report = onceover.profile(pl.DataFrame({"lo": lo, "hi": hi}))
    hits = _hits(report, "lo", "hi")
    assert len(hits) == 1
    m = _payload(hits[0])
    assert m["order_basis"] == "predictive"
    assert m["basis_spearman"] >= 0.5


def test_derived_pair_states_a_functional_basis():
    rng = random.Random(8)
    n = 5000
    base = [float(rng.randrange(1, 200)) for _ in range(n)]
    df = pl.DataFrame({"base": base, "derived": [2.0 * x + 3.0 for x in base]})
    report = onceover.profile(df)
    hits = _hits(report, "base", "derived")
    assert len(hits) == 1
    assert _payload(hits[0])["order_basis"] == "functional"


def test_temporal_pair_states_a_temporal_basis():
    rng = random.Random(7)
    n = 5000
    day, hour = 86_400_000_000, 3_600_000_000
    epoch = 1_704_067_200_000_000
    start = [epoch + day * rng.randrange(300) for _ in range(n)]
    end = [s + hour * rng.randrange(1, 72) for s in start]
    df = pl.DataFrame({"start": start, "end": end}).with_columns(
        pl.col("start", "end").cast(pl.Datetime("us"))
    )
    report = onceover.profile(df)
    hits = _hits(report, "start", "end")
    assert len(hits) == 1
    assert _payload(hits[0])["order_basis"] == "temporal"


def test_id_code_against_a_count_states_nothing():
    # The children <= agent shape from issue #293: an id code,
    # large on most rows, against a small count. The order
    # holds on every row and beats random pairing (small codes
    # appear only on childless rows, so the in-row pairing
    # crosses far less than chance), but the columns have no
    # relation: no statement, and the coverage entry counts
    # the unstated direction.
    rng = random.Random(13)
    n = 150_000
    children = []
    agent = []
    for _ in range(n):
        r = rng.random()
        if r < 0.88:
            child = 0
        elif r < 0.95:
            child = 1
        elif r < 0.98:
            child = 2
        elif r < 0.99:
            child = 3
        else:
            child = 9
        children.append(child)
        if rng.random() < 0.15:
            agent.append(None)
        elif child == 0 and rng.random() < 0.10:
            agent.append(float(rng.randrange(1, 60)))
        else:
            agent.append(float(rng.randrange(200, 541)))
    df = pl.DataFrame({"children": children, "agent": agent})
    report = onceover.profile(df, detectors=["order_constraints"])
    assert _hits(report) == []
    detail = report.coverage["order_constraints"]["detail"]
    assert "not stated: no stated basis" in detail, detail


def test_unrelated_counts_crossing_systematically_state_nothing():
    # Two unrelated zero-inflated counts: one direction is
    # violated on 6% of rows, the reverse on 30%, and both are
    # zero on most rows — the systematic shape with no basis.
    n = 5000
    lower = [0.0] * n
    upper = [0.0] * n
    for k in range(n):
        b = k % 100
        if 64 <= b < 94:
            upper[k] = 1.0 + (k * 7) % 5
        elif b >= 94:
            lower[k] = 1.0 + (k * 11) % 5
    df = pl.DataFrame({"lower": lower, "upper": upper})
    report = onceover.profile(df, detectors=["order_constraints"])
    assert _hits(report) == []


def test_linked_counts_crossing_both_ways_state_nothing():
    # accepted is generated perturbed by a few units: accepted
    # exceeds generated on 24% of rows and falls below it on 65%.
    # Neither order holds and no decomposition makes accepted a part
    # of generated, so nothing is stated (issue #374): from the
    # values alone this pair cannot be told apart from two
    # differently scaled temperatures.
    n = 2000
    rng = random.Random(3)
    gen = [50.0 + rng.randrange(200) for _ in range(n)]
    acc = list(gen)
    for k in range(n):
        if k < 480:
            acc[k] += 5.0
        elif k < 1780:
            acc[k] -= 3.0
    df = pl.DataFrame({"generated": gen, "accepted": acc})
    report = onceover.profile(df, detectors=["order_constraints"])
    assert _hits(report, "accepted", "generated") == []
