"""onceover: first look at tabular data.

A native Rust profiler for Polars DataFrames.
"""

import json as _json

from onceover._native import ProfileReport as _NativeProfileReport
from onceover._native import profile as _profile_native

__version__ = "0.1.2"
__all__ = ["profile", "ProfileReport", "available_detectors"]

_SCHEMA_VERSION = "0.2"


def profile(df, budget: str = "standard", deadline_seconds: float | None = None, *,
            sample_rows: int | None = None, max_predictors: int | None = None,
            max_pairs: int | None = None, detectors: list[str] | None = None,
            max_findings: int = 100, max_rules: int = 20,
            hints: dict | None = None, target: str | None = None):
    """Give a Polars DataFrame a once-over.

    Parameters
    ----------
    df : polars.DataFrame
        The table to profile (Arrow interchange; Polars 1.28–2.x).
    budget : str
        "quick" (summaries and integrity checks), "standard" (default passes with caps),
        or "deep" (wider caps and deeper search for those passes).
    deadline_seconds : float, optional
        Soft wall-clock budget. Deterministic by default (None); when set,
        remaining passes may be skipped and reported in `coverage`.

    sample_rows : int, optional
        Override row samples for relationship searches. Column summaries and
        identity verification still examine all rows. Small samples can leave
        too few rows for model fitting; check coverage.
    max_predictors : int, optional
        Predictors retained per model after screening all usable columns.
    max_pairs : int, optional
        Detailed numeric pairs retained after screening all numeric pairs.

    detectors : list[str], optional
        Detector IDs to select; None selects the default set, which excludes
        the experimental "views" pass. Required detectors can run as resources
        without contributing their own findings.
    max_findings, max_rules : int
        Report-wide selection limits, applied across all contributing methods.
    hints : dict, optional
        Declared structure the profiler cannot infer, checked exactly by
        the "expectations" pass (evidence tagged
        ``reference_source="declared"``). Keys: ``time`` (axis column),
        ``order_by`` (columns rows are ordered by), ``no_gaps`` (columns
        expected to form a complete sequence), ``cadence`` (per-column:
        "daily", "business_day", "weekly", "monthly", or a day/step
        count), ``dimensions`` and ``measures`` (panel structure).
        Unknown columns or unusable types are reported in the pass
        coverage, not raised.
    target : str, optional
        Designate a response column for the opt-in target-aware mode.
        Adds the ``target_shapes`` pass, which reports the shape of
        each numeric predictor's relationship to the target (linear /
        binned-curve / single-step R² shares) where a shape is
        decisive. Every other detector behaves exactly as without it.
        When ``detectors`` is given explicitly, ``target_shapes``
        runs only if listed. An unknown or non-numeric target is
        reported in the pass coverage, not raised.

    Returns
    -------
    ProfileReport
        Ranked findings ledger + per-column cards + coverage block.
    """
    return ProfileReport(_profile_native(df, budget, deadline_seconds, sample_rows=sample_rows,
                                         max_predictors=max_predictors, max_pairs=max_pairs, detectors=detectors,
                                         max_findings=max_findings, max_rules=max_rules,
                                         hints=_json.dumps(hints) if hints is not None else None,
                                         target=target))


def available_detectors():
    """Available detector IDs, produced categories, and resource dependencies."""
    from onceover._native import available_detectors as _available
    return _json.loads(_available())


