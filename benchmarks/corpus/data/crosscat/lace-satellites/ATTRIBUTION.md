# Lace Satellites — attribution

- **What it is**: 1,164 satellites x 21 columns of operator,
  purpose, launch, and orbital characteristics (target column
  `Class_of_Orbit`) — one of the two example datasets shipped
  with Lace, the maintained CrossCat implementation. Issue #22's
  head-to-head baseline for the views pass.
- **Distribution and license**: the Lace project
  (github.com/promised-ai/lace), MIT License — the repository's
  LICENSE, verified 2026-10-30. The table is bundled as data.csv
  exactly as the project distributes it
  (cli/resources/datasets/satellites/data.csv); prepare.py
  re-downloads it and verifies the pinned sha256. The bundled
  file is byte-identical to the copy pylace's `lace.examples`
  loads (sha256 checked against the installed package,
  2026-10-30), so the head-to-head profiles the same bytes on
  both sides.
- **Original compilation**: not credited in the Lace materials
  (repository and codebook checked 2026-10-30); recorded as
  uncredited rather than guessed.
