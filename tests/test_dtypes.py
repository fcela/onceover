"""Regression tests for issue #75: profile() accepts every Polars dtype.

Series construction for the narrow integer widths (Int8/Int16/UInt8/
UInt16), Decimal, Struct and fixed-size Array is feature-gated in the
Rust polars build; with the gates off, ingestion raised
``ComputeError: cannot create series from <dtype>`` (and panicked for
Decimal) before any detector ran. Narrow integers must be profiled as
numbers; Decimal/Struct/Array columns are opaque to the profiler but
must not break the profile of the rest of the frame.
"""

import datetime

import polars as pl
import pytest

import onceover

INT_DTYPES = [
    pl.Int8,
    pl.Int16,
    pl.Int32,
    pl.Int64,
    pl.UInt8,
    pl.UInt16,
    pl.UInt32,
    pl.UInt64,
]


@pytest.mark.parametrize("dtype", INT_DTYPES)
def test_integer_widths_profile(dtype):
    df = pl.DataFrame({"x": pl.Series([1, 2, 3, 4, 5, 6, 7, 8], dtype=dtype)})
    rep = onceover.profile(df, budget="standard")
    card = rep.columns["x"]
    assert card["n"] == 8
    assert card["distinct"] == 8


@pytest.mark.parametrize("dtype", INT_DTYPES)
def test_integer_widths_carry_values(dtype):
    # A constant narrow-int column reads as constant, not as nulls:
    # the values must survive ingestion, not just the dtype label.
    df = pl.DataFrame({"x": pl.Series([7] * 20, dtype=dtype)})
    rep = onceover.profile(df, budget="standard")
    card = rep.columns["x"]
    assert card["distinct"] == 1
    assert card["null_count"] == 0


def _opaque_frames():
    return {
        "decimal": pl.DataFrame(
            {"x": pl.Series(["1.50", "2.50", "3.50"], dtype=pl.Decimal(10, 2))}
        ),
        "struct": pl.DataFrame({"x": pl.Series([{"a": 1}, {"a": 2}, {"a": 3}])}),
        "array": pl.DataFrame(
            {"x": pl.Series([[1, 2], [3, 4], [5, 6]], dtype=pl.Array(pl.Int64, 2))}
        ),
    }


@pytest.mark.parametrize("kind", ["decimal", "struct", "array"])
def test_opaque_dtypes_profile_without_error(kind):
    df = _opaque_frames()[kind]
    other = pl.DataFrame({"y": [10, 20, 30]})
    rep = onceover.profile(df.hstack(other), budget="quick")
    assert set(rep.columns) == {"x", "y"}


@pytest.mark.parametrize(
    "series",
    [
        pl.Series([1.5, 2.5, 3.5], dtype=pl.Float32),
        pl.Series(
            [datetime.timedelta(days=1), datetime.timedelta(days=2)],
            dtype=pl.Duration,
        ),
        pl.Series([datetime.time(1, 2), datetime.time(3, 4)], dtype=pl.Time),
        pl.Series([[1, 2], [3]], dtype=pl.List(pl.Int64)),
        pl.Series(["a", "b"], dtype=pl.Categorical),
        pl.Series([b"a", b"bc"], dtype=pl.Binary),
    ],
)
def test_other_dtypes_still_profile(series):
    rep = onceover.profile(pl.DataFrame({"x": series}), budget="quick")
    assert set(rep.columns) == {"x"}
