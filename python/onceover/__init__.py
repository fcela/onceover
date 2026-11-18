"""onceover: first look at tabular data.

A native Rust profiler for Polars DataFrames, plus a Python-side
multi-table pass (`profile_many`) that reports candidate foreign
keys across a set of related tables.

New callers: `agent_guide()` returns the packaged quickstart for
this surface; `capabilities()` returns its compact machine-readable
companion.
"""

import json as _json
import math
import operator as _operator
import os
import re

from onceover._many import MultiTableReport, profile_many
from onceover._native import ProfileReport as _NativeProfileReport
from onceover._native import profile as _profile_native

__version__ = "0.1.6"
__all__ = [
    "profile",
    "ProfileReport",
    "profile_many",
    "MultiTableReport",
    "available_detectors",
    "taxonomy",
    "agent_guide",
    "capabilities",
    "infer_spec",
]

_SCHEMA_VERSION = "0.8"

#: The budget names `profile` accepts. The native parser is the
#: authority — its unknown-budget error lists the same names — and
#: the tests pin this list to that error.
_BUDGETS = ("quick", "standard", "deep")


def _findings_frame_schema(*, with_detector: bool = False):
    """The single source of truth for findings_frame()'s schema."""
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


def profile(df, budget: str = "standard", deadline_seconds: float | None = None, *,
            sample_rows: int | None = None, max_predictors: int | None = None,
            max_pairs: int | None = None, detectors: list[str] | None = None,
            max_findings: int = 100, max_rules: int = 20,
            hints: dict | None = None, target: str | None = None,
            timing: bool = False,
            source_row_cap: int | None = None, exact: bool = False,
            sample_seed: int = 0, spec: dict | None = None):
    """Give a Polars DataFrame a once-over.

    New callers: `agent_guide()` returns the packaged quickstart
    for this surface (the handover, the budgets, reading the
    report); `capabilities()` returns its compact companion.

    Parameters
    ----------
    df : polars.DataFrame, polars.LazyFrame, str, or os.PathLike
        The table to profile (Arrow interchange; Polars 1.28–2.x), a
        lazy frame, or a path to a Parquet, CSV, or IPC file — see
        ``source_row_cap`` for how lazy and file inputs are
        collected.
    budget : str
        "quick" (summaries and integrity checks), "standard" (default passes with caps),
        or "deep" (wider caps and deeper search for those passes).
    deadline_seconds : float, optional
        Soft wall-clock budget. Deterministic by default (None); when set,
        remaining passes may be skipped and reported in `coverage`, and the
        dependence screen, the hotspots scan, the discontinuities pass, the
        surprises scan, and the duplicate-encodings scan stop cooperatively
        inside their own loops, disclosing in their coverage entries how
        much of their search space was examined. Work before a pass's first
        check (extraction, column cards, the frame model) is not
        interruptible, so a very short deadline can still be exceeded.

    sample_rows : int, optional
        Override row samples for relationship searches, including both search
        and scoring in row_clusters. Column summaries and identity verification
        still examine all rows. Small samples can leave
        too few rows for model fitting; check coverage.
    max_predictors : int, optional
        Predictors retained per model after screening all usable columns.
    max_pairs : int, optional
        Detailed numeric pairs retained after screening all numeric pairs.

    detectors : list[str], optional
        Detector IDs to select; None selects the default set (31 of the
        46 registered detectors). Excluded unless requested: the
        on-request detectors "row_clusters" (experimental),
        "date_gaps", "pivot", "period_associations", "drift",
        "granularity", "distribution_change", "slice_shifts",
        "decision_tables", "interactions", and "entity_fingerprints" \u2014 each
        records a skipped coverage entry in default runs \u2014 and the
        target-triggered "outcome_account", "outcome_contrasts",
        "target_shapes", "leakage", and "model_failures", which
        run when a target is designated.
        "expectations" and "group_contrast" are in the default set
        but engage only when `hints` declare what they check.
        Required detectors can run as resources without contributing
        their own findings. `available_detectors()` reports each
        detector's membership (in the default set, or on request
        only) and its trigger (every run, declared hints, or a
        designated target).
    max_findings, max_rules : int
        Report-wide selection limits, applied across all contributing methods.
    hints : dict, optional
        Declared knowledge about the table that the profiler cannot
        infer. The keys fall in four classes. Metadata \u2014 what the
        columns mean and what the data is for, carried into the
        report's frame account and accounted for in the
        ``declarations`` coverage entry (what was checked, what was
        carried unchecked; nothing is silently dropped):
        ``gloss`` (per-column meanings in the caller's words \u2014 a
        text per column, or a map of level label to text for a
        categorical column's levels; carried unchecked and stated
        in the prose of findings whose subject is the column),
        ``units`` (per-column units, stated through the findings'
        subject ``unit`` field), ``event`` (the level of the
        designated target that counts as the event: with it, the
        outcome contrasts state event rates against the pooled
        event rate and measured absences as nulls; the named level
        is checked against the target's levels), ``entity`` (the
        frame's entity column for the panel passes, used instead
        of the inferred entity; checked to exist),
        ``recorded_after`` (columns recorded after the outcome or
        derived from it: the leakage screen's declared suspects,
        checked first and stated as such, and excluded from the
        predictor pools of the model-failure and predictability
        screens), ``purpose`` (what the data is for, one string;
        carried unchecked), and ``questions`` (the questions the
        data serves: findings matched to a question by the columns
        it names are admitted ahead of unmatched findings of equal
        score, and the ``questions`` coverage entry states how
        many findings answer each question, including zero).
        Declaring any metadata key engages the pointed mode's
        stated nulls. Structure to verify
        \u2014 the "expectations" pass checks these exactly (evidence
        tagged ``reference_source="declared"``): ``order_by``
        (columns rows are ordered by), ``no_gaps`` (columns expected
        to form a complete sequence), ``cadence`` (per-column:
        "daily", "business_day", "weekly", "monthly", or a day/step
        count), ``hierarchies`` (known hierarchies: each entry is one
        hierarchy's level columns, ordered finest to coarsest \u2014
        every adjacent mapping is verified exactly; a hierarchy that
        holds is stated once and the inferred restatements of the
        chain group under it, a hierarchy that does not hold is
        stated once as a deviation naming every broken mapping), and
        ``dimensions`` with ``measures`` (a declared panel, whose
        completeness over the declared axis is checked).
        Instructions to the run: ``time`` (the report's axis
        column), ``ordered_axis`` (an ordering column that is not a
        date or datetime \u2014 a dose, an age, a sequence number \u2014
        for the ordered-axis battery), ``level_order`` (per-column
        declared level orders for categorical columns),
        ``roles`` (per-column semantic role override for the frame
        model: one of "identifier", "row_number", "timestamp",
        "timestamp_part", "dimension", "flag", "measure", "text").
        These four structural keys are checked once and their
        applied/refused status is stated in the ``declarations``
        coverage entry; an unusable column is refused, not dropped.
        ``exclude_columns`` (columns removed before profiling
        starts: no card, and no finding can name them; the
        ``column_exclusions`` coverage entry states the exclusion
        and its user-declared source), and ``contrast`` (a group
        contrast: ``{"column": name, "value": scalar}`` declares one
        subset of the rows \u2014 the rows where the column equals the
        value, or a boolean column's true rows when ``value`` is
        omitted \u2014 and the "group_contrast" pass profiles that
        subset against the rest of the table, column by column), and
        ``weights`` (numeric columns whose values weight the rows).
        The "hotspots" pass compares each cell's share of each
        weight with its row share. The on-request "entity_fingerprints"
        pass compares entity mixes using each usable weight separately,
        or row counts when no weight applies. A weight that is missing
        from the frame, nonnumeric, negative, constant, or totals zero
        is refused and recorded in the pass's coverage.
        Presentation: ``labels`` (per-column display labels: claims
        and interpretations name the column by its label instead of
        the raw column name, while subjects, evidence, metrics, and
        column cards keep the raw name and finding ids are unchanged
        \u2014 labels are presentation, never identity. Substitution is
        scoped to each finding: a finding's prose uses a column's
        label only when the finding's own subject or specification
        names that column. A label that names a different column,
        one label declared for two columns, an empty label, or a
        label for an unknown column is not applied: the raw name is
        kept and the ``labels`` coverage entry states each case).
        The pre-0.8 key names ``axis`` and ``ordinal`` are accepted
        as aliases for ``ordered_axis`` and ``level_order`` for one
        release; the ``hints`` coverage entry states the rename when
        they are used. Unknown columns or unusable types are
        reported in the pass coverage, not raised. The column the
        frame model assembles as the report's time axis is excluded
        from target roles and from being profiled as a measure by
        the series pass by structure, whatever its assigned role; a
        ``roles`` entry for the axis column overrides that default.
    target : str, optional
        Designate a response column; part of the same declaration
        surface as `hints`. Engages the target-triggered passes:
        ``outcome_account`` (the outcome's own distribution \u2014
        center, spread, and range for a numeric outcome; the share
        of each level, and the declared event's rate, for a
        categorical one), ``outcome_contrasts`` (per-level outcome
        contrasts: each categorical level's event rate \u2014 or
        median, for a numeric outcome \u2014 against the pooled
        outcome, with measured absences stated as nulls when
        metadata is declared), ``target_shapes`` (the shape of each
        numeric predictor's relationship to the target \u2014 linear /
        binned-curve / single-step R\u00b2 shares \u2014 where a shape
        is decisive), ``leakage`` (a column that determines the
        target, or relates to it far more strongly than any other),
        and ``model_failures`` (slices where a cross-fitted model
        of the target fails). When ``detectors`` is given
        explicitly, the target-triggered passes run only if listed.
        An unknown target is reported in the passes' coverage, not
        raised.
    timing : bool, optional
        Record per-pass wall time as ``elapsed_ms`` in each coverage
        entry: the interval between the previous entry being recorded
        and this one (passes run sequentially; the first entry also
        carries extraction and the column cards). Opt-in because
        timing makes the coverage section nondeterministic; findings
        are identical either way.
    source_row_cap : int, optional
        LazyFrame and file inputs only: the most rows collected from
        the source (default 200,000). A source at or under the cap is
        collected in full; above it, a deterministic stratified
        sample of ``source_row_cap`` rows is collected at the source
        — one row per equal stratum, hash-chosen within the stratum,
        drawn with ``sample_seed``. The report's top-level ``source``
        block and its ``input`` coverage entry state the basis
        (``full`` or ``sampled``, with the sample design), and on a
        sampled basis every finding's evidence scope records the
        sample and the source population. CSV inputs infer their
        schema from the whole file, so a type that appears only in
        late rows is not silently read as nulls.
    exact : bool, optional
        LazyFrame and file inputs only: collect the whole source
        regardless of ``source_row_cap``. Exact statistics on a
        larger-than-memory input are this opt-in; memory is then
        the caller's responsibility.
    sample_seed : int, optional
        Seed for the source sample (default 0). The same source,
        cap, and seed give the same sample and the same report.

    ``source_row_cap``, ``exact``, and ``sample_seed`` are rejected
    for DataFrame inputs, which are always profiled in full.

    Returns
    -------
    ProfileReport
        Ranked findings ledger + per-column cards + coverage block.
    """
    import polars as _pl

    # Argument shapes are checked here, before the native call, so a
    # wrong type names what the argument accepts instead of surfacing
    # as a binding extraction error.
    if not isinstance(budget, str):
        raise TypeError(
            "budget must be a string, one of "
            + " | ".join(_BUDGETS)
            + f"; got {type(budget).__name__}"
        )
    if budget not in _BUDGETS:
        raise ValueError(
            f"unknown budget {budget!r} (expected {' | '.join(_BUDGETS)})"
        )
    if detectors is not None:
        if isinstance(detectors, str) or not isinstance(detectors, (list, tuple)):
            raise TypeError(
                "detectors must be a list of detector IDs (strings); "
                "capabilities() lists the registered IDs"
            )
        for _detector in detectors:
            if not isinstance(_detector, str):
                raise TypeError(
                    "detectors entries must be detector ID strings; "
                    f"got {type(_detector).__name__}"
                )
    if hints is not None and not isinstance(hints, dict):
        raise TypeError(
            "hints must be a dict keyed by hint name; "
            "agent_guide() lists the hint keys"
        )
    if target is not None and not isinstance(target, str):
        raise TypeError(
            "target must be a column name (a string); "
            f"got {type(target).__name__}"
        )
    if deadline_seconds is not None and (
        isinstance(deadline_seconds, bool)
        or not isinstance(deadline_seconds, (int, float))
    ):
        raise TypeError("deadline_seconds must be a number of seconds")
    for _cap_name, _cap_value in (
        ("sample_rows", sample_rows),
        ("max_predictors", max_predictors),
        ("max_pairs", max_pairs),
        ("max_findings", max_findings),
        ("max_rules", max_rules),
    ):
        if _cap_value is None:
            continue
        if _as_int(_cap_value) is None:
            raise TypeError(f"{_cap_name} must be an integer")
        if _cap_value <= 0:
            raise ValueError(f"{_cap_name} must be positive")

    if spec is not None:
        hints, target = _apply_spec(df, spec, hints, target)
    source = None
    if isinstance(df, _pl.DataFrame):
        if source_row_cap is not None or exact or sample_seed:
            raise ValueError(
                "source_row_cap, exact and sample_seed apply to LazyFrame "
                "and file inputs; a DataFrame is already in memory and is "
                "always profiled in full"
            )
    else:
        df, source = _materialize_source(
            df, source_row_cap=source_row_cap, exact=exact,
            sample_seed=sample_seed)
    return ProfileReport(_profile_native(df, budget, deadline_seconds, sample_rows=sample_rows,
                                         max_predictors=max_predictors, max_pairs=max_pairs, detectors=detectors,
                                         max_findings=max_findings, max_rules=max_rules,
                                         hints=_json.dumps(hints) if hints is not None else None,
                                         target=target, timing=timing,
                                         source=_json.dumps(source) if source is not None else None))


