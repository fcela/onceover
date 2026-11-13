"""The opt-in slice_shifts detector (issue #272): searched
conjunction-conditional distribution shifts, confirmed on held-out
data.

The pass searches conjunctions of categorical levels and numeric
thresholds against every numeric target (tail share and median
shift) and every categorical target (level shares), states only
what a deterministic split-half search re-confirms on the held-out
half after Benjamini-Hochberg control over the confirmation
family, and deduplicates slices with identical row sets. The
fixtures plant each form at a known strength, plus the two
negative shapes the control stack exists for: a shuffled target
(no statement) and rate-exactly-0 slices (confirmed under the
zero-tolerant rule).
"""

import random

import polars as pl

import onceover


def _slice_findings(report):
    return [f for f in report.findings if f.get("type") == "slice_shift"]


def _metrics(finding):
    return finding["evidence"][0]["metrics"]


def _conditions(finding):
    return [
        (p["column"], p["operator"], p["value"])
        for p in finding["specification"]["conditions"]
    ]


def _tail_frame(n=4000, seed=2721):
    # Depth-2 planted tail: within segment B and channel store,
    # 40% of rows carry an extreme amount. Everywhere else the
    # amount is the same baseline distribution.
    rng = random.Random(seed)
    segment, channel, amount = [], [], []
    for _ in range(n):
        seg = rng.choice(["A", "B", "C", "D"])
        ch = rng.choice(["web", "store"])
        if seg == "B" and ch == "store" and rng.random() < 0.40:
            amount.append(rng.gauss(300.0, 20.0))
        else:
            amount.append(rng.gauss(50.0, 10.0))
        segment.append(seg)
        channel.append(ch)
    return pl.DataFrame(
        {"segment": segment, "channel": channel, "amount": amount}
    )


def test_planted_depth2_tail_slice_is_stated():
    report = onceover.profile(_tail_frame(), detectors=["slice_shifts"])
    assert report.coverage["slice_shifts"]["status"] in (
        "ran_exact",
        "ran_sampled",
    )
    hits = [
        f
        for f in _slice_findings(report)
        if _metrics(f)["form"] == "tail_shift"
        and set(_conditions(f))
        == {("segment", "eq", "B"), ("channel", "eq", "store")}
    ]
    assert hits, "the planted depth-2 slice was not stated"
    m = _metrics(hits[0])
    assert m["tail_share"] >= 0.30
    assert m["tail_share_confirm"] >= 0.10
    assert m["q_value"] <= 0.05
    assert m["candidates_evaluated"] > 50
    assert m["search_candidates"] >= 1


def test_planted_depth1_median_slice_is_stated():
    # A pure median shift: segment D's amounts sit in a tight band
    # around 80 while the baseline median is 50, and segment C
    # carries the frame's extremes, so the global 95th percentile
    # sits far above D's band — D's shift cannot present as a tail
    # share, only as a median shift.
    rng = random.Random(415)
    segment, amount = [], []
    for i in range(4000):
        seg = rng.choice(["A", "B", "C", "D"])
        if seg == "D":
            amount.append(72.0 + (i % 17))
        elif seg == "C" and rng.random() < 0.25:
            amount.append(rng.gauss(300.0, 20.0))
        else:
            amount.append(rng.gauss(50.0, 10.0))
        segment.append(seg)
    df = pl.DataFrame({"segment": segment, "amount": amount})
    report = onceover.profile(df, detectors=["slice_shifts"])
    hits = [
        f
        for f in _slice_findings(report)
        if _metrics(f)["form"] == "median_shift"
        and _conditions(f) == [("segment", "eq", "D")]
    ]
    assert hits, "the planted depth-1 median shift was not stated"
    assert _metrics(hits[0])["median_shift_iqr"] >= 0.75


