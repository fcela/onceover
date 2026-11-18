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
