"""Order constraints (issue #132): `a <= b` between two
columns of compatible kind and scale, holding exactly or violated
on a small share of rows. The plants mirror the issue's protocol
(5,000 rows, several seeds, the pair in both column positions).
Controls: independent columns, shuffled pairs, identical columns,
and an ordinary functional dependency with no order must stay
quiet; a derived pair (one column a function of the other) fires
but ranks below an unrelated exact order and below any order
finding that reports violations.
"""

import datetime as dt
import random

import polars as pl

import onceover


def _hits(report, lower=None, upper=None) -> list:
    out = [
        f
        for f in report.findings
        if f.get("type") == "order_constraint"
    ]
    if lower is not None:
        out = [
            f
            for f in out
            if f["evidence"][0]["metrics"]["lower"] == lower
            and f["evidence"][0]["metrics"]["upper"] == upper
        ]
    return out


def _payload(finding) -> dict:
    return finding["evidence"][0]["metrics"]


def _plant(seed: int, swapped: bool):
    rng = random.Random(seed)
    n = 5000
    lo = [rng.uniform(0.0, 50.0) for _ in range(n)]
    hi = [x + rng.uniform(0.0, 50.0) for x in lo]
    lo_v, hi_v = list(lo), list(hi)
    for i in rng.sample(range(n), 25):  # 0.5% violations
        lo_v[i] = hi_v[i] + rng.uniform(0.1, 5.0)
    start = [
        dt.datetime(2024, 1, 1) + dt.timedelta(days=rng.randrange(300))
        for _ in range(n)
    ]
    end = [s + dt.timedelta(hours=rng.randrange(1, 72)) for s in start]
    end_v = list(end)
    bad_end = rng.sample(range(n), 25)  # 0.5% violations
    for i in bad_end:
        end_v[i] = start[i] - dt.timedelta(hours=rng.randrange(1, 48))
    cols = {
        "lo": lo,
        "hi": hi,
        "lo_v": lo_v,
        "hi_v": hi_v,
        "start": start,
        "end": end,
        "end_v": end_v,
    }
    if swapped:
        cols = {k: cols[k] for k in reversed(list(cols))}
    return pl.DataFrame(cols), bad_end


def test_issue_plants_fire_in_both_positions_all_seeds():
    for seed in (1, 2, 3):
        for swapped in (False, True):
            df, bad_end = _plant(seed, swapped)
            report = onceover.profile(df)
            exact = _hits(report, "lo", "hi")
            assert len(exact) == 1, (seed, swapped)
            m = _payload(exact[0])
            assert m["violations"] == 0
            assert m["n"] == 5000
            viol = _hits(report, "lo_v", "hi_v")
            assert len(viol) == 1, (seed, swapped)
            m = _payload(viol[0])
            assert m["violations"] == 25
            assert m["n"] == 5000
            # The violating rows are retrievable and really violate.
            lo_v, hi_v = df["lo_v"].to_list(), df["hi_v"].to_list()
            rows = m["example_rows"]
            assert rows and all(lo_v[r] > hi_v[r] for r in rows)
            assert m["first_row"] == rows[0]
            temp = _hits(report, "start", "end_v")
            assert len(temp) == 1, (seed, swapped)
            m = _payload(temp[0])
            assert m["violations"] == 25
            starts, ends = df["start"].to_list(), df["end_v"].to_list()
            rows = m["example_rows"]
            assert rows and all(starts[r] > ends[r] for r in rows)
            assert set(rows) <= set(bad_end)


def test_exact_temporal_order_fires():
    df, _ = _plant(7, False)
    report = onceover.profile(df)
    hits = _hits(report, "start", "end")
    assert len(hits) == 1
    assert _payload(hits[0])["violations"] == 0


def test_independent_and_shuffled_pairs_stay_quiet():
    rng = random.Random(5)
    n = 5000
    a = [rng.uniform(0.0, 100.0) for _ in range(n)]
    b = [rng.uniform(0.0, 100.0) for _ in range(n)]
    lo = [rng.uniform(0.0, 50.0) for _ in range(n)]
    hi = [x + rng.uniform(0.0, 50.0) for x in lo]
    shuffled = list(hi)
    rng.shuffle(shuffled)
    df = pl.DataFrame({"a": a, "b": b, "lo": lo, "hi_s": shuffled})
    report = onceover.profile(df)
    assert _hits(report) == []


def test_identical_columns_and_plain_fd_stay_quiet():
    rng = random.Random(6)
    n = 5000
    base = [rng.uniform(0.0, 7.0) for _ in range(n)]
    df = pl.DataFrame(
        {
            "base": base,
            "base_copy": list(base),
            # A function of base that crosses it constantly, so no
            # near-universal order holds either way: the FD pass's
            # statement, not this pass's.
            "mod7": [float(int(x * 10) % 7) for x in base],
        }
    )
    report = onceover.profile(df)
    assert _hits(report) == []


