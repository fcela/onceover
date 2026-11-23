"""Thin Python wrapper around the native Rust `ProfileReport`."""

import json as _json

from onceover._native import (
    budgeted_report_json as _budgeted_report_json,
    compact_report_json as _compact_report_json,
    diff_reports_json as _diff_reports_json,
    finding_slot_from_json as _finding_slot_from_json,
    render_digest_json as _render_digest_json,
    select_from_json as _select_from_json,
)


def _findings_frame_schema(*, with_detector: bool = False):
    """Single source of truth for findings_frame()'s Polars schema."""
    import polars as pl

    schema = {
        "id": pl.String,
        "category": pl.String,
        "type": pl.String,
        "scope": pl.String,
        "object": pl.String,
        "support_kind": pl.String,
        "conditionality": pl.String,
        "temporality": pl.String,
        "quality_dimension": pl.String,
        "status": pl.String,
        "columns": pl.List(pl.String),
        "inputs": pl.List(pl.String),
        "target": pl.String,
        "interestingness": pl.Float64,
        "claim": pl.String,
        "n_evidence": pl.UInt32,
        "basis": pl.String,
    }
    if with_detector:
        schema["detector_interestingness"] = pl.Float64
    return schema


class ProfileReport:
    """Profiling results and evidence.

    All rendering, compacting, budgeting, finding lookup, filtering, and
    diffing execute in Rust (`onceover-report`); this wrapper exposes
    ergonomic Python properties and Polars DataFrame views.
    """

    def __init__(self, native):
        self._native = native
        self._data = None

    def _ensure(self):
        if self._data is None:
            self._data = _json.loads(self._native.to_json())

    def _has_native(self) -> bool:
        return getattr(self, "_native", None) is not None

    @property
    def data(self) -> dict:
        self._ensure()
        return self._data

    @property
    def overview(self) -> dict:
        return self.data["overview"]

    @property
    def findings(self) -> list:
        return self.data["findings"]

    @property
    def grouped_findings(self) -> list:
        return self.data.get("grouped_findings") or []

    @property
    def columns(self) -> dict:
        return self.data["columns"]

    @property
    def coverage(self) -> dict:
        return self.data["coverage"]

    @property
    def groups(self) -> list:
        return self.data["groups"]

    @property
    def source(self) -> dict | None:
        return self.data.get("source")

    def finding(self, finding_id: str):
        """One finding by stable id or short report-local handle (`"f1"`, `"g1"`)."""
        if not isinstance(finding_id, str):
            return None
        if self._has_native():
            slot = self._native.finding_slot(finding_id)
        else:
            slot = _finding_slot_from_json(_json.dumps(self.data), finding_id)
        if slot is None:
            return None
        is_grouped, idx = slot
        return self.grouped_findings[idx] if is_grouped else self.findings[idx]

    def select(
        self,
        *,
        category: str | None = None,
        type: str | None = None,
        column: str | None = None,
        target: str | None = None,
        scope: str | None = None,
        object: str | None = None,
        support: str | None = None,
        conditionality: str | None = None,
        temporality: str | None = None,
        quality_dimension: str | None = None,
        status: str | None = None,
        include_grouped: bool = False,
    ) -> list:
        """Findings matching all supplied criteria (evaluated in Rust)."""
        kw = dict(
            category=category,
            type=type,
            column=column,
            target=target,
            scope=scope,
            object=object,
            support=support,
            conditionality=conditionality,
            temporality=temporality,
            quality_dimension=quality_dimension,
            status=status,
            include_grouped=include_grouped,
        )
        if self._has_native():
            return _json.loads(self._native.select(**kw))
        return _json.loads(_select_from_json(_json.dumps(self.data), **kw))

    def diff(self, other) -> dict:
        if self._has_native() and other._has_native():
            return _json.loads(self._native.diff(other._native))
        return _json.loads(
            _diff_reports_json(_json.dumps(self.data), _json.dumps(other.data))
        )

    def findings_frame(self):
        import polars as pl

        findings = self.findings
        has_detector = any("detector_interestingness" in f for f in findings)
        schema = _findings_frame_schema(with_detector=has_detector)
        rows = []
        for f in findings:
            subject = f.get("subject") or {}
            evidence = f.get("evidence") or []
            facets = f.get("facets") or {}
            row = {
                "id": f.get("id"),
                "category": f.get("category"),
                "type": f.get("type"),
                "scope": facets.get("scope"),
                "object": facets.get("object"),
                "support_kind": facets.get("support"),
                "conditionality": facets.get("conditionality"),
                "temporality": facets.get("temporality"),
                "quality_dimension": facets.get("quality_dimension"),
                "status": facets.get("status"),
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
        return pl.DataFrame(rows, schema=schema)

    def associations(self):
        import polars as pl

        schema = {
            "a": pl.String,
            "b": pl.String,
            "kind": pl.String,
            "n": pl.UInt64,
            "pearson": pl.Float64,
            "spearman": pl.Float64,
            "xi_ab": pl.Float64,
            "xi_ba": pl.Float64,
            "cramers_v": pl.Float64,
            "eta": pl.Float64,
            "p_value": pl.Float64,
            "q_value": pl.Float64,
            "tier": pl.String,
        }
        rows = self.data.get("associations") or []
        return pl.DataFrame(rows, schema=schema) if rows else pl.DataFrame(schema=schema)

    def to_json(self, max_chars: int | None = None, *, compact: bool = False) -> str:
        """The report as JSON, optionally compact and/or size-budgeted (in Rust)."""
        if self._has_native():
            return self._native.to_json(compact=compact, max_chars=max_chars)
        raw = _json.dumps(self.data)
        if max_chars is not None:
            return _budgeted_report_json(raw, max_chars=max_chars, compact=compact)
        if compact:
            return _compact_report_json(raw)
        return raw

    def to_markdown(self) -> str:
        return self._native.to_markdown()

    def to_digest(
        self,
        budget_chars: int = 4000,
        *,
        compact: bool = False,
        include_schema: bool = False,
    ) -> str:
        """Context-budgeted plain-text briefing digest (rendered in Rust)."""
        if self._has_native():
            return self._native.to_digest(
                budget_chars=budget_chars,
                compact=compact,
                include_schema=include_schema,
            )
        return _render_digest_json(
            _json.dumps(self.data),
            budget_chars=budget_chars,
            compact=compact,
            include_schema=include_schema,
        )

    def to_brief(self, budget_chars: int = 2500) -> str:
        """Token-economical LLM briefing: schema strip + `[f1]` handles (in Rust)."""
        if self._has_native():
            return self._native.to_brief(budget_chars=budget_chars)
        return _render_digest_json(
            _json.dumps(self.data),
            budget_chars=budget_chars,
            compact=True,
            include_schema=True,
        )

    def verify(self, df):
        from . import _checks

        data = self.data
        return _checks.verify(
            [*data["findings"], *(data.get("grouped_findings") or [])],
            df,
            source=data.get("source"),
        )

    def rows(self, finding_id: str, df):
        from . import _checks

        finding = self.finding(finding_id)
        if finding is None:
            raise ValueError(f"no finding with id {finding_id}")
        return _checks.rows_of(finding, df, source=self.data.get("source"))

    def to_checks(self, fmt: str = "polars", name: str = "onceover"):
        from . import _checks

        data = self.data
        findings = [*data["findings"], *(data.get("grouped_findings") or [])]
        if fmt == "polars":
            return _checks.constraints(findings)
        if fmt == "great_expectations":
            return _checks.great_expectations_suite(findings, name)
        raise ValueError(
            f"unknown format {fmt!r}; use polars or great_expectations"
        )

    def to_html(self) -> str:
        from . import _html, __version__

        return _html.render(self.data, version=__version__)

    def save(self, path: str, fmt: str = "json") -> str:
        if fmt == "json":
            text = self.to_json()
        elif fmt == "markdown":
            text = self.to_markdown()
        elif fmt == "html":
            text = self.to_html()
        else:
            raise ValueError(
                f"unknown format {fmt!r}; use json, markdown or html"
            )
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(text)
        return path

    def __repr__(self) -> str:
        return self._native.__repr__()

    def _repr_markdown_(self) -> str:
        return (
            self.to_digest()
            + "\nFull report: `report.to_markdown()`; a larger "
            "digest: `report.to_digest(n)`; a subset of findings: "
            "`report.select(...)`.\n"
        )
