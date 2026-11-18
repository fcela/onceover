# Gate report

## TL;DR

Latest change `c20279a` (2026-11-18), Merge pull request #453 from fcela/fix/five-immediate-priorities: compared with `e495b4c`, 12 total metrics better, 0 worse, 4 worse and accepted.

| measure | latest (`c20279a`) | previous (`e495b4c`) | first recorded (`1618886`) |
|---|---:|---:|---:|
| Must-tier reference facts stated in the 4,000-character digest | 15 of 19 (unchanged) | 15 of 19 | 7 of 11 |
| Must-tier reference facts stated in the 4,000-character digest, target mode | 17 of 19 (unchanged) | 17 of 19 | 7 of 11 |
| Should-tier reference facts stated in the 4,000-character digest | 32 of 45 (-1, worse) | 33 of 45 | 7 of 12 |
| Should-tier reference facts stated in the 4,000-character digest, target mode | 20 of 45 (-1, worse) | 21 of 45 | 8 of 12 |
| Planted structures stated in the 4,000-character digest | 59 (+1, better) | 58 | 20 |
| Facts repeated in the 4,000-character digest | 0 (unchanged) | 0 | 4 |
| Facts repeated in the 4,000-character digest, target mode | 0 (unchanged) | 0 | 3 |
| Relationship statements in the 4,000-character digest of shuffled tables (all false by construction) | 21 (-1, better) | 22 | 136 |
| Statements that fail verification against the data | 0 (unchanged) | 0 | 0 |

