#!/usr/bin/env python3
"""Runtime program (issue #26): scaling grid + per-detector attribution.

Two measurements:
1. Grid: synthetic tables over rows x columns at quick/standard/deep
   budgets (deterministic seed; mixed numeric/categorical content with
   planted structure so detectors have work to do).
2. Attribution: on reference corpus tables, time each detector alone
   (profile with detectors=[id]) and the full default set, so the
   expensive passes are named with numbers.

Writes benchmarks/corpus/runs/runtime-<UTC>.json and prints a table.
Synthetic only for the grid; reference tables come from the registry.
"""
from __future__ import annotations

import json
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

import polars as pl

BENCH = Path(__file__).parent
ROOT = BENCH.parent.parent
sys.path.insert(0, str(ROOT / "python"))
import onceover  # noqa: E402


def synthetic(rows: int, cols: int) -> pl.DataFrame:
    import random
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


def timed(df, **kw):
    t = time.time()
    r = onceover.profile(df, **kw)
    return round(time.time() - t, 2), len(r.data["findings"])


def main() -> int:
    out = {"started": datetime.now(timezone.utc).isoformat(), "grid": [], "attribution": []}
    for rows in (1_000, 10_000, 100_000):
        for cols in (10, 50, 100):
            df = synthetic(rows, cols)
            for budget in ("quick", "standard"):
                s, n = timed(df, budget=budget)
                out["grid"].append({"rows": rows, "cols": cols, "budget": budget,
                                    "seconds": s, "findings": n})
                print(f"grid {rows:>7} x {cols:>3} {budget:<9} {s:>7.2f}s  {n} findings")
    grid_deep, _ = timed(synthetic(10_000, 50), budget="deep")
    out["grid"].append({"rows": 10_000, "cols": 50, "budget": "deep",
                        "seconds": grid_deep, "findings": _})
    print(f"grid   10000 x  50 deep      {grid_deep:>7.2f}s")

    detectors = [d["id"] for d in onceover.available_detectors()]
    for ref in ("data/gritbot/thyroid/data.csv", "data/crosscat/congressional-voting/data.csv"):
        df = pl.read_csv(BENCH / ref, infer_schema_length=None)
        full_s, _ = timed(df)
        per = {}
        for det in detectors:
            s, _ = timed(df, detectors=[det])
            per[det] = s
        out["attribution"].append({"table": ref, "full_default_seconds": full_s,
                                   "per_detector_seconds": per})
        top = sorted(per.items(), key=lambda kv: -kv[1])[:4]
        print(f"attribution {ref}: full {full_s:.2f}s | top: "
              + ", ".join(f"{k} {v:.2f}s" for k, v in top))
    path = BENCH / "runs" / ("runtime-" + datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ") + ".json")
    path.write_text(json.dumps(out, indent=1))
    print("run record:", path)
    return 0


if __name__ == "__main__":
    sys.exit(main())
