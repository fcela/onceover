#!/usr/bin/env python3
"""Family-consolidation evaluation for issue #116.

Profiles the three case-study datasets exactly as the corpus runner
does (same CSV read, budget, detectors, hints from registry.json)
and measures report selection against an independently annotated set
of useful statements per dataset. The annotation comes from the
case-study ground truth, never from the profiler's output:

- Adult: ~/workspace/unseen-test-adult (ground truth computed with
  Polars before profiling; case study iteration 3, 2026-11-03).
- Bike Sharing: ~/workspace/unseen-test-bike (seven independent
  spot checks plus eight measured misses, 2026-11-03).
- State unemployment: the target account in
  ~/workspace/research_notes/onceover-beijing-test-20261102/
  target-account-unemployment.md (2026-11-03).

Ranking convention (shared with benchmarks/quality/battery.py):
statements are read in the order a reader meets them — the frame
account first (grain, then dominant structures, as the digest
presents them), then the findings in report order. A useful item is
recalled at rank k when its first matching statement sits at
position <= k in that stream. Grouped statements (schema 0.6) are
not in the stream: they spend no digest slot, so they cannot recall
an item at any rank — that is the consolidation trade this measures.

Usage:
    python benchmarks/family_evaluation.py --out-dir DIR
Writes report-<dataset>.json and evaluation.json into DIR and
prints the before/after-ready tables.
"""
from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

import polars as pl

BENCH = Path(__file__).parent
ROOT = BENCH.parent

DATASETS = ["adult-census-income", "bike-sharing-hourly", "state-unemployment-long"]


def cols(f: dict) -> set:
    return set((f.get("subject") or {}).get("columns") or [])


def typ(f: dict) -> str:
    return f.get("type") or ""


def form(f: dict) -> str:
    return (f.get("specification") or {}).get("form") or ""


def target(f: dict):
    return (f.get("subject") or {}).get("target")


def inputs(f: dict) -> set:
    return set((f.get("subject") or {}).get("inputs") or [])


def claim(f: dict) -> str:
    return f.get("claim") or ""


# --- the annotation: useful statements, from ground truth ---------

def adult_items() -> dict:
    pair = {"education", "education_num"}
    return {
        "encoding_bijection": lambda f: typ(f) in ("functional", "duplicate_encoding")
        and pair <= cols(f),
        "exact_duplicate_rows": lambda f: typ(f) == "duplicate_rows",
        "capital_loss_zero_mass": lambda f: typ(f) == "value_concentration"
        and "capital_loss" in cols(f),
        "capital_gain_zero_mass": lambda f: typ(f) == "value_concentration"
        and "capital_gain" in cols(f),
        "native_country_concentration": lambda f: typ(f) == "value_concentration"
        and "native_country" in cols(f),
        "income_predictability": lambda f: typ(f) == "predictive"
        and target(f) == "income",
        "capital_gain_hotspot": lambda f: typ(f) == "hotspot"
        and {"capital_gain", "income"} <= cols(f),
        "dominant_structure": lambda f: "marital_status" in claim(f)
        or "occupation" in claim(f),
        "missingness_stated": lambda f: typ(f) == "missing_values",
        "capital_gain_top_code": lambda f: "99,999" in claim(f)
        or "99999" in claim(f),
    }


def bike_items() -> dict:
    tri = {"cnt", "casual", "registered"}
    return {
        "additive_identity": lambda f: typ(f) == "functional"
        and form(f) in ("sum", "difference", "product", "ratio",
                        "linear_combination")
        and cols(f) == tri,
        "fd_triangle": lambda f: typ(f) == "functional"
        and form(f) == "value_mapping"
        and cols(f) == tri,
        "dteday_to_holiday_fd": lambda f: typ(f) == "functional"
        and target(f) == "holiday"
        and "dteday" in inputs(f),
        "calendar_determinism": lambda f: typ(f) == "functional"
        and "dteday" in inputs(f)
        and target(f) in ("mnth", "season", "weekday", "workingday", "yr",
                          "holiday"),
        "instant_key": lambda f: typ(f) == "uniqueness"
        and "instant" in cols(f),
        "natural_key_dteday_hr": lambda f: {"dteday", "hr"} <= cols(f)
        and typ(f) in ("uniqueness", "duplicate_rows", "account"),
        "grid_gaps": lambda f: typ(f) in ("gap", "sequence_gap",
                                          "missing_combinations"),
        "holiday_concentration": lambda f: typ(f) == "value_concentration"
        and "holiday" in cols(f),
        "temp_atemp_association": lambda f: typ(f) == "statistical"
        and {"temp", "atemp"} <= cols(f),
        "second_year_growth": lambda f: "143.8" in claim(f)
        or "234.7" in claim(f)
        or "63%" in claim(f),
        "weathersit_4_rarity": lambda f: "weathersit" in claim(f)
        and ("= 4" in claim(f) or "3 rows" in claim(f)),
        "windspeed_zero_floor": lambda f: "windspeed" in claim(f)
        and ("0 on" in claim(f) or "zero" in claim(f)),
    }


