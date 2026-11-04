# Frame model step 2: the account block and digest re-ranking (2026-11-03)

Protocol: the three evaluation frames of the frame-model design
(Beijing Multi-Site Air Quality, Metro Interstate Traffic Volume,
state unemployment long form) plus Bike Sharing hourly, the corpus
frame whose selected findings were dominated by calendar
restatements. Each was profiled with the baseline build (main at
ddf3a9ef, schema 0.4) and with the step-2 build (schema 0.5), pass
1, no hints, standard budget, on the same machine. Beijing was read
with `null_values=['NA']`; the other three are the corpus files as
published (string timestamps, no date parsing). The acceptance
standard is the three target accounts written before
implementation: the digest, led by the account block, should make
their statements, or correct them with measured evidence where a
target statement is wrong. Finding counts and the top-10
composition are the record; timings are not compared.

## What step 2 adds

- The `frame` section gains the account: `grain` (the frame's key,
  verified by hashing the candidate tuples; factorized when the
  frame crosses its dimensions with its time axis, with the
  balance and the absent-combination count measured; the near key
  when the axis nearly identifies rows, with its duplicate count),
  the axis span fields (`min`, `max`, `distinct_values`, `step`),
  and `dominant` (up to two groupings — the axis itself, its
  annual/weekly/daily cycles when the axis samples them, or a
  dimension — whose strongest measure has at least 0.10 of its
  variance between the grouping's levels, eta-squared, with the
  shares and the peak level). Schema 0.4 → 0.5, additive.
- The account is stated three ways from the same records: the
  digest leads with it (grain, axis, dominant structure, before
  the headline), the markdown report's frame section states it,
  and the grain / near key is also a frame-scope finding that
  replaces the bare key finding for the same columns.
- Presentation order is by report band — account, completeness,
  relations, column structure, deviations — with the selection
  score inside a band, in the report body and the digest.
  Selection is unchanged: the caps consume candidates in
  demotion-adjusted score order (see "Selection and the caps"
  below).
- Restatement demotion: a relation finding (dependency,
  association, conditional rule) whose subject lies inside the
  calendar family — the axis's own columns, every column the axis
  determines exactly (measured directly: where the timestamp is
  itself unique, the dependency pass reports no dependency from
  it, so the family cannot be seeded from mined dependencies
  alone),
  closed under the exact functional dependencies — is demoted
  with reason `calendar_derivation`; a finding whose whole
  subject is scaffolding columns is demoted with reason
  `scaffolding_subject`. Factor 0.6, declared in the evidence,
  counted in the `frame_model` coverage entry, never a removal.
- One coherence fix: the dependence pass's coverage summary now
  says "strong pairwise" where it scopes its own statement, so it
  no longer reads as contradicting the multivariate predictability
  findings in the same report.

## Beijing Multi-Site Air Quality (420,768 rows × 18)

The digest now opens:

```
account: the grain is 12 station × 35,064 year/month/day/hour: 420,768 rows, every combination exactly once — (station, year, month, day, hour) jointly identify rows
account: time axis — a timestamp assembled from year/month/day/hour, 2013-03-01 00:00:00 to 2017-02-28 23:00:00, in 1 hour steps (35064 distinct values)
account: the annual cycle organises the frame: 82% of the variance in TEMP, 80% of the variance in DEWP, 68% of the variance in PRES sits between months of the year; TEMP averages highest in July (27.194)
account: the daily cycle organises the frame: 22% of the variance in O3, 12% of the variance in WSPM sits between hours of the day; O3 averages highest at 16:00 (102.31)
```

Statement by statement against the target account:

1. **Grain and key — made.** The factorized grain is stated with
   the target's numbers (12 × 35,064 = 420,768, balanced). The
   (No, station) key finding is also in the report, at rank 2;
   the grain statement is the account's form of the same fact.
2. **Time axis — made.** Span 2013-03-01 00:00 to 2017-02-28
   23:00, hourly step, the four part columns named as the axis
   source. (Temporal passes running along the axis is step 1's
   result and still holds.)
