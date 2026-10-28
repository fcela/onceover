# Report schema

The JSON report (`report.to_json()`, `report.data`) is versioned:
`schema_version` is `"0.2"` in onceover 0.1.2. This inventory was
taken from a live report (the corpus's thyroid table) on 2026-10-28;
the corpus runner fails loudly if finding fields change shape,
because its expectations match on them.

## Top level

| key | content |
|---|---|
| `schema_version` | string, e.g. `"0.2"` |
| `overview` | `n_rows`, `n_columns`, `headline` (one-line summary), `dtype_counts`, `role_counts`, `top_findings` (finding ids) |
| `columns` | object keyed by column name; each entry: `name`, `dtype`, `role`, `n`, `null_count`, `null_fraction`, `distinct`, `distinct_ratio`, `numeric_stats`, `concentration`, `top_values` |
| `findings` | list of finding objects (below) |
| `groups` | column groups the passes used (e.g. views partitions): `id`, `columns`, and group metadata |
| `selection` | how the report was selected down: `candidates`, `verified_candidates`, `distinct_statements`, `selected` |
| `coverage` | object keyed by detector id; each entry: `status` (`ran_exact`, `ran_sampled`, `capped`, `skipped`, `not_applicable`, `error`) and `detail` (free text — can be very long on wide tables; see the context-budget note in `docs/benchmarks/2026-10-28-context-digest.md`) |

## Finding object

| key | content |
|---|---|
| `id` | stable finding id within the report |
| `category` | e.g. `dependencies`, `conditional_rules`, `concentration`, `gaps`, `changes`, `associations`, `uniqueness`, `duplicates`, `formats`, `missingness`, `intervals`, `column_roles`, `ordering` |
| `subject` | what the finding is about: `columns` (list), usually `target` and/or `inputs`, `unit` |
| `specification` | the pattern instance: a `definition` (e.g. comparison mode) and, for typed patterns, a `form` (`value_mapping`, `sum`, `linear_combination`, `piecewise_linear`, ...) |
| `claim` | the human-readable statement, with the measured numbers in it |
| `evidence` | list of evidence entries: `method` (`detector`, `algorithm`, `provenance`), `basis` (`measured` / `declared`), `scope` (sampling, population rows, rows evaluated), plus method-specific measurements |
| `interestingness` | the ranking score used for selection |
| `detector_interestingness` | the producing detector's own score before cross-detector ranking |
| `interpretation` | what the finding means / why it might matter, when the detector provides one |

Findings produced from user-declared hints carry evidence with
`basis: "declared"` and `reference_source: "declared"` in the
expectations pass, versus `"inferred"` for the dates pass —
declared structure is never silently presented as discovered
structure.
