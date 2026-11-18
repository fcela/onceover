# Entity fingerprints: first step

Issue #448.

## What it does

`entity_fingerprints` is an on-request detector (name it in `detectors=[...]`). For the entity column, and for each column with the `dimension` role (2 to 30 levels), it builds every entity's row counts over the dimension's levels and asks four questions:

1. **Do entities differ in mix at all?** It runs a chi-square test of homogeneity on the entity × level table. A statement needs the excess over sampling noise to be material: Cramér's V of the excess, sqrt(max(0, χ² − df) / (N·(L − 1))), at least 0.1, which is the association passes' floor. When the test fails, coverage states "entities do not differ in mix beyond sampling noise" with the statistic.
2. **Groups or a continuum?**
   - Shares are shrunk toward the pooled shares (a Dirichlet prior with a method-of-moments strength), then embedded by square roots, so Euclidean distance is the Hellinger distance.
   - The signal space is the principal axes whose variance exceeds twice the multinomial noise.
   - k-means runs for k = 1..8. k is chosen by the gap statistic: the smallest k within one standard error of the largest gap against uniform draws over the principal-axis box.
   - Groups whose union passes the homogeneity test of step 1 are merged.
   - A single group with material differences is stated as a continuum, named by the two levels at the ends of the first principal axis.
3. **Misfits.** With two or more groups, an entity whose counts reject its group's pooled profile by chi-square at 0.05 / (number of entities).
4. **Specialists.** The share of entities (with 10 or more rows) whose effective number of levels, exp(entropy), is 1.5 or less. It is stated when that share is at least 10% and at least three times the share under multinomial sampling from the pooled shares at each entity's own support.

The entity is the declared `entity` hint, else the grain's single non-axis factor (the entity `infer_spec` reports). With neither, the pass is skipped and coverage says so.

## Fixtures (benchmarks/corpus/data/synthetic, #456)

| table | result |
|---|---|
| `entity_profiles` | category: 4 groups. These are A, B and C, plus the 12 planted misfits, which share one frozen-and-dairy profile and so form a group of their own. 12 misfits are stated: 11 are planted drifters (an annual mix between B and A) and 1 is not. location: 6 groups (five single-location groups and one spread group); 71% specialists, against 5.0 effective levels overall. |
| `entity_profiles_null_shuffled` | nothing stated (excess V 0.000 on both dimensions) |
| `entity_profiles_null_one_profile` | nothing stated; customers with 3 to 8 rows are not misfits |
| `entity_profiles_continuum` | category: the continuum statement (fresh against household), not groups; location: nothing |

Each run takes 0.1 s or less (400 entities, 20–35k rows).

## Not yet

- weighting by a measure;
- joint dimensions;
- drift of an entity's mix over the time axis (the drifters above would then be explained);
- a real-data case;
- default-set membership, which waits on the precision ceilings (`docs/agents.md`).

The tables are registered in the corpus with their expectations.

## Temporal follow-up

With a resolved axis, the detector compares the row shares in up to
six periods on each side of each boundary. It keeps up to 24 exact
axis values, or forms up to 24 periods with similar row counts for
finer axes, keeping ties together. The matched cohort requires 12
rows per side per entity and at least 20 such entities. The baseline
is the equally weighted mean of the matched entities' shares. An
individual comparison excludes that entity from the baseline.

A shared change requires a 15 percentage point shift in the mean
share and at least two thirds of matched entities to move by at
least 10 points in the same direction. The largest shared change per
dimension is reported. The individual scan excludes windows that
overlap any detected shared break. For an individual change, a
level's share must move at least 20 points and exceed the same
direction of peer movement by at least 20 points. A two-sided exact
conditional period-label test (hypergeometric tails) controls for
the number of rows on each side. The Bonferroni bar accounts for
entities, levels, boundaries and dimensions. Each entity is shown at
its strongest boundary. The result depends on the axis grain and
assumes exchangeable rows within an entity under the test null.

| Probe | Temporal result |
|---|---|
| Planted switch, 2025-07-01 | 11 of 40 planted drifters recovered and no false positives. All 11 are localized to July and the increase in fresh share. The strongest example goes from 2% to 72%, versus 30% to 34% among matched peers, and appears in the 4,000-character digest. |
| Shuffled, one-profile with sparse entities, continuum | Zero drift findings. |
| Shared discontinuation | The ordered-axis control replaces a level across 30 existing entities and adds 10 new entities at the last stage. The shared shift is found at that stage and produces zero individual alerts. |
| Row order | Shuffling the planted rows with the same resolved spec gives identical drift evidence. |
| Time grain and tail | Using two-month periods retains all 11 and recovers two more planted shifts, all at July. Appending a copy of December as January of the following year retains the original 11 at July. |

A supplemental homogeneous probe used NumPy's default RNG for seeds
0 through 19. Each run generated 120 entities across six periods.
The first 60 entities had 1–5 rows each and the other 60 had 50–150.
Levels were drawn independently of period and entity with shares
0.45, 0.25, 0.20 and 0.10. None of the 20 runs produced a finding.

At monthly grain 29 planted drifters remain below the reporting bar.
Two of the 11 recovered customers also appear as annual mix misfits.
Comparisons that overlap a shared change are withheld, so a concurrent
entity-specific change can be missed.

### AdventureWorks SalesOrderHeader

The bundled Microsoft sample has 31,465 orders from 19,119 customers.
Use `customer_id` as entity, `order_date` as time, and
`ship_method_id` and `territory_id` as dimensions. The run covers the
5,000 customers with the most orders, in 24 equal-support periods.
The static pass states that all 117 customers with at least 10 orders
specialize in one ship method and in one territory. Independently
grouping the source rows by customer gives exactly one method and one
territory for each of the 19,119 customers. Only 14 customers have the
24 total orders they would need to contribute to a drift comparison,
so no boundary has a big enough matched cohort. Coverage records zero
customers tested for individual changes.

The temporal scan continues to use row counts and separate dimensions.
Measure weighting and joint dimensions remain open. The detector
continues to run on request.

### Repository checks

The corpus recorded 227/227 machine expectations on main and 228/228
on this change, with the additional expectation covering the July
break. The 11 previously documented misses are present in both runs.
The full seed-0 gate against main at `0b47836` passed with all 61 scored
totals unchanged under the gate's tolerances and no exceeded ceilings.
