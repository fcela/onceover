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
| Hotspots | one combination | before each pair in pair evaluation, and before each triple in triple evaluation |
| Discontinuities | one scan unit (axis unit or diagonal pair), and one target | before each unit inside a target screen; before each target |

Routes not listed are bounded by construction and carry no
checkpoint: the dependence in-place stage-1 route scores at
most `4 × pair_cap` pairs, stage 2 rescores at most that many
survivors, and the hotspots pair-totals enumeration is
arithmetic over bin counts — it totals the pair universe the
disclosure is measured against and never counts rows.

Stopping leaves deterministic *state* (counts and completed
prefixes), and two truncation rules keep a stop from silently
changing the statistics:

- **Dependence stage 1** returns no pairs at all when the
  screen is cut: survivor selection is a global ranking, and a
  partial screen cannot reproduce it. The coverage entry says
  how many pairs were scored in which pass. Detailed
  statistics, by contrast, are per-pair, so a stop there keeps
  the completed prefix and its findings.
- **Hotspots** computes the Bonferroni bar over the examined
  cells, as the pass does on a complete run (pair cells plus
  the selected triples' cells). The deadline is polled in
  both phases. A stop during pair evaluation skips the
  triples phase entirely: triple selection spends the pair
  pre-scores, so a selection cannot rest on a partial screen.
  A stop during triple evaluation keeps the completed
  evaluations; their hits are filtered by the bar over the
  cells examined at the stop point. Findings rendered on a
  stopped run say the scan stopped, state how many cells went
  unexamined, and carry a `cells_evaluated` evidence metric
  giving the examined-cell denominator the bar came from.
- **Discontinuities** withholds a stopped target's findings
  (its typing, reporting bars, and parsimony suppression all
  assume the complete flagged set) but keeps its examined unit
  counts.

## Disclosure shape

No schema change: a stopped pass reuses the existing `capped`
coverage status, and the detail string — the report's standing
place for counts — states the examined share in the pass's own
units. Verbatim, from the overrun fixtures below:

- Dependence: `0 of 719400 numeric pairs (deadline); stage-1 screening stopped at the deadline in its second pass after 524288 of 719400 pairs; survivor selection needs the complete screen, so no pairs were selected and no detailed statistics were computed; 0 strong pairwise, 0 redundancy links`
- Hotspots: `600 eligible columns, 32506 of 179700 pair combinations examined, 2632986 of 14555700 pair cells examined before the deadline stopped the scan, z bar 5.89 (Bonferroni over the examined cells, family alpha 0.01); triples were not selected or examined — selection needs the complete pair screen; counts over the encoded sample of 2000 rows`
- Discontinuities: `…; deadline reached mid-pass: 4 of 160 targets fully screened, 1 stopped mid-screen (findings withheld), 155 not started`

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
(K pair combinations then K triples in hotspots, K chunks
scored in the dependence screen, K axis units in a
discontinuities target).

Deterministic work-unit budgets were considered and not
adopted: the existing budget configuration is wall-clock
(`deadline_seconds`), no pass exposes a work-unit budget
today, and a second budget currency would need its own
calibration story per pass. The prefix guarantee plus exact
examined counts give the caller the same auditability.

## Overrun fixtures

Synthetic frames (seed 20261104; integers uniform on 0–7 as
float64; floats standard normal with a planted step in column
0 along column 1), profiled with a single detector enabled and
`timing=True`. The unbounded walls below were measured on
this 4-core VM while it was otherwise idle. The stopped walls
and disclosures were measured in the same session while a
release build ran on the same VM: at idle speeds the
dependence fixture finishes inside its 2 s deadline (and the
Python stop tests behave the same way — see the verification
status), so a stop engages only once the machine is loaded,
which is also when a deadline matters. Read the stopped
column as "the deadline plus one quantum of work under load",
not as a benchmark.

| Fixture | Unbounded wall (idle) | Deadline | Stopped wall (under load) | Disclosure |
| --- | --- | --- | --- | --- |
| Dependence, 1,200 cols × 100 rows (719,400 pairs) | 1.0 s | 2.0 s | 2.23 s | stage-1 stop after 524,288 of 719,400 pairs (2 chunks) |
| Hotspots, 600 cols × 2,000 rows (179,700 pair combinations) | 2.5 s | 2.0 s | 2.01 s | 32,506 of 179,700 pair combinations; 2,632,986 of 14,555,700 pair cells; triples not selected or examined |
| Discontinuities, 160 cols × 20,000 rows (160 targets) | 23.4 s | 3.0 s | 3.11 s | 4 of 160 targets fully screened; 1 stopped mid-screen; 155 not started |

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
| Dependence, 900 cols × 100 rows | 503 ms | 480 ms | −5% |
| Hotspots, 600 cols × 2,000 rows | 2,490 ms | 2,443 ms | −2% |
| Discontinuities, 160 cols × 20,000 rows | 23,966 ms | 22,441 ms | −6% |

The before/after pair above was measured back to back in one
session; a second pair of runs in the same session swapped
the ordering on the dependence pass (497 ms before vs 519 ms
after) and stayed within a few percent on the other two
(hotspots 2,473 ms before vs 2,493 ms after; discontinuities
24,320 ms before vs 22,678 ms after). Run-to-run load noise is
larger than any checkpoint cost: a checkpoint is one
comparison against an unset value per work quantum, and the
measurements show no overhead above that noise.

## Verification status

Passed on this tree: the full Rust test suite (276 tests, 0
failed), including this stage's deadline tests — stop before
the first quantum, the exact examined prefix under a counting
stop predicate (with the stopped findings' `cells_evaluated`
evidence pinned at 80 on a pair stop and 368 on a triple
stop), disclosure counts, and no-deadline versus
far-future-deadline equality for each of the three passes. One
existing determinism test now reads the fields of
`DiscOutcome`, the return type this change introduces; its
assertions are unchanged. `cargo clippy --workspace
--all-targets -- -D warnings` and `cargo fmt --all -- --check`
are clean; the release metadata check passes and its tests
pass (5). The corpus identity run above was repeated on this
tree: 23 of 23 identical. The Python suite passes
`tests/test_deadline.py` (4 tests) under ordinary build load,
and the full suite runs 413 passed, 4 skipped on this tree's
release build; two of the deadline module's stop tests are
wall-clock sensitive — on an idle machine their fixtures
finish inside the 2 s deadline (the dependence fixture's
unbounded wall above is 1.0 s), so the full-suite run on the
idle machine records those two as failures while the module
passes 4 of 4 under load, and the deterministic Rust counting
tests pin the stop behavior at any machine speed. The strict
book build passes on this tree (49 pages, executed with
`--strict`); the book changes are notebook wording edits
aligning the deadline description with the behavior specified
here.
