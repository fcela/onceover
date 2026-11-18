# Precision targets in the gate

Issue #374, step 1. Changes to the gate only; library code is unchanged.

## Why

Before this change the gate compared a head build against its base and nothing else. A base that states 100 false relationships on shuffled tables passes against itself, and so does every head that keeps the count. The reference statements measured what a report states, never what it should leave out. As a result the gate could not separate a report that adds true facts from one that adds restatements of facts it already carries.

## What changed

- **`must_not` reference statements** (`components/reference.py`). A `must_not` statement describes findings that should not be stated. It is matched against every finding in the full report and the lead finding of every digest line. The metrics `reference.must_not_{4k,8k,full}_{default,target}` count matching findings, and lower is better. Matching fields are listed in `benchmarks/gate/README.md`. The new fields are `columns_within` (every finding column inside a set), `exclude_types`, `categories`, `effect_below` (the largest association effect in the evidence) and `max_matches` (only matches beyond this count are counted).
- **Absolute ceilings** (`ceilings.json`, read by `compare.py`). The comparison fails when a listed metric's head value is above its limit, whatever the base holds. The output prints every ceiling with its status, and notes when the head is below a limit so that the same pull request can lower it.
- **Rule in `docs/agents.md`.** While any ceiling is above zero, no new detectors, finding types, hint keys or report fields. This enforces #317.

## Statements added

Every statement with a `check` was verified against the data (`check_statement` returns `True`).

| dataset | id | describes |
|---|---|---|
| bike-sharing-hourly | `calendar-restated` | change, ordering, concentration, dependency, association, rule or anomaly statements whose columns all lie within dteday and the six columns it determines (yr, mnth, season, weekday, holiday, workingday; check: one value per date). Single-column concentration and incomplete periods are not counted. |
| bike-sharing-hourly | `hour-or-row-as-entity` | entry, exit, entity-outlier, one-way-flag or entity-concentration statements naming hr (hour of day, 24 values crossed with every date) or instant (the row number) |
| bike-sharing-hourly | `code-as-series` | weathersit, a 1–4 category code, scanned as a numeric series |
| bike-sharing-hourly | `hourly-profile-per-level` (cnt, registered, casual) | level contrasts by hr beyond the first: the hourly profile of a measure is one fact |
| bike-sharing-hourly | `temp-atemp-no-order` | an order violation between temp and atemp; each exceeds the other on more than 20% of rows |
| adult-census-income | `education-recoding-restated` | dependency, association, rule, anomaly or concentration statements naming education_num, other than the duplicate-encoding statement (check: one-to-one on every row) |
| adult-census-income | `negligible-association` | `statistical` associations whose largest stated effect is below 0.1 |

## Baseline

Main at `3ea33b1`, release build of the native extension, Python 3.13, macOS arm64, seed 0, full gate run (99 s). The committed ceilings are these values.

| metric | value |
|---|---:|
| `reference.must_not_4k_default` | 8 |
| `reference.must_not_8k_default` | 18 |
| `reference.must_not_full_default` | 81 |
| `reference.must_not_4k_target` | 3 |
| `reference.must_not_8k_target` | 16 |
| `reference.must_not_full_target` | 60 |
| `permuted.false_4k` | 48 |
| `permuted.false_full` | 100 |
| `permuted.false_4k_target` | 30 |
| `permuted.false_full_target` | 51 |

Per statement, default mode (4k / 8k / full):

| statement | 4k | 8k | full |
|---|---:|---:|---:|
| adult `education-recoding-restated` | 2 | 4 | 23 |
| adult `negligible-association` | 0 | 0 | 7 |
| bike `calendar-restated` | 4 | 11 | 33 |
| bike `hour-or-row-as-entity` | 1 | 1 | 1 |
| bike `code-as-series` | 1 | 1 | 2 |
| bike `hourly-profile-per-level` (cnt) | 0 | 0 | 4 |
| bike `hourly-profile-per-level-registered` | 0 | 0 | 5 |
| bike `hourly-profile-per-level-casual` | 0 | 0 | 5 |
| bike `temp-atemp-no-order` | 0 | 1 | 1 |

Of the 100 findings in bike's full default report, 33 restate the calendar. Of 100 in Adult's, 23 restate the education recoding.

## Reproduce

```sh
source .venv/bin/activate
maturin develop --release
python benchmarks/gate/run.py --out base.json --commit 3ea33b1
python benchmarks/gate/compare.py base.json base.json   # passes: every ceiling is at its limit
python -m pytest tests/test_gate.py -q
```

## Limits

The `must_not` statements cover two datasets, chosen because the restatements on them were measured in #374. They are written against observed failure shapes: a report could restate the same structure in a form no statement describes. The permuted count has the same limitation, documented in `components/permuted.py`: a statement that follows from one column's distribution alone (two columns whose ranges do not overlap satisfy `a <= b`) counts as false.
