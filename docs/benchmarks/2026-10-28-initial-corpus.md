# Benchmark corpus: first run (2026-10-28)

Protocol: harness from issue #20 (`benchmarks/corpus/run_benchmarks.py`),
standard budget, onceover 0.1.2 at main 1cd09b28. Every bundled dataset
is hash-verified before profiling. Run record:
`benchmarks/corpus/runs/2026-10-28-first-full.json` plus the bank-marketing
rerun after a registry hash refresh (see "Hash note" below).

## Results

| dataset | rows | findings | time | expectations |
|---|---|---|---|---|
| thyroid (allhypo) | 3,772 | 68 | 1.1s | 8/8 |
| abalone | 4,177 | 32 | 0.2s | 3/3 |
| soybean (683, text labels) | 683 | 100 | 0.2s | 4/4 |
| splice | 3,190 | 13 | 3.1s | 2/2 |
| online-retail-ii (test-only, local) | 1,067,371 | 5 | 2.5s | 5/5 |
| bank-marketing | 45,211 | 36 | 1.1s | 2/2 |
| sec-edgar-company-facts | 11,543 | 29 | 0.3s | 3/3 |

Skipped, with reasons recorded in the registry: churn and
crystallography (no redistributable/public source), Olist (license
gate), credit-card-fraud (Kaggle-gated), FRED-MD (source host returned
HTTP 403 from the build environment).

283 findings across 7 tables in about 8.5 seconds of profiling.

## GritBot case dispositions

RuleQuest's published GritBot output is the pack's ground truth
(https://rulequest.com/gritbot-examples.html).

- **Soybean case 78 — found.** The bundled row 78 is leafspots_halo =
  absent with leafspot_size = gt-1/8, and the profiler reports
  class + leafspots_halo -> leafspot_size with exactly 1 violation in
  the file: the structure the case breaks.
- **Abalone (23 cases) — partial.** The measurement web (weights,
  length, diameter) is reported as predictive dependencies, but no
  finding isolates the violating rows (part weight above whole weight,
  length below diameter), and no sum identity over the weight parts is
  reported. A known-miss tripwire on the sum form is in the registry.
- **Splice cases 550, 839 — partial.** The near-universal positional
  patterns are reported as conditional rules (base_30 = G in 100% of
  intron_exon diagnostic rows, base_29/base_31/base_32 similar), but
  the individual breaking cases are not isolated.
- **Soybean cases 558, 614 — missed.** Single-case conditional
  contradictions; no current finding type isolates them.
- **Thyroid, all five cases — missed.** The age-455 patient, the two
  pregnant patients in their seventies, and the two assay-combination
  cases produce no finding. Age appears only as an input to predictive
  dependencies; no finding isolates an extreme value within a column
  or within a subset. Two known-miss tripwires (intervals on age;
  conditional rules over pregnant+age) are registered.
- **Churn, crystallography — unevaluated** (no data; see registry).

The pattern across packs: structures are found; the individual
surprising cases inside them are not. Case isolation -- a value that
is surprising within a subset defined by other columns -- is the
capability the GritBot pack measures, and it is currently absent.

## Business pack observations

- **Online Retail II** yields 5 findings on 1,067,371 rows:
  customer_id -> country (99.95%, 80 violations), description +
  unit_price -> stock_code (99.77%, 468 violations), 34,335 exact
  duplicate rows, country concentration (91.9% UK), stock_code format
  mix. Neither the 19,494 cancellation invoices (22,950
  negative-quantity rows) nor the 243,007 rows without a customer id
  produce a finding. Coverage notes functional_dependencies capped at
  this size.
- **EDGAR company facts**: form <-> fiscal_period rules and the
  concept structure are found. Restatements -- the same (company,
  concept, period_end) reported again with a later filed date and
  sometimes a different value -- are not isolated by any finding; a
  duplicates tripwire is registered.
- **Bank marketing**: the never-contacted structure (pdays = -1 with
  poutcome missing) is reported as a conditional rule, because in the
  file as served the missing outcome is the literal string "NaN".

## Two preprocessing measurements

1. **Bank "NaN" decode (tried, reverted).** Decoding the string "NaN"
   to a true null made the table cleaner and the never-contacted
   structure invisible: with nulls, no detector reported it. The
   bundle keeps the file as served; the interaction is recorded in
   the dataset's ATTRIBUTION.md. Preprocessing choices change what a
   profiler can see; the corpus now has a measured instance.
2. **Retail invoice typing (bug, fixed).** The xlsx stores most
   invoice numbers numerically; a numeric-typed read silently nulled
   the 19,494 C-prefixed cancellation invoice numbers. Caught by
   counting rows before and after; prepare.py now forces string
   types for Invoice and StockCode.

## Hash note

The registry hash for bank-marketing was refreshed once during this
run (the NaN experiment above changed the file after the first hash
was recorded). The final bundled file is the as-served preparation;
its hash is in the registry and verifies.

## Not verified / open

- Splice position numbering (GritBot's A30/A34 vs base_30/base_34)
  assumes the same window origin; the ARFF/names documentation for
  the original file was not re-read for this run.
- FRED-MD expectations are unwritten until a bundle exists.
- Northwind/Chinook sample databases (in issue #21) are not yet
  registered; their redistribution terms were not verified.
