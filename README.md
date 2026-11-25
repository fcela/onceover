# Gate report

## TL;DR

Latest change `0320a87` (2026-11-24), Release 0.1.10 with distribution slices: compared with `8d135c1`, 0 total metrics better, 0 worse, 0 worse and accepted.

| measure | latest (`0320a87`) | previous (`8d135c1`) | first recorded (`1618886`) |
|---|---:|---:|---:|
| Must-tier reference facts stated in the 4,000-character digest | 15 of 19 (unchanged) | 15 of 19 | 7 of 11 |
| Must-tier reference facts stated in the 4,000-character digest, target mode | 17 of 19 (unchanged) | 17 of 19 | 7 of 11 |
| Should-tier reference facts stated in the 4,000-character digest | 33 of 45 (unchanged) | 33 of 45 | 7 of 12 |
| Should-tier reference facts stated in the 4,000-character digest, target mode | 20 of 45 (unchanged) | 20 of 45 | 8 of 12 |
| Planted structures stated in the 4,000-character digest | 59 (unchanged) | 59 | 20 |
| Facts repeated in the 4,000-character digest | 0 (unchanged) | 0 | 4 |
| Facts repeated in the 4,000-character digest, target mode | 0 (unchanged) | 0 | 3 |
| Relationship statements in the 4,000-character digest of shuffled tables (all false by construction) | 4 (unchanged) | 4 | 136 |
| Statements that fail verification against the data | 0 (unchanged) | 0 | 0 |

Biggest improvements: none.
Biggest regressions: none.
Accepted regressions: none.

Hard lines: none breached — no statement failed or errored in verification, and no must-tier reference fact was lost from the 4,000-character digest.

46 change(s) recorded. Latest: `0320a87` (2026-11-24), onceover 0.1.10, seeds [0].

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
| 2026-11-24 | `0320a87` | Release 0.1.10 with distribution slices | 59 | 15 | 17 | 0 | 4 | 118 | 0 | 0 | 9.664 |
| 2026-11-23 | `8d135c1` | Merge pull request #476 from fcela/release/0.1.9 | 59 | 15 | 17 | 0 | 4 | 118 | 0 | 0 | 8.274 (+0.04) |
| 2026-11-23 | `f494748` | Merge pull request #475 from fcela/fix/adversarial-shootout | 59 | 15 | 17 | 0 | 4 | 118 | 0 | 0 | 8.234 |
| 2026-11-22 | `c1acd5f` | Merge pull request #465 from fcela/release/0.1.8 | 59 | 15 | 17 | 0 | 4 (-17, better) | 118 (-12, better) | 0 | 0 | 9.99 |
| 2026-11-18 | `a0527ef` | Release 0.1.7 with hierarchical entity peers | 59 | 15 | 17 | 0 | 21 | 130 | 0 | 0 | 17.049 (+0.176) |
| 2026-11-18 | `ad88088` | Merge pull request #459 from fcela/feat/448-weighted-joint-fingerprin… | 59 | 15 | 17 | 0 | 21 | 130 | 0 | 0 | 16.873 |
| 2026-11-18 | `84d830f` | Merge pull request #458 from fcela/feat/448-fingerprint-drift | 59 | 15 | 17 | 0 | 21 | 130 | 0 | 0 | 15.512 |
| 2026-11-18 | `0b47836` | Merge pull request #457 from fcela/feat/448-entity-fingerprints | 59 | 15 | 17 | 0 | 21 | 130 | 0 | 0 | 20.491 (+3.615, worse) |
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

## Latest change: `0320a87` Release 0.1.10 with distribution slices

Compared with `8d135c1`: 0 total metrics better, 0 worse, 0 worse and accepted. Rows marked `total` sum the gated datasets; the other rows name one dataset.

### Better

None.

### Worse

None.

### Worse and accepted

None.

## Every total metric, last 8 changes

Oldest on the left. A value in a column is followed by its movement against the column before it.

