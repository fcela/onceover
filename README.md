# Gate report

## TL;DR

Latest change `3889755` (2026-11-17), Merge pull request #367 from fcela/feat/310-pointed-mode: compared with `34718c6`, 10 total metrics better, 0 worse, 1 worse and accepted.

| measure | latest (`3889755`) | previous (`34718c6`) | first recorded (`1618886`) |
|---|---:|---:|---:|
| Must-tier reference facts stated in the 4,000-character digest | 16 of 19 (unchanged) | 16 of 19 | 7 of 11 |
| Must-tier reference facts stated in the 4,000-character digest, target mode | 17 of 19 (+1, better) | 16 of 19 | 7 of 11 |
| Should-tier reference facts stated in the 4,000-character digest | 29 of 45 (unchanged) | 29 of 45 | 7 of 12 |
| Should-tier reference facts stated in the 4,000-character digest, target mode | 18 of 45 (+2, better) | 16 of 45 | 8 of 12 |
| Planted structures stated in the 4,000-character digest | 56 (+1, better) | 55 | 20 |
| Facts repeated in the 4,000-character digest | 0 (unchanged) | 0 | 4 |
| Facts repeated in the 4,000-character digest, target mode | 0 (unchanged) | 0 | 3 |
| Relationship statements in the 4,000-character digest of shuffled tables (all false by construction) | 48 (unchanged) | 48 | 136 |
| Statements that fail verification against the data | 0 (unchanged) | 0 | 0 |

