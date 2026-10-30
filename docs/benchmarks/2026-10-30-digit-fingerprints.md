# Digit fingerprints detector

Date: 2026-10-30. Issue #59: amounts and counts pile on round
values when a human or a rounding process produced them, and
the detector set said nothing about the recording process.
This change adds the `digits` detector (onceover-deps, exact
counts), three checks in the formats family.

## The three checks

**Heaping** (integer-valued columns, >= 500 values, >= 20
distinct). For units 1000, 100, 10 — largest first, and only
when the unit is small against the column's range — the share
of nonzero values divisible by the unit is compared with the
1/U share a smooth recording implies. A pile reports when the
share is at least 3x the baseline at z >= 6. Zeros are
excluded from the multiples test (0 is a multiple of every
unit) and their share is stated in the evidence separately,
alongside the 0/5 terminal-digit shares.

**Terminal-digit preference** (same eligibility). The
last-digit distribution against uniformity, chi-square on
9 df with a strict bar (44, ~1e-6) plus a materiality floor
(top digit >= 15%). Suppressed when heaping already fired on
the column — one phenomenon, one finding.

**Benford deviation** (float and integer columns). First
significant digits of positive values against Benford's law,
behind a conservative eligibility rule, recorded in coverage
as a count: >= 500 values, >= 95% of them positive,
max/min >= 1000 (three orders of magnitude), >= 100 distinct
values, and — for integer-valued columns only — distinct
share < 50%. That last clause excludes identifier-like
integer columns; continuous quantities are near-unique by
nature and stay eligible (an earlier draft applied the rule
to all columns and wrongly excluded genuine amount columns —
caught by the uniform-first-digit fixture). A deviation
reports only when it is both significant (chi-square on 8 df,
bar 34) and material (mean absolute deviation >= 0.012, the
published nonconformity boundary for first digits), so a
conforming column stays quiet at any sample size.

Scope limit, stated rather than silent: fixed-precision
non-integer columns (money with cents) are not digit-tested
in this version; their last significant digit is the cents
digit and needs decimal-exact extraction.

## Verification

- Acceptance fixtures (tests/test_digits.py and battery):
  a measured column with 30% of values rounded to the
  nearest 10 fires heaping at unit 10; the same column
  unrounded is quiet on both integer checks. A
  Benford-conforming column (products of uniform draws) is
  quiet; a uniform-first-digit column of the same size
  fires. A bounded column with uniform first digits is
  quiet through ineligibility, and its coverage line
  records 0 Benford-eligible columns.
- Battery: four cases added (`formats-heaping-rounded`,
  `formats-heaping-control`, `formats-benford-conforming`,
  `formats-benford-uniform`), all pass. On this branch's
  build the battery reads 53 pass / 4 fail / 0 gap /
  9 observed of 66: the two standing drift fails plus the
  sentinel and hotspot cases, which need PRs #77/#78 and
  pass on those builds. No other case changed status.
- Corpus: expectations unchanged.