class ProfileReport:
    """Profiling results and evidence.

    Access the full versioned JSON via `.data` (dict), the ranked findings
    via `.findings`, filtered findings via `.select(category=, type=,
    column=, target=)` and one finding by id via `.finding(id)`, the same
    findings as a Polars DataFrame via `.findings_frame()`, a
    context-budgeted digest via `.to_digest(chars)`, a size-budgeted
    JSON view via `.to_json(max_chars=...)`, the one-glance summary via
    `.overview`, per-column cards via `.columns`, and pass coverage
    via `.coverage`. The report's contract is `docs/report-schema.json`.
    """

    def __init__(self, native: _NativeProfileReport):
        self._native = native
        self._data = None

    def _ensure(self):
        if self._data is None:
            self._data = _json.loads(self._native.to_json())

    @property
    def data(self) -> dict:
        """The complete report as a (schema-versioned) dict."""
        self._ensure()
        return self._data

    @property
    def overview(self) -> dict:
        self._ensure()
        return self._data["overview"]

    @property
    def findings(self) -> list:
        """The ranked findings ledger (highest interestingness first)."""
        self._ensure()
        return self._data["findings"]

    @property
    def columns(self) -> dict:
        self._ensure()
        return self._data["columns"]

    @property
    def coverage(self) -> dict:
        """What was tested, at what scale, what was skipped and why."""
        self._ensure()
        return self._data["coverage"]

    @property
    def groups(self) -> list:
        """Connected findings; group membership does not imply redundancy."""
        self._ensure()
        return self._data["groups"]

    def finding(self, finding_id: str):
        """One finding by its stable id, or None if the report has no
        finding with that id. The returned dict is the live report
        entry (the same object `.findings` carries)."""
        for f in self.findings:
            if f.get("id") == finding_id:
                return f
        return None

    def select(self, category=None, type=None, column=None, target=None) -> list:
        """Findings matching every given filter, in report order.

        `category` and `type` match the finding's fields exactly
        (note `type` is absent for single-kind categories);
        `column` matches membership in the finding's subject columns;
        `target` matches the subject target. With no filters, returns
        all findings. The dicts are the live report entries."""
        out = []
        for f in self.findings:
            if category is not None and f.get("category") != category:
                continue
            if type is not None and f.get("type") != type:
                continue
            subject = f.get("subject") or {}
            if column is not None and column not in (subject.get("columns") or []):
                continue
            if target is not None and subject.get("target") != target:
                continue
            out.append(f)
        return out

    def findings_frame(self):
        """The findings ledger as a Polars DataFrame, one row per finding.

        Columns: id, category, type, columns, inputs, target,
        interestingness, claim, n_evidence, basis — plus
        detector_interestingness when the report carries it. Rows are in
        report order (highest interestingness first). The full evidence
        records stay in `.findings` / `.data`; this frame is the
        sortable, filterable index into them.
        """
        import polars as pl

        findings = self.findings
        has_detector = any("detector_interestingness" in f for f in findings)
        rows = []
        for f in findings:
            subject = f.get("subject") or {}
            evidence = f.get("evidence") or []
            row = {
                "id": f.get("id"),
                "category": f.get("category"),
                "type": f.get("type"),
                "columns": list(subject.get("columns") or []),
                "inputs": list(subject.get("inputs") or []),
                "target": subject.get("target"),
                "interestingness": f.get("interestingness"),
                "claim": f.get("claim"),
                "n_evidence": len(evidence),
                "basis": evidence[0].get("basis") if evidence else None,
            }
            if has_detector:
                row["detector_interestingness"] = f.get("detector_interestingness")
            rows.append(row)
        if not rows:
            schema = {
                "id": pl.String,
                "category": pl.String,
                "type": pl.String,
                "columns": pl.List(pl.String),
                "inputs": pl.List(pl.String),
                "target": pl.String,
                "interestingness": pl.Float64,
                "claim": pl.String,
                "n_evidence": pl.UInt32,
                "basis": pl.String,
            }
            if has_detector:
                schema["detector_interestingness"] = pl.Float64
            return pl.DataFrame(schema=schema)
        return pl.DataFrame(rows)

    def to_json(self, max_chars: int | None = None) -> str:
        """The report as JSON. With `max_chars`, a size-budgeted view:
        findings are kept in full in rank order until the budget is
        reached; the rest collapse to `finding_index` entries (id,
        category, type, columns, interestingness, claim). If that is
        still too large, column cards collapse to `column_names`, and
        finally coverage detail strings are truncated. A `budget`
        record in the output states what was cut; the view never
        exceeds `max_chars`. Raises ValueError if `max_chars` is
        below the minimal skeleton. The full report stays available
        via `.data` / `to_json()`."""
        if max_chars is None:
            return self._native.to_json()
        import copy

        data = copy.deepcopy(self.data)
        total = len(data["findings"])
        index: list[dict] = []
        state = {"columns": "full", "coverage_detail": "full"}

        def render() -> str:
            view = {
                "schema_version": data["schema_version"],
                "overview": data["overview"],
                "columns": data["columns"],
                "findings": data["findings"],
                "finding_index": list(reversed(index)),
                "groups": data["groups"],
                "selection": data["selection"],
                "coverage": data["coverage"],
                "budget": {
                    "max_chars": max_chars,
                    "full_findings": len(data["findings"]),
                    "indexed_findings": len(index),
                    "columns": state["columns"],
                    "coverage_detail": state["coverage_detail"],
                },
            }
            if state["columns"] == "names_only":
                view["column_names"] = list(data["columns"].keys())
                view["columns"] = {}
            return _json.dumps(view)

        out = render()
        while len(out) > max_chars and data["findings"]:
            f = data["findings"].pop()
            subject = f.get("subject") or {}
            index.append(
                {
                    "id": f.get("id"),
                    "category": f.get("category"),
                    "type": f.get("type"),
                    "columns": list(subject.get("columns") or []),
                    "interestingness": f.get("interestingness"),
                    "claim": f.get("claim"),
                }
            )
            out = render()
        if len(out) > max_chars and state["columns"] == "full":
            state["columns"] = "names_only"
            out = render()
        if len(out) > max_chars:
            for entry in data["coverage"].values():
                detail = entry.get("detail") or ""
                if len(detail) > 160:
                    entry["detail"] = detail[:160] + " [truncated]"
            state["coverage_detail"] = "truncated"
            out = render()
        if len(out) > max_chars:
            # Coverage detail strings are the known size sink on wide
            # tables; the statuses alone still say what ran.
            for entry in data["coverage"].values():
                entry["detail"] = ""
            state["coverage_detail"] = "status_only"
            out = render()
        if len(out) > max_chars:
            raise ValueError(
                f"max_chars={max_chars} is below the minimal budgeted "
                f"report skeleton ({len(out)} chars with all {total} "
                "findings indexed)"
            )
        return out

    def to_markdown(self) -> str:
        return self._native.to_markdown()

    def to_digest(self, budget_chars: int = 4000) -> str:
        """A context-budgeted plain-text digest of this report.

        Header (shape, schema, headline, coverage exceptions,
        finding counts by category), then one line per rollup group
        — findings sharing category, type, form, and target — with
        the group's highest-interestingness claim. Groups are
        ordered by that claim's interestingness. When the rolled
        lines exceed `budget_chars`, the digest keeps as many as
        fit and ends with a line stating how many groups were
        omitted; it never exceeds the budget (budgets under ~200
        characters return a truncated header). The full record
        stays in `.data` / `to_json()`.
        """
        data = self.data
        findings = data["findings"]
        overview = data.get("overview", {})
        lines = [
            f"profile digest: {overview.get('n_rows', '?')} rows x "
            f"{overview.get('n_columns', '?')} columns, "
            f"{len(findings)} findings, schema "
            f"{data.get('schema_version', '?')}"
        ]
        if overview.get("headline"):
            lines.append("headline: " + overview["headline"])
        cov = data.get("coverage", {})
        skipped = [
            f"{k}={v.get('status')}"
            for k, v in cov.items()
            if v.get("status") not in ("ran_exact", "ran_sampled")
        ]
        lines.append(
            "coverage: " + (", ".join(skipped) if skipped else "all passes ran")
        )
        cats: dict[str, int] = {}
        for f in findings:
            cats[f["category"]] = cats.get(f["category"], 0) + 1
        lines.append(
            "findings by category: "
            + ", ".join(
                f"{c} {n}"
                for c, n in sorted(cats.items(), key=lambda kv: -kv[1])
            )
        )
        header = "\n".join(lines) + "\n"

        groups: dict[tuple, list[dict]] = {}
        for f in findings:
            subj = f.get("subject") or {}
            key = (
                f.get("category"),
                f.get("type"),
                (f.get("specification") or {}).get("form"),
                subj.get("target"),
            )
            groups.setdefault(key, []).append(f)
        rolled = []
        for (cat, typ, form, target), fs in groups.items():
            fs_sorted = sorted(fs, key=lambda x: -(x.get("interestingness") or 0))
            head = f"[{cat}] {typ or ''}{'/' + form if form else ''}"
            if target:
                head += f" -> {target}"
            if len(fs_sorted) > 1:
                head += f" (x{len(fs_sorted)})"
            rolled.append((fs_sorted[0].get("interestingness") or 0,
                           head + ": " + fs_sorted[0]["claim"]))
        rolled.sort(key=lambda kv: -kv[0])
        body_lines = [line for _, line in rolled]

        def omission(dropped: int) -> str:
            return (
                f"... [{dropped} rolled-up finding groups omitted to fit "
                f"{budget_chars} chars; {len(findings)} findings total]"
            )

        full = header + "\n" + "\n".join(body_lines) + "\n"
        if len(full) <= budget_chars:
            return full
        kept: list[str] = []
        used = len(header) + 1 + len(omission(len(body_lines))) + 1
        for ln in body_lines:
            if used + len(ln) + 1 > budget_chars:
                break
            kept.append(ln)
            used += len(ln) + 1
        dropped = len(body_lines) - len(kept)
        result = header + "\n" + "\n".join(kept + [omission(dropped)]) + "\n"
        if len(result) <= budget_chars:
            return result
        # Degrade the header itself: drop the category-count line,
        # then the coverage line, keeping the profile line.
        for keep in (2, 1):
            short_header = "\n".join(lines[:keep]) + "\n"
            result = short_header + omission(len(body_lines)) + "\n"
            if len(result) <= budget_chars:
                return result
        return (lines[0] + "\n")[:budget_chars]

    def save(self, path: str, fmt: str = "json") -> str:
        """Write the report to `path` as json or markdown. Returns the path."""
        text = self.to_json() if fmt == "json" else self.to_markdown()
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(text)
        return path

    def __repr__(self) -> str:
        return self._native.__repr__()

    def _repr_markdown_(self) -> str:
        return self._native.to_markdown()
