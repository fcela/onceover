# Report schema

The JSON report (`report.to_json()`, `report.data`) is versioned:
`schema_version` is `"0.6"` in onceover 0.1.3. The machine-readable
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
| `schema_version` | string, e.g. `"0.5"` |
| `overview` | `n_rows`, `n_columns`, `headline` (one-line summary), `dtype_counts`, `role_counts`, `top_findings` (top five claim strings) |
| `frame` | the frame model: `roles` (one entry per column: `column`, `role` — `identifier`, `index`, `timestamp`, `timestamp_part`, `dimension`, `measure`, or `text`, or null when no assignment was made — and `evidence`, the measurement the assignment rests on), when a time axis was found `axis` (`name`, `source` — `hint`, `native`, `string_parse`, `timestamp_parts`, or `year_candidate` — `description`, `confidence` — `declared`, `inferred`, or `low` — plus `parts` for an assembled axis, `source_column` when the axis is or was parsed from one column, and the measured span: `min`, `max`, `distinct_values`, and `step`, the modal step label), and the account computed from the model (schema 0.5): `grain` and `dominant`, inventoried under "The frame account" below. Absent in reports built outside the pipeline |
| `columns` | object keyed by column name; each entry: `name`, `dtype`, `role`, `semantic_role` and `semantic_role_evidence` (the frame model's role for the column and its evidence; absent when no assignment was made), `n`, `null_count`, `null_fraction`, `distinct`, `distinct_ratio`, `numeric_stats`, `concentration`, `top_values` |
| `findings` | list of finding objects (below) |
| `grouped_findings` | statements grouped under a verified relationship family's lead (schema 0.6): full finding objects, each carrying `grouped_under`; absent when no family formed. Grouped statements are not in `findings` and spend no selection or digest slot; the family semantics are defined in `docs/findings.md`, "Verified relationship families" |
| `groups` | connected findings (findings sharing columns): `id`, `columns`, `finding_ids`, `description` |
| `selection` | how the report was selected down: `candidates`, `verified_candidates`, `distinct_statements`, `selected`, and (schema 0.6) `families` and `grouped_statements` |
| `coverage` | object keyed by pipeline pass name (which can differ from detector ID); each entry: `status` (`ran_exact`, `ran_sampled`, `capped`, `skipped`, `not_applicable`, `error`) and `detail` (free text — can be very long on wide tables; see the context-budget note in `docs/benchmarks/2026-10-28-context-digest.md`); on timing-enabled runs (`profile(..., timing=True)`) each entry also carries `elapsed_ms`, the wall time attributed to the pass — the interval since the previous entry was recorded (passes run sequentially; the first entry also carries extraction and the column cards) |

## The frame model

The `frame` section records two judgements the pipeline makes
before any detector runs. The semantic roles say what each column
is for: an `index` is a monotonic integer sequence (including a
sequence that restarts per group), an `identifier` holds a
distinct label per row (for whole numbers: packed close together, or
named like a key) or is a whole-number column named like a key
(`customer_id`, `orderKey`), a `timestamp` is a native date or datetime
column, a `timestamp_part` is a member of a set of integer columns
(year and month at minimum) that jointly form a timestamp, a
`dimension` is a low-cardinality grouping column (a numeric column
qualifies only with at most 32 whole-number values, or 12 fractional
ones), a `measure` is a numeric quantity, and `text` is free text. A `roles` hint
(`hints={"roles": {"col": "measure"}}`) overrides the assignment
for that column. The axis is the frame's time axis, chosen in
precedence order: a `time` hint, a native date/datetime column, a
string column whose values parse as timestamps, a timestamp
assembled from part columns, or — recorded at low confidence — a
single year-like integer column.

Detectors consume the model in two ways. The temporal passes
(breaks, ordered partitions, drift, period associations, dates)
receive the axis, with string and part columns materialized as a
real date or datetime column, so they run on frames whose time is
stored as text or split into parts. And columns the model marks
as scaffolding — `index`, `identifier`, and `timestamp` columns,
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
A near key is likewise a finding (category `duplicates`,
algorithm `near_key_axis`, scope `frame`).

`dominant` lists the frame's dominant structures, strongest first
(at most two): `kind` (`time`, `cycle`, or `dimension`), `label`
("shared time pattern", "annual cycle", "weekly cycle", "daily
cycle", or the dimension column's name), `grouping` (what was
grouped by), `shares` (the measures the grouping organises,
strongest first: the share of each measure's variance that sits
between groups), `peak` (the highest-mean group for the strongest
measure: `label`, `measure`, `mean`), and `statement`. A grouping
is stated only when it organises at least 0.10 of some measure's
variance; up to two groupings are stated, strongest first.

The selected findings are presented in report-band order:
frame-scope and uniqueness findings (the account) first, then
missingness, gaps, and duplicates (completeness), then
dependencies, associations, and conditional rules (relations),
then column roles, formats, ordering, and concentration, then
changes and intervals (deviations); the selection score orders
findings within a band. Selection itself is unchanged: the caps
consume candidates in demotion-adjusted score order, so no
finding is lost to the re-ranking — the band orders the report
body, not the budget. A verified relationship family's lead
(schema 0.6) presents at the earliest band among itself and its
grouped members, because the lead stands for the family's
strongest content; a duplicate-encoding lead whose members are
dependencies therefore presents with the relations. Relation findings whose subject lies
inside the calendar family (the axis's own columns, every column
the axis determines exactly — measured directly, since a
timestamp that is itself unique has no mined dependency from it
to seed the family — and the closure of those under the exact
functional dependencies) are demoted in ranking, as is any
finding whose whole subject is scaffolding columns — never
removed — with the reason (`calendar_derivation` or
`scaffolding_subject`) and a 0.6 factor recorded in their
evidence metrics; the demotion counts are stated in the
`frame_model` coverage entry.

## Finding object

| key | content |
|---|---|
| `id` | stable finding id within the report |
| `category` | one of `dependencies`, `conditional_rules`, `concentration`, `gaps`, `changes`, `associations`, `uniqueness`, `duplicates`, `formats`, `missingness`, `intervals`, `column_roles`, `ordering` |
| `facets` | the finding's properties as fields: `scope` (`column`, `columns`, `rows`, `sequence`, `frame`), `object` (`distribution`, `relation`, `structure`, `temporal_behaviour`, `role`), `evidence` (`exact`, `approximate`, `model_based`, `inferred`), `conditionality` (`unconditional`, `value_conditioned`, `segment_conditioned`), `temporality` (`atemporal`, `ordered`, `change_across_segments`), `status` (`regularity`, `deviation`), and `quality_dimension` (`completeness`, `uniqueness`, `validity`; only on data-quality findings). The category is derived from the facets by the fixed table in `docs/taxonomy-v1-design.md`; the per-type assignment is tabulated in `docs/findings.md` |
| `type` | subtype within the category; always present, and part of the finding `id`. By category: `missing_values`, `sentinel_values` (missingness); `value_concentration`, `hotspot`, plus `modal_share` / `spike_cells` from the views pass (concentration); `categorical_role`, `identifier_role`, `duplicate_encoding`, `long_format` (column_roles); `uniqueness` (uniqueness); `duplicate_rows` (duplicates); `format_variation`, `heaping`, `digit_preference`, `benford_deviation`, `boundary_mass`, `content_anomaly` (formats); `functional`, `predictive`, `order_constraint` (dependencies); `if_then` (conditional_rules); `statistical`, `period_association` (associations); `gap`, `sequence_gap`, `missing_combinations` (gaps); `interval_variation`, `extreme_value`, `categorical_outlier` (intervals); `column_ordering` (ordering); `residual_level`, `level_shift`, `trend_change`, `variance_change`, `transient_excursion`, `step_discontinuity`, `distribution_shift`, `ordered_partition`, `series_structure` (changes) |
| `subject` | what the finding is about: `columns` (list), usually `target` and/or `inputs`, `unit` |
| `specification` | the pattern instance: a `definition` (e.g. comparison mode) and, for typed patterns, a `form` (`value_mapping`, `sum`, `linear_combination`, `piecewise_linear`, ...) |
| `claim` | the human-readable statement, with the measured numbers in it |
| `evidence` | list of evidence entries: `method` (`detector`, `algorithm`, `provenance`), `basis` (`measured` / `estimated` / `inferred`), `scope` (`sampling`, `population_rows`, `rows_evaluated`, `missing_values`), `validation` (`method`, `used_for_selection`, row counts), and `metrics` — the measurements, plus the finding type's bespoke evidence payload where one is defined |
| `interestingness` | the ranking score used for selection |
| `detector_interestingness` | the producing detector's own score before cross-detector ranking |
| `grouped_members` | on a verified relationship family's lead only: the ids of the statements grouped under it (their full records are in `grouped_findings`); omitted otherwise |
| `grouped_under` | on a grouped statement only: the lead finding's id; grouped statements appear in `grouped_findings`, not `findings`. Omitted otherwise |
| `interpretation` | what the finding means / why it might matter, when the detector provides one |

Findings produced from user-declared hints come from the
expectations pass and carry `reference_source: "declared"` in
their evidence metrics, versus `"inferred"` for the dates pass —
declared structure is never silently presented as discovered
structure.