# ------------------------------------------------------------------
# The frame spec (issue #449): one resolved interpretation of a
# frame -- every column's role, the time axis, the entity, the
# target, the weights -- each entry marked declared or inferred
# with its evidence. `infer_spec` returns it without profiling;
# `profile(df, spec=...)` runs on it exactly.

_SPEC_VERSION = 1
_SPEC_ROLES = ("identifier", "row_number", "timestamp", "timestamp_part",
               "dimension", "flag", "measure", "text")
# Hint keys the spec carries; with a spec they are not also accepted
# as hints, so the run has one source for each.
_SPEC_HINT_KEYS = ("roles", "time", "entity", "weights")


def infer_spec(df, hints: dict | None = None, target: str | None = None) -> dict:
    """The frame's resolved interpretation, without profiling it.

    Returns a JSON-serializable document:
    - ``columns``: each column's role with ``source`` ("declared"
      when ``hints["roles"]`` named it, else "inferred") and the
      ``evidence`` the assignment rests on;
    - ``time_axis``: the axis column (null when none), its source
      and evidence;
    - ``entity``: the entity column (null when none);
    - ``target`` and ``weights``: as declared (inference does not
      choose an outcome or a weight).

    Only the frame model and the cheap structural passes it needs
    run. Edit the document and pass it back as
    ``profile(df, spec=...)``: the run then uses every entry
    exactly as written, each as a declaration.
    """
    import polars as _pl

    if not isinstance(df, _pl.DataFrame):
        df, _ = _materialize_source(df, source_row_cap=None, exact=False, sample_seed=0)
    hints = dict(hints or {})
    report = profile(df, budget="quick", detectors=["keys", "duplicate_encodings"],
                     hints=hints or None, target=target)
    frame = report.data.get("frame") or {}
    declared_roles = hints.get("roles") or {}
    columns = {}
    for r in frame.get("roles") or []:
        name = r["column"]
        columns[name] = {
            "role": r.get("role"),
            "source": "declared" if name in declared_roles else "inferred",
            "evidence": r.get("evidence"),
        }
    axis = frame.get("axis") or None
    time_axis = {"column": None, "source": "none", "evidence": "no time axis found"}
    if axis:
        time_axis = {
            "column": axis.get("source_column") or axis.get("name"),
            "source": "declared" if axis.get("source") == "hint" else "inferred",
            "evidence": axis.get("description"),
        }
        if axis.get("parts"):
            time_axis["parts"] = list(axis["parts"])
    entity = {"column": None, "source": "none", "evidence": "no entity found"}
    if hints.get("entity"):
        entity = {"column": hints["entity"], "source": "declared", "evidence": "declared by hints.entity"}
    else:
        n = (report.data.get("overview") or {}).get("n_rows") or df.height
        grain = frame.get("grain") or {}
        axis_name = axis.get("name") if axis else None
        factors = [f for f in grain.get("factorization") or [] if f.get("column") != axis_name]
        if len(factors) == 1 and factors[0].get("distinct", 0) >= 10:
            col = factors[0]["column"]
            card = (report.data.get("columns") or {}).get(col) or {}
            nonnull = n - (card.get("null_count") or 0)
            if (columns.get(col, {}).get("role") in ("identifier", "dimension")
                    and 2 * factors[0]["distinct"] <= nonnull):
                entity = {"column": col, "source": "inferred",
                          "evidence": f"the grain's non-axis factor: {grain.get('statement')}"}
    return {
        "spec_version": _SPEC_VERSION,
        "columns": columns,
        "time_axis": time_axis,
        "entity": entity,
        "target": {"column": target, "source": "declared" if target else "none"},
        "weights": {"columns": list(hints.get("weights") or []),
                    "source": "declared" if hints.get("weights") else "none"},
    }


