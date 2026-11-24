"""onceover: first look at tabular data.

A native Rust profiler for Polars DataFrames (`profile`, `infer_spec`,
`profile_many`, `partition_scagnostics`) with a minimal Python binding layer.

New callers: `agent_guide()` returns the packaged quickstart for
this surface; `capabilities()` returns its compact machine-readable
companion.
"""

import json as _json

from onceover._digest import (  # noqa: F401
    _DIGEST_SECTION_TITLES,
    _digest_numbers,
    _quality_line,
    _statement_class,
)
from onceover._many import MultiTableReport, profile_many
from onceover._native import ProfileReport as _NativeProfileReport  # noqa: F401
from onceover._native import available_detectors as _available_native
from onceover._native import profile as _profile_native
from onceover._report import (  # noqa: F401
    ProfileReport,
    _findings_frame_schema,
)
from onceover._source import (  # noqa: F401
    _DEFAULT_SOURCE_ROW_CAP,
    _SOURCE_ROW_INDEX,
    _as_int,
    _materialize_source,
    _scan_path,
    _stratified_indices,
)
from onceover._spec import (  # noqa: F401
    _SPEC_ROLES,
    _SPEC_VERSION,
    _apply_spec,
    infer_spec,
)

__version__ = "0.1.10"
__all__ = [
    "profile",
    "ALL_DETECTORS",
    "RECOMMENDED_DETECTORS",
    "partition_scagnostics",
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


class _RecommendedDetectors(tuple):
    __slots__ = ()

    def __repr__(self):
        return repr(list(self))


_registry = _json.loads(_available_native())
ALL_DETECTORS = tuple(detector["id"] for detector in _registry)
RECOMMENDED_DETECTORS = _RecommendedDetectors(
    detector["id"]
    for detector in _registry
    if detector["membership"] == "default_set"
)
del _registry


def partition_scagnostics(
    df, *, pairs=None, partition_by=None, metrics=None, seed: int = 0
) -> dict:
    """Find scatterplot structure that becomes stronger within observed groups.

    ``pairs`` selects numeric column pairs to examine; ``partition_by`` selects
    candidate context columns; ``metrics`` selects any non-empty subset of
    ``"monotonic"``, ``"functional"``, ``"binned_striated"``,
    ``"binned_stringy"``, and ``"binned_clumpy"`` (all five by default).
    Sampling and discovery/holdout splitting are content-keyed and
    deterministic for a given ``seed``.
    """
    from onceover._native import (
        partition_scagnostics as _partition_scagnostics_native,
    )

    return _json.loads(
        _partition_scagnostics_native(
            df,
            pairs=pairs,
            partition_by=partition_by,
            metrics=metrics,
            seed=seed,
        )
    )


def profile(
    df,
    budget: str = "standard",
    deadline_seconds: float | None = None,
    *,
    sample_rows: int | None = None,
    max_predictors: int | None = None,
    max_pairs: int | None = None,
    detectors: list[str] | tuple[str, ...] | None = RECOMMENDED_DETECTORS,
    max_findings: int = 100,
    max_rules: int = 20,
    hints: dict | None = None,
    target: str | None = None,
    timing: bool = False,
    source_row_cap: int | None = None,
    exact: bool = False,
    sample_seed: int = 0,
    spec: dict | None = None,
):
    """Give a Polars DataFrame, LazyFrame, or file path a once-over.

    New callers: `agent_guide()` returns the packaged quickstart;
    `capabilities()` returns its compact machine-readable companion.

    By default `detectors` selects `RECOMMENDED_DETECTORS` (31 of the
    48 registered detectors); pass `detectors=ALL_DETECTORS` to request
    every registered detector, or a list of detector IDs from
    `available_detectors()`.
    """
    import polars as _pl

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
        if isinstance(detectors, str) or not isinstance(
            detectors, (list, tuple)
        ):
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
                "and file inputs; a DataFrame is already loaded in full "
                "(no source-level sampling); use budget='deep' or "
                "sample_rows= to raise internal pass caps"
            )
    else:
        df, source = _materialize_source(
            df,
            source_row_cap=source_row_cap,
            exact=exact,
            sample_seed=sample_seed,
        )
    if isinstance(detectors, _RecommendedDetectors):
        detectors = None
    return ProfileReport(
        _profile_native(
            df,
            budget,
            deadline_seconds,
            sample_rows=sample_rows,
            max_predictors=max_predictors,
            max_pairs=max_pairs,
            detectors=detectors,
            max_findings=max_findings,
            max_rules=max_rules,
            hints=_json.dumps(hints) if hints is not None else None,
            target=target,
            timing=timing,
            source=_json.dumps(source) if source is not None else None,
        )
    )


def available_detectors():
    """Available detector IDs, produced categories, resource dependencies,
    membership, trigger, and a one-line purpose per detector."""
    from onceover._native import available_detectors as _available

    return _json.loads(_available())


def taxonomy():
    """The finding taxonomy: every pattern with its type, its derived
    category, and its facets, plus registered forms and facet vocabularies."""
    from onceover._native import taxonomy as _taxonomy

    return _json.loads(_taxonomy())


def capabilities() -> dict:
    """The compact capability manifest derived from the detector registry."""
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
    """The packaged agent guide shipped inside the package."""
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
