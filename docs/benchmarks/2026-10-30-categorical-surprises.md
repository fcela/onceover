# Categorical surprises — a value contradicting its context's near-universal pattern

Date: 2026-10-30. Change (issue #68): the surprises pass gains a
categorical analogue of its numeric test. Where the numeric pass
isolates a value extreme within a subset defined by one
categorical column, the categorical pass isolates a *level* that
contradicts a near-universal pattern inside a context: within a
conjunction of one to three condition levels, one level of the
target column holds at least 99% of at least 30 rows, and at
most 3 rows break the pattern. The finding (category
`intervals`, type `categorical_outlier`) names the context, the
dominant level and share, and the breaking rows with their
values.

## Method

- Targets are text/bool columns with 2–32 levels; conditions are
  any eligible column (text, bool, or low-cardinality integer),
  coded once and held as row bitsets.
- Contexts are searched levelwise. Level 1 evaluates every
  condition level. Levels 2–3 are built only from the 12
  condition columns whose single levels come closest to the bar
  for that target; pairs are kept for triple extension when
  their share is at least 0.95 (beam 128 by share, then size).
  The restriction matters: purity does not inherit downward — a
  context at 86% with 68 breakers can sharpen to 99.6% with 2
  under one more condition — so extension is ranked by each
  column's best single-level share rather than pruned by parent
  purity alone.
- Candidates are ranked by binomial significance: the log10
  tail of the dominant count under the dominant level's global
  rate. At most 3 findings per target; a breaking row is
  reported once per target; the pass cap (20) is shared with
  the numeric pass, which runs first.
- Score: 0.60 + 0.30 × min(1, −log10 tail / 60).

## The GritBot soybean cases

The published case list (RuleQuest, "Sample Results with
GritBot", fetched 2026-10-30) gives the three soybean anomalies
verbatim:

- case 78: `leafspot-size = gt-1/8` (221 cases, 99.5% `dna`),
  condition `leafspots-halo = absent`.
- case 614: `stem = norm` (191 cases, 99.5% `abnorm`),
  condition `stem-cankers = above-sec-nde`.
- case 558: `seed = abnorm` (405 cases, 99.8% `norm`),
  conditions `fruiting-bodies = absent`, `mold-growth =
  absent`, `seed-discolor = absent`.

The registry's earlier notes for 558 and 614 paraphrased these
incorrectly (a leaves/leaf-mild pattern that does not occur in
the bundled file; a canker level the file does not contain).
The notes have been corrected against the published list, and
both documented cases are promoted from known-miss to found,
with `finding` expectations added.

On the bundled file the pass evaluates 107,506 contexts in
0.09s and reports 20 categorical findings, 19 after report
selection. All three GritBot rows are isolated:

- Row 613 (case 614) under GritBot's exact context:
  `stem_cankers = above-sec-nde` (191 rows, 99.5% `abnorm`).
- Row 557 (case 558) under the pair context `mold_growth =
  absent` and `seed_discolor = absent` (474 rows, 99.6%
  `norm`; rows 250 and 557 break it). The pair is preferred
  over GritBot's triple by the brevity tie-break (fewer
  conditions at comparable significance); the triple context
  (405 rows, 99.8%, row 557 alone) is also a candidate.
- Row 77 (case 78) under `leafspot_size = gt-1/8`-family
  contexts (its halo/size combination was already isolated by
  the functional-dependency finding; the categorical finding
  names the row directly).

Soybean registry expectations: 6/6 (was 4/4 with two
documented misses).

## Null behaviour

- Congressional voting (435 rows, 17 categorical columns): 10
  findings, all party-line breaks of the recorded shape — e.g.
  `physician_fee_freeze` = y in 99.4% of the republican +
  el_salvador_aid = y context with one breaker. That column is
  the file's classic near-perfect vote. The dataset's registry
  expectations (3/3) are unchanged and still pass.
- Battery null (80/20 splits in every context, 400 rows): no
  findings. Battery planted case (one breaker in a 99.5%
  context): isolated.
- Corpus: every runnable dataset ok, all registry expectations
  met; no other dataset's findings changed category counts
  except soybean (categorical outliers added) — the numeric
  surprises results are untouched, since the categorical scan
  runs after them inside the same pass cap.

## Verification

- Rust unit tests: single-condition breaker isolated; pair-only
  context isolated (each single condition uninformative);
  ordinary variation quiet; binomial tail ordering. Python
  tests mirror the planted and null cases.
- Detection battery (68 cases): 53 pass, 6 fail, 0 gap,
  9 observed; both new categorical cases pass. The 6 fails are
  the 2 documented drift boundary cases and 4 cases for
  detectors on other branches (sentinels, hotspots, digits).
- Suites: Python 127 passed / 1 skipped (125 + 2 new); Rust
  workspace 169 passed / 0 failed.

## Scope limits, stated

- Targets are text/bool columns only; low-cardinality integer
  columns serve as conditions but are not categorical targets
  in this version (the numeric pass covers integer targets).
- Contexts are conjunctions of levels of distinct columns, at
  most 3 conditions, over the top-12 ranked condition columns
  for depth ≥ 2. A pattern visible only under a 4-way context,
  or only through a low-ranked condition column at depth ≥ 2,
  is out of reach.
- The global (unconditioned) case is deliberately not flagged:
  a column that is 99% one level overall is concentration's
  story, not a per-row surprise.
