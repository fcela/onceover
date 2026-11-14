# Per-level contrasts: one level against the pooled rest

Date: 2026-11-14. Change: issue #275. Detector:
`level_contrasts` (new detector, in the default set). Finding
type: `level_contrast` (`associations`).

## Question

The screens that compare a categorical column with a numeric
measure test level means and state an association. They are
blind to one level — a source, a site, an instrument — that
is off in scale or shape while its mean is unremarkable, and
the declared group contrast states the same family of facts
only for a subset the caller names. The question was whether
an automatic scan can state the off level directly, with the
measured medians and IQRs in the claim: "Readings from site
'K7' run 12% high and twice as variable as every other site
(median 4.48 vs 4.01 pooled, IQR 0.9 vs 0.45); no other site
differs from the pooled distribution by more than 3%."

## Method

For each low-cardinality categorical column (at most 25
distinct values) and each numeric measure, every level is
compared with the pooled rest of the rows. Calendar parts of
the frame's date axis — day of week, month of year, hour of
day — enter as derived groupings of the same scan.

Frozen bars, set before the evaluation runs:

- A level and its rest each need at least 30 complete rows;
  a pair needs at least 3 distinct values.
- Location: the level's median differs from the rest's
  median by at least 0.5 pooled IQRs, backed by a
  Mann–Whitney rank-sum test.
- Scale: the level's IQR is at least 1.5 times, or at most
  two thirds of, the median IQR of the other levels, backed
  by a rank-sum test on absolute deviations from each row's
  own level median. The comparison is made against the other
  levels' typical spread, not the pooled rest's IQR: the
  pooled rest is a mixture, and a mixture's IQR inflates
  whenever one level is shifted, which would make every
  other level look narrow by comparison (measured below).
- Constant multiple: the level's nine decile quotients
  against the rest concentrate at one quotient at least
  1.25x from 1 (quotient coefficient of variation at most
  0.025, largest decile deviation at most 0.06), the rest's
  deciles sit on one side of zero, and the level's IQR ratio
  moves with the quotient — same direction, at least half
  its size on the log scale. A rescaling scales the spread
  by the same factor as the values; a pure location shift on
  a distribution bounded away from zero also concentrates
  its decile quotients but leaves the spread unchanged, and
  the coherence requirement is what keeps the two apart
  (measured below).
- Location and scale p-values across the scan form one
  enumerable family (levels × measures × two tests),
  corrected together by Benjamini–Hochberg; a form is
  stated only when its effect bar clears and q ≤ 0.05.
- A location contrast that falls under the bar once the
  pair's most-shifted level is removed from its rest is not
  stated — the primary level explains it. A grouping with
  exactly two levels states its contrast once, keeping the
  larger effect: the two levels are each other's rest.
- A calendar level is eligible only when its rows span at
  least two distinct cycles (two weeks for a weekday, two
  years for a month, two days for an hour). The hour part is
  skipped when every axis tick is exactly midnight.
- At most 400 pairs are examined and 20 findings reported,
  ranked by effect size.

Activation: the detector is in the default set and runs at
every budget. Its scans are exact quantile work in the same
cost class as the other exact passes, and it closes a stated
blindness of the mixed-pair screen, which is itself in the
default set; the corpus outcome below is the check that the
addition does not disturb the recorded expectations.

## Planted fixtures

Shipped as `tests/test_level_contrasts.py`, with Rust unit
tests for the same shapes in `crates/onceover-mines`:

- Location: four sites, `K7` shifted +1.2 on a common base.
  Stated, form `location`, with the "no other level"
  sentence: the other sites' largest shift is 0.38 pooled
  IQRs in the worked example.
- Scale with equal means: `WIDE` spreads its values three
  times as wide around the same centre. Stated, form
  `scale`, IQR ratio 3.0 in the worked example; the centres
  agree to within 0.2 in the fixture's assertion.
- Constant multiple: `CM` carries exactly 2.54x the same
  underlying readings. Stated, form `constant_multiple`,
  quotient 2.54 (within 0.05 asserted), and it is the only
  statement for the pair.
- Calendar: 731 daily readings with Saturdays +3.0. The
  day-of-week contrast for Saturday is stated, form
  `location`, and no other weekday is stated.
- Silent controls: four sites on one distribution (no
  statements); the same values under shuffled level labels
  (no statements); a 10-row level shifted to a constant
  50.0, below the 30-row floor (no statements).

Gate, in CI order, on the final tree: release metadata
check passes; `scripts/test_release.py` 6 passed;
`cargo fmt --check` clean; clippy clean with
`-D warnings`; `cargo test --workspace` 423 passed,
0 failed; `pytest tests/` 649 passed, 1 skipped. The
strict book build exits 0, and the rendered capabilities
page carries the new registry row and the corrected counts
(41 registered, 32 in the default set).

## Two failure shapes found by the fixtures

The fixtures caught three defects in the first
implementation; each is fixed in the shipped code and
pinned by a regression test.

1. The pooled rest is a mixture, and a mixture's spread
   inflates. The first implementation compared IQRs against
   the pooled rest and centred the scale test's deviations
   on the rest's median. On the calendar fixture, Saturday's
   shift inflated the rest's IQR for every other weekday to
   0.789 against a typical level's 0.60, so Friday (own IQR
   0.480) read as 0.61x — a spurious "narrower" statement.
   The shipped scale form compares against the median IQR
   of the other levels and ranks deviations from each row's
   own level median. The location form had the mirror
   defect on a three-level fixture (one level shifted +3.5:
   the other two read -0.54 pooled IQRs against a rest that
   was half the shifted level), which is what the
   set-the-primary-aside rule answers: a contrast that
   falls under its bar once the pair's most extreme level
   on that form is set aside is not stated. The same rule
   on the scale form keeps the constant-multiple fixture
   to one statement — the unscaled levels look narrow only
   because the multiple is wide.
