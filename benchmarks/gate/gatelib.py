"""Shared helpers for the gate: dataset loading, profiling with a cache,
digest parsing, and the finding vocabulary the components use.

Nothing here names a dataset, a plant or a component.
"""
from __future__ import annotations

import importlib.util
import json
import re
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

import polars as pl

GATE = Path(__file__).resolve().parent
BENCH = GATE.parent
ROOT = BENCH.parent
CORPUS = BENCH / "corpus"
QUALITY = BENCH / "quality"

# Files the quality battery and the corpus harness already provide are
# imported, not copied.
for _p in (str(QUALITY),):
    if _p not in sys.path:
        sys.path.insert(0, _p)


def _load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    spec.loader.exec_module(mod)
    return mod


corpus_run = _load_module("gate_corpus_run_benchmarks", CORPUS / "run_benchmarks.py")

import onceover  # noqa: E402  (after the path setup above)


# ---------------------------------------------------------------- registry

def load_registry() -> dict:
    return json.loads((GATE / "registry.json").read_text(encoding="utf-8"))


def corpus_entries() -> dict:
    reg = json.loads((CORPUS / "registry.json").read_text(encoding="utf-8"))
    return {e["id"]: e for e in reg["datasets"]}


def resolve_target(df: pl.DataFrame, target) -> str | None:
    """A target is a column name, "@last" (the last column), or null."""
    if target is None:
        return None
    if target == "@last":
        return df.columns[-1]
    if target not in df.columns:
        raise ValueError(f"target {target!r} is not a column")
    return target


# ---------------------------------------------------------------- findings

# Categories whose findings state a relationship between columns.
RELATION_CATEGORIES = {
    "dependencies", "associations", "conditional_rules", "intervals",
    "changes", "missingness", "concentration", "anomalies",
}
# A statement of this type is a relationship wherever it is filed.
RELATION_TYPES = {"duplicate_encoding"}


def finding_columns(f: dict) -> set:
    """Columns a finding states something about: its subject columns plus
    its target. A model-based finding (predictive) lists every input of
    the model in its subject; the columns it states are the target and
    the drivers named in its first evidence block."""
    subject = f.get("subject") or {}
    cols = set(subject.get("columns") or [])
    for ev in (f.get("evidence") or [])[:1]:
        drivers = (ev.get("metrics") or {}).get("drivers")
        if isinstance(drivers, list) and drivers and all(isinstance(d, str) for d in drivers):
            cols = set(drivers)
    if subject.get("target"):
        cols.add(subject["target"])
    return cols


def is_relationship(f: dict) -> bool:
    """A finding that states something about two or more columns together."""
    if len(finding_columns(f)) < 2:
        return False
    return f.get("category") in RELATION_CATEGORIES or f.get("type") in RELATION_TYPES


def fact_key(f: dict, rename: dict | None = None) -> tuple:
    """Identity of a stated fact: category, type and the sorted columns,
    after mapping column names back through `rename` (new -> original)."""
    rename = rename or {}
    cols = sorted(rename.get(c, c) for c in finding_columns(f))
    return (f.get("category"), f.get("type"), tuple(cols))


# ---------------------------------------------------------------- digest

_LINE = re.compile(
    r"^\[(?P<cat>[^\]]+)\] (?P<type>[^\s/:]+)(?:/(?P<sub>\S+))?"
    r"(?: -> (?P<target>.+?))?(?: \(x(?P<n>\d+)\))?: (?P<rest>.*)$")
_EXEMPLARS = re.compile(r" \[exemplars: (?P<ids>[^\]]*)\]$")
# The briefing line format (issue #317 phase 2): the claim in plain
# words, followed by the one finding id that speaks for the fact.
_BRIEFING_LINE = re.compile(r"^(?P<claim>.*) \[(?P<fid>[^\[\]\s]+)\]$")
_LONG_DECIMAL = re.compile(r"\d+\.\d{6,}")
# Lines of the digest that are not statements about the data.
NON_STATEMENT_PREFIXES = ("coverage:", "findings by category:", "profile digest:", "...")


@dataclass
class DigestLine:
    text: str
    is_finding: bool
    category: str | None = None
    type: str | None = None
    count: int = 1
    claim: str = ""
    lead: dict | None = None
    members: list = field(default_factory=list)  # lead plus exemplars

    @property
    def columns(self) -> set:
        return finding_columns(self.lead) if self.lead else set()

    @property
    def kind(self) -> tuple:
        return (self.category, self.type)


