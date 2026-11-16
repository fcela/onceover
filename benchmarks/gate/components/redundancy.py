"""Redundancy and budget of the 4,000-character digest.

    repeated        finding lines whose column set and relation kind
                    (category, type) repeat an earlier finding line.
                    A line's column set is that of the finding it
                    stands for (the lead finding of a rolled-up group).
    finding_lines   number of lines the digest parser resolves to
                    findings (one per statement or merged fact).
    finding_permille  characters on finding lines per thousand
                    characters of the digest (header, account,
                    coverage and omission lines hold the rest).
    long_decimals   numbers printed with six or more decimals.

Measured in default mode and, when the dataset declares a target, in
target mode.
"""
from __future__ import annotations

import gatelib as g

NAME = "redundancy"
DESCRIPTION = "Repeated facts, finding lines, share of characters on findings, and over-long numbers in the 4k digest."
DIGEST_CHARS = 4000

METRICS = {}
for _mode in ("default", "target"):
    METRICS[f"repeated_{_mode}"] = {"direction": "lower", "kind": "count"}
    METRICS[f"finding_lines_{_mode}"] = {"direction": "info", "kind": "count"}
    METRICS[f"finding_permille_{_mode}"] = {"direction": "higher", "kind": "count"}
    METRICS[f"long_decimals_{_mode}"] = {"direction": "lower", "kind": "count"}


def measure(prof) -> dict:
    text = prof.digest(DIGEST_CHARS)
    lines = prof.lines(DIGEST_CHARS)
    seen, repeated, finding_lines, finding_chars = set(), 0, 0, 0
    for l in lines:
        if not l.is_finding:
            continue
        finding_lines += 1
        finding_chars += len(l.text) + 1
        if l.lead is None:
            continue
        key = (frozenset(l.columns), l.kind)
        if key in seen:
            repeated += 1
        seen.add(key)
    return {"repeated": repeated, "finding_lines": finding_lines,
            "finding_permille": (1000 * finding_chars) // max(len(text), 1),
            "long_decimals": g.long_decimals(text)}


def run(ctx) -> dict:
    out = {k: 0 for k in METRICS}
    detail = {}
    modes = [("default", ctx.profile())]
    if ctx.target:
        modes.append(("target", ctx.profile(target=ctx.target)))
    for mode, prof in modes:
        m = measure(prof)
        for k, v in m.items():
            out[f"{k}_{mode}"] = v
        detail[mode] = m
    return {"metrics": out, "detail": detail}
