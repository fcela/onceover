"""Smoke test of the differential gate (benchmarks/gate/, issue #317).

Runs the --fast subset end to end, checks the scorecard schema, and
checks the exit codes of compare.py on hand-made scorecards. The gate's
plant generators need numpy, which is not a library dependency, so the
module is skipped where numpy is missing.
"""
import copy
import json
import subprocess
import sys
from pathlib import Path

import pytest

pytest.importorskip("numpy", reason="the gate's plants and shuffles need numpy")

GATE = Path(__file__).resolve().parent.parent / "benchmarks" / "gate"


def compare(base, head, tmp_path, *extra):
    b, h = tmp_path / "b.json", tmp_path / "h.json"
    b.write_text(json.dumps(base))
    h.write_text(json.dumps(head))
    # Isolate the synthetic cards from the repository's live accepted
    # file: these tests judge compare.py's tolerance arithmetic, not
    # the acceptances in force. A caller passing its own --accepted
    # overrides this one (argparse takes the last value).
    empty = tmp_path / "accepted-empty.json"
    empty.write_text(json.dumps({"accepted": []}))
    return subprocess.run([sys.executable, str(GATE / "compare.py"), str(b), str(h),
                           "--accepted", str(empty), *extra],
                          capture_output=True, text=True)


def card(count=5, seconds=10.0, mismatch=0, held_count=1):
    ds = {"status": "ok", "held_out": False, "components": {
        "planted": {"status": "ok", "metrics": {"found_4k": count}},
        "cost": {"status": "ok", "metrics": {"profile_seconds": seconds}},
        "truth": {"status": "ok", "metrics": {"mismatch": mismatch}}}}
    held = {"status": "ok", "held_out": True, "components": {
        "planted": {"status": "ok", "metrics": {"found_4k": held_count}}}}
    return {
        "schema": 1,
        "provenance": {"onceover_version": "0", "commit": "x", "seeds": [0]},
        "metric_specs": {
            "planted.found_4k": {"direction": "higher", "kind": "count"},
            "cost.profile_seconds": {"direction": "lower", "kind": "time"},
            "truth.mismatch": {"direction": "lower", "kind": "count", "zero": True}},
        "datasets": {"a": ds, "h": held},
        "totals": {"planted": {"found_4k": count}, "cost": {"profile_seconds": seconds},
                   "truth": {"mismatch": mismatch}},
        "held_out_totals": {"planted": {"found_4k": held_count}},
    }


def test_compare_equal(tmp_path):
    r = compare(card(), card(), tmp_path)
    assert r.returncode == 0, r.stdout + r.stderr
    assert "PASS" in r.stdout


def test_compare_improved_lists_improvement(tmp_path):
    r = compare(card(count=5), card(count=7), tmp_path)
    assert r.returncode == 0
    assert "## Improved" in r.stdout and "planted.found_4k" in r.stdout.split("## Improved")[1]


def test_compare_regressed(tmp_path):
    r = compare(card(count=5), card(count=4), tmp_path)
    assert r.returncode == 1
    assert "FAIL" in r.stdout


def test_compare_time_within_tolerance(tmp_path):
    assert compare(card(seconds=10.0), card(seconds=11.5), tmp_path).returncode == 0
    assert compare(card(seconds=0.2), card(seconds=0.9), tmp_path).returncode == 0
    assert compare(card(seconds=10.0), card(seconds=14.0), tmp_path).returncode == 1


def test_compare_zero_metric_fails_even_when_base_has_it(tmp_path):
    assert compare(card(mismatch=1), card(mismatch=1), tmp_path).returncode == 1


def test_compare_held_out_excluded_by_default(tmp_path):
    base, head = card(held_count=3), card(held_count=1)
    assert compare(base, head, tmp_path).returncode == 0
    assert compare(base, head, tmp_path, "--include-held-out").returncode == 1


def test_compare_dataset_lost(tmp_path):
    head = copy.deepcopy(card())
    head["datasets"]["a"] = {"status": "skipped", "held_out": False, "components": {},
                             "reason": "gone"}
    assert compare(card(), head, tmp_path).returncode == 1


def test_fast_run_writes_valid_scorecard(tmp_path):
    out = tmp_path / "s.json"
    r = subprocess.run([sys.executable, str(GATE / "run.py"), "--fast", "--out", str(out),
                        "--commit", "test"], capture_output=True, text=True)
    assert r.returncode == 0, r.stdout + r.stderr
    sc = json.loads(out.read_text(encoding="utf-8"))
    assert sc["schema"] == 1
    assert sc["provenance"]["commit"] == "test"
    assert {"onceover_version", "polars", "python", "seeds"} <= set(sc["provenance"])
    assert sc["datasets"] and all(d["status"] == "ok" for d in sc["datasets"].values())
    for name, comp in sc["totals"].items():
        for metric in comp:
            assert f"{name}.{metric}" in sc["metric_specs"]
    assert sc["totals"]["truth"]["mismatch"] == 0
    assert sc["totals"]["reference"]["check_failed"] == 0
    # a scorecard compared with itself passes
    same = subprocess.run([sys.executable, str(GATE / "compare.py"), str(out), str(out)],
                          capture_output=True, text=True)
    assert same.returncode == 0, same.stdout


