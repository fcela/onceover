"""Planted recovery: inject one known structure into a real table,
profile, and check whether the 4,000-character digest states it, on
which line, and whether the full report has it.

The plant catalogue is gate_plants.PLANTS. A plant's defining property
is verified on the generated frame (Polars/NumPy) before profiling.
"""
from __future__ import annotations

import re
import zlib

import numpy as np

import gatelib as g
from gate_plants import PLANTS

import battery  # benchmarks/quality/battery.py: finding matching

NAME = "planted"
DESCRIPTION = ("Known structures injected into real tables: stated in the "
               "4k digest, and present in the full report.")
DIGEST_CHARS = 4000

METRICS = {
    "planted": {"direction": "info", "kind": "count"},
    "found_full": {"direction": "higher", "kind": "count"},
    "found_4k": {"direction": "higher", "kind": "count"},
    "line_sum_4k": {"direction": "info", "kind": "count"},
}

# A matching finding may name this many columns beyond the planted ones
# (a model-based finding that lists every input column is not a
# statement about the planted structure).
MAX_EXTRA_COLUMNS = 2


def finding_matches(f: dict, spec: dict) -> bool:
    """battery.matches (type in the plant's types, planted columns among
    the subject columns), tightened to the columns the finding states
    (gatelib.finding_columns) and bounded in width."""
    if not battery.matches(f, spec):
        return False
    cols = g.finding_columns(f)
    if not spec["columns"] <= cols or len(cols) > len(spec["columns"]) + MAX_EXTRA_COLUMNS:
        return False
    if spec.get("target") and (f.get("subject") or {}).get("target") != spec["target"]:
        return False
    return True


def run(ctx) -> dict:
    names = ctx.option(NAME, "plants") or list(PLANTS)
    out = {"planted": 0, "found_full": 0, "found_4k": 0, "line_sum_4k": 0}
    detail = {}
    for name in names:
        builder = PLANTS[name]
        for seed in ctx.seeds:
            rng = np.random.default_rng([seed, zlib.crc32(name.encode())])
            built = builder(ctx.df, rng)
            key = f"{name}/seed{seed}"
            if built is None:
                detail[key] = {"status": "not_applicable"}
                continue
            frame, spec = built
            if not spec["verify"](frame):
                raise AssertionError(f"plant {name} (seed {seed}) failed its own verification")
            prof = ctx.profile(frame, target=spec.get("target"))
            pool = prof.findings + (prof.data.get("grouped_findings") or [])
            in_full = any(finding_matches(f, spec) for f in pool)
            line_no, via = None, None
            for i, line in enumerate(prof.lines(DIGEST_CHARS)):
                if not line.is_finding:
                    continue
                if any(finding_matches(f, spec) for f in line.members):
                    line_no, via = i, "finding"
                    break
            if line_no is None and spec.get("patterns"):
                for i, line in enumerate(prof.lines(DIGEST_CHARS)):
                    if line.text.startswith(g.NON_STATEMENT_PREFIXES) or not line.text:
                        continue
                    if all(re.search(p, line.text) for p in spec["patterns"]):
                        line_no, via = i, "text"
                        break
            out["planted"] += 1
            out["found_full"] += int(in_full)
            out["found_4k"] += int(line_no is not None)
            out["line_sum_4k"] += line_no or 0
            detail[key] = {"columns": sorted(spec["columns"]), "verified": True,
                           "in_full_report": in_full, "digest_line": line_no, "via": via}
    return {"metrics": out, "detail": detail}
