"""Reproducible scale and recovery check; run in a fresh process.

Install onceover[benchmark], then run from the repository root:
    python benchmarks/scale.py --rows 2000000 --columns 200 --output results.json
"""

import argparse
import json
import os
import platform
from pathlib import Path
import threading
import time

import numpy as np
import polars as pl
import psutil

import onceover


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows", type=int, default=2_000_000)
    parser.add_argument("--columns", type=int, default=200)
    parser.add_argument("--budget", choices=["quick", "standard", "deep"], default="standard")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--threads", type=int, default=4)
    args = parser.parse_args()
    if args.rows < 300 or args.columns < 12:
        parser.error("use at least 300 rows and 12 columns")

    if args.threads < 1:
        parser.error("threads must be positive")
    os.environ["RAYON_NUM_THREADS"] = str(args.threads)
    rng = np.random.default_rng(20241024)
    n = args.rows
    data = {"id": np.arange(n, dtype=np.int64)}
    data.update({f"noise_{i:03}": rng.random(n) for i in range(args.columns - 7)})
    x = rng.uniform(-1, 1, n)
    codes = rng.integers(0, 4, n, dtype=np.int64)
    dirty = np.arange(n, dtype=np.int64)
    dirty[-1] = 0
    data.update({"late_x": x, "late_affine": 3 * x + 2, "late_curve": x * x,
                 "late_code": codes, "dirty_id": dirty})
    df = pl.DataFrame(data).with_columns(pl.col("late_code").cast(pl.String).alias("late_label"))
    assert df.shape == (n, args.columns)

    process = psutil.Process()
    before = process.memory_info().rss
    peak = [before]
    stop = threading.Event()

    def sample_memory():
        while not stop.wait(0.02):
            peak[0] = max(peak[0], process.memory_info().rss)

    monitor = threading.Thread(target=sample_memory, daemon=True)
    monitor.start()
    started = time.perf_counter()
    try:
        report = onceover.profile(df, budget=args.budget)
    finally:
        elapsed = time.perf_counter() - started
        peak[0] = max(peak[0], process.memory_info().rss)
        stop.set()
        monitor.join()

    findings = report.findings
    checks = {
        "all_column_cards": len(report.columns) == args.columns,
        "exact_unique_count": report.columns["id"]["distinct"] == n,
        "exact_dirty_count": report.columns["dirty_id"]["distinct"] == n - 1,
        "primary_key": any(f["category"]=="uniqueness" and f["subject"]["columns"]==["id"]
                           for f in findings),
        "dirty_key": any(f["category"]=="uniqueness" and f["subject"]["columns"]==["dirty_id"]
                         and any(e["metrics"].get("duplicates")==1 for e in f["evidence"]) for f in findings),
    }
    if args.budget != "quick":
        checks.update({
            "late_nonlinear_target": any(f["category"]=="dependencies" and f.get("type")=="predictive"
                                         and f["subject"].get("target")=="late_curve" for f in findings),
            "late_identity": any(f["category"]=="dependencies" and f["specification"].get("form")=="affine"
                                 and set(f["subject"]["columns"])=={"late_x","late_affine"}
                                 for f in findings),
            "late_dependency": any(f["category"]=="dependencies" and f.get("type")=="functional"
                                   and f["subject"].get("inputs")==["late_code"]
                                   and f["subject"].get("target")=="late_label" for f in findings),
        })
    result = {
        "rows": n, "columns": args.columns, "budget": args.budget, "seed": 20241024,
        "platform": platform.platform(), "python": platform.python_version(),
        "onceover": onceover.__version__, "polars": pl.__version__,
        "rayon_threads": args.threads, "system_memory_gib": psutil.virtual_memory().total / 2**30,
        "profile_seconds": elapsed,
        "memory_sample_interval_seconds": 0.02,
        "rss_before_profile_gib": before / 2**30,
        "peak_profile_rss_gib": peak[0] / 2**30,
        "additional_peak_rss_gib": (peak[0] - before) / 2**30,
        "checks": checks, "coverage": report.coverage, "schema_version":report.data["schema_version"], "selection":report.data["selection"],
        "noise_targets_flagged": sum(f["category"]=="dependencies" and f.get("type")=="predictive"
                                     and f["subject"].get("target", "").startswith("noise_") for f in findings),
    }
    rendered = json.dumps(result, indent=2)
    print(rendered, flush=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered + "\n")
    if not all(checks.values()):
        raise SystemExit("Planted structure was missed; see checks above")


if __name__ == "__main__":
    main()
