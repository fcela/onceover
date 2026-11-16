"""Reference statements: hand-written facts about a dataset, scored
against the digest and the full report.

The statement file for a dataset is benchmarks/gate/reference/<gate
dataset id>.json (format in benchmarks/gate/README.md). A statement is
matched in a scope (the 4,000-character digest, the 8,000-character
digest, or the full report) in one of two ways:

    structural  a digest line stands for a finding (looked up in the
                report by exemplar id or claim text), or a full-report
                finding, whose subject columns include every column in
                "columns", include at least one of "columns_any" (when
                given), and whose category or type fits "kind" (when
                given);
    text        when no finding matches, every regular expression in
                "patterns" matches one statement line of the digest, or
                one claim of the full report.

Each statement is scored in default mode and, when the dataset declares a
target, in target mode. A statement may also carry "check", a Polars
expression that must evaluate to True on the data. A check that does not
hold means the statement itself is wrong; those are counted in
`check_failed`, which must be zero.
"""
from __future__ import annotations

import json
import re

import polars as pl

import gatelib as g

NAME = "reference"
DESCRIPTION = "Hand-written reference statements found in the 4k digest, the 8k digest and the full report."

SCOPES = [("4k", 4000), ("8k", 8000), ("full", None)]
TIERS = ["must", "should"]
MODES = ["default", "target"]

METRICS = {"check_failed": {"direction": "lower", "kind": "count", "zero": True}}
for _tier in TIERS:
    METRICS[f"{_tier}_total"] = {"direction": "info", "kind": "count"}
    for _mode in MODES:
        for _scope, _ in SCOPES:
            METRICS[f"{_tier}_{_scope}_{_mode}"] = {"direction": "higher", "kind": "count"}

# "kind" in a statement selects findings by category or type.
KINDS = {
    "relates": {"categories": {"dependencies", "associations", "conditional_rules", "changes"}},
    "encoding": {"types": {"duplicate_encoding", "functional"}},
    "identity": {"types": {"arithmetic_identity", "affine", "functional"}},
    "order": {"types": {"order_constraint", "order_violation"}},
    "sentinel": {"types": {"sentinel_values", "boundary_mass"}},
    "missingness": {"categories": {"missingness"}},
    "outlier": {"types": {"extreme_value", "categorical_outlier", "transient_excursion"}},
    "concentration": {"categories": {"concentration"}},
    "role": {"categories": {"column_roles", "uniqueness"}},
}


def kind_fits(f: dict, kind: str | None) -> bool:
    if not kind:
        return True
    spec = KINDS[kind]
    return f.get("category") in spec.get("categories", ()) or f.get("type") in spec.get("types", ())


def finding_fits(f: dict, st: dict) -> bool:
    cols = g.finding_columns(f)
    if not set(st.get("columns") or []) <= cols:
        return False
    any_cols = st.get("columns_any") or []
    if any_cols and not (set(any_cols) & cols):
        return False
    if st.get("types") and f.get("type") not in st["types"]:
        return False
    return kind_fits(f, st.get("kind"))


def text_fits(texts, patterns) -> bool:
    return bool(patterns) and any(all(re.search(p, t, re.I) for p in patterns) for t in texts)


def matched_in_digest(prof, chars: int, st: dict):
    lines = prof.lines(chars)
    for l in lines:
        if l.is_finding and any(finding_fits(f, st) for f in l.members):
            return "finding"
    if text_fits(g.statement_lines(lines), st.get("patterns")):
        return "text"
    return None


def matched_in_full(prof, st: dict):
    pool = prof.findings + (prof.data.get("grouped_findings") or [])
    if any(finding_fits(f, st) for f in pool):
        return "finding"
    if text_fits([f.get("claim") or "" for f in pool], st.get("patterns")):
        return "text"
    return None


def load_statements(dataset_id: str):
    path = g.GATE / "reference" / f"{dataset_id}.json"
    if not path.exists():
        return None
    doc = json.loads(path.read_text(encoding="utf-8"))
    return doc["statements"]


def check_statement(df, st: dict):
    expr = st.get("check")
    if not expr:
        return None
    value = df.select(eval(expr, {"pl": pl}).alias("v")).item()  # noqa: S307  (repository-controlled file)
    return bool(value)


def run(ctx) -> dict:
    statements = load_statements(ctx.id)
    if statements is None:
        return {"status": "not_applicable", "reason": "no reference statements for this dataset"}
    modes = ["default"] + (["target"] if ctx.target else [])
    profs = {"default": ctx.profile()}
    if ctx.target:
        profs["target"] = ctx.profile(target=ctx.target)
    out = {k: 0 for k in METRICS}
    detail = {}
    for st in statements:
        tier = st["tier"]
        rec = {"tier": tier, "provisional": bool(st.get("provisional")), "matched": {}}
        checked = check_statement(ctx.df, st)
        if checked is False:
            out["check_failed"] += 1
        rec["check"] = checked
        out[f"{tier}_total"] += 1
        for mode in modes:
            if mode not in (st.get("modes") or MODES):
                continue
            for scope, chars in SCOPES:
                via = (matched_in_full(profs[mode], st) if chars is None
                       else matched_in_digest(profs[mode], chars, st))
                rec["matched"][f"{mode}/{scope}"] = via
                if via:
                    out[f"{tier}_{scope}_{mode}"] += 1
        detail[st["id"]] = rec
    return {"metrics": out, "detail": detail}