3. **Completeness — not made; it is step 3.** CO's missingness
   (4.9% of hours, 9.1% at Dongsi) is a completeness-against-the-
   grain statement. Step 2 reserves its position — completeness
   is the second report band — and states none of it; the
   per-column counts still exist only in the column cards.
4. **Dominant structure — made.** The annual cycle is stated as
   the organising structure with measured shares (TEMP 0.82,
   DEWP 0.80, PRES 0.68), and the daily cycle second (O3 0.22).
   The top 10 findings before were ten changes findings (the
   step discontinuities the target counted at 25 in the top
   ranks); after, the top 10 are the grain, the key, and eight
   dependency findings — no changes finding appears. The
   discontinuities are not demoted by step 2 (they are measure
   findings, not calendar restatements; cycle-aware demotion of
   discontinuities is step 4) — they rank last because deviations
   are the last band. 20 findings in the selected report carry a
   step-2 demotion (18 `calendar_derivation`, 2
   `scaffolding_subject`).
5. **Relations — made, re-ranked as the target requires.** The
   PM2.5 predictability finding (holdout R² 0.87, the target's
   number) is at rank 5; the temperature local-regimes finding
   is at rank 9; the RAIN concentration and the wind-speed
   isolation gap are in the selected report.
6. **What to do next — not made.** No step of the frame-model
   sequence so far produces next-step guidance; the account
   carries no recommendation. Recorded as an open item against
   the target account, not silently dropped.

Selected-set composition, before → after (100 findings both):
changes 72 → 72, dependencies 11 → 11, associations 7 → 7,
uniqueness 1 → 2 (the grain statement joins the key finding),
formats 2 → 5, column_roles 4 → 0, concentration 1 → 1,
intervals 2 → 2. The set barely moves — by design (below); the
order is what changes.


## Metro Interstate Traffic Volume (48,204 rows × 9)

Statement by statement:

1. **The key that fails — made.** The account states the near
   key with the target's exact numbers: "the time axis does not
   identify rows: 40,575 distinct date_time values in 48,204
   rows — 7,629 rows repeat a timestamp already present". It is
   the report's rank-1 finding and the digest's grain line. The
   17 full-row duplicates are stated separately at rank 2.
2. **Time axis and its holes — half made.** Span and frequency
   are stated (2012-10-02 09:00 to 2018-09-30 23:00, 1-hour
   steps, 40,575 distinct values). The hole count — 11,976 of
   52,551 hourly slots absent (22.8%) — is grid completeness,
   step 3, and is not stated.
3. **What the columns are — made, and already the report's best
   content.** The rain_1h sentinel is at rank 3 with the
   target's numbers (121 occurrences of 0.3, 97 inside rows
   13039–23670); weather_description → weather_main at rank 7
   (0 violations); holiday concentration at rank 8.
4. **Dominant structure — made, with one target expectation
   corrected by measurement.** The daily cycle in
   traffic_volume is stated (eta² 0.77), and the annual cycle
   in temp second (0.70). The target also expects a weekly
   cycle; measured directly, the marginal day-of-week share of
   traffic_volume variance is 0.054 — below the 0.10 dominance
   bar — so the account does not state one. The rush-hour
   structure the target points at is carried by the daily
   cycle; the weekly modulation is real but small as a
   marginal share.

Selected findings 37 → 38 (the near-key statement is the
addition); the top 10 goes from five changes findings to none.

## State unemployment, long form (31,059 rows × 3)

Statement by statement:

1. **Grain and key — made in full.** "The grain is 51 state ×
   609 date: 31,059 rows, every combination exactly once", and
   the axis statement carries the span (1976-01-01 to
   2026-09-01, 1-month steps, 609 distinct values). Before, the
   bare composite-key finding was the report's only account of
   the frame; the grain statement replaces it (the selected set
   is the same 7 findings, re-ranked).
2. **The one hole — not made; it is step 3.** October 2025
   missing for all 51 states at once is completeness against
   the grain. Nothing in step 2 states it.
