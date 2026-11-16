# Gate report

## TL;DR

Latest change `064132d` (2026-11-16), Merge pull request #334 from fcela/feat/317-digest-briefing: compared with `2601a72`, 16 total metrics better, 8 worse, 0 worse and accepted.

| measure | latest (`064132d`) | previous (`2601a72`) | first recorded (`1618886`) |
|---|---:|---:|---:|
| Must-tier reference facts stated in the 4,000-character digest | 10 of 12 (+3, better) | 7 of 11 | 7 of 11 |
| Must-tier reference facts stated in the 4,000-character digest, target mode | 11 of 12 (+4, better) | 7 of 11 | 7 of 11 |
| Should-tier reference facts stated in the 4,000-character digest | 6 of 16 (-1, worse) | 7 of 12 | 7 of 12 |
| Should-tier reference facts stated in the 4,000-character digest, target mode | 10 of 16 (+2, better) | 8 of 12 | 8 of 12 |
| Planted structures stated in the 4,000-character digest | 29 (+9, better) | 20 | 20 |
| Facts repeated in the 4,000-character digest | 0 (-4, better) | 4 | 4 |
| Facts repeated in the 4,000-character digest, target mode | 0 (-3, better) | 3 | 3 |
| Relationship statements in the 4,000-character digest of shuffled tables (all false by construction) | 78 (-58, better) | 136 | 136 |
| Statements that fail verification against the data | 0 (unchanged) | 0 | 0 |

Biggest improvements: redundancy long decimals default 53 → 0; redundancy long decimals target 41 → 0; Facts repeated in the 4,000-character digest 4 → 0
Biggest regressions: invariance scale 5 → 8; invariance noise 30 → 38; Should-tier reference facts stated in the 4,000-character digest 7 → 6
Accepted regressions: none.

Hard lines: none breached — no statement failed or errored in verification, and no must-tier reference fact was lost from the 4,000-character digest.

5 change(s) recorded. Latest: `064132d` (2026-11-16), onceover 0.1.5, seeds [0].

Each row is one change on main, newest first. A value is followed, in parentheses, by its movement against the change before it, with `better` or `worse` when the metric has a direction. Seconds are compared only between rows produced on the same platform.

## Headline metrics

- **planted found in 4k digest**: known structures injected into real tables that the 4,000-character digest states (higher is better).
- **must statements in 4k digest**: hand-written reference statements of tier must that the digest states, default mode (higher is better).
- **must statements in 4k digest, target mode**: the same count with the dataset's target designated (higher is better).
- **statements on shuffled tables (4k digest)**: relationship statements in the digest of a table whose columns were shuffled independently, where no relationship exists (lower is better).
- **facts changed by layout**: stated facts lost or gained when columns are reordered, rows shuffled, columns renamed, a noise column added, or a column rescaled (lower is better).
- **repeated facts in 4k digest**: digest lines that restate the columns and relation of an earlier line (lower is better).
- **statement mismatches**: statements whose recount against the data disagrees with the report (lower is better).
- **profile seconds**: summed median profile time (lower is better).

| date | commit | change | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 2026-11-16 | `064132d` | Merge pull request #334 from fcela/feat/317-digest-briefing | 29 (+9, better) | 10 (+3, better) | 11 (+4, better) | 78 (-58, better) | 430 (+22, worse) | 0 (-4, better) | 0 | 12.396 (+0.063) |
| 2026-11-16 | `2601a72` | Merge pull request #326 from fcela/feat/gate-report-tldr | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 12.333 (+0.008) |
| 2026-11-15 | `daf7a79` | Merge pull request #324 from fcela/feat/315-corpus-targets | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 12.325 |
| 2026-11-15 | `ca275fd` | Merge pull request #323 from fcela/feat/317-gate-history-report | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 13.385 |
| 2026-11-15 | `1618886` | Merge pull request #320 from fcela/fix/295-agent-token-economy | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 6.859 |

