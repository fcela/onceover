#!/usr/bin/env python3
"""Keep a history of gate scorecards and render it as one readable report.

    python benchmarks/gate/report.py add scorecard.json --history DIR \\
        --commit SHA --date 2026-11-15T20:36:24Z --subject "Merge ..."
    python benchmarks/gate/report.py render --history DIR --out REPORT.md

`add` stores one history entry per commit: the scorecard without its
per-component detail, plus the commit, its date, its subject line and
the accepted regressions in force (accepted.json). Entry files are
named by commit date, so sorting the file names gives the order of
the changes.

`render` writes a markdown report:

  1. A TL;DR: the verdict on the latest change, the digest metrics
     that decide whether the library is improving against the previous
     change and the first recorded one, the biggest movements, and
     the hard lines (verification failures, a lost must-tier fact).
  2. Headline metrics, one row per change, newest first, each value
     with its movement against the change before it.
  3. The latest change: what improved, what got worse, what was
     accepted, in totals and per dataset.
  4. Every total metric over the most recent changes.
  5. The latest scorecard per dataset.
  6. The held-out datasets, which the pass/fail comparison leaves out.

The headline metrics and their labels are the "headline" list in
registry.json. Time metrics are compared only between entries produced
on the same platform, because seconds measured on different machines
say nothing about the library.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).parent))
import compare as cmp  # noqa: E402

GATE = Path(__file__).parent
TOL = SimpleNamespace(time_rel=cmp.DEFAULT_TIME_REL, time_floor=cmp.DEFAULT_TIME_FLOOR)
WORDS = {"improved": "better", "worse": "worse"}


def slim(card: dict) -> dict:
    """The scorecard without per-component detail (examples, per-plant
    records). The metrics, statuses and totals are kept."""
    out = {k: v for k, v in card.items() if k != "datasets"}
    out["datasets"] = {}
    for ds, rec in card["datasets"].items():
        r = {k: v for k, v in rec.items() if k != "components"}
        r["components"] = {
            name: {k: v for k, v in res.items() if k != "detail"}
            for name, res in rec.get("components", {}).items()}
        out["datasets"][ds] = r
    return out


def entry_name(date: str, commit: str) -> str:
    stamp = re.sub(r"[^0-9]", "", date)[:14]
    return f"{stamp}-{commit[:7]}.json"


def add(args) -> int:
    card = json.loads(Path(args.scorecard).read_text(encoding="utf-8"))
    history = Path(args.history)
    history.mkdir(parents=True, exist_ok=True)
    accepted = [] if args.accepted == "none" else cmp.load_accepted(args.accepted)
    entry = {
        "meta": {"commit": args.commit, "date": args.date, "subject": args.subject,
                 "accepted": accepted},
        "scorecard": slim(card),
    }
    path = history / entry_name(args.date, args.commit)
    path.write_text(json.dumps(entry, indent=1) + "\n", encoding="utf-8")
    print(f"history entry: {path}")
    return 0


def load_history(history: Path) -> list[dict]:
    entries = []
    for path in sorted(history.glob("*.json")):
        entries.append(json.loads(path.read_text(encoding="utf-8")))
    return entries


def headline_spec() -> list[dict]:
    reg = json.loads((GATE / "registry.json").read_text(encoding="utf-8"))
    return reg.get("headline", [])


def total(entry: dict, metric: str, held_out: bool = False):
    comp, name = metric.split(".", 1)
    block = entry["scorecard"].get("held_out_totals" if held_out else "totals", {})
    return block.get(comp, {}).get(name)


def spec_of(entry: dict, metric: str) -> dict:
    return entry["scorecard"].get("metric_specs", {}).get(metric, {})


def comparable(spec: dict, prev: dict, cur: dict) -> bool:
    """Time metrics are compared only on the same platform."""
    if spec.get("kind") != "time":
        return True
    a, b = prev["scorecard"]["provenance"], cur["scorecard"]["provenance"]
    return a.get("platform") == b.get("platform")


def movement(spec: dict, prev, cur, prev_entry: dict, cur_entry: dict) -> tuple[str, str]:
    """(verdict, text) for a value against the previous one."""
    if prev is None or cur is None or not comparable(spec, prev_entry, cur_entry):
        return "same", ""
    delta = round(cur - prev, 3)
    if delta == 0:
        return "same", ""
    verdict = cmp.judge(spec, prev, cur, TOL, "total")
    sign = "+" if delta > 0 else ""
    word = WORDS.get(verdict)
    return verdict, f" ({sign}{cmp.fmt(delta)}{', ' + word if word else ''})"


def cell(entry: dict, prev: dict | None, metric: str, held_out: bool = False) -> str:
    cur = total(entry, metric, held_out)
    if cur is None:
        return "-"
    if prev is None:
        return cmp.fmt(cur)
    _, text = movement(spec_of(entry, metric), total(prev, metric, held_out), cur, prev, entry)
    return cmp.fmt(cur) + text


def direction_note(spec: dict) -> str:
    return {"higher": "higher is better", "lower": "lower is better"}.get(
        spec.get("direction"), "information only")


def short(text: str, n: int = 70) -> str:
    text = " ".join((text or "").split()).replace("|", "/")
    return text if len(text) <= n else text[: n - 1] + "…"


def headline_table(entries: list[dict], headline: list[dict], held_out: bool = False) -> list[str]:
    cols = [h for h in headline if any(total(e, h["metric"], held_out) is not None for e in entries)]
    lines = ["| date | commit | change | " + " | ".join(h["label"] for h in cols) + " |",
             "|---|---|---|" + "---:|" * len(cols)]
    for i in range(len(entries) - 1, -1, -1):
        e, prev = entries[i], entries[i - 1] if i else None
        m = e["meta"]
        cells = [cell(e, prev, h["metric"], held_out) for h in cols]
        lines.append(f"| {m['date'][:10]} | `{m['commit'][:7]}` | {short(m['subject'])} | "
                     + " | ".join(cells) + " |")
    return lines


def classify(prev: dict, cur: dict) -> dict:
    """Totals and per-dataset values of `cur` judged against `prev`."""
    out = {"improved": [], "worse": [], "accepted": []}
    accepted = {(a["metric"], a.get("scope", "total")): a for a in cur["meta"].get("accepted", [])}
    accepted_totals = set()

    def judge(scope, metric, b, h, spec):
        if b is None or h is None or not comparable(spec, prev, cur):
            return
        verdict = cmp.judge(spec, b, h, TOL, "total" if scope == "total" else scope)
        if verdict == "same":
            return
        row = (metric, scope, b, h)
        if verdict == "worse":
            entry = accepted.get((metric, scope))
            if entry and cmp.within(spec, h, entry["limit"]):
                if scope == "total":
                    accepted_totals.add(metric)
                out["accepted"].append(row + (entry["reason"],))
                return
            if scope != "total" and metric in accepted_totals:
                out["accepted"].append(row + (accepted[(metric, "total")]["reason"],))
                return
        out[verdict].append(row)

    for comp, metrics in cur["scorecard"]["totals"].items():
        for name, h in metrics.items():
            metric = f"{comp}.{name}"
            judge("total", metric, total(prev, metric), h, spec_of(cur, metric))
    for ds, rec in cur["scorecard"]["datasets"].items():
        before = prev["scorecard"]["datasets"].get(ds)
        if rec.get("held_out") or not before or rec["status"] != "ok" or before["status"] != "ok":
            continue
        for comp, res in rec["components"].items():
            bres = before["components"].get(comp)
            if not bres or res.get("status") != "ok" or bres.get("status") != "ok":
                continue
            for name, h in res.get("metrics", {}).items():
                metric = f"{comp}.{name}"
                judge(ds, metric, bres.get("metrics", {}).get(name), h, spec_of(cur, metric))
    return out


def change_rows(rows: list, with_reason: bool = False) -> list[str]:
    head = "| metric | where | before | after | change |" + (" reason |" if with_reason else "")
    lines = [head, "|---|---|---:|---:|---:|" + ("---|" if with_reason else "")]
    for row in sorted(rows, key=lambda r: (r[1] != "total", r[0], r[1])):
        metric, scope, b, h = row[:4]
        delta = round(h - b, 3)
        text = ("+" if delta > 0 else "") + cmp.fmt(delta)
        line = f"| {metric} | {scope} | {cmp.fmt(b)} | {cmp.fmt(h)} | {text} |"
        if with_reason:
            line += f" {row[4]} |"
        lines.append(line)
    return lines


TLDR_METRICS = [
    "reference.must_4k_default",
    "reference.must_4k_target",
    "reference.should_4k_default",
    "reference.should_4k_target",
    "planted.found_4k",
    "redundancy.repeated_default",
    "redundancy.repeated_target",
    "permuted.false_4k",
    "truth.mismatch",
]

# The metric specs carry direction and kind but no words, so the
# TL;DR's plain labels live here. A metric missing from this map
# falls back to its registry headline label.
TLDR_LABELS = {
    "reference.must_4k_default":
        "Must-tier reference facts stated in the 4,000-character digest",
    "reference.must_4k_target":
        "Must-tier reference facts stated in the 4,000-character digest, target mode",
    "reference.should_4k_default":
        "Should-tier reference facts stated in the 4,000-character digest",
    "reference.should_4k_target":
        "Should-tier reference facts stated in the 4,000-character digest, target mode",
    "planted.found_4k":
        "Planted structures stated in the 4,000-character digest",
    "redundancy.repeated_default":
        "Facts repeated in the 4,000-character digest",
    "redundancy.repeated_target":
        "Facts repeated in the 4,000-character digest, target mode",
    "permuted.false_4k":
        "Relationship statements in the 4,000-character digest of shuffled tables "
        "(all false by construction)",
    "truth.mismatch":
        "Statements that fail verification against the data",
}


def tldr_label(metric: str, headline: list[dict]) -> str:
    if metric in TLDR_LABELS:
        return TLDR_LABELS[metric]
    for h in headline:
        if h["metric"] == metric:
            return h["label"]
    return metric.replace(".", " ").replace("_", " ")


def tldr_value(entry: dict, metric: str) -> str:
    """A TL;DR cell value; reference counts are stated against the
    number of reference statements of their tier."""
    v = total(entry, metric)
    if v is None:
        return "-"
    m = re.fullmatch(r"reference\.(must|should)_.+", metric)
    if m:
        n = total(entry, f"reference.{m.group(1)}_total")
        if n is not None:
            return f"{cmp.fmt(v)} of {cmp.fmt(n)}"
    return cmp.fmt(v)


def tldr_marker(cur: dict, prev: dict, metric: str) -> str:
    b, h = total(prev, metric), total(cur, metric)
    if b is None or h is None:
        return ""
    verdict, text = movement(spec_of(cur, metric), b, h, prev, cur)
    return text if verdict != "same" else " (unchanged)"


def tldr_movements(rows: list, headline: list[dict], limit: int = 3) -> list[str]:
    """The largest total-scope movements in a classify() row list,
    largest relative change first."""
    totals = [r for r in rows if r[1] == "total"]

    def rank(r):
        delta = abs(r[3] - r[2])
        return (-(delta / max(abs(r[2]), 1)), -delta, r[0])

    out = []
    for r in sorted(totals, key=rank)[:limit]:
        text = f"{tldr_label(r[0], headline)} {cmp.fmt(r[2])} → {cmp.fmt(r[3])}"
        if len(r) > 4:
            text += f" ({r[4]})"
        out.append(text)
    return out


def tldr_hard_lines(entries: list[dict]) -> list[str]:
    cur = entries[-1]
    lines = []
    for metric, words in (("truth.mismatch", "statements failed verification against the data"),
                          ("truth.error", "statements errored during verification")):
        v = total(cur, metric)
        if v:
            lines.append(f"Hard line breached: {cmp.fmt(v)} {words} in the latest scorecard.")
    if len(entries) > 1:
        prev = entries[-2]
        for mode in ("default", "target"):
            metric = f"reference.must_4k_{mode}"
            b, h = total(prev, metric), total(cur, metric)
            if b is not None and h is not None and h < b:
                lines.append("Hard line breached: a must-tier reference fact was lost from "
                             f"the 4,000-character digest ({mode} mode): "
                             f"{cmp.fmt(b)} → {cmp.fmt(h)}.")
    if not lines:
        lines.append("Hard lines: none breached — no statement failed or errored in "
                     "verification, and no must-tier reference fact was lost from "
                     "the 4,000-character digest.")
    return lines


def tldr_section(entries: list[dict], headline: list[dict]) -> list[str]:
    cur = entries[-1]
    m = cur["meta"]
    out = ["", "## TL;DR", ""]
    classified = classify(entries[-2], cur) if len(entries) > 1 else None
    if classified is None:
        out.append(f"`{m['commit'][:7]}` ({m['date'][:10]}) is the first recorded change; "
                   "there is nothing to compare it with yet.")
        cols = [("latest", cur)]
    else:
        prev = entries[-2]
        tot = lambda rows: sum(1 for r in rows if r[1] == "total")  # noqa: E731
        out.append(f"Latest change `{m['commit'][:7]}` ({m['date'][:10]}), "
                   f"{short(m['subject'], 80)}: compared with `{prev['meta']['commit'][:7]}`, "
                   f"{tot(classified['improved'])} total metrics better, "
                   f"{tot(classified['worse'])} worse, "
                   f"{tot(classified['accepted'])} worse and accepted.")
        cols = [("latest", cur), ("previous", prev), ("first recorded", entries[0])]
    out += ["", "| measure | " + " | ".join(f"{name} (`{e['meta']['commit'][:7]}`)"
                                            for name, e in cols) + " |",
            "|---|" + "---:|" * len(cols)]
    for metric in TLDR_METRICS:
        cells = []
        for i, (_name, e) in enumerate(cols):
            text = tldr_value(e, metric)
            if i == 0 and classified is not None:
                text += tldr_marker(cur, entries[-2], metric)
            cells.append(text)
        out.append(f"| {tldr_label(metric, headline)} | " + " | ".join(cells) + " |")
    if classified is not None:
        improved = tldr_movements(classified["improved"], headline)
        worse = tldr_movements(classified["worse"], headline)
        accepted = tldr_movements(classified["accepted"], headline)
        out += ["",
                "Biggest improvements: " + ("; ".join(improved) if improved else "none."),
                "Biggest regressions: " + ("; ".join(worse) if worse else "none."),
                "Accepted regressions: " + ("; ".join(accepted) if accepted else "none.")]
    out += [""] + tldr_hard_lines(entries)
    return out


def latest_section(entries: list[dict]) -> list[str]:
    cur = entries[-1]
    m = cur["meta"]
    out = ["", f"## Latest change: `{m['commit'][:7]}` {short(m['subject'], 100)}", ""]
    if len(entries) < 2:
        return out + ["This is the first recorded change; there is nothing to compare it with."]
    prev = entries[-2]
    c = classify(prev, cur)
    tot = lambda rows: sum(1 for r in rows if r[1] == "total")  # noqa: E731
    out.append(f"Compared with `{prev['meta']['commit'][:7]}`: {tot(c['improved'])} total metrics "
               f"better, {tot(c['worse'])} worse, {tot(c['accepted'])} worse and accepted. "
               "Rows marked `total` sum the gated datasets; the other rows name one dataset.")
    for title, key, reason in (("Better", "improved", False), ("Worse", "worse", False),
                               ("Worse and accepted", "accepted", True)):
        out += ["", f"### {title}", ""]
        out += change_rows(c[key], reason) if c[key] else ["None."]
    return out


def all_metrics_section(entries: list[dict], last: int) -> list[str]:
    recent = entries[-last:]
    metrics = []
    for e in recent:
        for comp, block in e["scorecard"]["totals"].items():
            for name in block:
                if f"{comp}.{name}" not in metrics:
                    metrics.append(f"{comp}.{name}")
    out = ["", f"## Every total metric, last {len(recent)} changes", "",
           "Oldest on the left. A value in a column is followed by its movement "
           "against the column before it.", "",
           "| metric | reading | " + " | ".join(f"`{e['meta']['commit'][:7]}`" for e in recent) + " |",
           "|---|---|" + "---:|" * len(recent)]
    first_index = len(entries) - len(recent)
    for metric in metrics:
        cells = []
        for k, e in enumerate(recent):
            i = first_index + k
            cells.append(cell(e, entries[i - 1] if i else None, metric))
        out.append(f"| {metric} | {direction_note(spec_of(recent[-1], metric))} | "
                   + " | ".join(cells) + " |")
    return out


def datasets_section(entry: dict, headline: list[dict]) -> list[str]:
    out = ["", "## Latest scorecard per dataset", "",
           "| dataset | kind | " + " | ".join(h["label"] for h in headline) + " |",
           "|---|---|" + "---:|" * len(headline)]
    for ds, rec in entry["scorecard"]["datasets"].items():
        name = ds + (" (held out)" if rec.get("held_out") else "")
        if rec.get("status") != "ok":
            out.append(f"| {name} | {rec.get('status')}: {short(rec.get('reason', ''), 60)} |"
                       + " |" * len(headline))
            continue
        cells = []
        for h in headline:
            comp, metric = h["metric"].split(".", 1)
            v = rec["components"].get(comp, {}).get("metrics", {}).get(metric)
            cells.append(cmp.fmt(v))
        out.append(f"| {name} | {rec.get('kind', '')} | " + " | ".join(cells) + " |")
    return out


def render(args) -> int:
    entries = load_history(Path(args.history))
    if not entries:
        print(f"no history entries in {args.history}", file=sys.stderr)
        return 2
    headline = headline_spec()
    latest = entries[-1]
    prov = latest["scorecard"]["provenance"]
    md = ["# Gate report"]
    md += tldr_section(entries, headline)
    md += ["",
          f"{len(entries)} change(s) recorded. Latest: `{latest['meta']['commit'][:7]}` "
          f"({latest['meta']['date'][:10]}), onceover {prov.get('onceover_version')}, "
          f"seeds {prov.get('seeds')}.", "",
          "Each row is one change on main, newest first. A value is followed, in "
          "parentheses, by its movement against the change before it, with `better` or "
          "`worse` when the metric has a direction. Seconds are compared only between "
          "rows produced on the same platform.", "",
          "## Headline metrics", ""]
    md += ["- **" + h["label"] + "**: " + h.get("meaning", "") + " ("
           + direction_note(spec_of(latest, h["metric"])) + ")." for h in headline]
    md += [""] + headline_table(entries, headline)
    md += latest_section(entries)
    md += all_metrics_section(entries, args.last)
    md += datasets_section(latest, headline)
    if any(e["scorecard"].get("held_out_totals") for e in entries):
        md += ["", "## Held-out datasets", "",
               "These datasets are scored and left out of the pass/fail comparison. "
               "A gain on the gated datasets that these do not share points to tuning "
               "on the gated datasets.", ""]
        md += headline_table(entries, headline, held_out=True)
    platforms = sorted({e["scorecard"]["provenance"].get("platform", "?") for e in entries})
    md += ["", "## Platforms", ""] + [f"- {p}" for p in platforms]
    text = "\n".join(md) + "\n"
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(text, encoding="utf-8")
    print(f"report: {args.out} ({len(entries)} entries)")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="command", required=True)
    a = sub.add_parser("add", help="store a scorecard as a history entry")
    a.add_argument("scorecard")
    a.add_argument("--history", required=True)
    a.add_argument("--commit", required=True)
    a.add_argument("--date", required=True, help="commit date, ISO 8601")
    a.add_argument("--subject", required=True, help="commit subject line")
    a.add_argument("--accepted", default=str(cmp.ACCEPTED),
                   help='accepted file in force at this commit, or "none"')
    a.set_defaults(func=add)
    r = sub.add_parser("render", help="write the markdown report from the history")
    r.add_argument("--history", required=True)
    r.add_argument("--out", required=True)
    r.add_argument("--last", type=int, default=8,
                   help="changes shown in the every-metric table (default 8)")
    r.set_defaults(func=render)
    args = ap.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
