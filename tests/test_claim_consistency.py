"""Claim/payload consistency gate (issue #115; failure class: issue #111).

A finding's claim is a statement about its own evidence payload. This
gate profiles the two case-study frames (Adult, Bike Sharing) and
asserts, for every selected finding of the affected types, that the
quantities the claim states agree with the payload it ships with:

- extreme_value: the reached value, the bulk edge, the gap, the
  central spread, the share beyond the edge, and — for the
  conditional form — that singleton wording ("all other values are
  <= edge") is used only when the payload lists exactly one extreme.
- categorical_outlier: the breaker count in the claim equals the
  payload's outlier count and row list, and the payload is internally
  consistent: n - round(dominant_share * n) must equal the outlier
  count (the issue #111 shape, where claim and count agree with each
  other but not with the stated share).
- predictive: the drivers the claim names, with their percentages,
  equal the payload's `drivers` and `driver_shares`; every named
  driver is one of the payload's `inputs`; and the holdout scores
  the claim states equal the payload's — the reduced (drivers-only)
  score when the claim reports a refit, the full model's `metric`
  otherwise, plus the categorical majority `baseline`.

Contradictions already on record are listed in KNOWN_FAILURES, keyed
to the issues that own the fixes; the gate asserts the reproduced
set matches the record exactly, so a fix landing (or a new
contradiction appearing) forces the record to be updated rather than
passing silently. The checkers are pure functions over finding
dicts, and the mutation tests below pin that a deliberately
corrupted claim or count is flagged.
"""

import json
import re
from pathlib import Path

import polars as pl

import onceover

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "benchmarks" / "corpus"

NUM = r"-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?"


def _close(a: float, b: float, tol: float = 1e-9) -> bool:
    return abs(a - b) <= tol * max(1.0, abs(a), abs(b))


def _metrics(f: dict) -> dict:
    """The finding's primary evidence metrics: its first evidence
    block, the measurement the claim was rendered from. Later blocks
    are supplementary and may describe other slices."""
    for ev in f.get("evidence") or []:
        return dict(ev.get("metrics") or {})
    return {}


def check_extreme_value(f: dict) -> list[str]:
    """Problem kinds for one extreme_value finding (empty = consistent)."""
    problems: list[str] = []
    m = _metrics(f)
    claim = f["claim"]
    side = m["side"]
    values = sorted(float(v) for v in m["extreme_values"])
    n_ext = int(m["extreme_count"])
    bulk = float(m["bulk_edge"])
    gap = float(m["gap"])
    spread = float(m["central_spread_p90_p10"])
    n = int(m["n"])
    if len(values) != n_ext:
        problems.append("payload-count")
        return problems
    extreme = values[-1] if side == "high" else values[0]
    inner = values[0] if side == "high" else values[-1]

    cluster = re.search(
        rf"cluster of (\d+) values from ({NUM}) to ({NUM}); "
        rf"its inner edge at ({NUM}) is separated from the "
        rf"(?:column|subset) remainder edge at ({NUM}) by ({NUM})", claim)
    if cluster:
        count, lo, hi, edge, bulk_c, gap_c = (
            int(cluster.group(1)), *(float(cluster.group(i)) for i in range(2, 7)))
        if count != n_ext:
            problems.append("cluster-count")
        if not (_close(lo, values[0]) and _close(hi, values[-1])):
            problems.append("cluster-range")
        if not _close(edge, inner):
            problems.append("cluster-edge")
        if not _close(bulk_c, bulk):
            problems.append("bulk-edge")
        if not _close(gap_c, gap):
            problems.append("gap")
    else:
        reached = re.search(rf"reaches ({NUM})", claim)
        if reached is None or not _close(float(reached.group(1)), extreme):
            problems.append("reached-value")
        if n_ext > 1 and "all other values are" in claim:
            # Singleton wording over a cluster: the stated bound
            # excludes extremes the payload itself lists beyond it.
            problems.append("singleton-wording")
        bound = re.search(rf"all (?:other|remaining) values are (?:<=|>=) ({NUM})",
                          claim)
        if bound is not None and not _close(float(bound.group(1)), bulk):
            problems.append("bulk-edge")
        stated_gap = re.search(rf"gap of ({NUM})", claim)
        if stated_gap is not None and not _close(float(stated_gap.group(1)), gap):
            problems.append("gap")

    pct = re.search(rf"({NUM})% of values are (?:<=|>=) ({NUM})", claim)
    if pct is not None:
        share = 100.0 * (n - n_ext) / n
        if abs(float(pct.group(1)) - round(share, 2)) > 0.005 + 1e-9:
            problems.append("share")
        if not _close(float(pct.group(2)), bulk):
            problems.append("bulk-edge")
    stated_spread = re.search(rf"p90-p10 = ({NUM})", claim)
    if stated_spread is not None and not _close(float(stated_spread.group(1)),
                                                spread):
        problems.append("spread")
    return problems