## Latest change: `064132d` Merge pull request #334 from fcela/feat/317-digest-briefing

Compared with `2601a72`: 16 total metrics better, 8 worse, 0 worse and accepted. Rows marked `total` sum the gated datasets; the other rows name one dataset.

### Better

| metric | where | before | after | change |
|---|---|---:|---:|---:|
| permuted.false_4k | total | 136 | 78 | -58 |
| permuted.false_4k_target | total | 67 | 44 | -23 |
| planted.found_4k | total | 20 | 29 | +9 |
| redundancy.finding_permille_default | total | 7118 | 10624 | +3506 |
| redundancy.finding_permille_target | total | 4531 | 7560 | +3029 |
| redundancy.long_decimals_default | total | 53 | 0 | -53 |
| redundancy.long_decimals_target | total | 41 | 0 | -41 |
| redundancy.repeated_default | total | 4 | 0 | -4 |
| redundancy.repeated_target | total | 3 | 0 | -3 |
| reference.must_4k_default | total | 7 | 10 | +3 |
| reference.must_4k_target | total | 7 | 11 | +4 |
| reference.should_4k_target | total | 8 | 10 | +2 |
| reference.should_8k_default | total | 9 | 12 | +3 |
| reference.should_8k_target | total | 8 | 14 | +6 |
| reference.should_full_default | total | 11 | 13 | +2 |
| reference.should_full_target | total | 10 | 14 | +4 |
| invariance.noise | adventureworks | 2 | 1 | -1 |
| invariance.reverse | bike-sharing-hourly | 27 | 15 | -12 |
| invariance.row_shuffle | thyroid | 23 | 15 | -8 |
| invariance.total | bike-sharing-hourly | 32 | 20 | -12 |
| invariance.total | thyroid | 40 | 32 | -8 |
| permuted.false_4k | adventureworks | 40 | 11 | -29 |
| permuted.false_4k | bike-sharing-hourly | 10 | 7 | -3 |
| permuted.false_4k | metro-interstate-traffic | 9 | 8 | -1 |
| permuted.false_4k | state-unemployment | 16 | 14 | -2 |
| permuted.false_4k | state-unemployment-long | 4 | 2 | -2 |
| permuted.false_4k | thyroid | 32 | 9 | -23 |
| permuted.false_4k_target | bike-sharing-hourly | 10 | 7 | -3 |
| permuted.false_4k_target | metro-interstate-traffic | 9 | 8 | -1 |
| permuted.false_4k_target | thyroid | 32 | 9 | -23 |
| planted.found_4k | abalone | 3 | 4 | +1 |
| planted.found_4k | absenteeism-at-work | 1 | 2 | +1 |
| planted.found_4k | adult-census-income | 2 | 4 | +2 |
| planted.found_4k | bike-sharing-hourly | 2 | 3 | +1 |
| planted.found_4k | hospital-dha | 1 | 3 | +2 |
| planted.found_4k | metro-interstate-traffic | 3 | 5 | +2 |
| planted.found_full | adult-census-income | 4 | 5 | +1 |
| redundancy.finding_permille_default | abalone | 627 | 918 | +291 |
| redundancy.finding_permille_default | absenteeism-at-work | 388 | 896 | +508 |
| redundancy.finding_permille_default | adult-census-income | 428 | 889 | +461 |
| redundancy.finding_permille_default | adventureworks | 463 | 832 | +369 |
| redundancy.finding_permille_default | bank-marketing | 591 | 891 | +300 |
| redundancy.finding_permille_default | bike-sharing-hourly | 544 | 840 | +296 |
| redundancy.finding_permille_default | congressional-voting | 581 | 905 | +324 |
| redundancy.finding_permille_default | hospital-dha | 555 | 888 | +333 |
| redundancy.finding_permille_default | metro-interstate-traffic | 590 | 844 | +254 |
| redundancy.finding_permille_default | state-unemployment | 551 | 849 | +298 |
| redundancy.finding_permille_default | state-unemployment-long | 553 | 559 | +6 |
| redundancy.finding_permille_default | thyroid | 589 | 864 | +275 |
| redundancy.finding_permille_target | abalone | 655 | 909 | +254 |
| redundancy.finding_permille_target | absenteeism-at-work | 418 | 896 | +478 |
| redundancy.finding_permille_target | adult-census-income | 455 | 880 | +425 |
| redundancy.finding_permille_target | bank-marketing | 614 | 878 | +264 |
| redundancy.finding_permille_target | bike-sharing-hourly | 578 | 835 | +257 |
| redundancy.finding_permille_target | congressional-voting | 600 | 895 | +295 |
| redundancy.finding_permille_target | metro-interstate-traffic | 612 | 844 | +232 |
| redundancy.finding_permille_target | state-unemployment-long | 0 | 559 | +559 |
| redundancy.finding_permille_target | thyroid | 599 | 864 | +265 |
| redundancy.long_decimals_default | abalone | 3 | 0 | -3 |
| redundancy.long_decimals_default | absenteeism-at-work | 1 | 0 | -1 |
| redundancy.long_decimals_default | adult-census-income | 7 | 0 | -7 |
| redundancy.long_decimals_default | adventureworks | 3 | 0 | -3 |
| redundancy.long_decimals_default | bank-marketing | 7 | 0 | -7 |
| redundancy.long_decimals_default | bike-sharing-hourly | 3 | 0 | -3 |
| redundancy.long_decimals_default | hospital-dha | 4 | 0 | -4 |
| redundancy.long_decimals_default | metro-interstate-traffic | 5 | 0 | -5 |
| redundancy.long_decimals_default | nab-nyc-taxi | 4 | 0 | -4 |
| redundancy.long_decimals_default | state-unemployment | 5 | 0 | -5 |
| redundancy.long_decimals_default | state-unemployment-long | 4 | 0 | -4 |
| redundancy.long_decimals_default | thyroid | 7 | 0 | -7 |
| redundancy.long_decimals_target | abalone | 3 | 0 | -3 |
| redundancy.long_decimals_target | absenteeism-at-work | 1 | 0 | -1 |
| redundancy.long_decimals_target | adult-census-income | 10 | 0 | -10 |
| redundancy.long_decimals_target | bank-marketing | 7 | 0 | -7 |
| redundancy.long_decimals_target | bike-sharing-hourly | 5 | 0 | -5 |
| redundancy.long_decimals_target | congressional-voting | 3 | 0 | -3 |
| redundancy.long_decimals_target | metro-interstate-traffic | 5 | 0 | -5 |
| redundancy.long_decimals_target | thyroid | 7 | 0 | -7 |
| redundancy.repeated_default | congressional-voting | 1 | 0 | -1 |
| redundancy.repeated_default | nab-nyc-taxi | 2 | 0 | -2 |
| redundancy.repeated_default | thyroid | 1 | 0 | -1 |
| redundancy.repeated_target | abalone | 1 | 0 | -1 |
| redundancy.repeated_target | congressional-voting | 1 | 0 | -1 |
| redundancy.repeated_target | thyroid | 1 | 0 | -1 |
| reference.must_4k_default | adult-census-income | 2 | 4 | +2 |
| reference.must_4k_default | bank-marketing | 1 | 2 | +1 |
| reference.must_4k_target | abalone | 2 | 3 | +1 |
| reference.must_4k_target | adult-census-income | 3 | 4 | +1 |
| reference.must_4k_target | bank-marketing | 1 | 2 | +1 |
| reference.must_4k_target | bike-sharing-hourly | 1 | 2 | +1 |
| reference.should_4k_default | adult-census-income | 1 | 2 | +1 |
| reference.should_4k_target | adult-census-income | 1 | 2 | +1 |
| reference.should_8k_default | adult-census-income | 2 | 3 | +1 |
| reference.should_8k_target | adult-census-income | 1 | 3 | +2 |
| reference.should_8k_target | bank-marketing | 2 | 3 | +1 |
| reference.should_8k_target | bike-sharing-hourly | 3 | 4 | +1 |
| reference.should_full_target | adult-census-income | 1 | 3 | +2 |

