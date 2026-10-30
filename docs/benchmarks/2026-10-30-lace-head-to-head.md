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
