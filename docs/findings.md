# Finding schema 0.2

A finding describes a statement about the data. Detectors supply evidence for the
statement. Detector names do not determine the public taxonomy.

Schema 0.2 replaces schema 0.1 and is used by package version 0.1.1.

## Categories and statement definitions

| Category | Type or required definition | Meaning |
|---|---|---|
| `missingness` | Missing counts and scope in evidence | Missing observations within recorded rows; also sentinel values standing in for missing data — one implausible value repeated in a narrow row window (the `sentinels` detector; value, count, window, and the column mean with and without it in the evidence) |
| `concentration` | Distinct counts, dominant shares, concentration measures | Concentration of a value distribution, including constants; also hotspots — cells of a 2–3 column combination holding far more rows than the marginals predict under independence (the `hotspots` detector; observed vs expected rows, ratio, and the multiplicity bar in the evidence) |
| `column_roles` | `specification.definition.role` | Suggested interpretation of a column, supported by observations |
| `column_roles` | Type `long_format`; `specification.definition` names `index`, `attribute`, and `measure` | The frame is in long format; pivoting on the attribute gives one column per level. At standard and deep budgets the wide form is profiled too and its findings are merged into the same report, tagged `source_form` in their evidence (coverage entry `pivot_profile`) |
| `uniqueness` | Compared columns and equality criterion | Single-column or composite uniqueness, with any recorded repetitions |
| `duplicates` | Subject unit, compared columns, comparison criterion, missing-value policy | Repetition under a stated comparison |
| `formats` | Format counts and examples | Inconsistent representations within a field; also granularity changes in float columns — the integer-valued share switching regime at a seam row (exact split scan; `change_row` and both shares in the evidence), from the granularity detector |
| `dependencies` | Type `functional` | Inputs determine a target, subject to scope, comparison, tolerance and exceptions |
| `dependencies` | Type `predictive` | Specified inputs support prediction of a target under recorded validation |
| `conditional_rules` | Type `if_then`; structured conditions and outcome | A conditional outcome, qualified by observed frequency and baseline |
| `associations` | Type `statistical` | Statistical association without claiming a particular mapping |
| `associations` | Type `period_association`; period column, period labels, statistic | A numeric association whose strength changes between periods |
| `gaps` | Domain, reference, interval bounds or absent values | Absences relative to an expected sequence or calendar |
| `intervals` | Observed interval statistics | Irregular spacing |
| `intervals` | Type `extreme_value`; extreme values and their row indices, the bulk edge and the isolation gap in the evidence | A value isolated from the bulk of its column, or from the bulk within one categorical subset, from the surprises detector; the claim states the gap as a multiple of the subset's central spread (p90-p10) |
| `ordering` | Direction, strictness and missing-value policy | Order of values within the existing row sequence; also the backfill shape — a column ordered except for a late block whose values return to earlier territory (at most 3 descents, no post-seam value above the pre-seam peak), reported with the seam row and tail length in the evidence |
| `changes` | Type `residual_level`; reference model and compared groups. Types `level_shift`, `trend_change`, `variance_change`; breakpoint in the statement definition. Type `transient_excursion`; event start, end and peak in the statement definition | Dated residual shifts; level, trend, and variance breaks in a series itself along a date, datetime, or integer-year axis from the breaks detector; short excursions from a local baseline (seasonal or rolling-median) from the same detector; type `step_discontinuity` with form `step` for a cross-sectional level jump of a target column across an axis threshold or a diagonal boundary in two columns, from the discontinuities detector; type `distribution_shift` with form `drift` for drift of a target's predictors along a declared time axis, from the drift detector; or cluster differences from the opt-in views pass; inspect the statement definition |

