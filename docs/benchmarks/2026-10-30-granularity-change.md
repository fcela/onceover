# Granularity-change detector — verification

Date: 2026-10-30. Change: a new `granularity` detector (issue
#56) reports float columns whose integer-valued share switches
regime at a seam row — the signature of a rounding or recording
change partway through a series.

Protocol: planted fixtures, the detection battery, and the
benchmark corpus.

- Fixtures (`tests/test_granularity.py`, 3 tests; unit tests on
  the split search in `crates/onceover-deps/src/granularity.rs`):
  a 2,000-value Gaussian series rounded from row 1,000 fires once,
  with `change_row` exactly 1,000 and integer shares < 5% before,
  > 95% after. Fully continuous and fully rounded columns do not
  fire (single regime), and neither do short columns (< 400
  values).
- Battery (60 planted cases): totals unchanged at 48 pass /
  2 fail / 0 gap / 10 observed (run 20261030T055509Z); the
  `rounding-change-midseries` case's found list now includes
  `granularity:formats` (the case is observe-only, so the verdict
  stays `observed`). No other case's found list changed.
- Corpus: every runnable dataset ok, all registry expectations
  met; no corpus dataset gains a granularity finding (their float
  columns hold single regimes).

Suites: Rust workspace 165 passed, clippy clean; Python 125
passed, 1 skipped. Capability manifest regenerated (25
detectors).
