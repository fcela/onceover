# Gate report

## TL;DR

Latest change `7612842` (2026-11-16), Merge pull request #341 from fcela/audit/317-detector-audit: compared with `40329ad`, 21 total metrics better, 4 worse, 0 worse and accepted.

| measure | latest (`7612842`) | previous (`40329ad`) | first recorded (`1618886`) |
|---|---:|---:|---:|
| Must-tier reference facts stated in the 4,000-character digest | 14 of 19 (+4, better) | 10 of 12 | 7 of 11 |
| Must-tier reference facts stated in the 4,000-character digest, target mode | 15 of 19 (+4, better) | 11 of 12 | 7 of 11 |
| Should-tier reference facts stated in the 4,000-character digest | 28 of 42 (+22, better) | 6 of 16 | 7 of 12 |
| Should-tier reference facts stated in the 4,000-character digest, target mode | 16 of 42 (+6, better) | 10 of 16 | 8 of 12 |
| Planted structures stated in the 4,000-character digest | 46 (+17, better) | 29 | 20 |
| Facts repeated in the 4,000-character digest | 0 (unchanged) | 0 | 4 |
| Facts repeated in the 4,000-character digest, target mode | 0 (unchanged) | 0 | 3 |
| Relationship statements in the 4,000-character digest of shuffled tables (all false by construction) | 73 (-5, better) | 78 | 136 |
| Statements that fail verification against the data | 0 (unchanged) | 0 | 0 |

Biggest improvements: Should-tier reference facts stated in the 4,000-character digest 6 → 28; reference should 8k default 12 → 33; reference should full default 13 → 35
Biggest regressions: invariance scale 8 → 10; invariance row shuffle 105 → 115; facts changed by layout 430 → 446
Accepted regressions: none.

Hard lines: none breached — no statement failed or errored in verification, and no must-tier reference fact was lost from the 4,000-character digest.

7 change(s) recorded. Latest: `7612842` (2026-11-16), onceover 0.1.5, seeds [0].

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
| 2026-11-16 | `7612842` | Merge pull request #341 from fcela/audit/317-detector-audit | 46 (+17, better) | 14 (+4, better) | 15 (+4, better) | 73 (-5, better) | 446 (+16, worse) | 0 | 0 | 16.266 |
| 2026-11-16 | `40329ad` | Merge pull request #342 from fcela/fix/317-max-findings | 29 | 10 | 11 | 78 | 430 | 0 | 0 | 12.259 (-0.137) |
| 2026-11-16 | `064132d` | Merge pull request #334 from fcela/feat/317-digest-briefing | 29 (+9, better) | 10 (+3, better) | 11 (+4, better) | 78 (-58, better) | 430 (+22, worse) | 0 (-4, better) | 0 | 12.396 (+0.063) |
| 2026-11-16 | `2601a72` | Merge pull request #326 from fcela/feat/gate-report-tldr | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 12.333 (+0.008) |
| 2026-11-15 | `daf7a79` | Merge pull request #324 from fcela/feat/315-corpus-targets | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 12.325 |
| 2026-11-15 | `ca275fd` | Merge pull request #323 from fcela/feat/317-gate-history-report | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 13.385 |
| 2026-11-15 | `1618886` | Merge pull request #320 from fcela/fix/295-agent-token-economy | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 6.859 |

## Latest change: `7612842` Merge pull request #341 from fcela/audit/317-detector-audit

Compared with `40329ad`: 21 total metrics better, 4 worse, 0 worse and accepted. Rows marked `total` sum the gated datasets; the other rows name one dataset.

### Better

