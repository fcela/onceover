"""Thin Python delegation layer for Rust digest rendering."""

import json as _json

from onceover._native import (
    digest_numbers as _digest_numbers,
    quality_line_json as _quality_line_json,
    render_digest_json as _render_digest_json,
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


def _statement_class(finding: dict) -> int:
    """Mirror of `onceover_report::statement_class` for unit-test checks."""
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
    return _quality_line_json(_json.dumps(data))


def render_digest(
    data: dict,
    budget_chars: int = 4000,
    *,
    compact: bool = False,
    include_schema: bool = False,
) -> str:
    return _render_digest_json(
        _json.dumps(data),
        budget_chars=budget_chars,
        compact=compact,
        include_schema=include_schema,
    )
