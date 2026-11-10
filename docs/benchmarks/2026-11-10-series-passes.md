# Series passes: tested trend, changepoint range, weekly axes, per-entity series (2026-11-10)

## Question

Issue #210 reports four gaps in the time-series passes, all scoped
by an audit of the 0.1.4 tree: the trend statement carries no
significance test, so a drift indistinguishable from noise is
stated the same way as a real one; the breaks pass locates a
changepoint as a single period with no range; a weekly axis is not
recognized on its own terms (year + week part columns are not
assembled, a lone week column is no axis at all, and the annual
cycle on weekly data is only probed past a two-and-a-half-year
span); and on a panel the series checks run on the aggregate only,
so a movement carried by a minority of entities reads as a general
one. This run records the before/after behaviour of the four
additions on synthetic cases built to those shapes, and the corpus
contract under the change.

## Protocol

- Builds: the unmodified 0.1.4 tree (baseline) and the issue-#210
  branch build, each installed with `maturin develop --release`
  into its own virtual environment; nothing else differed.
- Entry point: `onceover.profile(df)` with the named detectors at
  the default budget.
- Case A (trend): weekly series of 156 points. A real drift
  (0.05 per step, noise sd 0.5); six random walks; six AR(1) series
  with coefficient 0.95. The deflated p-values quoted below were
  recomputed from the same fixtures outside the library.
- Case B (range): 400 daily points, a level shift planted at index
  200 (2020-07-19). Strong form: +4.0 at noise sd 1.2, one row per
  day. Dense form: +2.0 at noise sd 1.2, three rows per day. Weak
  form: +1.5 at noise sd 1.2, one row per day.
- Case C (weekly cycle): weekly points carrying a sine of period
  52 and amplitude 3 at noise sd 0.5, at 115 points (about 2.2
  years) and at 170 points (about 3.3 years).
- Case D (parts): a 12-person panel over years 2023–2024 × weeks
  1–26 with `year` and `week` integer columns and no date column.
- Case E (lone week): the same panel shape with the `week` column
  only.
- Case F (minority trend): 40 persons × 80 weeks; 8 persons rise
  at 0.3 per week, the rest are flat; noise sd 2. A short form
  (26 weeks, rise 2.0 per week) checks the per-entity pass below
  the aggregate's 60-point minimum.
- Case G (no carrier): 40 persons × 80 weeks; every person rises
  at 0.05 per week at noise sd 3 — individually invisible,
  collectively strong.
- Case H (shared break): 30 persons × 40 weeks; 6 persons shift
  up by 8 at week 20; noise sd 1.5.
- Corpus: `python benchmarks/corpus/run_benchmarks.py` on both
  builds, plus a per-dataset diff of finding counts, category
  counts, and expectation outcomes between the two runs' JSON
  records.

## Results

### Trend (case A)

- Baseline: the real drift is stated as a trend. So are two of the
  six random walks (the seeds whose line fits explain 61% and 65%
  of variance), stated the same way as the drift. No AR(1) series
  is stated (the line-fit gates already exclude them).
- Branch: the drift is stated, with `slope_p_value` 6.6e-107 in
  the evidence. No random walk and no AR(1) series is stated. The
  deflated p-values of the six walks are 0.022, 0.090, 0.0019,
  0.0012, 0.122, 0.308; of the AR(1) series, all are at or above
  0.037 apart from draws the line-fit gates exclude first. A real
  drift of 0.15 per step over AR(1) noise at 0.9 scores p 0.0001
  and 0.0005 on two seeds and is stated.
- The bar is p < 0.001, not 0.01: at 0.01 two of the six walks
  still clear it (0.0019 and 0.0012 are the closest calls at
  0.001). The deflated t is approximate under near-unit-root
  residuals, and the pass tests many series per report; the
  stricter bar is the compensation, and it is stated in the
  method text and the book.

### Changepoint range (case B)

- Baseline: the strong form is located exactly (breakpoint
  2020-07-19) with no range of any kind.
- Branch, strong form: range 2020-07-18 to 2020-07-19, best
  estimate 2020-07-19. Dense form: breakpoint and range both
  exactly 2020-07-19. Weak form (one row per day): the best
  estimate on the training partition is 2020-07-21 and the range
  spans 2020-07-21 to 2020-07-23 — on that draw the training
  partition's own location error is about two days, and the range
  is a statement about the scanned series, not a guarantee.
  Unit tests on the full series (no partition) verify containment
  for level, trend, and variance breaks, and that the range
  narrows as noise falls.