def _apply_spec(df, spec: dict, hints: dict | None, target: str | None):
    """Translate a spec into the run's declarations. An entry that
    matches what inference resolves on its own is left to inference,
    so an unedited spec reproduces the plain run exactly; an entry
    that differs becomes a declaration (a role, the axis, the entity,
    the weights). Declaring one role can move another column's
    inferred role, so the comparison repeats until every role the run
    resolves is the spec's. Raises ValueError on a spec that does not
    describe this frame, or whose roles cannot all be held."""
    if not isinstance(spec, dict) or spec.get("spec_version") != _SPEC_VERSION:
        raise ValueError(f"spec must be a dict with spec_version {_SPEC_VERSION} (from infer_spec)")
    import polars as _pl

    if not isinstance(df, _pl.DataFrame):
        df, _ = _materialize_source(df, source_row_cap=None, exact=False, sample_seed=0)
    names = list(df.columns)
    cols = spec.get("columns")
    if not isinstance(cols, dict):
        raise ValueError("spec['columns'] must map each column name to its entry")
    missing = [c for c in names if c not in cols]
    extra = [c for c in cols if c not in names]
    if missing or extra:
        raise ValueError(f"spec does not describe this frame: columns missing from the spec {missing}, "
                         f"spec columns not in the frame {extra}")
    wanted = {}
    for name, entry in cols.items():
        role = (entry or {}).get("role")
        if role is not None and role not in _SPEC_ROLES:
            raise ValueError(f"spec role {role!r} for column {name!r} is not one of {_SPEC_ROLES}")
        wanted[name] = role
    hints = dict(hints or {})
    clash = [k for k in _SPEC_HINT_KEYS if k in hints]
    if clash:
        raise ValueError(f"hints {clash} are carried by the spec; edit the spec instead")
    spec_target = (spec.get("target") or {}).get("column")
    if spec_target and target and spec_target != target:
        raise ValueError(f"target {target!r} differs from the spec's target {spec_target!r}")
    target = spec_target or target
    axis = (spec.get("time_axis") or {}).get("column")
    entity = (spec.get("entity") or {}).get("column")
    for label, col in (("time_axis", axis), ("entity", entity)):
        if col and col not in names and not (label == "time_axis" and (spec.get("time_axis") or {}).get("parts")):
            raise ValueError(f"spec {label} column {col!r} is not a column of the frame")
    weights = list((spec.get("weights") or {}).get("columns") or [])
    if weights:
        hints["weights"] = weights
    if entity:
        hints["entity"] = entity
    declared_roles = {}
    for _ in range(4):
        trial = dict(hints)
        if declared_roles:
            trial["roles"] = dict(declared_roles)
        resolved = infer_spec(df, hints=trial, target=target)
        if axis != resolved["time_axis"]["column"] and "time" not in hints:
            if not axis:
                raise ValueError("the spec states no time axis, but the frame has one the run would use; "
                                 "declare the axis column's role instead of removing the axis")
            hints["time"] = axis
            continue
        differ = {c: r for c, r in wanted.items()
                  if r is not None and resolved["columns"].get(c, {}).get("role") != r}
        if not differ:
            if declared_roles:
                hints["roles"] = declared_roles
            return hints, target
        declared_roles.update(differ)
    raise ValueError(f"the spec's roles cannot all be held together: {sorted(differ)} still resolve differently")


#: Default cap on rows collected from a lazy or file source.
_DEFAULT_SOURCE_ROW_CAP = 200_000

#: Row-index column used to select the source sample; a source that
#: already carries this name is rejected rather than overwritten.
_SOURCE_ROW_INDEX = "__onceover_source_row"


def _stratified_indices(n: int, cap: int, seed: int) -> list[int]:
    """The deterministic stratified source sample: one row per
    equal stratum of the source, the row within a stratum chosen by
    a splitmix64 hash of the stratum index and the seed — the design
    `onceover_core::sample_rows` uses for relationship searches,
    applied at the source, with no periodic stride to alias against
    periodic data."""
    mask = (1 << 64) - 1
    out = []
    for j in range(cap):
        lo = (j * n) // cap
        span = ((j + 1) * n) // cap - lo
        if span <= 0:
            continue
        x = (j + seed + 0x9E3779B97F4A7C15) & mask
        x = ((x ^ (x >> 30)) * 0xBF58476D1CE4E5B9) & mask
        x = ((x ^ (x >> 27)) * 0x94D049BB133111EB) & mask
        x ^= x >> 31
        out.append(lo + x % span)
    return out


def _as_int(value):
    """An integer argument as a Python int: int itself and any type
    that implements __index__ (NumPy integers), never bool. None for
    anything else (floats included)."""
    if isinstance(value, bool):
        return None
    try:
        return _operator.index(value)
    except TypeError:
        return None


def _scan_path(path):
    """A path input as (LazyFrame, kind), via the matching scan
    constructor."""
    import polars as pl

    suffix = os.path.splitext(os.fspath(path))[1].lower()
    if suffix in (".parquet", ".pq"):
        return pl.scan_parquet(path), "parquet"
    if suffix == ".csv":
        # infer_schema_length=None: the schema comes from the whole
        # file, so a type that appears only in late rows is not
        # silently read as nulls.
        return pl.scan_csv(path, infer_schema_length=None), "csv"
    if suffix in (".ipc", ".feather", ".arrow"):
        return pl.scan_ipc(path), "ipc"
    raise ValueError(
        f"unsupported file extension {suffix or '(none)'!r}: profile() "
        "reads .parquet, .pq, .csv, .ipc, .feather and .arrow files"
    )


def _materialize_source(df, *, source_row_cap, exact, sample_seed):
    """Resolve a LazyFrame or path input to (frame, source):
    collect in full when the source fits the cap (or exact=True),
    otherwise collect a stratified sample of the source at the cap.
    `source` is the report's source block, stating the basis of
    every finding the report will contain."""
    import polars as pl

    if isinstance(df, pl.LazyFrame):
        lf, kind = df, "lazyframe"
    elif isinstance(df, (str, os.PathLike)):
        lf, kind = _scan_path(df)
    else:
        raise TypeError(
            "profile() expects a polars DataFrame, a polars LazyFrame, "
            "or a path to a Parquet, CSV or IPC file; got "
            f"{type(df).__name__}"
        )
    if source_row_cap is None:
        cap = _DEFAULT_SOURCE_ROW_CAP
    else:
        cap = _as_int(source_row_cap)
        if cap is None or cap <= 0:
            raise ValueError("source_row_cap must be a positive integer")
    seed = _as_int(sample_seed)
    if seed is None or seed < 0:
        raise ValueError("sample_seed must be a nonnegative integer")
    sample_seed = seed
    try:
        n = lf.select(pl.len()).collect(engine="streaming").item()
    except Exception:
        n = lf.select(pl.len()).collect().item()
    source = {"kind": kind, "rows": n, "rows_profiled": n, "basis": "full"}
    if exact or n <= cap:
        return lf.collect(), source
    if _SOURCE_ROW_INDEX in lf.collect_schema():
        raise ValueError(
            f"source already has a column named {_SOURCE_ROW_INDEX!r}")
    indices = _stratified_indices(n, cap, sample_seed)
    work = (
        lf.with_row_index(_SOURCE_ROW_INDEX)
        .filter(pl.col(_SOURCE_ROW_INDEX).is_in(indices))
        .drop(_SOURCE_ROW_INDEX)
    )
    try:
        frame = work.collect(engine="streaming")
    except Exception:
        frame = work.collect()
    source["rows_profiled"] = frame.height
    source["basis"] = "sampled"
    source["sample"] = {"method": "stratified", "seed": sample_seed,
                        "cap": cap}
    return frame, source


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


