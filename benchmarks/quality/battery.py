"""Planted-signal discovery-quality battery (issue #115).

Profiles synthetic frames whose ground truth is known by
construction (benchmarks/quality/plants.py) and reports, per plant
family:

- detector recall: a profile restricted to the family's detectors
  reports a matching finding;
- post-selection recall: the default profile's selected findings
  contain a match;
- top-10 / top-25 recall: the first match ranks within k when the
  selected findings are ordered by the library's `interestingness`
  score, descending (the report's list order is not the ranking;
  the digest ranks by this score);
- false positives: family-matching findings on null frames (where
  every match is a false positive by construction) and
  family-matching findings on planted frames that involve no
  planted signal (reported per family);
- control collapse: permuting a plant's response column destroys
  the signal; the match rate on permuted frames is reported next
  to the base rate.

Perturbation variants: 5% MCAR missingness in the signal columns,
column-order reversal, and wide frames with the signal planted
beyond column 64.

Results are machine-readable JSON with provenance (library
version, platform, package versions, the commit passed with
--commit, the full seed/config lists) so a run can be reproduced
and compared. Usage:

    python benchmarks/quality/battery.py --fast
    python benchmarks/quality/battery.py --full --commit <sha> \
        --out benchmarks/quality/results/full.json
"""

import argparse
import json
import platform as platform_mod
import sys
import time
from collections import defaultdict
from pathlib import Path

import numpy as np
import polars as pl

sys.path.insert(0, str(Path(__file__).resolve().parent))
import plants as plant_mod  # noqa: E402

import onceover  # noqa: E402

FAST_SEEDS = [0, 1]
FULL_SEEDS = list(range(12))


def columns_of(f: dict) -> set:
    subject = f.get("subject") or {}
    cols = set(subject.get("columns") or [])
    if subject.get("target"):
        cols.add(subject["target"])
    return cols


def matches(f: dict, spec: dict) -> bool:
    return f.get("type") in spec["types"] and spec["columns"] <= columns_of(f)


def first_match_rank(findings: list[dict], spec: dict) -> int | None:
    """1-based rank of the first match in interestingness order."""
    ranked = sorted(findings, key=lambda f: -f.get("interestingness", 0.0))
    for i, f in enumerate(ranked, start=1):
        if matches(f, spec):
            return i
    return None


def any_match(findings: list[dict], spec: dict) -> bool:
    return any(matches(f, spec) for f in findings)


def target_match(f: dict, spec: dict) -> bool:
    """Looser secondary match: the finding reports the planted
    response column with a family type, in any context. Distinguishes
    "the signal's values were reported, under another grouping" from
    "nothing about the signal was reported"."""
    return (f.get("type") in spec["types"]
            and spec["response"] in columns_of(f))


def stray_family_findings(findings: list[dict], spec: dict,
                          signal_columns: set) -> int:
    """Findings of the family's types that involve none of the
    planted signal columns: on a planted frame these are the
    family's false positives (they can only concern noise columns).
    """
    return sum(1 for f in findings
               if f.get("type") in spec["types"]
               and not (columns_of(f) & signal_columns))


def profile(df: pl.DataFrame, spec: dict | None, restricted: bool):
    detectors = spec["detectors"] if (restricted and spec) else None
    hints = (spec or {}).get("hints")
    return onceover.profile(df, detectors=detectors, hints=hints)


def run_plant(builder, seed: int, variant: str = "base") -> dict:
    df, spec = builder(seed)
    raw = {c: df[c].to_numpy() for c in df.columns}
    assert spec["verify"](raw), f"plant verification failed: {builder.__name__} seed {seed}"
    if variant == "permuted":
        rng = np.random.default_rng(seed + 100_000)
        resp = spec["response"]
        df = df.with_columns(pl.Series(resp, rng.permutation(df[resp].to_numpy())))
    if variant == "missing5":
        rng = np.random.default_rng(seed + 200_000)
        n = df.height
        for col in spec["columns"]:
            mask = rng.random(n) < 0.05
            if mask.any():
                df = df.with_columns(
                    pl.when(pl.Series(mask)).then(None)
                    .otherwise(pl.col(col)).alias(col))
    if variant == "shuffled":
        df = df.select(list(reversed(df.columns)))

    rec = {"seed": seed, "variant": variant}
    t = time.time()
    det = profile(df, spec, restricted=True)
    rec["detector_recall"] = any_match(det.data["findings"], spec)
    rec["detector_target_recall"] = any(
        target_match(f, spec) for f in det.data["findings"])
    full = profile(df, spec, restricted=False)
    findings = full.data["findings"]
    grouped = full.data.get("grouped_findings") or []
    rec["selected_target_recall"] = any(
        target_match(f, spec) for f in findings)
    rank = first_match_rank(findings, spec)
    rec["selected_recall"] = rank is not None
    # Explicit selection outcome (schema 0.6): a statement grouped
    # under a verified relationship family is stated in the report
    # but spends no selection slot, so it is recorded separately
    # rather than folded into either "selected" or "missed".
    grouped_hit = any_match(grouped, spec)
    rec["grouped_recall"] = bool(grouped_hit and rank is None)
    rec["selection_outcome"] = (
        "selected" if rank is not None
        else "grouped" if grouped_hit
        else "missed")
    rec["stated_recall"] = rec["selection_outcome"] != "missed"
    rec["rank"] = rank
    rec["top10"] = bool(rank and rank <= 10)
    rec["top25"] = bool(rank and rank <= 25)
    rec["stray_family_findings"] = stray_family_findings(
        findings, spec, set(spec["columns"]))
    rec["seconds"] = round(time.time() - t, 2)
    return rec


