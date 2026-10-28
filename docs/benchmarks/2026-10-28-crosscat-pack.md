# Benchmark corpus: CrossCat pack (2026-10-28)

Protocol: same harness as the initial corpus entry
(`benchmarks/corpus/run_benchmarks.py`), standard budget, onceover
0.1.2 at main 1cd09b28. CrossCat-pack entries run the default detector
set plus the views pass (the pack exists to exercise column/row
partitioning, views' home ground). Run record:
`benchmarks/corpus/runs/2026-10-28-crosscat.json`.

## Results

| dataset | shape | findings | time | expectations |
|---|---|---|---|---|
| congressional-voting | 435 x 17 | 77 | 0.1s | 3/3 |
| hospital-dha | 307 x 64 | 100 (cap) | 1.1s | 3/3 |
| state-unemployment | 609 x 52 | 100 (cap) | 1.9s | 2/2 |
| gse15258-expression | 86 x 5,006 | 100 (cap) | 20.2s | 3/3 |

Skipped: lace-animals, lace-satellites — the Lace examples data could
not be sourced from this environment (the probcomp GitHub org hosts
CrossCat and BayesDB but no Lace repository); both stay on the
registry with the reason recorded, so the views head-to-head remains
specified rather than silently dropped.

## Observations

- **Voting**: views partitions the 435 members into 3 row clusters
  whose headline votes shift by 1.00 between clusters — the party
  blocs, recovered without the party column as a target. Conditional
  rules onto party also fire, as registered.
- **Hospital (DHA)**: 64 columns is exactly the views cap; the pass
  runs. The spending total TTL_MDCR_SPND differs from the sum of its
  8 published components by at most 2 currency units across all 307
  rows (measured directly) — consistent with rounding in the source's
  per-decedent averages, and below the identity detector's exactness
  bar, so no identity is reported. Registered as a known-miss
  tripwire: if identities gain a tolerance mode, this is the case
  that should light up.
- **State unemployment**: views runs on the 51 state columns; state
  series associate strongly (national cycle), as registered.
- **GSE15258**: views reports `ran_exact` but on the first 64 columns
  only; its coverage detail names the 4,942 dropped columns — a
  63 KB coverage string. The cap is disclosed behavior, and the
  disclosure itself is a context-budget problem (issue #27 has the
  measured instance now).
- Three of the four tables saturate the report's 100-finding
  selection cap. What the cap drops is not visible from the report;
  that is also #27 material, recorded here as measured.
