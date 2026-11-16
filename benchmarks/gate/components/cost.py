"""Cost: median wall time of three default profiles, size of the JSON
report, and the number of findings. Time is compared on totals with a
relative tolerance; size and count are shown but do not fail the
comparison."""
from __future__ import annotations

import statistics

import onceover

NAME = "cost"
DESCRIPTION = "Median profile time over three runs, JSON size, number of findings."
RUNS = 3

METRICS = {
    "profile_seconds": {"direction": "lower", "kind": "time"},
    "json_chars": {"direction": "info", "kind": "count"},
    "findings": {"direction": "info", "kind": "count"},
}


def run(ctx) -> dict:
    first = ctx.profile()
    times = [first.seconds]
    kw = {"budget": ctx.budget}
    if ctx.detectors:
        kw["detectors"] = ctx.detectors
    if ctx.hints:
        kw["hints"] = ctx.hints
    import time
    for _ in range(RUNS - 1):
        t0 = time.perf_counter()
        onceover.profile(ctx.df, **kw)
        times.append(time.perf_counter() - t0)
    med = statistics.median(times)
    metrics = {"profile_seconds": round(med, 3),
               "json_chars": len(first.report.to_json()),
               "findings": len(first.findings)}
    return {"metrics": metrics, "detail": {"runs_seconds": [round(t, 3) for t in times]}}
