# Gate report

## TL;DR

Latest change `2601a72` (2026-11-16), Merge pull request #326 from fcela/feat/gate-report-tldr: compared with `daf7a79`, 0 total metrics better, 0 worse, 0 worse and accepted.

| measure | latest (`2601a72`) | previous (`daf7a79`) | first recorded (`1618886`) |
|---|---:|---:|---:|
| Must-tier reference facts stated in the 4,000-character digest | 7 of 11 (unchanged) | 7 of 11 | 7 of 11 |
| Must-tier reference facts stated in the 4,000-character digest, target mode | 7 of 11 (unchanged) | 7 of 11 | 7 of 11 |
| Should-tier reference facts stated in the 4,000-character digest | 7 of 12 (unchanged) | 7 of 12 | 7 of 12 |
| Should-tier reference facts stated in the 4,000-character digest, target mode | 8 of 12 (unchanged) | 8 of 12 | 8 of 12 |
| Planted structures stated in the 4,000-character digest | 20 (unchanged) | 20 | 20 |
| Facts repeated in the 4,000-character digest | 4 (unchanged) | 4 | 4 |
| Facts repeated in the 4,000-character digest, target mode | 3 (unchanged) | 3 | 3 |
| Relationship statements in the 4,000-character digest of shuffled tables (all false by construction) | 136 (unchanged) | 136 | 136 |
| Statements that fail verification against the data | 0 (unchanged) | 0 | 0 |

Biggest improvements: none.
Biggest regressions: none.
Accepted regressions: none.

Hard lines: none breached — no statement failed or errored in verification, and no must-tier reference fact was lost from the 4,000-character digest.

4 change(s) recorded. Latest: `2601a72` (2026-11-16), onceover 0.1.5, seeds [0].

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
| 2026-11-16 | `2601a72` | Merge pull request #326 from fcela/feat/gate-report-tldr | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 12.333 (+0.008) |
| 2026-11-15 | `daf7a79` | Merge pull request #324 from fcela/feat/315-corpus-targets | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 12.325 |
| 2026-11-15 | `ca275fd` | Merge pull request #323 from fcela/feat/317-gate-history-report | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 13.385 |
| 2026-11-15 | `1618886` | Merge pull request #320 from fcela/fix/295-agent-token-economy | 20 | 7 | 7 | 136 | 408 | 4 | 0 | 6.859 |

## Latest change: `2601a72` Merge pull request #326 from fcela/feat/gate-report-tldr

Compared with `daf7a79`: 0 total metrics better, 0 worse, 0 worse and accepted. Rows marked `total` sum the gated datasets; the other rows name one dataset.

### Better

None.

### Worse

None.

### Worse and accepted

None.

## Every total metric, last 4 changes

Oldest on the left. A value in a column is followed by its movement against the column before it.

| metric | reading | `1618886` | `ca275fd` | `daf7a79` | `2601a72` |
|---|---|---:|---:|---:|---:|
| planted.planted | information only | 46 | 46 | 46 | 46 |
| planted.found_full | higher is better | 34 | 34 | 34 | 34 |
| planted.found_4k | higher is better | 20 | 20 | 20 | 20 |
| planted.line_sum_4k | information only | 253 | 253 | 253 | 253 |
| permuted.false_full | lower is better | 144 | 144 | 144 | 144 |
| permuted.false_4k | lower is better | 136 | 136 | 136 | 136 |
| permuted.false_full_target | lower is better | 75 | 75 | 75 | 75 |
| permuted.false_4k_target | lower is better | 67 | 67 | 67 | 67 |
| permuted.findings_full | information only | 333 | 333 | 333 | 333 |
| invariance.reverse | lower is better | 269 | 269 | 269 | 269 |
| invariance.row_shuffle | lower is better | 104 | 104 | 104 | 104 |
| invariance.rename | lower is better | 0 | 0 | 0 | 0 |
| invariance.noise | lower is better | 30 | 30 | 30 | 30 |
| invariance.scale | lower is better | 5 | 5 | 5 | 5 |
| invariance.total | lower is better | 408 | 408 | 408 | 408 |
| reference.check_failed | lower is better | 0 | 0 | 0 | 0 |
| reference.must_total | information only | 11 | 11 | 11 | 11 |
| reference.must_4k_default | higher is better | 7 | 7 | 7 | 7 |
| reference.must_8k_default | higher is better | 11 | 11 | 11 | 11 |
| reference.must_full_default | higher is better | 11 | 11 | 11 | 11 |
| reference.must_4k_target | higher is better | 7 | 7 | 7 | 7 |
| reference.must_8k_target | higher is better | 11 | 11 | 11 | 11 |
| reference.must_full_target | higher is better | 11 | 11 | 11 | 11 |
| reference.should_total | information only | 12 | 12 | 12 | 12 |
| reference.should_4k_default | higher is better | 7 | 7 | 7 | 7 |
| reference.should_8k_default | higher is better | 9 | 9 | 9 | 9 |
| reference.should_full_default | higher is better | 11 | 11 | 11 | 11 |
| reference.should_4k_target | higher is better | 8 | 8 | 8 | 8 |
| reference.should_8k_target | higher is better | 8 | 8 | 8 | 8 |
| reference.should_full_target | higher is better | 10 | 10 | 10 | 10 |
| redundancy.repeated_default | lower is better | 4 | 4 | 4 | 4 |
| redundancy.finding_lines_default | information only | 111 | 111 | 111 | 111 |
| redundancy.finding_permille_default | higher is better | 7118 | 7118 | 7118 | 7118 |
| redundancy.long_decimals_default | lower is better | 53 | 53 | 53 | 53 |
| redundancy.repeated_target | lower is better | 3 | 3 | 3 | 3 |
| redundancy.finding_lines_target | information only | 69 | 69 | 69 | 69 |
| redundancy.finding_permille_target | higher is better | 4531 | 4531 | 4531 | 4531 |
| redundancy.long_decimals_target | lower is better | 41 | 41 | 41 | 41 |
| truth.mismatch | lower is better | 0 | 0 | 0 | 0 |
| truth.error | lower is better | 0 | 0 | 0 | 0 |
| truth.ok | information only | 858 | 858 | 858 | 858 |
| truth.not_checkable | information only | 1423 | 1423 | 1423 | 1423 |
| cost.profile_seconds | lower is better | 6.859 | 13.385 | 12.325 | 12.333 (+0.008) |
| cost.json_chars | information only | 3335873 | 3335878 (+5) | 3335878 | 3335878 |
| cost.findings | information only | 1089 | 1089 | 1089 | 1089 |