def check_categorical_outlier(f: dict) -> list[str]:
    """Problem kinds for one categorical_outlier finding."""
    problems: list[str] = []
    m = _metrics(f)
    claim = f["claim"]
    n = int(m["n"])
    share = float(m["dominant_share"])
    count = int(m["outlier_count"])
    rows = [int(r) for r in m["outlier_rows"]]

    stated = re.search(
        rf"is (.+?) in ({NUM})% of rows — (\d+) rows? breaks? the pattern: (.*)$",
        claim)
    if stated is None:
        return ["claim-shape"]
    label, pct, k, tail = (stated.group(1), float(stated.group(2)),
                           int(stated.group(3)), stated.group(4))
    if label != str(m["dominant_level"]):
        problems.append("dominant-label")
    if abs(pct - round(share * 100.0, 1)) > 0.05 + 1e-9:
        problems.append("share")
    if k != count or k != len(rows):
        problems.append("breaker-count")
    named = [int(r) for r in re.findall(r"row (\d+) \(", tail)]
    if named != rows:
        problems.append("breaker-rows")
    # The payload must agree with itself: the stated dominant share
    # implies an exception count, and it must be the listed one.
    implied = n - round(share * n)
    if implied != count:
        problems.append("share-implied-count")
    return problems


def check_predictive(f: dict) -> list[str]:
    """Problem kinds for one predictive finding (empty = consistent).

    The claim names the model's drivers with their shares of the
    split gain; the payload carries the same names and shares in
    `drivers` / `driver_shares`, the full input list in `inputs`,
    the full model's holdout score in `metric`, and — when the
    driver set is a proper subset of the inputs and the claim
    reports it — the score of a model refit on the drivers alone
    in `reduced_metric`. The claim's headline score is the reduced
    model's when a refit is reported and the full model's
    otherwise; a categorical claim also states the majority
    baseline. When the payload has no drivers the claim keeps the
    bracket form and lists every input.

    Scope: type `predictive` also covers other passes' statements
    (local regimes, interactions, target shapes). This checker
    applies only to the screen's findings, identified by their
    claim shape; a screen-shaped claim whose payload lacks the
    screen's keys is itself a shape problem.
    """
    problems: list[str] = []
    m = _metrics(f)
    claim = f["claim"]
    if " is predictable from " not in claim:
        return []
    if not {"metric", "inputs", "drivers"} <= m.keys():
        return ["claim-shape"]
    drivers = [str(d) for d in m.get("drivers") or []]
    shares = [float(s) for s in m.get("driver_shares") or []]
    inputs = [str(i) for i in m.get("inputs") or []]
    metric = float(m["metric"])
    reduced = m.get("reduced_metric")
    reduced = None if reduced is None else float(reduced)
    baseline = float(m["baseline"])

    parsed = re.search(r"^.+? is predictable from (.+?) \(holdout (.+)\)$",
                       claim)
    if parsed is None:
        return ["claim-shape"]
    named_part, rest = parsed.group(1), parsed.group(2)

    if not drivers:
        bracket = re.fullmatch(r"\[(.*)\]", named_part)
        if bracket is None:
            return ["claim-shape"]
        listed = [p for p in bracket.group(1).split(", ") if p]
        if listed != inputs:
            problems.append("driver-names")
    else:
        named: list[tuple[str, int]] = []
        for piece in named_part.split(", "):
            pm = re.fullmatch(r"(.+) \((\d+)% of gain\)", piece)
            if pm is None:
                return ["claim-shape"]
            named.append((pm.group(1), int(pm.group(2))))
        if [n for n, _ in named] != drivers:
            problems.append("driver-names")
        if len(shares) != len(drivers):
            problems.append("driver-share")
        else:
            for (_, pct), share in zip(named, shares):
                if abs(pct - share * 100.0) > 0.5 + 1e-9:
                    problems.append("driver-share")
                    break
        if any(d not in inputs for d in drivers):
            problems.append("driver-not-input")

    def stated(value: float) -> float:
        return round(value, 2)

    if rest.startswith("R² = "):
        scores = re.fullmatch(
            rf"R² = ({NUM})(?: using only these drivers; "
            rf"({NUM}) using all inputs)?", rest)
        if scores is None:
            return problems + ["claim-shape"]
        first = float(scores.group(1))
        if scores.group(2) is not None:
            if reduced is None:
                problems.append("reduced-metric")
            elif abs(first - stated(reduced)) > 0.005 + 1e-9:
                problems.append("reduced-metric")
            if abs(float(scores.group(2)) - stated(metric)) > 0.005 + 1e-9:
                problems.append("full-metric")
        else:
            if abs(first - stated(metric)) > 0.005 + 1e-9:
                problems.append("metric")
            if reduced is not None:
                problems.append("reduced-missing")
    elif rest.startswith("accuracy "):
        scores = re.fullmatch(
            rf"accuracy ({NUM}) vs ({NUM}) majority baseline"
            rf"(?:; ({NUM}) using all inputs)?", rest)
        if scores is None:
            return problems + ["claim-shape"]
        first = float(scores.group(1))
        if abs(float(scores.group(2)) - stated(baseline)) > 0.005 + 1e-9:
            problems.append("baseline")
        if scores.group(3) is not None:
            if reduced is None:
                problems.append("reduced-metric")
            elif abs(first - stated(reduced)) > 0.005 + 1e-9:
                problems.append("reduced-metric")
            if abs(float(scores.group(3)) - stated(metric)) > 0.005 + 1e-9:
                problems.append("full-metric")
        else:
            if abs(first - stated(metric)) > 0.005 + 1e-9:
                problems.append("metric")
            if reduced is not None:
                problems.append("reduced-missing")
    else:
        problems.append("claim-shape")
    return problems


