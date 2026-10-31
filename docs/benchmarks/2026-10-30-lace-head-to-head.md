# Views head-to-head: onceover's greedy views vs Lace (CrossCat)

Date: 2026-10-30. Issue #22's comparison note. The two Lace
example tables are bundled in the corpus (crosscat pack) exactly
as the Lace project distributes them; the files are byte-identical
to the copies pylace's `lace.examples` loads (sha256 verified), so
both systems profile the same bytes.

## Method

- **Lace side**: pylace 0.10.1 example engines (`Satellites()`,
  `Animals()`), 16 posterior states each, as constructed by the
  package's own example loader. The comparison uses Lace's
  `depprob`: for each pair of columns, the posterior probability
  that a dependence path exists between them. Lace's engines drop
  the identifier column, so pairs are computed over 20 satellite
  columns and 85 animal features.
- **onceover side**: the views partition from views-focused
  profiles (standard budget, views detector enabled). In the
  corpus profiles the views findings on Satellites are crowded
  out of the 100-finding report cap by the functional-dependency
  flood; the coverage line still records the partition's shape
  (18 views, 2 multi-column, 38 findings).
- A pair counts as **Lace-dependent** at depprob >= 0.5 and as
  **recovered** when onceover places both columns in one view.

## Satellites (1,164 x 21)

onceover's partition: 18 views, 2 multi-column —
{Class_of_Orbit, Perigee_km, Period_minutes} and
{Inclination_radians, longitude_radians_of_geo}; the other 16
columns are singleton views.

Lace's depprob on this table saturates: minimum 0.500, median
0.938, and 73 of 190 pairs at >= 0.99 — the posterior places
nearly every pair in a shared view, so a pairwise comparison
cannot discriminate. The measured difference is structural: Lace
returns one dense dependence structure; the greedy pass returns
two small views and singletons. The four pairs onceover does join
all sit at depprob 1.000. Within the orbital block, Lace rates
every pair 1.000 except those involving Eccentricity (0.812);
onceover's functional-dependency pass independently records the
block's sharpest fact, Apogee_km -> Class_of_Orbit with 0
violations in 1,163 rows.

## Animals (50 x 85 features)

The 64-column views cap binds first: 22 feature columns are
dropped before the pass runs (named in the corpus entry's
coverage expectation). Of Lace's 605 dependent pairs, 254 involve
a dropped column and cannot be recovered by the pass as
configured.

On the 351 dependent pairs where both columns were viewed,
onceover recovers 7 (2.0%). 65 viewed pairs sit at depprob >= 0.9
in Lace and are split by the greedy partition — among them
big ~ small, paws ~ claws, pads ~ paws, pads ~ claws, and a
flippers cluster (flippers ~ swims, flippers ~ plankton,
flippers ~ strainteeth, all 1.000; the pass groups flippers with
quadrapedal/walks and plankton with skimmer/strainteeth instead).
In the other direction, 3 of onceover's 10 same-view pairs fall
below 0.5 in Lace: orange ~ red (0.188), yellow ~ horns (0.188 —
in a view that also contains the `id` name column), and
flys ~ insects (0.375). Mean depprob over onceover's same-view
pairs is 0.762, against 0.279 over split pairs: the merges the
pass does make are mostly Lace-supported; the gap is recall, and
on this table recall is capped first by the column cap and then
by the greedy partition's conservatism.

## Filing

Per issue #22's acceptance, the misses are filed against the
views detector as issue #73, carrying these measurements, rather
than absorbed here.

## Follow-up: the issue #73 fix (2026-10-31)

Same tables, same Lace reference. The depprob matrix was recomputed
with pylace 0.10.1 for this follow-up and reproduces the recorded
aggregates exactly (605 pairs at depprob >= 0.5; 351 both-viewed
under the old selection; 7 recovered). The onceover side is the
views-focused profile (standard budget, views detector enabled) on
the fixed code. Two changes, both in
`crates/onceover-mines/src/views.rs`:

1. **Cap selection by dependence pre-screen.** When more columns
   are eligible than the 64-column cap, the pass no longer takes
   the first 64 in table order. A pre-screen ranks each column by
   its strongest pairwise dependence with any other eligible
   column - normalized mutual information (MI divided by the
   smaller marginal entropy) over the search sample, continuous
   columns in 8 equal-frequency bins - and keeps the top 64. A
   column with more distinct values than half its present rows
   (an identifier) gets no ranking signal from any pair: under
   raw MI it ranked first on this table by memorizing the other
   columns. The screen is bounded - at most 256 rows and 250,000
   pair evaluations, on a fixed stride over the pair list beyond
   that - because an unbounded screen on a several-thousand-column
   table costs more than the pass's work budget (measured on
   GSE15258 below).
2. **Merge attempts 64 -> 256**, picked from the sweep below.

### Animals, after

- The identifier is now the first column dropped; the kept set is
  chosen by dependence. Reachable dependent pairs: **360 of 605**
  (was 351).
- Recovered: **27 of 360** (was 7 of 351). Same-view pairs 38
  (was 10), of which 11 fall below 0.5 in Lace (was 3); mean
  depprob over same-view pairs 0.724 (was 0.762). Recall quadruples;
  precision per joined pair is slightly lower.
- Sweep with the new selection, recovered at merge attempts
  64 / 128 / 256 / 512: **19 / 23 / 27 / 28**. (The first screen
  iteration - raw MI, no identifier guard - reached 16 at 64
  attempts with a smaller reachable pool, 338.) 256 is the knee:
  512 adds one recovered pair at the cost of two more
  below-0.5 joins.
- Still missed by the screen: `paws`, whose depprob with `claws`
  is 1.000 but whose 50-row sample association ranks below the
  cut. Recorded in the design doc's evaluation notes.

### Satellites, after

Partition unchanged: 18 views, the same 2 multi-column views
({Class_of_Orbit, Perigee_km, Period_minutes} and
{Inclination_radians, longitude_radians_of_geo}).

### Wide tables

On GSE15258 (5,006 columns) the unbounded screen exhausted the
pass's work budget and flipped coverage to capped; with the
bounded screen the corpus profile reports ran_exact, selects 64
of 5,006 columns by dependence, and takes 12.6 s (the unbounded
run took 44.5 s in the same harness). Corpus: 16/16 datasets ok.
