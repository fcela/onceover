"""Statement truth: `report.verify(df)` recounts the report's exact
statements through Polars. It returns one row per checked quantity with
a `status`: "ok" (the recount agrees), "mismatch" (it disagrees),
"not_checkable" (a sampled or model-based statement with nothing exact
to recount), or "error". Any mismatch or error is a regression: the
metrics are marked `zero`, so the comparison fails whenever the head
build reports one, whatever the base build reported.

Measured on the dataset's default profile and, when a target is
declared, on its target profile.
"""
from __future__ import annotations

NAME = "truth"
DESCRIPTION = "Counts of ok / mismatch / not_checkable / error from report.verify on the profiled table."

METRICS = {
    "mismatch": {"direction": "lower", "kind": "count", "zero": True},
    "error": {"direction": "lower", "kind": "count", "zero": True},
    "ok": {"direction": "info", "kind": "count"},
    "not_checkable": {"direction": "info", "kind": "count"},
}


def tally(prof, df) -> dict:
    res = prof.report.verify(df)
    counts = {k: 0 for k in METRICS}
    for status in res["status"].to_list():
        if status not in counts:
            raise ValueError(f"unknown verify status {status!r}")
        counts[status] += 1
    return counts


def run(ctx) -> dict:
    out = {k: 0 for k in METRICS}
    detail = {}
    runs = [("default", ctx.profile())]
    if ctx.target:
        runs.append(("target", ctx.profile(target=ctx.target)))
    for mode, prof in runs:
        c = tally(prof, ctx.df)
        for k, v in c.items():
            out[k] += v
        detail[mode] = c
    return {"metrics": out, "detail": detail}
