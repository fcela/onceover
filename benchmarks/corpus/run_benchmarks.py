#!/usr/bin/env python3
"""Benchmark harness: profile the registry datasets and check expectations.

Usage:
    python benchmarks/corpus/run_benchmarks.py [--pack gritbot] [--only thyroid]
                                        [--budget standard] [--json-out PATH]

Design (issue #20):
- The registry (registry.json) is the single source: per dataset it records
  pack, mode (bundled | test-only), license, source, shape, content hash
  for bundled data, expectations, and notes.
- Bundled datasets are hash-verified before profiling. Test-only datasets
  are profiled only when their data.csv exists locally (produced by the
  dataset's prepare.py); otherwise they are reported as skipped with the
  reason from the registry.
- Expectations are predicates over the report's findings. An expectation
  with "expect": "missed" asserts the predicate currently matches nothing:
  it is a known-miss tripwire, so a future detector that starts matching
  it shows up in the run diff as an improvement to promote, not a failure.
- A run writes a JSON record (default benchmarks/runs/<UTC>.json, gitignored)
  and prints a summary. Exit code is 0 unless a dataset errored or a
  "found" expectation missed / a "missed" expectation matched.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
import time
from datetime import datetime, timezone
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


def check_expectations(findings: list[dict], expectations: list[dict],
                       coverage: dict | None = None) -> list[dict]:
    results = []
    for exp in expectations:
        kind = exp.get("kind", "finding")
        if kind == "anomaly_coverage":
            import re as _re
            from datetime import datetime as _dt, timedelta as _td
            windows = exp["windows"]
            tol = _td(hours=exp.get("tolerance_hours", 24))
            stamps = []
            for f in findings:
                if f.get("category") not in ("changes", "gaps", "intervals"):
                    continue
                blob = json.dumps(f)
                for m in _re.finditer(r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}", blob):
                    try:
                        stamps.append(_dt.fromisoformat(m.group(0)))
                    except ValueError:
                        pass
            covered = 0
            for w in windows:
                wt = _dt.fromisoformat(w)
                if any(abs((s2 - wt)) <= tol for s2 in stamps):
                    covered += 1
            ok = covered >= exp.get("min_covered", 1)
            results.append({"kind": kind, "ok": ok, "covered": covered,
                            "windows": len(windows),
                            "min_covered": exp.get("min_covered", 1),
                            "note": exp.get("note")})
            continue
        if kind == "documented_case":
            results.append({"kind": kind, "ref": exp.get("ref"),
                            "status": exp.get("status"), "ok": True,
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
        csv = ddir / "data.csv"
        if not csv.exists():
            rec["status"] = "skipped"
            rec["reason"] = entry.get("skip_reason", "data.csv not present (test-only dataset; run its prepare.py)")
            run["datasets"].append(rec)
            print(f"{entry['id']:<28} SKIPPED  {rec['reason'][:80]}")
            continue
        if entry["mode"] == "bundled":
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
            bad = [e for e in rec["expectations"] if not e["ok"]]
            rec["status"] = "ok" if not bad and not cov_bad else "expectations_failed"
            if bad or cov_bad:
                failures += 1
            print(f"{entry['id']:<28} {rec['status']:<20} {rec['rows']:>8} rows  "
                  f"{rec['findings']:>3} findings  {rec['seconds']:>6.1f}s  "
                  f"expectations {sum(e['ok'] for e in rec['expectations'])}/{len(rec['expectations'])}")
            for e in bad:
                print(f"    FAILED: {e.get('kind')} expect={e.get('expect')} "
                      f"pred={e.get('predicate')} note={e.get('note')}")
        except Exception as exc:  # noqa: BLE001 - recorded, not hidden
            rec["status"] = "error"
            rec["reason"] = f"{type(exc).__name__}: {exc}"
            failures += 1
            print(f"{entry['id']:<28} ERROR    {rec['reason'][:100]}")
        run["datasets"].append(rec)

    out = Path(args.json_out) if args.json_out else (
        BENCH / "runs" / (datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ") + ".json"))
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(run, indent=1))
    print(f"\nrun record: {out}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