The mathematical form is separate from dependency type. Current forms include
`value_mapping`, `affine`, `piecewise_linear`, `period_split`, `pivot`, `isolation_gap`, `excursion`, and `interaction_h`. A decision-table algorithm can
`value_mapping`, `affine`, `piecewise_linear`, `period_split`, `pivot`, `isolation_gap`, `excursion`, `step_shape`, and `curved_shape`. A decision-table algorithm can
support a predictive dependency without establishing an exact functional
constraint. A high predictive score never upgrades a statement to an exact
constraint.

When the pivot pass verifies a long-format frame, the relationships
between the attribute's levels exist only in the pivoted wide form, so
at the standard and deep budgets the pipeline profiles that form as
well (one row per index tuple, one column per level) and merges its
findings into the same report. A wide-form finding is recognizable by
the `source_form: "pivoted wide form"` metric in its evidence; its
columns are the wide form's columns (the index columns and the level
names), which need not exist in the profiled frame. The coverage entry
`pivot_profile` states the wide form's shape and how many findings it
contributed, or why it was not profiled: the quick budget skips it,
wide forms over 64 columns or under 8 index tuples are out of bounds,
and a mostly-spent deadline skips it. The wide form is profiled with
the same configuration minus any designated target, and it is never
itself re-pivoted.

A predictive statement with form `interaction_h` names two input columns and
one target. It states that the pair's joint effect on the target, under the
predictability screen's GBM surrogate, contains an interaction component:
the part of the joint partial dependence that the sum of the two single
partial dependences does not explain. The evidence metrics are `h2` (the
interaction share of the joint partial-dependence variance),
`interaction_variance` (the interaction component's variance, in target
units squared), `interaction_se` and `interaction_z` (its standard error and
separation from zero), `joint_variance`, `first_variance` and
`second_variance` (the partial-dependence variances), `target_variance`,
`interaction_share_of_target`, and `holdout_r2` next to
`additive_holdout_r2` — the holdout R² of a depth-1 refit of the same
target. A pair is reported only when the surrogate's holdout R² clears
0.10, exceeds the additive refit's by at least 0.02, and the pair clears
`h2` ≥ 0.05, `interaction_z` ≥ 4, and `interaction_variance` ≥ 1% of
`target_variance`. Numeric targets only; categorical targets are not
evaluated by this pass.
A predictive statement with form `step_shape` or `curved_shape` comes from
the opt-in target-aware mode: it exists only in a profile where the caller
designated a target column (`profile(df, target="y")`). It names one input
column and the designated target, and states the shape of the input's
relationship to the target. The evidence metrics are `linear_r2`,
`curve_r2` (binned means in up to 20 equal-count bins, adjusted for the
bins' degrees of freedom), `step_r2` (the best single split of the sorted
input), `step_share` (what the step explains beyond the line, relative to
what the curve adds), `curvature` (`curve_r2` − `linear_r2`), `pearson`,
`spearman`, and `n_used` (complete cases: rows missing the input or the
target are excluded). A `step_shape` statement adds `step_at`, the located
cut. A predictor is reported only when a shape is decisive: for a step,
`step_r2` ≥ 0.05, `step_r2` − `linear_r2` ≥ 0.05, and `step_share` ≥ 0.8;
for a curve, `curve_r2` ≥ 0.10 and `curvature` ≥ 0.05. The shares are of
the target's total variance, so a weak curved term under a dominant
second effect is not reported. At most 10 shape statements per target.

Gaps require a reference. A temporal gap states its assumed cadence or calendar;
an integer-sequence detector can use the same category with a declared increment.
How that reference was obtained belongs in evidence. Duplicate statements specify
whether rows, values, tuples, or columns were compared and which matching rule
was used. Missing values comparing equal is an explicit policy.

## Selection and ranking

Detectors assign each finding an `interestingness` score on their own
scale. Selection does not silently replace it. The report's
`interestingness` is the selection score — the mean of the detector's
score and a ledger evidence score computed from validation quality,
effect strength, specificity, and simplicity — and the detector's
original score is preserved in `detector_interestingness`. Findings
are ordered by the selection score, and the same order decides which
findings survive the `max_findings` cap.

