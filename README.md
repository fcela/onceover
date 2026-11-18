# Gate report

## TL;DR

Latest change `c22795b` (2026-11-18), Merge pull request #456 from fcela/feat/448-entity-fixtures: compared with `cb5da2f`, 0 total metrics better, 0 worse, 0 worse and accepted.

| measure | latest (`c22795b`) | previous (`cb5da2f`) | first recorded (`1618886`) |
|---|---:|---:|---:|
| Must-tier reference facts stated in the 4,000-character digest | 15 of 19 (unchanged) | 15 of 19 | 7 of 11 |
| Must-tier reference facts stated in the 4,000-character digest, target mode | 17 of 19 (unchanged) | 17 of 19 | 7 of 11 |
| Should-tier reference facts stated in the 4,000-character digest | 32 of 45 (unchanged) | 32 of 45 | 7 of 12 |
| Should-tier reference facts stated in the 4,000-character digest, target mode | 20 of 45 (unchanged) | 20 of 45 | 8 of 12 |
| Planted structures stated in the 4,000-character digest | 59 (unchanged) | 59 | 20 |
| Facts repeated in the 4,000-character digest | 0 (unchanged) | 0 | 4 |
| Facts repeated in the 4,000-character digest, target mode | 0 (unchanged) | 0 | 3 |
| Relationship statements in the 4,000-character digest of shuffled tables (all false by construction) | 21 (unchanged) | 21 | 136 |
| Statements that fail verification against the data | 0 (unchanged) | 0 | 0 |

Biggest improvements: none.
Biggest regressions: none.
Accepted regressions: none.

Hard lines: none breached — no statement failed or errored in verification, and no must-tier reference fact was lost from the 4,000-character digest.

38 change(s) recorded. Latest: `c22795b` (2026-11-18), onceover 0.1.6, seeds [0].

Each row is one change on main, newest first. A value is followed, in parentheses, by its movement against the change before it, with `better` or `worse` when the metric has a direction. Seconds are compared only between rows produced on the same platform.

## Headline metrics

- **planted found in 4k digest**: known structures injected into real tables that the 4,000-character digest states (higher is better).
- **must statements in 4k digest**: hand-written reference statements of tier must that the digest states, default mode (higher is better).
- **must statements in 4k digest, target mode**: the same count with the dataset's target designated (higher is better).
- **restatements in the full report**: findings matching a must_not reference statement (restated structure, statistics carrying no information), full report, default mode (lower is better).
- **statements on shuffled tables (4k digest)**: relationship statements in the digest of a table whose columns were shuffled independently, where no relationship exists (lower is better).
- **facts changed by layout**: stated facts lost or gained when columns are reordered, rows shuffled, columns renamed, a noise column added, or a column rescaled (lower is better).
- **repeated facts in 4k digest**: digest lines that restate the columns and relation of an earlier line (lower is better).
- **statement mismatches**: statements whose recount against the data disagrees with the report (lower is better).
- **profile seconds**: summed median profile time (lower is better).

