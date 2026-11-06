# Budget-order selection — 2026-11-05

The change as merged (`296c0508`) measured against its parent
tree (`9dd66df9`); current-main checks against `f5a43b5b`.
Issue #126, partitions and hotspots legs: when a pass's
candidate cap binds, what gets examined was decided by column
position — the ordered-partitions pass screened the first
`max_responses` (128 at standard) eligible responses in column
order, and the hotspots pass examined the first `max_triples`
(400 at deep) candidate triples in feature-index order. A
strong response or triple sitting past the cap was never
examined, and coverage did not say the choice had been
positional. Both caps now spend on a cheap whole-population
score, and coverage discloses the truncation and the selection
rule whenever a cap binds.

## What changed

**Ordered partitions** (`crates/onceover-mines/src/partitions.rs`).
Every eligible response is prescored before the cap binds. The
prescore is the larger of two between-group variance shares
(eta-squared) of the response, computed on the same sampled,
t-ordered rows the screen itself uses: the share between 16
contiguous time blocks, and the share between each offered
dimension's levels. These are the two split families the
model-based tree searches — a breakpoint along the ordering
axis, a bipartition of a dimension's levels — so a response
carrying either structure scores far above the pure-noise floor
of ~(groups − 1) / (n − 1) (≈ 0.025 for 16 blocks at n = 600),
while a response with neither scores at the floor regardless of
its position. The screened set is the top `max_responses` by
score, ties by column index, so selection is a deterministic
function of the table. Rows are rebuilt for the screened subset;
no per-response data is retained across the cap. When the cap
binds, coverage appends: "response cap 128 bound: S of E eligible
responses screened, chosen by prescore (between-block and
between-level variance shares), not column order".

**Hotspots** (`crates/onceover-mines/src/hotspots.rs`). Pairs are
evaluated first, as before, but the Bonferroni z bar is applied
after triple selection, when the examined-cell total is final:
each pair now returns its cells clearing the absolute bars plus
a pre-score — its strongest cell z over the cells clearing the
count floors (observed ≥ min_observed, expected ≥ min_expected).
The ratio and share bars are deliberately not part of the
pre-score: a strong triple's pairs dilute toward their margins,
so a pair can sit below the ratio bar (the plant below: pair
ratio 1.54 against a bar of 2.0) while its z (14.1) still
marks the triple as worth examining. Triple selection streams
the whole cell-capped triple universe over the ≤ 8-bin features
— bounded by the encoder's feature cap, C(64, 3) = 41,664 at
deep — scoring each triple by its strongest constituent pair,
and keeps the top `max_triples` in a bounded heap keyed by
(score, enumeration ordinal). The selected triples are evaluated
in enumeration order. Streaming enumeration and its memory bound
are unchanged: the universe is never materialized. When the cap
binds, coverage appends: "triple cap 400 bound: S of U candidate
triples examined, chosen by pairwise pre-score (strongest
constituent pair cell), not column order".

Identity when a cap does not bind: the examined sets are the
same as before (all eligible responses; the whole triple
universe), the Bonferroni denominator is the same, and the
z bar filters the same candidate hits, so findings and coverage
are unchanged. Ties in either ranking resolve by position, so
the previous column-order behavior survives as the tie-break.

## Plants

Two planted frames pin the behavior (regression tests:
`tests/test_budget_order.py`).

**Partitions.** 600 daily rows, 140 seeded noise responses plus
one planted level-break response (first half N(0, 0.5), second
half N(5, 0.5)); 141 eligible responses against the standard
cap of 128. Planted prescore ≈ 0.96 against a noise floor of
≈ 0.025.

- Base, planted last: not screened, no finding. Coverage:
  "128 of 141 responses screened (response cap 128)".
- Base, planted first: stated — "planted_response follows 2
  level+trend regimes along date: first split break at
  2021-10-28, 1 split in all, level+trend SSE reduced 86.7%".
- Patched, planted last: stated, with the identical claim.
- Patched, planted first (the same data, the response moved to
  the front): stated, identical claim. Coverage in both:
  "response cap 128 bound: 128 of 141 eligible responses
  screened, chosen by prescore (between-block and
  between-level variance shares), not column order".

**Hotspots.** 5,000 rows, 20 nominal columns of 4 levels; 800
rows piled onto one triple cell. 1,140 candidate triples
against the deep cap of 400. The twin frames are pure column
permutations of one another (the planted triple is always
c17, c18, c19; only its position changes).

- Base, triple in the last three positions: not examined, no
  finding (590 combinations examined, 400 triples — the first
  400 in index order).
- Base, triple permuted to the first three positions: stated —
  "cell (c17 = 'a', c18 = 'a', c19 = 'a') holds 866 of 5000
  scanned rows (17.3%) against 252.5 expected under
  independence of c17, c18, c19 — 3.4x the baseline
  (z = 38.6, bar 5.1 over 28640 cells examined)".
- Patched, both positions: stated, with the identical claim in
  both — the same 866 rows, 252.5 expected, z = 38.6, and the
  same 28,640 examined cells. Coverage in both appends:
  "triple cap 400 bound: 400 of 1140 candidate triples
  examined, chosen by pairwise pre-score (strongest
  constituent pair cell), not column order".

