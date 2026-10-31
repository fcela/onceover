# Report schema

The JSON report (`report.to_json()`, `report.data`) is versioned:
`schema_version` is `"0.2"` in onceover 0.1.3. The machine-readable
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
| `schema_version` | string, e.g. `"0.2"` |
| `overview` | `n_rows`, `n_columns`, `headline` (one-line summary), `dtype_counts`, `role_counts`, `top_findings` (top five claim strings) |
| `columns` | object keyed by column name; each entry: `name`, `dtype`, `role`, `n`, `null_count`, `null_fraction`, `distinct`, `distinct_ratio`, `numeric_stats`, `concentration`, `top_values` |
| `findings` | list of finding objects (below) |
| `groups` | connected findings (findings sharing columns): `id`, `columns`, `finding_ids`, `description` |
| `selection` | how the report was selected down: `candidates`, `verified_candidates`, `distinct_statements`, `selected` |
| `coverage` | object keyed by pipeline pass name (which can differ from detector ID); each entry: `status` (`ran_exact`, `ran_sampled`, `capped`, `skipped`, `not_applicable`, `error`) and `detail` (free text — can be very long on wide tables; see the context-budget note in `docs/benchmarks/2026-10-28-context-digest.md`); on timing-enabled runs (`profile(..., timing=True)`) each entry also carries `elapsed_ms`, the wall time attributed to the pass — the interval since the previous entry was recorded (passes run sequentially; the first entry also carries extraction and the column cards) |

## Finding object

| key | content |
|---|---|
| `id` | stable finding id within the report |
| `category` | one of `dependencies`, `conditional_rules`, `concentration`, `gaps`, `changes`, `associations`, `uniqueness`, `duplicates`, `formats`, `missingness`, `intervals`, `column_roles`, `ordering` |
| `type` | subtype within the category; always present, and part of the finding `id`. By category: `missing_values`, `sentinel_values` (missingness); `value_concentration`, `hotspot`, plus `modal_share` / `spike_cells` from the views pass (concentration); `categorical_role`, `identifier_role`, `duplicate_encoding`, `long_format` (column_roles); `uniqueness` (uniqueness); `duplicate_rows` (duplicates); `format_variation`, `heaping`, `digit_preference`, `benford_deviation` (formats); `functional`, `predictive` (dependencies); `if_then` (conditional_rules); `statistical`, `period_association` (associations); `gap`, `sequence_gap`, `missing_combinations` (gaps); `interval_variation`, `extreme_value`, `categorical_outlier` (intervals); `column_ordering` (ordering); `residual_level`, `level_shift`, `trend_change`, `variance_change`, `transient_excursion`, `step_discontinuity`, `distribution_shift` (changes) |
| `subject` | what the finding is about: `columns` (list), usually `target` and/or `inputs`, `unit` |
| `specification` | the pattern instance: a `definition` (e.g. comparison mode) and, for typed patterns, a `form` (`value_mapping`, `sum`, `linear_combination`, `piecewise_linear`, ...) |
| `claim` | the human-readable statement, with the measured numbers in it |
| `evidence` | list of evidence entries: `method` (`detector`, `algorithm`, `provenance`), `basis` (`measured` / `estimated` / `inferred`), `scope` (`sampling`, `population_rows`, `rows_evaluated`, `missing_values`), `validation` (`method`, `used_for_selection`, row counts), and `metrics` — the measurements, plus the finding type's bespoke evidence payload where one is defined |
| `interestingness` | the ranking score used for selection |
| `detector_interestingness` | the producing detector's own score before cross-detector ranking |
| `interpretation` | what the finding means / why it might matter, when the detector provides one |

Findings produced from user-declared hints come from the
expectations pass and carry `reference_source: "declared"` in
their evidence metrics, versus `"inferred"` for the dates pass —
declared structure is never silently presented as discovered
structure.
