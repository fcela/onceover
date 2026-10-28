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
            max_findings: int = 100, max_rules: int = 20):
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

    Returns
    -------
    ProfileReport
        Ranked findings ledger + per-column cards + coverage block.
    """
    return ProfileReport(_profile_native(df, budget, deadline_seconds, sample_rows=sample_rows,
                                         max_predictors=max_predictors, max_pairs=max_pairs, detectors=detectors,
                                         max_findings=max_findings, max_rules=max_rules))


def available_detectors():
    """Available detector IDs, produced categories, and resource dependencies."""
    from onceover._native import available_detectors as _available
    return _json.loads(_available())


class ProfileReport:
    """Profiling results and evidence.

    Access the full versioned JSON via `.data` (dict), the ranked findings
    via `.findings`, the same findings as a Polars DataFrame via
    `.findings_frame()`, the one-glance summary via `.overview`,
    per-column cards via `.columns`, and pass coverage via `.coverage`.
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

    def to_json(self) -> str:
        return self._native.to_json()

    def to_markdown(self) -> str:
        return self._native.to_markdown()

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
