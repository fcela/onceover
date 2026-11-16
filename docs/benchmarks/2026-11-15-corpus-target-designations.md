# Corpus target designations: the target-gated passes under machine check (2026-11-15)

Issue #315. Three registered detectors are target-gated —
`target_shapes`, `leakage`, and `model_failures` — and run only when
a caller designates a target column. The benchmark corpus designated
no target anywhere, so the three passes never ran in a corpus
profile and no expectation named their finding types (`predictive`
from the shapes pass, `leakage_signature`, `model_failure`). This
change designates targets in the corpus configuration and pins the
passes' behavior from a measured run. No detector code changes.

## What changed

- `benchmarks/corpus/run_benchmarks.py`: the profile call passes
  `target=entry.get("target")`.
- `benchmarks/corpus/registry.json`: a top-level `target` key on
  fifteen entries — adult-census-income (`income`), bank-marketing
  (`subscribed_term_deposit`), taiwanese-bankruptcy (`bankrupt`),
  south-german-credit (`credit_risk`), thyroid (`diagnosis`),
  soybean (`class`), splice (`class`), abalone (`rings`),
  absenteeism-at-work (`absenteeism_time_hours`),
  bike-sharing-hourly (`cnt`), metro-interstate-traffic
  (`traffic_volume`), congressional-voting (`party`),
  lace-satellites (`Class_of_Orbit`), credit-card-fraud (`class`),
  incident-event-log (`priority`). Every column name was verified
  against the bundled file's header before it was written.
  congressional-voting and lace-satellites carry explicit
  `detectors` lists, which suppress target activation, so both
  lists gain the three target-gated ids. Every remaining entry
  records its no-target reason in its notes.
- `benchmarks/corpus/README.md`: the entry-level `target` key is
  documented, with its distinction from the `target` field inside
  a `finding` expectation.
- Expectations: 90 added (per targeted entry: one
  `coverage_status` expectation per pass, and one `finding`
  expectation per type, each with `target` set to the designated
  column), and 8 pre-existing expectations re-baselined with their
  mechanism recorded (below).

## Protocol

Verdicts were set from measurements, never asserted in advance:

1. The designations landed with no new expectations. The corpus
   ran on the unmodified configuration first (the before run) and
   again with the designations in place.
2. A separate measurement pass profiled each targeted entry
   exactly as the runner does and attributed every finding to its
   producing detector through `evidence[].method.detector` — the
   report's `predictive` type is emitted by several detectors, so
   type alone does not attribute a statement to the shapes pass.
3. Expectations were pinned to the measured state: coverage
   statuses exactly as read, findings found or missed exactly as
   they appear in the report body (selected or grouped).
4. Where a statement's absence needed a mechanism, a diagnostic
   profile at `max_findings=1000` separated generation from
   selection: a statement present at the wide cap but absent from
   the 100-finding body is cap displacement.

## Measured results

All three passes record coverage status `ran_exact` on all fifteen
entries — the designation activates them everywhere, including on
the two entries with explicit detector lists. "Shapes reported" is
the shapes pass's own coverage count; "in body" counts statements
attributed to the pass in the selected and grouped findings.

| Dataset | Target | Shapes reported / in body | Leakage: strongest column | Model failures stated / in body | Findings before → after | Seconds before → after |
|---|---|---|---|---|---|---|
| thyroid | diagnosis | n/a (categorical target) | tsh 0.676 | declines (4 classes) | 100 → 100 | 2.09 → 2.01 |
| abalone | rings | 4 / 2 | shell_weight 0.398 | 1 / 1 | 90 → 93 | 0.42 → 0.59 |
| soybean | class | n/a (categorical target) | fruit_spots 0.321 | declines (19 classes) | 80 → 80 | 0.55 → 0.53 |
| splice | class | n/a (categorical target) | base_30 0.262 | declines (3 classes) | 47 → 47 | 5.35 → 5.29 |
| bank-marketing | subscribed_term_deposit | n/a (categorical target) | last_contact_duration_seconds 0.207 | 20 / 20 | 100 → 100 | 4.09 → 5.05 |
| taiwanese-bankruptcy | bankrupt | 10 / 0 | Interest Expense Ratio 0.259 | 6 / 6 | 100 → 100 | 15.74 → 18.16 |
| absenteeism-at-work | absenteeism_time_hours | 0 / 0 | reason_for_absence 0.265 | 0 / 0 | 100 → 100 | 0.53 → 0.63 |
| south-german-credit | credit_risk | n/a (categorical target) | status 0.105 | 11 / 11 | 77 → 88 | 0.46 → 0.50 |
| incident-event-log | priority | n/a (categorical target) | urgency 0.710 | declines (4 classes) | 100 → 100 | 26.45 → 28.44 |
| credit-card-fraud | class | 2 / 2 | amount 0.124 | 16 / 16 | 97 → 84 | 32.02 → 35.64 |
| congressional-voting | party | n/a (categorical target) | physician_fee_freeze 0.765 | 20 / 20 | 63 → 83 | 0.15 → 0.18 |
| lace-satellites | Class_of_Orbit | n/a (categorical target) | Type_of_Orbit 0.894 | declines (4 classes) | 99 → 99 | 0.97 → 0.97 |
| metro-interstate-traffic | traffic_volume | 0 / 0 | temp 0.071 | 0 / 0 | 89 → 89 | 3.35 → 3.68 |
| bike-sharing-hourly | cnt | 3 / 0 | registered 0.878 | 19 / 10 | 100 → 100 | 3.60 → 4.43 |
| adult-census-income | income | n/a (categorical target) | relationship 0.208 | 18 / 18 | 100 → 100 | 3.63 → 4.91 |

