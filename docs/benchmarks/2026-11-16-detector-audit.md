# Per-detector audit: which default detectors state anything the gate can check

Issue #317 phase 3 calls for a per-detector audit across the gate
tables: a default detector with no matched reference statement and
outside the integrity checks moves to opt-in; still unmatched at
the full table set, it is deleted. This record is that audit. It
was measured on main at 064132d8, and it carries the membership
changes the audit justifies: `decision_tables`, `interactions`,
and `hotspots` move from the default set to on-request.

The audit measures the library as it stands on main. Draft PRs
#338, #339, and #340 (phase 3 families 1–3) are unmerged and are
not reflected here. All six detectors those PRs gate —
`order_constraints`, `breaks`, `ordered_partitions`,
`ordered_axis`, `discontinuities`, `outliers` — matched at least
one reference statement in this audit, so the audit moves none of
them and no interaction with those PRs arises.

## Method

- Tables: all 18 gate datasets (13 gated, 5 held out), each in
  default mode and, where the corpus designates a target, in
  target mode. Matching uses the full reports, not the digest.
- Matching is the gate's own: `finding_fits` in
  `benchmarks/gate/components/reference.py` — the finding's
  subject columns must cover the statement's `columns` (and
  intersect `columns_any` when given), with the statement's type
  and kind filters applied — over each profile's findings and
  grouped findings.
- Attribution: every finding's evidence carries
  `method.detector`. A statement credits each detector whose
  evidence appears on a fitting finding. `rule_evaluator` is a
  holdout-evaluation method recorded on rule findings, not a
  detector; findings carrying it belong to `association_rules`
  or `covering_rules`.
- A detector matches if at least one of its findings, on at
  least one gate table in at least one mode, matches a reference
  statement. Matched counts below are distinct (dataset,
  statement, mode) credits.
- Finding counts are findings carrying the detector's evidence,
  summed over the 18 datasets, per mode. A merged statement
  counts once for each detector that contributed evidence to it.
- Cross-check: this collection's per-statement matching was
  compared against the gate scorecard's own reference component
  on the same profiles — 0 mismatches.

## The instrument's limits

The reference statements are provisional, and they cover only
part of what the library states. That is the known weakness of
this audit, stated plainly:

- 39 statements cover 7 of the 18 gate datasets. The other 11
  datasets carry no reference file, so a detector whose
  statements land mainly on those tables is invisible here.
- No statement uses the `concentration` kind, and the `relates`
  kind excludes the concentration category, so a `hotspots`
  finding could only ever match the two kind-less statements
  (both on sec-edgar-company-facts), whose columns it does not
  cover. The audit structurally cannot credit the hotspots
  statement family.
- The `missingness` kind requires the missingness category. The
  state-unemployment panel's missing-month statement (tier
  `must`) is stated in substance by `panel_coverage` — the
  2025-10 period totals 0 against a typical 274.7 — as a
  `partial_period` finding in the gaps category, and earns no
  credit.
- Two statements matched on claim text alone (bank-marketing
  `subscription-housing-loan` in default mode, gse15258
  `disease-constant` in both modes) credit no detector.

A move to opt-in is reversible and keeps the detector one
explicit name away. That is why the plan accepts the provisional
statements as sufficient evidence for a move but not for a
deletion. The deletion rule applies at the full table set, which
the gate is not; nothing is deleted.

## Classification of the 32 default detectors

19 matched. 8 are unmatched but are integrity checks. 3 are
unmatched and not integrity checks, and move to on-request. 2
cannot be measured by the gate at all.

