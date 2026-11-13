# Report selection under cap pressure

Date: 2026-11-13. Change: report selection (issue #271). Base: main at
91b6053b (version 0.1.5). This record holds the before/after
measurement for the selection change and the outcome of each bar in
the frozen evaluation protocol. The design and the protocol were
fixed before implementation; the protocol's bars are scored as
written, including where they fail.

## What changed

Selection gains a statement class per finding, computed from its
facets and (category, type) by a fixed rule: account (frame-scope
and uniqueness), structure (relations and conditional statements),
shape (changes, plus intervals/extreme_value), hygiene (everything
else). The class is a selection-time property; it is not a report
field.

Under cap pressure only — distinct statements after collapse exceed
`max_findings` — admission becomes class-major: account statements
first, then structure and shape in score order, then hygiene. Three
rationing caps bind under pressure:

- a group cap of 12 selected statements per (category, type, form);
- per-detector caps extended to `breaks` (25) and `views` (25);
- a hygiene reserve: at most 20 hygiene statements in a report of
  100. If structure and shape candidates are exhausted first,
  hygiene still stops at the reserve and the report comes out
  shorter.

The selection summary gains per-class candidate, selected, and
turned-away counts. The coverage selection entry discloses the
group-cap counts both by group label and by the detector that
produced the statements, the reserve count, and the standing
per-detector cap counts. The digest prints structure groups first,
then shape, then hygiene, with a round-robin across the two insight
classes when the character budget binds. Without cap pressure,
admission and membership are exactly as before.

## Protocol

Fixed set (standard budget, no hints): Airbnb listings (29,918
rows), Appliances Energy typed (19,735), Appliances Energy raw (the
uncapped control), Adult (48,842), Bike Sharing (17,379), Taiwanese
Bankruptcy (6,819), and the state-unemployment panel (609 rows, 51
state series). The before-run profiles the set with the unmodified
library; the after-run with the change. A frozen classifier (the
design study's rule, fixed before counting) assigns each reported
finding to account / relation / conditional / shape_change /
hygiene; insight = relation + conditional + shape_change. Digest
measures use the 4,000-character digest.

## Results

Before -> after, per report:

| report | digest insight share | hygiene share of selected | max (category,type,form) group | max detector | score-top-25 insight | selected |
|---|---|---|---|---|---|---|
| airbnb | 0.43 -> 0.67 | 0.25 -> 0.00 | 25 -> 12 | 25 -> 23 | 24 -> 24 | 100 -> 100 |
| appliances-typed | 0.82 -> 0.82 | 0.03 -> 0.00 | 25 -> 12 | 25 -> 25 | 24 -> 24 | 100 -> 100 |
| appliances-raw (control) | 0.67 -> 0.67 | 0.20 -> 0.20 | 26 -> 26 | 42 -> 42 | 25 -> 25 | 81 -> 81 |
| adult | 0.50 -> 1.00 | 0.37 -> 0.25 | 13 -> 12 | 29 -> 28 | 11 -> 11 | 99 -> 80 |
| bike-sharing | 0.67 -> 0.89 | 0.08 -> 0.00 | 40 -> 12 | 53 -> 25 | 18 -> 23 | 100 -> 100 |
| taiwanese-bankruptcy | 0.67 -> 0.89 | 0.08 -> 0.09 | 25 -> 12 | 25 -> 21 | 25 -> 25 | 100 -> 100 |
| panel | 0.82 -> 0.75 | 0.05 -> 0.00 | 25 -> 12 | 25 -> 25 | 24 -> 24 | 100 -> 100 |

Candidate class mix after the change (candidates offered /
selected / turned away, from the selection summaries; bar B7):

| report | candidates | account | structure | shape | hygiene |
|---|---|---|---|---|---|
| airbnb | 752 | 2 / 2 / 0 | 323 / 58 / 265 | 193 / 40 / 153 | 169 / 0 / 169 |
| appliances-typed | 371 | 2 / 2 / 0 | 108 / 49 / 59 | 232 / 49 / 183 | 28 / 0 / 28 |
| appliances-raw | 81 | — | — | — | — |
| adult | 112 | 0 / 0 / 0 | 66 / 60 / 6 | 0 / 0 / 0 | 37 / 20 / 17 |
| bike-sharing | 188 | 2 / 2 / 0 | 70 / 51 / 19 | 78 / 47 / 31 | 35 / 0 / 35 |
| taiwanese-bankruptcy | 246 | 5 / 5 / 0 | 128 / 62 / 66 | 65 / 24 / 41 | 41 / 9 / 32 |
| panel | 407 | 2 / 2 / 0 | 123 / 50 / 73 | 275 / 48 / 227 | 7 / 0 / 7 |

(Turned-away counts include statements grouped under family leads;
the control report is under no pressure and records no class mix.)

## Bar outcomes (frozen protocol, section 5 of the design study)

- **B1, digest value — failed.** Airbnb digest insight share rose
  0.43 -> 0.67 against a bar of 0.75. Adult met the bar
  (0.50 -> 1.00). The panel's share fell 0.82 -> 0.75, against the
  bar's condition that no other report falls. Bike Sharing and
  Taiwanese Bankruptcy rose to 0.89; Appliances-typed and the
  control were unchanged.
- **B2, digest order — passed.** In every digest, no hygiene group
  line appears before an insight group line.
- **B3, no-pressure invariance — passed.** The control report's
  selected membership is identical before and after: the same 81
  finding ids.
- **B4, hygiene reserve — failed on Adult.** Hygiene is 25% of
  Adult's selected findings against a cap of 20%. The mechanism is
  arithmetic: Adult's structure candidates run out under the group
  cap, the report shortens to 80 selected, and the fixed reserve of
  20 becomes a quarter of it. Every other capped report is at or
  below 9%.
- **B5, monoculture — passed for groups; one detector above the
  stated line.** No (category, type, form) group exceeds 12 on any
  capped report (transient_excursion 40 -> 12; step_discontinuity
  groups bounded on the three reports where they reached 25). The
  breaks detector falls 53 -> 25 as the bar states. On Adult,
  `association_rules` selects 28 (from 29) under its own
  pre-existing per-detector cap, above the bar's 25 read literally;
  the bar's parenthetical named only the breaks flood.
- **B6, no class loss and better heads — failed in both halves.**
  The hygiene class is absent from the selected set on four capped
  reports (Airbnb, appliances-typed, bike-sharing, panel) because
  the reserve is a ceiling, not a floor: where insight classes fill
  the cap, no hygiene statement is admitted. Adult's score-ordered
  top-25 insight share is unchanged at 44% (11 of 25) against a bar
  of 70%: the change rations admission and does not recalibrate
  scores, so the score head does not move. By admission order,
  Adult's first 25 selected statements are all insight class. No
  other report's score-top-25 share dropped by more than 5 points
  (bike-sharing rose 18 -> 23; the rest are unchanged).
- **B7, candidate accounting — passed.** The candidate class mix is
  tabled above from the selection summaries.
- **B8, cost and gates — passed with two stated limits.** Report
  JSON size stayed within +5% on every report (airbnb +0.9%,
  appliances-typed +4.3%, adult -14.9%, bike-sharing +3.5%,
  taiwanese-bankruptcy -3.8%, panel -1.1%, control unchanged).
  Wall time is not certified: single-run profile times on the
  measuring machine vary by more than the bar (the same unmodified
  code measured 52.7 s and 6.6 s for appliances-typed in two
  before-runs; the after-run measured 119.4 s under load), so the
  +10% figure can neither be confirmed nor refuted from these runs.
  Gates: release check passed; cargo fmt and clippy clean; cargo
  test 401 passed, 0 failed; pytest 615 passed, 1 skipped; the
  strict book build passed; the corpus contract passed after the
  expectation updates listed below (93 of 93 machine-verified
  expectations; runner exit code 2, the standing convention).

## Changed expectations

Each change below is explained by the selection change that caused
it, and is recorded where the expectation lives.

Corpus registry (`benchmarks/corpus/registry.json`), re-baselined
2026-11-13 with the mechanism in each note:

- soybean, two `intervals/categorical_outlier` findings (GritBot
  cases 558 and 614): found -> missed at the default cap. The
  statements are still generated and are selected at
  max_findings=300; the (intervals, categorical_outlier) group cap
  of 12 displaces them (7 turned away in that group).
- absenteeism-at-work, `employee_id -> education` and
  `employee_id -> social_drinker`: found -> missed at the default
  cap. Still generated by the functional pass; the
  (dependencies, functional, value_mapping) group cap of 12
  displaces them (199 turned away in that group; the 12 admitted
  are two-input dependencies).
- bike-sharing-hourly, `intervals/row_inconsistency` and
  `concentration/value_concentration` on holiday: found -> missed
  at the default cap. Both are still generated and are selected at
  max_findings=300; at the default cap the report's 100 slots are
  filled before hygiene-class statements are admitted (35 hygiene
  candidates turned away).
- bike-sharing-hourly, `(cnt, casual) -> registered` functional
  dependency: missed -> found. The statement is now retained as a
  grouped member under the verified relationship family led by the
  `(casual, registered) -> cnt` direction, instead of being
  displaced from the report entirely.
- state-unemployment, `changes/ordered_partition` minimum count:
  20 -> 12. The group cap of 12 bounds per-series partition
  statements in the capped report (13 turned away, 12 selected);
  the pass-level expectation (trees for at least 30 of 51 series)
  is unchanged and still pinned in tests/test_partitions_panel.py.

Worked example (`notebooks/onceover_analysis.ipynb` and
`tests/test_notebook_contract.py`): the example now profiles its
fixture with max_findings=200. At the default cap the fixture's
report is under selection pressure (103 distinct statements) and
the hygiene reserve turns away 12 statements, among them the phone
formats statement and the order_date gap statement that the
notebook's summary and date section read. The notebook states the
raised cap and the reason at the profile call.

## Follow-up

The three bar failures share one mechanism — the hygiene reserve
acts as a ceiling but not a floor, and as a fixed count rather than
a share — plus one open question about the score-ordered head
under an admission-rationing design. They are filed as a separate
issue for a design decision; nothing in this change adjusts the
reserve to make a bar pass after the fact.

## Provenance

Before-run: the unmodified library at main 91b6053b, built and
installed in a clean environment. After-run: this change, same
environment. Datasets are the in-repo corpus copies and the two
local demonstration files named in the design study. Measuring
machine: a shared sandbox (6 GiB), which is why wall time is
reported as not certified rather than estimated.