def _statement_class(finding: dict) -> int:
    """The statement class of a finding (issue #271), mirroring the
    Rust ledger's selection-time classification: 0 account
    (frame-scope statements and keys), 1 structure (how columns and
    segments relate), 2 shape (how values distribute and move),
    3 hygiene (data-quality facts). First matching rule decides."""
    facets = finding.get("facets") or {}
    cat = finding.get("category")
    if facets.get("scope") == "frame" or cat == "uniqueness":
        return 0
    if (
        finding.get("type") == "duplicate_encoding"
        or cat in ("dependencies", "associations", "conditional_rules")
        or facets.get("conditionality")
        in ("value_conditioned", "segment_conditioned")
    ):
        return 1
    if cat == "changes" or (
        cat == "anomalies" and finding.get("type") == "extreme_value"
    ):
        return 2
    return 3


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
    empty = sum(1 for card in columns.values() if card.get("card_role") == "all_null")
    constant = sum(1 for card in columns.values() if card.get("card_role") == "constant")
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


# ------------------------------------------------------------------
# The briefing digest (issue #317 phase 2): facts, sections, numbers
#
# A fact is a distinct set of stated columns. Findings that share
# a column set state the same fact, however many detectors state
# it and in whichever direction, so they merge into one digest
# line before rendering. The stated columns of a finding are its
# subject columns with the target appended; when the first
# evidence block names drivers — a model-based finding lists
# every screened input in its subject, but the columns it credits
# are its drivers — the drivers replace the subject columns.
# This is the same accounting the gate harness uses, so a fact
# here is a fact there.

_DIGEST_SECTION_ORDER = (
    "row",
    "wrong",
    "redundant",
    "target",
    "relationships",
    "anomalies",
    "not_examined",
)
_DIGEST_SECTION_TITLES = {
    "row": "What a row is",
    "wrong": "What is wrong",
    "redundant": "What is redundant",
    "target": "What relates to the target",
    "relationships": "Other relationships",
    "anomalies": "Anomalies",
    "not_examined": "Not examined",
}
# Shares of the finding-line budget, per section. When no target
# is designated, the target section's share passes to the other
# relationships.
_DIGEST_SECTION_SHARES = {
    "wrong": 0.24,
    "redundant": 0.13,
    "target": 0.26,
    "relationships": 0.24,
    "anomalies": 0.13,
}
# Finding types that state a fact exactly. An exact form is the
# digest line for its fact; statistical restatements fold into it.
_DIGEST_EXACT_TYPES = {"functional", "arithmetic_identity", "affine", "duplicate_encoding"}
_DIGEST_NUMBER = re.compile(r"\d+\.\d+(?:[eE][+-]?\d+)?")


def _digest_columns(finding: dict) -> list:
    subj = finding.get("subject") or {}
    cols = list(subj.get("columns") or [])
    for ev in (finding.get("evidence") or [])[:1]:
        drivers = (ev.get("metrics") or {}).get("drivers")
        if (
            isinstance(drivers, list)
            and drivers
            and all(isinstance(d, str) for d in drivers)
        ):
            cols = list(drivers)
    target = subj.get("target")
    if target and target not in cols:
        cols.append(target)
    return cols


def _digest_numbers(text: str) -> str:
    """Round the decimal numbers in a claim to three significant
    figures. Integers — counts, years — are exact and stay exact.
    Below a thousandth, three significant figures in plain
    notation would print six or more decimal places, so the
    compact scientific form carries the same precision."""

    def sub(m: "re.Match") -> str:
        try:
            value = float(m.group(0))
        except ValueError:
            return m.group(0)
        if value == 0:
            return "0"
        places = 2 - math.floor(math.log10(abs(value)))
        rounded = round(value, places)
        if rounded == int(rounded):
            whole = int(rounded)
            return f"{whole:,}" if abs(whole) >= 10000 else str(whole)
        if abs(rounded) < 0.001:
            return f"{rounded:.2e}"
        return f"{rounded:,}"

    return _DIGEST_NUMBER.sub(sub, text)


def _digest_line_text(finding: dict) -> str:
    return _digest_numbers(finding.get("claim") or "")


def _fact_precedence(finding: dict) -> int:
    """Which finding speaks for a fact: leak columns first, then
    the exact forms, then order constraints, then rules and the
    outcome screen's statements, then the statistical forms. The
    outcome account and contrasts sit with the rules: they state
    the outcome's own numbers, which a statistical restatement of
    the same columns would only blur."""
    typ = finding.get("type")
    if typ == "leakage_signature":
        return 0
    if typ in _DIGEST_EXACT_TYPES:
        return 1
    if typ in ("order_constraint", "order_violation"):
        return 2
    if typ in ("if_then", "outcome_distribution", "outcome_contrast"):
        return 3
    return 4


def _is_weak_association(finding: dict) -> bool:
    if finding.get("type") != "statistical":
        return False
    return any(
        (ev.get("metrics") or {}).get("tier") == "weak"
        for ev in finding.get("evidence") or []
    )


def _digest_facts(findings: list, grouped: list) -> tuple[list, list]:
    """Merge findings into facts: one entry per distinct column
    set, carrying the representative finding (exact forms win),
    every member finding, and the grouped members any member
    leads. Weak-tier associations are returned separately: they
    collapse into a single line and join a fact only when a
    stronger finding states the same columns."""
    grouped_by_id = {f["id"]: f for f in grouped}
    weak = []
    by_key: dict[tuple, dict] = {}
    facts = []
    for f in findings:
        cols = _digest_columns(f)
        if _is_weak_association(f):
            weak.append((f, cols))
            continue
        key = tuple(sorted(cols)) if cols else ("", f["id"])
        if f.get("type") == "outcome_contrast":
            # A calendar-part contrast (its grouping is a part of
            # the axis, named in the metrics, not a column) is a
            # different fact from the axis statements on the same
            # columns — the breaks along the axis and the outcome
            # by calendar part answer different questions, and
            # merging them lets one speak for the other.
            for ev in f.get("evidence") or []:
                grouping = (ev.get("metrics") or {}).get("grouping_column")
                if isinstance(grouping, str) and grouping and grouping not in cols:
                    key = key + ("calendar:" + grouping,)
                break
        fact = by_key.get(key)
        if fact is None:
            fact = {"columns": set(cols), "members": [], "rep": f}
            by_key[key] = fact
            facts.append(fact)
        fact["members"].append(f)
        if (_fact_precedence(f), -(f.get("interestingness") or 0)) < (
            _fact_precedence(fact["rep"]),
            -(fact["rep"].get("interestingness") or 0),
        ):
            fact["rep"] = f
    remaining_weak = []
    for f, cols in weak:
        fact = by_key.get(tuple(sorted(cols))) if cols else None
        if fact is not None:
            fact["members"].append(f)
        else:
            remaining_weak.append(f)
    for fact in facts:
        members = list(fact["members"])
        for f in fact["members"]:
            for mid in f.get("grouped_members") or []:
                if mid in grouped_by_id:
                    members.append(grouped_by_id[mid])
        fact["members"] = members
    return facts, remaining_weak


def _designated_target(findings: list) -> str | None:
    """The designated target, when the report carries one. The
    report records no target field; the target-gated screens are
    the record — their findings name the target they ran against.
    A designated target no screen stated a finding for cannot be
    told apart from an ordinary column here."""
    for f in findings:
        if f.get("type") in (
            "leakage_signature",
            "model_failure",
            "outcome_distribution",
            "outcome_contrast",
        ):
            target = (f.get("subject") or {}).get("target")
            if target:
                return target
    return None


