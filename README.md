# Gate report

## TL;DR

Latest change `7cc00d4` (2026-11-18), Merge pull request #380 from fcela/fix/374-derived-basis: compared with `de041e2`, 16 total metrics better, 0 worse, 3 worse and accepted.

| measure | latest (`7cc00d4`) | previous (`de041e2`) | first recorded (`1618886`) |
|---|---:|---:|---:|
| Must-tier reference facts stated in the 4,000-character digest | 16 of 19 (unchanged) | 16 of 19 | 7 of 11 |
| Must-tier reference facts stated in the 4,000-character digest, target mode | 17 of 19 (unchanged) | 17 of 19 | 7 of 11 |
| Should-tier reference facts stated in the 4,000-character digest | 31 of 45 (+2, better) | 29 of 45 | 7 of 12 |
| Should-tier reference facts stated in the 4,000-character digest, target mode | 19 of 45 (+1, better) | 18 of 45 | 8 of 12 |
| Planted structures stated in the 4,000-character digest | 56 (unchanged) | 56 | 20 |
| Facts repeated in the 4,000-character digest | 0 (unchanged) | 0 | 4 |
| Facts repeated in the 4,000-character digest, target mode | 0 (unchanged) | 0 | 3 |
| Relationship statements in the 4,000-character digest of shuffled tables (all false by construction) | 46 (-2, better) | 48 | 136 |
| Statements that fail verification against the data | 0 (unchanged) | 0 | 0 |

