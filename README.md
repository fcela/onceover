# Gate report

## TL;DR

Latest change `79208eb` (2026-11-18), State how targets move, give models the calendar, drop vacuous and mirrored sta…: compared with `e3a2c9c`, 6 total metrics better, 2 worse, 4 worse and accepted.

| measure | latest (`79208eb`) | previous (`e3a2c9c`) | first recorded (`1618886`) |
|---|---:|---:|---:|
| Must-tier reference facts stated in the 4,000-character digest | 15 of 19 (-1, worse) | 16 of 19 | 7 of 11 |
| Must-tier reference facts stated in the 4,000-character digest, target mode | 17 of 19 (unchanged) | 17 of 19 | 7 of 11 |
| Should-tier reference facts stated in the 4,000-character digest | 33 of 45 (+1, better) | 32 of 45 | 7 of 12 |
| Should-tier reference facts stated in the 4,000-character digest, target mode | 21 of 45 (+1, better) | 20 of 45 | 8 of 12 |
| Planted structures stated in the 4,000-character digest | 58 (+1, better) | 57 | 20 |
| Facts repeated in the 4,000-character digest | 0 (unchanged) | 0 | 4 |
| Facts repeated in the 4,000-character digest, target mode | 0 (unchanged) | 0 | 3 |
| Relationship statements in the 4,000-character digest of shuffled tables (all false by construction) | 22 (unchanged) | 22 | 136 |
| Statements that fail verification against the data | 0 (unchanged) | 0 | 0 |

