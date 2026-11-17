"""Weighted: the hinted run (issue #329).

A dataset whose registry entry declares `weights` is profiled
once more with those weights in the hints — a separate cached
profile; the unhinted profiles the other components use are
untouched. In the hinted run:

- reference statements carrying a `hinted_tier` are scored at
  the 4k digest, 8k digest and full report scopes, counted
  under must/should exactly as the reference component counts
  its tiers (in the unhinted runs the same statements keep
  their `tier` and their recorded misses);
- controls declared in the entry as `weighted_controls` must
  stay silent: a control is satisfied when no weighted finding
  (a hotspot finding whose evidence names a weight) fits the
  control's columns and kind in the hinted full report. A
  satisfied control counts as a must match at every scope; a
  firing control is a must miss.
"""

from components import reference as ref

NAME = "weighted"
DESCRIPTION = (
    "Hinted run with declared weights: hinted-tier reference statements "
    "found in the 4k digest, the 8k digest and the full report of the "
    "weighted profile; declared controls silent under their weight."
)

SCOPES = [("4k", 4000), ("8k", 8000), ("full", None)]
TIERS = ["must", "should"]

METRICS = {}
for _tier in TIERS:
    METRICS[f"{_tier}_total"] = {"direction": "info", "kind": "count"}
    for _scope, _ in SCOPES:
        METRICS[f"{_tier}_{_scope}"] = {"direction": "higher", "kind": "count"}
METRICS["weighted_findings"] = {"direction": "info", "kind": "count"}


def is_weighted(f: dict) -> bool:
    """A weighted hotspot finding: its first evidence block
    names the weight it stands on."""
    for ev in f.get("evidence") or []:
        return "weight" in (ev.get("metrics") or {})
    return False


def run(ctx) -> dict:
    weights = ctx.entry.get("weights")
    if not weights:
        return {"status": "not_applicable", "reason": "no weights declared for this dataset"}
    hints = dict(ctx.hints or {})
    hints["weights"] = list(weights)
    prof = ctx.profile(df=ctx.df, hints=hints, key="weighted")
    out = {k: 0 for k in METRICS}
    detail = {}
    pool = prof.findings + (prof.data.get("grouped_findings") or [])
    out["weighted_findings"] = sum(1 for f in prof.findings if is_weighted(f))
    statements = [
        s for s in (ref.load_statements(ctx.id) or []) if s.get("hinted_tier")
    ]
    for st in statements:
        tier = st["hinted_tier"]
        rec = {"tier": tier, "matched": {}}
        out[f"{tier}_total"] += 1
        for scope, chars in SCOPES:
            via = (
                ref.matched_in_full(prof, st)
                if chars is None
                else ref.matched_in_digest(prof, chars, st)
            )
            rec["matched"][scope] = via
            if via:
                out[f"{tier}_{scope}"] += 1
        detail[st["id"]] = rec
    for i, control in enumerate(ctx.entry.get("weighted_controls") or []):
        st = {"columns": control["columns"], "kind": control.get("kind")}
        fired = any(is_weighted(f) and ref.finding_fits(f, st) for f in pool)
        rec = {"tier": "must", "control": True, "matched": {}}
        out["must_total"] += 1
        if fired:
            rec["matched"]["full"] = "fired"
        else:
            for scope, _ in SCOPES:
                out[f"must_{scope}"] += 1
                rec["matched"][scope] = "silent"
        detail[control.get("id", f"control-{i}")] = rec
    return {"metrics": out, "detail": detail}
