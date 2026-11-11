# Per-period measure volume: corpus behaviour (2026-11-10)

## Question

Issue #227: per-period coverage (issue #203) counted rows and
distinct entities per period, but not the volume of the measured
activity. A period can hold the usual number of rows and
entities while the activity in it collapses — total sessions or
total tokens per week — and the row and entity counts cannot
see that. The change totals the frame's principal measures in
each period, reports each period's total alongside the row and
entity counts, and reports a period when a principal measure's
total in it falls far below the typical period's. This record
covers the corpus behaviour of the extension: the corpus
contract before and after, and the datasets whose finding sets
changed.

## Protocol

- Builds: the unmodified tree at the current main head and the
  issue-#227 branch build, both `maturin develop --release`
  into separate environments; nothing else differed.
- Corpus: `python benchmarks/corpus/run_benchmarks.py --json-out
  <path>` over all 34 registered datasets, default budget, on
  each build.
- Synthetic panels: the cases in `tests/test_panel_coverage.py`
  and in the panel pass's Rust tests (a volume collapse with
  rows and entities steady; a period short on rows and volume
  together; a signed measure; ten measure columns against the
  cap of 8; a wide volume spread alongside a narrow count
  shortfall, and the other way round).
- The selection rule, as shipped: the principal measures are
  the frame model's measure-role columns that extract to a
  numeric kind, ranked by non-null observation count (most
  observed first, column order breaking ties), at most 8. The
  cap keeps the per-period table and the statements bounded on
  wide frames; the coverage entry states it when it binds.
  Each period's total for a measure is the sum of its non-null
  values in that period.
- The flag rule, as shipped: a period is reported when a
  principal measure's total in it is below 75% of that
  measure's median period total — the same share rule the row
  and entity counts use, without the small-count floors (at
  least 4 rows, at least 2 entities), which guard count noise
  that sums do not have. A measure whose median total is not
  positive, or that carries negative values, is totalled but
  never flags: its sum is a level, not a volume. A period is
  stated once however many of its volumes fall short; a
  shortfall on a second volume is a clause in the same
  `partial_period` statement, not a second statement.
- The quarter-share guard, as shipped: periods are singled out
  only when they make up at most a quarter of all periods, and
  the guard is applied to each kind of shortfall separately —
  the row and entity counts are judged apart from the measure
  volume, and a stated period carries clauses only for the
  kinds that were admitted. The first build of this extension
  judged the kinds together, and the corpus run below shows
  what that cost: on bike-sharing-hourly, 499 of 731 periods
  fall below 75% of some principal measure's typical total
  (daily totals of rider counts and weather readings vary by
  construction), so the joint guard suppressed the six
  short-day statements the counts have made since issue #203.
  Judged separately, the counts' spread there is six periods
  of 731 and their statements stand; the coverage entry
  reports the volume spread instead of naming periods on it.

## Results

Corpus contract, both builds: 23 datasets profiled, 11 skipped;
machine-verified expectations 93/93 passed; documented cases
found 18, missed 10, partial 4; the documented misses sit in
the same five datasets (thyroid, adventureworks,
metro-interstate-traffic, bike-sharing-hourly,
adult-census-income); runner exit code 2 on both builds
(machine checks clean; documented misses on record). No corpus
expectation changed.

Finding sets changed on two datasets, both verified by
profiling the dataset on the main build and on the branch
build directly:

- nab-ec2-latency (4,032 rows): 6 `partial_period` findings
  appear, all on volume alone. The frame is a single latency
  series at a five-minute cadence: each of its 4,021 periods
  holds one row,
  so the row count can never fall short (a period with no row
  is absent, not partial), and the counts stated nothing. The
  stated periods are single observations whose value falls to
  51–69% of the median 45.02 — five of them in one window,
  2014-03-21 03:01–03:41, and one on 2014-03-14. The report
  grows from 9 findings to 15; nothing is displaced.
- bike-sharing-hourly (17,379 rows): the same 6
  `partial_period` findings the counts have stated since issue
  #203, on the same six short days (1, 8, 11, 12, 16, and 17 of
  a typical 24 hourly records), with the same category counts
  (dependencies 13, changes 74, column_roles 0, gaps 6). The
  volume names no further period here: 499 of 731 days fall
  below 75% of some principal measure's typical total, too
  wide a spread for the volume to name a typical period, and
  the coverage entry says so. The per-period table in each
  finding's evidence now carries the seven principal measures'
  totals per day (temp, atemp, hum, windspeed, casual,
  registered, cnt).

One dataset exercises the negative-value rule:

- sec-edgar-company-facts (11,543 rows): the single principal
  measure is `value`, whose per-period total is typically
  296,558,000,000. The column carries negative values, so it
  is totalled but never flags, exactly as the rule states.
  The counts' own spread is unchanged — 141 of 365 periods
  (39%) below 75% of the median 14 rows — so the dataset's
  finding set is identical to the main build's and its
  coverage entry reports the spread, as it did for issue #203.

Synthetic panels, after the change (the main build totals no
measures and states nothing on volume):

- A panel whose rows and entities hold steady while the
  measure's values collapse to an eighth in one week states
  that week on volume alone: total tokens 547.5 against a
  typical 4380 (sum(90..=129); the collapsed week is
  sum(90..=129) / 8).
- A panel whose rows and volume fall short in the same week
  states the week once, with both clauses: 16 rows against a
  typical 40, total tokens 195 (sum(90..=105) / 8).
- A signed measure (values ±(40 + index)) is totalled — the
  coverage entry reports its typical total, 2380 — and never
  flags when its total swings negative.
- A wide volume spread (four of ten periods below 75% of the
  median total) does not silence a narrow count shortfall:
  the short week is stated on its counts, with no volume
  clause. The other way round, a wide count spread (three of
  ten periods short on rows, their totals held near typical
  by quadrupled per-row values) does not silence a volume
  collapse: the collapsed week is stated on volume alone.

## Reading

The extension states the case it was built for — activity
collapsing inside a period whose rows and entities hold — and
on the corpus that case is nab-ec2-latency's dip window, which
no count could have named. Where per-period measure totals
vary by construction, as on bike-sharing-hourly, the volume
names nothing and the counts' statements are untouched: the
separate guard is what keeps the new signal from suppressing
the old one, and the first build's joint guard is recorded
above as the counterexample. The corpus contract is
untouched: no expectation was edited, and 21 of the 23
profiled datasets produce identical category counts (the 11
skipped datasets skip on both builds).
