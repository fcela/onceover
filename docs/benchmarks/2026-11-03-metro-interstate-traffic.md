# Corpus: Metro Interstate Traffic Volume added (2026-11-03)

Protocol: `benchmarks/corpus/run_benchmarks.py`, standard budget,
onceover 0.1.3 at main bcf9dcde, run in this environment (2 cores).
Single-dataset run record: `benchmarks/corpus/runs/20261103T175035Z.json`.
Full-corpus run record: `benchmarks/corpus/runs/20261103T175207Z.json`
(21 ok / 0 fail / 11 skipped over 32 registered datasets; before this
addition: 20 ok / 0 fail / 11 skipped over 31).

## Metro Interstate Traffic Volume — new bundled dataset

UCI dataset 492 (DOI 10.24432/C5X60B, verified on the UCI dataset
page 2026-11-03; CC BY 4.0; John Hogue, 2019): hourly westbound I-94
traffic volume for MN DoT ATR station 301 (Minneapolis–St Paul) with
the hour's weather and holiday label, 2012-10-02 09:00 to 2018-09-30
23:00 — 48,204 rows, 9 columns. Bundled under
`benchmarks/corpus/data/anomaly/metro-interstate-traffic/` with
`prepare.py` and `ATTRIBUTION.md`, in the anomaly pack: a real-world
hourly demand stream alongside the NAB streams, but without published
anomaly labels, so its expectations pin observed behaviour and
documented misses rather than label coverage.

Preparation is decompression only: `prepare.py` verifies the source
zip against a pinned sha256
(`b99aeabcbd6cc86f642da3a79d90883425798f58abd3b3302da2fa19dda73768`,
matched by a fresh download on 2026-11-03) and writes the published
CSV byte for byte (`data.csv` sha256
`749c90d720360a4215bb15345526073c079ba4cc95e3fa558796d083f85fce9e`).
The dataset's quirks are kept as served: the timestamp repeats
(40,575 distinct values; 7,629 rows are extra copies of an hour),
22.8% of the hourly grid has no row, non-holidays are the literal
string `None`, `date_time` stays a string, and ten rows read
`temp` = 0.0 K.

## Provenance of the expectations

The dataset was profiled as an unseen case study on 2026-11-03,
before bundling (pass 1, frame exactly as published, standard
budget: 24 findings in 1.8 s). Every quantitative claim below was
also computed independently of the profiler with Polars during that
study, and the bundled run reproduces the study's profile exactly:
24 findings, 11/11 expectations passing, 1.6 s.

Expectations pinned:

- Exact functional dependency `weather_description -> weather_main`
  (38 descriptions onto 11 mains, 0 violations in 48,204 rows;
  verified independently).
- Approximate dependency `(temp, weather_main) -> snow_1h` at
  99.91% (44 violations; the count verified independently by
  group-mode counting).
- Value concentrations: `holiday` is `None` on 99.87% of rows,
  `snow_1h` is 0 on 99.87%, `rain_1h` is 0 on 92.81%.
- Sentinel pattern: `rain_1h` = 0.3 on 121 rows, 97 of them inside
  rows 13039–23670 — reproduced exactly.
- 17 exact duplicate rows, and a `max_findings` guard of 40 against
  the first run's 24.
- Three documented cases, all status `missed`, recorded for hand
  review rather than as finding expectations: the duplicate
  timestamps (no finding or coverage entry reports that the hour
  key repeats; keys coverage reads "0 single-column unique,
  0 composite"), the hourly grid gaps (no span or completeness
  statement; the temporal passes decline because `date_time` loads
  as a string), and the ten 0.0 K readings (no finding isolates
  them). These are the frame-model cases this entry exists to hold:
  the near-key statement, axis assembly for string timestamps, and
  completeness against the time grid.
