# Order statements require a stated basis

Date: 2026-11-16. Change: issue #293 (issue #317 phase 3, first
statement family). Code: `crates/onceover-deps/src/order.rs`,
`crates/onceover/src/pipeline.rs`.

## Question

The order pass stated order constraints and systematic order
violations between columns with no relation an analyst would
name: on the hotel-bookings frame, `children <= agent` (a count
against a travel-agent ID code), `previous_cancellations <=
booking_changes`, and three more of the same shape sat among the
highest-scored findings. Nothing in the screen distinguished a
pair whose ordering means something from a pair whose columns
merely share a numeric type and a small range.

## The rule

Following the direction recorded on issue #293, a certified or
systematic order direction is now stated only with a stated
basis, checked in this order and recorded in the finding's
evidence as `order_basis`:

- `functional` — the pair is functionally related in the run's
  exact functional dependencies (the pairs the pass already
  received as `derived_pairs`);
- `temporal` — both columns are timestamps of one unit, two
  recordings on the one time scale;
- `predictive` — a predictive dependency between the columns:
  Spearman rank correlation at least 0.5 in magnitude on the
  pass's screening sample, recorded as `basis_spearman`;
- `part_whole` — for certified candidates only, one column is a
  component of the other under the pass's existing component
  test (identity or containment route, recorded as
  `part_whole_basis`).

A direction that passes a reporting bar without a basis is not
stated and does not fall through to another form; it is counted
in the pass's coverage entry ("N reportable order directions
not stated: no stated basis (...)"). The part-against-whole
violation form is unchanged: its component relation is its
basis by the same standard.

Calibration of the 0.5 bar, measured on full data with SciPy
before the change: the weakest genuine pair that must keep its
statement, the issue #132 protocol plant (`hi = lo + U(0,50)`),
has Spearman 0.698; the corpus-pinned satellite pairs sit at
0.812–0.972; the five hotel pairs named on the issue sit at
−0.073 to 0.076. The bar sits between the two groups with
margin on both sides.

## Method

Before/after on one machine, one registry: the gate
(`benchmarks/gate/run.py`) run against a release build of the
base commit (daf7a790) and of the changed tree; order findings
counted in full reports per gate dataset through the gate
harness's own data preparation, in default and target modes;
and the hotel-bookings frame (119,390 rows × 32 columns)
profiled under both builds.

## Results

Gate totals over the gated datasets, base → changed:

| Metric | Base | Changed |
|---|---:|---:|
| reference `must` at 4k, default / target (of 11) | 7 / 7 | 7 / 7 |
| reference `must` at 8k and full, both modes | 11 | 11 |
| reference `should` at 4k, default / target (of 12) | 7 / 8 | 7 / 8 |
| reference `should` full, default / target | 11 / 10 | 11 / 10 |
| planted found, full / at 4k (of 46) | 34 / 20 | 34 / 20 |
| permuted false relationship statements, full / 4k | 144 / 136 | 138 / 130 |
| permuted false statements with a target, full / 4k | 75 / 67 | 74 / 66 |
| repeated facts at 4k, default / target | 4 / 3 | 4 / 3 |
| invariance violations, total | 408 | 408 |
| truth: ok / mismatch / error | 858 / 0 / 0 | 858 / 0 / 0 |
| check_failed | 0 | 0 |
| findings in gated default profiles | 1089 | 1086 |
| profile seconds, summed | 38.1 | 43.2 |

`compare.py` reads FAIL with 2 worse rows, both the same
metric: the target-mode finding-line share on
absenteeism-at-work (418 → 401 per-mille; total 4531 → 4514).
The two suppressed order statements were finding lines in that
digest; with them gone, a larger share of the fixed digest
budget is header and section text. No recall metric moves.
12 rows improve (the permuted reductions and the default-mode
finding-line share), 39 are unchanged. No acceptance entries
were added; the baseline decision stands with the maintainer.

Order findings in full reports, per gate dataset (default
mode; target mode in parentheses where the dataset has a
target):

| Dataset | Before | After | Directions no longer stated |
|---|---:|---:|---|
| absenteeism-at-work | 5 (5) | 3 (3) | `body_mass_index <= age` (certified, no basis), `weight <= hit_target` (systematic, no basis) |
| hospital-dha | 24 | 21 | `HOSP_BEDS <= PYMT_P_MD_VISIT`, `PARTB_EVAL_MGMT <= MD_PYMT_P_DCD`, `PNEUM_SCORE <= CHF_SCORE`, `SNF_BEDS <= PYMT_P_MD_VISIT` (no basis); one direction gained: `HI_IC_BEDS <= TTL_IC_BEDS`, previously displaced by the findings cap, now stated with the part/whole basis |
| all other gate datasets | — | unchanged | — |

Totals: 65 → 60 order findings in default mode, 18 → 16 across
the target-mode datasets.

Hotel-bookings frame: 10 order findings → 5. No longer stated:
`children <= agent` (certified, 1 violation), `children <=
booking_changes` (systematic, 7,062 violations),
`previous_cancellations <= booking_changes` (systematic,
6,256), `booking_changes <= total_of_special_requests`
(systematic, 11,851), and `days_in_waiting_list <= lead_time`
(certified, 0 violations — a plausible bound, but the pair
carries none of the four bases: rank correlation below the
bar, no functional relation, no component relation; recorded
here as the rule's cost on this frame). Still stated, all
through the part-against-whole form with the containment
route: `total_of_special_requests <= stays_in_week_nights`,
`stays_in_weekend_nights <= adults`, `booking_changes <=
stays_in_weekend_nights`, `stays_in_weekend_nights <=
stays_in_week_nights`, `total_of_special_requests <=
stays_in_weekend_nights`.

## Scope

The containment route of the component test is unchanged, and
it certifies pairs the stated-basis rule was written to stop:
measured on the current build, containment statements include
bank `contacts_before_this_campaign <=
contacts_during_this_campaign` (Spearman −0.108) and three of
the five hotel statements above. A rank-correlation floor
cannot repair that route — two satellite pairs it certifies
measure 0.415 and 0.651, above a genuine contained-part test
pair at 0.388 — so tightening it is a separate change with its
own measurement, filed as issue #337. This change is
therefore Part of #293, not its closure.

## Provenance

Base commit daf7a790 (onceover 0.1.5, report schema 0.8),
release builds (`maturin develop --release`), Polars 2.1.0.
Gate scorecards: `w317p3-gate-before.json` /
`w317p3-gate-after.json` run outputs (same registry, same
machine). Order counts: `count_order.py` over the gate
registry. Hotel frame: `~/workspace/demo-hotel/hotels.csv`
(Antonio, de Almeida & Nunes, Data in Brief 2019), read with
null values NA/NULL/empty.
