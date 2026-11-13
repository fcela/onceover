# Predictable missingness: the indicator screen

Date: 2026-11-13. Change: issue #274. Detector: `missingness`
(new screen inside the existing pass; no new detector).

## Question

The missingness pass states each column's missing rate and, when
one clears a Bonferroni bar, the single condition behind it: a
grouping level, a numeric threshold, a point on the time axis.
It cannot state a pattern that only exists jointly — a
conjunction of two columns' levels, or several weak conditions
that together identify the gaps. The question was whether a
model-based screen can state those patterns with the same
discipline: a held-out measurement, a fixed bar, and silence
below it.

## Method

For each candidate column, the screen fits the library's
histogram gradient-boosting model to the column's missingness
indicator (squared loss on the 0/1 target) on a seeded 75/25
split of the encoded rows, and measures the holdout AUC by the
rank-sum identity with average ranks for ties. The target
column's own encoded features never enter the predictor pool,
and frame-role exclusions apply as elsewhere. Gains are
aggregated to the column level before the driver shortlist is
taken, under the same gain-share bars as the predictability
screen (at least 2% of total gain, or 10% of the top driver's
share, at most 8 drivers).

Frozen bars, set before the evaluation runs:

- A column is a candidate with at least 30 missing rows, a
  missing rate of at least 1% and at most 95%.
- A finding is stated when the holdout AUC reaches 0.65.
- At most 8 candidates are screened per run at the standard
  budget, 16 at deep, in missing-count order; the screen does
  not run at the quick budget.
- The pass's coverage entry states how many candidates were
  screened and how many were stated, in every outcome.

## Planted fixtures

Shipped as `tests/test_predictable_missingness.py`, plus a Rust
unit test for the AUC computation (perfect separation 1.0,
reversed 0.0, all-tied 0.5, one cross-class tie 0.875, absent
class not a measurement):

- Structural: `agent` is absent exactly when `channel` is
  "direct" (2,474 of 6,000 rows). Stated, AUC 1.00, single
  driver `channel`.
- Conjunction: `w` is absent exactly when `region` is "south"
  and `tier` is "gold" (1,508 of 6,000 rows). Stated, AUC 1.00,
  drivers `region` (68% of gain) and `tier` (32%). No single
  condition in the pass's existing search states this pattern.
- Missing completely at random at a 30% rate: not stated. The
  screen fits the candidate and reports no finding.
- Quick budget: the screen does not run and nothing is stated,
  on the structural fixture.

## Real-data sweep

Six frames profiled at the standard budget with the change
applied (wall times on a contended sandbox, indicative only):

| frame | rows | candidates | stated | time |
|---|---|---|---|---|
| Inside Airbnb NYC listings | 29,918 | — | 0 | 43.1s |
| UCI Appliances Energy (raw) | 19,735 | — | 0 | 7.1s |
| UCI Adult | 48,842 | 3 | 3 | 3.0s |
| Bike Sharing (hourly) | 17,379 | — | 0 | 3.2s |
| Taiwanese Bankruptcy | 6,819 | — | 0 | 15.5s |
| Hotel Booking Demand | 119,390 | 2 | 2 | 25.7s |

The candidate counts for the frames with no candidates at the
bar are recorded in each profile's coverage entry; the table
reports the two frames where the screen had work to do.

UCI Adult (missing values are the file's `?` code). All three
statements verified against the raw file:

- `occupation` missing is predictable from `work_class` (73% of
  gain), `age`, `income` — holdout AUC 0.996.
- `work_class` missing is predictable from `occupation` (81%) —
  AUC 0.995. The two statements are one fact read from both
  sides: `work_class` is missing on 2,799 rows, `occupation` on
  2,809, and every row missing `work_class` is missing
  `occupation` (the overlap is the full 2,799). The pass's
  co-missing block search requires a Jaccard overlap of 0.9;
  this pair's is 0.996 over the union, and the block statement
  also fires — the screen's statements name the direction and
  the strength, the block names the shared rows.
- `native_country` missing (857 rows) is predictable at AUC
  0.692, drivers `education_num`, `hours_per_week`, `race`,
  `education`, `age`. A weaker, demographic pattern; it clears
  the 0.65 bar and is stated at the lowest score tier.

Hotel Booking Demand. Both candidates stated by the pass:

- `agent` (missing on 16,340 of 119,390 rows, 13.7%): drivers
  `company` (48% of gain) and `distribution_channel` (36%).
- `company` (missing on 112,593 rows, 94.3%): drivers
  `market_segment` (63%), `distribution_channel` (14%),
  `agent` (9%).

Both match the dataset's structure (direct bookings carry no
agent; a company is recorded mainly for corporate bookings
arriving through particular segments and channels). One
limitation, recorded plainly: the frame produces 458 findings
before selection, and at the default report cap of 100 both
statements are displaced by higher-scored findings. They are
present when the cap is raised. Whether a class of statement
should be protected from displacement is the report-selection
question tracked separately (#280); this change does not
adjust scores to win selection.

The remaining four frames produced no statement: their columns
missing at the candidate bar are missing in ways a model of the
other columns cannot identify on held-out data, which is the
screen's negative control at scale.

## Provenance

- Inside Airbnb NYC listings: snapshot of 14 September 2026,
  CC BY 4.0 (file used by the earlier case studies).
- UCI Appliances Energy: UCI Machine Learning Repository
  dataset 374.
- UCI Adult: UCI dataset 2, DOI 10.24432/C5XW20; the corpus
  copy under `benchmarks/corpus/data/business/`.
- Bike Sharing: UCI dataset 275, DOI 10.24432/C5W894; corpus
  copy under `benchmarks/corpus/data/anomaly/`.
- Taiwanese Bankruptcy: UCI dataset 572, DOI 10.24432/C5004D;
  corpus copy under `benchmarks/corpus/data/business/`.
- Hotel Booking Demand: Antonio, de Almeida & Nunes, "Hotel
  Booking Demand Datasets", Data in Brief (2019), CC BY 4.0;
  TidyTuesday mirror of 2020-02-11, read with
  `null_values=["NA", "NULL", ""]`.
