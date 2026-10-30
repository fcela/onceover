#!/usr/bin/env python3
"""CI timing gate (issue #26): the fast tier of the runtime grid.

Profiles two synthetic tables with per-pass timing on and checks
wall time against generous thresholds. The thresholds are not
performance targets — they exist to catch order-of-magnitude
regressions (a pass going quadratic, a cap ceasing to bind) before
they ship. Full grid: benchmarks/corpus/runtime_grid.py; results
and hotspot dispositions: docs/benchmarks/.

Exit code 0 = within thresholds, 1 = regression or broken timing.
"""
from __future__ import annotations

import random
import sys
import time

import polars as pl

import onceover

# (rows, cols, budget, threshold_seconds). Reference points from the
# 2026-10-28 grid on the project build host: 10k x 50 standard
# 5.37s, 1k x 10 standard 0.09s. Thresholds sit ~30x above the
# reference so shared CI hardware passes comfortably while a 10x
# regression trips.
CASES = [
    (10_000, 50, "standard", 180.0),
    (1_000, 10, "standard", 60.0),
]


def synthetic(rows: int, cols: int) -> pl.DataFrame:
    rng = random.Random(20261028)
    data = {}
    n_cat = max(1, cols // 4)
    for i in range(cols - n_cat):
        base = [rng.gauss(0, 1) for _ in range(min(rows, 500))]
        data[f"num_{i}"] = [base[j % len(base)] + (j % 7) * 0.01 for j in range(rows)]
    for i in range(n_cat):
        levels = [f"L{k}" for k in range(5)]
        data[f"cat_{i}"] = [levels[rng.randrange(5)] for _ in range(rows)]
    return pl.DataFrame(data)


def main() -> int:
    failed = False
    for rows, cols, budget, threshold in CASES:
        df = synthetic(rows, cols)
        t = time.time()
        report = onceover.profile(df, budget=budget, timing=True)
        wall = time.time() - t
        cov = report.data["coverage"]
        ran = {k: v for k, v in cov.items() if v["status"].startswith("ran")}
        missing = [k for k, v in ran.items() if "elapsed_ms" not in v]
        top = sorted(ran.items(), key=lambda kv: -kv[1]["elapsed_ms"])[:5]
        print(
            f"{rows} x {cols} {budget}: {wall:.2f}s (threshold {threshold:.0f}s), "
            f"{len(report.findings)} findings"
        )
        for k, v in top:
            print(f"    {k:<28} {v['elapsed_ms']:>10.1f} ms")
        if missing:
            print(f"FAIL: passes without elapsed_ms: {missing}")
            failed = True
        if wall > threshold:
            print(f"FAIL: {wall:.2f}s exceeds the {threshold:.0f}s gate")
            failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
