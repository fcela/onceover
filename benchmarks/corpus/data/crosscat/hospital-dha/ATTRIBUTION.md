# Hospital measures (DHA table) — attribution

- **Source**: the CrossCat project's distributed hospital table,
  `www/data/dha.csv` in https://github.com/probcomp/crosscat
  (repository license Apache-2.0). Underlying measures are Medicare
  spending and outcome measures by hospital (306 facilities).
- **Retrieved**: 2026-10-28.
- This is the hospital table the CrossCat evaluation used; issue #22
  names CMS hospital measures — the CMS Provider Data portal is the
  same measure family from CMS directly, and is the recorded
  alternate source if a first-party pull is wanted later.

## Preprocessing log

None — byte-identical copy. Column names are the source's measure
codes; renaming them without the source codebook would invent
information.