| Detector | Class | Matched credits | Findings (default / target) | Action |
|---|---|---:|---:|---|
| predictability | matched | 16 | 103 / 57 | stays |
| level_contrasts | matched | 12 | 216 / 164 | stays |
| association_rules | matched | 9 | 140 / 91 | stays |
| ordered_axis | matched | 8 | 156 / 89 | stays |
| functional_dependencies | matched | 6 | 94 / 45 | stays |
| keys | matched | 6 | 11 / 6 | stays |
| order_constraints | matched | 6 | 65 / 18 | stays |
| covering_rules | matched | 4 | 19 / 11 | stays |
| missingness | matched | 4 | 39 / 16 | stays |
| boundary_mass | matched | 2 | 16 / 16 | stays |
| breaks | matched | 2 | 85 / 47 | stays |
| discontinuities | matched | 2 | 53 / 28 | stays |
| duplicate_encodings | matched | 2 | 15 / 6 | stays |
| identities | matched | 2 | 3 / 1 | stays |
| numeric_dependence | matched | 2 | 80 / 45 | stays |
| ordered_partitions | matched | 2 | 22 / 10 | stays |
| outliers | matched | 2 | 119 / 90 | stays |
| regimes | matched | 2 | 9 / 6 | stays |
| sentinels | matched | 2 | 4 / 4 | stays |
| column_profile | integrity | 0 | 67 / 52 | stays |
| content_anomalies | integrity | 0 | 24 / 17 | stays |
| digit_fingerprints | integrity | 0 | 18 / 14 | stays |
| panel_coverage | integrity | 0 | 16 / 16 | stays |
| row_consistency | integrity | 0 | 26 / 14 | stays |
| row_duplicates | integrity | 0 | 5 / 4 | stays |
| series | integrity | 0 | 35 / 7 | stays |
| sortedness | integrity | 0 | 7 / 5 | stays |
| decision_tables | unmatched | 0 | 20 / 12 | moves to on-request |
| hotspots | unmatched | 0 | 41 / 38 | moves to on-request |
| interactions | unmatched | 0 | 1 / 0 | moves to on-request |
| expectations | not measurable | 0 | 0 / 0 | stays |
| group_contrast | not measurable | 0 | 0 / 0 | stays |

## Why the unmatched integrity passes stay

The reference statements enumerate relations and events in the
data. The integrity passes state the frame checklist — what is
wrong with the file as delivered — which the statements were not
written to enumerate. Per detector:

- `column_profile`: column summaries, role suggestions,
  per-column missing rates, value concentration, string formats.
  This is the frame account itself.
- `content_anomalies`: what text columns hold — label variants,
  stray signs, far-out dates, mixed formats. The formats family
  of the checklist.
- `digit_fingerprints`: heaping, terminal-digit preference,
  Benford deviation — statements about how values were
  recorded. The formats family.
- `row_duplicates`: exact duplicate rows. The duplicates
  family.
- `row_consistency`: near-duplicate rows (rows repeating on all
  but one column, stated with chance baselines) and rows far
  from a tight two-column relation or carrying swapped values.
  The duplicates family plus row-level error checks; its
  statements name erroneous rows, not relations among columns.
- `panel_coverage`: per-period volume against the typical
  period — row counts, entity counts, measure totals. The panel
  form of the completeness check. It states the panel's missing
  month in substance, in a form the matching cannot credit (see
  the limits above).
- `series`: its gap findings are the time-axis completeness
  check — on metro-interstate-traffic it states that the axis
  is missing 11,976 of 52,551 hourly steps (22.79%), the
  longest gap 7,386 steps — and its panel scan states grouping
  levels that appear, end, or shift share over time. Its
  `series_structure` statements (trend, cycle, persistence) are
  descriptive rather than checks, and its best-known one,
  metro's daily cycle at autocorrelation 0.71, is invisible
  here because metro carries no reference file. The detector is
  classed integrity on its completeness statements; the
  descriptive part is the first thing a fuller statement set
  should test.
- `sortedness`: statements of the form "column X is
  non-decreasing with row order" — a property of the file as
  delivered, part of the frame account alongside duplicates
  (how the rows sit in the file), not a relation among the
  data's variables. No reference statement enumerates
  file-order facts.

`keys`, `missingness`, `sentinels`, and `boundary_mass` are
integrity-family passes too (key/grain checks, missingness and
its structure, sentinel values, values piled at a boundary);
they are classed matched because statements credit them
directly.

## Why the two hint-triggered passes are not moved

`expectations` and `group_contrast` produced no findings on any
gate table. Both engage only on declared hints — `expectations`
on `order_by` / `no_gaps` / `cadence` / `hierarchies`,
`group_contrast` on `contrast` — and no gate dataset declares
any of those hints (only nab-nyc-taxi declares a hint at all,
`time`). The gate holds no evidence about either pass, in
either direction. Moving one to on-request would break the
hints contract: a caller who declares a sequence expectation or
a contrast expects the check to run without also naming the
detector. They stay in the default set, recorded here as not
measurable by the gate rather than as matched or as failed.

