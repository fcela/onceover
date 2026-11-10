# Panel concentration: concentration over time and outlier entities (2026-11-10)

## Question

Issue #205 reports two panel shapes the shipped concentration
findings cannot state, both from one real analysis of a
person × week panel. The concentration findings are whole-column
only: they pool the periods, so a measure quietly concentrating
into a few accounts over time is invisible; and the outlier checks
work on cells, so an entity whose every row sits at a hundred
times the typical level produces no extreme cell to flag. This run
records the before/after behaviour of the two panel readings —
`concentration_over_time` and `outlier_entity`, computed on the
frame's entity × time grain by the series detector's panel scan —
on synthetic cases built to the reported shapes, and the corpus
contract under the change.

## Protocol

- Builds: the unmodified 0.1.4 tree at the issue branch's base
  commit and the issue-#205 branch build, each installed with
  `maturin develop --release` into its own virtual environment;
  nothing else differed.
- Entry point: `onceover.profile(df)` at the default budget.
- Case 1 (rising): 60 entities × 12 weekly periods. Ordinary
  entities hold 100 per period; one entity ramps from 260 to
  4,900, so its share of each period's total rises from about 4%
  to about 45%.
- Case 2 (outlier): 60 entities × 12 weekly periods. Ordinary
  entities wiggle between 90 and 118 per period; one entity runs
  at about 10,400 per period, about 100 times the typical
  entity's total.
- Case 3 (balanced control): 60 entities × 12 weekly periods,
  near-equal values stable over time (100 plus a deterministic
  wiggle of at most 22).
- Case 4 (flat control): a frame with no time axis.
- Case 5 (sparse periods): the corpus's chinook invoices table —
  412 rows whose `invoice_date` axis is a near-unique timestamp,
  so most periods hold a single invoice. It is a corpus table,
  not a synthetic case; it is listed here because it decided a
  rule (see Results).

## Results

Before: on the unmodified build, cases 1 and 2 produce no
statement of either kind and no `panel_concentration` coverage
entry exists; the only concentration readings are the whole-column
ones, which pool the periods.

The operational rules of the two readings:

- Concentration over time: per period, the share of the period's
  total of a measure held by the top group of entities — the
  largest ceil(1% of the entities), at least one. Only periods in
  which at least half of the entities are active count. The
  statement fires when the median counted period's share is at
  least 30% (level), or when the mean share over the last third
  of periods exceeds the first third by at least 10 share points
  and reaches at least 20% (rising). At least 4 counted periods
  and at least 20 entities are required.
- Outlier entities: the measure summed per entity over all
  periods. An entity is reported when its total is at least 10
  times the typical entity's total (the median, or the upper
  quartile when the median is zero) and above the upper quartile
  plus three interquartile ranges. At least 20 entities are
  required. Shares need a non-negative measure with a positive
  total; a two-valued (0/1) column is a flag, not a volume, and
  is not examined.

After:

- Case 1 fires `concentration_over_time` in its rising form: the
  top entity's share is 32.1% in the median period, up from 12.7%
  over the first third of periods to 41.8% over the last third;
  the largest entity of the last period holds 45.4%. It also
  fires `outlier_entity` for the same entity (total 30,960,
  25.8 times the typical total of 1,200; 30.4% of all of the
  measure).
- Case 2 fires `outlier_entity` naming the entity (total 124,972,
  100 times the typical total of 1,248; 62.9% of all of the
  measure), and `concentration_over_time` in its level form
  (median period top share 62.9%).
- Case 3 fires neither statement; the coverage entry records the
  scan ran with 0 statements.
- Case 4 records the coverage entry as skipped: no entity × time
  grain.
- Case 5 decided the shared-period rule. The first branch build
  stated a 100% top share on chinook for three entity columns —
  every period held one invoice, so the top share was 100% by
  construction and said nothing about concentration. With the
  rule that a period counts only when at least half of the
  entities are active in it, chinook states neither finding and
  its finding count is unchanged from the baseline run (10).
  The evidence of a concentration statement records how many
  periods were set aside as sparse.

Corpus contract: identical before and after — 23 datasets ok,
11 skipped; machine-verified expectations 93/93 in both runs;
the reviewed documented cases read found 18, missed 10, partial 4
in both runs; the runner's exit code is 2 in both runs (documented
misses on record). No expectation was changed.

## Cost

The scan accumulates, per entity column (at most 3) and measure
(at most 12 per grain), one sum per (entity, period) pair, so its
work and memory are bounded by the panel's rows times the measures
examined, and it reports at most 10 statements. It rides on the
series detector's enablement and runs inside the existing series
section of the pipeline; profiles of the synthetic panels above
complete in the same time as the rest of the default battery on
frames of that size, and the corpus run's per-dataset times are
unchanged within run-to-run variation.
