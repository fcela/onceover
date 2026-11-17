# Gate report

## TL;DR

Latest change `eef295a` (2026-11-17), Merge pull request #350 from fcela/feat/329-weighted-concentration: compared with `610812e`, 3 total metrics better, 4 worse, 0 worse and accepted.

| measure | latest (`eef295a`) | previous (`610812e`) | first recorded (`1618886`) |
|---|---:|---:|---:|
| Must-tier reference facts stated in the 4,000-character digest | 16 of 19 (unchanged) | 16 of 19 | 7 of 11 |
| Must-tier reference facts stated in the 4,000-character digest, target mode | 16 of 19 (unchanged) | 16 of 19 | 7 of 11 |
| Should-tier reference facts stated in the 4,000-character digest | 29 of 45 (unchanged) | 29 of 45 | 7 of 12 |
| Should-tier reference facts stated in the 4,000-character digest, target mode | 16 of 45 (unchanged) | 16 of 45 | 8 of 12 |
| Planted structures stated in the 4,000-character digest | 55 (+8, better) | 47 | 20 |
| Facts repeated in the 4,000-character digest | 0 (unchanged) | 0 | 4 |
| Facts repeated in the 4,000-character digest, target mode | 0 (unchanged) | 0 | 3 |
| Relationship statements in the 4,000-character digest of shuffled tables (all false by construction) | 48 (+1, worse) | 47 | 136 |
| Statements that fail verification against the data | 0 (unchanged) | 0 | 0 |

Biggest improvements: Planted structures stated in the 4,000-character digest 47 → 55; planted found full 51 → 59; redundancy finding permille default 14962 → 16634
Biggest regressions: invariance noise 27 → 29; Relationship statements in the 4,000-character digest of shuffled tables (all false by construction) 47 → 48; permuted false full 99 → 100
Accepted regressions: none.

Hard lines: none breached — no statement failed or errored in verification, and no must-tier reference fact was lost from the 4,000-character digest.

11 change(s) recorded. Latest: `eef295a` (2026-11-17), onceover 0.1.5, seeds [0].

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
| 2026-11-17 | `eef295a` | Merge pull request #350 from fcela/feat/329-weighted-concentration | 55 (+8, better) | 16 | 16 | 48 (+1, worse) | 336 (+2, worse) | 0 | 0 | 10.723 (+0.239) |
| 2026-11-17 | `610812e` | Merge pull request #345 from fcela/feat/gate-concentration | 47 | 16 (+1, better) | 16 (+1, better) | 47 | 334 (-5, better) | 0 | 0 | 10.484 |
| 2026-11-16 | `956aa92` | Merge pull request #344 from fcela/feat/317-findings-generation | 47 | 15 | 15 | 47 | 339 | 0 | 0 | 10.433 |
| 2026-11-16 | `ce38fe6` | Merge pull request #339 from fcela/fix/317-axis-gated-breaks | 47 (+1, better) | 15 (+1, better) | 15 | 47 (-26, better) | 339 (-107, better) | 0 | 0 | 10.511 |
| 2026-11-16 | `7612842` | Merge pull request #341 from fcela/audit/317-detector-audit | 46 (+17, better) | 14 (+4, better) | 15 (+4, better) | 73 (-5, better) | 446 (+16, worse) | 0 | 0 | 16.266 |
| 2026-11-16 | `40329ad` | Merge pull request #342 from fcela/fix/317-max-findings | 29 | 10 | 11 | 78 | 430 | 0 | 0 | 12.259 (-0.137) |
| 2026-11-16 | `064132d` | Merge pull request #334 from fcela/feat/317-digest-briefing | 29 (+9, better) | 10 (+3, better) | 11 (+4, better) | 78 (-58, better) | 430 (+22, worse) | 0 (-4, better) | 0 | 12.396 (+0.063) |
| 2026-11-16 | `2601a72` | Merge pull request #326 from fcela/feat/gate-report-tldr | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 12.333 (+0.008) |
| 2026-11-15 | `daf7a79` | Merge pull request #324 from fcela/feat/315-corpus-targets | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 12.325 |
| 2026-11-15 | `ca275fd` | Merge pull request #323 from fcela/feat/317-gate-history-report | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 13.385 |
| 2026-11-15 | `1618886` | Merge pull request #320 from fcela/fix/295-agent-token-economy | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 6.859 |

## Latest change: `eef295a` Merge pull request #350 from fcela/feat/329-weighted-concentration

Compared with `610812e`: 3 total metrics better, 4 worse, 0 worse and accepted. Rows marked `total` sum the gated datasets; the other rows name one dataset.

### Better

| metric | where | before | after | change |
|---|---|---:|---:|---:|
| planted.found_4k | total | 47 | 55 | +8 |
| planted.found_full | total | 51 | 59 | +8 |
| redundancy.finding_permille_default | total | 14962 | 16634 | +1672 |