### The shapes pass

The pass decomposes a numeric target, so on the nine categorical
targets it records `ran_exact` with a detail stating the decline,
and on credit-card-fraud and taiwanese-bankruptcy it treats the
0/1 target as numeric. It reports decisive shapes on four entries.
On abalone (4 reported) and credit-card-fraud (2) some or all
reach the body; on taiwanese-bankruptcy (10 reported) and
bike-sharing-hourly (3) all are displaced at the 100-finding cap —
generated, but neither selected nor grouped. The `predictive`
expectations record this exactly: found where a shapes statement
reaches the body (or where another pass states a predictive
relation on the designated target, which the note says), missed
where nothing on the target reaches the body, with the
displacement named in the note.

### The leakage screen

No entry produces a `leakage_signature`; all fifteen expectations
are known-miss tripwires with the measured mechanism in the note.
On thirteen entries the strongest single column is below the 0.80
bar. Two clear the bar and fail the dominance route, which
requires the strongest to dwarf the runner-up by 1.5x:
lace-satellites (`Type_of_Orbit` 0.894 against
`Inclination_radians` 0.659) and bike-sharing-hourly (`registered`
0.878 against `instant` 0.706). Bike-sharing is the designed pair
control — `casual` and `registered` are the exact components of
`cnt` — and the screen is single-column by design, so the pair
does not fire it; the identity itself is stated by the identities
pass and was already expected. Incident-event-log is the designed
determination control: `priority` is determined exactly by the
pair (`impact`, `urgency`), which the functional pass states,
while each column alone scores 0.710 / 0.706. On lace-satellites,
`Apogee_km` determines `Class_of_Orbit` exactly (the functional
expectation on the entry), but it takes a distinct value on
nearly every row, so the determination is vacuous under the
screen's average-group-size guard (see
`docs/benchmarks/2026-11-13-leakage-signatures.md`).

### The model-failure pass

The pass states slice failures on eight entries (abalone,
bank-marketing, taiwanese-bankruptcy, south-german-credit,
credit-card-fraud, congressional-voting, bike-sharing-hourly,
adult-census-income), in each case up to its 20-statement cap or
the confirmed count. On bike-sharing-hourly it states 19 and 10
reach the body at the report cap. It declines the five multi-class
categorical targets (it evaluates numeric and binary targets),
and on absenteeism-at-work and metro-interstate-traffic it runs
and confirms nothing: 48 of 434 slice candidates clear the search
bars on absenteeism and none confirm on the held-out half;
nothing clears the search bars on metro.

## Pre-existing expectations re-checked

Designating a target changes the target column's role across the
whole profile, so every pre-existing expectation on the fifteen
entries was re-checked against the designated run. Eight changed
state, all by one mechanism — report-cap displacement by the
target passes' findings — and each was re-baselined to a
known-miss with the mechanism in its note, following the
registry's existing practice:

- adult-census-income (7): near-duplicate rows, duplicate rows,
  value concentration on `capital_loss`, `capital_gain`, and
  `native_country`, and boundary mass on `capital_gain` and
  `capital_loss`. Each statement still generates: at
  `max_findings=1000` all seven match (3, 1, 1, 1, 1, 4, and 4
  findings respectively).
- bank-marketing (1): the `formats`-category expectation. Six
  formats findings match at `max_findings=1000`; none reaches the
  100-finding body once the target is designated.

One pre-existing failure is not part of this change.
incident-event-log's value-concentration expectation on `impact`
fails identically on the unmodified main configuration whenever
the prepared file is present locally (the before run below): the
statement generates — it matches at `max_findings=1000` — and is
displaced at the 100-finding cap with no target involved.
Designation did not change its state, so it is left as measured
before this change and recorded here rather than re-baselined
under this issue.

## Tallies

Local runs (both test-only files present locally and
hash-verified, so 100 machine-verified expectations run before
this change; in CI the two test-only entries skip and the totals
are lower):

- Before (unmodified main configuration): 99/100 machine-verified
  passed; the one failure is the incident-event-log expectation
  above.
- Designations in place, before pinning: 91/100 — the eight
  cap-displaced expectations above plus the pre-existing failure.
- Final, with the 90 new expectations pinned and the 8
  re-baselines applied: 189/190 machine-verified passed. The one
  failure is the pre-existing incident-event-log expectation.
  Documented cases are unchanged throughout (18 found, 11 missed,
  4 partial — reviewed documentation, not machine checks).

## Provenance

- Tree: main at ca275fdfe1e82ca35e5967136341e91aff2c866a, the
  base of branch `feat/315-corpus-targets`.
- Build: `maturin develop --release` from that tree, CPython
  3.12.3, onceover 0.1.5.
- Runs: `python benchmarks/corpus/run_benchmarks.py` (before,
  designated, and final configurations), 2026-11-15. The two
  test-only data files were present locally and verified against
  their registry sha256 pins before profiling.
- Diagnostics: per-entry profiles at the standard cap with
  findings attributed by `evidence[].method.detector`, and at
  `max_findings=1000` for the displacement checks.
