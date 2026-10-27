"""Reproduce the views documentation's sensor and negative-control checks.

Run against the installed repository package:
    python benchmarks/views.py --output /tmp/onceover-views.json
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import time


def sample_rows(n, cap):
    """Match onceover-core's deterministic stratified row sampler."""
    if cap <= 0:
        return []
    if n <= cap:
        return list(range(n))
    mask = (1 << 64) - 1
    rows = []
    for j in range(cap):
        start, end = j * n // cap, (j + 1) * n // cap
        value = (j + 0x9E3779B97F4A7C15) & mask
        value = ((value ^ (value >> 30)) * 0xBF58476D1CE4E5B9) & mask
        value = ((value ^ (value >> 27)) * 0x94D049BB133111EB) & mask
        rows.append(start + (value ^ (value >> 31)) % (end - start))
    return rows


def sensor_fixture(rows, seed):
    import numpy as np
    import polars as pl

    rng = np.random.default_rng(seed)
    position = np.arange(rows)
    half = rows // 2
    base = np.where(position < half, 20.0, 32.0)
    drift = np.maximum(position - half, 0) * 3.0 / (rows - half)
    temperature = base + drift + rng.normal(0, np.where(position < half, 0.8, 1.3))
    pressure = np.where(position < half, 1010.0, 990.0) + rng.normal(0, 5, rows)
    humidity = rng.normal(50, 10, rows)
    stuck_start, stuck_end = rows // 5, rows // 5 + min(600, rows // 20)
    temperature[stuck_start:stuck_end] = 21.5
    available = np.concatenate((position[:stuck_start], position[stuck_end:]))
    spikes = np.sort(rng.choice(available, 15, replace=False))
    temperature[spikes] = 999.9
    frame = pl.DataFrame({
        "temperature": temperature,
        "pressure": pressure,
        "humidity": humidity,
        "device_id": np.where(position < 2 * rows // 5, "sensor_a", "sensor_b"),
        "timestamp": 1_700_000_000_000_000 + position * 60_000_000,
    }).with_columns(pl.col("timestamp").cast(pl.Datetime("us")))
    return frame, {
        "id": "sensor-v1", "seed": seed, "rows": rows,
        "shift_row": half, "stuck_start": stuck_start, "stuck_end_exclusive": stuck_end,
        "spike_rows": spikes.tolist(), "device_change_row": 2 * rows // 5,
    }


def metrics_for(finding):
    return [e["metrics"] for e in finding["evidence"] if e["method"]["detector"] == "views"]


def summarize(report, truth, evaluated_rows):
    expected = set(truth["spike_rows"]) & set(evaluated_rows)
    flagged = set()
    contrasts = []
    associations = []
    for finding in report.findings:
        if finding["category"] == "associations":
            associations.append({"columns": finding["subject"]["columns"],
                                 "evidence": metrics_for(finding)})
        if finding.get("type") == "spike_cells" and finding["subject"]["columns"] == ["temperature"]:
            for metrics in metrics_for(finding):
                flagged.update(cell["row"] for cell in metrics.get("cells", []))
        if finding["category"] == "changes" and "temperature" in finding["subject"]["columns"]:
            definition = finding["specification"]["definition"]
            contrasts.append({"columns": finding["subject"]["columns"], "definition": definition})
    # This is a row-range diagnostic, not a change-point estimator. It is
    # defined before evaluating the fixture and may fail on overlapping groups.
    boundary = None
    for contrast in contrasts:
        definition = contrast["definition"]
        if definition.get("headline_column") != "temperature":
            continue
        clusters = sorted((c for c in definition.get("clusters", []) if "mean" in c),
                          key=lambda c: c["mean"])
        if len(clusters) >= 2:
            split = max(range(1, len(clusters)),
                        key=lambda i: clusters[i]["mean"] - clusters[i - 1]["mean"])
            boundary = max(c["rows_max"] for c in clusters[:split])
            break
    return {
        "sample_rows": len(evaluated_rows), "sampled_spike_rows": sorted(expected),
        "flagged_temperature_rows": sorted(flagged),
        "recovered_spike_rows": sorted(expected & flagged),
        "sampled_spikes": len(expected), "recovered_spikes": len(expected & flagged),
        "reported_temperature_cells": len(flagged),
        "reported_unplanted_temperature_cells": len(flagged - expected),
        "spike_recall": len(expected & flagged) / len(expected) if expected else None,
        "candidate_boundary_from_cluster_ranges": boundary,
        "candidate_boundary_error_rows": abs(boundary - truth["shift_row"]) if boundary is not None else None,
        "temperature_cluster_contrasts": contrasts, "associations": associations,
        "coverage": report.coverage["views"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows", type=int, default=50_000)
    parser.add_argument("--seed", type=int, default=20261027)
    parser.add_argument("--threads", type=int, default=4)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--report-dir", type=Path)
    args = parser.parse_args()
    if args.rows < 2000 or args.threads < 1:
        parser.error("use at least 2,000 rows and one thread")
    os.environ["RAYON_NUM_THREADS"] = str(args.threads)
    os.environ["POLARS_MAX_THREADS"] = str(args.threads)

    import numpy as np
    import polars as pl
    import onceover

    frame, truth = sensor_fixture(args.rows, args.seed)
    results = {}
    if args.report_dir:
        args.report_dir.mkdir(parents=True, exist_ok=True)
    for name, cap in [("standard", 20_000), ("full_sample", args.rows)]:
        started = time.perf_counter()
        report = onceover.profile(frame, detectors=["views"], sample_rows=cap)
        elapsed = time.perf_counter() - started
        rendered = report.to_json()
        repeated = onceover.profile(frame, detectors=["views"], sample_rows=cap).to_json()
        results[name] = summarize(report, truth, sample_rows(args.rows, cap))
        results[name].update(profile_seconds=elapsed, deterministic=rendered == repeated,
                             report_sha256=hashlib.sha256(rendered.encode()).hexdigest())
        if args.report_dir:
            (args.report_dir / f"sensor-{name}.json").write_text(rendered + "\n")

    noise = pl.DataFrame({"noise": np.random.default_rng(args.seed).normal(size=2000)})
    control = onceover.profile(noise, detectors=["views"])
    controls = {
        "stationary_noise_rows": noise.height,
        "stationary_noise_cluster_findings": [f for f in control.findings if f["category"] == "changes"],
    }
    # A small input avoids rerunning all default detectors on the sensor table.
    controls["views_disabled_by_default"] = (
        onceover.profile(noise).coverage["views"]["detail"] == "detector not enabled")
    checks = {
        "standard_repeat_identical": results["standard"]["deterministic"],
        "full_sample_repeat_identical": results["full_sample"]["deterministic"],
        "views_disabled_by_default": controls["views_disabled_by_default"],
        "standard_reported_cells_use_evaluated_rows": set(results["standard"]["flagged_temperature_rows"]).issubset(set(sample_rows(args.rows, 20_000))),
        "full_sample_covers_all_planted_spikes": results["full_sample"]["sampled_spikes"] == 15,
    }
    payload = {
        "fixture": truth,
        "benchmark_source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "input_rows_sha256": hashlib.sha256(frame.hash_rows(seed=0).to_numpy().tobytes()).hexdigest(),
        "environment": {"platform": platform.platform(), "python": platform.python_version(),
                        "onceover": onceover.__version__, "numpy": np.__version__,
                        "polars": pl.__version__, "threads": args.threads,
                        "package": onceover.__file__},
        "results": results, "controls": controls, "checks": checks,
        "scope": "Views-only sensor benchmark; cluster-range boundary is diagnostic, spike threshold is uncalibrated.",
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, indent=2) + "\n")
    print(json.dumps({"output": str(args.output), "checks": checks,
                      "runs": {name: {k: result[k] for k in
                                ["profile_seconds", "sampled_spikes", "recovered_spikes",
                                 "reported_unplanted_temperature_cells",
                                 "candidate_boundary_from_cluster_ranges"]}
                               for name, result in results.items()}}, indent=2))
    if not all(checks.values()):
        raise SystemExit("Views validation checks failed; inspect output")


if __name__ == "__main__":
    main()