### Worse

| metric | where | before | after | change |
|---|---|---:|---:|---:|
| invariance.noise | total | 27 | 29 | +2 |
| invariance.total | total | 334 | 336 | +2 |
| permuted.false_4k | total | 47 | 48 | +1 |
| permuted.false_full | total | 99 | 100 | +1 |

### Worse and accepted

None.

## Every total metric, last 8 changes

Oldest on the left. A value in a column is followed by its movement against the column before it.

| metric | reading | `2601a72` | `064132d` | `40329ad` | `7612842` | `ce38fe6` | `956aa92` | `610812e` | `eef295a` |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| planted.planted | information only | 46 | 46 | 46 | 65 (+19) | 65 | 65 | 65 | 76 (+11) |
| planted.found_full | higher is better | 34 | 33 (-1, worse) | 33 | 50 (+17, better) | 52 (+2, better) | 52 | 51 (-1, worse) | 59 (+8, better) |
| planted.found_4k | higher is better | 20 | 29 (+9, better) | 29 | 46 (+17, better) | 47 (+1, better) | 47 | 47 | 55 (+8, better) |
| planted.line_sum_4k | information only | 253 | 389 (+136) | 389 | 582 (+193) | 586 (+4) | 586 | 584 (-2) | 621 (+37) |
| permuted.false_full | lower is better | 144 | 144 | 144 | 126 (-18, better) | 99 (-27, better) | 99 | 99 | 100 (+1, worse) |
| permuted.false_4k | lower is better | 136 | 78 (-58, better) | 78 | 73 (-5, better) | 47 (-26, better) | 47 | 47 | 48 (+1, worse) |
| permuted.false_full_target | lower is better | 75 | 79 (+4, worse) | 79 | 64 (-15, better) | 51 (-13, better) | 51 | 51 | 51 |
| permuted.false_4k_target | lower is better | 67 | 44 (-23, better) | 44 | 42 (-2, better) | 30 (-12, better) | 30 | 30 | 30 |
| permuted.findings_full | information only | 333 | 333 | 333 | 340 (+7) | 313 (-27) | 313 | 313 | 318 (+5) |
| invariance.reverse | lower is better | 269 | 279 (+10, worse) | 279 | 285 (+6, worse) | 203 (-82, better) | 203 | 199 (-4, better) | 199 |
| invariance.row_shuffle | lower is better | 104 | 105 (+1, worse) | 105 | 115 (+10, worse) | 109 (-6, better) | 109 | 106 (-3, better) | 106 |
| invariance.rename | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| invariance.noise | lower is better | 30 | 38 (+8, worse) | 38 | 36 (-2, better) | 25 (-11, better) | 25 | 27 (+2, worse) | 29 (+2, worse) |
| invariance.scale | lower is better | 5 | 8 (+3, worse) | 8 | 10 (+2, worse) | 2 (-8, better) | 2 | 2 | 2 |
| invariance.total | lower is better | 408 | 430 (+22, worse) | 430 | 446 (+16, worse) | 339 (-107, better) | 339 | 334 (-5, better) | 336 (+2, worse) |
| reference.check_failed | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_total | information only | 11 | 12 (+1) | 12 | 19 (+7) | 19 | 19 | 19 | 19 |
| reference.must_4k_default | higher is better | 7 | 10 (+3, better) | 10 | 14 (+4, better) | 15 (+1, better) | 15 | 16 (+1, better) | 16 |
| reference.must_8k_default | higher is better | 11 | 11 | 11 | 15 (+4, better) | 15 | 15 | 16 (+1, better) | 16 |
| reference.must_full_default | higher is better | 11 | 11 | 11 | 16 (+5, better) | 16 | 16 | 17 (+1, better) | 17 |
| reference.must_4k_target | higher is better | 7 | 11 (+4, better) | 11 | 15 (+4, better) | 15 | 15 | 16 (+1, better) | 16 |
| reference.must_8k_target | higher is better | 11 | 11 | 11 | 15 (+4, better) | 15 | 15 | 16 (+1, better) | 16 |
| reference.must_full_target | higher is better | 11 | 11 | 11 | 16 (+5, better) | 16 | 16 | 17 (+1, better) | 17 |
| reference.should_total | information only | 12 | 16 (+4) | 16 | 42 (+26) | 42 | 42 | 45 (+3) | 45 |
| reference.should_4k_default | higher is better | 7 | 6 (-1, worse) | 6 | 28 (+22, better) | 29 (+1, better) | 29 | 29 | 29 |
| reference.should_8k_default | higher is better | 9 | 12 (+3, better) | 12 | 33 (+21, better) | 34 (+1, better) | 34 | 35 (+1, better) | 35 |
| reference.should_full_default | higher is better | 11 | 13 (+2, better) | 13 | 35 (+22, better) | 35 | 35 | 37 (+2, better) | 37 |
| reference.should_4k_target | higher is better | 8 | 10 (+2, better) | 10 | 16 (+6, better) | 15 (-1, worse) | 15 | 16 (+1, better) | 16 |
| reference.should_8k_target | higher is better | 8 | 14 (+6, better) | 14 | 20 (+6, better) | 20 | 20 | 21 (+1, better) | 21 |
| reference.should_full_target | higher is better | 10 | 14 (+4, better) | 14 | 20 (+6, better) | 20 | 20 | 22 (+2, better) | 22 |
| redundancy.repeated_default | lower is better | 4 | 0 (-4, better) | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.finding_lines_default | information only | 111 | 223 (+112) | 223 | 284 (+61) | 270 (-14) | 270 | 267 (-3) | 272 (+5) |
| redundancy.finding_permille_default | higher is better | 7118 | 10624 (+3506, better) | 10624 | 14934 (+4310, better) | 14962 (+28, better) | 14962 | 14962 | 16634 (+1672, better) |
| redundancy.long_decimals_default | lower is better | 53 | 0 (-53, better) | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.repeated_target | lower is better | 3 | 0 (-3, better) | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.finding_lines_target | information only | 69 | 162 (+93) | 162 | 189 (+27) | 181 (-8) | 181 | 181 | 181 |
| redundancy.finding_permille_target | higher is better | 4531 | 7560 (+3029, better) | 7560 | 9383 (+1823, better) | 9397 (+14, better) | 9397 | 9404 (+7, better) | 9404 |
| redundancy.long_decimals_target | lower is better | 41 | 0 (-41, better) | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.mismatch | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.error | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.ok | information only | 858 | 836 (-22) | 836 | 852 (+16) | 952 (+100) | 952 | 914 (-38) | 915 (+1) |
| truth.not_checkable | information only | 1423 | 1440 (+17) | 1440 | 1653 (+213) | 1432 (-221) | 1432 | 1469 (+37) | 1473 (+4) |
| cost.profile_seconds | lower is better | 12.333 (+0.008) | 12.396 (+0.063) | 12.259 (-0.137) | 16.266 | 10.511 | 10.433 | 10.484 | 10.723 (+0.239) |
| cost.json_chars | information only | 3335878 | 3366352 (+30474) | 3366352 | 3942609 (+576257) | 3775831 (-166778) | 3775831 | 3839158 (+63327) | 3874726 (+35568) |
| cost.findings | information only | 1089 | 1089 | 1089 | 1288 (+199) | 1182 (-106) | 1182 | 1198 (+16) | 1203 (+5) |
| weighted.must_total | information only | - | - | - | - | - | - | - | 4 |
| weighted.must_4k | higher is better | - | - | - | - | - | - | - | 3 |
| weighted.must_8k | higher is better | - | - | - | - | - | - | - | 4 |
| weighted.must_full | higher is better | - | - | - | - | - | - | - | 4 |
| weighted.should_total | information only | - | - | - | - | - | - | - | 0 |
| weighted.should_4k | higher is better | - | - | - | - | - | - | - | 0 |
| weighted.should_8k | higher is better | - | - | - | - | - | - | - | 0 |
| weighted.should_full | higher is better | - | - | - | - | - | - | - | 0 |
| weighted.weighted_findings | information only | - | - | - | - | - | - | - | 15 |