## The three moves

- `decision_tables` — 32 findings across the gate tables, 0
  matched. It states compact predictive tables for a target.
  The gate's `relates` statements could have matched its
  findings — `predictability`'s findings earned 16 credits on
  the same statement set, over the same datasets — and none of
  its findings matched any statement. It is not an integrity
  check. It requires `predictability`, which stays in the
  default set, so naming `decision_tables` runs it exactly as
  before.
- `interactions` — 1 finding across all 18 gate tables and both
  modes, 0 matched. It states non-additive predictor pairs
  under a fitted target model; not an integrity check. The
  near-zero production is itself the observation: in 30 gate
  profiles it stated one finding.
- `hotspots` — 79 findings, 0 matched. The instrument limit
  above applies in full: the statement set contains no
  concentration statement this detector could match. The move
  follows #317's rule as written, on the evidence standard the
  plan accepts for moves. It is reversible, and phase 5's rule
  is the path back: a gate statement that this detector newly
  states returns it to the default set. Issues #329 and #330,
  which concern this detector's coverage and reporting bars,
  are unaffected by the move. It is not an integrity check: its
  statements are relations among columns (cells against their
  marginals), not frame checks.

## Also observed, outside the default set

No action; recorded for completeness.

- `model_failures` (on-request, target-triggered): 99 findings,
  all in target mode, and 12 matched credits — the
  most-credited target pass. Already on request; phase 3's
  model-failures item needs no change.
- `target_shapes` (on-request, target-triggered): 2 findings,
  1 matched credit.
- `leakage` (on-request, target-triggered): 0 findings on every
  gate table. Silence at its stated bar on these tables is the
  correct behavior.

## Statements no detector matched

- state-unemployment-long: `missing-month` (must; stated in
  substance by `panel_coverage`, uncredited — see the limits),
  `state-spread` (should), `state-comovement` (should).
- sec-edgar-company-facts: `filing-key-not-unique` (should; a
  negative uniqueness statement — `keys` states positive
  uniqueness), `filed-after-period-end` (should).
- gse15258-expression: `geo-restates-sample`,
  `das28-improvement`, `das28-na`, `probe-1556053-response`
  (all must). The wide table's statements are stated nowhere in
  the full report — a detection limit on a 5,006-column frame,
  not one detector's absence.
- bike-sharing-hourly: `cnt-year` (should) matched in target
  mode only, via `model_failures`; no default-mode finding
  states it.

## Consequences carried through

- Registry membership for the three detectors; the registry
  counts move to 29 in the default set and 14 on request, of 43
  registered. Triggers are unchanged (the three are
  always-triggered).
- One dispatch consequence in `pipeline.rs`: the dependence
  screen stamped a `decision_tables` coverage entry (an
  applicability verdict) whether or not the pass was selected,
  which pre-empted the standard on-request skip entry the
  closing coverage loop writes for non-selected detectors. The
  stamp is now guarded on the pass being enabled, so a default
  run records `decision_tables` as skipped, naming it
  on-request, as the documentation states. `interactions` and
  `hotspots` already recorded the standard skip.
- `docs/capability-manifest.json` regenerated from the
  installed build.
- The default-set counts and on-request lists updated in the
  `profile()` docstring, the packaged agent guide, the root
  `AGENTS.md`, `docs/findings.md`, and the book's `api`,
  `capabilities`, and `coverage` pages; the `hotspots`
  methodology page now names the detector in its example and
  states that it is on request, and the findings page's
  methods-and-scores example names `decision_tables` explicitly
  (its subject is the merge of the two detectors' evidence into
  one statement). Book notebooks whose stored outputs changed
  were re-executed.
