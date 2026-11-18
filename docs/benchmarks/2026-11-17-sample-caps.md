# The sample caps, measured for statement loss (issue #363)

The shared encoded sample (`EncodeConfig::row_cap`: 10,000 / 20,000 /
50,000 rows at quick / standard / deep), the predictability screen's
fitting sample (`ScreenConfig::max_model_rows`: 5,000 at quick and
standard, 10,000 at deep), and the missingness pass's search sample
(`SEARCH_ROWS`: 20,000 at every tier) were all set as cost bounds.
The runtime records price them in seconds. No record priced the
other side: what the statements lose. This record measures the loss
on planted tables where the caps bind, and sets the defaults from
the two curves together.

Harness: `crates/onceover-mines/tests/sample_caps.rs` (ignored sweep
tests; run with `cargo test --release --test sample_caps --
--ignored --nocapture`). All probes are deterministic (hash-spread
groups, seeded noise). Runtimes are wall seconds on a shared,
contended build machine and are indicative, not precise; the
recovery readouts are exact.

## Where each cap actually binds

Reading the passes before planting anything established the
operative bounds, two of which are not the ones the configuration
names:

- **Screen.** Per target, the fit runs on
  min(encoded rows, `max_model_rows`) rows, split 75/25 into
  training and holdout. Both caps are live.
- **Slice shifts.** The pass draws its own stride sample at
  `SliceConfig::row_cap`, which the pipeline sets from the tier's
  encoding cap. Live.
- **Model failures.** `ModelFailureConfig::row_cap` is written by
  the pipeline and read nowhere. The search runs on the cross-fit
  residual set, whose size is min(encoded rows,
  4 × `max_model_rows`) — 20,000 rows at the standard tier by
  either bound, 40,000 at deep, where the screen-derived bound is
  the tighter one.
- **Missingness.** The pattern search runs on a stride sample of
  `SEARCH_ROWS` (20,000, all tiers). Reported patterns are
  recounted over all rows, so the sample gates discovery only.
  The pass's predictability route for missingness indicators
  encodes with `EncodeConfig::default()` and
  `ScreenConfig::default()` directly, not the tier's values.

## The floors that are not caps

Two support floors scale with the sample and therefore bound the
smallest statement a pass can make at *any* cap:

- Slice shifts and model failures both require a slice to hold
  max(50, 1% of the sample) rows (`min_support`,
  `min_support_frac = 0.01`). A slice under 1% of the rows cannot
  be stated however large the sample grows; at exactly 1% the
  realized count in a stride sample straddles the floor, and
  recovery is a coin flip. The floors sweep in the harness prints
  the counts: at 1% share the planted group holds 188 of a
  20,000-row sample drawn from 200,000 rows (floor 200), 193 of
  20,000 drawn from 1,000,000 rows (floor 200), 535 of 50,000
  (floor 500), 1,930 of 200,000 (floor 2,000), and 9,940 of the
  full 1,000,000 rows (floor 10,000) — under the floor in four of
  the five, over it only at the 50,000-row sample, and the
  recovery table below follows those counts exactly.
- The missingness pattern search requires a compared group to
  hold max(30, 0.5% of the sample) rows. Below a 0.5% share no
  group contrast can be stated at any sample size.

These floors are design bars (the slice protocol's, recorded as
reasoned in the constants census), not cost bounds. The cap
question is what happens *above* them.

## Model failures: no cap loss above the floor

Probe: `y = 2·x1 + x2 + noise` everywhere, with the noise standard
deviation ten times larger inside a planted segment (an accuracy
failure no fit can absorb). Segment shares 5%, 2%, 1%, 0.5%, 0.1%
of 200,000 and 1,000,000 rows; six cap arms from the standard tier
(20,000 encoded / 5,000 model rows) to uncapped, plus two
attribution arms isolating each bound.

- At 5% and 2%, the failure slice is stated under every arm,
  including the standard tier's. No loss.
- At 1%, it is stated under most arms and missed under one
  (uncapped encoding, 5,000 model rows) — the knife-edge cell:
  the slice's support in that arm's modelled rows landed under
  the 1% floor. The same arm at 200,000 encoded rows states it.
  This is floor jitter, not a cap effect: the modelled-row count
  is 20,000 in both.
- At 0.5% and 0.1%, no arm states it, including uncapped. The
  floor bars it, not the caps.

Runtime: 0.4–0.7 s per run at the standard arm on 200,000 rows,
4.5–4.7 s uncapped; the cross-fit dominates and grows with the
residual set, not with the table.

## Missingness: no loss from the search sample

