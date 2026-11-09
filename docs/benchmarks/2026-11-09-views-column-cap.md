# Views column cap: 64 against 256 (2026-11-09)

## Question

The views pass examines at most 64 eligible columns
(`ViewsConfig.max_columns`); a pairwise-dependence pre-screen keeps
the strongest 64 and the rest go unexamined. Issue #214 asks whether
64 is the right default. This run settles the question with the
in-repo corpus: profile the wide corpus tables with the views pass
at the shipped cap of 64 and at 256, and compare coverage gained
against runtime and report size.

## Protocol

- Build: the issue-#214 branch build (`maturin develop --release`),
  once with `max_columns: 64` (shipped) and once with
  `max_columns: 256`; nothing else differed between the two builds.
- Entry point: `onceover.profile(df, detectors=["views"],
  max_findings=500)` on each dataset's committed CSV, two runs per
  dataset per cap, median wall time reported.
- Datasets: every corpus table with at least 62 columns —
  taiwanese-bankruptcy (96 columns, 6,819 rows), lace-animals (86,
  50), hospital-dha (64, 307), splice (62, 3,190), and
  gse15258-expression (5,006, 86). Narrower corpus tables cannot
  reach the cap and are out of scope by construction.
- Recorded per run: wall time, findings by category and type,
  columns eligible / examined / dropped past the cap (from the
  `unexamined_columns` finding's evidence at each cap).

## Results

| Dataset | Columns | Cap 64: time, findings, dropped | Cap 256: time, findings, dropped |
|---|---|---|---|
| taiwanese-bankruptcy | 96 | 6.31 s, 128, 32 | 11.87 s, 185, 0 |
| lace-animals | 86 | 0.07 s, 140, 22 | 0.11 s, 182, 0 |
| hospital-dha | 64 | 1.01 s, 109, 0 | 1.01 s, 109, 0 |
| splice | 62 | 2.34 s, 124, 0 | 2.40 s, 124, 0 |
| gse15258-expression | 5,006 | 3.58 s, 113, 4,942 | 6.91 s, 484, 4,750 |

No run hit the search work budget at either cap.

## Reading

Raising the cap buys complete coverage only for tables between 65
and 256 columns: taiwanese-bankruptcy and lace-animals are fully
examined at 256, at 1.9x and 1.6x the wall time. For the widest
table it buys little coverage — 4,750 of 5,006 columns still drop —
while roughly doubling the time (3.58 s to 6.91 s) and quadrupling
the findings (113 to 484), and report size is the binding cost of a
profile. Tables at or under 64 columns are unaffected either way.

The evidence is not one-sided, so the default stays at 64. What
changed instead is the disclosure: the dropped columns are now
stated as an `unexamined_columns` finding (issue #214), so a reader
of the findings can see the cap bind and act on it — profile the
named columns separately — rather than the information living only
in coverage.
