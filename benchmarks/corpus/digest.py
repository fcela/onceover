#!/usr/bin/env python3
"""Context-budgeted report digest (issue #27, prototype).

Produces a bounded-size plain-text digest of a profile report:
header (shape, budget, coverage one-liners), findings grouped by
category with near-duplicate rollups, and the top findings per
category by interestingness. Degradation order as the budget
shrinks: full claims -> rolled-up claims -> category counts ->
header only. Every digest states what it dropped.

Usage: python benchmarks/corpus/digest.py <data.csv> [--budget-chars 4000]
Prints the digest and, on stderr, the full report's JSON size for
comparison.
"""
from __future__ import annotations

import argparse
import json
import sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).parent.parent.parent
sys.path.insert(0, str(ROOT / "python"))


def rollup_key(f: dict) -> tuple:
    subj = f.get("subject") or {}
    return (f.get("category"), f.get("type"),
            (f.get("specification") or {}).get("form"), subj.get("target"))


def build_digest(report: dict, budget_chars: int) -> str:
    data = report["data"] if "data" in report else report
    findings = data["findings"]
    overview = data.get("overview", {})
    lines = []
    lines.append(f"profile digest: {overview.get('n_rows', '?')} rows x "
                 f"{overview.get('n_columns', '?')} columns, "
                 f"{len(findings)} findings, schema "
                 f"{data.get('schema_version', '?')}")
    if overview.get("headline"):
        lines.append("headline: " + overview["headline"])
    cov = data.get("coverage", {})
    skipped = [f"{k}={v.get('status')}" for k, v in cov.items()
               if v.get("status") not in ("ran_exact", "ran_sampled")]
    lines.append("coverage: " + (", ".join(skipped) if skipped else "all passes ran"))
    cats = Counter(f["category"] for f in findings)
    lines.append("findings by category: " +
                 ", ".join(f"{c} {n}" for c, n in cats.most_common()))
    header = "\n".join(lines) + "\n"

    groups: dict[tuple, list[dict]] = defaultdict(list)
    for f in findings:
        groups[rollup_key(f)].append(f)
    rolled = []
    for key, fs in groups.items():
        fs_sorted = sorted(fs, key=lambda x: -(x.get("interestingness") or 0))
        rolled.append((key, fs_sorted))
    rolled.sort(key=lambda kv: -(kv[1][0].get("interestingness") or 0))

    body_lines = []
    for (cat, typ, form, target), fs in rolled:
        head = f"[{cat}] {typ or ''}{'/' + form if form else ''}"
        if target:
            head += f" -> {target}"
        if len(fs) > 1:
            head += f" (x{len(fs)})"
        body_lines.append(head + ": " + fs[0]["claim"])
    body = "\n".join(body_lines)

    full = header + "\n" + body + "\n"
    if len(full) <= budget_chars:
        return full
    # degrade: keep header + as many rolled lines as fit, say what dropped
    kept, used = [], len(header) + 60
    for ln in body_lines:
        if used + len(ln) + 1 > budget_chars:
            break
        kept.append(ln)
        used += len(ln) + 1
    dropped = len(body_lines) - len(kept)
    return (header + "\n" + "\n".join(kept) +
            f"\n... [{dropped} rolled-up finding groups omitted to fit "
            f"{budget_chars} chars; {len(findings)} findings total]\n")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("csv")
    ap.add_argument("--budget-chars", type=int, default=4000)
    ap.add_argument("--detectors")
    args = ap.parse_args()
    import polars as pl
    import onceover
    df = pl.read_csv(args.csv, infer_schema_length=None)
    detectors = args.detectors.split(",") if args.detectors else None
    report = onceover.profile(df, detectors=detectors)
    full_json = report.to_json()
    digest = build_digest(report.data, args.budget_chars)
    print(digest)
    print(f"[full report JSON: {len(full_json)} chars; digest: "
          f"{len(digest)} chars]", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