| metric | reading | `0b47836` | `84d830f` | `ad88088` | `a0527ef` | `c1acd5f` | `f494748` | `8d135c1` | `0320a87` |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| planted.planted | information only | 76 | 76 | 76 | 76 | 76 | 76 | 76 | 76 |
| planted.found_full | higher is better | 60 | 60 | 60 | 60 | 61 (+1, better) | 61 | 61 | 61 |
| planted.found_4k | higher is better | 59 | 59 | 59 | 59 | 59 | 59 | 59 | 59 |
| planted.line_sum_4k | information only | 606 | 606 | 606 | 606 | 604 (-2) | 605 (+1) | 605 | 605 |
| permuted.false_full | lower is better | 38 | 38 | 38 | 38 | 7 (-31, better) | 7 | 7 | 7 |
| permuted.false_4k | lower is better | 21 | 21 | 21 | 21 | 4 (-17, better) | 4 | 4 | 4 |
| permuted.false_full_target | lower is better | 16 | 16 | 16 | 16 | 1 (-15, better) | 1 | 1 | 1 |
| permuted.false_4k_target | lower is better | 9 | 9 | 9 | 9 | 1 (-8, better) | 1 | 1 | 1 |
| permuted.findings_full | information only | 223 | 223 | 223 | 223 | 208 (-15) | 208 | 208 | 208 |
| invariance.reverse | lower is better | 87 | 87 | 87 | 87 | 73 (-14, better) | 73 | 73 | 73 |
| invariance.row_shuffle | lower is better | 11 | 11 | 11 | 11 | 11 | 11 | 11 | 11 |
| invariance.rename | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| invariance.noise | lower is better | 31 | 31 | 31 | 31 | 33 (+2, worse) | 33 | 33 | 33 |
| invariance.scale | lower is better | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| invariance.total | lower is better | 130 | 130 | 130 | 130 | 118 (-12, better) | 118 | 118 | 118 |
| reference.check_failed | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_total | information only | 19 | 19 | 19 | 19 | 19 | 19 | 19 | 19 |
| reference.must_4k_default | higher is better | 15 | 15 | 15 | 15 | 15 | 15 | 15 | 15 |
| reference.must_8k_default | higher is better | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 16 |
| reference.must_full_default | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_4k_target | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_8k_target | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_full_target | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.should_total | information only | 45 | 45 | 45 | 45 | 45 | 45 | 45 | 45 |
| reference.should_4k_default | higher is better | 32 | 32 | 32 | 32 | 31 (-1, worse) | 33 (+2, better) | 33 | 33 |
| reference.should_8k_default | higher is better | 35 | 35 | 35 | 35 | 34 (-1, worse) | 36 (+2, better) | 36 | 36 |
| reference.should_full_default | higher is better | 37 | 37 | 37 | 37 | 36 (-1, worse) | 36 | 36 | 36 |
| reference.should_4k_target | higher is better | 20 | 20 | 20 | 20 | 20 | 20 | 20 | 20 |
| reference.should_8k_target | higher is better | 22 | 22 | 22 | 22 | 22 | 22 | 22 | 22 |
| reference.should_full_target | higher is better | 23 | 23 | 23 | 23 | 23 | 23 | 23 | 23 |
| reference.must_not_total | information only | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 |
| reference.must_not_4k_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_8k_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_full_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_4k_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_8k_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_full_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.repeated_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.finding_lines_default | information only | 248 | 248 | 248 | 248 | 246 (-2) | 260 (+14) | 260 | 260 |
| redundancy.finding_permille_default | higher is better | 15952 | 15952 | 15952 | 15952 | 15947 (-5, worse) | 16347 (+400, better) | 16347 | 16347 |
| redundancy.long_decimals_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.repeated_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.finding_lines_target | information only | 179 | 179 | 179 | 179 | 179 | 183 (+4) | 183 | 183 |
| redundancy.finding_permille_target | higher is better | 9391 | 9391 | 9391 | 9391 | 9392 (+1, better) | 9581 (+189, better) | 9581 | 9581 |
| redundancy.long_decimals_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.mismatch | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.error | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.ok | information only | 870 | 870 | 870 | 870 | 1090 (+220) | 1090 | 1090 | 1090 |
| truth.not_checkable | information only | 1359 | 1359 | 1359 | 1359 | 1225 (-134) | 1225 | 1225 | 1225 |
| cost.profile_seconds | lower is better | 20.491 (+3.615, worse) | 15.512 | 16.873 | 17.049 (+0.176) | 9.99 | 8.234 | 8.274 (+0.04) | 9.664 |
| cost.json_chars | information only | 3640798 (+3040) | 3640798 | 3640798 | 3640798 | 3570148 (-70650) | 3570096 (-52) | 3570096 | 3573136 (+3040) |
| cost.findings | information only | 1074 | 1074 | 1074 | 1074 | 1038 (-36) | 1038 | 1038 | 1038 |
| weighted.must_total | information only | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| weighted.must_4k | higher is better | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 |
| weighted.must_8k | higher is better | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 |
| weighted.must_full | higher is better | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| weighted.should_total | information only | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_4k | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_8k | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_full | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.weighted_findings | information only | 19 | 19 | 19 | 19 | 19 | 19 | 19 | 19 |

## Latest scorecard per dataset

