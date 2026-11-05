"""onceover: first look at tabular data.

A native Rust profiler for Polars DataFrames.
"""

import json as _json

from onceover._native import ProfileReport as _NativeProfileReport
from onceover._native import profile as _profile_native

__version__ = "0.1.3"
__all__ = ["profile", "ProfileReport", "available_detectors"]

_SCHEMA_VERSION = "0.7"


def profile(df, budget: str = "standard", deadline_seconds: float | None = None, *,
            sample_rows: int | None = None, max_predictors: int | None = None,
            max_pairs: int | None = None, detectors: list[str] | None = None,
            max_findings: int = 100, max_rules: int = 20,
            hints: dict | None = None, target: str | None = None,
            timing: bool = False):
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
        Detector IDs to select; None selects the default set (27 of the
        34 registered detectors). Excluded unless requested: the opt-in
        detectors "views" (experimental), "dates", "pivot",
        "period_associations", "drift" and "granularity" — each records
        a skipped coverage entry in default runs — and "target_shapes",
        which runs when a target is designated. "expectations" is in
        the default set but engages only when `hints` are declared.
        Required detectors can run as resources without contributing
        their own findings. `available_detectors()` reports each
        detector's activation.
    max_findings, max_rules : int
        Report-wide selection limits, applied across all contributing methods.
    hints : dict, optional
        Declared structure the profiler cannot infer, checked exactly by
        the "expectations" pass (evidence tagged
        ``reference_source="declared"``). Keys: ``time`` (axis column),
        ``order_by`` (columns rows are ordered by), ``no_gaps`` (columns
        expected to form a complete sequence), ``cadence`` (per-column:
        "daily", "business_day", "weekly", "monthly", or a day/step
        count), ``dimensions`` and ``measures`` (panel structure), and
        ``roles`` (per-column semantic role override for the frame
        model: one of "identifier", "index", "timestamp",
        "timestamp_part", "dimension", "measure", "text").
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
    timing : bool, optional
        Record per-pass wall time as ``elapsed_ms`` in each coverage
        entry: the interval between the previous entry being recorded
        and this one (passes run sequentially; the first entry also
        carries extraction and the column cards). Opt-in because
        timing makes the coverage section nondeterministic; findings
        are identical either way.

    Returns
    -------
    ProfileReport
        Ranked findings ledger + per-column cards + coverage block.
    """
    return ProfileReport(_profile_native(df, budget, deadline_seconds, sample_rows=sample_rows,
                                         max_predictors=max_predictors, max_pairs=max_pairs, detectors=detectors,
                                         max_findings=max_findings, max_rules=max_rules,
                                         hints=_json.dumps(hints) if hints is not None else None,
                                         target=target, timing=timing))


def _report_band(finding: dict) -> int:
    """The report band of a finding (frame-model step 2), mirroring
    the Rust ledger's ordering: 0 account (frame-scope statements
    and keys), 1 completeness and frame integrity, 2 relations,
    3 column structure, 4 deviations."""
    if (finding.get("facets") or {}).get("scope") == "frame":
        return 0
    cat = finding.get("category")
    if cat == "uniqueness":
        return 0
    if cat in ("missingness", "gaps", "duplicates"):
        return 1
    if cat in ("dependencies", "associations", "conditional_rules"):
        return 2
    if cat in ("column_roles", "formats", "ordering", "concentration"):
        return 3
    return 4


def _quality_line(data: dict) -> str | None:
    """One line of exact completeness facts from the column cards and the
    duplicate-row statement, or None when there is nothing to state."""
    columns = data.get("columns") or {}
    gaps = [
        (card.get("null_fraction") or 0, name)
        for name, card in columns.items()
        if 0 < (card.get("null_fraction") or 0) < 1
    ]
    parts = []
    if gaps:
        rate, name = max(gaps, key=lambda g: (g[0], g[1]))
        parts.append(
            f"missing values in {len(gaps)} column(s), most in {name} ({rate:.1%})"
        )
    empty = sum(1 for card in columns.values() if card.get("role") == "all_null")
    constant = sum(1 for card in columns.values() if card.get("role") == "constant")
    if empty:
        parts.append(f"{empty} empty column(s)")
    if constant:
        parts.append(f"{constant} constant column(s)")
    for finding in data.get("findings") or []:
        if finding.get("type") == "duplicate_rows":
            claim = finding["claim"].split(" rows repeat", 1)[0]
            if claim.isdigit():
                parts.append(f"{claim} duplicate row(s)")
            break
    return "quality: " + "; ".join(parts) if parts else None


def _account_lines(frame: dict) -> list[str]:
    """The digest's account block, from the report's `frame`
    section: the grain statement, the time axis with its measured
    span and step, and each dominant structure's statement. Empty
    when the frame model computed no account (a frame with no axis,
    no key candidate, and no organising grouping)."""
    lines: list[str] = []
    grain = frame.get("grain") or {}
    if grain.get("statement"):
        lines.append("account: " + grain["statement"])
    axis = frame.get("axis") or {}
    if axis:
        line = "account: time axis — " + axis.get("description", axis.get("name", ""))
        if axis.get("min") and axis.get("max"):
            line += f", {axis['min']} to {axis['max']}"
            if axis.get("step"):
                line += f", in {axis['step']} steps"
            if axis.get("distinct_values"):
                line += f" ({axis['distinct_values']} distinct values)"
        lines.append(line)
    for structure in frame.get("dominant") or []:
        if structure.get("statement"):
            lines.append("account: " + structure["statement"])
    return lines


def available_detectors():
    """Available detector IDs, produced categories, resource dependencies,
    and activation: "default" (runs in the default set), "opt_in" (runs
    only when explicitly selected), "hint_gated" (default set, engages
    only on declared hints), or "target_gated" (runs when a target is
    designated)."""
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
    def grouped_findings(self) -> list:
        """Statements grouped under a verified relationship family's
        lead (schema 0.6): full finding records, each carrying
        `grouped_under` with the lead's id. They spend no digest
        slot; the lead finding names them in `grouped_members`.
        Empty when no family formed."""
        self._ensure()
        return self._data.get("grouped_findings") or []

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
        finding with that id. Selected findings are searched first,
        then the grouped statements (`grouped_findings`), so a lead's
        `grouped_members` always resolve. The returned dict is the
        live report entry (the same object `.findings` or
        `.grouped_findings` carries)."""
        for f in self.findings:
            if f.get("id") == finding_id:
                return f
        for f in self.grouped_findings:
            if f.get("id") == finding_id:
                return f
        return None

    def select(
        self,
        category=None,
        type=None,
        column=None,
        target=None,
        scope=None,
        object=None,
        evidence=None,
        conditionality=None,
        temporality=None,
        quality_dimension=None,
        status=None,
        include_grouped=False,
    ) -> list:
        """Findings matching every given filter, in report order.

        `category` and `type` match the finding's fields exactly;
        `column` matches membership in the finding's subject columns;
        `target` matches the subject target. The remaining arguments
        match the finding's facets exactly (`scope`, `object`,
        `evidence`, `conditionality`, `temporality`,
        `quality_dimension`, `status`); a facet filter never matches a
        finding that does not carry the facet. With no filters,
        returns all findings. With `include_grouped=True`, grouped
        statements (`grouped_findings`) are appended after the
        selected findings, in their own report order. The dicts are
        the live report entries."""
        facet_filters = {
            "scope": scope,
            "object": object,
            "evidence": evidence,
            "conditionality": conditionality,
            "temporality": temporality,
            "quality_dimension": quality_dimension,
            "status": status,
        }
        pool = list(self.findings)
        if include_grouped:
            pool.extend(self.grouped_findings)
        out = []
        for f in pool:
            if category is not None and f.get("category") != category:
                continue
            if type is not None and f.get("type") != type:
                continue
            facets = f.get("facets") or {}
            if any(
                want is not None and facets.get(name) != want
                for name, want in facet_filters.items()
            ):
                continue
            subject = f.get("subject") or {}
            if column is not None and column not in (subject.get("columns") or []):
                continue
            if target is not None and subject.get("target") != target:
                continue
            out.append(f)
        return out

    def diff(self, other) -> dict:
        """Compare this report (the base) against `other`.

        Findings are matched by their stable `id`, so the result
        states what a re-profile of the same table gained, lost, or
        changed: `added` (in `other`, not in base), `removed` (in
        base, not in `other`), and `changed` (same id, but the
        claim text or interestingness moved). Entries carry the
        finding's id, category, type, subject columns, claim, and
        interestingness, so the diff is readable without fetching
        either report; fetch the full finding with `.finding(id)`
        on the report that holds it. Coverage status changes are
        reported per pass. Evidence internals are not diffed: a
        finding whose evidence changed but whose claim and score
        held counts as unchanged.
        """
        def brief(f):
            subject = f.get("subject") or {}
            return {
                "id": f.get("id"),
                "category": f.get("category"),
                "type": f.get("type"),
                "columns": list(subject.get("columns") or []),
                "claim": f.get("claim"),
                "interestingness": f.get("interestingness"),
            }

        base = {f["id"]: f for f in self.findings}
        new = {f["id"]: f for f in other.findings}
        changed = []
        for fid in base.keys() & new.keys():
            a, b = base[fid], new[fid]
            sa, sb = a.get("interestingness") or 0.0, b.get("interestingness") or 0.0
            if a.get("claim") != b.get("claim") or abs(sa - sb) > 1e-9:
                changed.append(
                    {
                        "id": fid,
                        "category": b.get("category"),
                        "type": b.get("type"),
                        "columns": list((b.get("subject") or {}).get("columns") or []),
                        "claim": [a.get("claim"), b.get("claim")],
                        "interestingness": [sa, sb],
                    }
                )
        cov_a = (self.data or {}).get("coverage") or {}
        cov_b = (other.data or {}).get("coverage") or {}
        coverage_changes = {}
        for name in sorted(set(cov_a) | set(cov_b)):
            sa = (cov_a.get(name) or {}).get("status")
            sb = (cov_b.get(name) or {}).get("status")
            if sa != sb:
                coverage_changes[name] = [sa, sb]
        return {
            "added": [brief(new[fid]) for fid in new.keys() - base.keys()],
            "removed": [brief(base[fid]) for fid in base.keys() - new.keys()],
            "changed": sorted(changed, key=lambda c: -abs(c["interestingness"][1] - c["interestingness"][0])),
            "unchanged": len(base.keys() & new.keys()) - len(changed),
            "coverage_changes": coverage_changes,
        }

    def findings_frame(self):
        """The findings ledger as a Polars DataFrame, one row per finding.

        Columns: id, category, type, columns, inputs, target,
        interestingness, claim, n_evidence, basis, and the facet
        columns scope, object, evidence_kind, conditionality,
        temporality, quality_dimension, status — plus
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
            facets = f.get("facets") or {}
            row = {
                "id": f.get("id"),
                "category": f.get("category"),
                "type": f.get("type"),
                "scope": facets.get("scope"),
                "object": facets.get("object"),
                "evidence_kind": facets.get("evidence"),
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
        if not rows:
            schema = {
                "id": pl.String,
                "category": pl.String,
                "type": pl.String,
                "scope": pl.String,
                "object": pl.String,
                "evidence_kind": pl.String,
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
            if has_detector:
                schema["detector_interestingness"] = pl.Float64
            return pl.DataFrame(schema=schema)
        return pl.DataFrame(rows)

    def associations(self):
        """Every pair the association screens tested, as a Polars
        DataFrame (schema 0.7).

        One row per screened pair: `a`, `b`, `kind` ("numeric",
        "nominal", or "mixed"), the pair-complete row count `n`,
        the effect sizes that screen computed (`pearson`,
        `spearman`, `xi_ab`, `xi_ba` for numeric pairs — ξ is
        directional, so both directions are carried; `cramers_v`
        for nominal pairs; `eta`, the correlation ratio, for mixed
        pairs), the raw per-pair `p_value`, the Benjamini–Hochberg
        `q_value` over the pairs tested by the same screen in this
        run, and the reporting `tier`: "strong" (significant after
        correction and at or above the effect bar), "weak"
        (significant, below the bar), or "none". Findings state
        the strong pairs individually and the weak pairs as
        findings too; this table is the full screen, including
        the pairs that earned no statement. Empty, with the same
        schema, when no association screen ran.
        """
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
        if not rows:
            return pl.DataFrame(schema=schema)
        return pl.DataFrame(rows, schema=schema)

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
        total_grouped = len(data.get("grouped_findings") or [])
        index: list[dict] = []
        grouped_index: list[dict] = []
        state = {"columns": "full", "coverage_detail": "full"}

        def index_entry(f: dict) -> dict:
            subject = f.get("subject") or {}
            return {
                "id": f.get("id"),
                "category": f.get("category"),
                "type": f.get("type"),
                "columns": list(subject.get("columns") or []),
                "interestingness": f.get("interestingness"),
                "claim": f.get("claim"),
            }

        def render() -> str:
            view = {
                "schema_version": data["schema_version"],
                "overview": data["overview"],
                "columns": data["columns"],
                "findings": data["findings"],
                "finding_index": list(reversed(index)),
                "grouped_findings": data.get("grouped_findings") or [],
                "grouped_index": list(reversed(grouped_index)),
                "groups": data["groups"],
                "selection": data["selection"],
                "coverage": data["coverage"],
                "associations": data.get("associations") or [],
                "budget": {
                    "max_chars": max_chars,
                    "full_findings": len(data["findings"]),
                    "indexed_findings": len(index),
                    "full_grouped_findings": len(data.get("grouped_findings") or []),
                    "indexed_grouped_findings": len(grouped_index),
                    "columns": state["columns"],
                    "coverage_detail": state["coverage_detail"],
                },
            }
            if state["columns"] == "names_only":
                view["column_names"] = list(data["columns"].keys())
                view["columns"] = {}
            return _json.dumps(view)

        out = render()
        while len(out) > max_chars and (data["findings"] or data.get("grouped_findings")):
            # Selected findings collapse first; grouped statements
            # (full records under grouped_findings) collapse into
            # their own index once no selected finding is left in
            # full, so the budget accounting stays exact per list.
            if data["findings"]:
                index.append(index_entry(data["findings"].pop()))
            else:
                grouped_index.append(index_entry(data["grouped_findings"].pop()))
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
                f"findings and {total_grouped} grouped findings indexed)"
            )
        return out

    def to_markdown(self) -> str:
        return self._native.to_markdown()

    def to_digest(self, budget_chars: int = 4000) -> str:
        """A context-budgeted plain-text digest of this report.

        The digest leads with the account (frame-model step 2):
        the grain statement, the time axis with its span and step,
        and the dominant structure, straight from the report's
        `frame` section. Then the header (shape, schema, headline,
        coverage exceptions, finding counts by category), then one
        line per rollup group — findings sharing category, type,
        form, and target — with the group's
        highest-interestingness claim; groups with more than one
        member also list up to three exemplar finding ids,
        expandable with `.finding(id)`. Groups are ordered by
        report band (account, completeness, relations, column
        structure, deviations), then by that claim's
        interestingness. A `quality:` line after the account states
        missing values, constant and empty columns, and duplicate rows,
        counted over all rows. When verified relationship families formed
        (schema 0.6), the header states how many statements were
        grouped, and a family lead's rolled line carries its grouped
        member ids in a `[grouped: ...]` suffix; the lead presents
        at the earliest band among itself and its members. Weak-tier
        tested associations (schema 0.7: significant after
        Benjamini–Hochberg correction but below the strong effect
        bar) never take individual digest lines: however many there
        are, they collapse into one line carrying the count and the
        strongest examples, pointing at `report.associations()` for
        the full table. When the rolled lines exceed `budget_chars`,
        the digest takes lines from each band in turn, strongest
        first, so that no band fills the budget alone, then ends with
        a line stating what was omitted by category and how to retrieve
        it; it never exceeds the budget (budgets under ~200 characters
        return a truncated header). The full record stays in `.data` /
        `to_json()`.
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
        account_lines = _account_lines(data.get("frame") or {})
        lines.extend(account_lines)
        quality = _quality_line(data)
        if quality:
            lines.append(quality)
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
        selection = data.get("selection") or {}
        n_grouped = selection.get("grouped_statements") or 0
        if n_grouped:
            lines.append(
                f"grouped: {n_grouped} statement(s) grouped under "
                f"{selection.get('families', 0)} verified relationship "
                "families (the lead findings carry the grouped ids; "
                "records in grouped_findings)"
            )
        header = "\n".join(lines) + "\n"

        grouped_by_id = {f["id"]: f for f in data.get("grouped_findings") or []}

        def band_of(f: dict) -> int:
            # A family lead presents at the earliest band among
            # itself and its grouped members (schema 0.6): the lead
            # stands for the family's strongest content.
            band = _report_band(f)
            for mid in f.get("grouped_members") or []:
                member = grouped_by_id.get(mid)
                if member is not None:
                    band = min(band, _report_band(member))
            return band

        def weak_assoc(f: dict) -> bool:
            # Weak-tier tested associations (schema 0.7) collapse
            # into a single digest line below, however many there
            # are: they are leads, not headline statements.
            if f.get("type") != "statistical":
                return False
            return any(
                (ev.get("metrics") or {}).get("tier") == "weak"
                for ev in f.get("evidence") or []
            )

        weak = [f for f in findings if weak_assoc(f)]
        stated = [f for f in findings if not weak_assoc(f)]

        groups: dict[tuple, list[dict]] = {}
        for f in stated:
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
            line = head + ": " + fs_sorted[0]["claim"]
            if len(fs_sorted) > 1:
                exemplars = ", ".join(f["id"] for f in fs_sorted[:3])
                line += f" [exemplars: {exemplars}]"
            head_finding = fs_sorted[0]
            member_ids = head_finding.get("grouped_members") or []
            if member_ids:
                shown = ", ".join(member_ids[:3])
                if len(member_ids) > 3:
                    shown += f", +{len(member_ids) - 3} more"
                line += f" [grouped: {shown}]"
            rolled.append(
                (
                    band_of(head_finding),
                    -(head_finding.get("interestingness") or 0),
                    line,
                    cat,
                    len(fs_sorted),
                )
            )
        if weak:
            weak_sorted = sorted(
                weak, key=lambda x: -(x.get("interestingness") or 0)
            )
            examples = "; ".join(f["claim"] for f in weak_sorted[:3])
            rolled.append(
                (
                    band_of(weak_sorted[0]),
                    -(weak_sorted[0].get("interestingness") or 0),
                    f"[associations] statistical/weak (x{len(weak)}): "
                    f"{len(weak)} tested association(s) below the strong "
                    f"effect bar — strongest: {examples} "
                    "[full table: report.associations()]",
                    "associations",
                    len(weak),
                )
            )
        rolled.sort(key=lambda kv: (kv[0], kv[1], kv[2]))
        body_lines = [entry[2] for entry in rolled]

        def omission(omitted: list[tuple]) -> str:
            by_category: dict[str, int] = {}
            for entry in omitted:
                by_category[entry[3]] = by_category.get(entry[3], 0) + entry[4]
            ordered = sorted(by_category.items(), key=lambda kv: (-kv[1], kv[0]))
            names = ", ".join(f"{c} {n}" for c, n in ordered[:4])
            retrieve = (
                f'; report.select(category="{ordered[0][0]}") retrieves them'
                if ordered
                else ""
            )
            return (
                f"... [{len(omitted)} rolled-up finding groups omitted to fit "
                f"{budget_chars} chars; {len(findings)} findings total: "
                f"{names}{retrieve}]"
            )

        full = header + "\n" + "\n".join(body_lines) + "\n"
        if len(full) <= budget_chars:
            return full
        # Take lines from each band in turn.
        queues: dict[int, list[int]] = {}
        for i, entry in enumerate(rolled):
            queues.setdefault(entry[0], []).append(i)
        kept_order: list[int] = []
        used = len(header) + 1 + len(omission(rolled)) + 1
        progressed = True
        while progressed:
            progressed = False
            for band in sorted(queues):
                queue = queues[band]
                while queue:
                    i = queue.pop(0)
                    if used + len(rolled[i][2]) + 1 <= budget_chars:
                        kept_order.append(i)
                        used += len(rolled[i][2]) + 1
                        progressed = True
                        break
        # The omission line changes with what it lists, so it can come out
        # longer than the reserve. Give back the latest lines until it fits.
        while kept_order:
            kept_ids = set(kept_order)
            kept = [rolled[i][2] for i in sorted(kept_ids)]
            omitted = [rolled[i] for i in range(len(rolled)) if i not in kept_ids]
            result = header + "\n" + "\n".join(kept + [omission(omitted)]) + "\n"
            if len(result) <= budget_chars:
                return result
            kept_order.pop()
        # Degrade the header itself: drop the category-count and
        # coverage lines, then the headline, keeping the profile
        # line and the account block — the account is the summary
        # of record, so it is the last header content to go. If the
        # whole account still does not fit alongside the omission
        # line, keep the grain statement (the account's first line)
        # with the profile line; the omission line itself is part
        # of the contract and is dropped only when the budget
        # cannot hold even it next to the profile line.
        core = 1 + len(account_lines)
        ladder = [lines[: core + 1], lines[:core]]
        if account_lines:
            ladder.append([lines[0], account_lines[0]])
        ladder.append([lines[0]])
        for cand in ladder:
            short_header = "\n".join(cand) + "\n"
            result = short_header + omission(rolled) + "\n"
            if len(result) <= budget_chars:
                return result
        return (lines[0] + "\n")[:budget_chars]

    def verify(self, df):
        """Recount this report's statements in `df` through Polars.

        Returns a DataFrame with one row per checked quantity: `status` is
        `ok`, `mismatch`, `not_checkable` (a sampled or model-based statement,
        with nothing exact to recount) or `error`. Polars shares no code with
        the detectors, so a claim that disagrees with the data shows up as a
        `mismatch`. Run it on the profiled frame to audit the report, or on
        newer data to see which statements still hold.
        """
        from . import _checks

        data = self.data
        return _checks.verify(
            [*data["findings"], *(data.get("grouped_findings") or [])], df
        )

    def rows(self, finding_id: str, df):
        """The rows behind one statement, with their positions in `__row`.

        Violations for a dependency or identity, missing rows, repeated rows,
        extreme values, rows breaking a pattern, or rows matching a rule's
        conditions. Raises ValueError for statements with no row set.
        """
        from . import _checks

        finding = self.finding(finding_id)
        if finding is None:
            raise ValueError(f"no finding with id {finding_id}")
        return _checks.rows_of(finding, df)

    def to_checks(self, fmt: str = "polars", name: str = "onceover"):
        """Exact statements as constraints to run on new data.

        `fmt="polars"` returns dicts with a Polars expression in `polars`.
        `fmt="great_expectations"` returns an expectation suite; constraints
        with no built-in expectation are listed under
        `meta.unmapped_constraints`. Statements with exceptions, or measured on
        a sample, are left out.
        """
        from . import _checks

        data = self.data
        findings = [*data["findings"], *(data.get("grouped_findings") or [])]
        if fmt == "polars":
            return _checks.constraints(findings)
        if fmt == "great_expectations":
            return _checks.great_expectations_suite(findings, name)
        raise ValueError(f"unknown format {fmt!r}; use polars or great_expectations")

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