Biggest improvements: invariance row shuffle 95 → 11; facts changed by layout 239 → 130; invariance noise 52 → 31
Biggest regressions: none.
Accepted regressions: profile seconds 10.867 → 15.716 (Standard-tier profiles pay for the screen's raised fitting-row cap (5,000 to 50,000 rows per target, issue #363): the gate total rises 41.5s to 56.5s on this base, concentrated in the datasets over 5,000 rows. The trade is measured and priced in docs/benchmarks/2026-11-17-sample-caps.md.); Should-tier reference facts stated in the 4,000-character digest, target mode 21 → 20 (Issue #374 step 3: dominant_driver age-weak (above). Record: docs/benchmarks/2026-11-18-null-bars.md.); reference should 8k target 23 → 22 (Issue #374 step 3: dominant_driver age-weak (above). Record: docs/benchmarks/2026-11-18-null-bars.md.)

Hard lines: none breached — no statement failed or errored in verification, and no must-tier reference fact was lost from the 4,000-character digest.

36 change(s) recorded. Latest: `c20279a` (2026-11-18), onceover 0.1.6, seeds [0].

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

## Latest change: `c20279a` Merge pull request #453 from fcela/fix/five-immediate-priorities

Compared with `e495b4c`: 12 total metrics better, 0 worse, 4 worse and accepted. Rows marked `total` sum the gated datasets; the other rows name one dataset.

### Better

| metric | where | before | after | change |
|---|---|---:|---:|---:|
| invariance.noise | total | 52 | 31 | -21 |
| invariance.reverse | total | 91 | 87 | -4 |
| invariance.row_shuffle | total | 95 | 11 | -84 |
| invariance.total | total | 239 | 130 | -109 |
| permuted.false_4k | total | 22 | 21 | -1 |
| permuted.false_4k_target | total | 10 | 9 | -1 |
| permuted.false_full | total | 39 | 38 | -1 |
| permuted.false_full_target | total | 17 | 16 | -1 |
| planted.found_4k | total | 58 | 59 | +1 |
| planted.found_full | total | 59 | 60 | +1 |
| redundancy.finding_permille_default | total | 15947 | 15952 | +5 |
| redundancy.finding_permille_target | total | 9389 | 9391 | +2 |
| invariance.noise | adult-census-income | 24 | 4 | -20 |
| invariance.noise | congressional-voting | 14 | 11 | -3 |
| invariance.noise | thyroid | 3 | 1 | -2 |
| invariance.reverse | absenteeism-at-work | 29 | 26 | -3 |
| invariance.reverse | congressional-voting | 3 | 2 | -1 |
| invariance.row_shuffle | abalone | 5 | 0 | -5 |
| invariance.row_shuffle | absenteeism-at-work | 19 | 2 | -17 |
| invariance.row_shuffle | adult-census-income | 2 | 0 | -2 |
| invariance.row_shuffle | bank-marketing | 12 | 2 | -10 |
| invariance.row_shuffle | column_order_stress | 7 | 5 | -2 |
| invariance.row_shuffle | congressional-voting | 32 | 0 | -32 |
| invariance.row_shuffle | hospital-dha | 10 | 0 | -10 |
| invariance.row_shuffle | thyroid | 6 | 0 | -6 |
| invariance.total | abalone | 5 | 0 | -5 |
| invariance.total | absenteeism-at-work | 54 | 34 | -20 |
| invariance.total | adult-census-income | 26 | 4 | -22 |
| invariance.total | bank-marketing | 18 | 10 | -8 |
| invariance.total | congressional-voting | 49 | 13 | -36 |
| invariance.total | hospital-dha | 16 | 6 | -10 |
| invariance.total | thyroid | 10 | 2 | -8 |
| permuted.false_4k | congressional-voting | 2 | 1 | -1 |
| permuted.false_4k_target | congressional-voting | 2 | 1 | -1 |
| permuted.false_full | congressional-voting | 2 | 1 | -1 |
| permuted.false_full_target | congressional-voting | 2 | 1 | -1 |
| planted.found_4k | absenteeism-at-work | 2 | 3 | +1 |
| planted.found_full | absenteeism-at-work | 3 | 4 | +1 |
| redundancy.finding_permille_default | abalone | 933 | 934 | +1 |
| redundancy.finding_permille_default | absenteeism-at-work | 903 | 905 | +2 |
| redundancy.finding_permille_default | adult-census-income | 888 | 890 | +2 |
| redundancy.finding_permille_default | bike-sharing-hourly | 838 | 839 | +1 |
| redundancy.finding_permille_default | column_order_stress | 952 | 953 | +1 |
| redundancy.finding_permille_default | congressional-voting | 908 | 910 | +2 |
| redundancy.finding_permille_target | absenteeism-at-work | 890 | 891 | +1 |
| redundancy.finding_permille_target | adult-census-income | 879 | 880 | +1 |
| redundancy.finding_permille_target | bank-marketing | 874 | 876 | +2 |

### Worse

None.

### Worse and accepted

| metric | where | before | after | change | reason |
|---|---|---:|---:|---:|---|
| cost.profile_seconds | total | 10.867 | 15.716 | +4.849 | Standard-tier profiles pay for the screen's raised fitting-row cap (5,000 to 50,000 rows per target, issue #363): the gate total rises 41.5s to 56.5s on this base, concentrated in the datasets over 5,000 rows. The trade is measured and priced in docs/benchmarks/2026-11-17-sample-caps.md. |
| reference.should_4k_default | total | 33 | 32 | -1 | Issue #374 step 3: dominant_driver age-weak (above). Record: docs/benchmarks/2026-11-18-null-bars.md. |
| reference.should_4k_target | total | 21 | 20 | -1 | Issue #374 step 3: dominant_driver age-weak (above). Record: docs/benchmarks/2026-11-18-null-bars.md. |
| reference.should_8k_target | total | 23 | 22 | -1 | Issue #374 step 3: dominant_driver age-weak (above). Record: docs/benchmarks/2026-11-18-null-bars.md. |
| invariance.noise | bank-marketing | 2 | 4 | +2 | Issue #374 step 4: the predictive screen's holdout and fold splits are now seeded from each target column's value histogram instead of its position, so reordering or renaming columns no longer changes the models (invariance.reverse 191 -> 87, rename 0 -> 0, total 319 -> 238). Adding a column or shuffling rows still moves the drivers each model names above 5% of gain; the new seeds draw a different realization of that pre-existing instability. Record: docs/benchmarks/2026-11-18-identity-order.md. Also: One additional noise-sensitive finding on bank-marketing with the screen's larger fitting sample (issue #363). The noise total is unchanged at 29 and adventureworks improves by 2. |
| invariance.noise | column_order_stress | 0 | 2 | +2 | Issue #435: adding an independent noise column changes two model-selected facts (0 -> 2), while row-shuffle differences on this table fall 7 -> 5 and its invariance total stays 7. Record: docs/benchmarks/2026-11-18-content-sampling.md. |
| redundancy.finding_permille_default | dominant_driver | 878 | 877 | -1 | Issue #435: rank-derived row keys redraw sample-dependent statements; the digest finding-character share moves 878 -> 877 per mille. Record: docs/benchmarks/2026-11-18-content-sampling.md. |
| redundancy.finding_permille_default | hospital-dha | 895 | 893 | -2 | Issue #435: rank-derived row keys redraw sample-dependent statements; the digest finding-character share moves 895 -> 893 per mille. Record: docs/benchmarks/2026-11-18-content-sampling.md. |
| redundancy.finding_permille_default | thyroid | 878 | 877 | -1 | Issue #435: rank-derived row keys redraw sample-dependent statements; the digest finding-character share moves 878 -> 877 per mille. Record: docs/benchmarks/2026-11-18-content-sampling.md. |
| redundancy.finding_permille_target | abalone | 925 | 924 | -1 | Issue #435: the target-mode digest finding-character share moves 925 -> 924 per mille when content keys replace positional splits. Record: docs/benchmarks/2026-11-18-content-sampling.md. |
| redundancy.finding_permille_target | congressional-voting | 904 | 903 | -1 | Issue #435: the target-mode digest finding-character share moves 904 -> 903 per mille when content keys replace positional splits. Record: docs/benchmarks/2026-11-18-content-sampling.md. |
| reference.should_4k_default | bank-marketing | 2 | 1 | -1 | Issue #374 step 3: dominant_driver age-weak (above). Record: docs/benchmarks/2026-11-18-null-bars.md. |
| reference.should_4k_target | bank-marketing | 3 | 2 | -1 | Issue #374 step 3: dominant_driver age-weak (above). Record: docs/benchmarks/2026-11-18-null-bars.md. |
| reference.should_8k_target | bank-marketing | 4 | 3 | -1 | Issue #374 step 3: dominant_driver age-weak (above). Record: docs/benchmarks/2026-11-18-null-bars.md. |

## Every total metric, last 8 changes

Oldest on the left. A value in a column is followed by its movement against the column before it.

| metric | reading | `7cc00d4` | `4a18255` | `e3a2c9c` | `79208eb` | `df24a40` | `3e45788` | `e495b4c` | `c20279a` |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| planted.planted | information only | 76 | 76 | 76 | 76 | 76 | 76 | 76 | 76 |
| planted.found_full | higher is better | 58 | 58 | 59 (+1, better) | 60 (+1, better) | 60 | 60 | 59 (-1, worse) | 60 (+1, better) |
| planted.found_4k | higher is better | 56 | 56 | 57 (+1, better) | 58 (+1, better) | 58 | 58 | 58 | 59 (+1, better) |
| planted.line_sum_4k | information only | 628 (-2) | 628 | 635 (+7) | 593 (-42) | 593 | 592 (-1) | 593 (+1) | 606 (+13) |
| permuted.false_full | lower is better | 86 (-14, better) | 86 | 39 (-47, better) | 39 | 39 | 39 | 39 | 38 (-1, better) |
| permuted.false_4k | lower is better | 46 (-2, better) | 46 | 22 (-24, better) | 22 | 22 | 22 | 22 | 21 (-1, better) |
| permuted.false_full_target | lower is better | 47 (-4, better) | 47 | 17 (-30, better) | 17 | 17 | 17 | 17 | 16 (-1, better) |
| permuted.false_4k_target | lower is better | 28 (-2, better) | 28 | 10 (-18, better) | 10 | 10 | 10 | 10 | 9 (-1, better) |
| permuted.findings_full | information only | 304 (-14) | 304 | 257 (-47) | 215 (-42) | 215 | 215 | 224 (+9) | 223 (-1) |
| invariance.reverse | lower is better | 197 (-3, better) | 185 (-12, better) | 87 (-98, better) | 91 (+4, worse) | 91 | 91 | 91 | 87 (-4, better) |
| invariance.row_shuffle | lower is better | 105 (-1, better) | 98 (-7, better) | 102 (+4, worse) | 96 (-6, better) | 96 | 96 | 95 (-1, better) | 11 (-84, better) |
| invariance.rename | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| invariance.noise | lower is better | 30 (+1, worse) | 30 | 40 (+10, worse) | 52 (+12, worse) | 52 | 52 | 52 | 31 (-21, better) |
| invariance.scale | lower is better | 2 | 2 | 1 (-1, better) | 1 | 1 | 1 | 1 | 1 |
| invariance.total | lower is better | 334 (-3, better) | 315 (-19, better) | 230 (-85, better) | 240 (+10, worse) | 240 | 240 | 239 (-1, better) | 130 (-109, better) |
| reference.check_failed | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_total | information only | 19 | 19 | 19 | 19 | 19 | 19 | 19 | 19 |
| reference.must_4k_default | higher is better | 16 | 16 | 16 | 15 (-1, worse) | 15 | 15 | 15 | 15 |
| reference.must_8k_default | higher is better | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 16 |
| reference.must_full_default | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_4k_target | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_8k_target | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.must_full_target | higher is better | 17 | 17 | 17 | 17 | 17 | 17 | 17 | 17 |
| reference.should_total | information only | 45 | 45 | 45 | 45 | 45 | 45 | 45 | 45 |
| reference.should_4k_default | higher is better | 31 (+2, better) | 31 | 32 (+1, better) | 33 (+1, better) | 33 | 33 | 33 | 32 (-1, worse) |
| reference.should_8k_default | higher is better | 36 | 36 | 35 (-1, worse) | 35 | 35 | 35 | 35 | 35 |
| reference.should_full_default | higher is better | 38 (+1, better) | 38 | 37 (-1, worse) | 37 | 37 | 37 | 37 | 37 |
| reference.should_4k_target | higher is better | 19 (+1, better) | 20 (+1, better) | 20 | 21 (+1, better) | 21 | 21 | 21 | 20 (-1, worse) |
| reference.should_8k_target | higher is better | 23 | 24 (+1, better) | 23 (-1, worse) | 23 | 23 | 23 | 23 | 22 (-1, worse) |
| reference.should_full_target | higher is better | 24 | 24 | 23 (-1, worse) | 23 | 23 | 23 | 23 | 23 |
| reference.must_not_total | information only | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 |
| reference.must_not_4k_default | lower is better | 0 (-8, better) | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_8k_default | lower is better | 1 (-17, better) | 1 | 0 (-1, better) | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_full_default | lower is better | 22 (-59, better) | 22 | 0 (-22, better) | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_4k_target | lower is better | 0 (-3, better) | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_8k_target | lower is better | 1 (-15, better) | 1 | 0 (-1, better) | 0 | 0 | 0 | 0 | 0 |
| reference.must_not_full_target | lower is better | 14 (-46, better) | 14 | 0 (-14, better) | 0 | 0 | 0 | 0 | 0 |
| redundancy.repeated_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.finding_lines_default | information only | 271 (-1) | 272 (+1) | 269 (-3) | 249 (-20) | 249 | 249 | 248 (-1) | 248 |
| redundancy.finding_permille_default | higher is better | 16631 (-5, worse) | 16632 (+1, better) | 16518 (-114, worse) | 15959 (-559, worse) | 15959 | 15959 | 15947 (-12, worse) | 15952 (+5, better) |
| redundancy.long_decimals_default | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.repeated_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| redundancy.finding_lines_target | information only | 195 (+2) | 193 (-2) | 191 (-2) | 180 (-11) | 180 | 180 | 181 (+1) | 179 (-2) |
| redundancy.finding_permille_target | higher is better | 9405 (-1, worse) | 9405 | 9386 (-19, worse) | 9388 (+2, better) | 9388 | 9387 (-1, worse) | 9389 (+2, better) | 9391 (+2, better) |
| redundancy.long_decimals_target | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.mismatch | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.error | lower is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| truth.ok | information only | 902 (-5) | 902 | 868 (-34) | 860 (-8) | 860 | 860 | 861 (+1) | 870 (+9) |
| truth.not_checkable | information only | 1508 (-29) | 1505 (-3) | 1388 (-117) | 1329 (-59) | 1329 | 1329 | 1356 (+27) | 1359 (+3) |
| cost.profile_seconds | lower is better | 9.663 (-1.089) | 10.53 | 10.82 | 10.846 (+0.026) | 10.883 (+0.037) | 10.79 (-0.093) | 10.867 (+0.077) | 15.716 (+4.849, worse) |
| cost.json_chars | information only | 3856558 (-49124) | 3845449 (-11109) | 3587335 (-258114) | 3611452 (+24117) | 3611452 | 3611452 | 3638120 (+26668) | 3637758 (-362) |
| cost.findings | information only | 1204 (-11) | 1201 (-3) | 1097 (-104) | 1050 (-47) | 1050 | 1050 | 1065 (+15) | 1074 (+9) |
| weighted.must_total | information only | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| weighted.must_4k | higher is better | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 |
| weighted.must_8k | higher is better | 4 | 4 | 4 | 3 (-1, worse) | 3 | 3 | 3 | 3 |
| weighted.must_full | higher is better | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| weighted.should_total | information only | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_4k | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_8k | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.should_full | higher is better | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| weighted.weighted_findings | information only | 19 (+4) | 19 | 19 | 20 (+1) | 20 | 20 | 19 (-1) | 19 |

## Latest scorecard per dataset

| dataset | kind | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | restatements in the full report | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| adult-census-income | tabular, mixed types | 5 | 4 | 4 | 0 | 0 | 4 | 0 | 0 | 2.593 |
| bank-marketing | tabular, mixed types | 3 | 1 | 2 | 0 | 0 | 10 | 0 | 0 | 2.821 |
| abalone | tabular, correlated measures | 5 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0.15 |
| bike-sharing-hourly | hourly series with calendar columns | 4 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 1.182 |
| metro-interstate-traffic | hourly time series | 5 | - | - | - | 2 | 6 | 0 | 0 | 2.065 |
| thyroid | clinical, many flags and missing values | 1 | - | - | - | 0 | 2 | 0 | 0 | 0.492 |
| absenteeism-at-work | small business table | 3 | - | - | - | 2 | 34 | 0 | 0 | 0.165 |
| hospital-dha | wide numeric, 64 columns | 4 | - | - | - | 0 | 6 | 0 | 0 | 0.623 |
| congressional-voting | categorical votes | 1 | - | - | - | 1 | 13 | 0 | 0 | 0.052 |
| adventureworks | sales orders | 0 | - | - | - | 6 | 39 | 0 | 0 | 3.37 |
| nab-nyc-taxi | single time series | 3 | - | - | - | 0 | 0 | 0 | 0 | 0.094 |
| state-unemployment | wide time series, one column per state | 0 | - | - | - | 4 | 6 | 0 | 0 | 1.182 |
| state-unemployment-long | panel, long form | 0 | 1 | 1 | 0 | 2 | 0 | 0 | 0 | 0.21 |
| south-german-credit (held out) | credit table | 5 | 0 | 0 | 0 | 1 | 6 | 0 | 0 | 0.145 |
| soybean (held out) | categorical agronomy table | 0 | - | - | - | 2 | 23 | 0 | 0 | 0.225 |
| lace-satellites (held out) | satellite catalogue | 1 | - | - | - | 4 | 24 | 0 | 0 | 0.219 |
| sec-edgar-company-facts (held out) | event log, filing events | 2 | 3 | 3 | 0 | 3 | 32 | 0 | 0 | 0.564 |
| gse15258-expression (held out) | wide table, 86 samples x 5,006 columns | - | 0 | 0 | 0 | - | - | - | - | - |
| column_order_stress | generated numeric battery, latent factors | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.186 |
| planted_mix | generated application records with plants | 4 | 2 | 3 | 0 | 0 | 1 | 0 | 0 | 0.222 |
| daily_regime_break | generated daily series, planted breaks | 4 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0.035 |
| dominant_driver | generated listings, numeric target | 5 | 2 | 2 | 0 | 0 | 1 | 0 | 0 | 0.123 |
| daily_level_collapse | generated daily series, planted sustained collapse | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.02 |
| marginal_ranges (held out) | generated range battery | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.041 |
| messy_intake (held out) | generated messy intake records | 1 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0.039 |
| weighted_noise | generated control: independent noise weight | 4 | - | - | - | 0 | 0 | 0 | 0 | 0.074 |
| weighted_large_cell | generated control: merely large cell | 4 | - | - | - | 0 | 1 | 0 | 0 | 0.057 |

## Held-out datasets

These datasets are scored and left out of the pass/fail comparison. A gain on the gated datasets that these do not share points to tuning on the gated datasets.

| date | commit | change | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | restatements in the full report | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
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