Probe: a column missing at 60% inside a planted group against 2%
outside it (strong contrast), and at 10% against 5% (weak
contrast), group shares 5%, 1%, 0.5%, 0.1% of 1,000,000 rows;
search samples 20,000 / 50,000 / 200,000 / all rows.

- Strong contrast: the pattern is stated at every sample size for
  shares 5%, 1%, and 0.5%. At 0.1% it is stated at no sample size
  — the 0.5% group-share floor bars it.
- Weak contrast: stated at every sample size at 5% and 1%.

The 20,000-row search sample costs nothing measurable on these
probes. The sample's other consumer, the numeric-condition
deciles, is bounded by its own split floor (5% of the sample)
before the sample size binds. `SEARCH_ROWS` stays at 20,000; the
pass now takes the size as a parameter
(`profile_missingness_with`), so the sweep is reproducible and a
future tier wiring has a seam.

## Slice shifts: the work budget never binds; the floor does

Probe: a segment carrying a +2.0 mean shift in a numeric target
and a tenfold rate change in a binary target (depth-1 plant), and
the same shift confined to the segment's upper half on a second
column (depth-2 plant). Samples 20,000 / 50,000 / 200,000 / all
rows. Findings on the plant:

| share | table | 20,000 | 50,000 | 200,000 | all rows |
|---|---|---|---|---|---|
| 5% | 200,000 | 14 / 15 | 14 / 15 | 14 / 15 | — |
| 5% | 1,000,000 | 14 / 15 | 14 / 15 | 14 / 15 | 14 / 15 |
| 2% | 200,000 | 10 / 8 | 11 / 10 | 13 / 10 | — |
| 2% | 1,000,000 | 8 / 8 | 12 / 10 | 12 / 10 | 11 / 10 |
| 1% | 200,000 | 0 / 0 | 0 / 0 | 0 / 0 | — |
| 1% | 1,000,000 | 0 / 0 | 6 / 4 | 0 / 0 | 0 / 0 |
| 0.5%, 0.1% | both | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |

(each cell is depth-1 / depth-2 findings on the plant)

- Every cell's pass status is `ran` with all slice definitions
  examined. The 300,000,000 row-visit work budget never binds on
  these probes, at any sample size; the silences below are not
  truncation.
- At 5%, no loss at any sample size.
- At 2%, the headline slice is stated at every size, but the
  20,000-row sample states fewer of the plant's restatements
  (8–10 findings against 11–13 at 50,000 rows and up): the
  split-sample confirmation has less to work with (at 1,000,000
  rows, depth 1: 12 candidates cleared the search bars at the
  20,000-row sample and 9 confirmed, against 13 cleared and 13
  confirmed at 50,000 rows).
- At 1%, recovery follows the floors table exactly: stated only
  in the one cell whose realized group count cleared the floor.
- Runtime grows with the sample but stays small: per-cell
  times across the 80-cell sweep (both table sizes, both
  depths, all sample sizes) run 0.02–2.4 s.

## Predictability screen: the model-row cap is the binding one

Probe: the target depends on a predictor only inside a planted
group (`y = 3·x` plus noise inside the group, pure noise
outside), so the group's share bounds the attainable pooled
holdout R². Two grids isolate the caps: grid A varies the
fitting-row cap on the full encoding; grid B varies the encoded
sample with fitting rows uncapped. Readout is the target's
holdout R² (the planted structure's ceiling is ≈0.13 at 5%
share, ≈0.03 at 1%, ≈0.015 at 0.5%).

- At 5% share, every cell reaches 0.118–0.132 with the planted
  driver named. No loss at any cap, on either grid.
- At 1%, grid A rises with fitting rows: 0.016 at 5,000, 0.014 at
  20,000, 0.019 at 50,000, 0.032 at 200,000 and uncapped (200,000
  rows); 0.017 / 0.016 / 0.020 / 0.029 / 0.030 on 1,000,000 rows.
  Grid B at the same share reaches 0.020–0.025 already at a
  20,000-row encoding (fitting rows are the minimum of the two
  caps, and the two grids' 20,000-row cells differ only in which
  20,000 rows they draw — the gap between them is subset noise,
  ±0.008 at this share). The robust reading: 5,000 fitting rows
  reach about half the attainable metric; the knee is at about
  50,000 fitting rows; the last increment to all rows adds a
  little at 1,000,000 rows.
- At 0.5%, nearly every cell is at or near zero, including
  50,000 fitting rows; only full-table fits on the 1,000,000-row
  table reach the ≈0.015 ceiling. The driver signal the screen
  hands to the passes downstream is effectively absent below
  ~50,000 fitting rows at this share.
- At 0.1%, no cell at any cap recovers anything (metrics
  ≤ 0.005, driver lists name noise columns). Uncapped included:
  the pooled signal is below this screen's reach, not below its
  caps.
