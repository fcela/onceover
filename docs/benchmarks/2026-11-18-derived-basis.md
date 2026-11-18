# Derived-column basis

Issue #374, step 2. Columns that restate other columns are now found before the detectors run. Each derivation is stated once, through a column role or the duplicate-encoding finding, and no later pass treats those columns as subjects.

## What it does

`crates/onceover/src/basis.rs` finds three kinds of column from their values:

- **Axis-derived columns.** The time axis determines the column exactly: each axis value occurs with one value of the column. Bike sharing's `yr`, `mnth`, `season`, `weekday`, `holiday` and `workingday` are each one value per `dteday`.
  - The test applies only where the axis repeats (at least half of the rows share their axis value with another row). On a one-row-per-day table it is vacuous, so there the column must instead be a function of a calendar key that repeats: year, quarter, month of year, year and month, day of month, day of week, week, or (on a datetime axis) hour and date.
  - Columns that exact dependencies make functions of the derived columns are added.
  - Measures are never tested. A measure the calendar determines is a time pattern of the data, not a derivation.
- **Sub-period columns.** A grain factor that subdivides the axis's period: an integer column on a consecutive range of at most 60 values, crossed with at least 95% of the axis's periods. Bike sharing's `hr` qualifies: 24 values crossed with 99% of 731 dates.
- **Recodings.** One side of an exact one-to-one duplicate encoding, where at least one side is a string label. A numeric level code (integers on a consecutive range of at most 32 values) is kept over its label, because the code carries the order of the levels and the models use it. Otherwise the label is kept. Two numeric columns paired one-to-one are left to the identity and dependency passes.

Axis-derived and sub-period columns get the `timestamp_part` role, with the evidence in the frame section and the column card. For example, `mnth`: "determined by dteday: one value per dteday value"; `hr`: "subdivides each dteday period: 24 consecutive values crossed with 99% of the 731 periods".

## How the passes use it

- **Subjects.** All three kinds join the existing target exclusions (`scaffold_excluded`). They are no longer targets of the dependency, predictability, missingness, identity, order, outlier or dependence passes.
- **Inputs.**
  - Axis-derived and sub-period columns stay predictors (the predictability screen gained `inputs_allowed`), level-contrast groupings, outcome-contrast groupings and model-failure inputs. `hr` and `workingday` still explain `cnt`.
  - A redundant recoding is no input either: the encoder skips it (`EncodeConfig::skip_columns`).
- **Series passes take measures only.** `breaks`, and `ordered_partitions` as responses, no longer scan flags or dimensions; a spike in a 0/1 holiday flag restates its values. Dimensions remain split variables in `ordered_partitions` (`PartitionsConfig::exclude_responses`).
- **Hotspots** skip combinations whose columns all lie in the axis and the columns it determines (`HotspotConfig::related_columns`). A sub-period is not in that group, so the hour × working-day cell, the rush-hour concentration the weighted run states, is still examined.
- **Level contrasts.**
  - Measures are now role `measure` only.
  - The scaffolding and redundant recodings are neither measures nor groupings.
  - An axis-derived column that partitions the rows exactly as a calendar part the pass derives (month of year, day of week) is examined once, as that part.
- **Entities** for the panel passes exclude `timestamp_part` and `row_number` columns. `entity_columns` and its duplicate `panel_entity_columns` are merged into one function.
- **Dropped:**
  - sortedness statements on derived columns ("mnth is ordered except for a late block — rows appended out of order" was the month resetting each January);
  - "behaves like a category" card statements on calendar parts;
  - dependency statements and rollups entirely inside the calendar family.

  The recoding's FDs stay grouped under the duplicate-encoding lead, as before.
- **Removed:** the `calendar_derivation` demotion in `account::demote_restatements` and `calendar_family`, which demoted restatements by a factor of 0.6 after they had been produced. The restatements no longer arise. `scaffolding_subject` demotion is unchanged.

## Results

Main at `227d742` (step 1 merged) against this branch, release builds, Python 3.13, macOS arm64, seed 0, full gate.

| metric | base | head |
|---|---:|---:|
| `reference.must_not_full_default` | 81 | 22 |
| `reference.must_not_8k_default` | 18 | 1 |
| `reference.must_not_4k_default` | 8 | 0 |
| `reference.must_not_full_target` | 60 | 14 |
| `permuted.false_full` | 100 | 86 |
| `permuted.false_4k` | 48 | 46 |
| `reference.must_*` (every scope and mode) | unchanged | unchanged |
| `reference.should_4k_default` | 29 | 30 |
| `planted.found_4k` | 56 | 57 |
| `weighted.must_*` | unchanged | unchanged |

Per statement (full default report): bike `calendar-restated` 33 → 0, `hour-or-row-as-entity` 1 → 0, `code-as-series` 2 → 0; Adult `education-recoding-restated` 23 → 0. The remaining 22 are the per-hour level contrasts (14), Adult associations with effect below 0.1 (7) and the temp/atemp order violation (1). Those are step 3.

The ceilings are lowered to the head values.

Corpus: 32 datasets run, 220/220 machine expectations pass, documented cases unchanged (17 found, 11 missed, 4 partial).

### Accepted differences

`accepted.json` records the following, each with its reason:

- **bike `invariance.reverse` 0 → 3.** Under column reversal, the drivers named for `hum` move among near-collinear inputs (temp/atemp, mnth/season). The predictability holdout split is seeded by column position.
- **Adult `invariance.noise` 6 → 7.** With a noise column added, one more level contrast (age by marital_status) is admitted under that pass's own 20-statement cap, because education_num restatements no longer take slots.
- **bank-marketing `should_8k_default` 4 → 3.** The base credited `subscription-housing-loan` through a text-pattern match on a rolled-up 8k digest line. No finding in the full report supports it, in either base or head.
- **`redundancy.finding_permille` -1 to -6.** The share of digest characters on finding lines moves slightly as restatement lines leave.

### Reference statement change

Adult's `education-recoding-restated` now names the label side (`education`). The kept side is the numeric level code, so statements naming the label are the restatements. On main, before this change, the statement named education_num; both versions count 23 restatements on main.

## Reproduce

```sh
source .venv/bin/activate && maturin develop --release
python benchmarks/gate/run.py --out head.json
python benchmarks/gate/compare.py base.json head.json
python benchmarks/corpus/run_benchmarks.py --json-out corpus.json
```

## Limits

- Timestamp parts are now recognised by value, but an hour column beside a date is not merged into one datetime axis. Along-axis passes still run along the date, with 24 rows per date.
- Adult's `education` → `education_num` choice depends on the level-code rule. A nominal code that happens to be consecutive (a region numbered 1–4) is kept over its readable label.
