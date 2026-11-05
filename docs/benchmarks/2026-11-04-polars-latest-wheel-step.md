# Wheel CI: the Polars 1.x step and Python 3.15

On main `65712306` (merges through PR #142) exactly one CI step was
red: "Test wheel on Polars 1.x" (linux-x86_64 only), in run
41916323238 on main and run 41914367575 on PR #141's branch. Every
other job and wheel step passed. Job logs are not downloadable
through the API token, so the step was reproduced locally in the
exact environment it runs in. The failure is one test, and the
defect is inside Polars, not in this library; the fix is a
test-construction change only. No library code changes.

## What the step actually runs

The wheels job runs its test steps sequentially in one job, so the
Polars step inherits the interpreter of the preceding
`setup-python` step:

| step | interpreter | installs (beyond the wheel) | polars resolved |
|---|---|---|---|
| Test wheel on Python 3.13 | 3.13 | pytest, scipy | 2.1.0 |
| Test minimum Python version | 3.10 | pytest, scipy | 2.0.0 |
| Test latest Python version | 3.15 | pytest | 2.1.0 |
| Test wheel on Polars 1.x | **3.15** | pytest, scipy, `polars>=1.28,<2` | **1.44.2** |

The library declares `polars>=1.28,<3`; the development environment
runs polars 2.1.0. The Polars step is the only place the 1.x line
is exercised, and the only place Python 3.15 meets it: 1.44.2 is
the latest 1.x release pip resolves today.

## Reproduction

Fresh venv, Python 3.15.2, the built wheel, pytest 9.2.0,
scipy 1.18.1, polars 1.44.2 — the full suite gives
`2 failed, 236 passed, 2 skipped`. One failure is
`tests/test_e2e.py::test_perf_standard_10k`, a wall-clock assertion
that also fails locally under every other environment on this
shared VM (see "The perf test" below). The other is the step's
failure:

```
FAILED tests/test_frame_model.py::test_datetime_axis_span_is_correct_in_every_time_unit
tests/test_frame_model.py:172: in test_datetime_axis_span_is_correct_in_every_time_unit
    base = pl.DataFrame({"ts": [start + timedelta(days=i) for i in range(400)], ...})
polars/_utils/construction/series.py:263: AttributeError: 'NoneType' object has no attribute '_s'
```

The test fails while *constructing* its input frame, before any
onceover code runs. Minimized against the installed polars:

- `pl.Series("ts", [datetime(2022, 5, 30), ...])` raises the same
  `AttributeError` under polars 1.44.2 on Python 3.15, for any
  list length; `pl.datetime_range` fails the same way.
- Inside polars' `sequence_to_pyseries`, the temporal branch ends
  at `return s._s` (series.py:263) with `s = None`: the preceding
  `Series.dt.cast_time_unit("us")` call **returns None** in this
  combination, even on a series already in `'us'`. Probed
  directly: `PySeries.new_from_any_values` and the wrapped series
  are fine; `s.dt.cast_time_unit("us")` is the call that yields
  None.
- Controls: the same construction works under polars 1.44.2 on
  Python 3.12, and under polars 2.1.0 on Python 3.15.2 (where
  `cast_time_unit` returns a series). Construction from dates,
  ISO strings, or integer ticks works in the broken combination
  too; only the Python-datetime path is affected.

The library itself never reaches the defect: neither
`python/onceover/` nor the Rust crates call `cast_time_unit` or
construct a Datetime series from Python datetime objects (grep
over both trees). `tests/test_events.py` already builds its
timestamps as epoch microseconds plus a cast, with a comment
recording this same polars-1.x/Python-3.15 defect; the frame-model
test's construction was the one place the pattern was missed.

## Fix

`tests/test_frame_model.py` now builds the base series the same
way `test_events.py` does: epoch microseconds as integers, cast
to `Datetime("us")`. The constructed series is value-identical to
the old construction (`Series.equals` against the
list-of-datetimes series in a working environment: same dtype,
400 midnights, 2022-05-30 through 2023-07-03). The test's actual
subject — the frame account's axis span after casts to `ms`,
`us`, and `ns` — is unchanged, as are its assertions. An
assertion was not weakened: the incidental construction route
changed, the durable property asserted did not.

## The perf test

`test_perf_standard_10k` asserts a 10,000-row profile completes in
under 5 s. Locally it fails in *every* environment tried —
both polars versions, Python 3.10/3.12/3.15 — whenever this VM's
load average is in the 7–10 range produced by concurrent agent
builds, with recorded profile times of 5.1–9.1 s tracking the
load, not the environment. Interleaved A/B timing of the same
profile on Python 3.15 shows no polars-version effect: per-pass
coverage timings are dominated by the same Rust passes in both
environments (discontinuities first, ≈1.4–1.8 s in both), warmup
totals overlap (2.1.0: 3,881 ms and 3,485 ms; 1.44.2: 4,028 ms and
2,786 ms), and total-profile medians overlap (2.1.0: 4.40 s and
2.53 s; 1.44.2: 5.08 s and 3.23 s, falling as load fell). In CI
the same test passes in the other three wheel steps on the same
runner class. It is recorded here as a local-load artifact of
these simulations, not as part of the step's failure and not as
fixed by this change; the PR's own CI is the unloaded check. In a
later, quieter window the same test passed solo in the exact
Polars-step environment (Python 3.15.2, polars 1.44.2), and it
passed in the final gate's full-suite run below.

## Acceptance simulations (fixed tree)

All four wheel variants, each in a fresh venv against the wheel
built from this tree (`maturin build --release`; the local wheel
was compiled with `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=8` and LTO
off, the disclosed local-build override — the change is
test-only, so the wheel's library code is identical either way).
Full `pytest tests/ -q` in each:

| variant | environment | result |
|---|---|---|
| (a) Python 3.13 step | Python 3.12.3 (CI: 3.13), polars 2.1.0, scipy 1.18.1 | 1 failed, 237 passed, 2 skipped |
| (b) latest Python | Python 3.15.2, polars 2.1.0, no scipy | 1 failed, 233 passed, 4 skipped |
| (c) Polars 1.x step | Python 3.15.2, polars 1.44.2, scipy 1.18.1 | 1 failed, 237 passed, 2 skipped |
| (d) minimum Python | Python 3.10.22, polars 2.0.0, scipy 1.15.3 | 1 failed, 237 passed, 2 skipped |

The single failure in each row is `test_perf_standard_10k` under
local load, per the section above; before the fix, row (c) also
failed the frame-model test (2 failed, 236 passed, 2 skipped).
Row (b)'s extra skips are the numpy-absent battery modules (the
`importorskip` from the previous wheel fix). Row (d)'s first
attempt was OOM-killed by concurrent builds and was re-run to
completion. Interpreters 3.15.2 and 3.10.22 were installed
locally via uv's standalone builds; row (a)'s interpreter is
3.12.3 in place of CI's 3.13, the one remaining local delta, and
it does not intersect the defect's combination (3.15 × 1.x).

## Gate

Final tree, CI order, unmasked exit codes: `scripts/release.py
check` exit 0 (metadata matches 0.1.3); `pytest
scripts/test_release.py` 5 passed; `cargo fmt --all -- --check`
exit 0; `cargo clippy --workspace --all-targets --locked -- -D
warnings` exit 0; `cargo test --workspace --locked` exit 0 (216
passed, 0 failed, 16 suites); `pytest tests/` exit 0 (238 passed,
2 skipped, development environment). Two environment notes: the
first local `cargo test` attempt lost a `polars-core` compile to
a SIGKILL while a sibling build ran (re-run clean, quoted above),
and the gate legs ran with the repository's own profiles — the
codegen-units override applied only to the local wheel built for
the simulations.
