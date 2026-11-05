# In-pass deadlines: budgets inside expensive passes (issue #117, stage 2)

The wide searches enumerate their spaces by streaming, so
memory stays bounded; the runtime budget had a matching hole:
the profile deadline was consulted only between passes, so a
single expensive pass could overrun the caller's requested
duration by its whole runtime. The corpus table in the 2026-11-04 code
review shows what that means in practice — discontinuities
alone took 42.3 s across the corpus, and on a wide synthetic
frame the dependence screen screens millions of pairs in one
call. A caller who set `deadline_seconds=5` could still wait a
minute.

Stage 2 makes the deadline effective inside the three passes
whose internal loops can run long, and discloses partial
coverage when a stop happens. This note records the design,
the work quanta, and the measured effect.

## What changed

A shared cooperative `Deadline` (an absolute instant:
profile start + the configured duration — the same instant the
between-pass checks measure against) is threaded to the three
passes. Each polls it at a fixed work quantum and stops at the
next quantum boundary:

| Pass | Quantum | Where the check sits |
| --- | --- | --- |
| Numeric dependence, stage-1 screening | one chunk of 2^18 pair ordinals | top of each chunk loop in both streaming passes |
| Numeric dependence, detailed statistics | 64 survivor pairs (capped path); one 2^18 chunk (uncapped path) | before each detailed block |
| Hotspots | one combination | before each combination in the evaluation enumeration |
| Discontinuities | one scan unit (axis unit or diagonal pair), and one target | before each unit inside a target screen; before each target |

Routes not listed are bounded by construction and carry no
checkpoint: the dependence in-place stage-1 route scores at
most `4 × pair_cap` pairs, stage 2 rescores at most that many
survivors, and the hotspots totals enumeration is arithmetic
over bin counts that defines the planned universe the
disclosure is measured against.

Stopping leaves deterministic *state* (counts and completed
prefixes), and two truncation rules keep a stop from silently
changing the statistics:

- **Dependence stage 1** returns no pairs at all when the
  screen is cut: survivor selection is a global ranking, and a
  partial screen cannot reproduce it. The coverage entry says
  how many pairs were scored in which pass. Detailed
  statistics, by contrast, are per-pair, so a stop there keeps
  the completed prefix and its findings.
- **Hotspots** keeps the Bonferroni bar computed over the full
  planned cell universe. The stop changes which cells were
  examined, never the bar the examined cells had to clear.
  Findings rendered on a stopped run say the scan stopped and
  carry a `cells_evaluated` evidence metric beside the planned
  `cells_examined`.
- **Discontinuities** withholds a stopped target's findings
  (its typing, reporting bars, and parsimony suppression all
  assume the complete flagged set) but keeps its examined unit
  counts.

## Disclosure shape

No schema change: a stopped pass reuses the existing `capped`
coverage status, and the detail string — the report's standing
place for counts — states the examined share in the pass's own
units. Verbatim, from the overrun fixtures below:

- Dependence: `0 of 719400 numeric pairs (deadline); stage-1 screening stopped at the deadline in its first pass after 524288 of 719400 pairs; survivor selection needs the complete screen, so no pairs were selected and no detailed statistics were computed; 0 strong pairwise, 0 redundancy links`
- Hotspots: `600 eligible columns, 16181 of 179700 combinations examined (0 of 0 triples), 1310661 of 14555700 cells examined before the deadline stopped the scan, z bar 6.17 (Bonferroni over the full 14555700-cell universe, family alpha 0.01 — the stop does not reduce the correction); counts over the encoded sample of 2000 rows; combinations past the stop were not examined, so the absence of a hotspot there is unmeasured, not a negative`
- Discontinuities: `…; deadline reached mid-pass: 0 of 160 targets fully screened, 4 stopped mid-screen (findings withheld), 156 not started`

A stopped pass also latches the profile's deadline state, so
the report gains the same `deadline` coverage summary entry a
between-pass skip produces, and later passes are skipped as
before.