def unemployment_items() -> dict:
    return {
        "grain_key": lambda f: {"date", "state"} <= cols(f)
        and typ(f) in ("uniqueness", "account")
        or (typ(f) == "account" and "identify" in claim(f)),
        "span_and_balance": lambda f: "609" in claim(f) or "1976" in claim(f),
        "october_2025_hole": lambda f: "2025-10" in claim(f),
        "national_cycle_peak": lambda f: "2020-04" in claim(f)
        or "30.5" in claim(f)
        or "13.6" in claim(f),
        "regime_tree": lambda f: typ(f) == "ordered_partition",
    }


ITEMS = {
    "adult-census-income": adult_items,
    "bike-sharing-hourly": bike_items,
    "state-unemployment-long": unemployment_items,
}


def statement_stream(data: dict) -> list[dict]:
    """The reader's stream: account statements, then findings in
    report order. Account statements are pseudo-findings whose claim
    is the account sentence."""
    stream: list[dict] = []
    frame = data.get("frame") or {}
    if frame.get("grain"):
        stream.append({"type": "account", "claim": frame["grain"]["statement"],
                       "subject": {"columns": frame["grain"].get("key_columns", [])}})
    for d in frame.get("dominant") or []:
        stream.append({"type": "account", "claim": d["statement"],
                       "subject": {"columns": []}})
    stream.extend(data["findings"])
    return stream


def evaluate(data: dict, items: dict) -> dict:
    stream = statement_stream(data)
    grouped = data.get("grouped_findings") or []
    rank_of_id = {f["id"]: i + 1 for i, f in enumerate(stream) if f.get("id")}
    ranks = {}
    for name, pred in items.items():
        rank = next((i + 1 for i, f in enumerate(stream) if pred(f)), None)
        if rank is None:
            # A grouped statement is read through its lead.
            rank = next(
                (rank_of_id[m["grouped_under"]] for m in grouped
                 if pred(m) and m.get("grouped_under") in rank_of_id),
                None,
            )
        ranks[name] = rank
    digest_full = None
    return {
        "n_findings": len(data["findings"]),
        "n_grouped": len(data.get("grouped_findings") or []),
        "selection": data.get("selection"),
        "ranks": ranks,
        "top10": sum(1 for r in ranks.values() if r is not None and r <= 10),
        "top25": sum(1 for r in ranks.values() if r is not None and r <= 25),
        "stated": sum(1 for r in ranks.values() if r is not None),
        "items_total": len(items),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out-dir", required=True)
    args = ap.parse_args()
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    sys.path.insert(0, str(ROOT / "python"))
    import onceover

    registry = json.loads((BENCH / "corpus" / "registry.json").read_text())
    entries = {d["id"]: d for d in registry["datasets"]}

    record = {"onceover_version": onceover.__version__, "datasets": {}}
    for ds in DATASETS:
        entry = entries[ds]
        csv = BENCH / "corpus" / entry["dir"] / entry.get("file", "data.csv")
        df = pl.read_csv(csv, infer_schema_length=None,
                         try_parse_dates=bool(entry.get("parse_dates")))
        t = time.time()
        report = onceover.profile(df, budget=entry.get("budget", "standard"),
                                  detectors=entry.get("detectors"),
                                  hints=entry.get("hints"))
        seconds = round(time.time() - t, 2)
        data = report.data
        (out_dir / f"report-{ds}.json").write_text(report.to_json())
        digest = report.to_digest(4000)
        ev = evaluate(data, ITEMS[ds]())
        ev["seconds"] = seconds
        ev["report_chars"] = len(report.to_json())
        ev["digest_chars_4000"] = len(digest)
        ev["digest_lines_4000"] = digest.count("\n")
        record["datasets"][ds] = ev
        print(f"{ds}: findings={ev['n_findings']} grouped={ev['n_grouped']} "
              f"top10={ev['top10']}/{ev['items_total']} "
              f"top25={ev['top25']}/{ev['items_total']} "
              f"stated={ev['stated']}/{ev['items_total']} "
              f"digest={ev['digest_chars_4000']} chars "
              f"report={ev['report_chars']} chars ({seconds}s)")
        for name, rank in ev["ranks"].items():
            print(f"    {name:<28} rank={rank}")
    (out_dir / "evaluation.json").write_text(json.dumps(record, indent=1))
    print(f"record: {out_dir / 'evaluation.json'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
