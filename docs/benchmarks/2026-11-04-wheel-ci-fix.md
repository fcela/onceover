# Wheel CI fix: three causes in one step

On main `6744b051` (the merges of PRs #120, #138, #139 and #140 over
`613303d4`) every Wheel job failed at the "Test wheel on Python 3.13"
step (run 41901508431); Rust checks, the Timing gate and Docs passed.
Job logs are not downloadable through the API token, so each cause
was reproduced locally against a build of the exact main tree. The
step contained three independent failures, each a merge interaction
or a consumer of a deliberate behavior change; no library code is
changed by the fix.

## Cause 1: the claim gate's record went stale under it

`tests/test_claim_consistency.py` (PR #139) pins a `KNOWN_FAILURES`
record of exactly the contradictions reproduced on its base
`613303d4` and fails if the reproduced set differs in either
direction. PR #120, merged under it, fixed the numeric
extreme-cluster rendering in `surprises.rs`, retiring four of the
seven records. Reproduced locally: the gate failed on
`assert not stale` with exactly those four singleton-wording
records (Adult `work_class = Federal-gov`; Bike Sharing
`season = 1`, `hr = 21`, `hr = 5`).

Fix by re-measurement, not by editing the record blind: the gate's
own profiling was re-run on `6744b051`. Exactly three contradictions
reproduce — the categorical `share-implied-count` instances on
Adult, owned by issue #111 and not touched by PR #120:

- `education = Some-college and relationship = Husband`
- `occupation = Other-service and relationship = Own-child and race = White`
- `occupation = Craft-repair and relationship = Husband`

No new contradiction appeared (PR #138's frame changes introduced
none on these frames). The record now holds exactly these three;
the second record's scope string was extended to the full scope the
claim carries on current main. The exact-match assertion is
unchanged; nothing is xfailed.

## Cause 2: the battery test imports numpy at collection

`tests/test_quality_battery.py` (PR #140) executes
`benchmarks/quality/plants.py` and `battery.py` at module level;
both import numpy at module level. The wheel-test environment
installs only the wheel and pytest, and the wheel does not depend
on numpy. Reproduced in a fresh venv with only the built wheel and
pytest installed: collection fails with
`ModuleNotFoundError: No module named 'numpy'` at
`plants.py:17`, exit code 2, before any test runs.

Fix: the test module now calls `pytest.importorskip("numpy", ...)`
at module level, with the reason stated in its docstring — the
battery's truth generation needs numpy, which is not a library
dependency; environments that install scipy run the module in
full. This is the pattern the suite already uses for `jsonschema`,
`scipy.stats` and `altair`. Numpy was not added as a package
dependency and the plants were not vendored.

## Cause 3: a scale test pinned pre-#138 dependency semantics

`tests/test_scale.py::test_unique_and_dirty_keys_above_one_million_values[standard]`
expected an affine dependency finding between `id` and `dirty_id`.
PR #138 deliberately excludes index- and identifier-roled columns
as dependency targets (its body records this test as excluded from
its validation, and a corpus expectation moved for the same
reason). On current main the frame roles the two columns `index`
and `identifier`, and the passes state the consequence in coverage:
`functional_dependencies` reports "no eligible dependency
determinants; frame-role target exclusions: id (index), dirty_id
(identifier)", and `arithmetic_identities` screened 0 candidate
pairs for the same reason. Reproduced identically against the dev
install and the wheel install, so the failure is the semantics
change, not packaging.

Fix: the test now pins the current semantics — the uniqueness half
is untouched, and the standard-budget block asserts that no affine
dependency is reported between the two key columns and that the
coverage detail states the frame-role exclusions naming both
columns. If either column becomes an eligible target again, the
test fails. Library behavior is unchanged.

## Acceptance: the failing step, simulated both ways

The wheel was built from the fixed tree and the full `tests/` suite
was run against the installed wheel in two fresh venvs (local
CPython 3.12; the CI leg runs 3.13 — all three causes are
interpreter-independent: a module import, claim strings, and role
semantics):

- Wheel + pytest only (no numpy): **234 passed, 4 skipped**,
  exit 0. The skips are the battery module (reason above) and the
  pre-existing `jsonschema`, `scipy.stats` and `altair`
  import-skips.
- Wheel + pytest + scipy: **238 passed, 2 skipped**, exit 0, and
  `tests/test_quality_battery.py` run standalone: **3 passed** —
  the battery executes where numpy is present.

Before the fix, the reproduced failures were: the battery module
failed collection in the wheel-only environment (standalone run,
exit 2); the gate failed in the full environment (the two PR #139 /
#140 test files together: 1 failed, 9 passed); and the scale
failure reproduced identically against both the dev install and
the wheel install, standalone and in the full suite.
