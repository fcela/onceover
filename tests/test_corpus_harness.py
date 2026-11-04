"""Unit tests for the corpus runner's accounting (issue #115).

The runner's results feed the corpus summary that reviewers read as
the project's accuracy evidence, so its accounting rules are pinned
here: documented cases are reviewed documentation and never count as
machine-verified passes; missed documented cases change the exit
status; and event-window expectations read typed event evidence from
a finding's own fields only — a timestamp string anywhere else in the
serialized finding must not satisfy a window.
"""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location(
    "run_benchmarks", ROOT / "benchmarks" / "corpus" / "run_benchmarks.py")
rb = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rb)


def _documented(status):
    return {"kind": "documented_case", "status": status,
            "ref": "some §", "note": "reviewed case"}


def test_documented_case_carries_no_machine_verdict():
    for status in ("found", "partial", "missed"):
        (res,) = rb.check_expectations([], [_documented(status)])
        assert "ok" not in res
        assert res["verified"] is False
        assert res["status"] == status


def _run(datasets):
    return {"started": "t", "datasets": datasets}


def test_summary_separates_machine_and_documented_tallies():
    run = _run([{
        "id": "a", "status": "ok",
        "expectations": [
            {"kind": "finding", "ok": True},
            {"kind": "finding", "ok": True},
            _documented("found"),
            _documented("missed"),
        ],
    }])
    summary, code = rb.summarize(run)
    assert summary["machine_verified"] == {"passed": 2, "total": 2}
    assert summary["documented_cases"] == {"found": 1, "missed": 1}
    assert summary["documented_missed_in"] == ["a"]
    # Machine checks clean, but a documented miss is on record: the
    # exit status must not read as a full pass.
    assert code == 2


def test_summary_exit_codes():
    clean = _run([{"id": "a", "status": "ok", "expectations":
                    [{"kind": "finding", "ok": True}, _documented("found")]}])
    assert rb.summarize(clean)[1] == 0

    machine_fail = _run([{"id": "a", "status": "expectations_failed",
                          "expectations": [{"kind": "finding", "ok": False},
                                           _documented("missed")]}])
    assert rb.summarize(machine_fail)[1] == 1

    errored = _run([{"id": "a", "status": "error", "expectations": []}])
    assert rb.summarize(errored)[1] == 1

    partial_only = _run([{"id": "a", "status": "ok", "expectations":
                          [_documented("partial")]}])
    assert rb.summarize(partial_only)[1] == 0


def _coverage(windows, findings, tol=24):
    (res,) = rb.check_expectations(findings, [{
        "kind": "anomaly_coverage", "windows": windows,
        "tolerance_hours": tol, "min_covered": 1,
    }])
    return res


WINDOW = ["2014-11-01 19:00"]


def _excursion(start, end, extra_metrics=None):
    metrics = {"event_start": start, "event_end": end,
               "peak_time": start, "peak_value": 10.0}
    metrics.update(extra_metrics or {})
    return {
        "type": "transient_excursion", "category": "intervals",
        "specification": {"definition": {
            "event_start": start, "event_end": end, "peak_time": start}},
        "evidence": [{"metrics": metrics}],
    }


def test_anomaly_coverage_uses_the_typed_event_interval():
    res = _coverage(WINDOW, [_excursion("2014-11-01T18:30:00",
                                        "2014-11-01T20:15:00")])
    assert res["ok"] is True and res["covered"] == 1 and res["events"] == 1

    # Just outside the tolerance band: not covered.
    res = _coverage(WINDOW, [_excursion("2014-11-03T20:00:00",
                                        "2014-11-03T21:00:00")])
    assert res["ok"] is False and res["covered"] == 0


def test_anomaly_coverage_ignores_unrelated_timestamps():
    # The labeled time appears verbatim in the finding — in its claim
    # prose and in an unrelated metric — while the finding's own event
    # interval is months away. The old whole-blob regex counted this
    # as covered; typed evidence must not.
    f = _excursion("2014-06-01T00:00:00", "2014-06-01T02:00:00",
                   extra_metrics={"reference": "2014-11-01T19:00:00"})
    f["claim"] = "spike near 2014-11-01T19:00:00 in a different series"
    res = _coverage(WINDOW, [f])
    assert res["ok"] is False and res["covered"] == 0

    # A finding type with no typed event fields contributes nothing,
    # even with a timestamp-shaped value in its payload.
    other = {"type": "extreme_value", "category": "values",
             "evidence": [{"metrics": {"when": "2014-11-01T19:00:00"}}]}
    res = _coverage(WINDOW, [other])
    assert res["ok"] is False and res["events"] == 0


def test_anomaly_coverage_breakpoint_and_gap_forms():
    shift = {"type": "level_shift", "category": "changes",
             "evidence": [{"metrics": {"breakpoint": "2014-11-01 18:00:00"}}]}
    res = _coverage(WINDOW, [shift])
    assert res["ok"] is True and res["covered"] == 1

    gap = {"type": "gap", "category": "gaps",
           "evidence": [{"metrics": {"from": "2014-10-31", "to": "2014-11-02"}}]}
    res = _coverage(WINDOW, [gap])
    assert res["ok"] is True and res["covered"] == 1

    far = {"type": "variance_change", "category": "changes",
           "evidence": [{"metrics": {"breakpoint": "2015-03-01 00:00:00"}}]}
    res = _coverage(WINDOW, [far])
    assert res["ok"] is False and res["covered"] == 0