| dataset | kind | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | restatements in the full report | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| adult-census-income | tabular, mixed types | 5 | 4 | 4 | 0 | 0 | 4 | 0 | 0 | 1.365 |
| bank-marketing | tabular, mixed types | 3 | 1 | 2 | 0 | 0 | 10 | 0 | 0 | 1.699 |
| abalone | tabular, correlated measures | 5 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0.104 |
| bike-sharing-hourly | hourly series with calendar columns | 4 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0.681 |
| metro-interstate-traffic | hourly time series | 5 | - | - | - | 0 | 3 | 0 | 0 | 1.271 |
| thyroid | clinical, many flags and missing values | 1 | - | - | - | 0 | 2 | 0 | 0 | 0.392 |
| absenteeism-at-work | small business table | 3 | - | - | - | 0 | 30 | 0 | 0 | 0.142 |
| hospital-dha | wide numeric, 64 columns | 4 | - | - | - | 0 | 6 | 0 | 0 | 0.59 |
| congressional-voting | categorical votes | 1 | - | - | - | 1 | 14 | 0 | 0 | 0.044 |
| adventureworks | sales orders | 0 | - | - | - | 3 | 33 | 0 | 0 | 1.66 |
| nab-nyc-taxi | single time series | 3 | - | - | - | 0 | 0 | 0 | 0 | 0.067 |
| state-unemployment | wide time series, one column per state | 0 | - | - | - | 0 | 6 | 0 | 0 | 1.12 |
| state-unemployment-long | panel, long form | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0.102 |
| south-german-credit (held out) | credit table | 5 | 0 | 0 | 0 | 1 | 6 | 0 | 0 | 0.123 |
| soybean (held out) | categorical agronomy table | 0 | - | - | - | 2 | 23 | 0 | 0 | 0.194 |
| lace-satellites (held out) | satellite catalogue | 1 | - | - | - | 4 | 24 | 0 | 0 | 0.186 |
| sec-edgar-company-facts (held out) | event log, filing events | 2 | 3 | 3 | 0 | 2 | 32 | 0 | 0 | 0.402 |
| gse15258-expression (held out) | wide table, 86 samples x 5,006 columns | - | 1 | 1 | 0 | - | - | - | - | - |
| column_order_stress | generated numeric battery, latent factors | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.14 |
| planted_mix | generated application records with plants | 4 | 2 | 3 | 0 | 0 | 1 | 0 | 0 | 0.117 |
| daily_regime_break | generated daily series, planted breaks | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.026 |
| dominant_driver | generated listings, numeric target | 5 | 2 | 2 | 0 | 0 | 1 | 0 | 0 | 0.069 |
| daily_level_collapse | generated daily series, planted sustained collapse | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.016 |
| marginal_ranges (held out) | generated range battery | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.024 |
| messy_intake (held out) | generated messy intake records | 1 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0.011 |
| weighted_noise | generated control: independent noise weight | 4 | - | - | - | 0 | 0 | 0 | 0 | 0.032 |
| weighted_large_cell | generated control: merely large cell | 4 | - | - | - | 0 | 1 | 0 | 0 | 0.027 |

## Held-out datasets

These datasets are scored and left out of the pass/fail comparison. A gain on the gated datasets that these do not share points to tuning on the gated datasets.

| date | commit | change | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | restatements in the full report | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 2026-11-24 | `0320a87` | Release 0.1.10 with distribution slices | 9 | 5 | 4 | 0 | 9 | 87 | 0 | 0 | 0.94 |
| 2026-11-23 | `8d135c1` | Merge pull request #476 from fcela/release/0.1.9 | 9 | 5 | 4 | 0 | 9 | 87 | 0 | 0 | 0.769 |
| 2026-11-23 | `f494748` | Merge pull request #475 from fcela/fix/adversarial-shootout | 9 | 5 | 4 | 0 | 9 | 87 | 0 | 0 | 0.769 |
| 2026-11-22 | `c1acd5f` | Merge pull request #465 from fcela/release/0.1.8 | 9 | 5 (+1, better) | 4 (+1, better) | 0 | 9 (-1, better) | 87 | 0 | 0 | 0.952 |
| 2026-11-18 | `a0527ef` | Release 0.1.7 with hierarchical entity peers | 9 | 4 | 3 | 0 | 10 | 87 | 0 | 0 | 1.305 (+0.005) |
| 2026-11-18 | `ad88088` | Merge pull request #459 from fcela/feat/448-weighted-joint-fingerprin… | 9 | 4 | 3 | 0 | 10 | 87 | 0 | 0 | 1.3 |
| 2026-11-18 | `84d830f` | Merge pull request #458 from fcela/feat/448-fingerprint-drift | 9 | 4 | 3 | 0 | 10 | 87 | 0 | 0 | 1.23 |
| 2026-11-18 | `0b47836` | Merge pull request #457 from fcela/feat/448-entity-fingerprints | 9 | 4 | 3 | 0 | 10 | 87 | 0 | 0 | 1.617 (+0.318) |
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
