# Corpus: Adult (Census Income) added (2026-11-03)

Protocol: `benchmarks/corpus/run_benchmarks.py`, standard budget,
onceover 0.1.3 at main cfe509e3, run in this environment. The
dataset was also profiled with the frame-model step-2 build
(PR #108, schema 0.5) as the account block's first unseen test;
the comparison is recorded below. Run records:
`benchmarks/corpus/runs/` (2026-11-03).

## Adult (Census Income) — new bundled dataset

UCI dataset 2 (DOI 10.24432/C5XW20, verified on the UCI dataset
page 2026-11-03; CC BY 4.0; Becker & Kohavi, 1996): 48,842
respondents from the 1994 Census (the published train and test
splits concatenated) with fourteen demographic and employment
attributes and the over-$50K income label. Bundled under
`benchmarks/corpus/data/business/adult-census-income/` with
`prepare.py` and `ATTRIBUTION.md`, in the business pack: the
classic census classification table, and the corpus's first
entry with no time axis and no key — the frame shapes the
earlier additions (panels, timestamped streams) do not cover.

Preparation is decode-and-rename only: `prepare.py` verifies
the source zip against a pinned sha256
(`7537312dd56c2b98035880805ce99e68183a30ee468aa5329d6df0fbb3cc21bb`,
matched by a fresh download on 2026-11-03), skips the test
file's header line, strips its labels' trailing period, decodes
the `?` missing marker to null, and renames the columns to
descriptive names (`fnlwgt` → `final_weight`, `education-num`
→ `education_num`, the label → `income`). Prepared file sha256
`eaf8404eee27ae20a76ace5244cbae63ba77dc546648ee9c495799e3a9352e6c`,
byte-identical to the file the case study profiled.

## Provenance of the expectations

The dataset was profiled as an unseen case study on 2026-11-03
(iteration 3 of the unseen-dataset loop), before bundling.
Ground truth was computed independently with Polars before
either profile was read; every quantitative claim below traces
to that computation or to the profile's own evidence payload.

Expectations pinned (9 checks, all passing):

- Exact functional dependencies in both directions between
  `education` and `education_num` (0 violations in 48,842 rows;
  a bijection over 16 levels, verified by group counting).
- The `duplicate_encoding` finding: the one place the report
  states the encoding as an encoding ("16 levels, paired
  one-to-one on all 48842 rows").
- Exact duplicate rows: 52 redundant rows in 49 groups, largest
  group 3, over all 15 columns.
- Value concentrations: capital_loss 95.3% zero (46,560 rows),
  capital_gain 91.7% zero (44,807), native_country 91.3%
  United-States (43,832).
- A `coverage_status` check that the keys pass records
  `not_applicable`: no key exists — no single column is unique
  and the full tuple repeats, so no subset can be one.
- A `max_findings` guard of 62, recording the selected count
  (the report is not cap-saturated on this frame).

Seven documented cases, reviewed by hand each run:

- **The missingness is stated nowhere** (status missed).
  work_class is null on 2,799 rows (5.73%), occupation on 2,809
  (5.75%), native_country on 857 (1.75%) after the `?` decoding;
  no finding in the 62 mentions missing values. The pattern of
  issue #100, on a second unseen frame.
- **The capital_gain top-code** (status missed): 99,999 on 244
  rows, the maximum of a column that is 0 on 91.74%. The
  sentinels pass scanned all six numeric columns and reported
  nothing.
- **The capital_loss mass points** (status missed): 1902 on 304
  rows, 1977 on 253, 1887 on 233, behind a 95.33% zero share.
- **The weight-only near-duplicates** (status missed): 6,374
  excess rows repeat a row differing only in `final_weight`
  (42,468 distinct rows ignoring it). The duplicates finding
  compares all 15 columns and reports the 52 exact repeats;
  `final_weight` — the CPS sampling weight, per the source's
  `adult.names` — is roled a plain measure and used as a
  predictor in six predictive findings.
- **The dominant structure** (status missed): marital_status
  explains 33.5% of the variance in age, occupation 31.0% of
  education_num, relationship 22.5% of age (eta-squared,
  measured with Polars). See the step-2 comparison for why the
  account does not state it.
- **An extreme-value claim misstates its evidence** (status
  partial): within work_class = Federal-gov, the claim says
  hours_per_week's other values are all ≤ 80 with a gap of 12;
  its own payload lists extreme values 99, 99, 92 over a bulk
  edge of 80. The true extremes are 92 (1 row) and 99 (2 rows);
  the gap isolating 99 is 7. The sentence template describes a
  single isolated value — the claim-rendering failure class
  first recorded on Bike Sharing, recurring here.
- **A categorical-outlier enumeration undercounts** (status
  partial): the Some-college & Husband slice (3,658 rows) is
  said to have 2 breaker rows; it has 3 (rows 3787, 39624,
  48118, all Married-AF-spouse). The finding's own
  dominant_share, 0.999180 = 3655/3658, implies 3 while its
  outlier_count says 2.

Fourteen claims were spot-checked against independent
computation (the duplicates figures, the three concentrations,
the bijection, the Husband slice's single Female breaker, the
Some-college slice and its breakers, the Federal-gov extremes,
the three terminal-digit shares, the capital-gain × income
hotspot against its full-data analog, the income majority
baseline, the Wife-rule baseline): twelve agreed exactly or,
for the two sample-scoped claims, were consistent with their
full-data analogs. The two partial cases above are the only
disagreements.

## The step-2 build on this frame

The frame-model step-2 build (schema 0.5) selects the identical
62 findings with identical claims and identical scores; its
added value here is presentation order and one coverage
disclosure, and the run is recorded because the absences are
the result:

- **Grain**: none stated. The `frame_model` coverage entry
  records "no exact key found among the dimension columns and
  the time axis" — the correct negative (no subset of columns
  is a key), but the digest and the markdown frame section carry
  no account line, so a reader cannot distinguish "no key" from
  "not computed". The near-key concept is axis-bound; the
  frame's actual near-key fact (the weight-only duplicates) is
  stated by neither build.
- **Dominant structure**: none stated, and the coverage claim
  "none at the 0.10 variance-share bar" is contradicted by
  measurement (33.5% and 31.0% above). The mechanism is
  upstream: the dominant search takes its measures from columns
  roled `measure`, and step 1 roles age, education_num,
  capital_loss and hours_per_week as `dimension` (74, 16, 99
  and 96 distinct values). The two measure-roled numerics peak
  at 0.050 (capital_gain by income) and 0.022 (final_weight by
  native_country), genuinely below the bar. The gridded-measure
  misroling recorded on Bike Sharing here empties the account.
- **Demotion**: zero findings demoted. The calendar family is
  seeded from the time axis; this frame has none, so the family
  is empty, and no subject is scaffolding. The education
  encoding's restatements — both FDs, the (education,
  education_num) hotspot, the categorical-role finding on
  education_num, the two predictability findings at R² 0.98 and
  0.95 — stand at full score; the encoding family would need
  the same FD closure seeded from an exact bijection rather
  than from an axis.
- **Order**: the digest re-bands (account, completeness,
  relations, column structure, deviations). The duplicates
  finding leads; the five step discontinuities — the
  highest-scored and least interpretable findings on this
  unordered frame — move from the digest's first line to ranks
  45–49.

## Corpus state

With this entry: 34 registered datasets, 23 bundled / 11
test-only. Full corpus run on the main build
(`benchmarks/corpus/runs/20261104T013245Z.json`): **23 ok /
0 fail / 11 skipped** — before this addition: 22 ok / 0 fail /
11 skipped over 33. The entry itself passes 16/16 checks
(9 expectations, 7 documented cases).
