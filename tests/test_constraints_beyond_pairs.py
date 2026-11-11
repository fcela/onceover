"""Constraint checks beyond pairs (issue #206).

Three statements the passes used to miss, each reproduced in the
shape it was reported in:

1. an exact-sum identity over four component columns, where the
   pair/triple enumeration cannot reach and the fitted greedy
   search takes a scaled copy of the total first — and the
   near-sum control, which must not be stated as an identity;
2. a paired-count order violated on 2,408 of 10,000 rows — past
   the certification bar in both directions, but systematic:
   the reverse order fails far more often. The control is two
   unrelated counts crossing both ways at similar rates;
3. one-way flags on an entity × time grain: a licence-style flag
   that switches on within entities and never off, with a lagged
   activity column; the guards are a flag on from the first period
   and a flag that switches both ways.
"""

import datetime
import random

import polars as pl

import onceover

MULTITERM_FORMS = {"sum", "difference", "linear_combination", "product", "ratio"}


def _identities_on(report, target):
    return [
        f
        for f in report.findings
        if f["subject"].get("target") == target
        and f["specification"].get("form") in MULTITERM_FORMS
    ]


def _sum_panel(perturb=False):
    rng = random.Random(206)
    persons, weeks = 200, 40
    parts = {name: [] for name in
             ("chat_tokens", "completion_tokens", "embedding_tokens", "image_tokens")}
    total, billed, shown, accepted = [], [], [], []
    for _ in range(persons * weeks):
        budget = max(5000.0, rng.gauss(20000, 3000))
        weights = [rng.expovariate(1.0) for _ in range(4)]
        wsum = sum(weights)
        running = 0.0
        for i, name in enumerate(parts):
            if i < 3:
                v = float(int(budget * weights[i] / wsum))
            else:
                v = float(int(budget) - int(running))
            parts[name].append(v)
            running += v
        t = float(int(budget))
        if perturb and rng.random() < 0.05:
            t += 50.0
        total.append(t)
        billed.append(float(round(t * 1.07)))
        s = float(int(budget * rng.uniform(0.015, 0.025)))
        shown.append(s)
        accepted.append(float(int(s * rng.uniform(0.3, 0.7))))
    start = datetime.date(2025, 1, 6)
    return pl.DataFrame({
        "person": [f"p{i:03d}" for i in range(persons) for _ in range(weeks)],
        "week": [start + datetime.timedelta(weeks=w)
                 for _ in range(persons) for w in range(weeks)],
        **parts,
        "total_tokens": total,
        "suggestions_shown": shown,
        "suggestions_accepted": accepted,
        "billed_tokens": billed,
    })


def test_four_component_exact_sum_is_found():
    report = onceover.profile(_sum_panel(), detectors=["identities"])
    hits = _identities_on(report, "total_tokens")
    assert len(hits) == 1, [f["claim"] for f in report.findings]
    hit = hits[0]
    assert hit["specification"]["form"] == "sum"
    metrics = hit["evidence"][0]["metrics"]
    assert set(metrics["terms"]) == {
        "chat_tokens", "completion_tokens", "embedding_tokens", "image_tokens",
    }
    assert metrics["violations"] == 0
    assert metrics["matching_rows"] == metrics["n"]


def test_near_sum_is_not_an_identity():
    report = onceover.profile(_sum_panel(perturb=True), detectors=["identities"])
    assert _identities_on(report, "total_tokens") == []


def _order_pair():
    rng = random.Random(206)
    n = 10_000
    gen = [rng.randint(20, 399) for _ in range(n)]
    acc = list(gen)
    perm = list(range(n))
    rng.shuffle(perm)
    for r in perm[:2408]:
        acc[r] = gen[r] + rng.randint(1, 29)
    for r in perm[2408:8908]:
        acc[r] = max(0, gen[r] - rng.randint(1, 14))
    return pl.DataFrame({"suggestions_generated": gen,
                         "suggestions_accepted": acc})


def test_systematic_order_violation_is_reported():
    report = onceover.profile(_order_pair(), detectors=["order_constraints"])
    hits = report.select(type="order_violation")
    assert len(hits) == 1, [f["claim"] for f in report.findings]
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["lower"] == "suggestions_accepted"
    assert metrics["upper"] == "suggestions_generated"
    assert metrics["violations"] == 2408
    assert metrics["reverse_violations"] >= 2 * 2408
    assert len(metrics["example_rows"]) > 0
    assert "2408 of 10000" in hits[0]["claim"]


