# Panel dynamics: entry and exit blocks, first activity per entity (2026-11-10)

## Question

Issue #204 reports that the shipped panel passes describe nothing
about how entities arrive, leave, and start: the series pass's
new-level and ended-level machinery works on values in a column
and excludes the entity axis. The reported case is a person × week
panel where 180 people first appear in a single week and the
profiler said nothing; first-activity week per person had to be
computed by hand. This run records the before/after behaviour of
the panel dynamics scan on synthetic cases built to the reported
shapes, and the corpus contract under the change.

## Protocol

- Builds: the unmodified tree at the issue branch's base commit
  (baseline) and the issue-#204 branch build, each installed with
  `maturin develop --release` into its own virtual environment;
  nothing else differed.
- Entry point: `onceover.profile(df)` at the default budget.
- Case 1 (blocks and lag): a 120 × 26 person × week panel (2,860
  rows). 60 people are present from the first week to the last;
  30 first appear in week 8 and stay; 30 are present from the
  first week and last appear in week 15. A token measure is zero
  until three periods after each person's first appearance and
  takes varied positive values after that.
- Case 2 (uniform churn): 48 entities over 12 periods, exactly
  two entities entering per period and entities leaving on a
  matching steady schedule, with a measure active from entry.
- Case 3 (flat frame): a frame with no time axis and no entity
  column.
- Corpus: `python benchmarks/corpus/run_benchmarks.py` on both
  builds, with the per-dataset JSON records diffed directly.

## Results

Case 1: the baseline reports no entry, exit, or first-activity
statement — its finding types on the panel are the pre-existing
set, and its coverage has no panel-dynamics entry. The branch
build reports one `entry_block` — 30 of 120 person entities first
appear in the period 2025-03-03, 25.0% of all entities, against a
typical entry of 0 per period — one `exit_block` — 30 of 120 are
last seen in the period 2025-04-21, on the same terms — and one
`first_activity`: the measure first becomes active after the
first observed period for all 120 entities, median lag 3 periods
(middle half 3 to 3), none starting on entry and none never
active. The rules: a period is an entry block when its
first-appearing count is at least 5, at least four times the
median first-appearing count over the candidate periods, and at
least 2% of all entities; the first period is not a candidate,
because every founding entity first appears there by
construction. Exit blocks mirror this on last-appearing counts,
with the last period excluded because entities present there are
still under observation. First activity is stated when at least
3 entities start late and late starters are at least 10% of the
entities that ever become active on the principal measure (the
measure with the most entities ever active; activity means
non-zero for a numeric measure, true for a boolean, present
otherwise). Each block finding carries the per-period entry and
exit counts for the whole axis, and the per-entity first, last,
and first-activity periods ride on the first-activity finding —
or, where no first-activity statement fires, on the largest block
finding — as profile evidence.

Case 2: no block finding; steady churn of two per period stays
under the count floor and the multiple of the typical level. The
coverage record states that 0 entry blocks and 0 exit blocks
were found. A companion case with a measure whose activity never
occurs records in coverage that no measure column shows any
activity, and a two-period axis records that the scan needs at
least 3 distinct periods.

Case 3: the coverage record for `panel_dynamics` is `skipped` —
no entity × time grain: the frame has no time axis with per-row
ticks — following the convention the other panel passes use.

Entities present in every period appear in no finding; they are
the background the counts are measured against. The pass states
when entities arrive, leave, and start; it does not restate
per-period row counts, which the per-period coverage check owns.

Corpus: 93/93 machine-verified expectations pass on the branch
build, and the documented cases are unchanged from the baseline
run (found 18, missed 10, partial 4; the run's exit code is 2 in
both runs — machine checks clean, documented misses on record).
The per-dataset records diff clean on all 34 datasets: the new
scan fires nowhere on the corpus, which carries no panel of the
reported shape with a block above the stated bars. No registry
expectation was updated.

The capability manifest regenerates byte-identical: the scan
rides on the existing `series` detector, whose declared
categories already include `changes`.

## Cost

The scan shares its inputs with the one-way-flag scan — the same
bounded table read (at most 200,000 rows) and the same entity
coding — and then walks each row once per entity column (at most
three) for entry and exit counts, plus one pass per measure
column (at most eight) for first activity. It runs only on
frames with a time axis, at the tiers where the series detector
already runs.