def _accepted(tmp_path, entries):
    path = tmp_path / "accepted.json"
    path.write_text(json.dumps({"accepted": entries}))
    return path


def test_accepted_regression_passes_within_its_limit(tmp_path):
    acc = _accepted(tmp_path, [{"metric": "planted.found_4k", "limit": 4, "reason": "traded for X"}])
    r = compare(card(count=5), card(count=4), tmp_path, "--accepted", str(acc))
    assert r.returncode == 0, r.stdout
    assert "## Worse and accepted" in r.stdout and "traded for X" in r.stdout


def test_accepted_regression_fails_beyond_its_limit(tmp_path):
    acc = _accepted(tmp_path, [{"metric": "planted.found_4k", "limit": 4, "reason": "traded for X"}])
    r = compare(card(count=5), card(count=3), tmp_path, "--accepted", str(acc))
    assert r.returncode == 1
    assert "accepted nothing" in r.stdout


def test_accepted_entry_does_not_cover_other_metrics(tmp_path):
    acc = _accepted(tmp_path, [{"metric": "planted.found_4k", "limit": 0, "reason": "r"}])
    r = compare(card(), card(mismatch=1), tmp_path, "--accepted", str(acc))
    assert r.returncode == 1


def test_accepted_entry_without_a_reason_is_rejected(tmp_path):
    acc = _accepted(tmp_path, [{"metric": "planted.found_4k", "limit": 4}])
    r = compare(card(count=5), card(count=4), tmp_path, "--accepted", str(acc))
    assert r.returncode == 2


def _report(*args):
    return subprocess.run([sys.executable, str(GATE / "report.py"), *args],
                          capture_output=True, text=True)


def test_report_history_and_render(tmp_path):
    hist = tmp_path / "history"
    acc = _accepted(tmp_path, [{"metric": "planted.found_4k", "limit": 4, "reason": "traded for X"}])
    steps = [("aaaaaaa1", "2026-01-01T00:00:00Z", "first change", card(count=5), "none"),
             ("bbbbbbb2", "2026-01-02T00:00:00Z", "second | change", card(count=7, seconds=30.0), "none"),
             ("ccccccc3", "2026-01-03T00:00:00Z", "third change", card(count=4, seconds=30.0), str(acc))]
    for sha, date, subject, sc, accepted in steps:
        path = tmp_path / f"{sha}.json"
        path.write_text(json.dumps(sc))
        r = _report("add", str(path), "--history", str(hist), "--commit", sha, "--date", date,
                    "--subject", subject, "--accepted", accepted)
        assert r.returncode == 0, r.stderr
    assert [p.name[-12:-5] for p in sorted(hist.glob("*.json"))] == ["aaaaaaa", "bbbbbbb", "ccccccc"]
    out = tmp_path / "REPORT.md"
    r = _report("render", "--history", str(hist), "--out", str(out))
    assert r.returncode == 0, r.stderr
    text = out.read_text(encoding="utf-8")
    # newest change first in the headline table
    assert text.index("`ccccccc`") < text.index("`bbbbbbb`") < text.index("`aaaaaaa`")
    # the latest change is judged against the one before it, with the accepted reason
    latest = text.split("## Latest change")[1].split("## Every total metric")[0]
    assert "traded for X" in latest and "planted.found_4k" in latest
    # movement is stated against the previous change
    assert "7 (+2, better)" in text
    # a pipe in a commit subject must not break the table
    assert "second | change" not in text


def test_report_render_without_history_fails(tmp_path):
    (tmp_path / "empty").mkdir()
    r = _report("render", "--history", str(tmp_path / "empty"), "--out", str(tmp_path / "r.md"))
    assert r.returncode == 2


def _reference_module():
    if str(GATE) not in sys.path:
        sys.path.insert(0, str(GATE))
    from components import reference
    return reference


def _finding(category, type_, columns):
    return {"category": category, "type": type_, "subject": {"columns": list(columns)}}


def test_reference_concentration_kind_credits_both_concentration_shapes():
    reference = _reference_module()
    value = _finding("concentration", "value_concentration", ["native_country"])
    assert reference.finding_fits(value, {"columns": ["native_country"], "kind": "concentration"})
    cell = _finding("concentration", "hotspot", ["clouds_all", "weather_main"])
    st = {"columns": ["clouds_all", "weather_main"], "kind": "concentration"}
    assert reference.finding_fits(cell, st)
    # the relates kind does not credit the concentration category
    assert not reference.finding_fits(cell, {**st, "kind": "relates"})
    assert not reference.finding_fits(value, {"columns": ["native_country"], "kind": "relates"})


