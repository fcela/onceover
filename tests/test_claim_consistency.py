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


CHECKERS = {
    "extreme_value": check_extreme_value,
    "categorical_outlier": check_categorical_outlier,
}

# Reproduced contradictions on current main, recorded so the gate can
# run green while the fixes are outstanding. Each entry names the
# issue that owns the fix. When a fix merges, the entry stops
# reproducing and this record must be edited in the same change —
# the gate fails until it is.
KNOWN_FAILURES: list[dict] = [
    # Numeric extreme-cluster rendering: singleton wording over a
    # payload that lists several extremes. Fixed on the draft branch
    # for PR #120 (issue #111's numeric half); these records stop
    # reproducing when it merges.
    {"dataset": "adult-census-income", "type": "extreme_value",
     "problem": "singleton-wording", "claim_contains": "work_class = Federal-gov",
     "issue": "#111, fixed by draft PR #120"},
    {"dataset": "bike-sharing-hourly", "type": "extreme_value",
     "problem": "singleton-wording", "claim_contains": "season = 1",
     "issue": "#111, fixed by draft PR #120"},
    {"dataset": "bike-sharing-hourly", "type": "extreme_value",
     "problem": "singleton-wording", "claim_contains": "hr = 21",
     "issue": "#111, fixed by draft PR #120"},
    {"dataset": "bike-sharing-hourly", "type": "extreme_value",
     "problem": "singleton-wording", "claim_contains": "hr = 5",
     "issue": "#111, fixed by draft PR #120"},
    # Categorical exception counts: the stated dominant share implies
    # one more exception row than the claim and the payload list.
    # Issue #111, not fixed by PR #120. The first entry is the
    # instance the corpus registry records as a partial documented
    # case; the other two are further instances the gate measured on
    # the same build (2026-11-04).
    {"dataset": "adult-census-income", "type": "categorical_outlier",
     "problem": "share-implied-count",
     "claim_contains": "education = Some-college and relationship = Husband",
     "issue": "#111"},
    {"dataset": "adult-census-income", "type": "categorical_outlier",
     "problem": "share-implied-count",
     "claim_contains": "occupation = Other-service and relationship = Own-child",
     "issue": "#111"},
    {"dataset": "adult-census-income", "type": "categorical_outlier",
     "problem": "share-implied-count",
     "claim_contains": "occupation = Craft-repair and relationship = Husband",
     "issue": "#111"},
]


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