| date | commit | change | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | restatements in the full report | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 2026-11-18 | `c22795b` | Merge pull request #456 from fcela/feat/448-entity-fixtures | 59 | 15 | 17 | 0 | 21 | 130 | 0 | 0 | 16.876 |
| 2026-11-18 | `cb5da2f` | Merge pull request #454 from fcela/feat/449-frame-spec-resolve | 59 | 15 | 17 | 0 | 21 | 130 | 0 | 0 | 15.419 (-0.297) |
| 2026-11-18 | `c20279a` | Merge pull request #453 from fcela/fix/five-immediate-priorities | 59 (+1, better) | 15 | 17 | 0 | 21 (-1, better) | 130 (-109, better) | 0 | 0 | 15.716 (+4.849, worse) |
| 2026-11-18 | `e495b4c` | Merge pull request #452 from fcela/fix/446-defaults-framing | 58 | 15 | 17 | 0 | 22 | 239 (-1, better) | 0 | 0 | 10.867 (+0.077) |
| 2026-11-18 | `3e45788` | Merge pull request #451 from fcela/fix/446-shared-basis | 58 | 15 | 17 | 0 | 22 | 240 | 0 | 0 | 10.79 (-0.093) |
| 2026-11-18 | `df24a40` | Merge pull request #447 from fcela/docs/381-capability-map | 58 | 15 | 17 | 0 | 22 | 240 | 0 | 0 | 10.883 (+0.037) |
| 2026-11-18 | `79208eb` | State how targets move, give models the calendar, drop vacuous and mi… | 58 (+1, better) | 15 (-1, worse) | 17 | 0 | 22 | 240 (+10, worse) | 0 | 0 | 10.846 (+0.026) |
| 2026-11-18 | `e3a2c9c` | Merge pull request #444 from fcela/fix/374-steps-3-5 | 57 (+1, better) | 16 | 17 | 0 (-22, better) | 22 (-24, better) | 230 (-85, better) | 0 | 0 | 10.82 |
| 2026-11-18 | `4a18255` | Merge pull request #443 from fcela/fix/363-sample-caps | 56 | 16 | 17 | 22 | 46 | 315 (-19, better) | 0 | 0 | 10.53 |
| 2026-11-18 | `7cc00d4` | Merge pull request #380 from fcela/fix/374-derived-basis | 56 | 16 | 17 | 22 (-59, better) | 46 (-2, better) | 334 (-3, better) | 0 | 0 | 9.663 (-1.089) |
| 2026-11-18 | `de041e2` | Merge pull request #385 from fcela/fix/378-residual-trend | 56 | 16 | 17 | 81 | 48 | 337 | 0 | 0 | 10.752 (-0.198) |
| 2026-11-17 | `114fff6` | Merge pull request #379 from fcela/docs/book-currency-2026-11-17 | 56 | 16 | 17 | 81 | 48 | 337 (+1, worse) | 0 | 0 | 10.95 |
| 2026-11-17 | `11dfa8e` | Merge pull request #377 from fcela/fix/343-cap-key-registry-guard | 56 | 16 | 17 | 81 | 48 | 336 | 0 | 0 | 11.688 (+0.186) |
| 2026-11-17 | `94f381b` | Merge pull request #376 from fcela/fix/views-deep-look | 56 | 16 | 17 | 81 | 48 | 336 | 0 | 0 | 11.502 (-3.447, better) |
| 2026-11-17 | `227d742` | Merge pull request #375 from fcela/gate/374-precision | 56 | 16 | 17 | 81 | 48 | 336 | 0 | 0 | 14.949 |
| 2026-11-17 | `3ea33b1` | Merge pull request #370 from fcela/fix/audit-followups-2026-11-17 | 56 | 16 | 17 | - | 48 | 336 | 0 | 0 | 10.8 |
| 2026-11-17 | `3889755` | Merge pull request #367 from fcela/feat/310-pointed-mode | 56 (+1, better) | 16 | 17 (+1, better) | - | 48 | 336 | 0 | 0 | 11.762 (-3.144, better) |
| 2026-11-17 | `34718c6` | Merge pull request #369 from fcela/fix/365-views-integrity | 55 | 16 | 16 | - | 48 | 336 | 0 | 0 | 14.906 |
| 2026-11-17 | `a7994e0` | Merge pull request #368 from fcela/fix/365-exact-decimals | 55 | 16 | 16 | - | 48 | 336 | 0 | 0 | 10.86 |
| 2026-11-17 | `3c2db24` | Merge pull request #366 from fcela/fix/365-fk-chance | 55 | 16 | 16 | - | 48 | 336 | 0 | 0 | 11.494 |
| 2026-11-17 | `7d27ac8` | Merge pull request #355 from fcela/docs/347-action-value | 55 | 16 | 16 | - | 48 | 336 | 0 | 0 | 10.75 |
| 2026-11-17 | `260b1c4` | Merge pull request #354 from fcela/docs/353-finding-value | 55 | 16 | 16 | - | 48 | 336 | 0 | 0 | 8.36 |
| 2026-11-17 | `526507a` | Merge pull request #358 from fcela/fix/ci-history-job | 55 | 16 | 16 | - | 48 | 336 | 0 | 0 | 10.716 (-0.13) |
| 2026-11-17 | `d057860` | Merge pull request #357 from fcela/release/0.1.6-prep | 55 | 16 | 16 | - | 48 | 336 | 0 | 0 | 10.846 |
| 2026-11-17 | `39741ac` | Merge pull request #356 from fcela/docs/audit-2026-11-17 | 55 | 16 | 16 | - | 48 | 336 | 0 | 0 | 14.337 (+4.168, worse) |
| 2026-11-17 | `52ed9d4` | Merge pull request #351 from fcela/feat/348-uci-expansion | 55 | 16 | 16 | - | 48 | 336 | 0 | 0 | 10.169 (+0.131) |
| 2026-11-17 | `cec0278` | Merge pull request #352 from fcela/docs/347-interestingness-study | 55 | 16 | 16 | - | 48 | 336 | 0 | 0 | 10.038 |
| 2026-11-17 | `eef295a` | Merge pull request #350 from fcela/feat/329-weighted-concentration | 55 (+8, better) | 16 | 16 | - | 48 (+1, worse) | 336 (+2, worse) | 0 | 0 | 10.723 (+0.239) |
| 2026-11-17 | `610812e` | Merge pull request #345 from fcela/feat/gate-concentration | 47 | 16 (+1, better) | 16 (+1, better) | - | 47 | 334 (-5, better) | 0 | 0 | 10.484 |
| 2026-11-16 | `956aa92` | Merge pull request #344 from fcela/feat/317-findings-generation | 47 | 15 | 15 | - | 47 | 339 | 0 | 0 | 10.433 |
| 2026-11-16 | `ce38fe6` | Merge pull request #339 from fcela/fix/317-axis-gated-breaks | 47 (+1, better) | 15 (+1, better) | 15 | - | 47 (-26, better) | 339 (-107, better) | 0 | 0 | 10.511 |
| 2026-11-16 | `7612842` | Merge pull request #341 from fcela/audit/317-detector-audit | 46 (+17, better) | 14 (+4, better) | 15 (+4, better) | - | 73 (-5, better) | 446 (+16, worse) | 0 | 0 | 16.266 |
| 2026-11-16 | `40329ad` | Merge pull request #342 from fcela/fix/317-max-findings | 29 | 10 | 11 | - | 78 | 430 | 0 | 0 | 12.259 (-0.137) |
| 2026-11-16 | `064132d` | Merge pull request #334 from fcela/feat/317-digest-briefing | 29 (+9, better) | 10 (+3, better) | 11 (+4, better) | - | 78 (-58, better) | 430 (+22, worse) | 0 (-4, better) | 0 | 12.396 (+0.063) |
| 2026-11-16 | `2601a72` | Merge pull request #326 from fcela/feat/gate-report-tldr | 20 | 7 | 7 | - | 136 | 408 | 4 | 0 | 12.333 (+0.008) |
| 2026-11-15 | `daf7a79` | Merge pull request #324 from fcela/feat/315-corpus-targets | 20 | 7 | 7 | - | 136 | 408 | 4 | 0 | 12.325 |
| 2026-11-15 | `ca275fd` | Merge pull request #323 from fcela/feat/317-gate-history-report | 20 | 7 | 7 | - | 136 | 408 | 4 | 0 | 13.385 |
| 2026-11-15 | `1618886` | Merge pull request #320 from fcela/fix/295-agent-token-economy | 20 | 7 | 7 | - | 136 | 408 | 4 | 0 | 6.859 |

