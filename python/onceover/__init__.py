"""onceover: first look at tabular data.

A native Rust profiler for Polars DataFrames.
"""

import json as _json

from onceover._native import ProfileReport as _NativeProfileReport
from onceover._native import profile as _profile_native

__version__ = "0.1.0"
__all__ = ["profile", "ProfileReport"]

_SCHEMA_VERSION = "0.1"


def profile(df, budget: str = "standard", deadline_seconds: float | None = None):
    """Give a Polars DataFrame a once-over.

    Parameters
    ----------
    df : polars.DataFrame
        The table to profile (zero-copy Arrow interchange; polars >= 1.x).
    budget : str
        "quick" (summaries and integrity checks), "standard" (all passes with caps),
        or "deep" (wider caps and deeper search).
    deadline_seconds : float, optional
        Soft wall-clock budget. Deterministic by default (None); when set,
        remaining passes may be skipped and reported in `coverage`.

    Returns
    -------
    ProfileReport
        Ranked findings ledger + per-column cards + coverage block.
    """
    return ProfileReport(_profile_native(df, budget, deadline_seconds))


class ProfileReport:
    """Profiling results and evidence.

    Access the full versioned JSON via `.data` (dict), the ranked findings
    via `.findings`, the one-glance summary via `.overview`, per-column
    cards via `.columns`, and pass coverage via `.coverage`.
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
