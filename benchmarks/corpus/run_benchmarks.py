#!/usr/bin/env python3
"""Benchmark harness: profile the registry datasets and check expectations.

Usage:
    python benchmarks/corpus/run_benchmarks.py [--pack gritbot] [--only thyroid]
                                        [--budget standard] [--json-out PATH]

Design (issue #20):
- The registry (registry.json) is the single source: per dataset it records
  pack, mode (bundled | test-only), license, source, shape, content hash
  for bundled data and for reference datasets, expectations, and notes.
- A dataset whose entry pins a sha256 is hash-verified before profiling:
  bundled datasets always, reference datasets (test-only with a pin) when
  their data.csv exists locally. Test-only datasets are profiled only when
  their data.csv exists locally (produced by the dataset's prepare.py);
  otherwise they are reported as skipped with the reason from the registry.
- Expectations are predicates over the report's findings. An expectation
  with "expect": "missed" asserts the predicate currently matches nothing:
  it is a known-miss tripwire, so a future detector that starts matching
  it shows up in the run diff as an improvement to promote, not a failure.
- `documented_case` expectations are reviewed documentation, not machine
  checks (issue #115): they carry no pass/fail of their own, are tallied
  separately in the summary by disposition (found / partial / missed),
  and never count toward a dataset's machine-verified pass tally.
- A run writes a JSON record (default benchmarks/runs/<UTC>.json, gitignored)
  and prints a summary that keeps machine-verified results and documented
  outcomes in separate tallies. Exit codes: 1 when a dataset errored or a
  machine-verified expectation failed; 2 when the machine checks are clean
  but documented cases stand at "missed" (a green machine run must not read
  as "every documented case passes"); 0 otherwise.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
import time
from collections import Counter
from datetime import datetime, timedelta, timezone
from pathlib import Path

import polars as pl

BENCH = Path(__file__).parent
ROOT = BENCH.parent.parent


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def match_finding(f: dict, pred: dict) -> bool:
    if "category" in pred and f.get("category") != pred["category"]:
        return False
    if "type" in pred and f.get("type") != pred["type"]:
        return False
    if "form" in pred and (f.get("specification") or {}).get("form") != pred["form"]:
        return False
    subj = f.get("subject") or {}
    if "target" in pred and subj.get("target") != pred["target"]:
        return False
    cols = set(subj.get("columns") or [])
    if "columns_any" in pred and not cols.intersection(pred["columns_any"]):
        return False
    if "columns_all" in pred and not set(pred["columns_all"]).issubset(cols):
        return False
    if "claim_contains" in pred and pred["claim_contains"] not in (f.get("claim") or ""):
        return False
    return True


def finding_metrics(f: dict) -> dict:
    """The finding's primary evidence metrics: its first evidence
    block, the measurement the claim was rendered from. Later blocks
    are supplementary (validation, corroboration) and may describe
    other slices, so they are never merged in."""
    for ev in f.get("evidence") or []:
        return dict(ev.get("metrics") or {})
    return {}


def parse_timestamp(value) -> datetime | None:
    """Parse an ISO date or datetime string; None for anything else."""
    if not isinstance(value, str):
        return None
    try:
        return datetime.fromisoformat(value)
    except ValueError:
        return None


def event_interval(f: dict) -> tuple[datetime, datetime] | None:
    """Typed event evidence: the (start, end) of the event a finding is
    about, read from the finding's own event/interval fields only
    (issue #115). Returns None when the finding carries no typed event
    evidence, so unrelated timestamps elsewhere in the serialized
    finding — coverage prose, segment boundaries of other findings'
    payloads, a different column's values — can never satisfy an
    event-window check.

    Sources, per finding type:
    - transient_excursion: specification.definition event_start /
      event_end (mirrored in the evidence metrics).
    - level_shift / trend_change / variance_change: the evidence
      metrics' breakpoint (a point event).
    - gap: the evidence metrics' from / to (the missing stretch).
    """
    typ = f.get("type")
    if typ == "transient_excursion":
        definition = (f.get("specification") or {}).get("definition") or {}
        metrics = finding_metrics(f)
        start = parse_timestamp(definition.get("event_start")
                                or metrics.get("event_start"))
        end = parse_timestamp(definition.get("event_end")
                              or metrics.get("event_end"))
        if start is not None and end is not None:
            return (start, end)
        peak = parse_timestamp(definition.get("peak_time")
                               or metrics.get("peak_time"))
        return (peak, peak) if peak is not None else None
    if typ in ("level_shift", "trend_change", "variance_change"):
        bp = parse_timestamp(finding_metrics(f).get("breakpoint"))
        return (bp, bp) if bp is not None else None
    if typ == "gap":
        metrics = finding_metrics(f)
        start = parse_timestamp(metrics.get("from"))
        end = parse_timestamp(metrics.get("to"))
        if start is not None and end is not None:
            return (start, end)
    return None


def check_expectations(findings: list[dict], expectations: list[dict],
                       coverage: dict | None = None) -> list[dict]:
    results = []
    for exp in expectations:
        kind = exp.get("kind", "finding")
        if kind == "anomaly_coverage":
            windows = [parse_timestamp(w) for w in exp["windows"]]
            tol = timedelta(hours=exp.get("tolerance_hours", 24))
            events = [iv for f in findings
                      if (iv := event_interval(f)) is not None]
            covered = 0
            for wt in windows:
                if wt is None:
                    continue
                # A window is covered when a typed event interval
                # overlaps the tolerance band around the labeled time.
                if any(start - tol <= wt <= end + tol
                       for start, end in events):
                    covered += 1
            ok = covered >= exp.get("min_covered", 1)
            results.append({"kind": kind, "ok": ok, "covered": covered,
                            "windows": len(windows),
                            "events": len(events),
                            "min_covered": exp.get("min_covered", 1),
                            "note": exp.get("note")})
            continue
        if kind == "documented_case":
            # Reviewed documentation, not a machine check (issue #115):
            # the result carries its reviewed disposition and no "ok" of
            # its own, so it can never be counted as a machine-verified
            # pass. The run summary tallies dispositions separately and
            # the exit status records missed cases (see summarize()).
            results.append({"kind": kind, "ref": exp.get("ref"),
                            "status": exp.get("status"),
                            "verified": False,
                            "note": exp.get("note")})
            continue
        if kind == "max_findings":
            ok = len(findings) <= exp["value"]
            results.append({"kind": kind, "value": exp["value"],
                            "actual": len(findings), "ok": ok,
                            "note": exp.get("note")})
            continue
        if kind == "coverage_ok":
            bad = {k: v for k, v in (coverage or {}).items()
                   if v.get("status") == "error"}
            results.append({"kind": kind, "ok": not bad,
                            "matched": len(bad), "note": exp.get("note")})
            continue
        if kind == "coverage_status":
            cov = (coverage or {}).get(exp["detector"], {})
            ok = cov.get("status") == exp["status"]
            if ok and "detail_contains" in exp:
                ok = exp["detail_contains"] in cov.get("detail", "")
            results.append({"kind": kind, "detector": exp["detector"],
                            "expected_status": exp["status"],
                            "actual_status": cov.get("status"), "ok": ok,
                            "note": exp.get("note")})
            continue
        hits = [f for f in findings if match_finding(f, exp)]
        expect = exp.get("expect", "found")
        if expect == "found":
            ok = len(hits) >= exp.get("min", 1)
        else:  # known-miss tripwire
            ok = len(hits) == 0
        results.append({"kind": "finding", "expect": expect, "ok": ok,
                        "matched": len(hits),
                        "matched_ids": [h.get("id") for h in hits[:5]],
                        "predicate": {k: v for k, v in exp.items()
                                      if k not in ("kind", "expect", "min", "note")},
                        "note": exp.get("note")})
    return results


def summarize(run: dict) -> tuple[dict, int]:
    """Run-level accounting: machine-verified results and documented
    outcomes in separate tallies (issue #115), plus the exit code.

    A dataset counts as machine-verified ok when every expectation that
    carries an "ok" passed and no coverage entry errored; documented
    cases never enter that tally. Exit code: 1 on any machine failure
    or dataset error, 2 when the machine checks are clean but
    documented cases stand at "missed", 0 otherwise.
    """
    statuses = Counter(d["status"] for d in run["datasets"])
    mv_passed = mv_total = 0
    documented: Counter = Counter()
    missed_in: list[str] = []
    for d in run["datasets"]:
        for e in d.get("expectations", []):
            if e.get("kind") == "documented_case":
                documented[e.get("status")] += 1
                if e.get("status") == "missed" and d["id"] not in missed_in:
                    missed_in.append(d["id"])
            elif "ok" in e:
                mv_total += 1
                mv_passed += bool(e["ok"])
    machine_failed = statuses.get("expectations_failed", 0) + statuses.get("error", 0)
    if machine_failed:
        code = 1
    elif documented.get("missed"):
        code = 2
    else:
        code = 0
    summary = {
        "datasets": dict(statuses),
        "datasets_total": len(run["datasets"]),
        "machine_verified": {"passed": mv_passed, "total": mv_total},
        "documented_cases": dict(documented),
        "documented_missed_in": missed_in,
        "exit_code": code,
    }
    return summary, code


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--pack")
    ap.add_argument("--only")
    ap.add_argument("--budget", default=None)
    ap.add_argument("--json-out")
    args = ap.parse_args()

    sys.path.insert(0, str(ROOT / "python"))
    import onceover

    registry = json.loads((BENCH / "registry.json").read_text())
    run = {"started": datetime.now(timezone.utc).isoformat(),
           "onceover_version": onceover.__version__, "datasets": []}
    failures = 0

    for entry in registry["datasets"]:
        if args.pack and entry["pack"] != args.pack:
            continue
        if args.only and entry["id"] != args.only:
            continue
        rec = {"id": entry["id"], "pack": entry["pack"], "mode": entry["mode"]}
        ddir = BENCH / entry["dir"]
        csv = ddir / entry.get("file", "data.csv")
        if not csv.exists():
            rec["status"] = "skipped"
            rec["reason"] = entry.get("skip_reason", "data file not present (test-only dataset; run its prepare.py)")
            run["datasets"].append(rec)
            print(f"{entry['id']:<28} SKIPPED  {rec['reason'][:80]}")
            continue
        if "sha256" in entry:
            digest = sha256(csv)
            if digest != entry["sha256"]:
                rec["status"] = "error"
                rec["reason"] = f"sha256 mismatch: registry {entry['sha256'][:12]}.. file {digest[:12]}.."
                run["datasets"].append(rec)
                failures += 1
                print(f"{entry['id']:<28} ERROR    hash mismatch")
                continue
            rec["sha256_verified"] = True
        try:
            df = pl.read_csv(csv, infer_schema_length=None,
                             try_parse_dates=bool(entry.get("parse_dates")))
            budget = args.budget or entry.get("budget", "standard")
            t = time.time()
            report = onceover.profile(df, budget=budget,
                                      detectors=entry.get("detectors"),
                                      hints=entry.get("hints"))
            rec["seconds"] = round(time.time() - t, 2)
            rec["rows"], rec["cols"] = df.height, df.width
            findings = report.data["findings"]
            rec["findings"] = len(findings)
            from collections import Counter
            rec["categories"] = dict(Counter(f["category"] for f in findings))
            cov_bad = {k: v for k, v in report.data["coverage"].items()
                       if v.get("status") == "error"}
            rec["coverage_errors"] = cov_bad
            rec["expectations"] = check_expectations(
                findings, entry.get("expectations", []),
                coverage=report.data["coverage"])
            bad = [e for e in rec["expectations"] if e.get("ok") is False]
            rec["status"] = "ok" if not bad and not cov_bad else "expectations_failed"
            if bad or cov_bad:
                failures += 1
            mv = [e for e in rec["expectations"] if "ok" in e]
            docs = Counter(e.get("status") for e in rec["expectations"]
                           if e.get("kind") == "documented_case")
            line = (f"{entry['id']:<28} {rec['status']:<20} {rec['rows']:>8} rows  "
                    f"{rec['findings']:>3} findings  {rec['seconds']:>6.1f}s  "
                    f"expectations {sum(e['ok'] for e in mv)}/{len(mv)} machine-verified")
            if docs:
                line += ("; documented " + ", ".join(
                    f"{k} {v}" for k, v in sorted(docs.items())))
            print(line)
            for e in bad:
                print(f"    FAILED: {e.get('kind')} expect={e.get('expect')} "
                      f"pred={e.get('predicate')} note={e.get('note')}")
        except Exception as exc:  # noqa: BLE001 - recorded, not hidden
            rec["status"] = "error"
            rec["reason"] = f"{type(exc).__name__}: {exc}"
            failures += 1
            print(f"{entry['id']:<28} ERROR    {rec['reason'][:100]}")
        run["datasets"].append(rec)

    summary, code = summarize(run)
    run["summary"] = summary
    out = Path(args.json_out) if args.json_out else (
        BENCH / "runs" / (datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ") + ".json"))
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(run, indent=1))
    ds = summary["datasets"]
    mv = summary["machine_verified"]
    print(f"\nrun record: {out}")
    print("summary: " + ", ".join(f"{v} {k}" for k, v in sorted(ds.items()))
          + f" over {summary['datasets_total']} datasets")
    print(f"machine-verified expectations: {mv['passed']}/{mv['total']} passed")
    if summary["documented_cases"]:
        print("documented cases (reviewed, not machine-verified): "
              + ", ".join(f"{k} {v}" for k, v in
                          sorted(summary["documented_cases"].items())))
    if summary["documented_missed_in"]:
        print("documented misses on record in: "
              + ", ".join(summary["documented_missed_in"]))
    print(f"exit code: {code}"
          + (" (machine-verified failures present)" if code == 1 else
             " (machine checks clean; documented misses on record)" if code == 2
             else " (clean)"))
    return code


if __name__ == "__main__":
    sys.exit(main())