def test_reference_completeness_kind_credits_gaps_not_missingness():
    reference = _reference_module()
    period = _finding("gaps", "partial_period", ["date", "state", "unemployment_rate"])
    st = {"columns": ["unemployment_rate", "date"], "kind": "completeness"}
    assert reference.finding_fits(period, st)
    # the missingness kind requires the missingness category and does
    # not credit a gaps finding
    assert not reference.finding_fits(period, {**st, "kind": "missingness"})


def test_must_not_matches_columns_within_and_excluded_types():
    reference = _reference_module()
    st = {"columns_within": ["dteday", "yr", "mnth"], "exclude_types": ["value_concentration"]}
    assert reference.violates(_finding("changes", "level_shift", ["yr", "dteday"]), st)
    # a column outside the set: not a restatement of the set
    assert not reference.violates(_finding("changes", "level_shift", ["cnt", "dteday"]), st)
    # an excluded type is not counted
    assert not reference.violates(_finding("concentration", "value_concentration", ["yr"]), st)
    # min_columns
    assert not reference.violates(_finding("changes", "level_shift", ["yr"]),
                                  {**st, "min_columns": 2})


def test_must_not_effect_below_reads_the_evidence():
    reference = _reference_module()
    weak = {**_finding("associations", "statistical", ["a", "b"]),
            "evidence": [{"metrics": {"pearson": -0.05, "spearman": 0.04, "xi_ab": 0.01}}]}
    strong = {**weak, "evidence": [{"metrics": {"pearson": 0.05, "cramers_v": 0.3}}]}
    no_effect = _finding("associations", "statistical", ["a", "b"])
    st = {"types": ["statistical"], "effect_below": 0.1}
    assert reference.violates(weak, st)
    assert not reference.violates(strong, st)
    assert not reference.violates(no_effect, st)


def test_must_not_max_matches_counts_only_the_excess():
    reference = _reference_module()

    class Prof:
        findings = [_finding("associations", "level_contrast", ["cnt", "hr"]) for _ in range(5)]

    st = {"columns": ["cnt", "hr"], "types": ["level_contrast"], "max_matches": 1}
    assert reference.violations_in_full(Prof(), st) == 4
    assert reference.violations_in_full(Prof(), {**st, "max_matches": 9}) == 0


def _ceilings(tmp_path, entries):
    path = tmp_path / "ceilings.json"
    path.write_text(json.dumps({"ceilings": entries}))
    return path


def test_ceiling_fails_above_the_limit_even_when_base_is_equal(tmp_path):
    ceil = _ceilings(tmp_path, [{"metric": "planted.found_4k", "limit": 4, "reason": "target"}])
    r = compare(card(count=5), card(count=5), tmp_path, "--ceilings", str(ceil))
    assert r.returncode == 1, r.stdout
    assert "## Ceilings" in r.stdout and "above" in r.stdout


def test_ceiling_passes_at_the_limit_and_notes_a_lower_head(tmp_path):
    ceil = _ceilings(tmp_path, [{"metric": "planted.found_4k", "limit": 5, "reason": "target"}])
    assert compare(card(count=5), card(count=5), tmp_path, "--ceilings", str(ceil)).returncode == 0
    ceil = _ceilings(tmp_path, [{"metric": "truth.mismatch", "limit": 3, "reason": "target"}])
    r = compare(card(), card(), tmp_path, "--ceilings", str(ceil))
    assert r.returncode == 0 and "lower the limit" in r.stdout


def test_ceiling_on_a_dataset_scope_and_on_an_unmeasured_metric(tmp_path):
    ceil = _ceilings(tmp_path, [
        {"metric": "planted.found_4k", "scope": "a", "limit": 4, "reason": "per dataset"}])
    assert compare(card(count=5), card(count=5), tmp_path, "--ceilings", str(ceil)).returncode == 1
    ceil = _ceilings(tmp_path, [{"metric": "permuted.false_full", "limit": 0, "reason": "r"}])
    r = compare(card(), card(), tmp_path, "--ceilings", str(ceil))
    assert r.returncode == 0 and "not measured" in r.stdout


def test_ceiling_entry_without_a_reason_is_rejected(tmp_path):
    ceil = _ceilings(tmp_path, [{"metric": "planted.found_4k", "limit": 4}])
    assert compare(card(), card(), tmp_path, "--ceilings", str(ceil)).returncode == 2


def test_committed_ceilings_name_measured_metrics():
    doc = json.loads((GATE / "ceilings.json").read_text(encoding="utf-8"))
    if str(GATE) not in sys.path:
        sys.path.insert(0, str(GATE))
    from components import permuted, reference
    known = {f"reference.{m}" for m in reference.METRICS} | {f"permuted.{m}" for m in permuted.METRICS}
    assert doc["ceilings"]
    for e in doc["ceilings"]:
        assert e["metric"] in known, e["metric"]
        assert e["reason"]
