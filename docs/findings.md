# Finding schema 0.7

A finding describes a statement about the data. Detectors supply evidence for the
statement. Detector names do not determine the public taxonomy.

Schema 0.7 adds tested associations (see "Tested associations"
below): association findings are stated only when a per-pair test
is significant after Benjamini–Hochberg correction, in a strong
tier at or above the effect bar and a weak tier below it, and the
report gains a top-level `associations` table carrying every
screened pair. Schema 0.6 adds verified relationship families (see "Consolidation and
selection"): the report gains a top-level `grouped_findings` list, the
finding object gains `grouped_members` on family leads and
`grouped_under` on grouped statements — both omitted when unset — and
`report.selection` gains `families` and `grouped_statements` counts.
Schema 0.5 adds the frame account to the report (axis span, grain and
dominant structure; see `report-schema.md`, "The frame account"); the
finding object itself is unchanged from schema 0.4. Schema 0.4 adds a `frame` section to the report and `semantic_role` /
`semantic_role_evidence` fields to the column cards (see
`report-schema.md`, "The frame model"); the finding object itself is
unchanged from schema 0.3. Schema 0.3 adds a `facets` object to every
finding (see "Facets" below); the category and type values are
unchanged from schema 0.2, which replaced schema 0.1. Schema 0.7 is
used by package version 0.1.3.

## Categories and statement definitions

| Category | Type or required definition | Meaning |
|---|---|---|
| `missingness` | Missing counts and scope in evidence | Missing observations within recorded rows: a statement for each column missing on at least 1% of rows, with the pattern behind it when one clears a Bonferroni bar (a grouping level, a numeric threshold, or a point on the time axis, from `pattern`, `pattern_column` and the rate metrics in the evidence), and blocks of columns missing on the same rows (`pattern`: `co_missing`), from the `missingness` detector; also sentinel values standing in for missing data — one implausible value repeated in a narrow row window (the `sentinels` detector; value, count, window, and the column mean with and without it in the evidence) |
| `concentration` | Distinct counts, dominant shares, concentration measures | Concentration of a value distribution, including constants; also hotspots — cells of a 2–3 column combination holding far more rows than the marginals predict under independence (the `hotspots` detector; observed vs expected rows, ratio, and the multiplicity bar in the evidence) |
| `column_roles` | `specification.definition.role` | Suggested interpretation of a column, supported by observations |
| `column_roles` | Type `long_format`; `specification.definition` names `index`, `attribute`, and `measure` | The frame is in long format; pivoting on the attribute gives one column per level. At standard and deep budgets the wide form is profiled too and its findings are merged into the same report, tagged `source_form` in their evidence (coverage entry `pivot_profile`) |
| `uniqueness` | Compared columns and equality criterion | Single-column or composite uniqueness, with any recorded repetitions |
| `duplicates` | Subject unit, compared columns, comparison criterion, missing-value policy | Repetition under a stated comparison; also type `near_duplicate_rows` from the `row_consistency` detector, for rows repeating another on every column but one beyond what chance coincidence on the compared columns explains, with `varying_column`, the count, and `expected_by_chance` in the evidence |
| `formats` | Format counts and examples | Inconsistent representations within a field; also type `content_anomaly` from the `content` detector, with `kind` in the evidence: `numbers_as_text`, `dates_as_text`, `booleans_as_text`, `email`, `url`, `uuid`, `ipv4` (the share of values that fit and the ones that do not), `label_variants` (labels differing only in case or spacing), `possible_misspelling` (rare labels close to a common one), `long_text`, `repeated_extreme` (one value repeated at an isolated extreme), `minority_sign`, `zero_inflation`, `two_modes`, `temporal_outlier`; also granularity changes in float columns — the integer-valued share switching regime at a seam row (exact split scan; `change_row` and both shares in the evidence), from the granularity detector; and recording-process fingerprints from the `digits` detector: heaping on round multiples, terminal-digit preference, and Benford first-digit deviation on eligible columns (observed shares against their baselines in the evidence); and boundary mass from the `boundaries` detector: a value piled at a numeric column's observed minimum or maximum (or, in a zero-inflated column, an unusual interior mass point) whose count dominates the median count of its nearest distinct values — the pile's count and share, the local baseline, the bulk's span and quartiles, and example rows in the evidence; the claim states the pile-up as observed and the interpretation names a recording cap or floor as one possible cause, never as the conclusion |
| `dependencies` | Type `functional` | Inputs determine a target, subject to scope, comparison, tolerance and exceptions; the same type carries exact arithmetic identities from the `identities` detector — pairwise affine fits, and multi-term identities (unit-coefficient sums and differences, products, ratios, and fitted linear combinations) over two or three terms, plus exact decompositions of four to eight parts found by a components-first search: candidate parts are the columns lying between zero and the target on at least 99% of sampled rows where both are present (a part cannot exceed the whole), at most 24 of them by mean share of the target, assembled by subtracting parts from the target on the sample and verified on every row under the same exactness standard (at least 99.9% of complete rows within the identity tolerance) |
| `dependencies` | Type `order_constraint` | One column is at most another on every evaluated row, or all but a small counted share of them, from the `order_constraints` detector: the two columns share a kind family (numeric, date, or datetime with the same unit) and compatible ranges; the claim names the direction, the evaluated rows, the violation count and share, the rows tied, and the worst violation with its row; pairs already tied by an exact functional dependency in either direction are stated but ranked below unrelated pairs, and violations of a near-universal order rank above exact orders |
| `dependencies` | Type `order_violation` | The complementary statement from the same `order_constraints` scan, for a pair whose order fails in bulk: one direction's violations pass the `order_constraint` certification share (1% of evaluated rows) but are systematic — at least 20 rows and at least 5% of the evaluated rows, while the reverse direction's violations are at least twice as many — and the two columns track each other on a comparable scale (the median row distance \|a − b\| / max(\|a\|, \|b\|) over sampled complete rows is at most 0.2, recorded in the evidence as `pair_distance`, and their medians agree within a factor of 2.5 when both are non-zero), which keeps the statement off pairs that merely overlap numerically, such as an age in years against a duration in seconds. The claim names the violated direction, the violating row count and share, the reverse direction's violation count and share, and the worst violation with its row; the violating rows are listed in the evidence |
| `dependencies` | Type `predictive` | Drivers support prediction of a target under recorded validation: the claim names the driver shortlist with each driver's share of the model's split gain, and the evidence records every input the model used |
| `conditional_rules` | Type `if_then`; structured conditions and outcome | A conditional outcome, qualified by observed frequency and baseline |
| `associations` | Type `statistical` | Statistical association without claiming a particular mapping |
| `associations` | Type `period_association`; period column, period labels, statistic | A numeric association whose strength changes between periods |
| `gaps` | Domain, reference, interval bounds or absent values | Absences relative to an expected sequence or calendar; the `series` detector checks the time axis against the grid of its modal step, in the default set, while the opt-in `dates` detector also checks other date columns; also type `unexamined_columns` from the views pass: the columns that pass did not examine — past its column cap, with the kept set chosen by the pairwise-dependence pre-screen, or left outside the search when its work budget ran out — with the reason, the cap, and the eligible and examined counts in the evidence, so a reader of the findings alone can tell that part of the table went unexamined (the views coverage entry names the same columns) |
| `intervals` | Observed interval statistics | Irregular spacing |
| `intervals` | Type `row_inconsistency`; `kind` and the row positions in the evidence | From the `row_consistency` detector: `residual_outlier` for rows far from the line two tightly related columns otherwise follow, `swapped_values` for rows where two columns with disjoint usual ranges each hold a value from the other's range |
| `intervals` | Type `extreme_value`; extreme values and their row indices, the bulk edge and the isolation gap in the evidence | A value isolated from the bulk of its column, or from the bulk within one categorical subset, from the surprises detector; the claim states the gap as a multiple of the subset's central spread (p90-p10) |
| `intervals` | Type `categorical_outlier`; the context (one to three condition levels), the dominant level and its share, and the pattern-breaking rows with their values in the evidence | A categorical value contradicting its context's near-universal pattern, from the surprises detector: within the context one level of the column holds at least 99% of at least 30 rows, and at most 3 rows break it. Contexts are conjunctions of levels from the conditioning columns whose single levels come closest to the bar; findings are ranked by binomial significance against the dominant level's global rate, at most 3 per column; a context is reported only when it adds a row no earlier statement about the column named, and it lists every row breaking its pattern |
| `ordering` | Direction, strictness and missing-value policy | Order of values within the existing row sequence; also the backfill shape — a column ordered except for a late block whose values return to earlier territory (at most 3 descents, no post-seam value above the pre-seam peak), reported with the seam row and tail length in the evidence |
| `changes` | Type `residual_level`; reference model and compared groups. Types `level_shift`, `trend_change`, `variance_change`; breakpoint in the statement definition. Type `transient_excursion`; event start, end and peak in the statement definition | Dated residual shifts; level, trend, and variance breaks in a series itself along a date, datetime, or integer-year axis from the breaks detector; short excursions from a local baseline (seasonal or rolling-median) from the same detector, kept only when the series flags few lone points or the event's flagged points cluster beyond the series' scattered background (a Bonferroni-corrected binomial tail), so a heavy-tailed column's ordinary tail is not reported as events; type `step_discontinuity` with form `step` for a cross-sectional level jump of a `measure` column across a threshold in another numeric column, from the discontinuities detector, or at the deep budget across a line in two columns; type `distribution_shift` with form `drift` for drift of a target's predictors along a declared time axis, from the drift detector; type `ordered_partition` with form `recursive_partition` for a model-based recursive partition of a numeric response along a date, datetime, or integer-year axis — per-node level + linear-trend fits; splits in the statement definition with their F statistics and gain shares, each split either a break date on the axis or a bipartition of a categorical dimension's levels ordered by their within-level fitted level; leaves in the evidence with period, dimension conditions on the path from the root, row count, level at period start and slope per year — from the ordered_partitions detector; type `series_structure` from the `series` detector, describing a whole series along the axis, with `kind` in the evidence: `trend` (stated only when the slope also clears an autocorrelation-deflated t-test at p < 0.001, with `slope_p_value`, `residual_autocorrelation` and `effective_points` in the evidence), `cycle`, `persistence`, `new_level`, `ended_level`, `share_shift`, and — on an entity × time grain — `entity_trend` and `entity_break`, the trend and level-break checks run per entity on the principal measure (the measure with the most entity × tick observations), naming the entities that carry each pattern with `entity_column`, `direction`, `entities_with_pattern` and `entities_tested` in the evidence; when the per-entity evidence is mixed, the aggregate `trend` statement says so (no entity carries it, or a minority does, with the counts in its evidence); type `one_way_flag` with form `absorbing_state` from the `series` detector's panel scan, for a boolean or 0/1 flag on an entity × time grain (the frame model's grain factor, or an identifier- or dimension-role column, crossed with the time axis) that switches off-to-on within entities and never on-to-off: the claim names the flag and the entity column, counts the entities showing an onset, and states that no on-to-off transition occurs; where a measure is associated with the flag — zero or missing on every pre-onset row for at least 80% of the onset entities that have pre-onset rows, and positive at or after onset for at least half of the onset entities — the claim also states the lag in axis periods from onset to first positive activity, as a median and middle half, with the count of entities that never turn positive; or cluster differences from the opt-in views pass; inspect the statement definition |

The mathematical form is separate from dependency type. Current forms include
`value_mapping`, `affine`, `piecewise_linear`, `period_split`, `pivot`, `isolation_gap`, `excursion`, `hierarchy`, and `interaction_h`. A decision-table algorithm can
`value_mapping`, `affine`, `piecewise_linear`, `period_split`, `pivot`, `isolation_gap`, `excursion`, `step_shape`, `curved_shape`, and `pairwise_order`. A decision-table algorithm can
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

A predictive statement with no form named comes from the
predictability screen (the `gbm` detector): a histogram gradient
boosting model fitted for the target against the other columns,
early-stopped on a held-out quarter of the rows. The claim names
the model's drivers, not every input the model split on. A driver
is an input whose share of the model's total split gain is at
least the larger of 2% and one tenth of the top driver's share;
the list is in gain order, holds at most eight names, and always
holds the top input. The evidence metrics are `metric` (the full
model's holdout score: R² for a numeric target, accuracy for a
categorical one, with `baseline` the majority-class accuracy),
`inputs` (every input the full model split on), `top_features`
and `gain_shares` (the top three inputs and their gain shares),
`drivers` and `driver_shares` (the named shortlist and its
shares), `rounds`, `n_train`, `n_holdout`, and `rows_used`. When
the driver set is a proper subset of the inputs, the claim
reports two scores: first the holdout score of a model refit on
the drivers alone (also in the evidence as `reduced_metric`,
with `reduced_rounds`), then the full model's score, so the
statement "predictable from the drivers" is itself measured
rather than inferred from the gain shares. Columns the frame
model roles as scaffolding (index, identifier, timestamp, and
timestamp parts of an assembled axis) are excluded as predictors
as well as targets, and the exclusion is stated in the pass's
coverage. The driver shortlist is also the feature pool the
downstream passes start from: the decision-table and rule passes
and the local-regime models take their candidate features from
the target's drivers when the screen produced them, and the
interaction pass orders its shortlist drivers-first, so no later
statement re-expands a target's inputs back to every column the
full model happened to split on.

Gaps require a reference. A temporal gap states its assumed cadence or calendar;
an integer-sequence detector can use the same category with a declared increment.
How that reference was obtained belongs in evidence. Duplicate statements specify
whether rows, values, tuples, or columns were compared and which matching rule
was used. Missing values comparing equal is an explicit policy.

## Facets

Every finding carries a `facets` object stating the finding's
properties as fields, in addition to its category and type. The
category is derived from the facets by a fixed table (checked by
`Finding::validate` and by the test suite for every pattern), so the
two never silently disagree. The design and the full derivation table
are in `docs/taxonomy-v1-design.md`.

| Field | Values |
|---|---|
| `scope` | `column`, `columns`, `rows`, `sequence`, `frame` — what the statement ranges over |
| `object` | `distribution`, `relation`, `structure`, `temporal_behaviour`, `role` — what kind of statement it is |
| `evidence` | `exact` (no exceptions in the evaluated rows), `approximate` (a measured rate or a tolerance), `model_based` (a fitted model), `inferred` (an interpretation) |
| `conditionality` | `unconditional`, `value_conditioned` (a condition on column values), `segment_conditioned` (different behaviour in different segments) |
| `temporality` | `atemporal`, `ordered` (about an ordering), `change_across_segments` (compares segments of an ordering) |
| `quality_dimension` | `completeness`, `uniqueness`, or `validity`, present only on data-quality findings |
| `status` | `regularity` (a structure that holds) or `deviation` (a departure from an expectation) |

Facets by finding type (a dash is an absent `quality_dimension`):

| Type | Scope | Object | Evidence | Conditionality | Temporality | Quality | Status |
|---|---|---|---|---|---|---|---|
| `missing_values`, `sentinel_values` | column | distribution | exact | unconditional | atemporal | completeness | deviation |
| `value_concentration` | column | distribution | exact | unconditional | atemporal | — | regularity |
| `hotspot` | columns | distribution | approximate | unconditional | atemporal | — | regularity |
| `categorical_role`, `identifier_role` | column | role | inferred | unconditional | atemporal | — | regularity |
| `long_format` | frame | role | inferred | unconditional | atemporal | — | regularity |
| `duplicate_encoding` | columns | relation | exact | unconditional | atemporal | — | regularity |
| `uniqueness` | column or columns | relation | exact | unconditional | atemporal | uniqueness | regularity |
| `duplicate_rows` | rows | structure | exact | unconditional | atemporal | uniqueness | deviation |
| `format_variation`, `boundary_mass`, `content_anomaly` | column | distribution | exact | unconditional | atemporal | validity | deviation |
| `heaping`, `digit_preference`, `benford_deviation` | column | distribution | approximate | unconditional | atemporal | validity | deviation |
| `functional` | columns | relation | exact, or approximate when the mapping has violations | unconditional | atemporal | — | regularity |
| `order_constraint` | columns | relation | exact, or approximate when the order has violations | unconditional | atemporal | — | regularity |
| `order_violation` | columns | relation | approximate | unconditional | atemporal | — | regularity |
| `predictive` | columns | relation | model_based | unconditional; `segment_conditioned` in the piecewise form | atemporal | — | regularity |
| `if_then` | columns | relation | exact, or approximate when the rule has violations | value_conditioned | atemporal | — | regularity |
| `statistical` | columns | relation | approximate | unconditional | atemporal | — | regularity |
| `period_association` | columns | relation | approximate | segment_conditioned | change_across_segments | — | regularity |
| `gap`, `sequence_gap` | sequence | structure | exact | unconditional | ordered | completeness | deviation |
| `missing_combinations` | columns | structure | exact | unconditional | atemporal | completeness | deviation |
| `unexamined_columns` | frame | structure | exact | unconditional | atemporal | completeness | deviation |
| `interval_variation` | sequence | distribution | exact | unconditional | ordered | — | deviation |
| `extreme_value` | column | distribution | exact | unconditional; `value_conditioned` within a categorical subset | atemporal | — | deviation |
| `categorical_outlier` | columns | distribution | approximate | value_conditioned | atemporal | — | deviation |
| `column_ordering` | column | structure | exact | unconditional | ordered | — | regularity |
| `residual_level`, `level_shift`, `trend_change`, `variance_change` | sequence | temporal_behaviour | model_based | unconditional | change_across_segments | — | regularity |
| `transient_excursion` | sequence | temporal_behaviour | approximate | unconditional | change_across_segments | — | deviation |
| `step_discontinuity` | columns | relation | approximate | unconditional | atemporal | — | deviation |
| `distribution_shift` | columns | distribution | model_based | unconditional | change_across_segments | — | regularity |
| `ordered_partition` | sequence | temporal_behaviour | model_based | segment_conditioned | change_across_segments | — | regularity |
| `one_way_flag` | columns | temporal_behaviour | exact | unconditional | ordered | — | regularity |

Consumers can filter on facets with `report.select(...)` (one keyword
per facet) and read them as columns of `report.findings_frame()`.

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
- An exact functional dependency's detector score is the lower of
  `0.5 + 0.35 × breadth` and its RFI clamped to [0, 1] (next
  bullet). Breadth is the
  number of distinct determinant tuples (`lhs_distinct` in the
  evidence) divided by the verified rows. A mapping verified over
  40 repeated inputs scores lower than the same mapping verified
  over 600 distinct inputs — and a near-unique determinant's RFI
  is close to what chance alone predicts, so breadth by itself no
  longer lifts such a dependency's score.
- Functional-dependency evidence carries `fi` (the fraction of the
  target's entropy the determinant accounts for),
  `expected_fi_chance` (the fraction expected under independence,
  from the exact hypergeometric expectation of the plug-in
  estimator for the observed margins), and `rfi` = `fi −
  expected_fi_chance`: the reliable fraction of information of
  Mandros, Boley & Vreeken (KDD 2017). An approximate dependency's
  detector score is `min(0.72, rfi clamped to [0, 1])`, and per
  target the reported
  approximate dependency is the eligible candidate with the
  highest RFI — a column joins the determinant only when it raises
  the chance-corrected score, not merely when it trims the
  violation count. The dependency search reports in its coverage
  when the standard budget widens to size-3 determinants or
  three-column keys for a target that no smaller determinant or
  key explains.
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
  measure predicted with the attribute among its inputs). The
  frame model declares two more, applied to findings ahead of the
  ledger (frame-model step 2): `calendar_derivation` (a relation
  finding — dependency, association, or conditional rule — whose
  subject lies inside the calendar family: the time axis's own
  columns, every column the axis determines exactly, and their
  closure under the exact functional dependencies) and
  `scaffolding_subject` (a finding whose whole subject is
  scaffolding columns — index, identifier, timestamp, or
  timestamp-part roles).

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
- `claim`: a readable description qualified by its measurements. When
  the caller declares display labels (`hints.labels`), claims and
  interpretations name columns by their labels; a label is
  presentation only — `subject`, `specification`, and `evidence`
  keep the raw column names, and the `id` is unchanged, because it
  is derived from the canonical statement rather than the claim
  text;
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
- all three breaks findings also carry `breakpoint_range_start`
  and `breakpoint_range_end` (with `breakpoint_range_start_raw` /
  `breakpoint_range_end_raw` in axis units): the located range of
  the changepoint — every split whose two-segment squared error is
  within 3.84 residual variances of the best split's, read off the
  detection scan's own cost curve — alongside the point estimate
  in `breakpoint`. The claim states the range with the point
  estimate as the best estimate.
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
  counts as unchanged. The diff also compares the column cards,
  under `columns`: the columns `other` added and the ones it
  removed (by name, with card dtype and role), the columns whose
  card dtype changed (`retyped` — stated distinctly from a
  removal plus an addition), and, for columns present in both
  with an unchanged dtype, the card quantities that moved, each
  with its before and after values, the absolute change, and the
  relative change where one is meaningful. The compared
  quantities are the card pass's exact counts — rows, nulls and
  their share, distinct values — and, for numeric columns, the
  mean, minimum, and maximum the card carries; the card carries
  no sum, so none is reported, and the rest of the card is not
  diffed. Column identity is by name, so a renamed column reads
  as a removal plus an addition, and row-level changes are not
  compared. Digest rollups for multi-member groups list
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

### Tested associations

Association statements (`associations` category, type `statistical`)
are tested, not bar-filtered. Three screens test pairs:

- numeric pairs (the `numeric_dependence` detector): Pearson and
  Spearman correlations with their t-approximation tests, and
  Chatterjee's ξ in both directions with a permutation-calibrated
  test (the permutation mean and variance of the rank-difference
  sum in closed form, normal tail). The pair's p-value is the
  smallest component p-value, Bonferroni-multiplied by the number
  of components computed.
- nominal pairs (the `association_rules` detector's pair screen):
  a G-test on the pair-complete contingency table, effect
  Cramér's V. A pair whose smallest expected cell count is below
  1 is carried untested.
- mixed pairs (a nominal column against a numeric column, same
  screen): a one-way F-test of the numeric column across the
  nominal column's levels, effect the correlation ratio η.

Each screen Benjamini–Hochberg-corrects over the pairs it tested
in the run (for the numeric screen under a pair cap, the tested
family is the survivor list the detailed statistics cover; the
coverage entry states the family size). A tested pair is stated
as a finding in one of two tiers, recorded as `tier` in the
evidence beside `p_value` and `q_value`:

- **strong**: q ≤ 0.05 and the effect at or above the 0.5 bar
  (maximum of |Pearson|, |Spearman|, ξ in both directions for
  numeric pairs; V or η for the pair screens);
- **weak**: q ≤ 0.05 and the effect below the bar. Weak findings
  carry the full evidence in the report JSON; in the digest they
  collapse into a single line with their count and strongest
  examples.

Every pair a screen computed statistics for — stated or not,
tested or not — is in the report's `associations` table
(`report.associations()` as a DataFrame): the pair, its kind, the
pair-complete row count, its effect sizes, its raw and adjusted
p-values, and its tier. Pairs involving frame-role scaffolding
columns are in the table but, as before, earn no finding.

Strong statements are unconditional, so they carry a Simpson
reversal check. For a strong pair with a signed effect, the
screen asks whether the pooled direction reverses inside every
usable stratum of another categorical column (a stratifier:
2–8 observed levels, not one of the pair's columns and not
withheld by frame role; a stratum is usable with at least 30
pair-complete rows and a defined effect, and the usable strata
must together cover at least half of the pair-complete rows).
Signed effects are the numeric pairs' larger correlation
(Pearson or Spearman, checked when it reaches 0.3 in absolute
value — below that the pair's strength is the unsigned ξ and
there is no material direction), point-biserial correlation for
a mixed pair whose nominal side has two observed levels, and
the log odds ratio for a nominal pair that is 2×2 on the
observed levels. Cramér's V beyond the 2×2 case and the
correlation ratio η over more than two levels are unsigned:
they have no direction to reverse and are not checked.

When every usable stratum's effect has the opposite sign to
the pooled effect and the combined within-stratum effect
differs from zero at |z| ≥ 3.84 (a Bonferroni bar at family
alpha 0.05 over the 400-evaluation budget), a qualification is
appended to the claim. The z bar matters when the association
vanishes inside the strata: the within effects are then noise,
both signs come out opposite to the pooled sign about a quarter
of the time with two strata, and without the bar the claim would
say the direction reverses when it does not. The qualification
names the stratifier and states the pooled and within-stratum
effects with their signs, and the finding's evidence gains a
`simpson` payload: the statistic, the pooled effect, the
stratum-weighted within effect (Fisher-z average for
correlations, inverse-variance weighting for log odds ratios)
and its z against zero (`within_z`), the per-stratum levels
with row counts and effects, the usable and total stratum
counts, and the share of
pair-complete rows the usable strata cover. Binary statistics
also name the level coded as 1 (`coded_level`, or
`coded_level_a` / `coded_level_b` for the odds ratio), so the
direction is interpretable. The finding is never removed,
demoted, or re-tiered, and the associations table is unchanged.
At most 24 stratifiers are considered per pair and 400
pair×stratifier evaluations run per screen; when a bound stops
a check, the screen's coverage entry says so.

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
outcome, and three predictive findings per target. Three per-detector caps bound
the report's context cost: at most 25 selected statements attributed to
`functional_dependencies`, 20 attributed to `gbm`, and 25 attributed to
`discontinuities`, where a statement is
attributed to the detector of its first evidence record. The 2026-11-02
battery measured the first two detectors producing about 41% of all report
characters (550,443 and 467,098 of 2,479,798); the 2026-11-04 battery added
the discontinuities cap after measuring it taking 81 of the 100 slots on the
state-unemployment panel, crowding out every gbm and numeric-dependence
statement (docs/benchmarks/2026-11-04-detector-battery-step2.md). A cap is never silent: when one
binds, coverage gains a `selection` entry with status `capped` stating how
many statements per detector were not selected. Rule evaluation requires at
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

### Verified relationship families

Selection also consolidates statements that restate one exact relation
(schema 0.6). The library's passes verify the same relation in several
forms: an exact bijection between two columns is stated by the
duplicate-encoding pass and again by the functional-dependency pass in
both directions, and an exact identity such as `cnt = casual +
registered` is stated once per rearrangement, with a functional
dependency per direction besides. Reporting each restatement as its own
finding spends digest slots on one fact. A verified relationship family
keeps one lead statement in `findings` and moves the restatements to
`grouped_findings`, where each remains a full finding record carrying
`grouped_under` with the lead's id; the lead carries `grouped_members`
with the grouped ids. Grouped statements spend no selection slot and no
digest slot, and `report.selection` records the family count and the
grouped count alongside the standing counts. When grouping occurs, the
`selection` coverage entry states how many statements were grouped
under how many families.

Families form only over exact, unconditional statements — the producing
pass recorded zero violations (for a duplicate encoding, an exact
one-to-one pairing; for a rollup, zero exceptions) and the statement
carries no conditions. The seed cases are:

- A declared hierarchy that holds (see Declared expectations) leads,
  and every exact statement internal to its level columns groups
  under it: the declaration asserts the levels form one structure,
  so any exact relation among them restates it.
- A hierarchy chain: a connected set of at least three columns whose
  exact pairwise relations (single-input functional dependencies,
  rollups, duplicate encodings) totally order their equivalence
  classes by determination — columns that determine each other form
  one level, and each level determines the next. The lead is a
  constructed statement (type `duplicate_encoding`, form `hierarchy`)
  naming the levels in order, finest to coarsest, with each adjacent
  mapping's row support and every member relation in its evidence
  (`hierarchy_levels`, `hierarchy_mappings`, `member_relations`).
  Members are the exact statements internal to the chain's columns
  that respect its direction: pairwise statements, which built the
  chain, and multi-input dependencies whose target is no finer than
  every input and strictly coarser than at least one. A single
  equivalence class of three or more columns — one grouping stated
  by several mutually determining columns — collapses the same way.
  A structure that branches or merges (two incomparable levels both
  determining a third) is not a chain: nothing is grouped by this
  seed, and the statements fall through to the seeds below.
- An exact duplicate encoding leads, and the exact functional
  dependencies over its column pair, in both directions, group under
  it.
- An exact rollup leads, and the exact functional dependency in its
  own direction (the finer column determines the coarser one) groups
  under it.
- Exact identities and affine dependencies over one column set are
  partitioned by the relation they assert. Two linear statements are
  rotations of one relation exactly when their recorded coefficient
  vectors — weights and intercept — are proportional, whatever
  surface form the scan reported (a sum, a difference, or a fitted
  linear combination of the rearranged terms). Each rotation class
  forms one family, and the exact functional dependencies over the
  set attach to the first class family over that set: a functional
  dependency over an identity's columns restates the identity, since
  the identity itself makes each column a function of the others.
- A set of exact functional dependencies over one column set whose
  directions cover every column — each column is the target of a
  dependency on the rest — is the dependency signature of one exact
  relation and forms a family on its own, led by its highest-ranked
  member.

Directional and scoped distinctions are preserved. A prediction
statement is not equivalent to an exact mapping and is never absorbed
into a family. A statement that holds only within a slice (it carries
conditions, or a value- or segment-conditioned facet) is never a
member. Two different relations over the same columns — different
coefficients, a different intercept, a sum and a product that both
happen to hold — land in different rotation classes and stay distinct.
A distributional statement over a family's columns (a hotspot cell, a
column's role classification) is not implied by the exact relation and
stays independent.

A family competes in selection at its best-ranked member's position:
when the first member is reached, the lead is admitted through the
standing caps and counters, charged exactly as if it had been reached
at its own position. If the lead is admitted, the remaining members
are grouped. If the lead is turned away, the family does not form and
every member competes on its own, exactly as before families existed.
A hierarchy chain's lead is constructed by the ledger rather than
produced by a detector: it is charged to the detector of the member
it was built from (a rollup member when the chain has one), and if
that detector's cap turns it away, the refusal is not counted among
the detector's capped statements — the lead was never a candidate
statement of its own.
A lead presents at the earliest report band among itself and its
grouped members, because the lead stands for the family's strongest
content: a duplicate-encoding lead whose members are dependencies
presents with the relations, not with the column-structure statements.
The digest states the grouped count in its header and appends the
grouped ids to the lead's rolled line.

## Declared expectations

Callers can declare structure the profiler cannot infer, through the
`hints` argument (Rust: `ProfileConfig.hints`):

- `time`: the time/ordering axis column;
- `order_by`: columns the rows are claimed to be ordered by;
- `no_gaps`: columns expected to form a complete sequence;
- `cadence`: per-column cadence — `daily`, `business_day`, `weekly`,
  `monthly`, or a fixed day/step count;
- `dimensions`, `measures`: the panel's declared dimension and measure
  columns;
- `hierarchies`: known hierarchies — each entry is one hierarchy's
  level columns, ordered finest to coarsest;
- `exclude_columns`: columns to leave out of the profile entirely.

The `expectations` pass checks the declarations exactly and tags its
evidence `reference_source: "declared"`, against the dates pass's
`"inferred"`. Five checks run:

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
- **Declared hierarchies**: every adjacent mapping of a declared
  hierarchy (each level determines the next) is verified exactly, on
  the rows where both levels are present. A hierarchy that holds
  yields one structural statement (type `duplicate_encoding`, form
  `hierarchy`; the declared levels and each mapping's row support in
  the evidence), and the inferred statements that restate the chain
  group under it as a verified relationship family. A hierarchy that
  does not hold yields one deviation statement naming every broken
  mapping, with the violating rows and the split levels per mapping
  in `broken_mappings`. The rule is symmetric with ordering: a
  satisfied declaration is stated once, a violated one is stated
  once, and neither is restated pair by pair.

Hints naming columns that are missing, or columns of an unusable type
for the check, are ignored with the reason recorded in the
`expectations` coverage entry; malformed hint values raise an error
at the API boundary.

Two declarations act outside the expectations pass. `exclude_columns`
is applied before any pass runs: an excluded column gets no column
card and no finding can name it, and the `column_exclusions` coverage
entry states the excluded columns, their source (user-declared), and
any declared names that are not columns of the table. And the column
the frame model assembles as the report's time axis is excluded from
target roles and from being profiled as a measure by the series pass
by structure, whatever role it was assigned — the protection no
longer depends on the axis column happening to carry the timestamp
role, so a bare week integer with the measure role is not a target.
The passes' coverage entries state the exclusion. A `roles` entry
for the axis column is an explicit user declaration and wins over
the structural default; `time` chooses which column the axis is.

## Detectors and extensions

```python
import onceover

onceover.available_detectors()
report = onceover.profile(df, detectors=["prism", "association_rules"], max_rules=10)
```

Each registry entry lists its ID, produced categories, resource dependencies,
and activation. The default set has 28 of the 35 registered detectors. Six are
opt-in and run only when named in `detectors`: `views` (experimental), and
`dates`, `pivot`, `period_associations`, `drift`, and `granularity` — the
2026-11-02 detector battery measured those five producing no findings in 40
runs over 20 datasets, so a default run no longer spends time or report space
on them; it records each as skipped in coverage, naming it opt-in (the
benchmark record is `docs/benchmarks/2026-11-02-detector-opt-in.md`).
`expectations` is in the default set but hint-gated: it engages only on
declared hints. `target_shapes` is target-gated: it runs when a target is
designated. Required detectors may execute to supply shared target
descriptions without contributing their own findings. An empty selection
still returns column summaries and coverage but no detector findings.

The `discontinuities` detector (standard and deep budgets) scans for
cross-sectional step jumps: for each numeric target column, step
indicators along every other numeric column's axis and — at the deep
budget only — across diagonal boundaries in column pairs, each fitted
against a smooth background by OLS. Rows are split into a fit half
that ranks candidates and an evaluation half that estimates the jump,
so the reported `z` is not inflated by the search. A flagged
candidate is reported only when its local level gap is
scale-invariant (the gap between the nearest 60
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
`t_fit`, held-out `gain`, `type_ratio`, `type_gap_t`, and `type_z`. The
2026-11-02 battery measured this detector as the runtime outlier (0.43 s
median marginal cost, +83.0 s of excess across 20 datasets), with the
cost concentrated in the diagonal scan on wide tables. The diagonal
scan therefore runs only at the deep budget; the standard budget scans
axes only. At the deep budget, a table with more than 20 numeric axes
pairs the 10 strongest axes per target — 45 column pairs; tables with
at most 20 numeric axes scan every pair.

The `drift` detector (standard and deep budgets) compares periods of a
declared time axis. It is opt-in: select it explicitly with
`detectors=["drift", ...]`. It engages only when the caller passes
`hints={"time": "<column>"}`; without a declared axis it records coverage
and emits nothing. The time column's rows are cut into contiguous periods
of roughly equal row counts (at most 8 at standard, 12 at deep; a period
needs at least 100 rows). Like the other model-based passes, it works on
the shared encoded row sample (20,000 rows at standard, 50,000 at deep):
on larger frames the periods are cut over the sample, the period row
counts in the evidence are sample counts, and the coverage detail states
the sample size. For each numeric target column other than the
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
| `categorical_outlier` | `intervals/categorical_outlier` |
| `sorted_by_index` | `ordering` |
| `residual_shift` | `changes/residual_level` |
| `level_shift` | `changes/level_shift` |
| `trend_change` | `changes/trend_change` |
| `variance_change` | `changes/variance_change` |
| `transient_excursion` | `changes/transient_excursion` |

The scientific notebook `notebooks/onceover_analysis.ipynb` uses schema 0.4; CI
executes it against the installed wheel. The notebook profiles with the
default set plus the opt-in `dates` detector (its calendar-completeness
results report the date-gap finding); the selection is shared with the
test suite as `notebook_detectors()` in `notebooks/onceover_example.py`.