def test_unrelated_counts_crossing_both_ways_stay_quiet():
    rng = random.Random(206)
    n = 10_000
    a = [0 if rng.random() < 0.6 else rng.randint(1, 59) for _ in range(n)]
    b = [0 if rng.random() < 0.6 else rng.randint(1, 59) for _ in range(n)]
    report = onceover.profile(
        pl.DataFrame({"queue_a": a, "queue_b": b}),
        detectors=["order_constraints"],
    )
    assert report.select(type="order_violation") == []
    assert report.select(type="order_constraint") == []


def _flag_panel():
    rng = random.Random(206)
    persons, weeks = 120, 52
    start = datetime.date(2025, 1, 6)
    person, week, licence, always_on, flaky, activity = [], [], [], [], [], []
    for p in range(persons):
        onset = rng.randint(10, 39) if p >= 60 else None
        lag = rng.randint(1, 3)
        for w in range(weeks):
            person.append(f"p{p:03d}")
            week.append(start + datetime.timedelta(weeks=w))
            on = onset is not None and w >= onset
            licence.append(on)
            always_on.append(True)
            flaky.append((p + w) % 5 < 2)
            activity.append(
                rng.randint(50, 499)
                if onset is not None and w >= onset + lag
                else 0
            )
    return pl.DataFrame({
        "person": person, "week": week, "licence_flag": licence,
        "always_on_flag": always_on, "flaky_flag": flaky,
        "activity_tokens": activity,
    })


def test_different_scales_crossing_asymmetrically_stay_quiet():
    # An age in years against a duration in seconds: the age
    # exceeds the duration on 12.5% of rows and the reverse order
    # fails on 87.5%, which passes the count, share, and asymmetry
    # terms — but the columns share no scale and do not track
    # their diagonal, so no systematic violation is stated.
    n = 2000
    age = [21 + (k * 29) % 70 for k in range(n)]
    duration = [12 if k % 8 == 0 else 240 + (k * 53) % 900 for k in range(n)]
    report = onceover.profile(
        pl.DataFrame({"age_years": age, "call_seconds": duration})
    )
    assert report.select(type="order_violation") == []


def test_one_way_flag_with_lag_is_reported():
    report = onceover.profile(_flag_panel())
    hits = report.select(type="one_way_flag")
    assert len(hits) == 1, [f["claim"] for f in report.findings]
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["flag"] == "licence_flag"
    assert metrics["entity_column"] == "person"
    assert metrics["entities_with_onset"] == 60
    assert metrics["on_to_off_transitions"] == 0
    assert metrics["activity_column"] == "activity_tokens"
    assert metrics["lag_periods_median"] in (1.0, 2.0, 3.0)
    assert metrics["lag_periods_p25"] <= metrics["lag_periods_median"]
    assert metrics["lag_periods_median"] <= metrics["lag_periods_p75"]


def test_one_way_flag_lag_reads_a_two_valued_activity_column():
    # Issue #232: the flag's related activity is a two-valued
    # token column (0 before onset, a constant 25 from two weeks
    # after onset). As a dimension it was invisible to the scan;
    # as a measure the lag is stated against it.
    persons, weeks = 60, 20
    start = datetime.date(2025, 1, 6)
    person, week, licence, tokens = [], [], [], []
    for p in range(persons):
        onset = 10 if p >= 30 else None
        for w in range(weeks):
            person.append(f"p{p:03d}")
            week.append(start + datetime.timedelta(weeks=w))
            licence.append(onset is not None and w >= onset)
            tokens.append(25 if onset is not None and w >= onset + 2 else 0)
    report = onceover.profile(pl.DataFrame({
        "person": person, "week": week, "licence_flag": licence,
        "tokens": tokens,
    }))
    hits = report.select(type="one_way_flag")
    assert len(hits) == 1, [f["claim"] for f in report.findings]
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["flag"] == "licence_flag"
    assert metrics["entities_with_onset"] == 30
    assert metrics["activity_column"] == "tokens"
    assert metrics["lag_periods_median"] == 2.0


def test_one_way_flag_coverage_without_a_grain():
    report = onceover.profile(pl.DataFrame({
        "a": [1.0, 2.0, 3.0] * 100,
        "b": [True, False, True] * 100,
    }))
    entry = report.coverage["one_way_flags"]
    assert entry["status"] == "skipped"
    assert "no entity" in entry["detail"]