def _fact_section(rep: dict, columns: set, designated: str | None) -> str:
    typ = rep.get("type")
    cat = rep.get("category")
    target = (rep.get("subject") or {}).get("target")
    if typ == "leakage_signature":
        return "target"
    if cat in ("missingness", "duplicates", "formats", "gaps"):
        return "wrong"
    if typ in ("sentinel_values", "boundary_mass"):
        return "wrong"
    if designated and target == designated:
        return "target"
    if cat == "concentration" and len(columns) <= 1:
        return "wrong"
    if typ in _DIGEST_EXACT_TYPES:
        return "redundant"
    if (
        designated
        and target is None
        and designated in columns
        and cat in ("dependencies", "associations", "conditional_rules", "changes")
    ):
        # A relationship fact about the designated target, stated
        # by a screen that does not mark its subject with a target
        # (the level contrasts are the common case). Findings that
        # carry a subject target of their own are routed by it:
        # a predictability statement about another column names
        # every screened input, the designated target included,
        # without being a statement about the target.
        return "target"
    if cat == "anomalies":
        return "anomalies"
    if cat == "concentration" and typ in (
        "outlier_entity",
        "spike_cells",
        "entity_specialists",
    ):
        return "anomalies"
    return "relationships"


def _row_lines(frame: dict) -> list[str]:
    """What a row is: the grain statement and the time axis, from
    the report's `frame` section, and the caller's declared
    purpose when one was declared. The dominant-structure account
    lines are not part of the briefing (issue #317 phase 2)."""
    lines: list[str] = []
    grain = frame.get("grain") or {}
    if grain.get("statement"):
        lines.append(grain["statement"])
    axis = frame.get("axis") or {}
    if axis:
        line = "time axis \u2014 " + axis.get("description", axis.get("name", ""))
        if axis.get("min") and axis.get("max"):
            line += f", {axis['min']} to {axis['max']}"
            if axis.get("step"):
                line += f", in {axis['step']} steps"
            if axis.get("distinct_values"):
                line += f" ({axis['distinct_values']} distinct values)"
        lines.append(line)
    declarations = frame.get("declarations") or {}
    if declarations.get("purpose"):
        lines.append(f"declared purpose \u2014 {declarations['purpose']}")
    return lines[:3]


def _not_examined_line(data: dict) -> str | None:
    """What was not examined: passes that run by default and were
    capped, cut by the deadline, or errored. Passes that run on
    request are never listed. Detector membership comes from the
    registry, so the list cannot drift from the default set."""
    cov = data.get("coverage") or {}
    try:
        registry = {d["id"]: d.get("membership") for d in available_detectors()}
    except Exception:
        registry = {}
    capped, errored = [], []
    for key, entry in cov.items():
        if key in ("selection", "deadline"):
            continue
        if registry.get(key) == "on_request":
            continue
        status = (entry or {}).get("status")
        if status == "capped":
            capped.append(key)
        elif status == "error":
            errored.append(key)
    bits = []
    if capped:
        bits.append("capped: " + ", ".join(sorted(capped)))
    if errored:
        bits.append("errored: " + ", ".join(sorted(errored)))
    if "deadline" in cov:
        bits.append("the run was cut short by its deadline")
    return "; ".join(bits) if bits else None


def available_detectors():
    """Available detector IDs, produced categories, resource dependencies,
    membership, trigger, and a one-line purpose per detector. Membership
    is "default_set" (runs in the default set) or "on_request" (runs
    only when explicitly selected); trigger is "always", "hints"
    (engages only on declared hints), or "target" (engages only when
    a target is designated)."""
    from onceover._native import available_detectors as _available
    return _json.loads(_available())


def taxonomy():
    """The finding taxonomy: every pattern with its type, its derived
    category, and its facets, plus the registered forms and the facet
    vocabularies. Facets are the canonical evaluation the taxonomy's
    own type lists use; an entry also carries a varied value beside
    the canonical one when a statement can differ
    (`support_with_violations`, `scope_one_column`,
    `conditionality_conditioned`). The taxonomy tables in
    docs/findings.md are generated from this export."""
    from onceover._native import taxonomy as _taxonomy
    return _json.loads(_taxonomy())


def capabilities() -> dict:
    """The compact capability manifest: what a caller needs to aim
    a `profile` call, derived from the detector registry on every
    call and never hand-maintained.

    Returns a dict with the package `version`, the report
    `schema_version`, the `budgets` `profile` accepts, the
    `hint_keys` the hints parser accepts (in declaration order),
    and one entry per registered detector with its `id`,
    `membership`, `trigger`, and one-line `purpose`, plus a
    `requires` list only when the detector needs specific columns
    or inputs.
    """
    from onceover._native import available_detectors as _available
    from onceover._native import hint_keys as _hint_keys

    detectors = []
    for record in _json.loads(_available()):
        entry = {
            "id": record["id"],
            "membership": record["membership"],
            "trigger": record["trigger"],
            "purpose": record["purpose"],
        }
        if record["requires"]:
            entry["requires"] = list(record["requires"])
        detectors.append(entry)
    return {
        "version": __version__,
        "schema_version": _SCHEMA_VERSION,
        "budgets": list(_BUDGETS),
        "hint_keys": list(_hint_keys()),
        "detectors": detectors,
    }


def agent_guide() -> str:
    """The packaged agent guide: the quickstart for this surface,
    shipped inside the package so it is version-matched to the
    build it describes.

    The guide teaches, in order, the handover (declaring the
    outcome, column meanings, and structure through `target` and
    `hints`), the call and the budgets, reading the report (digest
    first), and detector selection. `capabilities()` is its
    machine-readable companion.
    """
    try:
        import importlib.resources as _resources

        return (
            _resources.files("onceover")
            .joinpath("agent_guide.md")
            .read_text(encoding="utf-8")
        )
    except Exception:
        from pathlib import Path

        return (Path(__file__).parent / "agent_guide.md").read_text(
            encoding="utf-8"
        )


# Card quantities the report diff compares, in report order. Every
# one is computed by the column pass on the full extracted table,
# before budget gating and detector selection, so two profiles of
# the same data carry the same values whatever budget or detector
# set produced them. The card carries no sum, so none is compared;
# the rest of the card (quartiles, top values, concentration,
# format patterns) is distribution detail the diff does not track.
_CARD_QUANTITIES = ("n", "null_count", "null_fraction", "distinct")
_CARD_NUMERIC_QUANTITIES = ("mean", "min", "max")


def _quantity_change(before, after) -> dict:
    """One card quantity that moved: the two values and the size of
    the move. `delta` is after minus before; `relative` is delta
    over before, carried only when before is a non-zero number (a
    move off zero has no meaningful relative size)."""
    entry = {"before": before, "after": after}
    if isinstance(before, (int, float)) and isinstance(after, (int, float)):
        entry["delta"] = after - before
        if before != 0:
            entry["relative"] = (after - before) / before
    return entry


def _card_changes(base_card: dict, other_card: dict) -> dict:
    """The compared quantities that moved between two cards for the
    same column, keyed by quantity name."""
    changes = {}
    for key in _CARD_QUANTITIES:
        before, after = base_card.get(key), other_card.get(key)
        if before != after:
            changes[key] = _quantity_change(before, after)
    base_stats = base_card.get("numeric_stats") or {}
    other_stats = other_card.get("numeric_stats") or {}
    for key in _CARD_NUMERIC_QUANTITIES:
        before, after = base_stats.get(key), other_stats.get(key)
        if before != after:
            changes[key] = _quantity_change(before, after)
    return changes


def _column_diff(base_columns: dict, other_columns: dict) -> dict:
    """Compare two reports' column cards, matched by column name.

    A column present on one side only is `added` or `removed`, so a
    renamed column reads as a removal plus an addition: identity is
    by name and renames are not guessed. A column present on both
    sides whose card dtype changed is `retyped`, stated distinctly
    from a removal plus an addition, and its quantities are not
    compared across the change. Any other shared column is
    `changed` when one of the compared card quantities moved, and
    counts towards `unchanged` when none did.
    """
    def brief(name, card):
        return {
            "name": name,
            "dtype": card.get("dtype"),
            "card_role": card.get("card_role"),
        }

    added, removed, retyped, changed = [], [], [], []
    unchanged = 0
    for name in sorted(set(base_columns) | set(other_columns)):
        if name not in base_columns:
            added.append(brief(name, other_columns[name]))
        elif name not in other_columns:
            removed.append(brief(name, base_columns[name]))
        else:
            base_card = base_columns[name]
            other_card = other_columns[name]
            if base_card.get("dtype") != other_card.get("dtype"):
                retyped.append(
                    {
                        "name": name,
                        "dtype": [
                            base_card.get("dtype"),
                            other_card.get("dtype"),
                        ],
                    }
                )
            else:
                changes = _card_changes(base_card, other_card)
                if changes:
                    changed.append({"name": name, "changes": changes})
                else:
                    unchanged += 1
    return {
        "added": added,
        "removed": removed,
        "retyped": retyped,
        "changed": changed,
        "unchanged": unchanged,
    }