Biggest improvements: reference must not 4k default 8 → 0; reference must not 4k target 3 → 0; reference must not 8k default 18 → 1
Biggest regressions: none.
Accepted regressions: invariance noise 29 → 30 (Issue #374 step 2: the adult-census-income change above. Record: docs/benchmarks/2026-11-18-derived-basis.md.); redundancy finding permille default 16636 → 16631 (Issue #374 step 2: finding-line character share moves by 1-4 per mille on four datasets as restatements leave the digest and shorter lines replace them. Record: docs/benchmarks/2026-11-18-derived-basis.md.); redundancy finding permille target 9406 → 9405 (Issue #374 step 2: same as the default-mode entry. Record: docs/benchmarks/2026-11-18-derived-basis.md.)

Hard lines: none breached — no statement failed or errored in verification, and no must-tier reference fact was lost from the 4,000-character digest.

29 change(s) recorded. Latest: `7cc00d4` (2026-11-18), onceover 0.1.6, seeds [0].

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

## Latest change: `7cc00d4` Merge pull request #380 from fcela/fix/374-derived-basis

Compared with `de041e2`: 16 total metrics better, 0 worse, 3 worse and accepted. Rows marked `total` sum the gated datasets; the other rows name one dataset.

### Better

| metric | where | before | after | change |
|---|---|---:|---:|---:|
| invariance.reverse | total | 200 | 197 | -3 |
| invariance.row_shuffle | total | 106 | 105 | -1 |
| invariance.total | total | 337 | 334 | -3 |
| permuted.false_4k | total | 48 | 46 | -2 |
| permuted.false_4k_target | total | 30 | 28 | -2 |
| permuted.false_full | total | 100 | 86 | -14 |
| permuted.false_full_target | total | 51 | 47 | -4 |
| reference.must_not_4k_default | total | 8 | 0 | -8 |
| reference.must_not_4k_target | total | 3 | 0 | -3 |
| reference.must_not_8k_default | total | 18 | 1 | -17 |
| reference.must_not_8k_target | total | 16 | 1 | -15 |
| reference.must_not_full_default | total | 81 | 22 | -59 |
| reference.must_not_full_target | total | 60 | 14 | -46 |
| reference.should_4k_default | total | 29 | 31 | +2 |
| reference.should_4k_target | total | 18 | 19 | +1 |
| reference.should_full_default | total | 37 | 38 | +1 |
| invariance.reverse | adult-census-income | 11 | 6 | -5 |
| invariance.reverse | adventureworks | 43 | 42 | -1 |
| invariance.row_shuffle | adult-census-income | 7 | 6 | -1 |
| invariance.total | adult-census-income | 24 | 19 | -5 |
| invariance.total | adventureworks | 45 | 44 | -1 |
| permuted.false_4k | absenteeism-at-work | 5 | 4 | -1 |
| permuted.false_4k | bike-sharing-hourly | 5 | 4 | -1 |
| permuted.false_4k_target | absenteeism-at-work | 5 | 4 | -1 |
| permuted.false_4k_target | bike-sharing-hourly | 5 | 4 | -1 |
| permuted.false_full | absenteeism-at-work | 8 | 7 | -1 |
| permuted.false_full | adventureworks | 40 | 30 | -10 |
| permuted.false_full | bike-sharing-hourly | 8 | 5 | -3 |
| permuted.false_full_target | absenteeism-at-work | 8 | 7 | -1 |
| permuted.false_full_target | bike-sharing-hourly | 8 | 5 | -3 |
| redundancy.finding_permille_default | adult-census-income | 887 | 888 | +1 |
| redundancy.finding_permille_default | bike-sharing-hourly | 840 | 842 | +2 |
| redundancy.finding_permille_target | adult-census-income | 879 | 880 | +1 |
| reference.must_not_4k_default | adult-census-income | 2 | 0 | -2 |
| reference.must_not_4k_default | bike-sharing-hourly | 6 | 0 | -6 |
| reference.must_not_4k_target | adult-census-income | 2 | 0 | -2 |
| reference.must_not_4k_target | bike-sharing-hourly | 1 | 0 | -1 |
| reference.must_not_8k_default | adult-census-income | 4 | 0 | -4 |
| reference.must_not_8k_default | bike-sharing-hourly | 14 | 1 | -13 |
| reference.must_not_8k_target | adult-census-income | 4 | 0 | -4 |
| reference.must_not_8k_target | bike-sharing-hourly | 12 | 1 | -11 |
| reference.must_not_full_default | adult-census-income | 30 | 7 | -23 |
| reference.must_not_full_default | bike-sharing-hourly | 51 | 15 | -36 |
| reference.must_not_full_target | adult-census-income | 19 | 0 | -19 |
| reference.must_not_full_target | bike-sharing-hourly | 41 | 14 | -27 |
| reference.should_4k_default | bank-marketing | 0 | 1 | +1 |
| reference.should_4k_default | bike-sharing-hourly | 2 | 3 | +1 |
| reference.should_4k_target | bike-sharing-hourly | 2 | 3 | +1 |
| reference.should_8k_default | bike-sharing-hourly | 3 | 4 | +1 |
| reference.should_full_default | bike-sharing-hourly | 3 | 4 | +1 |

### Worse

None.

### Worse and accepted

| metric | where | before | after | change | reason |
|---|---|---:|---:|---:|---|
| invariance.noise | total | 29 | 30 | +1 | Issue #374 step 2: the adult-census-income change above. Record: docs/benchmarks/2026-11-18-derived-basis.md. |
| redundancy.finding_permille_default | total | 16636 | 16631 | -5 | Issue #374 step 2: finding-line character share moves by 1-4 per mille on four datasets as restatements leave the digest and shorter lines replace them. Record: docs/benchmarks/2026-11-18-derived-basis.md. |
| redundancy.finding_permille_target | total | 9406 | 9405 | -1 | Issue #374 step 2: same as the default-mode entry. Record: docs/benchmarks/2026-11-18-derived-basis.md. |
| invariance.noise | adult-census-income | 6 | 7 | +1 | Issue #374 step 2: with a noise column added, one more level contrast (age by marital_status) is stated; the pass's own 20-statement cap now admits it because the education_num restatements no longer take slots. Record: docs/benchmarks/2026-11-18-derived-basis.md. |
| invariance.reverse | bike-sharing-hourly | 0 | 3 | +3 | Issue #374 step 2: the derived-column basis changes which columns are predictability targets; under column reversal the drivers named for hum move among near-collinear inputs (temp/atemp, mnth/season), whose holdout split is seeded by column position. Record: docs/benchmarks/2026-11-18-derived-basis.md. |
| invariance.total | bike-sharing-hourly | 0 | 3 | +3 | Issue #374 step 2: the invariance.reverse change on this dataset. Record: docs/benchmarks/2026-11-18-derived-basis.md. |
| redundancy.finding_permille_default | abalone | 930 | 926 | -4 | Issue #374 step 2: finding-line character share moves by 1-4 per mille on four datasets as restatements leave the digest and shorter lines replace them. Record: docs/benchmarks/2026-11-18-derived-basis.md. |
| redundancy.finding_permille_default | adventureworks | 836 | 833 | -3 | Issue #374 step 2: finding-line character share moves by 1-4 per mille on four datasets as restatements leave the digest and shorter lines replace them. Record: docs/benchmarks/2026-11-18-derived-basis.md. |
| redundancy.finding_permille_default | bank-marketing | 888 | 887 | -1 | Issue #374 step 2: finding-line character share moves by 1-4 per mille on four datasets as restatements leave the digest and shorter lines replace them. Record: docs/benchmarks/2026-11-18-derived-basis.md. |
| redundancy.finding_permille_target | bike-sharing-hourly | 839 | 837 | -2 | Issue #374 step 2: same as the default-mode entry. Record: docs/benchmarks/2026-11-18-derived-basis.md. |
| reference.should_8k_default | bank-marketing | 4 | 3 | -1 | Issue #374 step 2: the base credited subscription-housing-loan through a text-pattern match on a rolled-up 8k digest line that no finding in the full report supports (full default: not matched in base or head). Record: docs/benchmarks/2026-11-18-derived-basis.md. |

## Every total metric, last 8 changes

Oldest on the left. A value in a column is followed by its movement against the column before it.

| metric | reading | `3889755` | `3ea33b1` | `227d742` | `94f381b` | `11dfa8e` | `114fff6` | `de041e2` | `7cc00d4` |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| planted.planted | information only | 76 | 76 | 76 | 76 | 76 | 76 | 76 | 76 |
| planted.found_full | higher is better | 58 (-1, worse) | 58 | 58 | 58 | 58 | 58 | 58 | 58 |
| planted.found_4k | higher is better | 56 (+1, better) | 56 | 56 | 56 | 56 | 56 | 56 | 56 |
| planted.line_sum_4k | information only | 630 (+9) | 630 | 630 | 630 | 630 | 630 | 630 | 628 (-2) |
| permuted.false_full | lower is better | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 86 (-14, better) |
| permuted.false_4k | lower is better | 48 | 48 | 48 | 48 | 48 | 48 | 48 | 46 (-2, better) |
| permuted.false_full_target | lower is better | 51 | 51 | 51 | 51 | 51 | 51 | 51 | 47 (-4, better) |
| permuted.false_4k_target | lower is better | 30 | 30 | 30 | 30 | 30 | 30 | 30 | 28 (-2, better) |
| permuted.findings_full | information only | 318 | 318 | 318 | 318 | 318 | 318 | 318 | 304 (-14) |
| invariance.reverse | lower is better | 199 | 199 | 199 | 199 | 199 | 200 (+1, worse) | 200 | 197 (-3, better) |
| invariance.row_shuffle | lower is better | 106 | 106 | 106 | 106 | 106 | 106 | 106 | 105 (-1, better) |
| invariance.rename | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| invariance.noise | lower is better | 29 | 29 | 29 | 29 | 29 | 29 | 29 | 30 (+1, worse) |
| invariance.scale | lower is better | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 |
| invariance.total | lower is better | 336 | 336 | 336 | 336 | 336 | 337 (+1, worse) | 337 | 334 (-3, better) |
| reference.check_failed | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_total | information only | 19 | 19 | 19 | 19 | 19 | 19 | 19 | 19 |
| reference.must_4k_default | higher is better | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 16 |
| reference.must_8k_default | higher is better | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 16 |
| reference.must_full_default | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_4k_target | higher is better | 17 (+1, better) | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_8k_target | higher is better | 17 (+1, better) | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_full_target | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.should_total | information only | 45 | 45 | 45 | 45 | 45 | 45 | 45 | 45 |
| reference.should_4k_default | higher is better | 29 | 29 | 29 | 29 | 29 | 29 | 29 | 31 (+2, better) |
| reference.should_8k_default | higher is better | 36 (+1, better) | 36 | 36 | 36 | 36 | 36 | 36 | 36 |
| reference.should_full_default | higher is better | 37 | 37 | 37 | 37 | 37 | 37 | 37 | 38 (+1, better) |
| reference.should_4k_target | higher is better | 18 (+2, better) | 18 | 18 | 18 | 18 | 18 | 18 | 19 (+1, better) |
| reference.should_8k_target | higher is better | 23 (+2, better) | 23 | 23 | 23 | 23 | 23 | 23 | 23 |
| reference.should_full_target | higher is better | 24 (+2, better) | 24 | 24 | 24 | 24 | 24 | 24 | 24 |
| redundancy.repeated_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.finding_lines_default | information only | 272 | 272 | 272 | 272 | 272 | 272 | 272 | 271 (-1) |
| redundancy.finding_permille_default | higher is better | 16636 (+2, better) | 16636 | 16636 | 16636 | 16636 | 16636 | 16636 | 16631 (-5, worse) |
| redundancy.long_decimals_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.repeated_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.finding_lines_target | information only | 193 (+12) | 193 | 193 | 193 | 193 | 193 | 193 | 195 (+2) |
| redundancy.finding_permille_target | higher is better | 9406 (+2, better) | 9406 | 9406 | 9406 | 9406 | 9406 | 9406 | 9405 (-1, worse) |
| redundancy.long_decimals_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.mismatch | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.error | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.ok | information only | 907 (-8) | 907 | 907 | 907 | 907 | 907 | 907 | 902 (-5) |
| truth.not_checkable | information only | 1524 (+51) | 1524 | 1524 | 1524 | 1524 | 1537 (+13) | 1537 | 1508 (-29) |
| cost.profile_seconds | lower is better | 11.762 (-3.144, better) | 10.8 | 14.949 | 11.502 (-3.447, better) | 11.688 (+0.186) | 10.95 | 10.752 (-0.198) | 9.663 (-1.089) |
| cost.json_chars | information only | 3880166 (+5440) | 3877987 (-2179) | 3877987 | 3877987 | 3877987 | 3905682 (+27695) | 3905682 | 3856558 (-49124) |
| cost.findings | information only | 1203 | 1203 | 1203 | 1203 | 1203 | 1215 (+12) | 1215 | 1204 (-11) |
| weighted.must_total | information only | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| weighted.must_4k | higher is better | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 |
| weighted.must_8k | higher is better | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| weighted.must_full | higher is better | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| weighted.should_total | information only | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_4k | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_8k | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_full | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.weighted_findings | information only | 15 | 15 | 15 | 15 | 15 | 15 | 15 | 19 (+4) |
| reference.must_not_total | information only | - | - | 9 | 9 | 9 | 9 | 9 | 9 |
| reference.must_not_4k_default | lower is better | - | - | 8 | 8 | 8 | 8 | 8 | 0 (-8, better) |
| reference.must_not_8k_default | lower is better | - | - | 18 | 18 | 18 | 18 | 18 | 1 (-17, better) |
| reference.must_not_full_default | lower is better | - | - | 81 | 81 | 81 | 81 | 81 | 22 (-59, better) |
| reference.must_not_4k_target | lower is better | - | - | 3 | 3 | 3 | 3 | 3 | 0 (-3, better) |
| reference.must_not_8k_target | lower is better | - | - | 16 | 16 | 16 | 16 | 16 | 1 (-15, better) |
| reference.must_not_full_target | lower is better | - | - | 60 | 60 | 60 | 60 | 60 | 14 (-46, better) |

## Latest scorecard per dataset

| dataset | kind | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | restatements in the full report | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| adult-census-income | tabular, mixed types | 5 | 4 | 4 | 7 | 1 | 19 | 0 | 0 | 1.103 |
| bank-marketing | tabular, mixed types | 3 | 2 | 2 | 0 | 1 | 35 | 0 | 0 | 1.33 |
| abalone | tabular, correlated measures | 5 | 3 | 3 | 0 | 0 | 7 | 0 | 0 | 0.1 |
| bike-sharing-hourly | hourly series with calendar columns | 3 | 2 | 2 | 15 | 4 | 3 | 0 | 0 | 0.67 |
| metro-interstate-traffic | hourly time series | 4 | - | - | - | 7 | 6 | 0 | 0 | 1.296 |
| thyroid | clinical, many flags and missing values | 1 | - | - | - | 6 | 35 | 0 | 0 | 0.558 |
| absenteeism-at-work | small business table | 2 | - | - | - | 4 | 64 | 0 | 0 | 0.156 |
| hospital-dha | wide numeric, 64 columns | 3 | - | - | - | 0 | 23 | 0 | 0 | 0.596 |
| congressional-voting | categorical votes | 1 | - | - | - | 2 | 60 | 0 | 0 | 0.046 |
| adventureworks | sales orders | 1 | - | - | - | 11 | 44 | 0 | 0 | 2.071 |
| nab-nyc-taxi | single time series | 3 | - | - | - | 0 | 0 | 0 | 0 | 0.05 |
| state-unemployment | wide time series, one column per state | 0 | - | - | - | 4 | 25 | 0 | 0 | 1.116 |
| state-unemployment-long | panel, long form | 0 | 1 | 1 | 0 | 2 | 0 | 0 | 0 | 0.084 |
| south-german-credit (held out) | credit table | 4 | 0 | 0 | 0 | 3 | 12 | 0 | 0 | 0.132 |
| soybean (held out) | categorical agronomy table | 0 | - | - | - | 4 | 49 | 0 | 0 | 0.2 |
| lace-satellites (held out) | satellite catalogue | 1 | - | - | - | 7 | 65 | 0 | 0 | 0.182 |
| sec-edgar-company-facts (held out) | event log, filing events | 1 | 3 | 3 | 0 | 2 | 25 | 0 | 0 | 0.28 |
| gse15258-expression (held out) | wide table, 86 samples x 5,006 columns | - | 0 | 0 | 0 | - | - | - | - | - |
| column_order_stress | generated numeric battery, latent factors | 0 | 0 | 0 | 0 | 0 | 9 | 0 | 0 | 0.138 |
| planted_mix | generated application records with plants | 4 | 2 | 3 | 0 | 1 | 1 | 0 | 0 | 0.16 |
| daily_regime_break | generated daily series, planted breaks | 4 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0.027 |
| dominant_driver | generated listings, numeric target | 5 | 2 | 2 | 0 | 0 | 1 | 0 | 0 | 0.081 |
| daily_level_collapse | generated daily series, planted sustained collapse | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.018 |
| marginal_ranges (held out) | generated range battery | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.023 |
| messy_intake (held out) | generated messy intake records | 1 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0.01 |
| weighted_noise | generated control: independent noise weight | 4 | - | - | - | 1 | 1 | 0 | 0 | 0.042 |
| weighted_large_cell | generated control: merely large cell | 4 | - | - | - | 0 | 1 | 0 | 0 | 0.021 |

## Held-out datasets

These datasets are scored and left out of the pass/fail comparison. A gain on the gated datasets that these do not share points to tuning on the gated datasets.

| date | commit | change | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | restatements in the full report | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
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