| metric | where | before | after | change |
|---|---|---:|---:|---:|
| invariance.noise | total | 38 | 36 | -2 |
| permuted.false_4k | total | 78 | 73 | -5 |
| permuted.false_4k_target | total | 44 | 42 | -2 |
| permuted.false_full | total | 144 | 126 | -18 |
| permuted.false_full_target | total | 79 | 64 | -15 |
| planted.found_4k | total | 29 | 46 | +17 |
| planted.found_full | total | 33 | 50 | +17 |
| redundancy.finding_permille_default | total | 10624 | 14934 | +4310 |
| redundancy.finding_permille_target | total | 7560 | 9383 | +1823 |
| reference.must_4k_default | total | 10 | 14 | +4 |
| reference.must_4k_target | total | 11 | 15 | +4 |
| reference.must_8k_default | total | 11 | 15 | +4 |
| reference.must_8k_target | total | 11 | 15 | +4 |
| reference.must_full_default | total | 11 | 16 | +5 |
| reference.must_full_target | total | 11 | 16 | +5 |
| reference.should_4k_default | total | 6 | 28 | +22 |
| reference.should_4k_target | total | 10 | 16 | +6 |
| reference.should_8k_default | total | 12 | 33 | +21 |
| reference.should_8k_target | total | 14 | 20 | +6 |
| reference.should_full_default | total | 13 | 35 | +22 |
| reference.should_full_target | total | 14 | 20 | +6 |
| invariance.noise | absenteeism-at-work | 9 | 6 | -3 |
| invariance.noise | adult-census-income | 9 | 8 | -1 |
| invariance.noise | congressional-voting | 7 | 6 | -1 |
| invariance.reverse | adult-census-income | 16 | 13 | -3 |
| invariance.reverse | bank-marketing | 20 | 17 | -3 |
| invariance.reverse | thyroid | 15 | 12 | -3 |
| invariance.row_shuffle | absenteeism-at-work | 27 | 21 | -6 |
| invariance.row_shuffle | adult-census-income | 12 | 11 | -1 |
| invariance.row_shuffle | bank-marketing | 25 | 21 | -4 |
| invariance.scale | adult-census-income | 3 | 1 | -2 |
| invariance.scale | bank-marketing | 3 | 2 | -1 |
| invariance.total | absenteeism-at-work | 75 | 66 | -9 |
| invariance.total | adult-census-income | 40 | 33 | -7 |
| invariance.total | bank-marketing | 51 | 43 | -8 |
| permuted.false_4k | bank-marketing | 8 | 5 | -3 |
| permuted.false_4k | hospital-dha | 9 | 4 | -5 |
| permuted.false_4k_target | bank-marketing | 8 | 5 | -3 |
| permuted.false_full | absenteeism-at-work | 9 | 8 | -1 |
| permuted.false_full | bank-marketing | 8 | 5 | -3 |
| permuted.false_full | hospital-dha | 9 | 4 | -5 |
| permuted.false_full | thyroid | 33 | 21 | -12 |
| permuted.false_full_target | absenteeism-at-work | 9 | 8 | -1 |
| permuted.false_full_target | bank-marketing | 8 | 5 | -3 |
| permuted.false_full_target | thyroid | 33 | 21 | -12 |
| planted.found_full | absenteeism-at-work | 3 | 4 | +1 |
| redundancy.finding_permille_default | absenteeism-at-work | 896 | 900 | +4 |
| redundancy.finding_permille_default | congressional-voting | 905 | 909 | +4 |
| redundancy.finding_permille_default | thyroid | 864 | 871 | +7 |
| redundancy.finding_permille_target | absenteeism-at-work | 896 | 900 | +4 |
| redundancy.finding_permille_target | congressional-voting | 895 | 902 | +7 |
| redundancy.finding_permille_target | thyroid | 864 | 871 | +7 |
| reference.should_4k_default | bike-sharing-hourly | 1 | 2 | +1 |

### Worse

| metric | where | before | after | change |
|---|---|---:|---:|---:|
| invariance.reverse | total | 279 | 285 | +6 |
| invariance.row_shuffle | total | 105 | 115 | +10 |
| invariance.scale | total | 8 | 10 | +2 |
| invariance.total | total | 430 | 446 | +16 |
| invariance.noise | adventureworks | 1 | 2 | +1 |
| invariance.noise | thyroid | 2 | 4 | +2 |
| invariance.reverse | congressional-voting | 17 | 28 | +11 |
| invariance.row_shuffle | congressional-voting | 14 | 26 | +12 |
| invariance.row_shuffle | thyroid | 15 | 17 | +2 |
| invariance.total | adventureworks | 42 | 43 | +1 |
| invariance.total | congressional-voting | 38 | 60 | +22 |
| invariance.total | thyroid | 32 | 33 | +1 |
| redundancy.finding_permille_default | adventureworks | 832 | 830 | -2 |
| redundancy.finding_permille_default | bank-marketing | 891 | 890 | -1 |
| redundancy.finding_permille_default | bike-sharing-hourly | 840 | 838 | -2 |
| redundancy.finding_permille_target | adult-census-income | 880 | 879 | -1 |
| redundancy.finding_permille_target | bank-marketing | 878 | 876 | -2 |
| reference.should_full_default | bank-marketing | 3 | 2 | -1 |

### Worse and accepted

None.

## Every total metric, last 7 changes

Oldest on the left. A value in a column is followed by its movement against the column before it.