CHECKERS = {
    "extreme_value": check_extreme_value,
    "categorical_outlier": check_categorical_outlier,
    "predictive": check_predictive,
}

# Reproduced contradictions on current main, recorded so the gate can
# run green while the fixes are outstanding. Each entry names the
# issue that owns the fix. When a fix merges, the entry stops
# reproducing and this record must be edited in the same change —
# the gate fails until it is.
KNOWN_FAILURES: list[dict] = []


def _finding_key(f: dict) -> tuple:
    return (f.get("type"), f["claim"])


def _profile_corpus_entry(entry: dict) -> list[dict]:
    csv = CORPUS / entry["dir"] / entry.get("file", "data.csv")
    df = pl.read_csv(csv, infer_schema_length=None,
                     try_parse_dates=bool(entry.get("parse_dates")))
    report = onceover.profile(df, budget=entry.get("budget", "standard"),
                              detectors=entry.get("detectors"),
                              hints=entry.get("hints"))
    return report.data["findings"]


def _matches(item: dict, rec: dict) -> bool:
    return (item["dataset"] == rec["dataset"] and item["type"] == rec["type"]
            and item["problem"] == rec["problem"]
            and rec["claim_contains"] in item["claim"])


def _run_gate() -> tuple[list[dict], list[dict]]:
    """(all problems, known-matched problems) over the two frames."""
    registry = json.loads((CORPUS / "registry.json").read_text())
    entries = {e["id"]: e for e in registry["datasets"]}
    found: list[dict] = []
    for dataset in ("adult-census-income", "bike-sharing-hourly"):
        for f in _profile_corpus_entry(entries[dataset]):
            checker = CHECKERS.get(f.get("type"))
            if checker is None:
                continue
            for problem in checker(f):
                found.append({"dataset": dataset, "type": f["type"],
                              "claim": f["claim"], "problem": problem})
    known = [item for item in found
             if any(_matches(item, rec) for rec in KNOWN_FAILURES)]
    return found, known