## Latest scorecard per dataset

| dataset | kind | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| adult-census-income | tabular, mixed types | 5 | 4 | 4 | 1 | 24 | 0 | 0 | 1.349 |
| bank-marketing | tabular, mixed types | 2 | 2 | 2 | 1 | 35 | 0 | 0 | 1.366 |
| abalone | tabular, correlated measures | 4 | 3 | 3 | 0 | 7 | 0 | 0 | 0.1 |
| bike-sharing-hourly | hourly series with calendar columns | 3 | 2 | 2 | 5 | 0 | 0 | 0 | 1.21 |
| metro-interstate-traffic | hourly time series | 4 | - | - | 7 | 6 | 0 | 0 | 1.318 |
| thyroid | clinical, many flags and missing values | 1 | - | - | 6 | 35 | 0 | 0 | 0.563 |
| absenteeism-at-work | small business table | 3 | - | - | 5 | 64 | 0 | 0 | 0.164 |
| hospital-dha | wide numeric, 64 columns | 3 | - | - | 0 | 23 | 0 | 0 | 0.577 |
| congressional-voting | categorical votes | 1 | - | - | 2 | 60 | 0 | 0 | 0.046 |
| adventureworks | sales orders | 1 | - | - | 11 | 44 | 0 | 0 | 2.349 |
| nab-nyc-taxi | single time series | 3 | - | - | 0 | 0 | 0 | 0 | 0.045 |
| state-unemployment | wide time series, one column per state | 0 | - | - | 4 | 25 | 0 | 0 | 1.078 |
| state-unemployment-long | panel, long form | 0 | 1 | 1 | 2 | 0 | 0 | 0 | 0.077 |
| south-german-credit (held out) | credit table | 4 | 0 | 0 | 3 | 12 | 0 | 0 | 0.131 |
| soybean (held out) | categorical agronomy table | 0 | - | - | 4 | 49 | 0 | 0 | 0.198 |
| lace-satellites (held out) | satellite catalogue | 1 | - | - | 7 | 65 | 0 | 0 | 0.183 |
| sec-edgar-company-facts (held out) | event log, filing events | 1 | 3 | 3 | 1 | 38 | 0 | 0 | 0.347 |
| gse15258-expression (held out) | wide table, 86 samples x 5,006 columns | - | 0 | 0 | - | - | - | - | - |
| column_order_stress | generated numeric battery, latent factors | 0 | 0 | 0 | 0 | 9 | 0 | 0 | 0.135 |
| planted_mix | generated application records with plants | 4 | 2 | 2 | 1 | 1 | 0 | 0 | 0.161 |
| daily_regime_break | generated daily series, planted breaks | 4 | 0 | 0 | 2 | 0 | 0 | 0 | 0.026 |
| dominant_driver | generated listings, numeric target | 5 | 2 | 2 | 0 | 1 | 0 | 0 | 0.082 |
| daily_level_collapse | generated daily series, planted sustained collapse | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0.017 |
| marginal_ranges (held out) | generated range battery | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.023 |
| messy_intake (held out) | generated messy intake records | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 0.011 |
| weighted_noise | generated control: independent noise weight | 4 | - | - | 1 | 1 | 0 | 0 | 0.04 |
| weighted_large_cell | generated control: merely large cell | 4 | - | - | 0 | 1 | 0 | 0 | 0.02 |

