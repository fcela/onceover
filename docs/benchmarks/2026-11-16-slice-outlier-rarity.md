# Categorical outliers in slices require the level rare overall: gate measurements (2026-11-16)

Issue #317, phase 3, third statement family. The `outliers` pass
states that a level breaks a pattern inside a context — a
conjunction of up to three conditions — when one level of a
categorical column holds at least 99% of a context's rows (at
least 30 rows) and at most three rows break the pattern. The
statement spotlights the breaking rows' levels as the anomaly.
It did not ask whether those levels are unusual in the column as
a whole, so a default report stated, for example, that one August
row breaks a July pattern inside a thin slice of the bank
marketing table, when August is the column's third most common
month. The rarity was an artifact of slicing thin. This record
holds the before/after measurements. Base is main at 064132d8
(0.1.5); the head build is this change. All runs use the
repository's own instruments: the gate in `benchmarks/gate/`
(seed 0, full runs), the corpus in `benchmarks/corpus/`, and a
count of `categorical_outlier` findings in the standard report
per gate dataset.

## What changed

- `crates/onceover-mines/src/surprises.rs`: a default run adds
  one requirement to the categorical side of the outliers pass.
  A statement is stated only when every level its breaking rows
  carry is rare in the target column overall — at or below 1% of
  the column's non-null values. The 1% bar is the codebase's
  existing rarity convention for a categorical level:
  `MAX_RARE_SHARE` in `crates/onceover-deps/src/content.rs`,
  where a label at or below that share is the rare one; the pass
  mirrors the constant and cites it. The requirement applies to
  contexts of any size (one to three conditions): the statement
  form and the harm are the same for a single-condition context.
  The scan itself is unchanged — the same contexts, the same
  in-slice bars (99% dominant share, 30 rows, at most three
  breaking rows), the same significance ranking.
- Withholding is counted against a shadow of the same selection
  run without the requirement. The coverage count is therefore
  the number of statements that would otherwise have been
  stated — bounded by the pass budget and the per-target cap —
  not the number of candidates the scan produced. A withheld
  statement leaves its selection slot free, so a later statement
  whose breaking levels are all rare is still stated. The
  outliers coverage entry carries the count with the reason:
  "N categorical outlier statements not stated: a deviating
  level is common in its column overall …".
- `crates/onceover/src/pipeline.rs`: the pipeline turns the
  requirement on for default runs. Naming the `outliers`
  detector explicitly (`detectors=["outliers"]`) keeps the
  previous behavior, per the plan's "or go opt-in" reading. The
  change is confined to the pass-configuration block; it does
  not touch the dispatch regions.
- The numeric side of the pass (`extreme_value` findings) is not
  in this family and is unchanged. No registry entry changes;
  the capability manifest regenerates identically.

## Gate totals (gated set, seed 0)

| Metric | Before | After |
|---|---:|---:|
| reference must at 4k, default | 10/12 | 10/12 |
| reference must at 4k, target | 11/12 | 11/12 |
| reference should at 4k, default | 6/16 | 6/16 |
| reference should at 4k, target | 10/16 | 10/16 |
| reference should at full, default | 13/16 | 12/16 |
| reference should at full, target | 14/16 | 14/16 |
| reference check_failed | 0 | 0 |
| planted found, full report | 33/46 | 33/46 |
| planted found, 4k digest | 29/46 | 29/46 |
| false statements on shuffled tables, full, default | 144 | 129 |
| false statements on shuffled tables, 4k, default | 78 | 75 |
| false statements on shuffled tables, full, target | 79 | 64 |
| false statements on shuffled tables, 4k, target | 44 | 41 |
| invariance violations, total | 430 | 445 |
| repeated facts, default / target | 0 / 0 | 0 / 0 |
| truth verify mismatches / errors | 0 / 0 | 0 / 0 |
| findings, all gated reports | 1089 | 1109 |
| report JSON characters | 3,366,352 | 3,417,295 |

Held-out reference totals are identical before and after in
every cell (must 3/7 at 4k and full in both modes, should 2/4).

Two movements need their mechanisms stated:

- One should-tier reference statement loses a match. Bank
  marketing's `subscription-housing-loan` statement matched the
  full default report by text before; the only default-mode
  line carrying that column pair was a slice-outlier statement
  whose breaking level is common overall, now withheld. It
  still matches at 8k and in the full report in target mode,
  and no 4k number moves.