## Latest scorecard per dataset

| dataset | kind | planted found in 4k digest | must statements in 4k digest | must statements in 4k digest, target mode | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| adult-census-income | tabular, mixed types | 2 | 2 | 3 | 2 | 30 | 0 | 0 | 1.736 |
| bank-marketing | tabular, mixed types | 2 | 1 | 1 | 8 | 44 | 0 | 0 | 1.726 |
| abalone | tabular, correlated measures | 3 | 2 | 2 | 1 | 13 | 0 | 0 | 0.184 |
| bike-sharing-hourly | hourly series with calendar columns | 2 | 2 | 1 | 10 | 32 | 0 | 0 | 1.537 |
| metro-interstate-traffic | hourly time series | 3 | - | - | 9 | 7 | 0 | 0 | 1.48 |
| thyroid | clinical, many flags and missing values | 1 | - | - | 32 | 40 | 1 | 0 | 0.818 |
| absenteeism-at-work | small business table | 1 | - | - | 4 | 68 | 0 | 0 | 0.237 |
| hospital-dha | wide numeric, 64 columns | 1 | - | - | 9 | 40 | 0 | 0 | 0.717 |
| congressional-voting | categorical votes | 1 | - | - | 1 | 38 | 1 | 0 | 0.049 |
| adventureworks | sales orders | 1 | - | - | 40 | 30 | 0 | 0 | 2.442 |
| nab-nyc-taxi | single time series | 3 | - | - | 0 | 0 | 2 | 0 | 0.046 |
| state-unemployment | wide time series, one column per state | 0 | - | - | 16 | 66 | 0 | 0 | 1.283 |
| state-unemployment-long | panel, long form | 0 | - | - | 4 | 0 | 0 | 0 | 0.078 |
| south-german-credit (held out) | credit table | 2 | - | - | 7 | 13 | 1 | 0 | 0.176 |
| soybean (held out) | categorical agronomy table | 0 | - | - | 7 | 54 | 0 | 0 | 0.201 |
| lace-satellites (held out) | satellite catalogue | 1 | - | - | 9 | 71 | 0 | 0 | 0.239 |

## Held-out datasets

These datasets are scored and left out of the pass/fail comparison. A gain on the gated datasets that these do not share points to tuning on the gated datasets.

| date | commit | change | planted found in 4k digest | statements on shuffled tables (4k digest) | facts changed by layout | repeated facts in 4k digest | statement mismatches | profile seconds |
|---|---|---|---:|---:|---:|---:|---:|---:|
| 2026-11-16 | `2601a72` | Merge pull request #326 from fcela/feat/gate-report-tldr | 3 | 23 | 138 | 1 | 0 | 0.616 (+0.002) |
| 2026-11-15 | `daf7a79` | Merge pull request #324 from fcela/feat/315-corpus-targets | 3 | 23 | 138 | 1 | 0 | 0.614 |
| 2026-11-15 | `ca275fd` | Merge pull request #323 from fcela/feat/317-gate-history-report | 3 | 23 | 138 | 1 | 0 | 0.626 |
| 2026-11-15 | `1618886` | Merge pull request #320 from fcela/fix/295-agent-token-economy | 3 | 23 | 138 | 1 | 0 | 0.327 |

## Platforms

- Linux-7.0.0-1014-aws-x86_64-with-glibc2.39
- Linux-7.0.0-1017-azure-x86_64-with-glibc2.39
- macOS-26.3-arm64-arm-64bit-Mach-O
