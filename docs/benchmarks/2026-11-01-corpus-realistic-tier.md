# Corpus: realistic tier — Taiwanese Bankruptcy added, Bank Marketing deepened (2026-11-01)

Protocol: `benchmarks/corpus/run_benchmarks.py`, standard budget,
onceover 0.1.2 at main 9ec271d, run in this environment (2 cores).
Run records: `benchmarks/corpus/runs/20261101T140925Z.json`
(taiwanese-bankruptcy) and `20261101T141140Z.json` (bank-marketing).

## Taiwanese Bankruptcy Prediction — new bundled dataset

UCI dataset 572 (DOI 10.24432/C5004D, CC BY 4.0): 6,819 companies,
95 financial-ratio features plus the bankruptcy flag, Taiwan Economic
Journal 1999–2009, 220 bankrupt (3.2%), no missing values. Bundled
under `benchmarks/corpus/data/business/taiwanese-bankruptcy/` with
`prepare.py` and `ATTRIBUTION.md`; the only transformations are the
served file's `, ` separator decode and the `Bankrupt?` -> `bankrupt`
rename.

Expectations pinned, 4/4 passing (90.1s, 100 findings):

- The accounting identity stated exactly by the functional pass:
  `Net worth/Assets = -1.000000 * Debt ratio % + 1.000000` on all
  6,819 rows. Verified independently with Polars: the two columns
  sum to 1.0 at float precision (min 0.9999999999999989, max
  1.000000000000001).
- Two ratios carried twice under two names, each stated as an exact
  identity on all rows: `Current Liability to Equity =
  1.000000 * Current Liabilities/Equity` and `Current Liability to
  Liability = 1.000000 * Current Liabilities/Liability`.
- Documented case (recorded, not a finding expectation):
  `Liability-Assets Flag` is documented as 1 when total liability
  exceeds total assets; it agrees with `Debt ratio % >= 1` on
  99.90% of rows (7 disagreements, measured with Polars).

The graded case this dataset adds: the ROA (A)/(B)/(C) family
correlates at 0.94–0.99 (measured) — near-duplicate engineered
ratios, not exact copies.

## Bank Marketing — expectations deepened from 2 to 5

Same bundled file (45,211 rows); expectations now 5/5 (13.8s,
71 findings). Added:

- Conditional rule, measured in the standard profile:
  `month = jun` -> `contact = "NaN"` in 84.67% of matching
  diagnostic rows against a 29.05% baseline.
- Hotspot on the same pair: cell (contact = 'NaN', month = 'jun')
  holds about 10% of scanned rows, roughly triple the
  independence expectation.
- Documented case for the target structure, measured with Polars:
  overall subscription rate 11.7%; by previous-campaign outcome,
  success 64.7% (n = 1,511) against 9.2% for never-contacted
  (n = 36,959); by month, mar 52.0% (n = 477) and dec 46.7%
  (n = 214) against may 6.7% (n = 13,766). Recorded as
  documentation because the standard-budget run carries no target.

The registry notes now also record that the literal string "NaN" is
this file's missing-value marker (contact 28.8%, poutcome 81.7%,
education 4.1% of rows), kept as served so the never-contacted
structure stays visible to the detectors.