### Worse

| metric | where | before | after | change |
|---|---|---:|---:|---:|
| invariance.noise | total | 30 | 38 | +8 |
| invariance.reverse | total | 269 | 279 | +10 |
| invariance.row_shuffle | total | 104 | 105 | +1 |
| invariance.scale | total | 5 | 8 | +3 |
| invariance.total | total | 408 | 430 | +22 |
| permuted.false_full_target | total | 75 | 79 | +4 |
| planted.found_full | total | 34 | 33 | -1 |
| reference.should_4k_default | total | 7 | 6 | -1 |
| invariance.noise | absenteeism-at-work | 2 | 9 | +7 |
| invariance.noise | adult-census-income | 7 | 9 | +2 |
| invariance.reverse | adult-census-income | 13 | 16 | +3 |
| invariance.reverse | adventureworks | 28 | 41 | +13 |
| invariance.reverse | state-unemployment | 66 | 72 | +6 |
| invariance.row_shuffle | adult-census-income | 9 | 12 | +3 |
| invariance.row_shuffle | bank-marketing | 19 | 25 | +6 |
| invariance.scale | adult-census-income | 1 | 3 | +2 |
| invariance.scale | bank-marketing | 2 | 3 | +1 |
| invariance.total | absenteeism-at-work | 68 | 75 | +7 |
| invariance.total | adult-census-income | 30 | 40 | +10 |
| invariance.total | adventureworks | 30 | 42 | +12 |
| invariance.total | bank-marketing | 44 | 51 | +7 |
| invariance.total | state-unemployment | 66 | 72 | +6 |
| permuted.false_4k | absenteeism-at-work | 4 | 5 | +1 |
| permuted.false_4k | congressional-voting | 1 | 2 | +1 |
| permuted.false_4k_target | absenteeism-at-work | 4 | 5 | +1 |
| permuted.false_4k_target | congressional-voting | 1 | 2 | +1 |
| permuted.false_4k_target | state-unemployment-long | 0 | 2 | +2 |
| permuted.false_full_target | state-unemployment-long | 0 | 4 | +4 |
| planted.found_full | absenteeism-at-work | 4 | 3 | -1 |
| planted.found_full | hospital-dha | 4 | 3 | -1 |
| redundancy.finding_permille_default | nab-nyc-taxi | 658 | 449 | -209 |
| reference.should_4k_default | abalone | 2 | 1 | -1 |
| reference.should_4k_default | bank-marketing | 1 | 0 | -1 |
| reference.should_4k_default | bike-sharing-hourly | 3 | 1 | -2 |
| reference.should_4k_target | abalone | 2 | 1 | -1 |