| metric | reading | `1618886` | `ca275fd` | `daf7a79` | `2601a72` | `064132d` | `40329ad` | `7612842` |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| planted.planted | information only | 46 | 46 | 46 | 46 | 46 | 46 | 65 (+19) |
| planted.found_full | higher is better | 34 | 34 | 34 | 34 | 33 (-1, worse) | 33 | 50 (+17, better) |
| planted.found_4k | higher is better | 20 | 20 | 20 | 20 | 29 (+9, better) | 29 | 46 (+17, better) |
| planted.line_sum_4k | information only | 253 | 253 | 253 | 253 | 389 (+136) | 389 | 582 (+193) |
| permuted.false_full | lower is better | 144 | 144 | 144 | 144 | 144 | 144 | 126 (-18, better) |
| permuted.false_4k | lower is better | 136 | 136 | 136 | 136 | 78 (-58, better) | 78 | 73 (-5, better) |
| permuted.false_full_target | lower is better | 75 | 75 | 75 | 75 | 79 (+4, worse) | 79 | 64 (-15, better) |
| permuted.false_4k_target | lower is better | 67 | 67 | 67 | 67 | 44 (-23, better) | 44 | 42 (-2, better) |
| permuted.findings_full | information only | 333 | 333 | 333 | 333 | 333 | 333 | 340 (+7) |
| invariance.reverse | lower is better | 269 | 269 | 269 | 269 | 279 (+10, worse) | 279 | 285 (+6, worse) |
| invariance.row_shuffle | lower is better | 104 | 104 | 104 | 104 | 105 (+1, worse) | 105 | 115 (+10, worse) |
| invariance.rename | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| invariance.noise | lower is better | 30 | 30 | 30 | 30 | 38 (+8, worse) | 38 | 36 (-2, better) |
| invariance.scale | lower is better | 5 | 5 | 5 | 5 | 8 (+3, worse) | 8 | 10 (+2, worse) |
| invariance.total | lower is better | 408 | 408 | 408 | 408 | 430 (+22, worse) | 430 | 446 (+16, worse) |
| reference.check_failed | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_total | information only | 11 | 11 | 11 | 11 | 12 (+1) | 12 | 19 (+7) |
| reference.must_4k_default | higher is better | 7 | 7 | 7 | 7 | 10 (+3, better) | 10 | 14 (+4, better) |
| reference.must_8k_default | higher is better | 11 | 11 | 11 | 11 | 11 | 11 | 15 (+4, better) |
| reference.must_full_default | higher is better | 11 | 11 | 11 | 11 | 11 | 11 | 16 (+5, better) |
| reference.must_4k_target | higher is better | 7 | 7 | 7 | 7 | 11 (+4, better) | 11 | 15 (+4, better) |
| reference.must_8k_target | higher is better | 11 | 11 | 11 | 11 | 11 | 11 | 15 (+4, better) |
| reference.must_full_target | higher is better | 11 | 11 | 11 | 11 | 11 | 11 | 16 (+5, better) |
| reference.should_total | information only | 12 | 12 | 12 | 12 | 16 (+4) | 16 | 42 (+26) |
| reference.should_4k_default | higher is better | 7 | 7 | 7 | 7 | 6 (-1, worse) | 6 | 28 (+22, better) |
| reference.should_8k_default | higher is better | 9 | 9 | 9 | 9 | 12 (+3, better) | 12 | 33 (+21, better) |
| reference.should_full_default | higher is better | 11 | 11 | 11 | 11 | 13 (+2, better) | 13 | 35 (+22, better) |
| reference.should_4k_target | higher is better | 8 | 8 | 8 | 8 | 10 (+2, better) | 10 | 16 (+6, better) |
| reference.should_8k_target | higher is better | 8 | 8 | 8 | 8 | 14 (+6, better) | 14 | 20 (+6, better) |
| reference.should_full_target | higher is better | 10 | 10 | 10 | 10 | 14 (+4, better) | 14 | 20 (+6, better) |
| redundancy.repeated_default | lower is better | 4 | 4 | 4 | 4 | 0 (-4, better) | 0 | 0 |
| redundancy.finding_lines_default | information only | 111 | 111 | 111 | 111 | 223 (+112) | 223 | 284 (+61) |
| redundancy.finding_permille_default | higher is better | 7118 | 7118 | 7118 | 7118 | 10624 (+3506, better) | 10624 | 14934 (+4310, better) |
| redundancy.long_decimals_default | lower is better | 53 | 53 | 53 | 53 | 0 (-53, better) | 0 | 0 |
| redundancy.repeated_target | lower is better | 3 | 3 | 3 | 3 | 0 (-3, better) | 0 | 0 |
| redundancy.finding_lines_target | information only | 69 | 69 | 69 | 69 | 162 (+93) | 162 | 189 (+27) |
| redundancy.finding_permille_target | higher is better | 4531 | 4531 | 4531 | 4531 | 7560 (+3029, better) | 7560 | 9383 (+1823, better) |
| redundancy.long_decimals_target | lower is better | 41 | 41 | 41 | 41 | 0 (-41, better) | 0 | 0 |
| truth.mismatch | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.error | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.ok | information only | 858 | 858 | 858 | 858 | 836 (-22) | 836 | 852 (+16) |
| truth.not_checkable | information only | 1423 | 1423 | 1423 | 1423 | 1440 (+17) | 1440 | 1653 (+213) |
| cost.profile_seconds | lower is better | 6.859 | 13.385 | 12.325 | 12.333 (+0.008) | 12.396 (+0.063) | 12.259 (-0.137) | 16.266 |
| cost.json_chars | information only | 3335873 | 3335878 (+5) | 3335878 | 3335878 | 3366352 (+30474) | 3366352 | 3942609 (+576257) |
| cost.findings | information only | 1089 | 1089 | 1089 | 1089 | 1089 | 1089 | 1288 (+199) |