def test_claims_agree_with_their_payloads():
    found, known = _run_gate()
    new = [item for item in found if item not in known]
    stale = [rec for rec in KNOWN_FAILURES
             if not any(_matches(item, rec) for item in found)]
    assert not new, (
        "claim/payload contradictions not on the known-failure record: "
        f"{new}. If one of these is a deliberate fix or a newly "
        "documented case, record it in KNOWN_FAILURES with its issue.")
    assert not stale, (
        f"known-failure records that no longer reproduce: {stale}. "
        "A fix has landed; remove the record in the same change.")


# --- Mutation tests: the checkers must flag corrupted claims --------

def _extreme_finding(claim: str, **metrics) -> dict:
    base = {"side": "high", "extreme_values": [99.0], "extreme_count": 1,
            "bulk_edge": 80.0, "gap": 19.0, "central_spread_p90_p10": 10.0,
            "n": 273, "extreme_rows": [5]}
    base.update(metrics)
    return {"type": "extreme_value", "claim": claim,
            "subject": {"target": "hours_per_week", "columns": ["work_class"]},
            "evidence": [{"metrics": base, "scope": "work_class = Federal-gov"}]}


SINGLETON_CLAIM = ("within work_class = Federal-gov (273 rows), "
                   "hours_per_week reaches 99 — isolated from the rest of "
                   "the subset: all other values are <= 80, a gap of 19 "
                   "(1.9x the central spread (p90-p10 = 10) of the remainder)")


def test_checker_accepts_a_consistent_claim():
    assert check_extreme_value(_extreme_finding(SINGLETON_CLAIM)) == []


def test_checker_flags_a_corrupted_bound_and_gap():
    bad_bound = _extreme_finding(SINGLETON_CLAIM.replace("<= 80", "<= 70"))
    assert "bulk-edge" in check_extreme_value(bad_bound)
    bad_gap = _extreme_finding(SINGLETON_CLAIM.replace("gap of 19", "gap of 12"))
    assert "gap" in check_extreme_value(bad_gap)
    bad_reach = _extreme_finding(SINGLETON_CLAIM.replace("reaches 99",
                                                         "reaches 95"))
    assert "reached-value" in check_extreme_value(bad_reach)


def test_checker_flags_singleton_wording_over_a_cluster():
    f = _extreme_finding(
        SINGLETON_CLAIM.replace("a gap of 19", "a gap of 12"),
        extreme_values=[99.0, 99.0, 92.0], extreme_count=3, gap=12.0)
    problems = check_extreme_value(f)
    assert "singleton-wording" in problems


def _categorical_finding(claim: str, **metrics) -> dict:
    base = {"n": 100, "dominant_level": "A", "dominant_share": 0.97,
            "outlier_count": 3, "outlier_rows": [7, 8, 9]}
    base.update(metrics)
    return {"type": "categorical_outlier", "claim": claim,
            "subject": {"target": "x", "columns": ["g"]},
            "evidence": [{"metrics": base, "scope": "g = G"}]}


CATEGORICAL_CLAIM = ("within g = G (100 rows), x is A in 97.0% of rows — "
                     "3 rows break the pattern: row 7 (x = B), "
                     "row 8 (x = B), row 9 (x = C)")


def test_checker_accepts_a_consistent_categorical_claim():
    assert check_categorical_outlier(_categorical_finding(CATEGORICAL_CLAIM)) == []


def test_checker_flags_a_corrupted_breaker_count():
    bad = _categorical_finding(CATEGORICAL_CLAIM.replace("3 rows break",
                                                         "2 rows break"))
    assert "breaker-count" in check_categorical_outlier(bad)


def test_checker_flags_the_share_implied_count_mismatch():
    # Claim and outlier_count agree with each other (2) but the stated
    # share implies 3 exceptions — the issue #111 shape. The claim's
    # named rows match the payload rows, so only the share-implied
    # check can fire.
    claim = ("within g = G (100 rows), x is A in 97.0% of rows — "
             "2 rows break the pattern: row 7 (x = B), row 8 (x = B)")
    f = _categorical_finding(claim, outlier_count=2, outlier_rows=[7, 8])
    assert check_categorical_outlier(f) == ["share-implied-count"]