- The pooled metric for a subgroup-only driver is bounded by
  the subgroup's share, so the screen's finding bars (R² ≥ 0.3)
  are out of reach for small subgroups at any cap. What the cap
  moves is the metric's approach to its ceiling — and, through
  the driver lists, what the decision-table, prism, and
  model-failure passes get to work with.
- Runtime (grid A, five targets, 1,000,000 rows): ≈1 s at 5,000
  fitting rows, ≈1.5 s at 20,000, ≈2.3 s at 50,000, ≈7–10 s at
  200,000, ≈21–26 s uncapped. Roughly linear in fitting rows
  past 50,000.

## Decisions

- **`ScreenConfig::max_model_rows`: 5,000 → 50,000 as the
  library default and the standard tier's value; deep 10,000 →
  200,000; quick's tier config keeps 5,000 explicitly (the
  screen is skipped at the quick tier today, so that value is
  inert until a quick screen exists).** This is the one cap
  with a first-order measured loss: half the attainable subgroup
  signal at 1% share, nearly all of it at 0.5%, at the old
  standard value. The knee of the loss curve is at ~50,000
  fitting rows and the runtime there is ≈2.3 s for five targets
  on a 1,000,000-row probe — the price of the knee is small.
  Deep takes the value where the 1% metric reaches its ceiling.
  Side effect, intended: the model-failure residual set bound
  (4 × model rows) rises with it, and the missingness indicator
  route, which builds its screen from `ScreenConfig::default()`,
  now fits on up to the full 20,000 rows of its own encoding
  instead of 5,000.
- **The encoded-sample ladder stays at 10,000 / 20,000 /
  50,000.** Above the floors, its measured loss is second-order:
  the slice confirmation depth at 2% share (above), which the
  deep tier's 50,000-row sample already answers, and nothing
  anywhere else — the screen's binding constraint was its own
  fitting rows, model failures show no loss, and the sample
  feeds every downstream pass, so its runtime price is paid by
  all of them. The curves do not support raising the default
  for all passes to recover restatements in one.
- **Coverage now states the carryable share.** Where the slice
  and model-failure passes run on a sample, their coverage
  entries state the smallest slice share that sample can carry
  (the support floor as a percentage of the sample, with the
  floor count), so a negative from those passes reads with its
  resolution attached.
- **`ModelFailureConfig::row_cap` is removed.** The field was
  written by the pipeline and read nowhere; the operative bound
  is min(encoded rows, 4 × `max_model_rows`), and a configuration
  field that does nothing misstates the pass's contract.
- **`SEARCH_ROWS` stays at 20,000**, now a parameter of
  `profile_missingness_with` so the measurement is reproducible
  and a future tier wiring has a seam. The measured loss at
  20,000 is zero on both contrasts; the binding limits are the
  pass's own floors.

## Gate results on the merged base

Verified against main at de041e20 (the base this change was
rebased onto before publication; the probes above ran on the
same code, one merge earlier):

- Digest gate compare: PASS — 0 worse, 0 above ceiling, 5 worse
  and accepted, 22 improved, 51 unchanged totals. Must-tier
  reference counts are unchanged (16 of 19 at the 4k digest);
  the should-tier 4k count improves 29 → 30.
- Accepted, with entries in `benchmarks/gate/accepted.json`:
  `cost.profile_seconds` total 41.5s → 56.5s (the price of the
  raised fitting-row cap, concentrated in the datasets over
  5,000 rows); `invariance.noise` +1 on adult-census-income
  (6 → 7) and bank-marketing (1 → 2), with the noise total
  unchanged at 29; `reference.should_8k_default` total 36 → 35,
  one should-tier statement on bank-marketing leaving the 8k
  digest as the screen's driver metrics shift the selection.
- Corpus: 219 of 220 machine-verified expectations pass. The
  one failure (daily_level_collapse's known-miss tripwire for
  `level_shift`) fails identically on the unmodified base: the
  capability arrived with the breaks recalibration merged ahead
  of this change, and promoting that expectation belongs to
  that change, not this one.
- The gate's own test module isolates its synthetic scorecards
  from the live accepted file (an empty accepted file is passed
  unless a test supplies its own): the cost acceptance above is
  the first entry that would otherwise leak into the tolerance
  test's hand-made cards.

## Method notes

- One seed per cell. The passes are deterministic given their
  rows; the variation across cells is the samples themselves,
  which is the quantity under study. Knife-edge cells are named
  as such rather than averaged away.
- Matched nulls (pure-noise target, unshifted slice,
  missing-completely-at-random) at the largest caps state
  nothing about the planted columns: the screen returns no fit
  for the noise target, the slice search states 0 findings, and
  the missingness search states 0 patterns.