## Latest change: `c22795b` Merge pull request #456 from fcela/feat/448-entity-fixtures

Compared with `cb5da2f`: 0 total metrics better, 0 worse, 0 worse and accepted. Rows marked `total` sum the gated datasets; the other rows name one dataset.

### Better

None.

### Worse

None.

### Worse and accepted

None.

## Every total metric, last 8 changes

Oldest on the left. A value in a column is followed by its movement against the column before it.

| metric | reading | `e3a2c9c` | `79208eb` | `df24a40` | `3e45788` | `e495b4c` | `c20279a` | `cb5da2f` | `c22795b` |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| planted.planted | information only | 76 | 76 | 76 | 76 | 76 | 76 | 76 | 76 |
| planted.found_full | higher is better | 59 (+1, better) | 60 (+1, better) | 60 | 60 | 59 (-1, worse) | 60 (+1, better) | 60 | 60 |
| planted.found_4k | higher is better | 57 (+1, better) | 58 (+1, better) | 58 | 58 | 58 | 59 (+1, better) | 59 | 59 |
| planted.line_sum_4k | information only | 635 (+7) | 593 (-42) | 593 | 592 (-1) | 593 (+1) | 606 (+13) | 606 | 606 |
| permuted.false_full | lower is better | 39 (-47, better) | 39 | 39 | 39 | 39 | 38 (-1, better) | 38 | 38 |
| permuted.false_4k | lower is better | 22 (-24, better) | 22 | 22 | 22 | 22 | 21 (-1, better) | 21 | 21 |
| permuted.false_full_target | lower is better | 17 (-30, better) | 17 | 17 | 17 | 17 | 16 (-1, better) | 16 | 16 |
| permuted.false_4k_target | lower is better | 10 (-18, better) | 10 | 10 | 10 | 10 | 9 (-1, better) | 9 | 9 |
| permuted.findings_full | information only | 257 (-47) | 215 (-42) | 215 | 215 | 224 (+9) | 223 (-1) | 223 | 223 |
| invariance.reverse | lower is better | 87 (-98, better) | 91 (+4, worse) | 91 | 91 | 91 | 87 (-4, better) | 87 | 87 |
| invariance.row_shuffle | lower is better | 102 (+4, worse) | 96 (-6, better) | 96 | 96 | 95 (-1, better) | 11 (-84, better) | 11 | 11 |
| invariance.rename | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| invariance.noise | lower is better | 40 (+10, worse) | 52 (+12, worse) | 52 | 52 | 52 | 31 (-21, better) | 31 | 31 |
| invariance.scale | lower is better | 1 (-1, better) | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| invariance.total | lower is better | 230 (-85, better) | 240 (+10, worse) | 240 | 240 | 239 (-1, better) | 130 (-109, better) | 130 | 130 |
| reference.check_failed | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_total | information only | 19 | 19 | 19 | 19 | 19 | 19 | 19 | 19 |
| reference.must_4k_default | higher is better | 16 | 15 (-1, worse) | 15 | 15 | 15 | 15 | 15 | 15 |
| reference.must_8k_default | higher is better | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 16 |
| reference.must_full_default | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_4k_target | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_8k_target | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_full_target | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.should_total | information only | 45 | 45 | 45 | 45 | 45 | 45 | 45 | 45 |
| reference.should_4k_default | higher is better | 32 (+1, better) | 33 (+1, better) | 33 | 33 | 33 | 32 (-1, worse) | 32 | 32 |
| reference.should_8k_default | higher is better | 35 (-1, worse) | 35 | 35 | 35 | 35 | 35 | 35 | 35 |
| reference.should_full_default | higher is better | 37 (-1, worse) | 37 | 37 | 37 | 37 | 37 | 37 | 37 |
| reference.should_4k_target | higher is better | 20 | 21 (+1, better) | 21 | 21 | 21 | 20 (-1, worse) | 20 | 20 |
| reference.should_8k_target | higher is better | 23 (-1, worse) | 23 | 23 | 23 | 23 | 22 (-1, worse) | 22 | 22 |
| reference.should_full_target | higher is better | 23 (-1, worse) | 23 | 23 | 23 | 23 | 23 | 23 | 23 |
| reference.must_not_total | information only | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 |
| reference.must_not_4k_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_8k_default | lower is better | 0 (-1, better) | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_full_default | lower is better | 0 (-22, better) | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_4k_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_8k_target | lower is better | 0 (-1, better) | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_full_target | lower is better | 0 (-14, better) | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.repeated_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.finding_lines_default | information only | 269 (-3) | 249 (-20) | 249 | 249 | 248 (-1) | 248 | 248 | 248 |
| redundancy.finding_permille_default | higher is better | 16518 (-114, worse) | 15959 (-559, worse) | 15959 | 15959 | 15947 (-12, worse) | 15952 (+5, better) | 15952 | 15952 |
| redundancy.long_decimals_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.repeated_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.finding_lines_target | information only | 191 (-2) | 180 (-11) | 180 | 180 | 181 (+1) | 179 (-2) | 179 | 179 |
| redundancy.finding_permille_target | higher is better | 9386 (-19, worse) | 9388 (+2, better) | 9388 | 9387 (-1, worse) | 9389 (+2, better) | 9391 (+2, better) | 9391 | 9391 |
| redundancy.long_decimals_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.mismatch | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.error | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.ok | information only | 868 (-34) | 860 (-8) | 860 | 860 | 861 (+1) | 870 (+9) | 870 | 870 |
| truth.not_checkable | information only | 1388 (-117) | 1329 (-59) | 1329 | 1329 | 1356 (+27) | 1359 (+3) | 1359 | 1359 |
| cost.profile_seconds | lower is better | 10.82 | 10.846 (+0.026) | 10.883 (+0.037) | 10.79 (-0.093) | 10.867 (+0.077) | 15.716 (+4.849, worse) | 15.419 (-0.297) | 16.876 |
| cost.json_chars | information only | 3587335 (-258114) | 3611452 (+24117) | 3611452 | 3611452 | 3638120 (+26668) | 3637758 (-362) | 3637758 | 3637758 |
| cost.findings | information only | 1097 (-104) | 1050 (-47) | 1050 | 1050 | 1065 (+15) | 1074 (+9) | 1074 | 1074 |
| weighted.must_total | information only | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| weighted.must_4k | higher is better | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 |
| weighted.must_8k | higher is better | 4 | 3 (-1, worse) | 3 | 3 | 3 | 3 | 3 | 3 |
| weighted.must_full | higher is better | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| weighted.should_total | information only | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_4k | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_8k | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_full | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.weighted_findings | information only | 19 | 20 (+1) | 20 | 20 | 19 (-1) | 19 | 19 | 19 |