### Worse and accepted

None.

## Every total metric, last 5 changes

Oldest on the left. A value in a column is followed by its movement against the column before it.

| metric | reading | `1618886` | `ca275fd` | `daf7a79` | `2601a72` | `064132d` |
|---|---|---:|---:|---:|---:|---:|
| planted.planted | information only | 46 | 46 | 46 | 46 | 46 |
| planted.found_full | higher is better | 34 | 34 | 34 | 34 | 33 (-1, worse) |
| planted.found_4k | higher is better | 20 | 20 | 20 | 20 | 29 (+9, better) |
| planted.line_sum_4k | information only | 253 | 253 | 253 | 253 | 389 (+136) |
| permuted.false_full | lower is better | 144 | 144 | 144 | 144 | 144 |
| permuted.false_4k | lower is better | 136 | 136 | 136 | 136 | 78 (-58, better) |
| permuted.false_full_target | lower is better | 75 | 75 | 75 | 75 | 79 (+4, worse) |
| permuted.false_4k_target | lower is better | 67 | 67 | 67 | 67 | 44 (-23, better) |
| permuted.findings_full | information only | 333 | 333 | 333 | 333 | 333 |
| invariance.reverse | lower is better | 269 | 269 | 269 | 269 | 279 (+10, worse) |
| invariance.row_shuffle | lower is better | 104 | 104 | 104 | 104 | 105 (+1, worse) |
| invariance.rename | lower is better | 0 | 0 | 0 | 0 | 0 |
| invariance.noise | lower is better | 30 | 30 | 30 | 30 | 38 (+8, worse) |
| invariance.scale | lower is better | 5 | 5 | 5 | 5 | 8 (+3, worse) |
| invariance.total | lower is better | 408 | 408 | 408 | 408 | 430 (+22, worse) |
| reference.check_failed | lower is better | 0 | 0 | 0 | 0 | 0 |
| reference.must_total | information only | 11 | 11 | 11 | 11 | 12 (+1) |
| reference.must_4k_default | higher is better | 7 | 7 | 7 | 7 | 10 (+3, better) |
| reference.must_8k_default | higher is better | 11 | 11 | 11 | 11 | 11 |
| reference.must_full_default | higher is better | 11 | 11 | 11 | 11 | 11 |
| reference.must_4k_target | higher is better | 7 | 7 | 7 | 7 | 11 (+4, better) |
| reference.must_8k_target | higher is better | 11 | 11 | 11 | 11 | 11 |
| reference.must_full_target | higher is better | 11 | 11 | 11 | 11 | 11 |
| reference.should_total | information only | 12 | 12 | 12 | 12 | 16 (+4) |
| reference.should_4k_default | higher is better | 7 | 7 | 7 | 7 | 6 (-1, worse) |
| reference.should_8k_default | higher is better | 9 | 9 | 9 | 9 | 12 (+3, better) |
| reference.should_full_default | higher is better | 11 | 11 | 11 | 11 | 13 (+2, better) |
| reference.should_4k_target | higher is better | 8 | 8 | 8 | 8 | 10 (+2, better) |
| reference.should_8k_target | higher is better | 8 | 8 | 8 | 8 | 14 (+6, better) |
| reference.should_full_target | higher is better | 10 | 10 | 10 | 10 | 14 (+4, better) |
| redundancy.repeated_default | lower is better | 4 | 4 | 4 | 4 | 0 (-4, better) |
| redundancy.finding_lines_default | information only | 111 | 111 | 111 | 111 | 223 (+112) |
| redundancy.finding_permille_default | higher is better | 7118 | 7118 | 7118 | 7118 | 10624 (+3506, better) |
| redundancy.long_decimals_default | lower is better | 53 | 53 | 53 | 53 | 0 (-53, better) |
| redundancy.repeated_target | lower is better | 3 | 3 | 3 | 3 | 0 (-3, better) |
| redundancy.finding_lines_target | information only | 69 | 69 | 69 | 69 | 162 (+93) |
| redundancy.finding_permille_target | higher is better | 4531 | 4531 | 4531 | 4531 | 7560 (+3029, better) |
| redundancy.long_decimals_target | lower is better | 41 | 41 | 41 | 41 | 0 (-41, better) |
| truth.mismatch | lower is better | 0 | 0 | 0 | 0 | 0 |
| truth.error | lower is better | 0 | 0 | 0 | 0 | 0 |
| truth.ok | information only | 858 | 858 | 858 | 858 | 836 (-22) |
| truth.not_checkable | information only | 1423 | 1423 | 1423 | 1423 | 1440 (+17) |
| cost.profile_seconds | lower is better | 6.859 | 13.385 | 12.325 | 12.333 (+0.008) | 12.396 (+0.063) |
| cost.json_chars | information only | 3335873 | 3335878 (+5) | 3335878 | 3335878 | 3366352 (+30474) |
| cost.findings | information only | 1089 | 1089 | 1089 | 1089 | 1089 |

