"""Regression tests for issue #75: profile() accepts every Polars dtype.

Series construction for the narrow integer widths (Int8/Int16/UInt8/
UInt16), Decimal, Struct and fixed-size Array is feature-gated in the
Rust polars build; with the gates off, ingestion raised
``ComputeError: cannot create series from <dtype>`` (and panicked for
Decimal) before any detector ran. Narrow integers must be profiled as
numbers; Struct/Array columns are opaque to the profiler but must not
break the profile of the rest of the frame.

Issue #211: Decimal columns must be analysed as numbers — a numeric
card, a measure role, and visibility to the numeric passes — not
merely accepted and left opaque.

Issue #216: Duration columns get the same treatment, analysed as
numbers in seconds. Time-of-day columns are counted but not
analysed: a time of day is a circular quantity, so order statistics
would mislead, and coverage states the treatment plainly.
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
        "struct": pl.DataFrame({"x": pl.Series([{"a": 1}, {"a": 2}, {"a": 3}])}),
        "array": pl.DataFrame(
            {"x": pl.Series([[1, 2], [3, 4], [5, 6]], dtype=pl.Array(pl.Int64, 2))}
        ),
    }


@pytest.mark.parametrize("kind", ["struct", "array"])
def test_opaque_dtypes_profile_without_error(kind):
    df = _opaque_frames()[kind]
    other = pl.DataFrame({"y": [10, 20, 30]})
    rep = onceover.profile(df.hstack(other), budget="quick")
    assert set(rep.columns) == {"x", "y"}


def test_decimal_profiles_as_a_number():
    # 120 money-like values: 40 distinct quarter steps, each seen
    # 3 times, so the column reads as a measure.
    values = [f"{(i % 40) * 0.25 + 0.5:.2f}" for i in range(120)]
    df = pl.DataFrame({"price": pl.Series(values, dtype=pl.Decimal(10, 2))})
    rep = onceover.profile(df, budget="standard")
    card = rep.columns["price"]
    assert card["dtype"] == "decimal"
    assert card["role"] == "continuous"
    assert card["semantic_role"] == "measure"
    assert card["distinct"] == 40
    stats = card["numeric_stats"]
    assert stats["min"] == 0.5
    assert stats["max"] == 10.25
    assert stats["mean"] == pytest.approx(5.375)


def test_decimal_nulls_are_counted():
    series = pl.Series(
        ["1.50", None, "2.50", None, "3.50", "4.50"], dtype=pl.Decimal(10, 2)
    )
    rep = onceover.profile(pl.DataFrame({"price": series}), budget="standard")
    card = rep.columns["price"]
    assert card["n"] == 6
    assert card["null_count"] == 2
    assert card["distinct"] == 4
    assert card["numeric_stats"]["mean"] == pytest.approx(3.0)


def test_decimal_affine_identity_is_detected():
    from _findings import select

    # double_price = 2 * price exactly, both decimal: the identity
    # pass must see the pair, as it does for float columns.
    prices = [((i * 37) % 40) * 0.25 + 1.0 for i in range(240)]
    df = pl.DataFrame(
        {
            "price": pl.Series([f"{p:.2f}" for p in prices], dtype=pl.Decimal(10, 2)),
            "double_price": pl.Series(
                [f"{2 * p:.2f}" for p in prices], dtype=pl.Decimal(10, 2)
            ),
        }
    )
    report = onceover.profile(df)
    assert select(
        report,
        "dependencies",
        subtype="functional",
        form="affine",
        column="double_price",
        include_grouped=True,
    )


def test_duration_profiles_as_a_number():
    # 120 span-like values: 40 distinct quarter-second steps, each
    # seen 3 times, so the column reads as a measure. Statistics are
    # in seconds: the stored ticks divided by the ticks per second.
    values = [
        datetime.timedelta(seconds=(i % 40) * 0.25 + 0.5) for i in range(120)
    ]
    df = pl.DataFrame({"elapsed": pl.Series(values, dtype=pl.Duration)})
    rep = onceover.profile(df, budget="standard")
    card = rep.columns["elapsed"]
    assert card["dtype"] == "duration"
    assert card["role"] == "continuous"
    assert card["semantic_role"] == "measure"
    assert card["distinct"] == 40
    stats = card["numeric_stats"]
    assert stats["min"] == 0.5
    assert stats["max"] == 10.25
    assert stats["mean"] == pytest.approx(5.375)


def test_duration_units_agree():
    # The same span stored at two time units extracts to the same
    # measure in seconds.
    df = pl.DataFrame(
        {
            "ms": pl.Series(
                [datetime.timedelta(milliseconds=1500)] * 30, dtype=pl.Duration("ms")
            ),
            "us": pl.Series(
                [datetime.timedelta(microseconds=1_500_000)] * 30,
                dtype=pl.Duration("us"),
            ),
        }
    )
    rep = onceover.profile(df, budget="standard")
    assert rep.columns["ms"]["numeric_stats"]["mean"] == pytest.approx(1.5)
    assert rep.columns["us"]["numeric_stats"]["mean"] == pytest.approx(1.5)


def test_duration_nulls_are_counted():
    series = pl.Series(
        [
            datetime.timedelta(seconds=1.5),
            None,
            datetime.timedelta(seconds=2.5),
            None,
            datetime.timedelta(seconds=3.5),
            datetime.timedelta(seconds=4.5),
        ],
        dtype=pl.Duration,
    )
    rep = onceover.profile(pl.DataFrame({"elapsed": series}), budget="standard")
    card = rep.columns["elapsed"]
    assert card["n"] == 6
    assert card["null_count"] == 2
    assert card["distinct"] == 4
    assert card["numeric_stats"]["mean"] == pytest.approx(3.0)


def test_duration_affine_identity_is_detected():
    from _findings import select

    # double_elapsed = 2 * elapsed exactly, both durations: the
    # identity pass must see the pair, as it does for float columns.
    spans = [((i * 37) % 40) * 0.25 + 1.0 for i in range(240)]
    df = pl.DataFrame(
        {
            "elapsed": pl.Series(
                [datetime.timedelta(seconds=s) for s in spans], dtype=pl.Duration
            ),
            "double_elapsed": pl.Series(
                [datetime.timedelta(seconds=2 * s) for s in spans], dtype=pl.Duration
            ),
        }
    )
    report = onceover.profile(df)
    assert select(
        report,
        "dependencies",
        subtype="functional",
        form="affine",
        column="double_elapsed",
        include_grouped=True,
    )


def test_time_is_counted_but_not_analysed():
    series = pl.Series(
        [datetime.time(1, 2), datetime.time(3, 4), None, datetime.time(23, 30)],
        dtype=pl.Time,
    )
    rep = onceover.profile(pl.DataFrame({"at": series}), budget="quick")
    card = rep.columns["at"]
    assert card["dtype"] == "time"
    assert card["n"] == 4
    assert card["null_count"] == 1
    # No statistics and no measure treatment: a time of day is a
    # circular quantity, so order statistics would mislead.
    assert card["numeric_stats"] is None
    assert card["semantic_role"] is None
    # Coverage states the treatment plainly, naming the column.
    detail = rep.coverage["extraction"]["detail"]
    assert "time-of-day" in detail
    assert "counted but not analysed" in detail
    assert "at" in detail


def test_time_is_not_counted_as_unsupported():
    # A frame with an unsupported column and a time column states
    # the two separately in the extraction coverage entry: the time
    # column is not lumped in with the unsupported types.
    df = pl.DataFrame(
        {
            "s": pl.Series([{"a": 1}, {"a": 2}, {"a": 3}]),
            "at": pl.Series(
                [datetime.time(1, 0), datetime.time(2, 0), datetime.time(3, 0)],
                dtype=pl.Time,
            ),
        }
    )
    rep = onceover.profile(df, budget="quick")
    detail = rep.coverage["extraction"]["detail"]
    assert "1 unsupported column type(s)" in detail
    assert "1 time-of-day column(s) counted but not analysed" in detail


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
