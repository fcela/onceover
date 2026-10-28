# SEC EDGAR company facts — attribution

- **Source**: SEC EDGAR company facts API,
  https://data.sec.gov/api/xbrl/companyfacts/ (per-company JSON).
- **License**: public domain (US government source). SEC fair-access
  rules apply to retrieval (declared User-Agent, <= 10 requests/second);
  `prepare.py` complies.
- **Retrieved**: 2026-10-28.
- **Contents**: one row per reported fact for six us-gaap concepts
  (Revenues, NetIncomeLoss, Assets, Liabilities,
  CashAndCashEquivalentsAtCarryingValue, OperatingIncomeLoss) across 12
  large filers (Apple, Microsoft, Alphabet, Amazon, NVIDIA, Meta,
  Berkshire Hathaway, JPMorgan Chase, Wells Fargo, Nike, Johnson &
  Johnson, AT&T): 11,543 fact rows.
- **Documented behavior the benchmark examines**: restatements — the
  same (company, concept, period_end) appears multiple times with
  different filed dates and sometimes different values. Nothing is
  deduplicated in preparation.

## Preprocessing log

1. Facts flattened from the API's per-concept unit lists; units kept to
   USD, USD/shares, and shares. No values altered.