## Latest scorecard per dataset

| dataset | kind | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| adult-census-income | tabular, mixed types | 4 | 4 | 4 | 2 | 40 | 0 | 0 | 1.732 |
| bank-marketing | tabular, mixed types | 2 | 2 | 2 | 8 | 51 | 0 | 0 | 1.745 |
| abalone | tabular, correlated measures | 4 | 2 | 3 | 1 | 13 | 0 | 0 | 0.184 |
| bike-sharing-hourly | hourly series with calendar columns | 3 | 2 | 2 | 7 | 20 | 0 | 0 | 1.567 |
| metro-interstate-traffic | hourly time series | 5 | - | - | 8 | 7 | 0 | 0 | 1.481 |
| thyroid | clinical, many flags and missing values | 1 | - | - | 9 | 32 | 0 | 0 | 0.826 |
| absenteeism-at-work | small business table | 2 | - | - | 5 | 75 | 0 | 0 | 0.238 |
| hospital-dha | wide numeric, 64 columns | 3 | - | - | 9 | 40 | 0 | 0 | 0.719 |
| congressional-voting | categorical votes | 1 | - | - | 2 | 38 | 0 | 0 | 0.048 |
| adventureworks | sales orders | 1 | - | - | 11 | 42 | 0 | 0 | 2.448 |
| nab-nyc-taxi | single time series | 3 | - | - | 0 | 0 | 0 | 0 | 0.046 |
| state-unemployment | wide time series, one column per state | 0 | - | - | 14 | 72 | 0 | 0 | 1.284 |
| state-unemployment-long | panel, long form | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0.078 |
| south-german-credit (held out) | credit table | 4 | - | - | 7 | 13 | 0 | 0 | 0.174 |
| soybean (held out) | categorical agronomy table | 0 | - | - | 4 | 54 | 0 | 0 | 0.205 |
| lace-satellites (held out) | satellite catalogue | 1 | - | - | 7 | 79 | 0 | 0 | 0.239 |
| sec-edgar-company-facts (held out) | event log, filing events | 1 | 3 | 3 | 1 | 36 | 0 | 0 | 0.501 |
| gse15258-expression (held out) | wide table, 86 samples x 5,006 columns | - | 0 | 0 | - | - | - | - | - |

