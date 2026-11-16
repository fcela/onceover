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
    return subprocess.run([sys.executable, str(GATE / "compare.py"), str(b), str(h), *extra],
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
    sc = json.loads(out.read_text())
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
