# Finding schema 0.2

A finding describes a statement about the data. Detectors supply evidence for the
statement. Detector names do not determine the public taxonomy.

Schema 0.2 replaces schema 0.1 and is used by package version 0.1.1.

## Categories and statement definitions

| Category | Type or required definition | Meaning |
|---|---|---|
| `missingness` | Missing counts and scope in evidence | Missing observations within recorded rows |
| `concentration` | Distinct counts, dominant shares, concentration measures | Concentration of a value distribution, including constants |
| `column_roles` | `specification.definition.role` | Suggested interpretation of a column, supported by observations |
| `column_roles` | Type `long_format`; `specification.definition` names `index`, `attribute`, and `measure` | The frame is in long format; pivoting on the attribute gives one column per level |
| `uniqueness` | Compared columns and equality criterion | Single-column or composite uniqueness, with any recorded repetitions |
| `duplicates` | Subject unit, compared columns, comparison criterion, missing-value policy | Repetition under a stated comparison |
| `formats` | Format counts and examples | Inconsistent representations within a field |
| `dependencies` | Type `functional` | Inputs determine a target, subject to scope, comparison, tolerance and exceptions |
| `dependencies` | Type `predictive` | Specified inputs support prediction of a target under recorded validation |
| `conditional_rules` | Type `if_then`; structured conditions and outcome | A conditional outcome, qualified by observed frequency and baseline |
| `associations` | Type `statistical` | Statistical association without claiming a particular mapping |
| `associations` | Type `period_association`; period column, period labels, statistic | A numeric association whose strength changes between periods |
| `gaps` | Domain, reference, interval bounds or absent values | Absences relative to an expected sequence or calendar |
| `intervals` | Observed interval statistics | Irregular spacing |
| `ordering` | Direction, strictness and missing-value policy | Order of values within the existing row sequence |
| `changes` | Type `residual_level`; reference model and compared groups. Types `level_shift`, `trend_change`, `variance_change`; breakpoint in the statement definition | Dated residual shifts; dated level, trend, and variance breaks in a series itself from the breaks detector; or cluster differences from the opt-in views pass; inspect the statement definition |

The mathematical form is separate from dependency type. Current forms include
`value_mapping`, `affine`, `piecewise_linear`, `period_split`, and `pivot`. A decision-table algorithm can
support a predictive dependency without establishing an exact functional
constraint. A high predictive score never upgrades a statement to an exact
constraint.

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

Three adjustments keep exact-but-expected statements from outranking
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

Demotion is the only triviality action: findings are never dropped
for being trivial, only ranked lower, with the reason recorded in
their evidence.

## Public structure

Each finding contains:

- `id`: derived from the canonical statement, independent of rank and detector ID;
- `category` and optional `type`;
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

## Report outputs

The same ledger is available in three forms:

- **JSON** (`report.data`, `report.to_json()`): the complete record —
  findings with every evidence record and metric, column cards, groups,
  selection counts, and coverage.
- **Markdown** (`report.to_markdown()`): the headline and overview,
  then each finding's claim followed directly by its evidence. An
  evidence block names the detector and algorithm, the validation
  method, and the rows evaluated, then lists the scalar metrics as
  `key=value` pairs (first 12; the JSON carries the rest). A metric
  whose value is a list of flat records — per-period coefficients, for
  example — renders as a table under the finding, capped at 20 rows.
  Piecewise-linear leaves keep their equation rendering and are not
  repeated as a table. Column cards and coverage close the document.
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
| `prism_rule`, `association_rule` | `conditional_rules/if_then` |
| `dependence` | `associations/statistical` |
| `period_associations` | `associations/period_association` |
| `redundant_group` | Derived `report.groups` view |
| `date_gap` | `gaps`, with a temporal domain and reference |
| `irregular_interval` | `intervals` |
| `sorted_by_index` | `ordering` |
| `residual_shift` | `changes/residual_level` |
| `level_shift` | `changes/level_shift` |
| `trend_change` | `changes/trend_change` |
| `variance_change` | `changes/variance_change` |

The scientific notebook `notebooks/onceover_analysis.ipynb` uses schema 0.2; CI
executes it against the installed wheel.
