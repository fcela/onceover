#!/usr/bin/env python3
"""Profile the gate datasets with the installed onceover and write a scorecard.

    python benchmarks/gate/run.py --out scorecard.json
    python benchmarks/gate/run.py --out s.json --only abalone --components planted,cost
    python benchmarks/gate/run.py --out s.json --fast --seeds 0,1 --commit <sha>

The scorecard holds, per dataset and component, the component's metrics,
and per component the totals over datasets that are not held out
(held-out totals are kept apart). Datasets and components come from
registry.json and components/__init__.py; nothing here names one.
"""
from __future__ import annotations

import argparse
import sys
import time
import traceback
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import gatelib as g  # noqa: E402
from components import REGISTRY  # noqa: E402

SCHEMA = 1


def csv_list(value: str | None) -> list[str] | None:
    return [v.strip() for v in value.split(",") if v.strip()] if value else None


def metric_specs(names: list[str]) -> dict:
    return {f"{n}.{m}": spec for n in names for m, spec in REGISTRY[n].METRICS.items()}


def run_dataset(entry: dict, components: list[str], seeds: list[int], fast: bool) -> dict:
    rec = {"held_out": bool(entry.get("held_out")), "kind": entry.get("kind"),
           "target": None, "components": {}}
    ctx, err = g.build_context(entry, seeds, fast)
    if ctx is None:
        rec["status"], rec["reason"] = err
        return rec
    rec["status"] = "ok"
    rec["rows"], rec["cols"] = ctx.df.height, ctx.df.width
    rec["target"] = ctx.target
    wanted = entry.get("components") or list(REGISTRY)
    for name in components:
        if name not in wanted:
            continue
        t0 = time.perf_counter()
        try:
            res = REGISTRY[name].run(ctx)
        except Exception as exc:  # recorded in the scorecard; the comparison treats it as a failure
            traceback.print_exc()
            res = {"status": "error", "reason": f"{type(exc).__name__}: {exc}"}
        res.setdefault("status", "ok")
        res["wall_seconds"] = round(time.perf_counter() - t0, 2)
        rec["components"][name] = res
    return rec


def totals(datasets: dict, held_out: bool) -> dict:
    out: dict = {}
    for rec in datasets.values():
        if rec["status"] != "ok" or rec["held_out"] != held_out:
            continue
        for name, res in rec["components"].items():
            if res["status"] != "ok":
                continue
            bucket = out.setdefault(name, {})
            for k, v in res["metrics"].items():
                bucket[k] = round(bucket.get(k, 0) + v, 3)
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--out", required=True, help="scorecard JSON to write")
    ap.add_argument("--only", help="comma-separated dataset ids")
    ap.add_argument("--components", help="comma-separated component names (default: all)")
    ap.add_argument("--seeds", default="0", help="comma-separated integers (default: 0)")
    ap.add_argument("--fast", action="store_true", help="only datasets marked fast")
    ap.add_argument("--commit", default="unknown", help="commit sha recorded as provenance")
    args = ap.parse_args()

    seeds = [int(s) for s in csv_list(args.seeds)]
    names = csv_list(args.components) or list(REGISTRY)
    unknown = [n for n in names if n not in REGISTRY]
    if unknown:
        print(f"unknown components: {unknown}; registered: {list(REGISTRY)}", file=sys.stderr)
        return 2
    only = csv_list(args.only)
    registry = g.load_registry()
    entries = [e for e in registry["datasets"]
               if (not only or e["id"] in only) and (not args.fast or e.get("fast"))]
    if only:
        missing = set(only) - {e["id"] for e in registry["datasets"]}
        if missing:
            print(f"unknown datasets: {sorted(missing)}", file=sys.stderr)
            return 2

    started = time.perf_counter()
    datasets = {}
    for entry in entries:
        t0 = time.perf_counter()
        datasets[entry["id"]] = run_dataset(entry, names, seeds, args.fast)
        rec = datasets[entry["id"]]
        flag = "held out" if rec["held_out"] else ""
        print(f"{entry['id']:<28} {rec['status']:<8} {time.perf_counter() - t0:6.1f}s {flag}"
              + (f"  {rec.get('reason', '')[:80]}" if rec["status"] != "ok" else ""), flush=True)

    card = {
        "schema": SCHEMA,
        "provenance": {**g.provenance(args.commit), "seeds": seeds, "components": names,
                       "fast": args.fast, "only": only},
        "metric_specs": metric_specs(names),
        "datasets": datasets,
        "totals": totals(datasets, held_out=False),
        "held_out_totals": totals(datasets, held_out=True),
        "seconds": round(time.perf_counter() - started, 1),
    }
    g.dump(card, Path(args.out))
    print(f"scorecard: {args.out} ({card['seconds']}s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