Four adjustments keep exact-but-expected statements from outranking
substantive ones:

- A functional dependency whose inputs contain an exact unique key
  holds by construction. Its selection score is multiplied by 0.6,
  and its evidence carries `triviality: "implied_by_unique_key"` with
  `triviality_factor: 0.6`. Near-unique (dirty) keys imply nothing and
  do not trigger the demotion. The FD lattice also screens
  near-unique determinants out during discovery, so plain FDs rarely
  reach this rule; it binds the producers that do not screen, such as
  arithmetic identities over a key column.
- An exact functional dependency's detector score is
  `0.5 + 0.35 × breadth`, where breadth is the number of distinct
  determinant tuples (`lhs_distinct` in the evidence) divided by the
  verified rows. A mapping verified over 40 repeated inputs scores
  lower than the same mapping verified over 600 distinct inputs.
- Key candidates are scored from the `plausibility_score` their
  evidence already records (type affinity, name tokens,
  completeness): the primary-key candidate's detector score is
  `0.3 + 0.65 × plausibility`. An integer column named like an
  identifier scores 0.95; an all-distinct float measurement scores
  about 0.61, and its interpretation says to treat it as a
  distinct-count check rather than a join key.
- Producers may declare a demotion the ledger applies: when a
  producer knows its finding restates the file's own structure, it
  records `triviality` (a reason) and `triviality_factor` in its
  evidence metrics. The ledger multiplies every applicable factor —
  declared ones and its own key-implied rule — into the selection
  score and writes the consolidated reasons and factor back to
  every evidence record of the finding. Declared bars today, each
  with factor 0.6: `continuous_measure_key_candidate` (uniqueness
  and identifier-role findings on float columns — a measurement
  that happens to be all-distinct is not a join key),
  `capitalization_only_format_mix` (a format finding whose two
  dominant patterns differ only in letter case; shape or separator
  changes that block parsing are not demoted), and
  `long_format_encoding` (a functional dependency from a verified
  long-format frame's measure column to its attribute column,
  which holds because the levels occupy disjoint value ranges;
  the same reason covers the layout's predictive restatements —
  the attribute predicted from the pooled measure, or the pooled
  measure predicted with the attribute among its inputs).

Demotion is the only triviality action: findings are never dropped
for being trivial, only ranked lower, with the reason recorded in
their evidence.

## Public structure

Each finding contains:

- `id`: derived from the canonical statement, independent of rank and detector ID;
- `category` and `type` (every finding carries both; the type is part
  of the canonical statement the `id` is derived from);
- `subject`: `columns`, optional `inputs`, `target`, and comparison `unit`;
- `specification`: form, conditions, outcome, and the definitions needed to
  interpret a reference or comparison;
- `claim`: a readable description qualified by its measurements;
- `evidence`: separate records from contributing detectors and verifiers;
- `interestingness`: the selection score described above, not a probability;
- `detector_interestingness`: the score the producing detector assigned;
- optional `interpretation`.

Predicates carry a column, an operator (`eq`, `ne`, `lt`, `le`, `gt`, `ge`), and a
typed scalar value. Conjunctions are lists. Numeric strings remain strings.
Temporal predicates can explicitly compare a `day_index` transformation. Numeric
bin boundaries retain their actual values rather than rounded display strings.

Each evidence record has `method` (detector, algorithm, provenance), `basis`
(`measured`, `estimated`, `inferred`), `scope`, `validation`, and `metrics`.
Scope records full/sample/unspecified coverage, population and evaluated counts,
and missing-value handling. Validation identifies direct comparisons,
cross-validation, diagnostic holdout or diagnostic partition, and whether those
results were used during selection. These dimensions replace `confidence`.

An exact comparison on sampled rows is not a full-population guarantee. Inferred
roles remain inferences even when the supporting counts cover the full dataset.