The same four outcomes hold on current main (`f5a43b5b`):
both plants are stated in both positions, with the claims and
coverage disclosures above unchanged.

## Corpus

Full runnable corpus (23 datasets at registry budgets; 11 of
the 34 registry entries are skipped because their data files
are not bundled), report JSON compared field by field with
`elapsed_ms` dropped: all 23 reports are byte-identical
between the base and patched builds.

No registry-budget report binds either cap: the largest
partitions response count is 51 (state-unemployment, cap 128),
and every registry-budget hotspots run reports 0 triples
examined, so the corpus leg is an identity proof for the
restructure rather than a selection-change measurement.

Bike-sharing (hourly), the dataset whose patched-side dump was
missing from the original run: the report is byte-identical to
the base build's, at the 100-finding report cap on both. Its
ordered-partitions coverage reads "15 numeric responses
partitioned along dteday (9 with accepted splits, 9 findings);
0 dimension(s) offered; minimum segment 24 rows, gain bar 2%,
F floor 15" — 15 responses against the cap of 128 — and its
hotspots coverage reads "8 eligible columns, 28 combinations
examined (0 triples), 1144 cells examined, z bar 4.45
(Bonferroni over cells, family alpha 0.01); counts over the
encoded sample of 17379 rows". Neither cap binds.

On current main the report as a whole has moved since the
change (the report schema is 0.7 and other passes have their
own later changes), but the two passes this change owns are
unchanged: the ordered-partition and hotspot findings and
their coverage entries are identical between the patched build
and current main on all 23 datasets.

Deep-forced slice (six datasets with the nominal-column counts
to exercise triples, budget forced to deep): the triple cap
binds on splice, soybean, thyroid, and south-german-credit;
absenteeism-at-work and adult-census-income examine their whole
universes (56 triples each) and their full reports are
byte-identical between the base and patched builds. Where the
cap binds, the examined set changes and coverage gains the
disclosure; the differences, per dataset:

- **Splice** — cap 400 of 35,990 candidate triples. Examined
  cells 66,033 → 70,975; z bar 5.25 → 5.26. Hotspot findings
  20 → 18, with no cell in common between the two sets: the
  first-400 index-order set is replaced wholesale by the
  top-400 by pre-score. The 22 non-hotspot findings are
  unchanged, claim for claim.
- **Soybean** — cap 400 of 6,545. Examined cells 27,441 →
  16,884; z bar 5.09 → 4.99. No hotspot findings on either
  build; the remaining findings (100, at the report cap) are
  unchanged, claim for claim. Only the coverage figures move.
- **Thyroid** — cap 400 of 1,771. Examined cells 5,090 →
  6,626; z bar 4.76 → 4.81. Hotspot findings 8 → 8, again with
  no cell in common: the patched set is the family of triples
  combining a false condition flag with t4u_measured = 'false'
  and fti_measured = 'false', holding 352–385 of 3,772 rows
  against 34.6–39.5 expected. The 92 non-hotspot findings
  are unchanged, claim for claim.
- **South-german-credit** — cap 400 of 680. Examined cells
  24,210 → 21,823; z bar 5.06 → 5.04. Hotspot findings 20 →
  20: 13 cells keep the same counts with only the z and bar
  figures moved (the re-denominated cells), 7 leave and 7
  enter. The 29 non-hotspot findings are unchanged, claim for
  claim.

## Overhead

The prescore costs one extra read of every eligible response's
sampled rows (partitions) and the pair pre-score bookkeeping
plus streaming the triple universe through the bounded heap
(hotspots). Timed on the plant frames and on a wide numeric
frame (5,000 rows, 300 noise responses, ordered partitions at
standard — the cap binds at 128), release builds, five
repetitions interleaved across builds, median [range]:

| frame | base | patched | current main |
|---|---|---|---|
| partitions plant | 0.01s [0.01–0.05] | 0.02s [0.01–0.04] | 0.01s [0.01–0.02] |
| hotspots plant | 0.03s [0.03–0.03] | 0.03s [0.03–0.04] | 0.04s [0.03–0.04] |
| wide, 300 responses | 0.24s [0.24–0.26] | 0.26s [0.25–0.30] | 0.27s [0.26–0.29] |

On the plant frames the difference between builds is at the
resolution of the runs (0.01s). On the wide frame the median
rises by 0.02s (patched) to 0.03s (current main) over 300
prescored responses, with the ranges of the three builds
overlapping; at both sizes the median shift is smaller than
the spread between repetitions of the same build on this
machine.

## Provenance

Plants, corpus dumps, comparisons, and timings were produced
on 2026-11-06 with release-profile builds (`maturin develop
--release`) of three trees: the change's parent `9dd66df9`,
the change as merged `296c0508`, and current main `f5a43b5b`.
Each tree was reconstructed from the GitHub API with every
blob's git hash verified. Python 3.12.3, Polars 2.1.0,
NumPy 2.5.4. Corpus loading mirrors
`benchmarks/corpus/run_benchmarks.py` exactly; the corpus
harness on current main reports 23 ok, 11 skipped, and 89/89
machine-verified expectations. Timing repetitions were
interleaved across builds so machine contention hits all
builds alike; load average at the start of the timing run was
0.46 (1-minute), on a 4-core, 7 GB VM that also runs other
work.
