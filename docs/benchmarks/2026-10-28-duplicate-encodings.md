# Duplicate-encoding advisory (issue #41)

Date: 2026-10-28. Library change: new default-on pass
`duplicate_encodings` (detector id `duplicate_encodings`, category
`column_roles`, finding type `duplicate_encoding`, forms
`duplicate_encoding` and `rollup`); the detector count goes from
19 to 20. The pass derives from the hygiene tier of mboostrs-dev's
autospec frame findings (a column that re-encodes a grouping
another column already records), rebuilt on onceover's exact
machinery: candidates are discovered on per-column hashes and
verified on canonical encodings before anything is emitted.

## Why

The FD pass reports the mapping underneath a code/label pair
(`region_id -> region_name`) but never states the actionable
reading: one of the two columns repeats a grouping the other
already carries, and a modelling or joining workflow should drop
or reconcile one. The advisory states it, and separates the
many-to-one case (a rollup: keep the level of detail needed) from
the one-to-one case (a duplicate encoding: keep either).

## Method

Eligible columns are label-like — text categories and whole-number
codes (a float column qualifies only if every value in the window
is whole) — excluding identifiers, with 3 to n/2 levels: only
label-like columns can be two encodings of one grouping, a key
determines its label trivially, and a measurement that takes one
value per group carries an order and a size a code does not.

For each pair, on rows where both columns are present (at least
20; at most 100,000 rows at standard, 250,000 at deep, stride
sampled): the pair is a candidate when its distinct value pairs
number at most 1.02 times the larger column's level count. When
the smaller level count is at least 0.9 of the larger, the form is
`duplicate_encoding` and the evidence field `exact` says whether
the pairing is one-to-one on every row; below that share the form
is `rollup`, the finer column is named first, and `exceptions`
counts the rows breaking the finer-to-coarser mapping (for a
duplicate encoding, the rows breaking the majority pairing in
either direction). The constants (slack 1.02, share 0.9, the
level bounds) are the originating check's. Scores — 0.62 exact,
0.56 partial, 0.5 rollup — sit below an exact functional
dependency's, so when both describe one pair the dependency
finding ranks first.

## Measured

Planted fixtures (Rust and Python tests): an exact code/label
bijection fires with `exact = true`; a 9-city to 3-state mapping
reports as a rollup with the finer column first; a key-determined
label does not fire; a concentrated mislabeling (5 rows of one
group carrying another group's label, 50 groups) fires as a
partial duplicate with `exceptions = 5`; independent categorical
columns do not fire; the quick budget skips the pass.

Corpus (every registered expectation still passes — 0 failures in
the run record): three findings. On sec-edgar-company-facts,
`cik` and `company` — 12 levels each, paired one-to-one with 0
exceptions, the textbook case. On soybean, two rollups:
`class` determines `int_discolor` (16 class levels present, 3
discoloration levels, 645 rows where both are present) and
`leaf_mild` (15 class levels, 3 levels, 575 rows) exactly — each
disease class carries one value of each symptom in this sample,
so the finer column maps onto the coarser with no exceptions; the
FD pass reports the same mappings as dependencies, and the
advisory adds the rollup reading. Every other bundled dataset (thyroid,
abalone, splice, bank-marketing, congressional-voting,
hospital-dha, state-unemployment, gse15258-expression, the two
NAB streams) gains none: their categorical pairs are not
near-functional in either direction.

## Not covered

Pairs in which one side is a genuine key (excluded by design —
the key findings cover them), two-level columns (a flag is not an
encoding of a grouping), and groupings encoded jointly by two or
more columns.