Some finding types carry a bespoke evidence payload in `metrics`: a table
whose columns are specific to that type, so the claim can be inspected
without a follow-up query. Payloads are capped per type; the scalar
metrics always carry the full counts the payload samples. In Markdown
each payload renders as a table under its finding. The payloads defined
so far:

- functional dependencies (`functional_dependency`,
  `approximate_fd`): `mapping` — rows of `{key, value, rows}`. For an
  approximate dependency these are the keys mapping to more than one
  value, worst ambiguity first (up to 8 keys, 3 values per key), with
  `mapping_ambiguous_keys` and `mapping_keys` giving the totals; for
  an exact dependency, the 8 largest keys with their single value.
  Above 100,000 distinct keys no mapping is collected and
  `mapping_omitted` says why.
- level shifts (`level_shift`): `segments` — the full training
  segmentation the finding is drawn from, rows of `{segment, from,
  to, rows, mean}`.
- trend changes (`trend_change`): `segments` — the two fitted
  segments, rows of `{segment, from, to, rows, slope}`.
- variance changes (`variance_change`): `segments` — the two
  segments of the split, rows of `{segment, from, to, rows, mean,
  variance}`.
- duplicate rows (`duplicates`): `duplicate_groups` — the largest
  duplicate groups (up to 10), rows of `{values, rows, first_row}`,
  where `values` renders the group's first row over the compared
  columns (first 8, joined with ` | `).
- distribution shift (`distribution_shift`): `periods` — one row per
  period with its range, row counts, and drift scores.

New detectors define their payload shape with the finding type; there
is no shared envelope beyond the metrics map itself.

## Report outputs

The same ledger is available in four forms:

- **JSON** (`report.data`, `report.to_json()`): the complete record —
  findings with every evidence record and metric, column cards, groups,
  selection counts, and coverage. The contract is
  `docs/report-schema.json`, validated against live reports in the
  test suite. Findings are addressable without rendering the whole
  record: `report.finding(id)` returns one finding (or None), and
  `report.select(category=, type=, column=, target=)` returns the
  findings matching every given filter, in report order.
  `report.to_json(max_chars=N)` renders a size-budgeted view that
  degrades in a stated order — findings past the budget collapse to
  `finding_index` entries (id, category, type, columns,
  interestingness, claim), then column cards collapse to
  `column_names`, then coverage details truncate and finally drop
  to status only — and carries a `budget` record stating what was
  cut. The view never exceeds `max_chars`. Two profiles of a table
  compare directly: `report.diff(other)` matches findings by their
  stable `id` and returns the findings `other` added, the ones it
  removed, the ones whose claim or interestingness changed (with
  the old and new values), an unchanged count, and any per-pass
  coverage status changes. Evidence internals are not diffed — a
  finding whose evidence changed but whose claim and score held
  counts as unchanged. Digest rollups for multi-member groups list
  up to three exemplar finding ids so a rolled-up line can be
  expanded with `report.finding(id)`.
- **Markdown** (`report.to_markdown()`): the headline and overview,
  then each finding's claim followed directly by its evidence. An
  evidence block names the detector and algorithm, the validation
  method, and the rows evaluated, then lists the scalar metrics as
  `key=value` pairs (first 12; the JSON carries the rest). A metric
  whose value is a list of flat records — per-period coefficients, for
  example — renders as a table under the finding, capped at 20 rows.
  Piecewise-linear leaves keep their equation rendering and are not
  repeated as a table. Column cards and coverage close the document.
- **Digest** (`report.to_digest(chars)`): a context-budgeted
  plain-text summary for a single pass of reading — header (shape,
  headline, coverage exceptions, counts by category), then one line
  per rollup group (findings sharing category, type, form, and
  target) carrying the group's strongest claim, strongest first.
  When the groups exceed the budget, the digest keeps as many as
  fit and ends by stating how many were omitted; it never exceeds
  the budget. Rollups are an index into the ledger, not a
  replacement for it.
