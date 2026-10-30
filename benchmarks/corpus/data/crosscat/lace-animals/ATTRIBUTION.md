# Lace Animals — attribution

- **What it is**: 50 animals x 85 binary feature columns plus an
  `id` name column (86 columns as distributed) — the Kemp &
  Tenenbaum animal-features table built from Osherson et al.
  feature ratings, and one of the two example datasets shipped
  with Lace, the maintained CrossCat implementation. Issue #22's
  head-to-head baseline for the views pass.
- **Distribution and license**: the Lace project
  (github.com/promised-ai/lace), MIT License — the repository's
  LICENSE, verified 2026-10-30. The table is bundled as data.csv
  exactly as the project distributes it
  (cli/resources/datasets/animals/data.csv); prepare.py
  re-downloads it and verifies the pinned sha256. The bundled
  file is byte-identical to the copy pylace's `lace.examples`
  loads (sha256 checked against the installed package,
  2026-10-30), so the head-to-head profiles the same bytes on
  both sides.
- **As served**: feature cells include missing entries; the file
  is bundled unchanged, missingness included.