def test_derived_pair_ranks_below_unrelated_exact_and_violations():
    rng = random.Random(8)
    n = 5000
    # A discrete base (199 distinct values, each repeated) so the
    # functional dependency base -> derived is a genuine value
    # mapping the FD pass records, not a by-product of uniqueness.
    base = [float(rng.randrange(1, 200)) for _ in range(n)]
    lo = [rng.uniform(0.0, 50.0) for _ in range(n)]
    hi = [x + rng.uniform(0.0, 50.0) for x in lo]
    lo_v, hi_v = list(lo), list(hi)
    for i in rng.sample(range(n), 25):
        lo_v[i] = hi_v[i] + rng.uniform(0.1, 5.0)
    df = pl.DataFrame(
        {
            "base": base,
            "derived": [2.0 * x + 3.0 for x in base],
            "lo": lo,
            "hi": hi,
            "lo_v": lo_v,
            "hi_v": hi_v,
        }
    )
    report = onceover.profile(df)
    order = [
        (
            f["evidence"][0]["metrics"]["lower"],
            f["evidence"][0]["metrics"]["upper"],
        )
        for f in report.findings
        if f.get("type") == "order_constraint"
    ]
    assert ("base", "derived") in order
    derived_pos = order.index(("base", "derived"))
    assert order.index(("lo_v", "hi_v")) < derived_pos
    assert order.index(("lo", "hi")) < derived_pos
    derived = _hits(report, "base", "derived")[0]
    assert _payload(derived)["functionally_related"] is True
    assert _payload(_hits(report, "lo", "hi")[0])["functionally_related"] is False


def test_pair_budget_and_findings_cap_are_disclosed():
    # 40 shifted copies of one base: every pair is eligible and
    # exact, so the shared max_pairs budget (496 at standard)
    # binds and the pass cap (25 findings) binds.
    n = 1000
    base = [float((k * 37) % 500) for k in range(n)]
    cols = {f"c{k:02d}": [x + float(k) for x in base] for k in range(40)}
    report = onceover.profile(pl.DataFrame(cols), detectors=["order_constraints"])
    entry = report.coverage["order_constraints"]
    assert entry["status"] == "capped", entry
    assert "496 of 780 eligible column pairs" in entry["detail"], entry
    assert len(_hits(report)) == 25


def test_quick_budget_records_the_pass_as_skipped():
    df, _ = _plant(3, False)
    report = onceover.profile(df, budget="quick")
    entry = report.coverage["order_constraints"]
    assert entry["status"] == "skipped", entry
    assert _hits(report) == []


def test_missing_values_are_excluded_from_the_pair_count():
    rng = random.Random(9)
    n = 5000
    lo = [rng.uniform(0.0, 50.0) for _ in range(n)]
    hi = [x + rng.uniform(0.0, 50.0) for x in lo]
    for i in range(0, n, 10):  # 10% of rows lose the upper value
        hi[i] = None
    df = pl.DataFrame({"lo": lo, "hi": hi})
    report = onceover.profile(df)
    hits = _hits(report, "lo", "hi")
    assert len(hits) == 1
    m = _payload(hits[0])
    assert m["n"] == 4500
    assert m["null_rows"] == 500
    assert m["population_rows"] == 5000
    scope = hits[0]["evidence"][0]["scope"]
    assert scope["sampling"] == "full"
    assert scope["rows_evaluated"] == 4500


def test_deterministic_across_runs():
    df, _ = _plant(11, False)
    r1 = onceover.profile(df, detectors=["order_constraints"])
    r2 = onceover.profile(df, detectors=["order_constraints"])
    assert [f["id"] for f in r1.findings] == [f["id"] for f in r2.findings]
    assert [f["claim"] for f in r1.findings] == [f["claim"] for f in r2.findings]


def test_independent_columns_with_rare_crossings_stay_quiet():
    # A heavy-tailed column set against a non-negative one has a compatible
    # range and few crossings under any pairing of the rows.
    rng = random.Random(11)
    n = 5000
    df = pl.DataFrame(
        {
            "tail": [rng.gauss(0, 1) / max(abs(rng.gauss(0, 1)), 1e-3) for _ in range(n)],
            "count": [rng.randint(0, 1000) for _ in range(n)],
        }
    )
    report = onceover.profile(df, detectors=["order_constraints"])
    assert not _hits(report)


def test_order_evidence_states_the_shuffled_violations():
    rng = random.Random(12)
    lo = [rng.uniform(0, 100) for _ in range(5000)]
    df = pl.DataFrame({"lo": lo, "hi": [v + rng.uniform(0, 5) for v in lo]})
    report = onceover.profile(df, detectors=["order_constraints"])
    metrics = _hits(report, "lo", "hi")[0]["evidence"][0]["metrics"]
    assert metrics["violations"] == 0
    assert metrics["violations_if_shuffled"] > 1000
