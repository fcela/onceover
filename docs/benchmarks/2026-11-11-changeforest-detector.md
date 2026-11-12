# Changeforest detector: planted-change acceptance, power probes, and cost (2026-11-11)

## Question

The breaks detector and the ordered-partitions detector state
changes in level, trend, and variance of one series at a time.
Neither can state a change in the joint distribution of the
measures along the time axis that leaves every single-series
summary intact — a covariance change at constant means and
variances is the standard example. This record covers the
acceptance measurements behind adopting the `changeforest` crate
(Londschien, Bühlmann & Kovács, "Random Forests for Change Point
Detection", JMLR 24 (2023); BSD-3-Clause) as an opt-in detector
that states such changes as `distribution_change` findings:
binary segmentation whose gain at a candidate split is the
log-likelihood ratio of a random forest trained to distinguish
the rows before the split from the rows after it, with a split
kept when the permutation p-value of its segment's maximum gain
is at most 0.01 (199 permutations).

Protocol: the acceptance fixtures below are planted-change
frames generated from fixed seeds; the shipped regression tests
(`crates/onceover-mines/src/changeforest.rs`,
`tests/test_changeforest.py`) assert the stated splits. Power
probes re-draw the same constructions under other seeds and count
outcomes; they are measurements, not tests. Wall times are
end-to-end `profile()` calls on this machine (release build)
with `detectors=["changeforest"]`, so they include extraction
and the frame model; the pass's own coverage detail is quoted
for the corpus frames.

## Dependency

`changeforest` 1.2.1, added to `onceover-mines` with `ndarray`
0.16. Its own dependencies resolve to `biosphere` (the author's
random-forest crate, BSD-3-Clause) and `rand` 0.8. One
resolution hazard: `biosphere` 0.4.2 moved to `ndarray` 0.17 and
no longer compiles with `changeforest` 1.2.1, which is written
against `ndarray` 0.16; the manifest therefore bounds
`biosphere` to `>=0.4.0, <0.4.2` (0.4.1 is the version resolved),
with the reason recorded next to the bound. No existing
workspace dependency changed version.

Determinism: the segmentation seed is fixed at 260 in the pass's
code. The crate draws its permutation streams from that seed,
and the forest's per-tree seeds are drawn from a fixed seed
inside the crate before any parallel work, so scheduling cannot
change the result. Two profiles of the covariance fixture
produce byte-identical findings; a regression test pins this at
both the Rust and Python levels.

## Acceptance fixtures

Covariance shift (the paper's change-in-covariance setup): 600
rows, 5 coordinates, correlation 0 on rows [0, 200), 0.7 on
[200, 400), 0 on [400, 600), every coordinate at mean 0 and
variance 1 throughout. The detector states splits at rows 196
and 398 on the shipped Rust fixture (planted 200 and 400) and at
both planted splits on the shipped Python fixture. A third split
at row 474 is also stated on the Rust fixture: the second split
is located two rows early, the two correlated rows left in the
remainder segment carry a faint signal, and the remainder states
one more split at the 0.01 bar. The same remainder segment in
isolation states nothing, and a re-seeded fixture states exactly
the two planted splits (201, 400). This is the method's
per-segment error bar operating as specified, and the tests
therefore assert the planted splits, not an exact count.

Shape change: one series, 1,200 rows; the second half is the
first half's noise folded to its absolute value and re-centred
and re-scaled, so level and variance are held and only the shape
changes. The shipped fixtures state the split at row 599 (Rust
draw) and 600 (Python draw), planted 600.

Controls: on a pure mean shift the breaks detector's
`level_shift` still fires (regression control in the Python
tests). On no-change frames (600 rows, 3 and 5 measures) the
detector states nothing; eight further independent draws of 600
× 5 independent normal measures also stated nothing.

## Power probes (same constructions, other seeds)

- Covariance setup as above, re-seeded once: exactly the two
  planted splits (201, 400).
- Folded-normal shape change at 300 rows a side (9 draws):
  a split within 20 rows of the planted one on 7 draws, no
  statement on 1, a mislocated split (row 565) on 1.
- Folded-normal shape change at 600 rows a side (9 draws with
  one generator, 8 with another): a split within 20 rows of the
  planted one on 9 of 9 and 6 of 8 draws respectively; of the
  remaining two draws, one adds a second split near the planted
  one and one mislocates.
- A symmetric bimodal mixture against a normal at 300 rows a
  side (11 draws), same mean and variance by construction:
  a split within tolerance on 5 draws, no statement on 5, one
  split 50 rows early. The folded contrast, which gives the
  second half a hard lower edge, is the easier shape change for
  this method at these lengths.

The reading: at the 0.01 bar the method is reliable on the
covariance setup at the paper's scale and on shape changes with
600 rows a side; at 300 rows a side a single-series shape change
is at the edge of its power. The acceptance fixtures use the
lengths where the method is reliable.

## Cost

End-to-end profile wall times, release build, this machine:

| Frame | Rows × measures | Time | Outcome |
|---|---|---|---|
| Covariance fixture | 600 × 5 | 0.27 s | 2 splits stated |
| Shape fixture | 1,200 × 1 | 0.12 s | 1 split stated |
| state-unemployment (corpus panel) | 609 × 51 (32 segmented) | 0.80 s | 15 significant splits, 10 stated |
| metro-interstate-traffic, standard | 48,204 × 4 (10,000 sampled) | 9.72 s | 164 significant splits, 10 stated |
| metro-interstate-traffic, deep | 48,204 × 4 (20,000 sampled) | 14.20 s | node budget reached; 257 splits found, 10 stated |
| bike-sharing-hourly, standard | 17,379 × 7 (10,000 sampled) | 14.06 s | 235 significant splits, 10 stated |
| bike-sharing-hourly, deep | 17,379 × 7 | 19.28 s | node budget reached; 259 splits found, 10 stated |

The two hourly frames are the cost drivers: their joint
distribution changes at a fine grain (weather, rush hours,
holidays), so an unbounded segmentation subdivides until the
minimum segment length stops it — measured before the bounds
were set, the pass stated 418 significant splits on a
20,000-row sample of the metro frame (1,124 on the full frame)
and took 22–59 s. The shipped bounds, each stated in coverage
when it binds: at most 10,000 complete rows segmented at the
standard budget (20,000 at deep; a stride sample in axis order
past the cap), at most 512 segments evaluated, at most 32
measures jointly, no segment shorter than 30 rows split, and at
most 10 splits stated — the largest by classifier gain, with
the total number of significant splits in the coverage entry
and in each finding's evidence (`significant_splits`).

## Verifier

`report.verify` does not recount `distribution_change`
statements: the split, its gain, and its p-value are model
outputs with no exact recount, so the verifier marks the
statement `not_checkable`, as it does the breaks statements.
The finding's evidence states this in its `validation` entry.
