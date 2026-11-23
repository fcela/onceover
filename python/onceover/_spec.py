"""Thin Python wrapper for Rust frame spec inference and application."""

import json as _json

from onceover._native import (
    apply_spec as _native_apply_spec,
    infer_spec as _native_infer_spec,
)

from ._source import _materialize_source

_SPEC_VERSION = 1
_SPEC_ROLES = (
    "identifier",
    "row_number",
    "timestamp",
    "timestamp_part",
    "dimension",
    "flag",
    "measure",
    "text",
)


def infer_spec(df, hints: dict | None = None, target: str | None = None) -> dict:
    """The frame's resolved interpretation without profiling it (runs in Rust)."""
    import polars as _pl

    if not isinstance(df, _pl.DataFrame):
        df, _ = _materialize_source(
            df, source_row_cap=None, exact=False, sample_seed=0
        )
    hints_json = _json.dumps(hints) if hints else None
    return _json.loads(_native_infer_spec(df, hints=hints_json, target=target))


def _apply_spec(df, spec: dict, hints: dict | None, target: str | None):
    """Validate and translate a spec into `(hints, target)` in Rust."""
    import polars as _pl

    if not isinstance(spec, dict):
        raise ValueError(
            f"spec must be a dict with spec_version {_SPEC_VERSION} (from infer_spec)"
        )
    if not isinstance(df, _pl.DataFrame):
        df, _ = _materialize_source(
            df, source_row_cap=None, exact=False, sample_seed=0
        )
    hints_json = _json.dumps(hints) if hints else None
    resolved_json, final_target = _native_apply_spec(
        df, _json.dumps(spec), hints=hints_json, target=target
    )
    resolved = _json.loads(resolved_json)
    # Strip empty default fields so downstream hints stay minimal.
    compact_hints = {k: v for k, v in resolved.items() if v}
    return compact_hints, final_target
