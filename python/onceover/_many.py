"""Thin Python wrapper for Rust multi-table profiling (`profile_many`)."""

import json as _json
from collections.abc import Mapping

import polars as pl

from onceover._native import (
    log_choose as _native_log_choose,
    profile_many as _native_profile_many,
)


def _log_choose(n: int, k: int) -> float:
    if k < 0 or k > n:
        return float("-inf")
    return _native_log_choose(int(n), int(k))


class MultiTableReport:
    """Result of :func:`profile_many`: candidate foreign keys and overlaps.

    All set algebra, probing, chance scoring, JSON budgeting, and digest
    rendering run in Rust (`onceover::many`).
    """

    def __init__(self, native_or_data):
        if isinstance(native_or_data, dict):
            self._native = None
            self._data = native_or_data
        else:
            self._native = native_or_data
            self._data = None

    @property
    def data(self) -> dict:
        if self._data is None:
            self._data = _json.loads(self._native.to_json())
        return self._data

    @property
    def tables(self):
        return self.data["tables"]

    @property
    def candidates(self):
        return self.data["candidates"]

    @property
    def overlaps(self):
        return self.data["overlaps"]

    @property
    def coverage(self):
        return self.data["coverage"]

    def candidates_frame(self):
        rows = [
            {k: v for k, v in record.items() if not isinstance(v, (list, dict))}
            for record in self.candidates
        ]
        return pl.DataFrame(rows)

    def overlaps_frame(self):
        rows = [
            {k: v for k, v in record.items() if not isinstance(v, (list, dict))}
            for record in self.overlaps
        ]
        return pl.DataFrame(rows)

    def to_json(self, max_chars: int | None = None) -> str:
        return self._native.to_json(max_chars=max_chars)

    def to_digest(self, budget_chars: int = 4000) -> str:
        return self._native.to_digest(budget_chars=budget_chars)

    def __repr__(self) -> str:
        strong = sum(1 for c in self.candidates if c["tier"] == "strong")
        return (
            f"MultiTableReport(tables={len(self.tables)}, "
            f"candidates={len(self.candidates)} "
            f"({strong} strong, {len(self.candidates) - strong} lower-tier), "
            f"overlaps={len(self.overlaps)})"
        )


def profile_many(tables, *, overlap_threshold: float = 0.9) -> MultiTableReport:
    """Report candidate foreign keys and partial overlaps across tables (in Rust)."""
    if not isinstance(tables, Mapping):
        raise TypeError("profile_many expects a mapping of table name to frame")
    if not tables:
        raise ValueError("profile_many requires at least one table")
    if not (0 < overlap_threshold <= 1):
        raise ValueError("overlap_threshold must be in (0, 1]")
    pairs = []
    for name, value in tables.items():
        if not isinstance(name, str):
            raise TypeError("table names must be strings")
        if isinstance(value, pl.LazyFrame):
            value = value.collect()
        if not isinstance(value, pl.DataFrame):
            raise TypeError(
                f"table {name!r} must be a Polars DataFrame or LazyFrame, "
                f"got {type(value).__name__}"
            )
        pairs.append((name, value))
    return MultiTableReport(
        _native_profile_many(pairs, overlap_threshold=float(overlap_threshold))
    )
