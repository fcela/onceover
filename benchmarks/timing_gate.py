#!/usr/bin/env python3
"""CI timing gate (issue #26): the fast tier of the runtime grid.

Profiles three synthetic tables with per-pass timing on and checks
wall time against generous thresholds. The thresholds are not
performance targets — they exist to catch order-of-magnitude
regressions (a pass going quadratic, a cap ceasing to bind) before
they ship. Full grid: benchmarks/corpus/runtime_grid.py; results
and hotspot dispositions: docs/benchmarks/.

Exit code 0 = within thresholds, 1 = regression or broken timing.
"""
from __future__ import annotations

import math
import random
import sys
import time

import polars as pl

import onceover

# (rows, cols, budget, threshold_seconds, check_late_identity). Reference points from the
# 2026-10-28 grid on the project build host: 10k x 50 standard
# 5.37s, 1k x 10 standard 0.09s. These deliberately generous limits
# absorb shared-CI variance and catch only large runtime regressions.
CASES = [
    (10_000, 50, "standard", 180.0, False),
    (1_000, 10, "standard", 60.0, False),
    # Keep a known relationship in the final two slots of a table wider
    # than 64 columns. This cheaply checks that bounded searches reach the
    # end of a wide frame as well as checking the timing path.
    (1_000, 80, "standard", 180.0, True),
]


def synthetic(rows: int, cols: int, late_identity: bool = False) -> pl.DataFrame:
    rng = random.Random(20261028)
    data = {}
    relation_cols = 2 if late_identity else 0
    n_cat = max(1, (cols - relation_cols) // 4)
    for i in range(cols - relation_cols - n_cat):
        base = [rng.gauss(0, 1) for _ in range(min(rows, 500))]
        data[f"num_{i}"] = [base[j % len(base)] + (j % 7) * 0.01 for j in range(rows)]
    for i in range(n_cat):
        levels = [f"L{k}" for k in range(5)]
        data[f"cat_{i}"] = [levels[rng.randrange(5)] for _ in range(rows)]
    if late_identity:
        x = [((i * 37) % 1_000) / 1_000.0 for i in range(rows)]
        data["late_x"] = x
        data["late_affine"] = [3.0 * value + 2.0 for value in x]
    return pl.DataFrame(data)


def timed_passes(coverage):
    """Return valid timed passes and names with missing/invalid timings."""
    valid = []
    invalid = []
    for name, info in coverage.items():
        if not isinstance(info, dict) or not isinstance(info.get("status"), str):
            continue
        if not info["status"].startswith("ran"):
            continue
        elapsed = info.get("elapsed_ms")
        if (
            isinstance(elapsed, bool)
            or not isinstance(elapsed, (int, float))
            or not math.isfinite(elapsed)
            or elapsed < 0
        ):
            invalid.append(name)
            continue
        valid.append((name, elapsed))
    return sorted(valid, key=lambda item: -item[1]), invalid


def late_identity_recovered(report) -> bool:
    return any(
        finding.get("category") == "dependencies"
        and finding.get("specification", {}).get("form") == "affine"
        and set(finding.get("subject", {}).get("columns", []))
        == {"late_x", "late_affine"}
        for finding in report.findings
    )


def main() -> int:
    failed = False
    for rows, cols, budget, threshold, check_late_identity in CASES:
        df = synthetic(rows, cols, late_identity=check_late_identity)
        if df.shape != (rows, cols):
            print(f"FAIL: fixture shape is {df.shape}, expected {(rows, cols)}")
            failed = True
            continue
        t = time.perf_counter()
        report = onceover.profile(df, budget=budget, timing=True)
        wall = time.perf_counter() - t
        cov = report.data["coverage"]
        top, invalid = timed_passes(cov)
        print(
            f"{rows} x {cols} {budget}: {wall:.2f}s (threshold {threshold:.0f}s), "
            f"{len(report.findings)} findings"
        )
        for name, elapsed_ms in top[:5]:
            print(f"    {name:<28} {elapsed_ms:>10.1f} ms")
        if invalid:
            print(f"FAIL: passes with missing or invalid elapsed_ms: {invalid}")
            failed = True
        if wall > threshold:
            print(f"FAIL: {wall:.2f}s exceeds the {threshold:.0f}s gate")
            failed = True
        if check_late_identity:
            if len(report.columns) != cols:
                print(f"FAIL: expected {cols} column cards, found {len(report.columns)}")
                failed = True
            if not late_identity_recovered(report):
                print("FAIL: affine identity in final columns was not recovered")
                failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