## Held-out datasets

These datasets are scored and left out of the pass/fail comparison. A gain on the gated datasets that these do not share points to tuning on the gated datasets.

| date | commit | change | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 2026-11-16 | `064132d` | Merge pull request #334 from fcela/feat/317-digest-briefing | 6 (+3, better) | 3 | 3 | 19 (-4, better) | 182 (+44, worse) | 0 (-1, better) | 0 | 1.119 (+0.503) |
| 2026-11-16 | `2601a72` | Merge pull request #326 from fcela/feat/gate-report-tldr | 3 | - | - | 23 | 138 | 1 | 0 | 0.616 (+0.002) |
| 2026-11-15 | `daf7a79` | Merge pull request #324 from fcela/feat/315-corpus-targets | 3 | - | - | 23 | 138 | 1 | 0 | 0.614 |
| 2026-11-15 | `ca275fd` | Merge pull request #323 from fcela/feat/317-gate-history-report | 3 | - | - | 23 | 138 | 1 | 0 | 0.626 |
| 2026-11-15 | `1618886` | Merge pull request #320 from fcela/fix/295-agent-token-economy | 3 | - | - | 23 | 138 | 1 | 0 | 0.327 |

## Platforms

- Linux-7.0.0-1014-aws-x86_64-with-glibc2.39
- Linux-7.0.0-1017-azure-x86_64-with-glibc2.39
- macOS-26.3-arm64-arm-64bit-Mach-O
