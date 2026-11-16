#!/usr/bin/env python3
"""Compare two gate scorecards and print a markdown report.

    python benchmarks/gate/compare.py base.json head.json [--out report.md]

Exit status 0 when no metric is worse, 1 when any metric is worse
beyond its tolerance, 2 on unreadable input.

Rules:
  - Count metrics have zero tolerance: the runs are seeded and
    deterministic, so any change in the worse direction fails.
  - Time metrics are compared on totals. A total may grow by
    max(--time-rel x base, --time-floor seconds) before it counts as
    worse; the floor keeps sub-second noise from failing.
  - "info" metrics are shown and never fail.
  - A metric marked "zero" (for example statement mismatches) fails
    whenever the head value is above zero, whatever the base holds.
  - Per-dataset values are compared as well as totals, so a gain on
    one dataset cannot hide a loss on another.
  - A dataset or component that ran in the base and failed or was
    skipped in the head counts as worse. Datasets and components
    present only in the head are listed and not compared.
  - Held-out datasets are listed in their own section and excluded
    from the verdict unless --include-held-out is given.
  - A worse metric listed in the accepted file (default:
    accepted.json next to this script) does not fail the comparison
    while the head value stays within the entry's limit. An entry
    names a metric, a scope ("total" or a dataset id), a limit and a
    reason. An accepted total also accepts that metric's per-dataset
    rows. Entries that accepted nothing are listed, so they can be
    removed once the base contains the change.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

EPS = 1e-6
ACCEPTED = Path(__file__).parent / "accepted.json"
DEFAULT_TIME_REL = 0.20
DEFAULT_TIME_FLOOR = 1.0


def load(path: str) -> dict:
    return json.loads(Path(path).read_text(encoding="utf-8"))


def fmt(v) -> str:
    if v is None:
        return "-"
    if isinstance(v, float) and not v.is_integer():
        return f"{v:.3f}".rstrip("0").rstrip(".")
    return str(int(v)) if isinstance(v, (int, float)) else str(v)


def judge(spec: dict, base: float, head: float, args, scope: str) -> str:
    """Return "worse", "improved" or "same" for one metric."""
    direction = spec.get("direction", "info")
    if direction == "info":
        return "same"
    if spec.get("kind") == "time":
        if scope != "total":
            return "same"
        tol = max(args.time_rel * abs(base), args.time_floor)
    else:
        tol = EPS
    delta = head - base
    if direction == "higher":
        delta = -delta
    if delta > tol:
        return "worse"
    if delta < -tol:
        return "improved"
    return "same"


def rows_for(base: dict, head: dict, specs: dict, args, include_held_out: bool, notes: list):
    rows = []  # (verdict, scope, name, base, head)
    problems = []

    def add_pair(scope, key, b, h, spec):
        verdict = judge(spec, b, h, args, scope)
        if spec.get("zero") and h > EPS:
            verdict = "worse"
        rows.append((verdict, scope, key, b, h, spec))

    base_t, head_t = (base["totals"], head["totals"]) if not include_held_out else (
        _merge(base), _merge(head))
    for comp, metrics in head_t.items():
        for m, h in metrics.items():
            spec = specs.get(f"{comp}.{m}", {})
            if comp in base_t and m in base_t[comp]:
                add_pair("total", f"{comp}.{m}", base_t[comp][m], h, spec)
            else:
                rows.append(("new", "total", f"{comp}.{m}", None, h, spec))

    for ds, b in base["datasets"].items():
        h = head["datasets"].get(ds)
        held = b.get("held_out")
        if held and not include_held_out:
            continue
        if b["status"] != "ok":
            continue
        if h is None or h["status"] != "ok":
            problems.append(f"dataset {ds}: ran in base, {'absent' if h is None else h['status']} in head"
                            + (f" ({h.get('reason')})" if h else ""))
            continue
        for comp, bres in b["components"].items():
            if bres["status"] != "ok":
                continue
            hres = h["components"].get(comp)
            if hres is None:
                problems.append(f"{ds}/{comp}: ran in base, absent in head")
                continue
            if hres["status"] != "ok":
                problems.append(f"{ds}/{comp}: ran in base, {hres['status']} in head ({hres.get('reason')})")
                continue
            for m, bv in bres["metrics"].items():
                if m not in hres["metrics"]:
                    continue
                spec = specs.get(f"{comp}.{m}", {})
                verdict = judge(spec, bv, hres["metrics"][m], args, ds)
                if verdict != "same":
                    rows.append((verdict, ds, f"{comp}.{m}", bv, hres["metrics"][m], spec))
    for ds, h in head["datasets"].items():
        if ds not in base["datasets"]:
            notes.append(f"dataset {ds} is new in head (not compared)")
        elif base["datasets"][ds]["status"] != "ok" and h["status"] == "ok":
            notes.append(f"dataset {ds} ran in head only (not compared)")
    return rows, problems


def load_accepted(path) -> list[dict]:
    """Entries of the accepted file; an absent file means none."""
    path = Path(path)
    if not path.exists():
        return []
    doc = json.loads(path.read_text(encoding="utf-8"))
    entries = doc.get("accepted", [])
    for e in entries:
        missing = {"metric", "limit", "reason"} - set(e)
        if missing:
            raise ValueError(f"accepted entry {e} lacks {sorted(missing)}")
        e.setdefault("scope", "total")
    return entries


def within(spec: dict, head: float, limit: float) -> bool:
    if spec.get("direction") == "higher":
        return head >= limit - EPS
    return head <= limit + EPS


def apply_accepted(rows: list, entries: list[dict]) -> tuple[list, list, list]:
    """Mark worse rows covered by an accepted entry.

    Returns the rows with verdict "accepted" where an entry applies,
    the (row, entry) pairs that were accepted, and the entries that
    accepted nothing."""
    used = set()
    accepted_totals = set()
    out, pairs = [], []
    # totals first, so an accepted total can cover its per-dataset rows
    order = sorted(range(len(rows)), key=lambda i: rows[i][1] != "total")
    marked = {}
    for i in order:
        verdict, scope, key, b, h, spec = rows[i]
        if verdict != "worse":
            continue
        for j, e in enumerate(entries):
            if e["metric"] != key:
                continue
            if e["scope"] == scope and within(spec, h, e["limit"]):
                marked[i] = e
                used.add(j)
                if scope == "total":
                    accepted_totals.add(key)
                break
            if e["scope"] == "total" and scope != "total" and key in accepted_totals:
                marked[i] = e
                break
    for i, row in enumerate(rows):
        if i in marked:
            out.append(("accepted",) + tuple(row[1:]))
            pairs.append((row, marked[i]))
        else:
            out.append(row)
    unused = [e for j, e in enumerate(entries) if j not in used]
    return out, pairs, unused


def _merge(card: dict) -> dict:
    out = {c: dict(m) for c, m in card["totals"].items()}
    for c, m in card.get("held_out_totals", {}).items():
        for k, v in m.items():
            out.setdefault(c, {})
            out[c][k] = out[c].get(k, 0) + v
    return out


def table(rows) -> list[str]:
    lines = ["| metric | scope | base | head | delta |", "|---|---|---:|---:|---:|"]
    for _, scope, key, b, h, _spec in rows:
        delta = "" if b is None else fmt(round(h - b, 3))
        if delta and not delta.startswith("-") and delta != "0":
            delta = "+" + delta
        lines.append(f"| {key} | {scope} | {fmt(b)} | {fmt(h)} | {delta} |")
    return lines


def held_out_section(base: dict, head: dict) -> list[str]:
    b, h = base.get("held_out_totals") or {}, head.get("held_out_totals") or {}
    if not b and not h:
        return []
    out = ["", "## Held-out datasets (not part of the verdict)", "",
           "| metric | base | head | delta |", "|---|---:|---:|---:|"]
    for comp in sorted(set(b) | set(h)):
        for m in sorted(set(b.get(comp, {})) | set(h.get(comp, {}))):
            bv, hv = b.get(comp, {}).get(m), h.get(comp, {}).get(m)
            d = "" if bv is None or hv is None else fmt(round(hv - bv, 3))
            out.append(f"| {comp}.{m} | {fmt(bv)} | {fmt(hv)} | {d} |")
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("base")
    ap.add_argument("head")
    ap.add_argument("--time-rel", type=float, default=DEFAULT_TIME_REL,
                    help="relative tolerance on total time (default 0.20)")
    ap.add_argument("--time-floor", type=float, default=DEFAULT_TIME_FLOOR,
                    help="absolute tolerance in seconds on total time (default 1.0)")
    ap.add_argument("--accepted", default=str(ACCEPTED),
                    help="file of accepted regressions (default: accepted.json next to this script)")
    ap.add_argument("--include-held-out", action="store_true")
    ap.add_argument("--out", help="also write the markdown here")
    args = ap.parse_args()
    try:
        base, head = load(args.base), load(args.head)
        specs = {**base.get("metric_specs", {}), **head.get("metric_specs", {})}
        entries = load_accepted(args.accepted)
    except (OSError, ValueError, KeyError) as exc:
        print(f"cannot read scorecards or the accepted file: {exc}", file=sys.stderr)
        return 2

    notes: list[str] = []
    rows, problems = rows_for(base, head, specs, args, args.include_held_out, notes)
    rows, accepted, unused = apply_accepted(rows, entries)
    for e in unused:
        notes.append(f"accepted entry for {e['metric']} ({e['scope']}) accepted nothing; "
                     "remove it if the base already contains the change")
    worse = [r for r in rows if r[0] == "worse"]
    improved = [r for r in rows if r[0] == "improved"]
    totals = [r for r in rows if r[1] == "total"]
    failed = bool(worse or problems)

    bp, hp = base.get("provenance", {}), head.get("provenance", {})
    md = ["# Gate comparison", "",
          f"**{'FAIL' if failed else 'PASS'}**: {len(worse) + len(problems)} worse, "
          f"{len(accepted)} worse and accepted, "
          f"{len(improved)} improved, {sum(1 for r in totals if r[0] == 'same')} unchanged totals.", "",
          f"- base: onceover {bp.get('onceover_version')} at {bp.get('commit')}",
          f"- head: onceover {hp.get('onceover_version')} at {hp.get('commit')}",
          f"- seeds: base {bp.get('seeds')}, head {hp.get('seeds')}"]
    if bp.get("seeds") != hp.get("seeds") or bp.get("fast") != hp.get("fast"):
        md.append("- the two scorecards were produced with different seeds or subsets; "
                  "differences may reflect that")
    if problems:
        md += ["", "## Failed or missing", ""] + [f"- {p}" for p in problems]
    if worse:
        md += ["", "## Worse", ""] + table(worse)
    if accepted:
        md += ["", "## Worse and accepted", "",
               "| metric | scope | base | head | limit | reason |", "|---|---|---:|---:|---:|---|"]
        for (_, scope, key, b, h, _spec), e in accepted:
            md.append(f"| {key} | {scope} | {fmt(b)} | {fmt(h)} | "
                      f"{fmt(e['limit']) if e['scope'] == scope else 'by total'} | {e['reason']} |")
    md += ["", "## Improved", ""]
    md += table(improved) if improved else ["None."]
    md += ["", "## Totals over gated datasets", ""] + table(totals)
    others = [r for r in rows if r[1] != "total" and r[0] in ("worse", "improved", "accepted")]
    if others:
        md += ["", "## Per-dataset changes", ""] + table(others)
    md += held_out_section(base, head)
    if notes:
        md += ["", "## Notes", ""] + [f"- {n}" for n in notes]
    text = "\n".join(md) + "\n"
    print(text)
    if args.out:
        Path(args.out).write_text(text, encoding="utf-8")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