Biggest improvements: invariance row shuffle 102 → 96; Should-tier reference facts stated in the 4,000-character digest, target mode 20 → 21; Should-tier reference facts stated in the 4,000-character digest 32 → 33
Biggest regressions: invariance reverse 87 → 91; facts changed by layout 230 → 240
Accepted regressions: invariance noise 40 → 52 (Issue #374 insight follow-up: with vacuous exact dependencies no longer stated, Adult's report fills to its 100-statement cap with substantive candidates (hotspots, associations), so adding a noise column or shuffling rows moves statements across the cap boundary; driver lists also move with one more predictor. Record: docs/benchmarks/2026-11-18-insight-quality.md.); weighted must 8k 4 → 3 (Issue #374 step 4: the bike-sharing-hourly entry above. Record: docs/benchmarks/2026-11-18-identity-order.md.); Must-tier reference facts stated in the 4,000-character digest 16 → 15 (Issue #374 step 3: the bank-marketing entry above. Record: docs/benchmarks/2026-11-18-null-bars.md.)

Hard line breached: a must-tier reference fact was lost from the 4,000-character digest (default mode): 16 → 15.

32 change(s) recorded. Latest: `79208eb` (2026-11-18), onceover 0.1.6, seeds [0].

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

## Latest change: `79208eb` State how targets move, give models the calendar, drop vacuous and mirrored statements (#374 follow…

Compared with `e3a2c9c`: 6 total metrics better, 2 worse, 4 worse and accepted. Rows marked `total` sum the gated datasets; the other rows name one dataset.

### Better

| metric | where | before | after | change |
|---|---|---:|---:|---:|
| invariance.row_shuffle | total | 102 | 96 | -6 |
| planted.found_4k | total | 57 | 58 | +1 |
| planted.found_full | total | 59 | 60 | +1 |
| redundancy.finding_permille_target | total | 9386 | 9388 | +2 |
| reference.should_4k_default | total | 32 | 33 | +1 |
| reference.should_4k_target | total | 20 | 21 | +1 |
| invariance.noise | absenteeism-at-work | 8 | 6 | -2 |
| invariance.noise | hospital-dha | 1 | 0 | -1 |
| invariance.noise | thyroid | 5 | 3 | -2 |
| invariance.reverse | thyroid | 3 | 1 | -2 |
| invariance.row_shuffle | thyroid | 14 | 6 | -8 |
| invariance.total | absenteeism-at-work | 55 | 54 | -1 |
| invariance.total | hospital-dha | 17 | 16 | -1 |
| invariance.total | thyroid | 22 | 10 | -12 |
| planted.found_4k | hospital-dha | 3 | 4 | +1 |
| planted.found_full | adventureworks | 1 | 2 | +1 |
| redundancy.finding_permille_default | abalone | 925 | 933 | +8 |
| redundancy.finding_permille_default | bike-sharing-hourly | 836 | 838 | +2 |
| redundancy.finding_permille_default | column_order_stress | 944 | 952 | +8 |
| redundancy.finding_permille_default | daily_level_collapse | 759 | 787 | +28 |
| redundancy.finding_permille_default | daily_regime_break | 796 | 814 | +18 |
| redundancy.finding_permille_default | dominant_driver | 870 | 878 | +8 |
| redundancy.finding_permille_default | hospital-dha | 894 | 895 | +1 |
| redundancy.finding_permille_default | nab-nyc-taxi | 449 | 644 | +195 |
| redundancy.finding_permille_default | thyroid | 872 | 878 | +6 |
| redundancy.finding_permille_target | abalone | 921 | 925 | +4 |
| redundancy.finding_permille_target | congressional-voting | 902 | 903 | +1 |
| redundancy.finding_permille_target | dominant_driver | 862 | 870 | +8 |
| redundancy.finding_permille_target | thyroid | 861 | 868 | +7 |
| reference.should_4k_default | abalone | 1 | 2 | +1 |
| reference.should_4k_target | abalone | 1 | 2 | +1 |

### Worse

| metric | where | before | after | change |
|---|---|---:|---:|---:|
| invariance.reverse | total | 87 | 91 | +4 |
| invariance.total | total | 230 | 240 | +10 |
| invariance.reverse | adventureworks | 33 | 39 | +6 |
| invariance.total | adventureworks | 33 | 39 | +6 |
| redundancy.finding_permille_target | absenteeism-at-work | 891 | 890 | -1 |
| redundancy.finding_permille_target | planted_mix | 909 | 900 | -9 |

### Worse and accepted

| metric | where | before | after | change | reason |
|---|---|---:|---:|---:|---|
| invariance.noise | total | 40 | 52 | +12 | Issue #374 insight follow-up: with vacuous exact dependencies no longer stated, Adult's report fills to its 100-statement cap with substantive candidates (hotspots, associations), so adding a noise column or shuffling rows moves statements across the cap boundary; driver lists also move with one more predictor. Record: docs/benchmarks/2026-11-18-insight-quality.md. |
| redundancy.finding_permille_default | total | 16518 | 15959 | -559 | Issue #374 insight follow-up: predictive claims gain a driver-profile clause and short noise tables lose their false finding lines (weighted_noise keeps none, which the per-mille share scores as 0). Record: docs/benchmarks/2026-11-18-insight-quality.md. |
| reference.must_4k_default | total | 16 | 15 | -1 | Issue #374 step 3: the bank-marketing entry above. Record: docs/benchmarks/2026-11-18-null-bars.md. |
| weighted.must_8k | total | 4 | 3 | -1 | Issue #374 step 4: the bike-sharing-hourly entry above. Record: docs/benchmarks/2026-11-18-identity-order.md. |
| invariance.noise | adult-census-income | 7 | 24 | +17 | Issue #374 insight follow-up: with vacuous exact dependencies no longer stated, Adult's report fills to its 100-statement cap with substantive candidates (hotspots, associations), so adding a noise column or shuffling rows moves statements across the cap boundary; driver lists also move with one more predictor. Record: docs/benchmarks/2026-11-18-insight-quality.md. |
| invariance.row_shuffle | absenteeism-at-work | 18 | 19 | +1 | Issue #374 insight follow-up: with vacuous exact dependencies no longer stated, Adult's report fills to its 100-statement cap with substantive candidates (hotspots, associations), so adding a noise column or shuffling rows moves statements across the cap boundary; driver lists also move with one more predictor. Record: docs/benchmarks/2026-11-18-insight-quality.md. |
| invariance.row_shuffle | planted_mix | 1 | 2 | +1 | Issue #374 insight follow-up: with vacuous exact dependencies no longer stated, Adult's report fills to its 100-statement cap with substantive candidates (hotspots, associations), so adding a noise column or shuffling rows moves statements across the cap boundary; driver lists also move with one more predictor. Record: docs/benchmarks/2026-11-18-insight-quality.md. |
| invariance.total | adult-census-income | 9 | 26 | +17 | Issue #374 insight follow-up: with vacuous exact dependencies no longer stated, Adult's report fills to its 100-statement cap with substantive candidates (hotspots, associations), so adding a noise column or shuffling rows moves statements across the cap boundary; driver lists also move with one more predictor. Record: docs/benchmarks/2026-11-18-insight-quality.md. |
| invariance.total | planted_mix | 1 | 2 | +1 | Issue #374 insight follow-up: with vacuous exact dependencies no longer stated, Adult's report fills to its 100-statement cap with substantive candidates (hotspots, associations), so adding a noise column or shuffling rows moves statements across the cap boundary; driver lists also move with one more predictor. Record: docs/benchmarks/2026-11-18-insight-quality.md. |
| redundancy.finding_permille_default | adventureworks | 835 | 834 | -1 | Issue #374 insight follow-up: predictive claims gain a driver-profile clause and short noise tables lose their false finding lines (weighted_noise keeps none, which the per-mille share scores as 0). Record: docs/benchmarks/2026-11-18-insight-quality.md. |
| redundancy.finding_permille_default | metro-interstate-traffic | 846 | 845 | -1 | Issue #374 insight follow-up: predictive claims gain a driver-profile clause and short noise tables lose their false finding lines (weighted_noise keeps none, which the per-mille share scores as 0). Record: docs/benchmarks/2026-11-18-insight-quality.md. |
| redundancy.finding_permille_default | planted_mix | 916 | 912 | -4 | Issue #374 insight follow-up: predictive claims gain a driver-profile clause and short noise tables lose their false finding lines (weighted_noise keeps none, which the per-mille share scores as 0). Record: docs/benchmarks/2026-11-18-insight-quality.md. |
| redundancy.finding_permille_default | weighted_large_cell | 835 | 758 | -77 | Issue #374 insight follow-up: predictive claims gain a driver-profile clause and short noise tables lose their false finding lines (weighted_noise keeps none, which the per-mille share scores as 0). Record: docs/benchmarks/2026-11-18-insight-quality.md. |
| redundancy.finding_permille_default | weighted_noise | 750 | 0 | -750 | Issue #374 insight follow-up: predictive claims gain a driver-profile clause and short noise tables lose their false finding lines (weighted_noise keeps none, which the per-mille share scores as 0). Record: docs/benchmarks/2026-11-18-insight-quality.md. |
| redundancy.finding_permille_target | adult-census-income | 880 | 879 | -1 | Issue #363 on the #374 step-2 base: the screen's larger fitting sample (5,000 to 50,000 rows) shifts which driver-level statements the selection keeps; the finding-line character share moves 880 -> 877. |
| redundancy.finding_permille_target | bank-marketing | 875 | 874 | -1 | Issue #310: the target section's composition changed (the outcome account and per-level outcome contrasts are longer lines); finding-char density moved 883 -> 873 (1.1%) while the section gained the outcome's own statements and the total rose. Record: docs/benchmarks/2026-11-17-pointed-mode.md. |
| redundancy.finding_permille_target | metro-interstate-traffic | 839 | 833 | -6 | Issue #310: same composition change as bank-marketing; 846 -> 832 (1.7%), total rose. Record: docs/benchmarks/2026-11-17-pointed-mode.md. |
| reference.must_4k_default | bank-marketing | 2 | 1 | -1 | Issue #374 step 3: subscription-duration is stated (8k digest and full report) by a level contrast that now ranks just below the 4k cut: consolidating per-level contrasts into one profile per (grouping, measure) frees the pass's 20-statement cap for more distinct pairs, which score above it under the hand-set level-contrast score (step 6 of #374 replaces that score). Record: docs/benchmarks/2026-11-18-null-bars.md. |
| weighted.must_8k | bike-sharing-hourly | 1 | 0 | -1 | Issue #374 step 4: in the weights-hinted run the rush-hour concentration (still stated in the full report) falls out of the 8k digest: cnt, registered and casual are now explained by hr, temp and workingday instead of by their own sum's parts, and those statements take the digest space. Record: docs/benchmarks/2026-11-18-identity-order.md. |

## Every total metric, last 8 changes

Oldest on the left. A value in a column is followed by its movement against the column before it.

| metric | reading | `94f381b` | `11dfa8e` | `114fff6` | `de041e2` | `7cc00d4` | `4a18255` | `e3a2c9c` | `79208eb` |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| planted.planted | information only | 76 | 76 | 76 | 76 | 76 | 76 | 76 | 76 |
| planted.found_full | higher is better | 58 | 58 | 58 | 58 | 58 | 58 | 59 (+1, better) | 60 (+1, better) |
| planted.found_4k | higher is better | 56 | 56 | 56 | 56 | 56 | 56 | 57 (+1, better) | 58 (+1, better) |
| planted.line_sum_4k | information only | 630 | 630 | 630 | 630 | 628 (-2) | 628 | 635 (+7) | 593 (-42) |
| permuted.false_full | lower is better | 100 | 100 | 100 | 100 | 86 (-14, better) | 86 | 39 (-47, better) | 39 |
| permuted.false_4k | lower is better | 48 | 48 | 48 | 48 | 46 (-2, better) | 46 | 22 (-24, better) | 22 |
| permuted.false_full_target | lower is better | 51 | 51 | 51 | 51 | 47 (-4, better) | 47 | 17 (-30, better) | 17 |
| permuted.false_4k_target | lower is better | 30 | 30 | 30 | 30 | 28 (-2, better) | 28 | 10 (-18, better) | 10 |
| permuted.findings_full | information only | 318 | 318 | 318 | 318 | 304 (-14) | 304 | 257 (-47) | 215 (-42) |
| invariance.reverse | lower is better | 199 | 199 | 200 (+1, worse) | 200 | 197 (-3, better) | 185 (-12, better) | 87 (-98, better) | 91 (+4, worse) |
| invariance.row_shuffle | lower is better | 106 | 106 | 106 | 106 | 105 (-1, better) | 98 (-7, better) | 102 (+4, worse) | 96 (-6, better) |
| invariance.rename | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| invariance.noise | lower is better | 29 | 29 | 29 | 29 | 30 (+1, worse) | 30 | 40 (+10, worse) | 52 (+12, worse) |
| invariance.scale | lower is better | 2 | 2 | 2 | 2 | 2 | 2 | 1 (-1, better) | 1 |
| invariance.total | lower is better | 336 | 336 | 337 (+1, worse) | 337 | 334 (-3, better) | 315 (-19, better) | 230 (-85, better) | 240 (+10, worse) |
| reference.check_failed | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_total | information only | 19 | 19 | 19 | 19 | 19 | 19 | 19 | 19 |
| reference.must_4k_default | higher is better | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 15 (-1, worse) |
| reference.must_8k_default | higher is better | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 16 |
| reference.must_full_default | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_4k_target | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_8k_target | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_full_target | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.should_total | information only | 45 | 45 | 45 | 45 | 45 | 45 | 45 | 45 |
| reference.should_4k_default | higher is better | 29 | 29 | 29 | 29 | 31 (+2, better) | 31 | 32 (+1, better) | 33 (+1, better) |
| reference.should_8k_default | higher is better | 36 | 36 | 36 | 36 | 36 | 36 | 35 (-1, worse) | 35 |
| reference.should_full_default | higher is better | 37 | 37 | 37 | 37 | 38 (+1, better) | 38 | 37 (-1, worse) | 37 |
| reference.should_4k_target | higher is better | 18 | 18 | 18 | 18 | 19 (+1, better) | 20 (+1, better) | 20 | 21 (+1, better) |
| reference.should_8k_target | higher is better | 23 | 23 | 23 | 23 | 23 | 24 (+1, better) | 23 (-1, worse) | 23 |
| reference.should_full_target | higher is better | 24 | 24 | 24 | 24 | 24 | 24 | 23 (-1, worse) | 23 |
| reference.must_not_total | information only | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 |
| reference.must_not_4k_default | lower is better | 8 | 8 | 8 | 8 | 0 (-8, better) | 0 | 0 | 0 |
| reference.must_not_8k_default | lower is better | 18 | 18 | 18 | 18 | 1 (-17, better) | 1 | 0 (-1, better) | 0 |
| reference.must_not_full_default | lower is better | 81 | 81 | 81 | 81 | 22 (-59, better) | 22 | 0 (-22, better) | 0 |
| reference.must_not_4k_target | lower is better | 3 | 3 | 3 | 3 | 0 (-3, better) | 0 | 0 | 0 |
| reference.must_not_8k_target | lower is better | 16 | 16 | 16 | 16 | 1 (-15, better) | 1 | 0 (-1, better) | 0 |
| reference.must_not_full_target | lower is better | 60 | 60 | 60 | 60 | 14 (-46, better) | 14 | 0 (-14, better) | 0 |
| redundancy.repeated_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.finding_lines_default | information only | 272 | 272 | 272 | 272 | 271 (-1) | 272 (+1) | 269 (-3) | 249 (-20) |
| redundancy.finding_permille_default | higher is better | 16636 | 16636 | 16636 | 16636 | 16631 (-5, worse) | 16632 (+1, better) | 16518 (-114, worse) | 15959 (-559, worse) |
| redundancy.long_decimals_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.repeated_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.finding_lines_target | information only | 193 | 193 | 193 | 193 | 195 (+2) | 193 (-2) | 191 (-2) | 180 (-11) |
| redundancy.finding_permille_target | higher is better | 9406 | 9406 | 9406 | 9406 | 9405 (-1, worse) | 9405 | 9386 (-19, worse) | 9388 (+2, better) |
| redundancy.long_decimals_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.mismatch | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.error | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.ok | information only | 907 | 907 | 907 | 907 | 902 (-5) | 902 | 868 (-34) | 860 (-8) |
| truth.not_checkable | information only | 1524 | 1524 | 1537 (+13) | 1537 | 1508 (-29) | 1505 (-3) | 1388 (-117) | 1329 (-59) |
| cost.profile_seconds | lower is better | 11.502 (-3.447, better) | 11.688 (+0.186) | 10.95 | 10.752 (-0.198) | 9.663 (-1.089) | 10.53 | 10.82 | 10.846 (+0.026) |
| cost.json_chars | information only | 3877987 | 3877987 | 3905682 (+27695) | 3905682 | 3856558 (-49124) | 3845449 (-11109) | 3587335 (-258114) | 3611452 (+24117) |
| cost.findings | information only | 1203 | 1203 | 1215 (+12) | 1215 | 1204 (-11) | 1201 (-3) | 1097 (-104) | 1050 (-47) |
| weighted.must_total | information only | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| weighted.must_4k | higher is better | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 |
| weighted.must_8k | higher is better | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 3 (-1, worse) |
| weighted.must_full | higher is better | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| weighted.should_total | information only | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_4k | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_8k | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_full | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.weighted_findings | information only | 15 | 15 | 15 | 15 | 19 (+4) | 19 | 19 | 20 (+1) |

## Latest scorecard per dataset

| dataset | kind | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | restatements in the full report | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| adult-census-income | tabular, mixed types | 5 | 4 | 4 | 0 | 0 | 26 | 0 | 0 | 1.627 |
| bank-marketing | tabular, mixed types | 3 | 1 | 2 | 0 | 0 | 18 | 0 | 0 | 1.847 |
| abalone | tabular, correlated measures | 5 | 3 | 3 | 0 | 0 | 5 | 0 | 0 | 0.102 |
| bike-sharing-hourly | hourly series with calendar columns | 4 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0.819 |
| metro-interstate-traffic | hourly time series | 5 | - | - | - | 2 | 6 | 0 | 0 | 1.417 |
| thyroid | clinical, many flags and missing values | 1 | - | - | - | 0 | 10 | 0 | 0 | 0.464 |
| absenteeism-at-work | small business table | 2 | - | - | - | 2 | 54 | 0 | 0 | 0.158 |
| hospital-dha | wide numeric, 64 columns | 4 | - | - | - | 0 | 16 | 0 | 0 | 0.6 |
| congressional-voting | categorical votes | 1 | - | - | - | 2 | 49 | 0 | 0 | 0.046 |
| adventureworks | sales orders | 0 | - | - | - | 6 | 39 | 0 | 0 | 2.039 |
| nab-nyc-taxi | single time series | 3 | - | - | - | 0 | 0 | 0 | 0 | 0.07 |
| state-unemployment | wide time series, one column per state | 0 | - | - | - | 4 | 6 | 0 | 0 | 1.123 |
| state-unemployment-long | panel, long form | 0 | 1 | 1 | 0 | 2 | 0 | 0 | 0 | 0.104 |
| south-german-credit (held out) | credit table | 5 | 0 | 0 | 0 | 1 | 10 | 0 | 0 | 0.13 |
| soybean (held out) | categorical agronomy table | 0 | - | - | - | 0 | 37 | 0 | 0 | 0.187 |
| lace-satellites (held out) | satellite catalogue | 1 | - | - | - | 4 | 53 | 0 | 0 | 0.182 |
| sec-edgar-company-facts (held out) | event log, filing events | 2 | 3 | 3 | 0 | 3 | 37 | 0 | 0 | 0.381 |
| gse15258-expression (held out) | wide table, 86 samples x 5,006 columns | - | 0 | 0 | 0 | - | - | - | - | - |
| column_order_stress | generated numeric battery, latent factors | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.137 |
| planted_mix | generated application records with plants | 4 | 2 | 3 | 0 | 0 | 2 | 0 | 0 | 0.108 |
| daily_regime_break | generated daily series, planted breaks | 4 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0.028 |
| dominant_driver | generated listings, numeric target | 5 | 2 | 2 | 0 | 0 | 1 | 0 | 0 | 0.075 |
| daily_level_collapse | generated daily series, planted sustained collapse | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.019 |
| marginal_ranges (held out) | generated range battery | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.025 |
| messy_intake (held out) | generated messy intake records | 1 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0.011 |
| weighted_noise | generated control: independent noise weight | 4 | - | - | - | 0 | 0 | 0 | 0 | 0.038 |
| weighted_large_cell | generated control: merely large cell | 4 | - | - | - | 0 | 1 | 0 | 0 | 0.025 |

## Held-out datasets

These datasets are scored and left out of the pass/fail comparison. A gain on the gated datasets that these do not share points to tuning on the gated datasets.

| date | commit | change | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | restatements in the full report | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
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