## Held-out datasets

These datasets are scored and left out of the pass/fail comparison. A gain on the gated datasets that these do not share points to tuning on the gated datasets.

| date | commit | change | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 2026-11-17 | `eef295a` | Merge pull request #350 from fcela/feat/329-weighted-concentration | 7 | 4 | 3 | 15 | 166 | 0 | 0 | 0.893 (+0.011) |
| 2026-11-17 | `610812e` | Merge pull request #345 from fcela/feat/gate-concentration | 7 | 4 | 3 | 15 | 166 | 0 | 0 | 0.882 |
| 2026-11-16 | `956aa92` | Merge pull request #344 from fcela/feat/317-findings-generation | 7 | 4 | 3 | 15 | 166 | 0 | 0 | 0.767 |
| 2026-11-16 | `ce38fe6` | Merge pull request #339 from fcela/fix/317-axis-gated-breaks | 7 | 4 | 3 | 15 (-1, better) | 166 (-17, better) | 0 | 0 | 0.885 |
| 2026-11-16 | `7612842` | Merge pull request #341 from fcela/audit/317-detector-audit | 7 (+1, better) | 4 (+1, better) | 3 | 16 (-3, better) | 183 (+1, worse) | 0 | 0 | 1.284 |
| 2026-11-16 | `40329ad` | Merge pull request #342 from fcela/fix/317-max-findings | 6 | 3 | 3 | 19 | 182 | 0 | 0 | 1.125 (+0.006) |
| 2026-11-16 | `064132d` | Merge pull request #334 from fcela/feat/317-digest-briefing | 6 (+3, better) | 3 | 3 | 19 (-4, better) | 182 (+44, worse) | 0 (-1, better) | 0 | 1.119 (+0.503) |
| 2026-11-16 | `2601a72` | Merge pull request #326 from fcela/feat/gate-report-tldr | 3 | - | - | 23 | 138 | 1 | 0 | 0.616 (+0.002) |
| 2026-11-15 | `daf7a79` | Merge pull request #324 from fcela/feat/315-corpus-targets | 3 | - | - | 23 | 138 | 1 | 0 | 0.614 |
| 2026-11-15 | `ca275fd` | Merge pull request #323 from fcela/feat/317-gate-history-report | 3 | - | - | 23 | 138 | 1 | 0 | 0.626 |
| 2026-11-15 | `1618886` | Merge pull request #320 from fcela/fix/295-agent-token-economy | 3 | - | - | 23 | 138 | 1 | 0 | 0.327 |

## Platforms

- Linux-7.0.0-1014-aws-x86_64-with-glibc2.39
- Linux-7.0.0-1017-azure-x86_64-with-glibc2.39
- macOS-26.3-arm64-arm-64bit-Mach-O