Biggest improvements: profile seconds 14.906 → 11.762; Should-tier reference facts stated in the 4,000-character digest, target mode 16 → 18; reference should 8k target 21 → 23
Biggest regressions: none.
Accepted regressions: planted found full 59 → 58 (Issue #310: the single absenteeism-at-work displacement above; every other dataset's found_full is unchanged or improved, and found_4k total improves. Record: docs/benchmarks/2026-11-17-pointed-mode.md.)

Hard lines: none breached — no statement failed or errored in verification, and no must-tier reference fact was lost from the 4,000-character digest.

22 change(s) recorded. Latest: `3889755` (2026-11-17), onceover 0.1.6, seeds [0].

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
| 2026-11-17 | `3889755` | Merge pull request #367 from fcela/feat/310-pointed-mode | 56 (+1, better) | 16 | 17 (+1, better) | 48 | 336 | 0 | 0 | 11.762 (-3.144, better) |
| 2026-11-17 | `34718c6` | Merge pull request #369 from fcela/fix/365-views-integrity | 55 | 16 | 16 | 48 | 336 | 0 | 0 | 14.906 |
| 2026-11-17 | `a7994e0` | Merge pull request #368 from fcela/fix/365-exact-decimals | 55 | 16 | 16 | 48 | 336 | 0 | 0 | 10.86 |
| 2026-11-17 | `3c2db24` | Merge pull request #366 from fcela/fix/365-fk-chance | 55 | 16 | 16 | 48 | 336 | 0 | 0 | 11.494 |
| 2026-11-17 | `7d27ac8` | Merge pull request #355 from fcela/docs/347-action-value | 55 | 16 | 16 | 48 | 336 | 0 | 0 | 10.75 |
| 2026-11-17 | `260b1c4` | Merge pull request #354 from fcela/docs/353-finding-value | 55 | 16 | 16 | 48 | 336 | 0 | 0 | 8.36 |
| 2026-11-17 | `526507a` | Merge pull request #358 from fcela/fix/ci-history-job | 55 | 16 | 16 | 48 | 336 | 0 | 0 | 10.716 (-0.13) |
| 2026-11-17 | `d057860` | Merge pull request #357 from fcela/release/0.1.6-prep | 55 | 16 | 16 | 48 | 336 | 0 | 0 | 10.846 |
| 2026-11-17 | `39741ac` | Merge pull request #356 from fcela/docs/audit-2026-11-17 | 55 | 16 | 16 | 48 | 336 | 0 | 0 | 14.337 (+4.168, worse) |
| 2026-11-17 | `52ed9d4` | Merge pull request #351 from fcela/feat/348-uci-expansion | 55 | 16 | 16 | 48 | 336 | 0 | 0 | 10.169 (+0.131) |
| 2026-11-17 | `cec0278` | Merge pull request #352 from fcela/docs/347-interestingness-study | 55 | 16 | 16 | 48 | 336 | 0 | 0 | 10.038 |
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

## Latest change: `3889755` Merge pull request #367 from fcela/feat/310-pointed-mode

Compared with `34718c6`: 10 total metrics better, 0 worse, 1 worse and accepted. Rows marked `total` sum the gated datasets; the other rows name one dataset.

### Better

| metric | where | before | after | change |
|---|---|---:|---:|---:|
| cost.profile_seconds | total | 14.906 | 11.762 | -3.144 |
| planted.found_4k | total | 55 | 56 | +1 |
| redundancy.finding_permille_default | total | 16634 | 16636 | +2 |
| redundancy.finding_permille_target | total | 9404 | 9406 | +2 |
| reference.must_4k_target | total | 16 | 17 | +1 |
| reference.must_8k_target | total | 16 | 17 | +1 |
| reference.should_4k_target | total | 16 | 18 | +2 |
| reference.should_8k_default | total | 35 | 36 | +1 |
| reference.should_8k_target | total | 21 | 23 | +2 |
| reference.should_full_target | total | 22 | 24 | +2 |
| planted.found_4k | abalone | 4 | 5 | +1 |
| planted.found_4k | bank-marketing | 2 | 3 | +1 |
| redundancy.finding_permille_default | bank-marketing | 887 | 888 | +1 |
| redundancy.finding_permille_default | bike-sharing-hourly | 839 | 840 | +1 |
| redundancy.finding_permille_target | abalone | 918 | 920 | +2 |
| redundancy.finding_permille_target | bike-sharing-hourly | 838 | 839 | +1 |
| redundancy.finding_permille_target | congressional-voting | 901 | 902 | +1 |
| redundancy.finding_permille_target | state-unemployment-long | 559 | 611 | +52 |
| reference.must_4k_target | planted_mix | 2 | 3 | +1 |
| reference.must_8k_target | planted_mix | 2 | 3 | +1 |
| reference.should_4k_target | state-unemployment-long | 2 | 4 | +2 |
| reference.should_8k_default | bank-marketing | 3 | 4 | +1 |
| reference.should_8k_target | state-unemployment-long | 2 | 4 | +2 |
| reference.should_full_target | state-unemployment-long | 2 | 4 | +2 |

### Worse

None.

### Worse and accepted

| metric | where | before | after | change | reason |
|---|---|---:|---:|---:|---|
| planted.found_full | total | 59 | 58 | -1 | Issue #310: the single absenteeism-at-work displacement above; every other dataset's found_full is unchanged or improved, and found_4k total improves. Record: docs/benchmarks/2026-11-17-pointed-mode.md. |
| planted.found_4k | absenteeism-at-work | 3 | 2 | -1 | Issue #310: same displacement as planted.found_full on this dataset; the shape statement is the digest's marginal line at the 4,000-character budget. Record: docs/benchmarks/2026-11-17-pointed-mode.md. |
| planted.found_full | absenteeism-at-work | 4 | 3 | -1 | Issue #310: the outcome account and contrasts on the planted target joined a report already saturated at the 100-finding cap (191 functional statements past their group cap alone); the target_driver shape statement (interestingness 0.6289) falls just below the head selection cut (0.6387). found_4k total improves 55 -> 56 across the gate. Record: docs/benchmarks/2026-11-17-pointed-mode.md. |
| redundancy.finding_permille_target | absenteeism-at-work | 902 | 891 | -11 | Issue #310: same composition change; 902 -> 891 (1.2%), total rose. Record: docs/benchmarks/2026-11-17-pointed-mode.md. |
| redundancy.finding_permille_target | bank-marketing | 883 | 873 | -10 | Issue #310: the target section's composition changed (the outcome account and per-level outcome contrasts are longer lines); finding-char density moved 883 -> 873 (1.1%) while the section gained the outcome's own statements and the total rose. Record: docs/benchmarks/2026-11-17-pointed-mode.md. |
| redundancy.finding_permille_target | dominant_driver | 891 | 882 | -9 | Issue #310: same composition change; 891 -> 882 (1.0%), total rose. Record: docs/benchmarks/2026-11-17-pointed-mode.md. |
| redundancy.finding_permille_target | metro-interstate-traffic | 846 | 832 | -14 | Issue #310: same composition change as bank-marketing; 846 -> 832 (1.7%), total rose. Record: docs/benchmarks/2026-11-17-pointed-mode.md. |
| redundancy.finding_permille_target | planted_mix | 913 | 906 | -7 | Issue #310: same composition change; 913 -> 906 (0.8%), total rose. Record: docs/benchmarks/2026-11-17-pointed-mode.md. |
| redundancy.finding_permille_target | thyroid | 874 | 871 | -3 | Issue #310: same composition change; 874 -> 871 (0.3%), total rose. Record: docs/benchmarks/2026-11-17-pointed-mode.md. |

## Every total metric, last 8 changes

Oldest on the left. A value in a column is followed by its movement against the column before it.

| metric | reading | `d057860` | `526507a` | `260b1c4` | `7d27ac8` | `3c2db24` | `a7994e0` | `34718c6` | `3889755` |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| planted.planted | information only | 76 | 76 | 76 | 76 | 76 | 76 | 76 | 76 |
| planted.found_full | higher is better | 59 | 59 | 59 | 59 | 59 | 59 | 59 | 58 (-1, worse) |
| planted.found_4k | higher is better | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 56 (+1, better) |
| planted.line_sum_4k | information only | 621 | 621 | 621 | 621 | 621 | 621 | 621 | 630 (+9) |
| permuted.false_full | lower is better | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 |
| permuted.false_4k | lower is better | 48 | 48 | 48 | 48 | 48 | 48 | 48 | 48 |
| permuted.false_full_target | lower is better | 51 | 51 | 51 | 51 | 51 | 51 | 51 | 51 |
| permuted.false_4k_target | lower is better | 30 | 30 | 30 | 30 | 30 | 30 | 30 | 30 |
| permuted.findings_full | information only | 318 | 318 | 318 | 318 | 318 | 318 | 318 | 318 |
| invariance.reverse | lower is better | 199 | 199 | 199 | 199 | 199 | 199 | 199 | 199 |
| invariance.row_shuffle | lower is better | 106 | 106 | 106 | 106 | 106 | 106 | 106 | 106 |
| invariance.rename | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| invariance.noise | lower is better | 29 | 29 | 29 | 29 | 29 | 29 | 29 | 29 |
| invariance.scale | lower is better | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 |
| invariance.total | lower is better | 336 | 336 | 336 | 336 | 336 | 336 | 336 | 336 |
| reference.check_failed | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_total | information only | 19 | 19 | 19 | 19 | 19 | 19 | 19 | 19 |
| reference.must_4k_default | higher is better | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 16 |
| reference.must_8k_default | higher is better | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 16 |
| reference.must_full_default | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_4k_target | higher is better | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 17 (+1, better) |
| reference.must_8k_target | higher is better | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 17 (+1, better) |
| reference.must_full_target | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.should_total | information only | 45 | 45 | 45 | 45 | 45 | 45 | 45 | 45 |
| reference.should_4k_default | higher is better | 29 | 29 | 29 | 29 | 29 | 29 | 29 | 29 |
| reference.should_8k_default | higher is better | 35 | 35 | 35 | 35 | 35 | 35 | 35 | 36 (+1, better) |
| reference.should_full_default | higher is better | 37 | 37 | 37 | 37 | 37 | 37 | 37 | 37 |
| reference.should_4k_target | higher is better | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 18 (+2, better) |
| reference.should_8k_target | higher is better | 21 | 21 | 21 | 21 | 21 | 21 | 21 | 23 (+2, better) |
| reference.should_full_target | higher is better | 22 | 22 | 22 | 22 | 22 | 22 | 22 | 24 (+2, better) |
| redundancy.repeated_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.finding_lines_default | information only | 272 | 272 | 272 | 272 | 272 | 272 | 272 | 272 |
| redundancy.finding_permille_default | higher is better | 16634 | 16634 | 16634 | 16634 | 16634 | 16634 | 16634 | 16636 (+2, better) |
| redundancy.long_decimals_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.repeated_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.finding_lines_target | information only | 181 | 181 | 181 | 181 | 181 | 181 | 181 | 193 (+12) |
| redundancy.finding_permille_target | higher is better | 9404 | 9404 | 9404 | 9404 | 9404 | 9404 | 9404 | 9406 (+2, better) |
| redundancy.long_decimals_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.mismatch | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.error | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.ok | information only | 915 | 915 | 915 | 915 | 915 | 915 | 915 | 907 (-8) |
| truth.not_checkable | information only | 1473 | 1473 | 1473 | 1473 | 1473 | 1473 | 1473 | 1524 (+51) |
| cost.profile_seconds | lower is better | 10.846 | 10.716 (-0.13) | 8.36 | 10.75 | 11.494 | 10.86 | 14.906 | 11.762 (-3.144, better) |
| cost.json_chars | information only | 3874726 | 3874726 | 3874726 | 3874726 | 3874726 | 3874726 | 3874726 | 3880166 (+5440) |
| cost.findings | information only | 1203 | 1203 | 1203 | 1203 | 1203 | 1203 | 1203 | 1203 |
| weighted.must_total | information only | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| weighted.must_4k | higher is better | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 |
| weighted.must_8k | higher is better | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| weighted.must_full | higher is better | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| weighted.should_total | information only | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_4k | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_8k | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_full | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.weighted_findings | information only | 15 | 15 | 15 | 15 | 15 | 15 | 15 | 15 |

## Latest scorecard per dataset

| dataset | kind | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| adult-census-income | tabular, mixed types | 5 | 4 | 4 | 1 | 24 | 0 | 0 | 1.453 |
| bank-marketing | tabular, mixed types | 3 | 2 | 2 | 1 | 35 | 0 | 0 | 1.437 |
| abalone | tabular, correlated measures | 5 | 3 | 3 | 0 | 7 | 0 | 0 | 0.108 |
| bike-sharing-hourly | hourly series with calendar columns | 3 | 2 | 2 | 5 | 0 | 0 | 0 | 1.285 |
| metro-interstate-traffic | hourly time series | 4 | - | - | 7 | 6 | 0 | 0 | 1.452 |
| thyroid | clinical, many flags and missing values | 1 | - | - | 6 | 35 | 0 | 0 | 0.598 |
| absenteeism-at-work | small business table | 2 | - | - | 5 | 64 | 0 | 0 | 0.171 |
| hospital-dha | wide numeric, 64 columns | 3 | - | - | 0 | 23 | 0 | 0 | 0.668 |
| congressional-voting | categorical votes | 1 | - | - | 2 | 60 | 0 | 0 | 0.047 |
| adventureworks | sales orders | 1 | - | - | 11 | 44 | 0 | 0 | 2.653 |
| nab-nyc-taxi | single time series | 3 | - | - | 0 | 0 | 0 | 0 | 0.054 |
| state-unemployment | wide time series, one column per state | 0 | - | - | 4 | 25 | 0 | 0 | 1.222 |
| state-unemployment-long | panel, long form | 0 | 1 | 1 | 2 | 0 | 0 | 0 | 0.081 |
| south-german-credit (held out) | credit table | 4 | 0 | 0 | 3 | 12 | 0 | 0 | 0.136 |
| soybean (held out) | categorical agronomy table | 0 | - | - | 4 | 49 | 0 | 0 | 0.197 |
| lace-satellites (held out) | satellite catalogue | 1 | - | - | 7 | 65 | 0 | 0 | 0.188 |
| sec-edgar-company-facts (held out) | event log, filing events | 1 | 3 | 3 | 1 | 38 | 0 | 0 | 0.376 |
| gse15258-expression (held out) | wide table, 86 samples x 5,006 columns | - | 0 | 0 | - | - | - | - | - |
| column_order_stress | generated numeric battery, latent factors | 0 | 0 | 0 | 0 | 9 | 0 | 0 | 0.147 |
| planted_mix | generated application records with plants | 4 | 2 | 3 | 1 | 1 | 0 | 0 | 0.178 |
| daily_regime_break | generated daily series, planted breaks | 4 | 0 | 0 | 2 | 0 | 0 | 0 | 0.031 |
| dominant_driver | generated listings, numeric target | 5 | 2 | 2 | 0 | 1 | 0 | 0 | 0.091 |
| daily_level_collapse | generated daily series, planted sustained collapse | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0.019 |
| marginal_ranges (held out) | generated range battery | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.025 |
| messy_intake (held out) | generated messy intake records | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 0.012 |
| weighted_noise | generated control: independent noise weight | 4 | - | - | 1 | 1 | 0 | 0 | 0.045 |
| weighted_large_cell | generated control: merely large cell | 4 | - | - | 0 | 1 | 0 | 0 | 0.022 |

## Held-out datasets

These datasets are scored and left out of the pass/fail comparison. A gain on the gated datasets that these do not share points to tuning on the gated datasets.

| date | commit | change | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 2026-11-17 | `3889755` | Merge pull request #367 from fcela/feat/310-pointed-mode | 7 | 4 | 3 | 15 | 166 | 0 | 0 | 0.934 (-0.277) |
| 2026-11-17 | `34718c6` | Merge pull request #369 from fcela/fix/365-views-integrity | 7 | 4 | 3 | 15 | 166 | 0 | 0 | 1.211 |
| 2026-11-17 | `a7994e0` | Merge pull request #368 from fcela/fix/365-exact-decimals | 7 | 4 | 3 | 15 | 166 | 0 | 0 | 0.897 |
| 2026-11-17 | `3c2db24` | Merge pull request #366 from fcela/fix/365-fk-chance | 7 | 4 | 3 | 15 | 166 | 0 | 0 | 0.931 |
| 2026-11-17 | `7d27ac8` | Merge pull request #355 from fcela/docs/347-action-value | 7 | 4 | 3 | 15 | 166 | 0 | 0 | 0.896 |
| 2026-11-17 | `260b1c4` | Merge pull request #354 from fcela/docs/353-finding-value | 7 | 4 | 3 | 15 | 166 | 0 | 0 | 0.688 |
| 2026-11-17 | `526507a` | Merge pull request #358 from fcela/fix/ci-history-job | 7 | 4 | 3 | 15 | 166 | 0 | 0 | 0.881 (-0.01) |
| 2026-11-17 | `d057860` | Merge pull request #357 from fcela/release/0.1.6-prep | 7 | 4 | 3 | 15 | 166 | 0 | 0 | 0.891 |
| 2026-11-17 | `39741ac` | Merge pull request #356 from fcela/docs/audit-2026-11-17 | 7 | 4 | 3 | 15 | 166 | 0 | 0 | 1.171 (+0.444) |
| 2026-11-17 | `52ed9d4` | Merge pull request #351 from fcela/feat/348-uci-expansion | 7 | 4 | 3 | 15 | 166 | 0 | 0 | 0.727 (+0.003) |
| 2026-11-17 | `cec0278` | Merge pull request #352 from fcela/docs/347-interestingness-study | 7 | 4 | 3 | 15 | 166 | 0 | 0 | 0.724 |
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