def run_wide(builder, seed: int) -> dict:
    """The plant's signal columns sit beyond column 64: 70 noise
    columns first, then the signal columns, then the plant's own
    noise columns."""
    df, spec = builder(seed)
    rng = np.random.default_rng(seed + 300_000)
    n = df.height
    wide = pl.DataFrame({f"w_{i}": rng.normal(size=n) for i in range(70)})
    for c in df.columns:
        if c in spec["columns"]:
            wide = wide.with_columns(df[c])
    for c in df.columns:
        if c not in wide.columns:
            wide = wide.with_columns(df[c])
    rec = {"seed": seed, "variant": "wide"}
    det = profile(wide, spec, restricted=True)
    rec["detector_recall"] = any_match(det.data["findings"], spec)
    rec["detector_target_recall"] = any(
        target_match(f, spec) for f in det.data["findings"])
    full = profile(wide, spec, restricted=False)
    rec["selected_target_recall"] = any(
        target_match(f, spec) for f in full.data["findings"])
    rank = first_match_rank(full.data["findings"], spec)
    rec["selected_recall"] = rank is not None
    grouped_hit = any_match(full.data.get("grouped_findings") or [], spec)
    rec["grouped_recall"] = bool(grouped_hit and rank is None)
    rec["selection_outcome"] = (
        "selected" if rank is not None
        else "grouped" if grouped_hit
        else "missed")
    rec["stated_recall"] = rec["selection_outcome"] != "missed"
    rec["rank"] = rank
    rec["top10"] = bool(rank and rank <= 10)
    rec["top25"] = bool(rank and rank <= 25)
    rec["stray_family_findings"] = stray_family_findings(
        full.data["findings"], spec, set(spec["columns"]))
    return rec


def run_null(seed: int, specs: list[dict]) -> dict:
    df = plant_mod.null_frame(seed)
    full = profile(df, None, restricted=False)
    findings = full.data["findings"]
    fp = {}
    for spec in specs:
        fp[spec["family"]] = fp.get(spec["family"], 0) + sum(
            1 for f in findings if f.get("type") in spec["types"])
    return {"seed": seed, "variant": "null", "false_positives": fp,
            "total_findings": len(findings)}


def summarize(records: list[dict]) -> dict:
    out = {}
    for key in ("detector_recall", "detector_target_recall",
                "selected_recall", "selected_target_recall",
                "grouped_recall", "stated_recall",
                "top10", "top25"):
        if records and key in records[0]:
            vals = [bool(r[key]) for r in records]
            out[key] = {"mean": round(float(np.mean(vals)), 3),
                        "min": bool(min(vals)), "n": len(vals)}
    ranks = [r["rank"] for r in records if r.get("rank")]
    if ranks:
        out["rank_median"] = float(np.median(ranks))
    strays = [r.get("stray_family_findings", 0) for r in records]
    out["stray_family_findings_mean"] = round(float(np.mean(strays)), 2)
    return out


def run_battery(full: bool, commit: str) -> dict:
    seeds = FULL_SEEDS if full else FAST_SEEDS
    pseeds = seeds if full else FAST_SEEDS
    started = time.time()
    results: dict = {"provenance": {
        "onceover_version": onceover.__version__,
        "commit": commit,
        "python": sys.version.split()[0],
        "polars": pl.__version__,
        "numpy": np.__version__,
        "platform": platform_mod.platform(),
        "config": {"seeds": seeds, "perturb_seeds": pseeds,
                   "fast": not full}},
        "plants": {}, "nulls": []}

    specs = []
    for name, builder in plant_mod.CORE_PLANTS.items():
        _, spec = builder(0)
        specs.append(spec)
        recs = [run_plant(builder, s) for s in seeds]
        entry = {"family": spec["family"], "base": summarize(recs),
                 "records": recs, "variants": {}}
        if name in plant_mod.PERMUTABLE:
            prec = [run_plant(builder, s, "permuted") for s in pseeds]
            entry["variants"]["permuted"] = {
                **summarize(prec), "records": prec}
        if name in plant_mod.MISSINGNESS:
            mrec = [run_plant(builder, s, "missing5") for s in pseeds]
            entry["variants"]["missing5"] = {
                **summarize(mrec), "records": mrec}
        if name in plant_mod.WIDE:
            wrec = [run_wide(builder, s) for s in pseeds]
            entry["variants"]["wide"] = {**summarize(wrec), "records": wrec}
        srec = [run_plant(builder, s, "shuffled") for s in pseeds]
        entry["variants"]["shuffled"] = {**summarize(srec), "records": srec}
        results["plants"][name] = entry
        print(f"{name:<22} base selected {entry['base']['selected_recall']['mean']:.2f} "
              f"detector {entry['base']['detector_recall']['mean']:.2f}")

    for s in seeds:
        results["nulls"].append(run_null(s, specs))
    fp_totals: dict = defaultdict(list)
    for rec in results["nulls"]:
        for fam, count in rec["false_positives"].items():
            fp_totals[fam].append(count)
    results["null_summary"] = {
        fam: {"mean_per_frame": round(float(np.mean(v)), 2),
              "max_per_frame": int(max(v)), "frames": len(v)}
        for fam, v in fp_totals.items()}
    results["seconds"] = round(time.time() - started, 1)
    return results


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--full", action="store_true")
    ap.add_argument("--fast", action="store_true")
    ap.add_argument("--commit", default="unknown")
    ap.add_argument("--out")
    args = ap.parse_args()
    results = run_battery(full=args.full, commit=args.commit)
    if args.out:
        out = Path(args.out)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(results, indent=1))
        print(f"results: {out}")
    print(f"total {results['seconds']}s; null false positives: "
          f"{json.dumps(results['null_summary'])}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
