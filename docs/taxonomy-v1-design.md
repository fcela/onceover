# Finding taxonomy v1: facets with a derived category

Status: implemented in schema 0.3 (this change). Decision: option B of the
taxonomy decision package — facets added, the category kept as a label
derived from the facets by a fixed, test-pinned table. The category and
the (category, type) pair remain the compatibility surface for filtering,
grouping, and report diffing; nothing that consumes a 0.2 report's
category values needs to change.

## Why

The 13 categories are not peers of one kind. Measured against the
profiling literature:

- Abedjan, Golab & Naumann's survey (2015) classifies profiling tasks by
  scope — single-column, multi-column, and dependency tasks — with
  conditional, partial, and approximate results as cross-cutting
  properties of a result, not as separate task families.
- A conditional functional dependency is defined as a functional
  dependency plus a condition (Fan et al., 2008). Filing conditioned
  dependencies under their own top-level category makes one property
  (having a condition) outrank the statement itself.
- Missingness, uniqueness, and format consistency are data-quality
  dimensions in the DAMA sense (completeness, uniqueness, validity),
  not statement kinds; they cross-cut the rest of the scheme.
- The temporal patterns (level shift, trend change, transient
  excursion, step) are one family — how a quantity behaves along an
  ordering — at different grain sizes (Foorthuis, 2021).

A single label per finding cannot state all of that, so boundary
findings lose information: a dependency that holds only under a
condition, or only within time segments, is filed under exactly one
label and the other property is invisible to a consumer filtering by
category.

## The facets

Every finding carries a `facets` object with seven fields. Six are
always present; `quality_dimension` is present only when the finding
is a data-quality statement.

| Field | Values | Meaning |
|---|---|---|
| `scope` | `column`, `columns`, `rows`, `sequence`, `frame` | What the statement ranges over |
| `object` | `distribution`, `relation`, `structure`, `temporal_behaviour`, `role` | What kind of statement it is |
| `evidence` | `exact`, `approximate`, `model_based`, `inferred` | How the statement is supported: holds without exception in the evaluated rows; holds at a measured rate or within a tolerance; rests on a fitted model; is an interpretation inferred from observations |
| `conditionality` | `unconditional`, `value_conditioned`, `segment_conditioned` | Whether the statement carries a condition: none; a condition on column values; different behaviour in different segments of an ordering or of an input |
| `temporality` | `atemporal`, `ordered`, `change_across_segments` | The role of order: none; the statement is about an ordering; the statement compares segments of an ordering |
| `quality_dimension` | `completeness`, `uniqueness`, `validity` | The DAMA dimension, when the finding is a quality statement |
| `status` | `regularity`, `deviation` | Whether the finding reports a structure that holds or a departure from an expectation |

Assignment is a property of the pattern, refined by the finding's own
record where the record decides it:

- A functional dependency is `exact` when its violation count is zero
  and `approximate` otherwise.
- A conditional rule is `exact` when it has no violations among the
  rows its condition selects, `approximate` otherwise.
- An extreme value is `value_conditioned` when it is stated within a
  categorical subset and `unconditional` otherwise.
- A predictive dependency is `segment_conditioned` in its piecewise
  form and `unconditional` otherwise.

## The derivation table

`category` is a function of the facets (plus whether the relation is
directed — the subject names a target or the specification an
outcome). The rules apply in order; the first that matches gives the
category. `Finding::validate` checks that a finding's stated category
equals the derived one, and the test suite checks the derivation for
every pattern.

| # | Condition | Category |
|---|---|---|
| 1 | quality = completeness, object = distribution | `missingness` |
| 2 | quality = completeness, object ≠ distribution | `gaps` |
| 3 | quality = uniqueness, status = regularity | `uniqueness` |
| 4 | quality = uniqueness, status = deviation | `duplicates` |
| 5 | quality = validity | `formats` |
| 6 | object = role | `column_roles` |
| 7 | object = relation, conditionality = value_conditioned | `conditional_rules` |
| 8 | temporality = change_across_segments, object ≠ relation | `changes` |
| 9 | object = relation, status = deviation | `changes` |
| 10 | object = relation, directed | `dependencies` |
| 11 | object = relation, not directed, evidence = exact | `column_roles` |
| 12 | object = relation, not directed, evidence ≠ exact | `associations` |
| 13 | object = temporal_behaviour | `changes` |
| 14 | object = distribution, temporality = ordered | `intervals` |
| 15 | object = distribution, status = deviation | `intervals` |
| 16 | object = distribution, status = regularity | `concentration` |
| 17 | object = structure, temporality = ordered | `ordering` |
| 18 | object = structure | `gaps` |

Two cells deserve a note:

- Rule 9 exists for the step discontinuity: a relation whose finding
  is a departure (the level on one side of a boundary does not continue
  the other side) is a change, not an association.
- Rule 11 is the scheme's one awkward cell. A duplicate encoding is an
  exact, symmetric relation between two columns; it is filed with the
  column roles because the statement is about what the columns are
  (two encodings of one quantity), not about a mapping or an
  association strength. If a second exact symmetric relation pattern
  appears, this rule is the place to revisit.
- Rule 8 excludes relations so that a period association — an
  association whose strength changes across periods — stays an
  association (rule 12) rather than becoming a change.

## What consumers get

- Filter on facets directly (`report.select(object="relation")`, the
  facet columns of `findings_frame()`, or the JSON), or keep filtering
  on category and type exactly as before.
- The distinction the flat scheme discarded is now readable: a
  conditional rule and a functional dependency differ in exactly the
  `conditionality` field; a key and duplicate rows share
  `quality_dimension = uniqueness` and differ in `status`.

## Known limits of v1

- Facet assignment is per pattern, with the four refinements listed
  above. A finding whose detector record carries a property the table
  does not read (for example, an association computed within periods
  but reported as one number) keeps the pattern's default facets.
- `evidence = inferred` is used only by the column roles. If more
  interpretation-style findings appear, the value's boundary against
  `model_based` needs a sharper statement.
- The derivation is checked for consistency, not derived at
  construction time for plugin findings: `from_statement` computes
  facets from the stated category and type, so a plugin that states
  an inconsistent pair is caught by `validate`, not silently
  relabelled.