class ProfileReport:
    """Profiling results and evidence.

    Access the full versioned JSON via `.data` (dict), the ranked findings
    via `.findings`, filtered findings via `.select(category=, type=,
    column=, target=)` and one finding by id via `.finding(id)`, the same
    findings as a Polars DataFrame via `.findings_frame()`, a
    context-budgeted digest via `.to_digest(chars)`, a size-budgeted
    JSON view via `.to_json(max_chars=...)`, a self-contained HTML
    rendering via `.to_html()` (needs the `html` extra), the
    one-glance summary via `.overview`, per-column cards via
    `.columns`, and pass coverage via `.coverage`. The report's
    contract is `docs/report-schema.json`.
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
        support=None,
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
        `support`, `conditionality`, `temporality`,
        `quality_dimension`, `status`); a facet filter never matches a
        finding that does not carry the facet. With no filters,
        returns all findings. With `include_grouped=True`, grouped
        statements (`grouped_findings`) are appended after the
        selected findings, in their own report order. The dicts are
        the live report entries."""
        facet_filters = {
            "scope": scope,
            "object": object,
            "support": support,
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

        The column cards are compared as well, under `columns`,
        matched by column name: `added` and `removed` columns
        (entries carry the name, card dtype, and card role; the
        full card is `.columns[name]` on the report that holds
        it), `retyped` columns (present in both, with a different
        card dtype — stated distinctly from a removal plus an
        addition, with the dtype pair, and not quantity-compared
        across the change), and `changed` entries for the shared
        columns whose compared card quantities moved, each
        quantity carrying its before and after values and the
        size of the move. The compared quantities are the card
        pass's exact counts — rows, nulls and their share,
        distinct values — and, for numeric columns, the mean,
        minimum, and maximum the card carries; the card carries
        no sum, so none is reported, and the rest of the card is
        not diffed. Those quantities are computed on the full
        table before budget gating, so profiles taken at
        different budgets or with different detector sets do not
        manufacture card differences. Column identity is by name:
        a renamed column reads as a removal plus an addition.
        Row-level changes are not compared.
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
            "columns": _column_diff(self.columns or {}, other.columns or {}),
        }

    def findings_frame(self):
        """The findings ledger as a Polars DataFrame, one row per finding.

        Columns: id, category, type, columns, inputs, target,
        interestingness, claim, n_evidence, basis, and the facet
        columns scope, object, support_kind, conditionality,
        temporality, quality_dimension, status — plus
        detector_interestingness when the report carries it. Rows are in
        report order (highest interestingness first). The full evidence
        records stay in `.findings` / `.data`; this frame is the
        sortable, filterable index into them.
        """
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
        findings = data["findings"]
        grouped = data.get("grouped_findings") or []
        total, total_grouped = len(findings), len(grouped)
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

        def split(removed: int) -> tuple[int, int]:
            # Selected findings collapse first; grouped statements
            # collapse only once no selected finding is left in full.
            if removed <= total:
                return total - removed, total_grouped
            return 0, total_grouped - (removed - total)

        def render(removed: int) -> str:
            full_findings, full_grouped = split(removed)
            view = {
                "schema_version": data["schema_version"],
                "frame": data.get("frame"),
                "overview": data["overview"],
                "columns": data["columns"],
                "findings": findings[:full_findings],
                "finding_index": [index_entry(f) for f in findings[full_findings:]],
                "grouped_findings": grouped[:full_grouped],
                "grouped_index": [index_entry(f) for f in grouped[full_grouped:]],
                "groups": data["groups"],
                "selection": data["selection"],
                "coverage": data["coverage"],
                "associations": data.get("associations") or [],
                "budget": {
                    "max_chars": max_chars,
                    "full_findings": full_findings,
                    "indexed_findings": total - full_findings,
                    "full_grouped_findings": full_grouped,
                    "indexed_grouped_findings": total_grouped - full_grouped,
                    "columns": state["columns"],
                    "coverage_detail": state["coverage_detail"],
                },
            }
            if state["columns"] == "names_only":
                view["column_names"] = list(data["columns"].keys())
                view["columns"] = {}
            return _json.dumps(view)

        # Binary-search the number of collapsed statements instead of
        # re-serializing once per finding (a quadratic rendering cost on
        # wide reports).
        ceiling = total + total_grouped
        if len(render(ceiling)) > max_chars:
            removed = ceiling
        else:
            lo, hi = 0, ceiling
            while lo < hi:
                mid = (lo + hi) // 2
                if len(render(mid)) > max_chars:
                    lo = mid + 1
                else:
                    hi = mid
            removed = lo
        out = render(removed)
        if len(out) > max_chars and state["columns"] == "full":
            state["columns"] = "names_only"
            out = render(removed)
        if len(out) > max_chars:
            for entry in data["coverage"].values():
                detail = entry.get("detail") or ""
                if len(detail) > 160:
                    entry["detail"] = detail[:160] + " [truncated]"
            state["coverage_detail"] = "truncated"
            out = render(removed)
        if len(out) > max_chars:
            # Coverage detail strings are the known size sink on wide
            # tables; the statuses alone still say what ran.
            for entry in data["coverage"].values():
                entry["detail"] = ""
            state["coverage_detail"] = "status_only"
            out = render(removed)
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
        """A context-budgeted plain-text briefing of this report.

        The briefing is organised by the questions a reader brings
        to a new table, in sections, each with its own share of the
        character budget so that no family of statements can crowd
        out another: what a row is (the grain statement and the
        time axis, from the report's `frame` section); what is
        wrong (missingness and its structure, duplicates,
        sentinels, impossible values, formats); what is redundant
        (one line per family of findings that restate one another);
        what relates to the target, when one is designated (leak
        columns first, then the strongest drivers); other strong
        relationships; anomalies; and what was not examined (only
        passes that run by default and were capped, cut by the
        deadline, or errored — passes that run on request are never
        listed). The section on what is wrong keeps its share in
        target mode too: designating a target no longer makes the
        hygiene findings vanish. A section that cannot fill its
        share passes what is left to the sections still holding
        facts, in section order.

        Inside the body, findings are merged into facts before
        rendering — one line per distinct set of columns — so a
        fact several detectors state is stated once.
        When an exact form of a fact exists (a functional
        dependency, an identity, an encoding), the exact form is
        the line and the statistical restatements fold into it.
        Each line is the representative finding's claim in plain
        words, with its numbers rounded to three significant
        figures, followed by that one finding's id in brackets;
        the merged findings are reachable through the report.
        Weak-tier tested associations (significant after
        Benjamini–Hochberg correction but below the strong effect
        bar) never take individual lines: however many there are,
        they collapse into one line carrying the count and the
        strongest examples, pointing at `report.associations()`.

        When the lines exceed `budget_chars`, each section keeps
        only what fits its share, strongest first, and the briefing
        ends with a line stating how many facts were omitted and
        how to retrieve them; it never exceeds the budget (budgets
        under ~200 characters return a truncated header). The full
        record stays in `.data` / `to_json()`.
        """
        data = self.data
        findings = data["findings"]
        overview = data.get("overview", {})
        profile_line = (
            f"profile digest: {overview.get('n_rows', '?')} rows x "
            f"{overview.get('n_columns', '?')} columns, "
            f"{len(findings)} findings, schema "
            f"{data.get('schema_version', '?')}"
        )
        fixed = [profile_line]
        source = data.get("source")
        if source and source.get("basis") == "sampled":
            fixed.append(
                f"source: {source.get('kind')}, {source.get('rows')} rows; "
                f"profiled on a stratified sample of "
                f"{source.get('rows_profiled')} rows \u2014 findings "
                "describe the sample"
            )
        row_lines = _row_lines(data.get("frame") or {})
        not_examined = _not_examined_line(data)

        designated = _designated_target(findings)
        facts, weak = _digest_facts(findings, data.get("grouped_findings") or [])
        grain_statement = ((data.get("frame") or {}).get("grain") or {}).get(
            "statement"
        )
        sections: dict[str, list[tuple[str, dict]]] = {}
        for fact in facts:
            rep = fact["rep"]
            if grain_statement and rep.get("claim") == grain_statement:
                # The grain statement already opens the briefing in
                # the row section; the key finding that states it
                # does not take a second line.
                continue
            section = _fact_section(rep, fact["columns"], designated)
            sections.setdefault(section, []).append(
                (_digest_line_text(rep), rep)
            )

        def relationship_sequence(entries, allowance):
            # The taking order inside a relationship section. Leak
            # lines first. Then the predictability directory — a
            # column's driver statement names its whole driver set
            # at once — when the whole directory fits the section's
            # allowance; after it, the section's lead families
            # (break and regime statements; the measures'
            # strongest level statements; the strongest hotspot)
            # are offered in step,
            # and the remaining kinds follow in breadth rounds. A
            # directory that cannot fit does not monopolise the
            # section: one breadth round across kinds comes first,
            # the directory's remainder after it, then breadth
            # rounds again.
            def score(e):
                return (-(e[1].get("interestingness") or 0), e[0])

            leakage = sorted(
                (e for e in entries if e[1].get("type") == "leakage_signature"),
                key=score,
            )
            directory = sorted(
                (e for e in entries if e[1].get("type") == "predictive"),
                key=score,
            )
            def bucket_key(rep):
                # A level contrast states how one measure separates
                # across a grouping's levels — the per-level
                # statement of that measure's relationship with the
                # grouping. Keyed as a single kind, the contrasts
                # crowd onto a few measures (every measure against
                # one grouping, at near-identical scores) and the
                # remaining measures' contrasts never reach the
                # early rounds. The kind therefore counts once per
                # measure it states: the strongest contrast of
                # every measure is offered before any measure's
                # second contrast, as the strongest line of every
                # kind is offered before any kind's second line.
                # An outcome contrast is keyed the same way, once
                # per grouping it states.
                if rep.get("type") == "level_contrast":
                    for ev in rep.get("evidence") or []:
                        measure = (ev.get("metrics") or {}).get(
                            "measure_column"
                        )
                        if isinstance(measure, str) and measure:
                            return (
                                rep.get("category"),
                                rep.get("type"),
                                measure,
                            )
                if rep.get("type") == "outcome_contrast":
                    for ev in rep.get("evidence") or []:
                        grouping = (ev.get("metrics") or {}).get(
                            "grouping_column"
                        )
                        if isinstance(grouping, str) and grouping:
                            return (
                                rep.get("category"),
                                rep.get("type"),
                                grouping,
                            )
                return (rep.get("category"), rep.get("type"))

            buckets: dict[tuple, list] = {}
            for e in entries:
                if e[1].get("type") in (
                    "leakage_signature",
                    "predictive",
                    "outcome_distribution",
                ):
                    continue
                buckets.setdefault(bucket_key(e[1]), []).append(e)
            for bucket in buckets.values():
                bucket.sort(key=score)

            def contrast_form(rep):
                for ev in rep.get("evidence") or []:
                    form = (ev.get("metrics") or {}).get("form")
                    if form:
                        return form
                return None

            lc_keys = {
                k
                for k, b in buckets.items()
                if b and b[0][1].get("type") == "level_contrast"
            }
            # The measures' strongest level statements, in the
            # order they are offered: the head of every measure's
            # bucket (see the bucket key note — breadth across
            # measures, as the section runs breadth across kinds),
            # a statement that locates its measure before one
            # that only spreads it, then by score. The location
            # is the relationship; the spread refines it, and on
            # sentinel-driven measures the spread statement is
            # often a restatement of the sentinel's mass.
            firsts = [buckets[k][0] for k in lc_keys]
            firsts.sort(
                key=lambda e: (
                    1 if contrast_form(e[1]) == "scale" else 0,
                    score(e),
                )
            )
            first_ids = {id(e) for e in firsts}
            # The outcome account and the outcome contrasts, in a
            # target section: the account is the base every
            # contrast compares against, so it leads the section
            # after any leak; each grouping's strongest outcome
            # contrast joins the lead families like a measure's
            # strongest level statement.
            outcome_account = sorted(
                (e for e in entries if e[1].get("type") == "outcome_distribution"),
                key=score,
            )
            oc_keys = {
                k
                for k, b in buckets.items()
                if b and b[0][1].get("type") == "outcome_contrast"
            }
            outcome_firsts = [buckets[k][0] for k in oc_keys]
            outcome_firsts.sort(key=score)
            outcome_first_ids = {id(e) for e in outcome_firsts}

            def round_lead(e):
                # The lead order inside a round. Break and regime
                # statements along an ordered axis lead: they are
                # the table's shape over its axis, the statements
                # a reader acts on first after the directory. The
                # measures' strongest level statements follow
                # them, with each grouping's strongest outcome
                # contrast and the strongest model-failure slice:
                # the pointed classes of a target section.
                # Everything else competes on score.
                rep = e[1]
                if rep.get("category") == "changes":
                    return 0
                if (
                    id(e) in first_ids
                    or id(e) in outcome_first_ids
                    or rep.get("type") == "model_failure"
                ):
                    return 1
                return 2

            def rounds_over(keys, first_depth):
                # Breadth rounds across the named buckets: the
                # strongest remaining line of every bucket first,
                # then the next line of every bucket, and so on.
                # A bucket's lines are taken from first_depth on.
                out = []
                depth = 0
                while True:
                    round_entries = [
                        buckets[k][first_depth[k] + depth]
                        for k in keys
                        if len(buckets[k]) > first_depth[k] + depth
                    ]
                    if not round_entries:
                        return out
                    round_entries.sort(
                        key=lambda e: (round_lead(e), score(e))
                    )
                    out.extend(round_entries)
                    depth += 1

            dir_chars = sum(
                len(t) + len(r.get("id") or "") + 4 for t, r in directory
            )
            if dir_chars <= allowance:
                # The lead families are offered in step,
                # alternating line for line: the break and regime
                # statements in breadth rounds across their kinds,
                # the measures' strongest level statements in
                # their queue order, and the strongest hotspot
                # statement — the cell where the table's rows
                # concentrate, the categorical counterpart of a
                # level statement — with the second exchange, so
                # each family's first statement precedes it.
                # Offered as blocks, each family can crowd the
                # others out of the section: the level statements
                # are long, and a block of them leaves no room
                # for the table's shape over its axis; the breaks
                # are numerous on an ordered table, and a block
                # of them leaves no room for the per-measure
                # statements that are the closest thing the
                # section has to outcome facts; and the hotspot
                # line, competing on score alone, is the first
                # casualty of either block. In step, all three
                # families are heard from the start.
                changes_keys = [
                    k
                    for k, b in buckets.items()
                    if b and b[0][1].get("category") == "changes"
                ]
                changes_queue = rounds_over(
                    changes_keys, {k: 0 for k in changes_keys}
                )
                hotspot_keys = [
                    k
                    for k, b in buckets.items()
                    if b and b[0][1].get("type") == "hotspot"
                ]
                hotspot_lead = buckets[hotspot_keys[0]][0] if hotspot_keys else None
                # The exchange leads with two lines of each
                # per-grouping family, not every grouping's head:
                # a wide table has a dozen groupings, and a lead
                # that admits all of them leaves no room in the
                # section for its other families (the model-failure
                # slices, the shapes). The remaining heads join
                # the breadth rounds below at depth 0, so every
                # grouping is still heard before any family's
                # second line.
                lead_outcome = outcome_firsts[:2]
                # In a target section (outcome contrasts present),
                # one level statement leads: the level statements
                # name the target only as their grouping, and two
                # of them in the lead crowd out the section's
                # other pointed families. Elsewhere two lead.
                lead_firsts = firsts[:1] if outcome_firsts else firsts[:2]
                lead_ids = {id(e) for e in lead_outcome + lead_firsts}
                lead = []
                hotspot_placed = False
                for i in range(
                    max(len(changes_queue), len(lead_firsts), len(lead_outcome))
                ):
                    if i < len(changes_queue):
                        lead.append(changes_queue[i])
                    # The outcome contrasts precede the level
                    # statements in the exchange: they state the
                    # designated outcome against its groupings,
                    # which is the section's subject, where a level
                    # statement names the target only as its
                    # grouping.
                    if i < len(lead_outcome):
                        lead.append(lead_outcome[i])
                    if i < len(lead_firsts):
                        lead.append(lead_firsts[i])
                    if i == 1 and hotspot_lead is not None:
                        lead.append(hotspot_lead)
                        hotspot_placed = True
                if hotspot_lead is not None and not hotspot_placed:
                    lead.append(hotspot_lead)
                rest_keys = [k for k in buckets if k not in changes_keys]
                rest = rounds_over(
                    rest_keys,
                    {
                        k: (
                            1
                            if (
                                (k in lc_keys or k in oc_keys)
                                and id(buckets[k][0]) in lead_ids
                            )
                            or k in hotspot_keys
                            else 0
                        )
                        for k in rest_keys
                    },
                )
                return leakage + outcome_account + directory + lead + rest
            # The directory does not fit, so it does not
            # monopolise the section: one breadth round across
            # kinds (the strongest line of every kind, the
            # measures' strongest level statement among them),
            # the directory's head alongside it, its remainder
            # after, then breadth rounds again — with the level
            # statements running as one queue among the kinds
            # (every measure's strongest first, then the
            # remaining contrasts by score), so a crowded
            # directory cannot push them out of the rounds
            # entirely, and they cannot fill the first round.
            contrast_rest = [
                e
                for k in lc_keys
                for e in buckets[k]
                if id(e) not in first_ids
            ]
            contrast_queue = firsts + sorted(contrast_rest, key=score)
            # The outcome contrasts keep their own buckets, as in
            # the directory path: each grouping's strongest
            # outcome contrast is a head in the first round, not a
            # follower in the level contrasts' queue — the outcome
            # contrasts are the section's subject when a target is
            # designated, and folding them behind the level
            # statements starved them out of crowded sections.
            tail = {
                k: b for k, b in buckets.items() if k not in lc_keys
            }
            if contrast_queue:
                tail[("associations", "level_contrast")] = contrast_queue
            saved = buckets
            buckets = tail
            try:
                first_round = [b[0] for b in buckets.values() if b]
                if directory:
                    first_round.append(directory[0])
                first_round.sort(key=lambda e: (round_lead(e), score(e)))
                rest = rounds_over(
                    list(buckets), {k: 1 for k in buckets}
                )
            finally:
                buckets = saved
            return leakage + outcome_account + first_round + directory[1:] + rest

        for name, entries in sections.items():
            entries.sort(
                key=lambda e: (-(e[1].get("interestingness") or 0), e[0])
            )
        if weak:
            weak_sorted = sorted(
                weak, key=lambda x: -(x.get("interestingness") or 0)
            )
            examples = "; ".join(
                _digest_numbers(f["claim"]) for f in weak_sorted[:3]
            )
            line = (
                f"{len(weak)} tested association(s) below the strong "
                f"effect bar \u2014 strongest: {examples} "
                "[full table: report.associations()]"
            )
            sections.setdefault("relationships", []).append(
                (line, weak_sorted[0])
            )
        quality = _quality_line(data)
        if quality:
            sections.setdefault("wrong", []).insert(
                0, (quality.removeprefix("quality: "), {})
            )

        def omission(omitted: list[dict]) -> str:
            by_category: dict[str, int] = {}
            for rep in omitted:
                cat = rep.get("category") or "?"
                by_category[cat] = by_category.get(cat, 0) + 1
            ordered = sorted(by_category.items(), key=lambda kv: (-kv[1], kv[0]))
            names = ", ".join(f"{c} {n}" for c, n in ordered[:4])
            retrieve = (
                f'; report.select(category="{ordered[0][0]}") retrieves them'
                if ordered
                else ""
            )
            return (
                f"... [{len(omitted)} fact(s) omitted to fit "
                f"{budget_chars} chars; {len(findings)} findings total: "
                f"{names}{retrieve}]"
            )

        titles = dict(_DIGEST_SECTION_TITLES)
        if designated:
            titles["target"] = f"What relates to the target ({designated})"
        blocks: dict[str, list[str]] = {}
        if row_lines:
            blocks["row"] = [titles["row"] + ":"] + row_lines
        if not_examined:
            blocks["not_examined"] = [titles["not_examined"] + ":", not_examined]

        fixed_text = "\n".join(fixed + [l for b in blocks.values() for l in b])
        # Reserve room for the section titles and the omission line.
        reserve = len(omission([f["rep"] for f in facts])) + 1 if facts else 0
        room = budget_chars - len(fixed_text) - reserve
        for name in ("wrong", "redundant", "target", "relationships", "anomalies"):
            room -= len(titles[name]) + 2
        shares = dict(_DIGEST_SECTION_SHARES)
        if not designated:
            shares["relationships"] += shares.pop("target")
        kept: dict[str, list[str]] = {}
        omitted_reps: list[dict] = []
        waiting: dict[str, list[tuple[str, dict]]] = {}
        used_total = 0
        for name in ("wrong", "redundant", "target", "relationships", "anomalies"):
            entries = sections.get(name) or []
            if not entries:
                continue
            allowance = int(shares[name] * max(room, 0))
            if name in ("target", "relationships"):
                entries = relationship_sequence(entries, allowance)
            used = 0
            chosen = []
            rest = []
            for text, rep in entries:
                line = text if not rep else f"{text} [{rep['id']}]"
                if rep and used + len(line) + 1 > allowance:
                    rest.append((line, rep))
                    continue
                chosen.append(line)
                used += len(line) + 1
            if chosen:
                kept[name] = chosen
            if rest:
                waiting[name] = rest
            used_total += used
        # A section that cannot fill its share does not strand the
        # budget: what is left passes to the sections still holding
        # facts, in section order, so every section's guaranteed
        # share is honoured first and the briefing still fills.
        leftover = max(room, 0) - used_total
        for name in ("wrong", "redundant", "target", "relationships", "anomalies"):
            for line, rep in waiting.get(name, []):
                if len(line) + 1 > leftover:
                    omitted_reps.append(rep)
                    continue
                kept.setdefault(name, []).append(line)
                leftover -= len(line) + 1
        for name, lines in kept.items():
            blocks[name] = [titles[name] + ":"] + lines

        def assemble(block_map: dict[str, list[str]], omitted: list[dict]) -> str:
            parts = list(fixed)
            for name in _DIGEST_SECTION_ORDER:
                block = block_map.get(name) or []
                if name not in ("row", "not_examined") and len(block) <= 1:
                    continue
                parts.extend(block)
            if omitted:
                parts.append(omission(omitted))
            return "\n".join(parts) + "\n"

        result = assemble(blocks, omitted_reps)
        # The fixed blocks and the omission line are estimates
        # against the real total; give back kept lines, last
        # section first, until the briefing fits.
        while len(result) > budget_chars:
            shrunk = False
            for name in reversed(_DIGEST_SECTION_ORDER):
                lines = blocks.get(name)
                if lines and len(lines) > 1:
                    dropped = lines.pop()
                    if name in kept:
                        for text, rep in sections.get(name, []):
                            if rep and f"{text} [{rep['id']}]" == dropped:
                                omitted_reps.append(rep)
                    shrunk = True
                    break
            if not shrunk:
                break
            result = assemble(blocks, omitted_reps)
        if len(result) > budget_chars:
            # Degrade the fixed blocks: the not-examined block goes
            # first, then the row block; the profile line and the
            # omission line are the last content to go.
            for drop in (("not_examined",), ("row", "not_examined")):
                smaller = {k: v for k, v in blocks.items() if k not in drop}
                result = assemble(smaller, omitted_reps)
                if len(result) <= budget_chars:
                    return result
            return (profile_line + "\n")[:budget_chars]
        return result

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

    def to_html(self) -> str:
        """The report as one self-contained HTML document.

        A rendering of the same report object `.data` / `.to_json()`
        define — every claim and number is read from the report, and
        nothing is recomputed from the source table. Charts are
        Altair / Vega-Lite with the Vega bundle inlined once, so the
        file opens offline and makes no external requests. Requires
        the optional `html` extra (`pip install 'onceover[html]'`);
        without it, raises ImportError naming the extra.
        """
        from . import _html

        return _html.render(self.data, version=__version__)

    def save(self, path: str, fmt: str = "json") -> str:
        """Write the report to `path` as json, markdown or html.

        The html format needs the optional `html` extra (see
        `to_html`). Returns the path.
        """
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
        # The notebook display is the digest, not the full report:
        # a full markdown report runs to tens of thousands of
        # characters on a finding-rich table, which floods the cell
        # output. The digest is budgeted for exactly this surface.
        # The footer names the full surfaces and states no omitted
        # count: the digest rolls findings up into groups, so a
        # count of rendered findings is not defined by its text.
        return (
            self.to_digest()
            + "\nFull report: `report.to_markdown()`; a larger "
            "digest: `report.to_digest(n)`; a subset of findings: "
            "`report.select(...)`.\n"
        )
