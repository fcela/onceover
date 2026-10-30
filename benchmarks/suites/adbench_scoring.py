#!/usr/bin/env python3
"""Findings-to-labels scoring for the ADBench Classical suite.

ADBench (Han et al., NeurIPS 2022) ships tabular datasets as .npz
files with a feature matrix X and binary anomaly labels y, but no
feature names. onceover produces findings, not row scores, so this
harness applies the documented mapping (docs/benchmarks/
2026-10-30-adbench-scoring.md):

  A labeled anomaly row counts as DETECTED when some finding's
  evidence explicitly implicates that row - a metrics payload
  carrying a list of row indices (a key ending in "_rows" whose
  value is a list of in-range integers, e.g. extreme_rows on
  extreme-value findings). Segment/range findings are not counted:
  implicating every row of a range would inflate detection. The
  implicated normal rows are reported alongside (the mapping's
  precision side).

Subset rule: every Classical dataset whose .npz in the ADBench
repository is <= 1 MB (37 of 47). The ten larger datasets are
excluded by size alone; the rule and the exclusions are recorded
in the write-up. Datasets are test-only: the .npz files are
fetched locally and never committed (suite membership does not
settle redistribution; the datasets aggregate many original
sources).

Usage:
  python benchmarks/suites/adbench_scoring.py --data-dir DIR \
      [--deadline 300] [--only NAME ...] [--out results.json]
"""
import argparse
import hashlib
import json
import pathlib
import time

import numpy as np
import polars as pl

import onceover


def implicated_rows(findings: list[dict], n_rows: int) -> tuple[set[int], dict]:
    """Collect explicitly implicated row indices from findings.

    Walks every evidence block's metrics payload (including nested
    payloads) for keys ending in '_rows' holding a list of integer
    row indices in range. Returns the row set and a per-mechanism
    count keyed by (category, type, metric key).
    """
    rows: set[int] = set()
    mechanisms: dict[str, int] = {}

    def walk(node, category, ftype):
        if isinstance(node, dict):
            for key, value in node.items():
                if (key.endswith("_rows") and isinstance(value, list)
                        and value and all(isinstance(v, int) and 0 <= v < n_rows
                                          for v in value)):
                    mech = f"{category}/{ftype}:{key}"
                    mechanisms[mech] = mechanisms.get(mech, 0) + len(value)
                    rows.update(value)
                else:
                    walk(value, category, ftype)
        elif isinstance(node, list):
            for item in node:
                walk(item, category, ftype)

    for finding in findings:
        for evidence in finding.get("evidence", []):
            walk(evidence.get("metrics", {}),
                 finding.get("category"), finding.get("type"))
    return rows, mechanisms


def score_dataset(path: pathlib.Path, deadline: float) -> dict:
    raw = path.read_bytes()
    data = np.load(path)
    X, y = data["X"], data["y"]
    n_rows, n_cols = X.shape
    # Cast to Float64: some suite files store X as integers (e.g.
    # letter as UInt8), which the profiler's ingestion rejects; the
    # cast is value-preserving for every suite dataset.
    frame = pl.DataFrame({f"x{i}": X[:, i].astype(np.float64)
                          for i in range(n_cols)})
    anomalies = {int(i) for i in np.where(y == 1)[0]}

    start = time.time()
    report = onceover.profile(frame, budget="standard",
                              deadline_seconds=deadline)
    seconds = time.time() - start

    rows, mechanisms = implicated_rows(report.findings, n_rows)
    coverage_errors = [name for name, cov in report.data["coverage"].items()
                       if cov.get("status") == "error"]
    return {
        "dataset": path.stem,
        "npz_sha256": hashlib.sha256(raw).hexdigest(),
        "rows": int(n_rows),
        "cols": int(n_cols),
        "anomalies": len(anomalies),
        "findings": len(report.findings),
        "implicated_rows": len(rows),
        "detected": len(anomalies & rows),
        "missed": len(anomalies - rows),
        "implicated_normal": len(rows - anomalies),
        "mechanisms": mechanisms,
        "coverage_errors": coverage_errors,
        "seconds": round(seconds, 2),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", required=True,
                        help="directory holding the subset .npz files")
    parser.add_argument("--deadline", type=float, default=300.0,
                        help="per-dataset profile deadline in seconds")
    parser.add_argument("--only", nargs="*", default=None,
                        help="dataset stems to run (default: all present)")
    parser.add_argument("--out", default=None,
                        help="write the results JSON here")
    args = parser.parse_args()

    paths = sorted(pathlib.Path(args.data_dir).glob("*.npz"))
    if args.only:
        paths = [p for p in paths if p.stem in set(args.only)]
    results = []
    for path in paths:
        try:
            rec = score_dataset(path, args.deadline)
        except Exception as exc:  # one dataset must not kill the suite
            rec = {"dataset": path.stem, "error": f"{type(exc).__name__}: {exc}"}
            print(f"{path.stem:26s} ERROR {exc}", flush=True)
            results.append(rec)
            continue
        results.append(rec)
        print(f"{rec['dataset']:26s} rows {rec['rows']:7d} cols {rec['cols']:4d} "
              f"anomalies {rec['anomalies']:5d} detected {rec['detected']:5d} "
              f"missed {rec['missed']:5d} implicated_normal {rec['implicated_normal']:5d} "
              f"findings {rec['findings']:4d} {rec['seconds']:7.1f}s", flush=True)
    scored = [r for r in results if "error" not in r]
    total_anom = sum(r["anomalies"] for r in scored)
    total_det = sum(r["detected"] for r in scored)
    print(f"\n{len(scored)} datasets scored, "
          f"{len(results) - len(scored)} errored; anomalies {total_anom}; "
          f"detected {total_det} ({total_det / max(total_anom, 1):.1%}); "
          f"total profile time {sum(r['seconds'] for r in scored):.1f}s")
    if args.out:
        pathlib.Path(args.out).write_text(json.dumps(results, indent=1) + "\n")
        print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