- Invariance rises by 15 net (reverse 279 → 284, row shuffle
  105 → 120, noise 38 → 36, scale 8 → 5), and the rise is one
  dataset: congressional-voting moves 38 → 63 while adult
  (40 → 34), bank (51 → 47), and soybean (54 → 52) improve and
  lace-satellites moves 79 → 81. On congressional-voting the
  ten slice-outlier statements are all withheld, and report
  selection redistributes the space they held to other passes
  (findings 66 → 92: if_then 12 → 20, statistical 12 → 20,
  predictive 12 → 19, content_anomaly 12 → 14, hotspot 2 → 13).
  The redistributed findings are less stable under column
  reversal and row shuffle than the statements they replaced.
- Truth "ok" tallies fall (836 → 705) with mismatches and
  errors at zero in both runs: the withheld statements carried
  checkable evidence, so there are fewer statements to verify,
  not fewer correct ones. Findings rise overall (1089 → 1109)
  by the same redistribution as above.

## Counts per gate dataset

`categorical_outlier` findings in the standard report, default
and target mode (the two modes agree on every dataset), and the
withheld count the coverage entry records at pass level:

| Dataset | Before | After | Withheld |
|---|---:|---:|---:|
| adult-census-income | 12 | 5 | 8 |
| bank-marketing | 12 | 0 | 13 |
| thyroid | 12 | 7 | 18 |
| absenteeism-at-work | 1 | 0 | 3 |
| congressional-voting | 10 | 0 | 10 |
| south-german-credit (held out) | 3 | 0 | 3 |
| soybean (held out) | 12 | 3 | 20 |
| lace-satellites (held out) | 1 | 0 | 1 |
| metro-interstate-traffic | 1 | 1 | 0 |

Every other gate dataset has none before or after. The withheld
count is at pass level (the pass states up to its own budget);
the report shows at most 12 of a type, so the two counts are
not expected to sum to the before column.

Verified example, checked by hand in Polars on the corpus file.
Before, bank-marketing's report stated: "within
personal_loan = yes and day_of_week = 31 and
subscribed_term_deposit = no (123 rows), month is jul in 99.2%
of rows — 1 row breaks the pattern: row 41356 (month = aug)".
The context holds exactly 123 rows (122 jul, 1 aug), and the
breaking level is ordinary: aug is 6,247 of 45,211 rows
(13.8%) of the month column, jul itself 6,895 (15.3%). The
statement is withheld now. Statements whose breaking levels are
rare survive: on adult-census-income the three statements
breaking on Married-AF-spouse remain (37 of 48,842 rows,
0.08%), as do the two breaking on race levels Other (0.83%)
and Amer-Indian-Eskimo (0.96%); the statements breaking on
income >50K (23.9%), sex Female (33.2%), and Federal-gov
(2.9%) are withheld.

## Corpus

173/173 machine-verified expectations pass before and after;
no expectation is re-baselined. The two soybean
`categorical_outlier` expectations remain known-misses, and
their mechanism changes: GritBot case 558 (seed = abnorm
breaking a seed = norm context) and case 614 (stem = norm
breaking a stem = abnorm context) were previously generated
and displaced at the default cap; in a default run they are
now not generated at all, because the breaking levels are
common overall (seed = abnorm is 115 of 683 rows, 16.8%;
stem = norm is 296 of 683, 43.3%). Both remain stated when
the detector is named explicitly. The three soybean statements
that remain in the default report all break on
mycelium = present, 6 of 683 rows (0.88%).

## Tests

- Rust, `crates/onceover-mines/src/surprises.rs`: a statement
  whose breaking level is common overall is withheld and
  counted; the count reaches the coverage detail; a statement
  whose breaking level is rare overall (0.2%) is stated; a
  five-context fixture pins the count to the statements the
  ungated selection would have stated (2 of 4 withheld
  candidates) and shows the rare-breaker statement still
  stated behind them.
- Python, `tests/test_surprises.py`: the existing
  single-breaker test now selects the detector explicitly (its
  breaking level is common overall); a default run of the same
  fixture states nothing and records the withheld count in
  coverage; a rare-level fixture states by default.
