# The frame account: binary responses, binned groupings, duplicate encodings

Date: 2026-11-14. Change: issue #286. Code: the frame account in
`crates/onceover/src/account.rs` (dominant structure), the role
assignment in `crates/onceover/src/frame.rs`, and the report
contract (`FrameStructureRate` in `crates/onceover-report`).

## Question

The frame account leads every report, and on three ordinary
tables it could not say the most important thing about any of
them. Measured on the unmodified code (base commit ad0a0fe3):

- Adult (48,842 rows): both account lines are about `age` —
  "the marital_status dimension organises the frame: 33% of the
  variance in age sits between marital_status values; age
  averages highest for Widowed (59.377)" and the same shape for
  `relationship` at 22%. The outcome column, `income`, appears
  nowhere: only numeric `measure` columns were eligible as
  account responses, and a two-level string column is roled
  `dimension`. The sampling weight `final_weight` (the census
  `fnlwgt`) is roled `measure`.
- Titanic (891 rows): the account prints the `pclass` grouping
  and the `class` grouping with identical numbers — "35% of the
  variance in fare, 14% of the variance in age" twice, once per
  encoding. `survived`, a 0/1 flag, appears nowhere: flags were
  excluded from the account's responses by construction.
- Diamonds (53,940 rows): no account line mentions `price`. The
  only stated peak is "table averages highest for Fair
  (59.054)" — a 0.5% difference on a 43–95 scale, stated as a
  headline. The continuous driver `carat` is roled `measure`,
  so it was structurally ineligible as a grouping.

## Change

- Every flag and every exactly-two-level column is a response,
  measured as the rate of its on state (the higher level in
  sorted order) and scored by the same eta-squared on its 0/1
  indicator as the measures, so a binary outcome competes with
  measures on one scale. A grouping that organises a binary
  response states the peak group's rate, its lift over the base
  rate, and its support, under two bars: at least 20 rows
  carrying the response in the peak group, and a peak-to-lowest
  group rate contrast of at least 0.10.
- Continuous measures are candidate groupings, binned on their
  decile edges (duplicated edges from tied values collapse).
  A binned grouping never scores its own source column.
- Duplicate and rollup encodings are collapsed before the
  groupings are chosen: a grouping whose partition duplicates,
  refines, or is refined by an earlier kept grouping is dropped,
  and two encodings of one binary outcome (identical or
  complementary on/off patterns) are measured once.
- A column named as a weight (a whole `weight`/`weights`/`wt`
  token, or `fnlwgt`) receives no role, so no consumer reads it
  as a measurement.
- A stated peak must clear a materiality bar: the spread of the
  grouping's group means must reach a tenth of the measure's
  own range.

Two further guards came out of the first changed build's output
on Titanic (below): a grouping whose two groups restate a
binary response under another column's encoding is not used as
a grouping, and a grouping never claims a response it
determines exactly (constant within every group), since that
share is 1 by construction.

## After-state on the same three tables

Adult — both lines now measure the outcome, and the weight is
out of the measures:

- "the relationship dimension organises the frame: 42% of the
  variance in sex, 22% of the variance in age, 21% of the
  variance in income sits between relationship values; age
  averages highest for Husband (43.906); the sex rate (Male) is
  highest for Husband at 100% against a base rate of 66.8% (lift
  1.496x; 19,716 rows in the group); the income rate (>50K) is
  highest for Wife at 46.9% against a base rate of 23.9% (lift
  1.96x; 2,331 rows in the group)"
- The `marital_status` line is the same shape (income share
  20%; income rate highest for Married-civ-spouse at 44.6%,
  lift 1.865x). `final_weight`'s role is now null, with the
  evidence "numeric column named as a weight (final_weight);
  a weight is not a measurement".

Titanic — the outcome is measured in both lines, no grouping
prints twice, and neither line is a construction:

- "the who dimension organises the frame: 90% of the variance
  in sex, 34% of the variance in age, 32% of the variance in
  survived sits between who values; age averages highest for
  man (33.173); the sex rate (male) is highest for man at 100%
  against a base rate of 64.8% (lift 1.544x; 537 rows in the
  group); the survived rate (1) is highest for woman at 75.6%
  against a base rate of 38.4% (lift 1.971x; 271 rows in the
  group)"
