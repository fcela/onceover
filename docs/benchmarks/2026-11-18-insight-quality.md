# Insight quality: what the digest tells a reader

A follow-up to issue #374. The question is whether the report states findings about the data or mechanical facts about the file.

## How the reports were read

The 4,000-character digests of six gate tables (Adult, bank marketing, bike sharing, abalone, thyroid, metro traffic) were read line by line with three questions:
- Does the line say something about the data a reader would act on?
- Does it say how, not only that?
- Is it a restatement of another line?

The share of mechanical lines over all 20 gated tables' digests was then counted by finding type.

## What was wrong

- **"Predictable from" never said how.** 54 predictive statements appeared in the 20 digests, and none said in which direction or by how much the target moves. "income is predictable from marital_status" did not say that married respondents earn more. "cnt is predictable from hr" did not say that rentals peak at 17:00. The fitted models and the encoded bins were available, and the report discarded them after computing gain shares.
- **The time axis's calendar never reached the models.** On metro traffic, the main fact is that traffic follows the hour of day and the day of week. The digest never stated it, because the hour exists only inside `date_time`. The predictive screen saw weather columns only. Level contrasts already derive calendar parts from the axis; the models did not.
- **Mechanical column facts led "What is wrong".**
  - Thyroid's first ten lines were "column X is highly concentrated: 98% false" for rare-condition flags, which are rare by definition.
  - Abalone's first three lines were Benford's-law deviations of shell weights, which span about one order of magnitude, so the law does not apply (#421).
  - Bike stated "numeric column temp behaves like a category (50 distinct values)" while the frame model calls temp a measure.
- **Vacuous exact dependencies.** "date_time → snow_1h" and "date_time → holiday" were exact because the timestamp is nearly unique (1.19 rows per value). Any column is a function of it. Exact FDs had no chance test; approximate FDs had one since step 3. These FDs also fed the calendar closure of the derived-column basis.
- **One fact, several lines.**
  - Abalone stated "X for sex = 'infant' sits below the pooled rest" five times, once per size measure.
  - Bike stated temp from atemp and atemp from temp.
  - Thyroid's `*_measured` columns are exact missingness indicators of the assay columns, and their restatements took 11 digest lines: missingness conditioned on them, FDs among them, a rule, a hotspot, and predictions of them from their own assays.

## Changes

- **Driver profile on predictive statements** (`gbm.rs::driver_profile`). For the top driver, over its encoded bins with at least max(30, 1% of rows) rows, the claim states where the target is highest and lowest: the median for a numeric target, or for a categorical target the share of its event (the less frequent of its two most frequent levels). When the bin profile is monotone over a numeric driver (Spearman |rho| ≥ 0.9), it says "rises with" or "falls with". The bins are in the evidence as `top_driver_profile`. Examples:
  - "rings's median rises with shell_weight: 13 where shell_weight > 0.516 against 5 where shell_weight <= 0.03"
  - "the share of rows with income = '>50K' is highest where marital_status = 'Married-civ-spouse' (44%) and lowest where marital_status = 'Never-married' (5%)"
- **Calendar features** (`encode.rs`, `EncodeConfig::calendar_axis`). A native date or datetime axis lends hour of day, day of week and month of year to the encoded matrix as nominal features. The predictive screen admits them as predictors even though the axis column is excluded. A part that partitions the rows exactly as an existing column does (bike's `hr`) is dropped. On metro: "traffic_volume is predictable from hour of day of date_time (88% of gain), day of week of date_time (11% of gain) (holdout R² = 0.91 …); traffic_volume's median is highest where hour of day of date_time = '16' (6176) and lowest where hour of day of date_time = '2' (314)".
- **A chance test for exact FDs and for approximate ones onto near-constant targets** (`fd.rs`). An FD is stated only when random pairing of the target's values with the determinant's groups would be expected to produce at least 20 violations (`MIN_CHANCE_VIOLATIONS`). The expectation, (rows − 1 per group) × (1 − Σ target share²), is in `FdStats::expected_violations_chance`. Failing FDs are not stated and not used as structure. The bar is the expected-count form of the order pass's test. It keeps price = f(store, product) at 1.25 rows per key (about 10,000 expected violations). It rejects date_time → snow_1h (about 15 expected) and FDs onto a column with one minority row.
- **Missingness indicators** (`basis.rs::missingness_indicators`). A two-valued column without missing values, one of whose levels occurs on exactly the rows where another column is missing, carries that evidence on its frame role. It is no pass's subject or input, except in the missingness pass, which states the relation once ("agent is missing where channel = direct" is the structural fact).
- **Mirrored numeric predictions** (`gbm.rs::mirrored_statements`). When each column of a numeric pair holds at least half of the other's gain, only the direction with the stronger holdout score is stated. Categorical pairs keep both directions, because a level of one can fix the other without the converse.
- **Same level across measures** (`levels.rs::merge_across_measures`). One level that sits on the same side of its pooled rest on at least 3 measures is stated once, with each measure's median shift (form `across_measures`), for example "sex = 'infant' sits below the rest on 6 measures, by median: length (−26%), diameter (−27%), whole_weight (−62%), …".
- **Column statements:**
  - a two-valued column's "highly concentrated" statement is not made (its rate is in the card; "nearly constant" at 99% stays);
  - "behaves like a category" is not stated for a column the frame model reads as a measure;
  - Benford eligibility requires the central 90% of positive values to span two orders of magnitude (`BENFORD_BULK_SPAN`, #421).
- **Gate harness.** The rename invariance maps calendar feature names ("hour of day of r_date_time") back to the original column.

## Results

Full gate, seed 0. Base: main at `4a18255` (#443 merged; #374 steps 3–5 not yet on main). Head: this branch, which includes steps 3–5.

| metric | main | this branch |
|---|---:|---:|
| `permuted.false_full` | 86 | 39 |
| `reference.must_not_full_default` | 22 | 0 |
| `planted.found_full` | 58 | 60 |
| `planted.found_4k` | 56 | 58 |
| `reference.should_4k_default` | 31 | 33 |
| `invariance.reverse` | 185 | 91 |
| `invariance.total` | 315 | 240 |

Digest composition over the 20 gated tables, 4k digests:

| | main | this branch |
|---|---:|---:|
| finding lines | 272 | 249 |
| mechanical column facts (concentration, Benford, roles, keys, ordering, formats) | 38 (14%) | 24 (10%) |
| predictive statements | 54 | 49 |
| … of which say how the target moves | 0 | 40 |

## Not yet addressed

- **No designated outcome in a default run.** The digest ranks "housing_loan is predictable from month" above anything about `subscribed_term_deposit`. In target mode the outcome passes say what drives the outcome; a default run treats every column as a target. A default run cannot know the outcome, and guessing it is the kind of heuristic #374 removed. Declaring the target is the remedy, and the digest could say so when no target is declared.
- **Covariate-from-covariate predictions.** "age is predictable from relationship" and "hum is predictable from hr, weathersit" are true and rarely the point. Ranking them below statements about columns that many others depend on is a scoring question (step 6 of #374).
- **Interactions stay on request.** Bike's commuter peak (hour × working day) is an interaction. The H-statistic pass exists but is off by default.
- **Seed and selection sensitivity.** Driver lists move when a column is added (`invariance.noise` 30 → 52, mostly Adult, whose report sits at its 100-statement cap).

## Reproduce

```sh
source .venv/bin/activate && maturin develop --release
python benchmarks/gate/run.py --out head.json && python benchmarks/gate/compare.py base.json head.json
```