## Latest scorecard per dataset

| dataset | kind | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | restatements in the full report | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| adult-census-income | tabular, mixed types | 5 | 4 | 4 | 0 | 0 | 4 | 0 | 0 | 2.762 |
| bank-marketing | tabular, mixed types | 3 | 1 | 2 | 0 | 0 | 10 | 0 | 0 | 2.936 |
| abalone | tabular, correlated measures | 5 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0.159 |
| bike-sharing-hourly | hourly series with calendar columns | 4 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 1.303 |
| metro-interstate-traffic | hourly time series | 5 | - | - | - | 2 | 6 | 0 | 0 | 2.186 |
| thyroid | clinical, many flags and missing values | 1 | - | - | - | 0 | 2 | 0 | 0 | 0.53 |
| absenteeism-at-work | small business table | 3 | - | - | - | 2 | 34 | 0 | 0 | 0.17 |
| hospital-dha | wide numeric, 64 columns | 4 | - | - | - | 0 | 6 | 0 | 0 | 0.676 |
| congressional-voting | categorical votes | 1 | - | - | - | 1 | 13 | 0 | 0 | 0.052 |
| adventureworks | sales orders | 0 | - | - | - | 6 | 39 | 0 | 0 | 3.649 |
| nab-nyc-taxi | single time series | 3 | - | - | - | 0 | 0 | 0 | 0 | 0.11 |
| state-unemployment | wide time series, one column per state | 0 | - | - | - | 4 | 6 | 0 | 0 | 1.313 |
| state-unemployment-long | panel, long form | 0 | 1 | 1 | 0 | 2 | 0 | 0 | 0 | 0.245 |
| south-german-credit (held out) | credit table | 5 | 0 | 0 | 0 | 1 | 6 | 0 | 0 | 0.15 |
| soybean (held out) | categorical agronomy table | 0 | - | - | - | 2 | 23 | 0 | 0 | 0.222 |
| lace-satellites (held out) | satellite catalogue | 1 | - | - | - | 4 | 24 | 0 | 0 | 0.228 |
| sec-edgar-company-facts (held out) | event log, filing events | 2 | 3 | 3 | 0 | 3 | 32 | 0 | 0 | 0.614 |
| gse15258-expression (held out) | wide table, 86 samples x 5,006 columns | - | 0 | 0 | 0 | - | - | - | - | - |
| column_order_stress | generated numeric battery, latent factors | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.199 |
| planted_mix | generated application records with plants | 4 | 2 | 3 | 0 | 0 | 1 | 0 | 0 | 0.246 |
| daily_regime_break | generated daily series, planted breaks | 4 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0.04 |
| dominant_driver | generated listings, numeric target | 5 | 2 | 2 | 0 | 0 | 1 | 0 | 0 | 0.134 |
| daily_level_collapse | generated daily series, planted sustained collapse | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.024 |
| marginal_ranges (held out) | generated range battery | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.044 |
| messy_intake (held out) | generated messy intake records | 1 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0.041 |
| weighted_noise | generated control: independent noise weight | 4 | - | - | - | 0 | 0 | 0 | 0 | 0.083 |
| weighted_large_cell | generated control: merely large cell | 4 | - | - | - | 0 | 1 | 0 | 0 | 0.059 |