## Determinism

The deadline remains what the configuration documentation
already called it: an opt-in source of nondeterminism. *Where*
a pass stops depends on wall time. *What* was examined does
not become arbitrary: stops happen only at quantum boundaries
in each pass's deterministic enumeration order, so the
examined set is always a prefix of that order, and the
coverage entry states its size. With no deadline set, the
checks are one comparison against an unset value and reports
are unchanged — the corpus identity run below is the
acceptance evidence. The Rust unit tests pin the prefix
property directly: a stop predicate that fires after exactly K
consultations leaves exactly the first K quanta examined
(K combinations in hotspots, K chunks scored in the dependence
screen, K axis units in a discontinuities target).

Deterministic work-unit budgets were considered and not
adopted: the existing budget configuration is wall-clock
(`deadline_seconds`), no pass exposes a work-unit budget
today, and a second budget currency would need its own
calibration story per pass. The prefix guarantee plus exact
examined counts give the caller the same auditability.

## Overrun fixtures

Synthetic frames (seed 20261104; integers uniform on 0–7 as
float64; floats standard normal with a planted step in column
0 along column 1),
profiled with a single detector enabled and `timing=True`.
Wall times are whole-profile seconds on this 4-core VM under
agent contention, so read the unbounded column as "far beyond
the budget", not as a benchmark.

| Fixture | Unbounded wall | Deadline | Stopped wall | Disclosure |
| --- | --- | --- | --- | --- |
| Dependence, 1,200 cols × 100 rows (719,400 pairs) | ~7 s (900-col sibling: 3.3 s) | 2.0 s | 2.10 s | stage-1 stop after 524,288 of 719,400 pairs (2 chunks) |
| Hotspots, 600 cols × 2,000 rows (179,700 combinations) | 22.0 s | 2.0 s | 2.07 s | 16,181 of 179,700 combinations; 1,310,661 of 14,555,700 cells |
| Discontinuities, 160 cols × 20,000 rows (160 targets) | 225.1 s | 3.0 s | 3.09 s | 0 of 160 targets fully screened; 4 stopped mid-screen; 156 not started |

The stopped wall lands within one quantum's work of the
deadline in every case (the residual is pre-pass pipeline time
plus the in-flight quantum: at most one chunk of pair scoring,
one combination scan, or one scan unit per in-flight target).

## Corpus identity

All 23 runnable corpus datasets were profiled with default
(no-deadline) configuration on the pre-change and post-change
builds, loading frames exactly as
`benchmarks/corpus/run_benchmarks.py` does, and the full
report JSONs compared recursively with only `elapsed_ms`
dropped. Result: 23 of 23 identical. No corpus dataset binds
the new machinery at normal budgets, because none sets a
deadline — the identity run is the regression evidence that
the checkpoints are free when unused.

## Checkpoint overhead

With no deadline set, each pass re-ran on the wide-table
fixture frames and its coverage `elapsed_ms` compared against
the pre-change build measured the same way in the same
session:

| Pass fixture | Before | After | Delta |
| --- | --- | --- | --- |
| Dependence, 900 cols × 

...[truncated 1866 chars]
## Verification status

Passed on this tree: the full Rust test suite (249 tests, 0
failed), including this stage's deadline tests — stop before
the first quantum, the exact examined prefix under a counting
stop predicate, disclosure counts, and no-deadline versus
far-future-deadline equality for each of the three passes. One
existing determinism test now reads the fields of
`DiscOutcome`, the return type this change introduces; its
assertions are unchanged. `cargo check --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo fmt --all -- --check` are clean; the release metadata
check passes and its tests pass (5).

Still running on this tree: the corpus re-dump under the
identity protocol above (the 23-of-23 result was measured on
the change set before it was finalized into this tree; the
expectation from the construction is 23 of 23 again), the
Python test module `tests/test_deadline.py` (4 tests, passing
on the identical change set), the full Python suite, and the
strict book build. Each needs a release build of this tree,
which has not completed yet.