def test_planted_level_share_slice_is_stated():
    rng = random.Random(96)
    region, subscribed = [], []
    for _ in range(4000):
        reg = rng.choice(["north", "south", "east", "west"])
        p = 0.45 if reg == "south" else 0.08
        subscribed.append("yes" if rng.random() < p else "no")
        region.append(reg)
    df = pl.DataFrame({"region": region, "subscribed": subscribed})
    report = onceover.profile(df, detectors=["slice_shifts"])
    hits = [
        f
        for f in _slice_findings(report)
        if _metrics(f)["form"] == "level_share"
        and _metrics(f)["target_level"] == "yes"
        and ("region", "eq", "south") in _conditions(f)
    ]
    assert hits, "the planted level-share slice was not stated"
    m = _metrics(hits[0])
    assert m["slice_rate"] >= 0.35
    assert m["rate_ratio"] >= 2.0


def test_shuffled_target_produces_no_statements():
    df = _tail_frame()
    amounts = df["amount"].to_list()
    random.Random(7).shuffle(amounts)
    shuffled = df.with_columns(pl.Series("amount", amounts))
    report = onceover.profile(shuffled, detectors=["slice_shifts"])
    assert _slice_findings(report) == [], (
        "a shuffled target must produce no slice statements: "
        f"{[f['claim'] for f in _slice_findings(report)]}"
    )


def test_identical_row_sets_produce_one_statement():
    # seg_a and seg_b are the same column twice, so seg_a = 'x',
    # seg_b = 'x' and their conjunction all name the same rows.
    # The planted tail lives on those rows; exactly one statement
    # may be made about the amount on that row set, at depth 1.
    rng = random.Random(1234)
    seg, amount = [], []
    for _ in range(4000):
        s = rng.choice(["x", "y", "z", "w"])
        if s == "x" and rng.random() < 0.40:
            amount.append(rng.gauss(300.0, 20.0))
        else:
            amount.append(rng.gauss(50.0, 10.0))
        seg.append(s)
    df = pl.DataFrame({"seg_a": seg, "seg_b": list(seg), "amount": amount})
    report = onceover.profile(df, detectors=["slice_shifts"])
    numeric_hits = [
        f
        for f in _slice_findings(report)
        if _metrics(f)["form"] in ("tail_shift", "median_shift")
    ]
    assert numeric_hits, "the planted slice was not stated"
    on_x = [
        f
        for f in numeric_hits
        if any(v == "x" for _, _, v in _conditions(f))
    ]
    assert len(on_x) == 1, (
        "identical row sets must produce one statement, got "
        f"{[f['claim'] for f in on_x]}"
    )
    assert len(_conditions(on_x[0])) == 1


def test_rate_zero_slice_confirms_under_the_zero_tolerant_rule():
    # No 'free' row ever converts, in either half by construction.
    # The strict ratio rule can never confirm a rate of exactly 0
    # (any held-out positive breaks it); the zero-tolerant rule
    # asks only that the held-out rate stay near zero.
    rng = random.Random(55)
    tier, converted = [], []
    for _ in range(4000):
        t = rng.choice(["free", "plus", "pro"])
        converted.append(
            "yes" if t != "free" and rng.random() < 0.27 else "no"
        )
        tier.append(t)
    df = pl.DataFrame({"tier": tier, "converted": converted})
    report = onceover.profile(df, detectors=["slice_shifts"])
    hits = [
        f
        for f in _slice_findings(report)
        if _metrics(f)["form"] == "level_share"
        and _metrics(f)["target_level"] == "yes"
        and ("tier", "eq", "free") in _conditions(f)
    ]
    assert hits, "the rate-0 slice was not stated"
    m = _metrics(hits[0])
    assert m["slice_rate"] == 0.0
    assert m["confirm_rate"] == 0.0


def test_slice_findings_are_not_checkable_and_carry_the_split():
    report = onceover.profile(_tail_frame(), detectors=["slice_shifts"])
    findings = _slice_findings(report)
    assert findings
    for f in findings:
        assert f["category"] == "associations"
        evidence = f["evidence"][0]
        assert evidence["validation"]["method"] == "diagnostic_holdout"
        assert evidence["validation"]["training_rows"] > 0
        assert evidence["validation"]["evaluation_rows"] > 0
    checked = report.verify(_tail_frame())
    slice_rows = checked.filter(pl.col("type") == "slice_shift")
    assert slice_rows.height == len(findings)
    assert set(slice_rows["status"].unique().to_list()) == {"not_checkable"}
