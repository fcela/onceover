# Backfill ordering findings — battery verification

Date: 2026-10-30. Change: the sortedness pass also reports the
backfill shape (issue #57) — a column ordered except for a late
block whose values return to earlier territory.

Protocol: the detection battery (60 planted cases,
`onceover-detection-battery`) before/after, the benchmark corpus,
and planted fixtures in the test suites.

- Battery: 47 pass / 2 fail / 1 gap / 10 observed before;
  **48 pass / 2 fail / 0 gap / 10 observed** after
  (runs 20261030T053758Z, 20261030T053834Z). The flipped case is
  `dates-backfill-out-of-order` (120 ordered months plus a 10-row
  block from earlier months appended at the end), now found by the
  sortedness pass on the month column. The 2 fails are unchanged
  (the documented single-binary-predictor drift boundary). No
  other case moved, so the new claim does not fire on the battery's
  sorted, shuffled, or noisy controls.
- Corpus: every runnable dataset ok, all registry expectations
  met, as before.
- Fixtures: `tests/test_backfill.py` (3 tests — the planted block
  fires with the seam metrics; a sorted column gets only the
  positive claim; a single dip that later reaches new highs does
  not fire) and unit tests on the scan in
  `crates/onceover-deps/src/sorted.rs`.

Suites: Rust workspace 164 passed, clippy clean; Python 122
passed, 1 skipped.
