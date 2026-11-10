# Hierarchy collapse: corpus contract before and after

Date: 2026-11-09 (implementation and measurement)
Issue: #207 (collapse redundant dependency findings; declared
hierarchies and column exclusions; the time axis as a target).

## Protocol

The corpus contract (`benchmarks/corpus/run_benchmarks.py`, 34
registered datasets) was run twice on this machine: once against the
unmodified 0.1.4 build and once against the build carrying the change,
same venv (Polars 2.1.0), same budgets from the registry. The runner's
exit code is 2 in both runs, which is the standing state: machine
checks clean, documented misses on record.

## Results

| | before | after |
|---|---|---|
| datasets ok / skipped | 23 / 11 | 23 / 11 |
| machine-verified expectations | 93/93 passed | 93/93 passed |
| documented cases found / missed / partial | 18 / 10 / 4 | 18 / 10 / 4 |

One dataset's selected count changed:

- **chinook** (Invoice table, 412 rows): 24 selected findings before,
  10 after. The billing block is one hierarchy: `billing_address`,
  `billing_postal_code` and `customer_id` determine each other (one
  level), then `billing_city`, `billing_state`, `billing_country`.
  Its 31 exact statements (functional dependencies both directions,
  rollups, the bijection) now group under a single hierarchy lead;
  grouped findings rose accordingly. The registry expectations still
  pass — the functional-dependency predicate now matches in
  `grouped_findings` — and the two affected expectation notes record
  the change and its reason.

No other dataset's findings count or expectation outcome changed.

## Synthetic hierarchy check

A generated table (2,400 rows; `unit` 24 values → `team` 8 →
`division` 4 → `company` 2, plus an unrelated measure and channel
column) produced 31 selected findings on the unmodified build, with
the hierarchy stated 9 times (6 exact statements selected, 3 grouped
across 3 families). After the change: 26 selected, one hierarchy lead
stating the four levels with each mapping's row support, all 9
statements grouped under it, one family.

A week-integer axis table (12 entities × weeks 1–40, axis declared
via `hints.time`) produced one predictive finding targeting the week
column on the unmodified build (the week column carries the measure
role). After the change: no finding targets the week column; the
structural time-axis exclusion is stated in coverage.