- Tests: the registry tests in `crates/onceover/src/detectors.rs`
  pin the new counts and the on-request list.
  `tests/test_detector_defaults.py` lists the three in its
  on-request constant (which also pins that a default run
  discloses each as skipped) and counts 27 always-triggered
  default detectors. `tests/test_hotspots.py` and one test in
  `tests/test_taxonomy.py` now name the detectors they exercise
  (the taxonomy test's subject is the merge of `predictability`
  and `decision_tables` evidence into one statement). The Rust
  integration test for deadline dispatch asserts the hotspots
  exemption on a run that names it; in a default run the pass
  now records the on-request skip. One test in
  `tests/test_report_access.py` names `interactions`: its
  fixture's only dependencies finding is the region × year
  interaction statement. The example notebook's shared selection
  (`notebooks/onceover_example.py`) names `decision_tables`
  alongside `date_gaps`, because the notebook's summary reads
  its churn statement — the same repair the 2026-11-02 battery
  required for `date_gaps`, pinned by
  `tests/test_notebook_contract.py`; the notebook
  (`notebooks/onceover_analysis.ipynb`) was re-executed.
- Corpus: 173/173 machine-verified expectations pass, unchanged
  from main. A scan of the corpus registry confirms no
  expectation names or depends on the three passes, so no
  expectation was re-baselined.

## Gate before and after

Same harness (`benchmarks/gate/run.py`, seed 0), main tree
against branch tree. Gated totals:

| Metric | Before | After |
|---|---:|---:|
| must at 4k, default | 10/12 | 10/12 |
| must at 4k, target | 11/12 | 11/12 |
| must at 8k and full, both modes | 11/12 | 11/12 |
| should at 4k, default | 6/16 | 7/16 |
| should at 4k, target | 10/16 | 10/16 |
| should at 8k, default | 12/16 | 13/16 |
| should at 8k and full, target | 14/16 | 14/16 |
| should full, default | 13/16 | 13/16 |
| check_failed | 0 | 0 |
| planted found at 4k | 29/46 | 29/46 |
| planted found full | 33/46 | 34/46 |
| permuted false statements at 4k (default / target) | 78 / 44 | 78 / 44 |
| permuted false statements full (default / target) | 144 / 79 | 144 / 79 |
| repeated facts | 0 | 0 |
| invariance total | 430 | 438 |
| truth ok / mismatch | 836 / 0 | 876 / 0 |
| findings (cost) | 1089 | 1104 |

Held-out totals: must 3/7 in every scope and mode, unchanged;
should at 4k 2/4, unchanged; should at 8k and full in default
mode 2/4 → 3/4; planted 6 at 4k unchanged and 7 → 8 full;
permuted false statements unchanged (19 at 4k, 33 full);
invariance 182 → 183; truth ok 265 → 267, mismatch 0.

No must or should number fell in any scope or mode, and planted
did not fall. The rises have one mechanism: the moved passes'
findings no longer take selection room, so statements from
staying passes are admitted that the cap previously displaced.
Named: bike-sharing `cnt-temperature` (should) now matches at
4k in default mode; bank-marketing `subscription-housing-loan`
(should) now matches at 8k in default mode;
sec-edgar-company-facts `filed-after-period-end` (should, held
out) is now stated in the full default report — a statement no
finding stated before; the absenteeism sentinel plant and the
sec-edgar target-driver plant enter the full report.
Invariance rose (reverse 279 → 286, row shuffle 105 → 109,
noise 38 → 35) by the same redistribution: the findings
admitted in the freed room are less stable under reversal and
shuffle than the ones they replaced. Findings rose slightly on
the gated tables (1089 → 1104) and fell on the held-out tables
(357 → 350): most of the moved passes' gate findings were
merged into statements also carried by staying passes
(`decision_tables`' predictive statements merge with
`predictability`'s), so removing the passes removes few
statements, and the freed room admits others.

The gate was re-run on the final tree (after the `pipeline.rs`
coverage guard described below): every metric is identical to
the table above except report JSON size (+338 chars gated,
+104 held out — the longer on-request skip detail now recorded
for `decision_tables`) and profile seconds, which is timing on
a shared box.

## What remains of phase 3

The default `max_findings` reduction. Families 1–3 are draft
PRs #338, #339, and #340; this audit is family 4; the
model-failures item needed no change (`model_failures` is
already on request, and the audit records it as the
most-credited target pass).