## Held-out datasets

These datasets are scored and left out of the pass/fail comparison. A gain on the gated datasets that these do not share points to tuning on the gated datasets.

| date | commit | change | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | restatements in the full report | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 2026-11-18 | `c22795b` | Merge pull request #456 from fcela/feat/448-entity-fixtures | 9 | 4 | 3 | 0 | 10 | 87 | 0 | 0 | 1.299 |
| 2026-11-18 | `cb5da2f` | Merge pull request #454 from fcela/feat/449-frame-spec-resolve | 9 | 4 | 3 | 0 | 10 | 87 | 0 | 0 | 1.222 (-0.011) |
| 2026-11-18 | `c20279a` | Merge pull request #453 from fcela/fix/five-immediate-priorities | 9 | 4 | 3 | 0 | 10 (+2, worse) | 87 (-51, better) | 0 | 0 | 1.233 (+0.307) |
| 2026-11-18 | `e495b4c` | Merge pull request #452 from fcela/fix/446-defaults-framing | 9 | 4 | 3 | 0 | 8 | 138 (-1, better) | 0 | 0 | 0.926 (+0.012) |
| 2026-11-18 | `3e45788` | Merge pull request #451 from fcela/fix/446-shared-basis | 9 | 4 | 3 | 0 | 8 | 139 | 0 | 0 | 0.914 (-0.01) |
| 2026-11-18 | `df24a40` | Merge pull request #447 from fcela/docs/381-capability-map | 9 | 4 | 3 | 0 | 8 | 139 | 0 | 0 | 0.924 (+0.008) |
| 2026-11-18 | `79208eb` | State how targets move, give models the calendar, drop vacuous and mi… | 9 | 4 | 3 | 0 | 8 (+1, worse) | 139 (+19, worse) | 0 | 0 | 0.916 (+0.017) |
| 2026-11-18 | `e3a2c9c` | Merge pull request #444 from fcela/fix/374-steps-3-5 | 9 (+2, better) | 4 | 3 | 0 | 7 (-9, better) | 120 (-31, better) | 0 | 0 | 0.899 |
| 2026-11-18 | `4a18255` | Merge pull request #443 from fcela/fix/363-sample-caps | 7 | 4 | 3 | 0 | 16 | 151 (-2, better) | 0 | 0 | 0.834 |
| 2026-11-18 | `7cc00d4` | Merge pull request #380 from fcela/fix/374-derived-basis | 7 | 4 | 3 | 0 | 16 (+1, worse) | 153 (-15, better) | 0 | 0 | 0.827 (-0.064) |
| 2026-11-18 | `de041e2` | Merge pull request #385 from fcela/fix/378-residual-trend | 7 | 4 | 3 | 0 | 15 | 168 | 0 | 0 | 0.891 (-0.01) |
| 2026-11-17 | `114fff6` | Merge pull request #379 from fcela/docs/book-currency-2026-11-17 | 7 | 4 | 3 | 0 | 15 | 168 (+2, worse) | 0 | 0 | 0.901 |
| 2026-11-17 | `11dfa8e` | Merge pull request #377 from fcela/fix/343-cap-key-registry-guard | 7 | 4 | 3 | 0 | 15 | 166 | 0 | 0 | 0.932 (-0.005) |
| 2026-11-17 | `94f381b` | Merge pull request #376 from fcela/fix/views-deep-look | 7 | 4 | 3 | 0 | 15 | 166 | 0 | 0 | 0.937 (-0.247) |
| 2026-11-17 | `227d742` | Merge pull request #375 from fcela/gate/374-precision | 7 | 4 | 3 | 0 | 15 | 166 | 0 | 0 | 1.184 |
| 2026-11-17 | `3ea33b1` | Merge pull request #370 from fcela/fix/audit-followups-2026-11-17 | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 0.892 |
| 2026-11-17 | `3889755` | Merge pull request #367 from fcela/feat/310-pointed-mode | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 0.934 (-0.277) |
| 2026-11-17 | `34718c6` | Merge pull request #369 from fcela/fix/365-views-integrity | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 1.211 |
| 2026-11-17 | `a7994e0` | Merge pull request #368 from fcela/fix/365-exact-decimals | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 0.897 |
| 2026-11-17 | `3c2db24` | Merge pull request #366 from fcela/fix/365-fk-chance | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 0.931 |
| 2026-11-17 | `7d27ac8` | Merge pull request #355 from fcela/docs/347-action-value | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 0.896 |
| 2026-11-17 | `260b1c4` | Merge pull request #354 from fcela/docs/353-finding-value | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 0.688 |
| 2026-11-17 | `526507a` | Merge pull request #358 from fcela/fix/ci-history-job | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 0.881 (-0.01) |
| 2026-11-17 | `d057860` | Merge pull request #357 from fcela/release/0.1.6-prep | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 0.891 |
| 2026-11-17 | `39741ac` | Merge pull request #356 from fcela/docs/audit-2026-11-17 | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 1.171 (+0.444) |
| 2026-11-17 | `52ed9d4` | Merge pull request #351 from fcela/feat/348-uci-expansion | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 0.727 (+0.003) |
| 2026-11-17 | `cec0278` | Merge pull request #352 from fcela/docs/347-interestingness-study | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 0.724 |
| 2026-11-17 | `eef295a` | Merge pull request #350 from fcela/feat/329-weighted-concentration | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 0.893 (+0.011) |
| 2026-11-17 | `610812e` | Merge pull request #345 from fcela/feat/gate-concentration | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 0.882 |
| 2026-11-16 | `956aa92` | Merge pull request #344 from fcela/feat/317-findings-generation | 7 | 4 | 3 | - | 15 | 166 | 0 | 0 | 0.767 |
| 2026-11-16 | `ce38fe6` | Merge pull request #339 from fcela/fix/317-axis-gated-breaks | 7 | 4 | 3 | - | 15 (-1, better) | 166 (-17, better) | 0 | 0 | 0.885 |
| 2026-11-16 | `7612842` | Merge pull request #341 from fcela/audit/317-detector-audit | 7 (+1, better) | 4 (+1, better) | 3 | - | 16 (-3, better) | 183 (+1, worse) | 0 | 0 | 1.284 |
| 2026-11-16 | `40329ad` | Merge pull request #342 from fcela/fix/317-max-findings | 6 | 3 | 3 | - | 19 | 182 | 0 | 0 | 1.125 (+0.006) |
| 2026-11-16 | `064132d` | Merge pull request #334 from fcela/feat/317-digest-briefing | 6 (+3, better) | 3 | 3 | - | 19 (-4, better) | 182 (+44, worse) | 0 (-1, better) | 0 | 1.119 (+0.503) |
| 2026-11-16 | `2601a72` | Merge pull request #326 from fcela/feat/gate-report-tldr | 3 | - | - | - | 23 | 138 | 1 | 0 | 0.616 (+0.002) |
| 2026-11-15 | `daf7a79` | Merge pull request #324 from fcela/feat/315-corpus-targets | 3 | - | - | - | 23 | 138 | 1 | 0 | 0.614 |
| 2026-11-15 | `ca275fd` | Merge pull request #323 from fcela/feat/317-gate-history-report | 3 | - | - | - | 23 | 138 | 1 | 0 | 0.626 |
| 2026-11-15 | `1618886` | Merge pull request #320 from fcela/fix/295-agent-token-economy | 3 | - | - | - | 23 | 138 | 1 | 0 | 0.327 |

## Platforms

- Linux-7.0.0-1014-aws-x86_64-with-glibc2.39
- Linux-7.0.0-1017-azure-x86_64-with-glibc2.39
- macOS-26.3-arm64-arm-64bit-Mach-O