- **DataFrame** (`report.findings_frame()`): one row per finding with
  `id`, `category`, `type`, `columns`, `inputs`, `target`,
  `interestingness`, `claim`, `n_evidence`, and the primary evidence
  record's `basis`, in report order. It is an index for sorting and
  filtering; the measurements themselves stay in the JSON findings.

## Consolidation and selection

1. Enabled detectors produce candidate statements and evidence.
2. Conditional rules receive a common predicate evaluation on a deterministic
   diagnostic partition. Complete conditions and outcome define eligibility.
3. Canonically equivalent statements combine their evidence. Counts from
   overlapping samples or different methods are never pooled or averaged.
4. Shared selection considers validation, effect relative to baseline, support,
   complexity and explicit statement form. Limits apply across detectors.

Rule evidence uses explicit `eligible_rows`, `antecedent_count`, `outcome_count`,
`joint_count`, `joint_support`, `conditional_rate`, `baseline_rate`, and `lift`.
Discovery-specific quantities remain attached to the discovery method. In
particular, PRISM's score on uncovered positives is not relabeled as validation
precision. Rule descriptions use the common evaluator's counts.

The common partition is diagnostic: discovery may already have seen those rows,
and evaluation participates in selection. It is not an independent confirmation
sample and has no multiple-testing guarantee.

Selection defaults to at most 100 findings, 20 conditional rules, two rules per
outcome, and three predictive findings per target. Rule evaluation requires at
least 30 eligible rows, 30 antecedent matches and 20 joint matches; the conditional
rate must exceed baseline and have at least 0.02 absolute improvement or lift
1.5. Rust `SelectionPolicy` configures these limits. Python exposes
`max_findings` and `max_rules`.

Different directions, inputs, conditions, references or expressions remain
separate statements. Correlation and prediction evidence are not interchangeable.
All predictors actually used by a fitted model are recorded; top feature
attributions are separate explanatory metadata.

`report.groups` is a connected view over selected statements. Group membership
asserts neither interchangeability nor redundancy. `report.selection` records
candidate, verified-candidate, distinct-statement and selected counts.

## Declared expectations

Callers can declare structure the profiler cannot infer, through the
`hints` argument (Rust: `ProfileConfig.hints`):

- `time`: the time/ordering axis column;
- `order_by`: columns the rows are claimed to be ordered by;
- `no_gaps`: columns expected to form a complete sequence;
- `cadence`: per-column cadence — `daily`, `business_day`, `weekly`,
  `monthly`, or a fixed day/step count;
- `dimensions`, `measures`: the panel's declared dimension and measure
  columns.

The `expectations` pass checks the declarations exactly and tags its
evidence `reference_source: "declared"`, against the dates pass's
`"inferred"`. Four checks run:

- **Declared-cadence gaps** on temporal columns, against the declared
  cadence (monthly included) with no minimum-series floor. A declared
  column is skipped by the inferred dates analysis, so a gap is
  reported once. A temporal `no_gaps` column without a declared
  cadence is enumerated at its modal interval.
- **Sequence completeness** on integer `no_gaps` columns, at the
  declared step or the modal step; the evidence lists the first 20
  absent values. Ranges wider than 1,000,000 expected values are
  skipped and noted in coverage.
- **Ordering**, in both directions: inversions against `order_by` are
  counted and the first offending rows located; a satisfied
  declaration yields the positive ordering statement.
- **Panel completeness**: the strict grid of declared dimension
  combinations × axis periods (declared-cadence enumeration, else the
  union of observed periods). Missing cells are counted with the
  worst groups and up to 10 absent cells in the evidence; when
  measures are declared, present cells null on every measure are
  counted separately as `measure_null_cells`.

