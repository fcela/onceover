# Supported Polars floor: the 1.x line fails on Python 3.15, floor moves to 2.0

On main `3f0ac8de` (merges through PR #191) exactly one CI check
was red: the wheel job's "Test wheel on Polars 1.x" step
(linux-x86_64 only). Every other step in that job passed — the
build, the Python 3.13 step, the example notebook, the minimum
Python step, and the latest Python step. Job logs are not
downloadable through the API token, so the step was reproduced
locally in the exact environment it runs in (this machine is
linux-x86_64, the same platform).

## What the step runs, and what pip resolves

The step runs under Python 3.15 (the interpreter left in place by
the preceding "Test latest Python version" step) and installs the
built wheel, pytest, `scipy>=1.15`, and `polars>=1.28,<2`. What
each wheel step resolves today:

| step | interpreter | polars resolved |
|---|---|---|
| Test wheel on Python 3.13 | 3.13 | 2.1.0 |
| Test minimum Python version | 3.10 | 2.1.0 |
| Test latest Python version | 3.15 | 2.1.0 |
| Test wheel on Polars 1.x | 3.15 | **1.44.2** |

1.44.2 is the newest 1.x release; 2.1.0 is the newest release
overall. The library declared `polars>=1.28,<3`, so the 1.x step
is the only place the 1.x line is exercised.

## Reproduction and root cause

Fresh venv, Python 3.15.2, the wheel built from `3f0ac8de`,
pytest 9.2.0, scipy 1.18.1, polars 1.44.2 — the full suite gives
`1 failed, 471 passed, 4 skipped`. The one failure:

```
FAILED tests/test_profile_many.py::test_temporal_family_dates
tests/test_profile_many.py:176: AssertionError: assert None is not None
```

The test builds two one-column tables of dates and asks
`profile_many` for the candidate foreign key between them. The
candidate is missing because the fixture's `day` columns are not
dates at all: coverage records both columns as type `Null`,
"not examined". The fixture constructs its dates with
`pl.Series(["2024-01-01", ...]).str.to_date()`, and in the
Polars 1.x line on Python 3.15 that call **silently returns
None** — no warning, no error — so the DataFrame is built with a
null column. Probed directly:

| environment | `Series.str.to_date()` |
|---|---|
| polars 1.28.1, Python 3.15.2 | returns None |
| polars 1.35.2, Python 3.15.2 | returns None |
| polars 1.44.2, Python 3.15.2 | returns None |
| polars 1.44.2, Python 3.12.3 | Date series, correct values |
| polars 2.0.0, Python 3.15.2 | Date series, correct values |
| polars 2.1.0, Python 3.15.2 | Date series, correct values |

The expression form (`pl.col("x").str.to_date()`) and
`Series.str.to_datetime()` work in the same 1.44.2 environment;
the defect is specific to the Series-namespace `str.to_date()`
in the 1.x line on Python 3.15. The library never calls it —
the only occurrences in the repository are the two fixture
lines in `tests/test_profile_many.py` (added with `profile_many`
in PR #186, the first change to exercise this construction; the
library's own temporal code uses expression forms). This is the
same defect family recorded on 2026-11-04
(`docs/benchmarks/2026-11-04-polars-latest-wheel-step.md`):
then, `Series.dt.cast_time_unit("us")` returned None under
polars 1.44.2 on Python 3.15 and broke a different test's
construction. That instance was routed around in the test; the
pattern has now recurred in a second place.

## The floor measurement

Full suite (`python -m pytest tests/ -q`) in fresh Python
3.15.2 venvs with the built wheel, pytest and scipy installed —
the failing step's exact conditions — varying only Polars:

| polars | result |
|---|---|
| 1.44.2 (newest 1.x) | 1 failed, 471 passed, 4 skipped |
| 2.0.0 | 472 passed, 4 skipped |
| 2.1.0 (current) | 472 passed, 4 skipped |

2.0.0 is the oldest Polars release at which the suite passes
under these conditions; the version just below it, 1.44.2,
fails. The supported range is therefore narrowed to the 2.x
line, with the floor one minor behind the current release.

## Change

- `pyproject.toml`: the package requirement is now
  `polars>=2.0,<3`.
- CI: the wheel step that exercised the old line is renamed
  "Test wheel on oldest supported Polars" and pins
  `polars>=2.0,<2.1` (the floor line's newest patch, 2.0.0
  today); the timing job's floor moves from `polars>=1.28` to
  `polars>=2.0`.
- The stated range is updated where it appears: `README.md`,
  the `profile()` docstring, and the book's install page; the
  book's release notes record the change under Unreleased.

No library code and no test changes: the failing test is
correct against a supported Polars, and the floor — not the
test — was the thing out of line.

## Correction — later the same day

The floor conclusion above was superseded the same day: the
supported floor stays in the Polars 1.x line, and the remedy is
the test fix this note did not make. The diagnosis above — the
one failing test, its root cause, and the 2.0.0 measurement —
stands as recorded; the range change it justified was reverted.
See `docs/benchmarks/2026-11-06-polars-floor-correction.md` for
the corrected outcome and its measurement: with the fixture
built by the expression form, the full suite passes on Polars
1.28.1, 1.44.2 and 2.1.0 alike, and the floor stays 1.28.
