# Corpus accounting and the claim/payload consistency gate

On the same build of the library, the corpus runner's old accounting
reported 113 of 113 expectations passed. Nineteen of those passes were
documented cases standing at "missed" — published anomalies the
library does not find — counted as successes because the check
appended `ok: True` unconditionally. Under the corrected accounting
the same run reads: 84 of 84 machine-verified expectations passed,
and 35 documented cases tallied separately (11 found, 5 partial,
19 missed), with exit code 2 recording that documented misses are on
record. Dataset statuses did not change: 23 ok, 0 failed,
11 skipped of 34 registered.

A new consistency gate profiles the Adult and Bike Sharing frames
and checks every selected finding's claim against its own evidence
payload. Of 18 findings checked, 7 claims contradict their payloads,
all in the two classes already on record: four numeric
extreme-cluster claims (fixed by the draft PR for the numeric half
of the claim-rendering issue) and three categorical exception counts
where the stated dominant share implies one more exception than the
claim and payload list. Two of the three categorical instances had
not been recorded anywhere before this run.

This entry records the protocol and measurements for the
harness-truthfulness work on the evaluation issue (#115). The
planted-signal battery is a separate follow-up.

## Corpus accounting: protocol

Both runs profile the 34 registered datasets with the same build
(onceover 0.1.3, main 613303d4) and the same registry expectations,
plus six `coverage_status` pins the audit added (below). The
"before" run uses the runner as it stood on main; the "after" run
uses the corrected runner. Profiles are deterministic; finding
counts agree between the runs on every dataset.

| | before | after |
|---|---|---|
| datasets ok / failed / skipped | 23 / 0 / 11 | 23 / 0 / 11 |
| expectation tally as printed | 113/113 passed | 84/84 machine-verified |
| documented cases in the tally | counted as passes | found 11, partial 5, missed 19 |
| exit code | 0 | 2 (documented misses on record) |

Documented misses stand on six of the running datasets: thyroid,
adventureworks, sec-edgar-company-facts, metro-interstate-traffic,
bike-sharing-hourly, adult-census-income. (Five further documented
cases sit on test-only datasets that skip in this environment.)

The corrected runner also distinguishes the tallies per dataset,
e.g. adult-census-income: "9/9 machine-verified; documented
missed 5, partial 2" where the old line read "expectations 16/16".

## Anomaly coverage: typed evidence vs the whole-blob regex

The old check extracted every timestamp-shaped string from the
serialized findings and tested the labeled windows against that bag.
The corrected check reads typed event evidence only: an excursion's
`event_start`/`event_end`, a break finding's `breakpoint`, a gap's
`from`/`to`. Coverage on the two registry entries that use the
check is unchanged, with far less material doing the work:

| dataset | windows | old: covered (stamps extracted) | new: covered (typed events) |
|---|---|---|---|
| nab-nyc-taxi | 5 | 5 (90) | 5 (10) |
| nab-ec2-latency | 3 | 3 (39) | 3 (3) |

A unit test pins the negative case the old method failed by
construction: a finding whose claim prose and an unrelated metric
contain the labeled time verbatim, with its own event interval
months away, does not cover the window; nor does a timestamp-shaped
value in the payload of a finding type that carries no event fields.

## Registry audit: completion pins

`coverage_ok` failed only on a coverage status of "error" and had
no registry users. The audit switched the entries where a pass
declining would silently void other expectations to explicit
`coverage_status` checks, at the statuses measured on this build:

- nab-nyc-taxi — breaks, ran_sampled (the anomaly check reads its excursions)
- nab-ec2-latency — breaks, ran_sampled (same)
- metro-interstate-traffic — breaks, ran_sampled; ordered_partitions, ran_exact
- bike-sharing-hourly — breaks, ran_sampled; ordered_partitions, ran_exact

All six pass in the after run (84 = 78 previous machine-verified
expectations + 6 pins).

## Claim/payload gate: protocol and results

`tests/test_claim_consistency.py` profiles Adult and Bike Sharing
exactly as the corpus runner does (standard budget, registry read
options) and runs pure checkers over every selected `extreme_value`
(6 findings) and `categorical_outlier` (12 findings) — 18 in total.
Checks read the first evidence block, the measurement the claim was
rendered from; later blocks are supplementary and may describe
other slices. The checkers verify the stated reached value, bulk
edge, gap, spread and share against the payload; that singleton
wording ("all other values are <= edge") appears only when the
payload lists one extreme; breaker counts and named rows against
the payload's row list; and payload self-consistency:
n − round(dominant_share × n) must equal the outlier count.

Contradictions measured (7):

| dataset | finding | contradiction |
|---|---|---|
| adult-census-income | extreme_value, work_class = Federal-gov, hours_per_week | claim bounds all other values at 80; payload lists extremes 99, 99, 92 |
| bike-sharing-hourly | extreme_value, season = 1, casual | bound 291; payload lists 367, 357, 352 |
| bike-sharing-hourly | extreme_value, hr = 21, casual | bound 107; payload lists 222, 195 |
| bike-sharing-hourly | extreme_value, hr = 5, casual | bound 10; payload lists 16, 15 |
| adult-census-income | categorical_outlier, education = Some-college, relationship = Husband, marital_status | share 0.999180 over 3,658 rows implies 3 exceptions; claim and payload list 2 |
| adult-census-income | categorical_outlier, occupation = Other-service, relationship = Own-child, income | share 0.997263 over 1,096 rows implies 3 exceptions; claim and payload list 2 |
| adult-census-income | categorical_outlier, occupation = Craft-repair, relationship = Husband, marital_status | share 0.999196 over 3,731 rows implies 3 exceptions; claim and payload list 2 |

The first four are the numeric rendering class, fixed by the draft
PR addressing that half of the claim-rendering issue (#111); the
last three are the categorical exception-count class (#111), of
which only the first was previously recorded (as a partial
documented case in the registry). The gate records all seven in a
known-failure table keyed to the owning issues and asserts the
reproduced set matches the record exactly: a fix landing, or a new
contradiction appearing, fails the test until the record is updated
in the same change. No assertion was weakened to accommodate the
known failures.

## Mutation demonstration

The gate's checkers were run against deliberately corrupted copies
of real profiled findings (2026-11-04, this build):

- Adult categorical_outlier (occupation = Other-service,
  relationship = Own-child, marital_status = Never-married slice;
  consistent as profiled, checker returns []): changing the claim's
  "2 rows break the pattern" to "3 rows break the pattern" →
  flagged `breaker-count`. Removing one row from the payload's
  `outlier_rows` instead → flagged `breaker-count`, `breaker-rows`.
- Bike Sharing extreme_value (mnth = 2, casual; consistent as
  profiled): changing the claim's bulk edge from 186 to 179 →
  flagged `bulk-edge`.

Unit tests in the gate file pin the same behavior for bound, gap,
reached-value, breaker-count, and share-implied-count corruptions
on synthetic findings, so the mutation property is exercised in CI
without profiling.
