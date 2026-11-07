# Supported Polars floor: correction — the 1.x line passes with the test fixed

`docs/benchmarks/2026-11-06-polars-floor.md` recorded why the
supported Polars floor moved to 2.0: under the wheel job's
oldest-line conditions (Python 3.15, the built wheel, pytest,
scipy), the full suite failed on Polars 1.44.2 with exactly one
failure, `tests/test_profile_many.py::test_temporal_family_dates`,
and passed on 2.0.0. That diagnosis stands — the failure and its
root cause are as recorded there. The conclusion does not: the
supported floor stays in the 1.x line, and the remedy is a fix to
the test, not a narrower range. This note records the corrected
outcome and the measurement behind it.

## The defect, restated

The failing test built its date columns with
`pl.Series(["2024-01-01", ...]).str.to_date()`. In the Polars 1.x
line on Python 3.15 that Series-namespace call silently returns
None, so the fixture's columns were `Null` and `profile_many`
correctly declined to examine them. The library never calls the
method; its temporal code uses expression forms. Probed again on
the oldest 1.x release (1.28.1, Python 3.15.2) while preparing
this change:

| construction | result on polars 1.28.1, Python 3.15.2 |
|---|---|
| `Series.str.to_date()` | returns None |
| `Series.str.to_datetime()` | returns None |
| `pl.DataFrame({...}).select(pl.col("day").str.to_date())` | Date column, correct values |
| `pl.Series([date(2024, 1, 1), ...])` (Python dates) | Date column, correct values |
| `pl.Series([datetime(2024, 1, 1), ...])` (Python datetimes) | raises `AttributeError` — the internal `cast_time_unit` path recorded on 2026-11-04 |

## The fix

`test_temporal_family_dates` now builds its dates with the
expression form — `pl.DataFrame({"day": [...]})` followed by
`.select(pl.col("day").str.to_date())` — with a comment in the
test naming the defect. No library code changes.

## Sweep of the same defect family

Every occurrence of the Series-namespace construction family
(`.str.to_date(`, `.str.to_datetime(`, `.dt.cast_time_unit(`) in
tests, the Python package, scripts, benchmarks, notebooks and
book sources:

- `tests/test_profile_many.py` — the two fixture lines. Fixed
  (above).
- `python/onceover/_checks.py` — one `str.to_datetime` call, on a
  `pl.lit(...)` expression inside a larger expression. Expression
  forms are not affected by the defect. No change.
- `tests/test_frame_model.py` — the 2026-11-04 instance: the one
  test that constructs timestamps from Python `datetime` objects
  already builds them from integer epoch microseconds and a cast,
  with a comment recording why. No change.
- The dated benchmark notes mention the calls as records, not
  code. No change.

Tests that build columns from Python `date` objects (several)
are unaffected: that construction was probed working on 1.28.1
above, and the suite runs below cover it.

## The floor measurement

Full suite (`python -m pytest tests/ -q`) in fresh Python 3.15.2
venvs with the wheel built from this change's tree, pytest
9.2.0 and scipy 1.18.1 installed — the wheel step's exact
conditions — varying only Polars:

| polars | result |
|---|---|
| 1.28.1 (oldest 1.x; what `polars>=1.28,<1.29` resolves) | 472 passed, 4 skipped |
| 1.44.2 (newest 1.x) | 472 passed, 4 skipped |
| 2.1.0 (current) | 472 passed, 4 skipped |

The declared floor before the 2.0 change was 1.28, and 1.28.1 is
green with the fixture fixed, so the floor stays 1.28 — no
bisection within 1.x was needed.

## Change

- `tests/test_profile_many.py`: the date fixture uses the
  expression form (above).
- `pyproject.toml`: the package requirement is back to
  `polars>=1.28,<3`.
- CI: the step renamed by the 2.0 change keeps its name ("Test
  wheel on oldest supported Polars") and pins
  `polars>=1.28,<1.29` — the floor line's newest patch, 1.28.1
  today; the timing job's floor is back to `polars>=1.28`.
- The stated range is restored where the 2.0 change moved it:
  `README.md`, the `profile()` docstring, and the book's install
  page read "Polars 1.28–2.x" again, and the book's release
  notes record that the range remains 1.28 and later.
