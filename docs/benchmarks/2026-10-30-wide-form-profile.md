# Wide-form profiling of long-format frames — verification runs

Date: 2026-10-30. Change: when the pivot pass verifies a long-format
frame, the pipeline also profiles the pivoted wide form and merges
its findings (tagged `source_form` in their evidence), and the
`long_format_encoding` demotion also covers the layout's predictive
restatements. Protocol: confirm no regressions on the standing
corpora, and confirm the new behavior on planted fixtures.

## Planted fixtures

- Rust facade tests (`crates/onceover/tests/wide_form.rs`, 3 tests)
  and Python tests (`tests/test_wide_form.py`, 2 tests) on a melted
  12 x 5 x 2 panel whose `life` level is a near-linear function of
  the `gdp` level: the wide profile runs (`pivot_profile` coverage
  `ran_exact`), the planted level-level association is present and
  tagged, the disjoint-range FD carries the demotion, and the quick
  budget skips the wide profile.
- The usage book's long-format panel (6 entities x 5 years x 3
  indicators) gains 5 wide-form findings, including the associations
  between the `co2_pc`, `gdp_pc`, and `life_exp` levels
  (`docs/book/usage/findings.ipynb`, re-executed).

## Regression corpora

- Benchmark corpus (`benchmarks/corpus/run_benchmarks.py`, standard
  budgets): every runnable dataset reports `ok` with all registry
  expectations met — including sec-edgar-company-facts (3/3), the
  corpus's long-format-shaped entry. Skips are the standing ones
  (source-host 403 for fred-md, Lace data not located, unwired
  suites, non-redistributable sources).
- Detection battery (`onceover-detection-battery`, 60 planted
  cases): unchanged at 47 pass, 2 fail, 1 gap, 10 observed
  (run 20261030T050125Z). The 2 fails are the documented
  single-binary-predictor drift boundary; the 1 gap is the
  out-of-order backfill case tracked separately.

## Suites

- Rust workspace: 162 passed, 0 failed; clippy clean.
- Python: 117 passed, 1 skipped.