3. **Dominant structure — made.** "The shared time pattern
   organises the frame: 54% of the variance in
   unemployment_rate sits between time points;
   unemployment_rate averages highest in 2020-04 (13.643)" —
   the target's national cycle, with its April 2020 peak mean
   (13.6% in the target account). The second stated structure
   is the state dimension (0.23) — the target's "state explains
   far less", now with both numbers on the record. The
   ordered_partitions finding (first split 2008-10-01) is in
   the report at rank 7.

## Bike Sharing hourly (17,379 rows × 17)

Before: 100 selected findings — 72 changes, 13 dependencies,
10 conditional rules, 3 intervals, 1 uniqueness, 1
concentration — and a top 10 of ten changes findings. Roughly
half the selected findings restated one calendar fact (the
case-study count was about 50 of 100).

After: the digest leads with "the grain is 24 hr × 731 dteday:
(hr, dteday) jointly identify rows in 17,379 rows — the cross
is not full (17,544 combinations, 165 absent)" — the natural
key and the absent-combination count, which the pre-step-2
report never stated at all. The dominant structures are the hr
dimension (registered 0.51, cnt 0.50, casual 0.31) and the
shared time pattern. The top 10 is now: the grain, the
`instant` primary key, a sentinel finding, the three functional
dependencies among cnt / casual / registered (the dataset's
account identity, present as dependencies), four predictive
findings, and a conditional rule — no changes finding appears
until the deviations band. 59 findings were demoted before
selection (the calendar derivations among the rules,
dependencies, and predictions, plus scaffolding-subject
findings); 27 of the selected 100 carry a demotion reason.

Selected-set composition, before → after: changes 72 → 77,
dependencies 13 → 13, conditional rules 10 → 4, intervals
3 → 3, uniqueness 1 → 2, concentration 1 → 1. The demotions
lower calendar-restatement scores, so at the 100-finding cap
some demoted rules trade places with undemoted changes
findings; the corpus expectations all still pass (below).

## Selection and the caps — the precise interaction

Step 2 does not change which findings a report selects, beyond
the demotions' effect on scores. The design's evaluation gate
is that no bundled dataset loses findings it currently earns
(except scaffolding-subject findings), and its standing rule is
no budget changes; the first implementation of this step
ordered selection itself by band and failed that gate — six
corpus datasets lost earned step-discontinuity and outlier
findings to band-3 column-structure findings at the cap. The
shipped semantics: caps consume candidates in
demotion-adjusted score order, exactly as before; the band
orders the selected findings for presentation.

The consequence to state plainly: cap pressure can still crowd
a true relation out of a capped report. On Bike Sharing, the
functional dependency dteday → holiday is generated on every
run and is still not selected — 100 higher-scored findings take
the slots, as the corpus registry records in its documented
missed case for this entry. Step 2 as designed does not address
report-level selection; the demotion of the calendar
restatements lowers some competing scores but does not free a
slot for it on this frame. Any change to that is a selection
-policy change, out of this step's scope.

## Corpus

The full corpus runner on the step-2 build: **33 registered,
22 ok / 0 fail / 11 skipped** — identical to the baseline on
main, with **no registry expectation changes**. Every dataset
keeps the findings it earned; the two documented cases the step
touches (traffic's duplicate timestamps, Bike Sharing's
dteday → holiday) behave as analysed above: the near key is now
stated in the traffic report, and the Bike Sharing dependency
remains a documented missed case for the reason stated.

## Not in this step

- Completeness against the grain (step 3): Beijing target
  statement 3, traffic's grid holes, the unemployment panel's
  October 2025 hole. The digest's band order reserves the
  completeness position directly under the account.
- Cycle-aware demotion of discontinuities (step 4): Beijing's
  seasonal discontinuities are re-ranked by band in this step,
  not demoted.
- Detector-applicability gating (Benford's law on bounded
  counts, traffic target statement): a frame-model judgment the
  design assigns to the frame model, but not part of the step-2
  specification; recorded here as a follow-up, not implemented.
- Next-step guidance (Beijing target statement 6): no step so
  far produces it.