2. A pure shift mimics a multiple. A +3 shift on readings
   in [10, 12] concentrates the nine decile quotients at
   about 1.27x, inside the quotient bars; the quotient test
   alone stated it as a constant multiple. The coherence
   requirement separates the two: a rescaling scales the
   spread by the quotient's factor, a shift leaves the
   spread unchanged. The shifted level is now stated as
   `location`, pinned by a dedicated fixture
   (`shifted_level_far_from_zero_is_location_not_multiple`).
3. Claim wording. The scale template characterized the
   centres as "much the same" unconditionally; the hotel
   run below produced a scale statement whose medians were
   3 against 69 pooled (a shift of 0.47 pooled IQRs — under
   the location bar, but not "much the same centre" in
   plain terms). The template now prints both medians
   without the characterization.

## Real-data evaluation

Hotel bookings, 119,390 rows x 32 columns, profiled with
the finished build (detector-only run, report cap raised so
selection does not cut the detector's statements). The
scan considered 23 groupings (21 categorical columns and
the two calendar parts of the reservation_status_date
axis) x 20 numeric measures: the 400-pair cap bound (460
pairs exist), 1,821 levels were tested, 3,642 location and
scale tests formed the Benjamini-Hochberg family, and 20
contrasts were reported — the detector's own cap. All 20
statements were recomputed from the raw file with an
independent Polars computation (level and pooled-rest
medians, level IQR, the typical other level's IQR, the
shift in pooled IQRs, and both row counts): 20 of 20 match
exactly.

The strongest statements:

- `lead_time` for market_segment `Aviation`: IQR 5 against
  a typical level's 74 (0.068x), medians 3 against 69
  pooled — aviation bookings are made a few days out,
  tightly, where the other segments book months ahead.
- `stays_in_week_nights` for `stays_in_weekend_nights` 4,
  5, 6, 8: medians 10, 11, 15, 20 against 2 pooled — the
  two stay-length columns move together in steps.
- `lead_time` for `previous_cancellations` 1 and 24:
  medians 243 and 205 against 66 pooled, IQRs 220 and 78
  against a typical 7.5 — guests with a prior cancellation
  who do book again book far ahead and variably.

One constant multiple was found: `adr` for August, the
month of year of the reservation_status_date axis, tracks
1.42x the pooled rest's deciles (median 128 against 90.9;
largest decile deviation 5.6%) — a proportional peak-season
uplift. In the full run it ranked below the detector's
20-statement cap and was localized by profiling `adr`
alone; its numbers come from that run's metrics,
recomputed the same way.

Two readings of the run as a whole. Several high-ranked
statements take the `agent` and `company` ID codes as
measures (a room type whose bookings arrive through one
agent-ID range states a median far from the pooled one).
The statements are true of the recorded IDs, and the
exposure is the existing screens' as much as this one's —
the pair screens also treat those columns as numeric
measures — but a reader should read an ID-code median as a
channel fact, not a magnitude. And at the default
100-finding report cap only 9 of the 20 statements survive
selection; which statements a capped report keeps is the
selection problem tracked separately (issue #280), not
something this change adjusts.

## Corpus

First run on the finished build: exit 1, five datasets
with failed expectations, 88/93 machine-verified. The
three standing failures (thyroid `pregnant`, adventureworks
`revision_number`, bike-sharing-hourly grouped family —
issue #294) were unchanged. Two changes were this
detector's, both on capped frames:

- Metro Interstate Traffic: the ballpark guard of 70
  findings was exceeded at 89. The report is under the
  100-finding cap, so the change is pure addition: 20
  level_contrast statements (hour-of-day contrasts on
  traffic_volume — 03:00 median 362 against 3,559 pooled,
  16:00 median 6,167 against 3,221 — weather scale
  contrasts on rain_1h, clouds_all, temp, and holiday
  contrasts), spot-verified against the raw data. The
  guard is raised to 89 with the reason recorded in the
  registry, and a level_contrast expectation is added for
  the frame.
- SEC EDGAR company facts: the near-duplicate statement
  (1,970 rows repeat another row on every column except
  value) left the report. The frame's report sits at the
  100-finding cap and the detector's 20 statements are
  admitted ahead of it — JPMorgan Chase (cik 19617) value
  median 391,154,000,000 against 14,042,000,000 pooled,
  and December period_end values 16.49x as variable as a
  typical month, both recomputed exactly. The displaced
  structure is unchanged in the data (exactly 1,970 rows,
  verified), so the expectation is demoted to a known-miss
  tripwire with the mechanism recorded in the registry —
  the corpus's standing treatment for cap displacement;
  it promotes back if a selection change returns the
  statement. A level_contrast expectation is added for
  the frame.

One redundancy observed on the EDGAR frame: `cik` and
`company` are duplicate encodings of one grouping, and the
scan states both, as do the existing screens.

After those registry updates, the corpus stands at exit 1
with exactly the three standing failures of issue #294,
92/95 machine-verified (the two added expectations pass).

## Provenance

Deterministic constructions only: the fixtures use a hashed
pseudo-noise sequence shared by the Rust and Python tests,
no random draws. The real-data frame is the hotel-bookings
file used by the earlier demonstrations (119,390 rows × 32
columns); every statement recorded above was recomputed from
the raw file with an independent Polars computation before
being written here.