def parse_digest(text: str, data: dict) -> list[DigestLine]:
    """Split a digest into lines and resolve each finding line to the
    report's findings: by claim text for the lead, by id for exemplars.

    Two line formats are recognised. The rolled-up format names the
    category and type in brackets and lists exemplar ids; the
    briefing format (issue #317 phase 2) prints the claim in plain
    words followed by the single finding id that speaks for the
    fact. A briefing line resolves to that finding plus its fact
    mates — every finding stating the same set of columns, and the
    grouped members those findings lead — because the briefing
    merges a fact's findings into the one line before rendering.
    The gate compares a base build against a head build, and the
    two can print different formats, so both stay readable here."""
    by_id, by_claim = {}, {}
    for f in list(data.get("findings") or []) + list(data.get("grouped_findings") or []):
        by_id[f["id"]] = f
        by_claim.setdefault(f.get("claim") or "", f)
    grouped_by_id = {f["id"]: f for f in data.get("grouped_findings") or []}
    by_fact: dict = {}
    for f in data.get("findings") or []:
        cols = finding_columns(f)
        if cols:
            by_fact.setdefault(frozenset(cols), []).append(f)
    mates: dict = {}
    for group in by_fact.values():
        members = list(group)
        for f in group:
            for mid in f.get("grouped_members") or []:
                if mid in grouped_by_id:
                    members.append(grouped_by_id[mid])
        for f in group:
            mates[f["id"]] = members
    out = []
    for raw in text.splitlines():
        m = _BRIEFING_LINE.match(raw)
        if m and m.group("fid") in by_id:
            lead = by_id[m.group("fid")]
            members = list(mates.get(lead["id"]) or [lead])
            if lead not in members:
                members.insert(0, lead)
            out.append(DigestLine(
                text=raw, is_finding=True, category=lead.get("category"),
                type=lead.get("type"), count=1, claim=m.group("claim"),
                lead=lead, members=members))
            continue
        if not raw.startswith("["):
            out.append(DigestLine(text=raw, is_finding=False))
            continue
        m = _LINE.match(raw)
        if not m:
            out.append(DigestLine(text=raw, is_finding=True))
            continue
        rest = m.group("rest")
        ex = _EXEMPLARS.search(rest)
        ids = []
        if ex:
            ids = [i.strip() for i in ex.group("ids").split(",") if i.strip()]
            rest = rest[:ex.start()]
        lead = by_claim.get(rest)
        members = [by_id[i] for i in ids if i in by_id]
        if lead is None and members:
            lead = members[0]
        if lead is not None and lead not in members:
            members.insert(0, lead)
        out.append(DigestLine(
            text=raw, is_finding=True, category=m.group("cat"),
            type=m.group("type"), count=int(m.group("n") or 1), claim=rest,
            lead=lead, members=members))
    return out


def statement_lines(lines: list[DigestLine]) -> list[str]:
    """Digest lines that carry statements (for text-pattern matching)."""
    return [l.text for l in lines if l.text and not l.text.startswith(NON_STATEMENT_PREFIXES)]


def long_decimals(text: str) -> int:
    """Numbers printed with six or more decimals."""
    return len(_LONG_DECIMAL.findall(text))


# ---------------------------------------------------------------- profiling

@dataclass
class Profiled:
    report: object
    data: dict
    seconds: float
    _digests: dict = field(default_factory=dict)

    @property
    def findings(self) -> list:
        return self.data.get("findings") or []

    def digest(self, chars: int) -> str:
        if chars not in self._digests:
            self._digests[chars] = self.report.to_digest(chars)
        return self._digests[chars]

    def lines(self, chars: int) -> list[DigestLine]:
        return parse_digest(self.digest(chars), self.data)


class Context:
    """What a component receives for one dataset."""

    def __init__(self, entry: dict, df: pl.DataFrame, corpus: dict, seeds: list,
                 fast: bool = False):
        self.entry = entry
        self.id = entry["id"]
        self.df = df
        self.target = resolve_target(df, entry.get("target"))
        self.hints = entry.get("hints", corpus.get("hints"))
        self.detectors = entry.get("detectors")
        self.budget = entry.get("budget", "standard")
        self.seeds = seeds
        self.fast = fast
        self.options = (entry.get("options") or {})
        self._cache: dict = {}

    def option(self, component: str, key: str, default=None):
        return (self.options.get(component) or {}).get(key, default)

    def profile(self, df: pl.DataFrame | None = None, target: str | None = None,
                hints: dict | None = None, key: str | None = None) -> Profiled:
        """Profile `df` (the dataset itself by default). Results for the
        dataset itself are cached by (target)."""
        own = df is None
        cache_key = ("own", target) if own else key
        if cache_key is not None and cache_key in self._cache:
            return self._cache[cache_key]
        frame = self.df if own else df
        kw = {"budget": self.budget}
        if self.detectors:
            kw["detectors"] = self.detectors
        h = hints if hints is not None else self.hints
        if h:
            kw["hints"] = h
        if target:
            kw["target"] = target
        t0 = time.perf_counter()
        report = onceover.profile(frame, **kw)
        secs = time.perf_counter() - t0
        prof = Profiled(report=report, data=report.data, seconds=secs)
        if cache_key is not None:
            self._cache[cache_key] = prof
        return prof

    def has_time_axis(self) -> bool:
        """True when the report's frame model finds a timestamp column or
        the dataset declares a time hint."""
        if (self.hints or {}).get("time"):
            return True
        roles = (self.profile().data.get("frame") or {}).get("roles") or []
        return any(r.get("role") == "timestamp" for r in roles)


def build_context(entry: dict, seeds: list, fast: bool = False):
    """Load a gate dataset entry through the corpus harness. Returns
    (Context, None) or (None, (status, reason))."""
    corpus = corpus_entries().get(entry["corpus"])
    if corpus is None:
        return None, ("error", f"corpus dataset {entry['corpus']!r} is not in the corpus registry")
    status, payload = corpus_run.locate(corpus)
    if status != "ok":
        return None, (status, payload)
    df = corpus_run.read_entry({**corpus, **(entry.get("read") or {})}, payload)
    return Context(entry, df, corpus, seeds, fast), None


def provenance(commit: str) -> dict:
    import platform
    out = {
        "onceover_version": onceover.__version__,
        "commit": commit,
        "python": sys.version.split()[0],
        "polars": pl.__version__,
        "platform": platform.platform(),
    }
    try:
        import numpy as np
        out["numpy"] = np.__version__
    except ImportError:
        pass
    return out


def dump(obj, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj, indent=1, sort_keys=False) + "\n", encoding="utf-8")
