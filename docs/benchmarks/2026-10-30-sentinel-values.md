# Sentinel values detector

Date: 2026-10-30. Issue #55: sentinel values standing in for
missing data (-999, -1 in a non-negative measure, 9999) produced
no finding, so a column's stated missingness understated its
real missingness and the sentinel polluted downstream
statistics. This change adds the `sentinels` detector
(onceover-deps, exact scan).

## Method

For each numeric column, every distinct value with at least 20
occurrences is a candidate. A candidate is reported when two
conditions hold together:

- **Window.** The shortest run of rows covering at least 80% of
  the value's occurrences spans at most 25% of the column — the
  occurrences form a block, not a scatter.
- **Bulk.** The value sits outside the column's central bulk:
  beyond the Tukey far-out fence (q1 - 3*IQR, q3 + 3*IQR) of the
  column's other values (recomputed exactly when the candidate
  exceeds 5% of the column), or across the other values' sign
  boundary — a negative value in a non-negative column, or a
  repeated 0 against strictly positive values, is a code however
  wide the fence is (the fence alone misses -1 in a 0-5 count
  column, whose fence extends to -6.25).

The fence is what separates a sentinel from a genuinely common
value in a skewed column; the window is what separates it from
the surprises pass's territory (isolated extreme values) and
from evenly spread encodings. Findings are capped at three per
column, largest first. The evidence payload names the value,
its count and share, the window's start/end/span rows, the
fence used, and the column mean with and without the value —
the pollution stated as a number. Type `sentinel_values`,
category `missingness`.

## Verification

- Detection battery: `missingness-sentinel-values` flips from
  observed to pass (battery now 49 pass / 2 fail / 0 gap /
  9 observed; the 2 fails are the pre-existing single-binary
  drift boundary). The planted fixture (-999 over rows 600-759
  of a Gaussian(50, 10) column) reports 92 occurrences with its
  covering window inside the block, mean 50.20 excluding the
  sentinel vs 1.93 with it.
- Null controls stay quiet (tests/test_sentinels.py): a clean
  Gaussian column, zero-inflated counts (0 is in-bulk), a
  scattered repeated far value (no window), and a constant
  column (concentration's finding, not this one). The -1 block
  in non-negative counts fires via the sign-boundary rule.
- Corpus: expectations unchanged across the bundled and
  reference datasets.
