# Report schema

The JSON report (`report.to_json()`, `report.data`) is versioned:
`schema_version` is `"0.8"` (the vocabulary renames of the
semantics application, in onceover 0.1.6; `"0.7"` was onceover 0.1.3). The machine-readable
contract is [`report-schema.json`](report-schema.json) (JSON Schema,
draft 2020-12); the test suite validates live reports against it
(`tests/test_report_schema.py`), including profiles of the bundled
corpus datasets. This page is the human-readable inventory of the
same contract. Two maps are open by design: evidence `metrics` and
specification `definition`, whose keys are defined per finding type
in `findings.md`.

## Top level

| key | content |
|---|---|
| `schema_version` | string, e.g. `"0.8"` |
| `overview` | `n_rows`, `n_columns`, `headline` (one-line summary), `dtype_counts`, `role_counts`, `top_findings` (top five claim strings) |
| `frame` | the frame model: `roles` (one entry per column: `column`, `role` — `identifier`, `row_number`, `timestamp`, `timestamp_part`, `dimension`, `flag`, `measure`, or `text`, or null when no assignment was made — and `evidence`, the measurement the assignment rests on), when a time axis was found `axis` (`name`, `source` — `hint`, `native`, `string_parse`, `timestamp_parts`, `year_candidate`, or `week_candidate` — `description`, `confidence` — `declared`, `inferred`, or `low` — plus `parts` for an assembled axis, `source_column` when the axis is or was parsed from one column, and the measured span: `min`, `max`, `distinct_values`, and `step`, the modal step label), and the account computed from the model (schema 0.5): `grain` and `dominant`, inventoried under "The frame account" below. When the caller declared metadata (the `gloss`, `units`, `event`, `entity`, `recorded_after`, `purpose`, or `questions` hints), `declarations` carries the declarations as made — each key present only when declared — and the `declarations` coverage entry accounts for what was checked, what was carried unchecked, and what was refused. The same coverage entry also states the applied/refused decisions for the structural hints `roles`, `level_order`, `time`, and `ordered_axis`; those instructions steer the frame model and the along-axis passes but do not ride the frame account. Absent in reports built outside the pipeline |
| `columns` | object keyed by column name; each entry: `name`, `dtype`, `card_role`, `semantic_role` and `semantic_role_evidence` (the frame model's role for the column and its evidence; absent when no assignment was made), `n`, `null_count`, `null_fraction`, `distinct`, `distinct_ratio`, `numeric_stats`, `concentration`, `top_values` |
| `findings` | list of finding objects (below) |
| `grouped_findings` | statements grouped under a verified relationship family's lead (schema 0.6): full finding objects, each carrying `grouped_under`; absent when no family formed. Grouped statements are not in `findings` and spend no selection or digest slot; the family semantics are defined in `docs/findings.md`, "Verified relationship families" |
| `associations` | every pair the association screens tested (schema 0.7): `a`, `b`, `kind` (`numeric`, `nominal`, `mixed`), pair-complete `n`, the effect sizes that screen computed (`pearson`, `spearman`, `xi_ab`, `xi_ba`; `cramers_v`; `eta`), the raw `p_value`, the Benjamini-Hochberg `q_value` over the pairs tested by the same screen in the run, and the reporting `tier` (`strong`, `weak`, `none`). Findings state the strong and weak pairs; this table is the full screen, including pairs that earned no statement. Absent when no association screen ran |
| `groups` | connected findings (findings sharing columns): `id`, `columns`, `finding_ids`, `description` |
| `selection` | how the report was selected down: `candidates`, `verified_candidates`, `distinct_statements`, `selected`, and (schema 0.6) `families` and `grouped_statements`; reports that ran under cap pressure also carry `candidates_by_class`, `selected_by_class`, and `capped_by_class` — statement counts per statement class (account, structure, shape, hygiene), where `capped_by_class` is candidates minus selected minus grouped per class |
| `coverage` | object keyed by pipeline pass name (which can differ from detector ID), with one entry for every registered detector — a detector that did not run has a `skipped` entry saying why (not enabled, opt-in, quick tier, deadline reached); each entry: `status` (`ran_exact`, `ran_sampled`, `capped`, `skipped`, `not_applicable`, `error`) and `detail` (free text — can be very long on wide tables; see the context-budget note in `docs/benchmarks/2026-10-28-context-digest.md`); on timing-enabled runs (`profile(..., timing=True)`) each entry also carries `elapsed_ms`, the wall time attributed to the pass — the interval since the previous entry was recorded (passes run sequentially; the first entry also carries extraction and the column cards). When the deadline stops one of the long searches (dependence screen, hotspots, discontinuities, surprises, duplicate encodings) mid-pass — or, for the first three, already expired when the pass is reached, in which case the pass is still dispatched and stops at its first work quantum — its entry reads `capped` and the detail states how much of the search space was examined (0 of the total when the deadline had already expired) — pairs, combinations, or targets completed of the total — so a partial negative is distinguishable from an exhaustive one. The surprises scan also reads `capped` when its deterministic work budget (row visits and bitset words) binds without a deadline; its detail states work spent and contexts and categorical targets examined, and later passes still run |
| `source` | present only when the input was a LazyFrame or a file path: `kind` (`lazyframe`, `parquet`, `csv`, or `ipc`), `rows` (rows in the full source, counted exactly), `rows_profiled` (rows the profile ran on), `basis` (`full` or `sampled`), and, when sampled, `sample` (`method`, `seed`, `cap`). Absent for in-memory DataFrame inputs; inventoried under "Source inputs" below |

## Source inputs

The `source` section records where the profiled rows came from
when the input was not an in-memory DataFrame. `rows` is counted
exactly before any sampling. When the source holds more rows than
the source row cap (default 200,000; `source_row_cap=`), the
profile runs on a stratified sample — one row per equal stratum of
the source, hash-chosen within the stratum, deterministic for a
given `sample_seed=` — and `basis` is `sampled`; `exact=True`
collects the source whole instead. When the basis is `sampled`,
every finding's evidence scope records `sampling: "sample"` with
`population_rows` set to the source row count, the overview
headline states the sample, and the coverage `input` entry records
the sample's size and seed, so no sampled number is presented as a
population number.

## Internal row caps

Separately from source sampling, passes with a row cap draw an
**internal** sample keyed by row content: a stable hash of the
canonical values in every column, with exact duplicate rows broken by
a stable occurrence ordinal. The selected indices are ordered by those
keys, so a sampled statistic or finding is a function of the table's
row multiset, not of its input order. Passes that read the row sequence
itself (the along-axis scans) stay positional by design. Sampled
evidence keeps the disclosures above; fields that name original row
positions, such as row-cluster ranges, remain positional descriptions
and are the exception to permutation invariance.

## The frame model

The `frame` section records two judgements the pipeline makes
before any detector runs. The semantic roles say what each column
is for: a `row_number` is a monotonic integer sequence (including a
sequence that restarts per group), an `identifier` holds a
distinct label per row (for whole numbers: packed close together, or
named like a key) or is a whole-number column named like a key
(`customer_id`, `orderKey`), a `timestamp` is a native date or datetime
column, a `timestamp_part` is a member of a set of integer columns
(year and month at minimum) that jointly form a timestamp, or a
column the time axis determines (one value per axis value where the
axis repeats, or a function of a repeating calendar key — year,
quarter, month, week, day of month, day of week, hour — on a
one-row-per-period table; measures are never re-roled), or a grain
factor that subdivides the axis's period (an hour beside a date); its
`evidence` says which, a
`dimension` is a low-cardinality grouping column (a numeric column
qualifies only with at most 32 whole-number values, or 12 fractional
ones), a `flag` is a two-valued column — a boolean with both states
present, or a numeric column holding only 0 and 1, the same
definition the panel passes' flag scan uses — which states a
condition on the row rather than partitioning the frame, so the
dimension consumers (grain inference, dominant structure, entity
axes) never receive it (a two-state string or categorical column
keeps the `dimension` role: its levels are labels, not states), a
`measure` is a numeric quantity, and `text` is free text.
One stated exception to the dimension rule: a numeric column with
exactly two distinct values other than 0 and 1 is a `measure`, not
a dimension — a two-level quantity (activity recorded as 0 or a
fixed token amount) is measured, not grouped by, and the panel
activity scans read measure-role columns. Boolean columns and the
0/1 pair take the `flag` role, and a two-state string or
categorical column stays a dimension: its levels are labels.
A `roles` hint
(`hints={"roles": {"col": "measure"}}`) overrides the assignment
for that column. The axis is the frame's time axis, chosen in
precedence order: a `time` hint, a native date/datetime column, a
string column whose values parse as timestamps, a timestamp
assembled from part columns, or — recorded at low confidence — a
single year-like integer column.

Detectors consume the model in two ways. The temporal passes
(breaks, ordered partitions, drift, period associations, date_gaps)
receive the axis, with string and part columns materialized as a
real date or datetime column, so they run on frames whose time is
stored as text or split into parts. And columns the model marks
as scaffolding — `row_number`, `identifier`, and `timestamp` columns,
plus the part columns of an assembled axis — are excluded as
detector targets and as discontinuity axes; every exclusion is
recorded in the affected pass's `coverage` entry. Column cards
still describe every column, and nothing is removed from the
report body.

## The frame account

Schema 0.5 adds the account: two records computed from the frame
model once the dependency pass has run, stated in the digest
before any finding. `grain` is the frame's key record:
`key_columns` (empty when no exact key exists), `factorization`
(one entry per component of a dimension × axis cross: `column`
and its `distinct` count), `balanced` (every combination of the
factors occurs exactly once — the product of the distinct counts
equals the row count), `combinations` (that product), `near_key`
(when no exact key exists and the axis nearly identifies rows:
the candidate `columns`, its `distinct` count, and the
`duplicate_rows` that repeat a value already present), and
`statement`, the account sentence exactly as the digest states
it. A factorized grain is also a finding (category `uniqueness`,
detector `keys`, algorithm `grain_factorization`, facet scope
`frame`); it replaces the bare key finding for the same columns.
When no factorized grain exists, the grain record falls back to
the dependency pass's exact key — except a key that needs a flag
column: the dependency pass still reports such a key as its own
finding, but the frame model does not promote it to the grain.
A near key is likewise a finding (category `duplicates`,
algorithm `near_key_axis`, scope `frame`).

`dominant` lists the frame's dominant structures, strongest first
(at most two): `kind` (`time`, `cycle`, `dimension`, or `binned` —
a continuous measure's quantile bins), `label`
("shared time pattern", "annual cycle", "weekly cycle", "daily
cycle", or the grouping column's name), `grouping` (what was
grouped by), `shares` (the responses the grouping organises,
strongest first: the share of each response's variance that sits
between groups), `peak` (the highest-mean group for the strongest
measure: `label`, `measure`, `mean`), `rates` (the binary
responses the grouping organises, strongest first, at most two:
for each, the `response` column, its `on_label` — the higher of
its two levels in sorted order — the `base_rate` over the whole
frame, and the peak group's `peak_group`, `peak_rate`,
`peak_support`, and `lift` over the base rate), and `statement`.
The responses are the measures — a column named as a weight is
not one — plus every two-level column (a flag, or a dimension
with exactly two levels), measured as the rate of its on state
and scored by the same eta-squared on its 0/1 indicator.
Duplicate and rollup encodings among the candidate groupings are
collapsed before scoring, so two encodings of one grouping never
occupy both slots, and two encodings of one binary outcome are
measured once. A grouping whose two groups restate a binary
response under another column's encoding is not used as a
grouping, and a grouping never claims a response it determines
exactly — constant within every group — since that share is 1 by
construction. A grouping is stated only when it organises at
least 0.10 of some response's variance; up to two groupings are
stated, strongest first. A peak is stated only when the spread
of its group means reaches a tenth of the measure's own range;
a smaller spread is not a headline peak.

The selected findings are presented in report-band order:
frame-scope and uniqueness findings (the account) first, then
missingness, gaps, and duplicates (completeness), then
dependencies, associations, and conditional rules (relations),
then column roles, formats, ordering, and concentration, then
changes and anomalies (deviations); the selection score orders
findings within a band. Selection itself is unchanged: the caps
consume candidates in demotion-adjusted score order, so no
finding is lost to the re-ranking — the band orders the report
body, not the budget. A verified relationship family's lead
(schema 0.6) presents at the earliest band among itself and its
grouped members, because the lead stands for the family's
strongest content; a duplicate-encoding lead whose members are
dependencies therefore presents with the relations. A column the
time axis determines is a `timestamp_part` before any pass runs
(see the `frame` roles above): it is no pass's subject, so the
report never states its level shifts, spikes, ordering, or its
dependencies on other calendar columns. A finding whose whole
subject is scaffolding columns is demoted in ranking — never
removed — with the reason `scaffolding_subject` and a 0.6 factor
recorded in its evidence metrics; the demotion counts are stated in
the `frame_model` coverage entry.

## Finding object

| key | content |
|---|---|
| `id` | stable finding id within the report |
| `category` | one of `missingness`, `concentration`, `column_roles`, `uniqueness`, `duplicates`, `formats`, `changes`, `anomalies`, `dependencies`, `conditional_rules`, `associations`, `gaps`, `ordering` |
| `facets` | the finding's properties as fields: `scope` (`column`, `columns`, `rows`, `sequence`, `frame`), `object` (`distribution`, `relation`, `structure`, `temporal_behaviour`, `role`), `support` (`exact`, `approximate`, `model_based`, `inferred`), `conditionality` (`unconditional`, `value_conditioned`, `segment_conditioned`), `temporality` (`atemporal`, `ordered`, `change_across_segments`), `status` (`regularity`, `deviation`), and `quality_dimension` (`completeness`, `uniqueness`, `validity`; only on data-quality findings). The category is derived from the facets by the fixed table in `docs/taxonomy-v1-design.md`; the per-type assignment is tabulated in `docs/findings.md` |
| `type` | subtype within the category; always present, and part of the finding `id`. By category: `missing_values`, `sentinel_values`, `predictable_missingness` (missingness); `value_concentration`, `hotspot`, plus `modal_share` / `spike_cells` from the row_clusters pass, and `concentration_over_time` / `outlier_entity` from the series detector's panel scan (concentration); `categorical_role`, `identifier_role`, `duplicate_encoding`, `long_format` (column_roles); `uniqueness` (uniqueness); `duplicate_rows`, `near_duplicate_rows` (duplicates); `format_variation`, `heaping`, `digit_preference`, `benford_deviation`, `boundary_mass`, `content_anomaly` (formats); `functional`, `affine`, `arithmetic_identity`, `predictive`, `leakage_signature`, `order_constraint`, `order_violation`, `model_failure` (dependencies); `if_then` (conditional_rules); `statistical`, `period_association`, `group_contrast`, `conditioned_association`, `slice_shift`, `level_contrast`, `axis_shape` (associations); `gap`, `sequence_gap`, `missing_combinations`, `partial_period` (gaps); `interval_variation`, `extreme_value`, `categorical_outlier`, `row_inconsistency` (anomalies); `column_ordering` (ordering); `residual_shift`, `level_shift`, `trend_change`, `variance_change`, `transient_excursion`, `step_discontinuity`, `predictor_drift`, `joint_distribution_change`, `ordered_partition`, `series_structure`, `one_way_flag`, `entry_block`, `exit_block`, `first_activity`, `slope_break`, `regime_contrast`, `spike_contrast` (changes). The generated taxonomy index in `docs/findings.md` carries the same list |
| `subject` | what the finding is about: `columns` (list), usually `target` and/or `inputs`, `unit` |
| `specification` | the pattern instance: a `definition` (e.g. comparison mode) and, for typed patterns, a `form` (`value_mapping`, `sum`, `linear_combination`, `piecewise_linear`, `hierarchy`, ...) |
| `claim` | the human-readable statement, with the measured numbers in it |
| `evidence` | list of evidence entries: `method` (`detector`, `algorithm`, `provenance`), `basis` (`measured` / `estimated` / `inferred`), `scope` (`sampling`, `population_rows`, `rows_evaluated`, `missing_values`), `validation` (`method`, `used_for_selection`, row counts), and `metrics` — the measurements, plus the finding type's bespoke evidence payload where one is defined |
| `interestingness` | the ranking score used for selection |
| `detector_interestingness` | the producing detector's own score before cross-detector ranking |
| `grouped_members` | on a verified relationship family's lead only: the ids of the statements grouped under it (their full records are in `grouped_findings`); omitted otherwise |
| `grouped_under` | on a grouped statement only: the lead finding's id; grouped statements appear in `grouped_findings`, not `findings`. Omitted otherwise |
| `interpretation` | what the finding means / why it might matter, when the detector provides one |

Findings produced from user-declared hints come from the
expectations pass and carry `reference_source: "declared"` in
their evidence metrics, versus `"inferred"` for the date_gaps pass —
declared structure is never silently presented as discovered
structure.