### Weekly axes (cases C, D, E)

- Case C, 115 weeks: baseline reports persistence (lag-1
  autocorrelation 0.93) and no cycle. Branch reports the annual
  cycle at lag 52 (autocorrelation 0.50, period "1 year"). At 170
  weeks both builds report the cycle.
- Case D: baseline takes the `year` column as a low-confidence
  year candidate and the series pass is skipped. Branch assembles
  the axis from `year`/`week` as ISO week Mondays (source
  `timestamp_parts`, step "1 week", description stating the axis
  is weekly), and the series pass runs on it.
- Case E: baseline records no axis. Branch records a
  `week_candidate` axis at low confidence with span 1..26, its
  description stating that week numbers repeat every year.
- A native weekly date axis also gains the weekly description
  ("the date column 't'; the axis is weekly") on the branch.

### Per-entity series (cases F, G, H)

- Case F: on the baseline, the short form states nothing (26
  ticks are below the aggregate minimum, and no per-entity pass
  exists). On the branch, an `entity_trend` finding names the
  carriers — "tokens rises along t for 8 of the 40 person
  entities tested: p00, p01, p02, p03, p04 and 3 more". On the
  branch's long form the aggregate trend also fires, and its
  statement is qualified: "the rise is carried by a minority of
  entities: 8 of 40 person series show a significant rise of
  their own", with `entities_with_trend` and `entities_tested`
  in its evidence.
- Case G: branch states the aggregate trend (83% of variance,
  p < 0.001) qualified with "no individual person series shows a
  significant rise of its own" and `entities_with_trend` 0 of 40.
  No `entity_trend` finding is produced.
- Case H: branch states an `entity_break` finding — "tokens
  shifts up along t at 2023-05-22 for 6 of the 30 person entities
  tested" — 2023-05-22 being the planted week-20 Monday. Baseline
  states nothing.
- One correction during verification: the first branch build also
  produced an `entity_break` on case F's short form, grouping
  four of the rising entities at the panel's midpoint — a steady
  ramp reads as a level shift to a two-mean scan. Entities
  carrying a significant trend are therefore not break-scanned;
  their movement is already stated as a trend. Case H's break
  carriers carry no significant trend (a step's line fit loses to
  its step fit), so the planted break is unaffected.

### Corpus

Identical on both builds: 93/93 machine-verified expectations,
documented cases 18 found / 10 missed / 4 partial, exit code 2.
The per-dataset diff shows no movement in finding counts,
grouped counts, category counts, statuses, or expectation
outcomes on any of the 34 datasets. Breaks claims are reworded
wherever they occur (the range is part of the claim); a spot
check on bike-sharing-hourly shows the new form, e.g. a level
shift "along dteday between 2012-01-01 and 2012-01-01 (best
estimate 2012-01-01)". No expectation was updated.

### Rules the change ships

- Trend: line R² at least 0.3, line beats the best single level
  shift by at least 0.02 of explained variance, and the slope's
  t-test — deflated by the residuals' lag-1 autocorrelation to an
  effective sample size of n(1-rho)/(1+rho) — clears p < 0.001.
- Changepoint range: every split whose two-segment squared error
  is within 3.84 residual variances of the best split's (the
  chi-square 95th percentile at one degree of freedom), read off
  the detection scan's own cost curve; the point estimate is
  retained as `breakpoint`.
- Weekly axes: year + ISO week parts assemble as week Mondays
  (a week part beside a month part is not part of the set); a
  lone week-number column is a low-confidence candidate; an axis
  whose modal step is seven days is described as weekly; the
  annual cycle on a weekly axis is tested at lag 52 once the span
  reaches two years.
- Per-entity: the principal measure (most entity × tick
  observations) on the grain's first entity column; a trend needs
  at least 12 points and the aggregate gates; a break needs at
  least 24 points, segments of at least 6, and a single-break
  gain of at least 0.25; at least 10 entities tested and at least
  3 carriers for a statement; at most 5 entities named, the rest
  counted. The aggregate trend is qualified when fewer than half
  the tested entities carry its direction, and states the counts.