## Latest scorecard per dataset

| dataset | kind | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| adult-census-income | tabular, mixed types | 4 | 4 | 4 | 2 | 33 | 0 | 0 | 1.787 |
| bank-marketing | tabular, mixed types | 2 | 2 | 2 | 5 | 43 | 0 | 0 | 1.802 |
| abalone | tabular, correlated measures | 4 | 2 | 3 | 1 | 13 | 0 | 0 | 0.183 |
| bike-sharing-hourly | hourly series with calendar columns | 3 | 2 | 2 | 7 | 20 | 0 | 0 | 1.852 |
| metro-interstate-traffic | hourly time series | 5 | - | - | 8 | 7 | 0 | 0 | 1.728 |
| thyroid | clinical, many flags and missing values | 1 | - | - | 9 | 33 | 0 | 0 | 0.835 |
| absenteeism-at-work | small business table | 2 | - | - | 5 | 66 | 0 | 0 | 0.224 |
| hospital-dha | wide numeric, 64 columns | 3 | - | - | 4 | 40 | 0 | 0 | 0.988 |
| congressional-voting | categorical votes | 1 | - | - | 2 | 60 | 0 | 0 | 0.057 |
| adventureworks | sales orders | 1 | - | - | 11 | 43 | 0 | 0 | 3.254 |
| nab-nyc-taxi | single time series | 3 | - | - | 0 | 0 | 0 | 0 | 0.064 |
| state-unemployment | wide time series, one column per state | 0 | - | - | 14 | 72 | 0 | 0 | 1.731 |
| state-unemployment-long | panel, long form | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0.096 |
| south-german-credit (held out) | credit table | 4 | - | - | 4 | 13 | 0 | 0 | 0.164 |
| soybean (held out) | categorical agronomy table | 0 | - | - | 4 | 49 | 0 | 0 | 0.24 |
| lace-satellites (held out) | satellite catalogue | 1 | - | - | 7 | 81 | 0 | 0 | 0.257 |
| sec-edgar-company-facts (held out) | event log, filing events | 1 | 3 | 3 | 1 | 38 | 0 | 0 | 0.475 |
| gse15258-expression (held out) | wide table, 86 samples x 5,006 columns | - | 0 | 0 | - | - | - | - | - |
| column_order_stress | generated numeric battery, latent factors | 0 | 0 | 0 | 0 | 9 | 0 | 0 | 0.55 |
| planted_mix | generated application records with plants | 4 | 2 | 2 | 1 | 2 | 0 | 0 | 0.699 |
| daily_regime_break | generated daily series, planted breaks | 4 | 0 | 0 | 2 | 2 | 0 | 0 | 0.04 |
| dominant_driver | generated listings, numeric target | 5 | 2 | 2 | 0 | 1 | 0 | 0 | 0.352 |
| daily_level_collapse | generated daily series, planted sustained collapse | 4 | 0 | 0 | 0 | 2 | 0 | 0 | 0.024 |
| marginal_ranges (held out) | generated range battery | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.131 |
| messy_intake (held out) | generated messy intake records | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 0.017 |

## Held-out datasets

These datasets are scored and left out of the pass/fail comparison. A gain on the gated datasets that these do not share points to tuning on the gated datasets.

| date | commit | change | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
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