- "the sex dimension organises the frame: 83% of the variance
  in adult_male, 30% of the variance in survived sits between
  sex values; the adult_male rate (true) is highest for male at
  93.1% against a base rate of 60.3% (lift 1.544x; 577 rows in
  the group); the survived rate (1) is highest for female at
  74.2% against a base rate of 38.4% (lift 1.933x; 314 rows in
  the group)"
- `pclass` and `class` no longer print at all: the collapse
  keeps one encoding, and `who` and `sex` organise more of the
  frame than either. The first changed build headlined `alive`
  — the outcome's own second encoding — as a grouping claiming
  100% of `survived`, and `who` claiming `adult_male` at 100%
  (a flag derived from it). Both are constructions, not
  structure; the two guards above remove them, and the lines
  quoted here are the guarded build's.

Diamonds — the account is now organised by continuous drivers,
and the immaterial peak is gone:

- "the y quantile bins organise the frame: 97% of the variance
  in x, 94% of the variance in carat, 92% of the variance in z
  sits between y quantile bins; x averages highest in 7.3 and
  above (7.8)"
- "the carat quantile bins organise the frame: 97% of the
  variance in x, 92% of the variance in z, 92% of the variance
  in y sits between carat quantile bins; x averages highest in
  1.51 and above (7.859)"
- `cut` no longer headlines, and its "table averages highest
  for Fair" peak is not stated: the spread does not reach a
  tenth of `table`'s range. One gap remains, stated plainly:
  no printed line names `price`. Measured independently from
  the raw table, the share of `price`'s variance between the
  carat decile bins is 0.8326 — price is organised by the carat
  grouping, but it ranks fourth among that grouping's
  responses, behind the three physical size measures
  (x 0.9658, z 0.9226, y 0.9204), and the account prints three
  shares per grouping. The mechanism the issue asked for is in
  place; on this table the printed shares are spent on three
  near-copies of one size axis before they reach price.

## Fixtures

- Rust (`crates/onceover/src/account.rs`): a binary response is
  stated as a rate; duplicate encodings occupy one account
  slot; a binned measure can organise a response; an immaterial
  peak is not stated; an outcome's own encoding is not a
  grouping; a determined response is not claimed. The account
  module runs 10 tests, all passing.
- Python (`tests/test_frame_account_binary.py`): the three
  tables in miniature — Adult states an `income` rate with
  lift above 1.5 and never treats `final_weight` as a measure;
  Titanic states a `survived` rate and never prints both
  `pclass` and `class`; Diamonds states a binned grouping and
  no immaterial `cut` peak. The file fails against the
  unmodified code and passes against the changed build.

## Corpus

`benchmarks/corpus/run_benchmarks.py` on the changed build:
exit 1, 90 of 93 machine-verified expectations passing — the
same three standing failures as on the unmodified code, and no
others: thyroid (a `pregnant` concentration expectation),
adventureworks (a `revision_number` value-concentration
expectation), and bike-sharing-hourly (the (cnt, casual) →
registered grouped-family expectation). No corpus expectation
changed under this fix. Adult's own corpus entry verifies
12 of 12.

## Gate

Release check passes; `scripts/test_release.py` 6 passed;
`cargo fmt --check` clean; clippy clean under `-D warnings`;
`cargo test --workspace --locked` 423 passed, 0 failed;
`pytest tests/ -q` 645 passed, 1 skipped.

## Provenance

Adult is the corpus copy of the UCI Adult dataset
(doi 10.24432/C5XW20), read with `null_values=["?"]`. Titanic
is the seaborn `titanic` table (891 rows) and Diamonds the
ggplot2 `diamonds` table (53,940 rows); both were fetched for
this evaluation only and are not committed to the repository.
The before-state was reproduced on the unmodified base commit
with the same three tables and the same profiling call
(`onceover.profile` at the standard budget).