Hints naming columns that are missing, or columns of an unusable type
for the check, are ignored with the reason recorded in the
`expectations` coverage entry; malformed hint values raise an error
at the API boundary. Declarations drive the expectations pass only —
column cards keep their inferred roles.

## Detectors and extensions

```python
import onceover

onceover.available_detectors()
report = onceover.profile(df, detectors=["prism", "association_rules"], max_rules=10)
```

Each registry entry lists its ID, produced categories, and resource dependencies.
Required detectors may execute to supply shared target descriptions without
contributing their own findings. An empty selection still returns column
summaries and coverage but no detector findings.

The `discontinuities` detector (standard and deep budgets) scans for
cross-sectional step jumps: for each numeric target column, step
indicators along every other numeric column's axis and across diagonal
boundaries in column pairs, each fitted against a smooth background by
OLS. Rows are split into a fit half that ranks candidates and an
evaluation half that estimates the jump, so the reported `z` is not
inflated by the search. A flagged candidate is reported only when its
local level gap is scale-invariant (the gap between the nearest 60
evaluation points per side vs the nearest 15 stays near 1, with the
small-scale gap significant); a steep smooth transition, whose gap
grows with scale, is not reported as a jump. Because the screen runs
for every numeric target, reporting applies stricter bars than
flagging: |z| >= 4 on the evaluation half and a held-out gain of at
least 0.5% of the target's evaluation-half variance. Diagonal
directions within one direction step of an axis are not scanned (the
axis scan covers them); at most one finding is reported per target
and axis set; and when an axis step and a near-parallel diagonal step
(within 25 degrees of the axis) type as the same jump at comparable
strength, the axis step is reported unless the diagonal's |z| is at
least 1.5x the axis step's. Evidence carries `kind`
(`axis` or `diagonal`), `threshold` (original units for axis findings,
standardized for diagonal, with `direction_deg`), `jump`, `se`, `z`,
`t_fit`, held-out `gain`, `type_ratio`, `type_gap_t`, and `type_z`.

The `drift` detector (standard and deep budgets) compares periods of a
declared time axis. It engages only when the caller passes
`hints={"time": "<column>"}`; without a declared axis it records coverage
and emits nothing. The time column's rows are cut into contiguous periods
of roughly equal row counts (at most 8 at standard, 12 at deep; a period
needs at least 100 rows). For each numeric target column other than the
time column itself, the screen's gradient-boosting model is fitted on the
earliest period's rows, with the time column withheld from the predictor
set — a model that splits on the time axis routes every later period to
one side by construction, which would read as maximal drift on stable
data. The reference model must reach a holdout R² of at least 0.2 for its
splits to be used. Each later period is then scored by routing its rows
and an equal number of reference-period rows through the fitted trees:
at every split, the two periods' left/right routing counts form a 2×2
table whose chi-square is computed, and the drift score is the mean
chi-square across splits, weighted by each split feature's share of the
reference model's total gain — splits on features the model barely uses
contribute almost nothing, however noisy their routing counts are.
Under no drift the score averages near 1; it grows with the size of the
shift and with the compared row count, which is why both sides are
compared at the smaller period's row count. A second score,
`concept_drift`, restricts the mean to splits with a leaf child. A
finding of type `distribution_shift` is emitted when a period's drift
score reaches 5.0 and the maximum over periods also beats a permutation
null: the reference rows and every period's compared rows are pooled,
reassigned to a pseudo-reference group and pseudo-period groups of the
original sizes, every period is rescored against the shared
pseudo-reference, and the maximum is retaken, over 16 replicates.
Evidence carries `time_column`,
`reference_period` (the first period's value range), `reference_rows`,
`reference_r2`, `periods` (one entry per later period with its value
range, row counts, `drift` and `concept_drift`; periods under 100
compared rows record null scores), `max_score`, `max_period` and
`null_max` (the largest replicate maximum). The
score sees only the variables the reference model splits on: drift in an
unused variable, or drift in the target's own distribution with its
predictors unchanged, does not register.

The `duplicate_encodings` detector (standard and deep budgets) reads pairs of
label-like columns — text categories and whole-number codes, excluding
identifiers and columns with fewer than 3 or more than n/2 levels — and states
the reading the functional-dependency findings underneath do not: the two
columns record the same grouping, or one rolls up to the other. A pair is a
candidate when its distinct value pairs number at most 1.02 times the larger
column's level count, counted on rows where both columns are present (at most
100,000 rows at standard, 250,000 at deep); candidates are verified on exact
canonical encodings before a finding is emitted. When the smaller level count
is at least 0.9 of the larger, the form is `duplicate_encoding` and the
evidence field `exact` says whether the pairing is one-to-one on every row;
below that share the form is `rollup`, the subject lists the finer column
first, and `exceptions` counts the rows breaking the finer-to-coarser mapping
(for a duplicate encoding, the rows breaking the majority pairing in either
direction). The advisory's scores (0.62 exact, 0.56 partial, 0.5 rollup) sit
below an exact functional dependency's, so when both describe one pair the
dependency finding ranks first.

