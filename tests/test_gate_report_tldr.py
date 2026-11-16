"""Tests for the TL;DR section at the head of the gate history report
(benchmarks/gate/report.py, issue #317).

The synthetic scorecards follow the shape of the committed baseline
(benchmarks/gate/results/main-1618886.json); history entries are built
with report.slim(), the same shape report.py stores.
"""
import importlib.util
import json
import subprocess
import sys
from pathlib import Path

GATE = Path(__file__).resolve().parent.parent / "benchmarks" / "gate"
sys.path.insert(0, str(GATE))

_spec = importlib.util.spec_from_file_location("gate_report", GATE / "report.py")
report = importlib.util.module_from_spec(_spec)
sys.modules["gate_report"] = report
_spec.loader.exec_module(report)

HEADLINE = report.headline_spec()


def card(must_default=7, must_target=7, should_default=7, should_target=8,
         found_4k=20, repeated=4, repeated_target=3, false_4k=136,
         mismatch=0, error=0):
    totals = {
        "planted": {"planted": 46, "found_full": 34, "found_4k": found_4k},
        "permuted": {"false_full": 144, "false_4k": false_4k,
                     "false_full_target": 75, "false_4k_target": 67},
        "reference": {"must_total": 11, "must_4k_default": must_default,
                      "must_4k_target": must_target,
                      "must_8k_default": 11, "must_full_default": 11,
                      "must_8k_target": 11, "must_full_target": 11,
                      "should_total": 12, "should_4k_default": should_default,
                      "should_8k_default": 9, "should_full_default": 11,
                      "should_4k_target": should_target,
                      "should_8k_target": 8, "should_full_target": 10},
        "redundancy": {"repeated_default": repeated,
                       "repeated_target": repeated_target},
        "truth": {"mismatch": mismatch, "error": error, "ok": 858,
                  "not_checkable": 1423},
        "cost": {"profile_seconds": 6.9},
    }
    specs = {}
    for comp, metrics in totals.items():
        for name in metrics:
            direction = "higher" if comp in ("planted", "reference") else "lower"
            specs[f"{comp}.{name}"] = {"direction": direction, "kind": "count"}
    specs["cost.profile_seconds"] = {"direction": "lower", "kind": "time"}
    specs["truth.mismatch"]["zero"] = True
    specs["truth.error"]["zero"] = True
    return {"schema": 1,
            "provenance": {"onceover_version": "0", "commit": "x", "seeds": [0],
                           "platform": "test"},
            "metric_specs": specs, "datasets": {}, "totals": totals,
            "held_out_totals": {}}


def entry(commit, date, subject, sc, accepted=None):
    return {"meta": {"commit": commit, "date": date, "subject": subject,
                     "accepted": accepted or []},
            "scorecard": report.slim(sc)}


def tldr(entries):
    return "\n".join(report.tldr_section(entries, HEADLINE))


def test_tldr_baseline_against_itself_reports_no_change():
    base = json.loads((GATE / "results" / "main-1618886.json").read_text(encoding="utf-8"))
    entries = [entry("1618886abcdef0", "2026-11-15T20:26:13Z", "baseline", base),
               entry("bbbbbbb0000000", "2026-11-15T21:00:00Z", "the same build again", base)]
    text = tldr(entries)
    assert "0 total metrics better, 0 worse, 0 worse and accepted" in text
    assert "(unchanged)" in text
    assert "Biggest improvements: none." in text
    assert "Biggest regressions: none." in text
    assert "Accepted regressions: none." in text
    assert "none breached" in text
    # the baseline states 7 of its 11 must-tier facts in the 4k digest
    assert "7 of 11" in text


def test_tldr_improvement_and_regression_land_in_the_right_lists():
    entries = [entry("aaaaaaa0000000", "2026-01-01T00:00:00Z", "first", card()),
               entry("bbbbbbb0000000", "2026-01-02T00:00:00Z", "second",
                     card(found_4k=23, repeated=6))]
    text = tldr(entries)
    assert "1 total metrics better, 1 worse, 0 worse and accepted" in text
    improved = text.split("Biggest improvements: ")[1].split("\n")[0]
    regressed = text.split("Biggest regressions: ")[1].split("\n")[0]
    assert "Planted structures" in improved and "20 → 23" in improved
    assert "Facts repeated" in regressed and "4 → 6" in regressed
    # the latest column carries the movement; the first recorded column
    # carries the first entry's value
    assert "23 (+3, better) | 20 | 20" in text
    assert "6 (+2, worse) | 4 | 4" in text


def test_tldr_accepted_regression_is_marked_accepted():
    accepted = [{"metric": "redundancy.repeated_default", "scope": "total", "limit": 6,
                 "reason": "traded for wider coverage"}]
    entries = [entry("aaaaaaa0000000", "2026-01-01T00:00:00Z", "first", card()),
               entry("bbbbbbb0000000", "2026-01-02T00:00:00Z", "second",
                     card(repeated=6), accepted=accepted)]
    text = tldr(entries)
    assert "0 total metrics better, 0 worse, 1 worse and accepted" in text
    assert "Biggest regressions: none." in text
    line = text.split("Accepted regressions: ")[1].split("\n")[0]
    assert "Facts repeated" in line and "4 → 6" in line
    assert "traded for wider coverage" in line


def test_tldr_verdict_counts_match_classify():
    prev = entry("aaaaaaa0000000", "2026-01-01T00:00:00Z", "first", card())
    cur = entry("bbbbbbb0000000", "2026-01-02T00:00:00Z", "second",
                card(found_4k=22, must_default=8, repeated=5, false_4k=130),
                accepted=[{"metric": "redundancy.repeated_default", "scope": "total",
                           "limit": 5, "reason": "r"}])
    c = report.classify(prev, cur)
    tot = lambda rows: sum(1 for r in rows if r[1] == "total")  # noqa: E731
    expected = (f"{tot(c['improved'])} total metrics better, {tot(c['worse'])} worse, "
                f"{tot(c['accepted'])} worse and accepted")
    assert expected in tldr([prev, cur])


def test_tldr_hard_lines_name_breaches():
    entries = [entry("aaaaaaa0000000", "2026-01-01T00:00:00Z", "first", card()),
               entry("bbbbbbb0000000", "2026-01-02T00:00:00Z", "second",
                     card(must_default=6, mismatch=2))]
    text = tldr(entries)
    assert "Hard line breached: 2 statements failed verification against the data" in text
    assert "must-tier reference fact was lost" in text and "7 → 6" in text
    assert "none breached" not in text


def test_tldr_single_entry_renders_without_comparison():
    text = tldr([entry("aaaaaaa0000000", "2026-01-01T00:00:00Z", "first", card())])
    assert "first recorded change" in text
    assert "7 of 11" in text
    assert "none breached" in text


def test_render_opens_with_the_tldr(tmp_path):
    hist = tmp_path / "history"
    hist.mkdir()
    steps = [("20260101000000-aaaaaaa.json",
              entry("aaaaaaa0000000", "2026-01-01T00:00:00Z", "first", card())),
             ("20260102000000-bbbbbbb.json",
              entry("bbbbbbb0000000", "2026-01-02T00:00:00Z", "second", card(found_4k=23)))]
    for name, e in steps:
        (hist / name).write_text(json.dumps(e))
    out = tmp_path / "REPORT.md"
    r = subprocess.run([sys.executable, str(GATE / "report.py"), "render",
                        "--history", str(hist), "--out", str(out)],
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    text = out.read_text(encoding="utf-8")
    assert text.index("## TL;DR") < text.index("## Headline metrics")