def _predictive_finding(claim: str, **metrics) -> dict:
    base = {"task": "regression", "metric": 0.83, "baseline": 0.0,
            "reduced_metric": 0.81,
            "inputs": ["atemp", "hum", "windspeed"],
            "drivers": ["atemp"], "driver_shares": [0.983],
            "top_features": ["atemp"], "gain_shares": [0.983],
            "n_train": 3750, "n_holdout": 1250, "rounds": 12,
            "rows_used": 5000}
    base.update(metrics)
    return {"type": "predictive", "claim": claim,
            "subject": {"target": "temp"},
            "evidence": [{"metrics": base, "scope": "all rows"}]}


PREDICTIVE_CLAIM = ("temp is predictable from atemp (98% of gain) "
                    "(holdout R² = 0.81 using only these drivers; "
                    "0.83 using all inputs)")


def test_checker_accepts_a_consistent_predictive_claim():
    assert check_predictive(_predictive_finding(PREDICTIVE_CLAIM)) == []


def test_checker_accepts_a_full_driver_claim_without_refit():
    claim = ("y is predictable from x1 (60% of gain), x2 (40% of gain) "
             "(holdout R² = 0.91)")
    f = _predictive_finding(
        claim, metric=0.91, reduced_metric=None,
        inputs=["x1", "x2"], drivers=["x1", "x2"],
        driver_shares=[0.6, 0.4], top_features=["x1", "x2"],
        gain_shares=[0.6, 0.4])
    assert check_predictive(f) == []


def test_checker_flags_a_corrupted_driver_share():
    bad = _predictive_finding(PREDICTIVE_CLAIM.replace("98% of gain",
                                                       "90% of gain"))
    assert "driver-share" in check_predictive(bad)


def test_checker_flags_a_swapped_driver_name():
    bad = _predictive_finding(PREDICTIVE_CLAIM.replace("atemp (98%",
                                                       "hum (98%"))
    assert "driver-names" in check_predictive(bad)


def test_checker_flags_a_corrupted_reduced_score():
    bad = _predictive_finding(PREDICTIVE_CLAIM.replace("R² = 0.81",
                                                       "R² = 0.71"))
    assert "reduced-metric" in check_predictive(bad)


def test_checker_flags_a_corrupted_full_score():
    bad = _predictive_finding(PREDICTIVE_CLAIM.replace("0.83 using all",
                                                       "0.93 using all"))
    assert "full-metric" in check_predictive(bad)


def test_checker_flags_a_dropped_refit_clause():
    claim = ("temp is predictable from atemp (98% of gain) "
             "(holdout R² = 0.83)")
    assert "reduced-missing" in check_predictive(
        _predictive_finding(claim))


def _predictive_cat_finding(claim: str, **metrics) -> dict:
    base = {"task": "classification", "metric": 0.90, "baseline": 0.61,
            "reduced_metric": 0.88, "inputs": ["f1", "f2"],
            "drivers": ["f1"], "driver_shares": [0.75],
            "top_features": ["f1"], "gain_shares": [0.75],
            "n_train": 3750, "n_holdout": 1250, "rounds": 9,
            "rows_used": 5000}
    base.update(metrics)
    return {"type": "predictive", "claim": claim,
            "subject": {"target": "cls"},
            "evidence": [{"metrics": base, "scope": "all rows"}]}


PREDICTIVE_CAT_CLAIM = (
    "cls is predictable from f1 (75% of gain) "
    "(holdout accuracy 0.88 vs 0.61 majority baseline; "
    "0.90 using all inputs)")


def test_checker_accepts_a_consistent_categorical_claim():
    f = _predictive_cat_finding(PREDICTIVE_CAT_CLAIM)
    assert check_predictive(f) == []


def test_checker_flags_a_corrupted_baseline():
    bad = _predictive_cat_finding(
        PREDICTIVE_CAT_CLAIM.replace("vs 0.61 majority", "vs 0.51 majority"))
    assert "baseline" in check_predictive(bad)