The experimental `views` detector is excluded from the default selection.
Request it explicitly at the standard or deep budget. It uses
`concentration/modal_share` for concentrated modeled values within fitted clusters and
`concentration/spike_cells` for cells below its predictive-density threshold.
It also emits `changes/residual_level` for differences between unordered
clusters; these findings do not identify a temporal breakpoint. See the
[views reference](book/methodology/view_clusters_reference.ipynb) for how to
interpret these outputs.

Rust extensions implement `Detector`, declare a `DetectorDescriptor`, and return
`Finding` candidates from `discover(&DetectorContext)`. The context provides the
normalized table, column summaries, optional shared encoding and configuration.
`profile_with_detectors` adds registered extensions. Output categories, subject
columns and statement definitions are validated. The shared consolidator and
selector handle extension results like built-in results. Failed or invalid
extensions are reported in coverage rather than silently changing the taxonomy.

## Migration from schema 0.1

| Previous kind | Current statement |
|---|---|
| `all_null` | `missingness` |
| `constant_column`, `near_constant`, `high_concentration` | `concentration` |
| `likely_categorical`, `identifier_like` | `column_roles`, with a role definition |
| `likely_primary_key`, `composite_key`, `dirty_key` | `uniqueness` |
| `duplicate_rows` | `duplicates`, unit `rows` |
| `format_inconsistency` | `formats` |
| `functional_dependency`, `approximate_fd` | `dependencies/functional`, form `value_mapping`; inspect violations |
| `arithmetic_identity` | `dependencies/functional`, form `affine` |
| `predictable_from`, `decision_table` | `dependencies/predictive` |
| `piecewise_relationship` | `dependencies/predictive`, form `piecewise_linear` |
| `interaction_h` | `dependencies/predictive`, form `interaction_h` |
| `step_shape`, `curved_shape` | `dependencies/predictive`, forms `step_shape` / `curved_shape` (target-aware mode only) |
| `prism_rule`, `association_rule` | `conditional_rules/if_then` |
| `dependence` | `associations/statistical` |
| `period_associations` | `associations/period_association` |
| `redundant_group` | Derived `report.groups` view |
| `date_gap` | `gaps`, with a temporal domain and reference |
| `irregular_interval` | `intervals` |
| `extreme_value` | `intervals/extreme_value` |
| `sorted_by_index` | `ordering` |
| `residual_shift` | `changes/residual_level` |
| `level_shift` | `changes/level_shift` |
| `trend_change` | `changes/trend_change` |
| `variance_change` | `changes/variance_change` |
| `transient_excursion` | `changes/transient_excursion` |

The scientific notebook `notebooks/onceover_analysis.ipynb` uses schema 0.2; CI
executes it against the installed wheel.
