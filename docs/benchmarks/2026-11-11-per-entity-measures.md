# Per-entity series: more measures, more entity columns (2026-11-11)

## Question

Issue #239 reports that the per-entity series pass examines only
the principal measure and only the grain's first entity column.
A panel whose second measure behaves differently by entity, or
whose pattern sits on a second entity dimension, receives no
per-entity examination of either. This run records the behaviour
of the extended pass on synthetic panels built to those shapes,
and the corpus contract under the change.

## Protocol

- Build: the issue-#239 branch tree, installed with
  `maturin develop --release` into a dedicated virtual
  environment (Polars 2.1.0); the baseline behaviour quoted is
  the pass as recorded in `2026-11-10-series-passes.md`.
- Entry point: `onceover.profile(df)` at the default budget.
- The four cases below are the regression tests shipped with
  the change (`tests/test_series.py`, the per-entity extension
  section); all dates are Mondays from 2023-01-02.
- Case A (second measure carries): 40 persons × 80 weeks. The
  principal measure `tokens` is flat; the second measure
  `clicks` rises at 0.3 per week for 8 of the 40 persons.
- Case B (carrier bar): the same panel with only 2 rising
  persons.
- Case C (second entity column): 120 persons × 80 weeks, each
  person in one of 12 stores. Every person rises at 0.05 per
  week at noise sd 3 — individually invisible.
- Case D (measure bound): 10 fully observed measures on a
  40-person panel; the per-entity pattern is planted in the
  tenth measure by observation-count ranking.
- Corpus: `python benchmarks/corpus/run_benchmarks.py` on the
  branch build, plus a per-dataset diff of finding counts,
  category counts, and expectation outcomes against the
  previous recorded run.

## Results

- Case A: the baseline pass examines `tokens` only and states
  nothing per entity for `clicks`. The extended pass states an
  `entity_trend` finding for `clicks` (8 of the 40 person
  entities tested), and the aggregate `clicks` trend statement
  is qualified against `clicks`' own per-entity account — a
  minority of entities: 8 of 40 — where the baseline qualified
  an aggregate trend against the principal measure's account
  only. Nothing is stated for `tokens`.
- Case B: no entity finding for `clicks`. The carrier bars are
  unchanged: a statement needs at least 10 entities tested and
  at least 3 carriers.
- Case C: no entity finding on the person axis (120 tested, no
  individual series carries the rise). On the store axis the
  same scan, grouped by store, states an `entity_trend` for 12
  of the 12 store entities tested: aggregating each store's
  persons makes the shared rise visible per store. The entity
  axis needed no separate machinery — the scan is parameterized
  by the grouping column — so both axes ship together.
- Case D: nothing is stated for the tenth measure; it is past
  the bound. Coverage records the bound where it binds:
  "per-entity series by person examined the first 8 of 10
  measures by entity × tick observations", alongside one
  coverage fragment per examined (entity column, measure)
  pair in the existing format.
- The principal measure's statements are unchanged: the
  per-entity cases of `2026-11-10-series-passes.md` (minority
  trend, no carrier, shared break) are covered by the
  pre-existing tests in `tests/test_series.py`, and the whole
  file passes on this build (29 tests: 25 pre-existing, 4 new).

### Corpus

The branch build's run reads: 23 datasets ok, 11 skipped;
machine-verified expectations 93/93; documented cases found
18, missed 10, partial 4; exit code 2 — the standing contract
for this tree, as recorded in `2026-11-10-series-passes.md`.
The per-dataset diff shows no dataset gaining or losing an
`entity_trend` or `entity_break` statement under the
extension: the corpus panels are single-measure or
single-entity-axis, so the wider scan finds nothing new to
state on them. No expectation was updated.

## Cost

Each examined (entity column, measure) pair costs one pass of
the existing per-entity scan over the panel's rows and can add
at most the existing per-pair statements (a trend, a break;
five entities named at most). The bounds are: at most 8
measures per entity column, and the entity columns the frame
model supplies (at most 3). The worst case is therefore 24
scans of the panel where the baseline ran 1, on panels small
enough to carry at least 10 entities per axis; coverage
states every pair examined, so the cost is visible in the
report.

## Rules the change ships

- Per-entity examination runs on each entity column the frame
  model supplies, in its priority order (the grain's entity
  column first, then identifier- and dimension-role
  candidates), and on the measures ranked by entity × tick
  observations on that grain — the principal measure (the most
  observations) first, then the rest by count, ties keeping
  column order — at most 8 measures per entity column.
- The carrier bars are unchanged and apply per (entity
  column, measure) pair: at least 10 entities tested, at
  least 3 carriers for a statement; each measure's statements
  carry its own tested and carrier counts.
- Each measure's aggregate trend statement is qualified
  against that measure's own per-entity account on the first
  entity column.
- Coverage names every examined pair and states the measure
  bound when it binds.
