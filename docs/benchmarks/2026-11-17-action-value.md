# Action value of the gate reference statements

This document measures the value of the actions that the digest gate's 89
labeled reference statements enable, under the per-family proxies recorded
on issue #347, and tests whether action value separates the gate's must
tier from its should tier. It is the follow-up to
`docs/benchmarks/2026-11-17-interestingness-study.md`, whose classification
of the 89 statements by action family is the starting point here.

What it found: action value is computable for 76 of the 89 statements. On
the population the earlier measures were computed on, action value does
not separate must from should. The pooled AUC is 0.500, the value of
chance; the within-dataset AUC is 0.545, inside the band the failed
compression baseline already occupies (0.511 on the same statements, 0.552
as recorded on all matched statements). On the wider set of all 76 priced
statements the pooled AUC is 0.493 and the within-dataset AUC is 0.478,
both at or below chance. The median action value of a must statement is
0.105 and of a should statement 0.091. The cornerstone of the
interestingness study — that the value of the enabled actions is what
distinguishes a must statement from a should statement — is falsified in
its quantitative form: measured this way, it separates no better than the
two measures that already failed.

Base: `main` at `cec0278c1574bddf276707c76cca344f0dab18e1`. The gate
datasets' data files are byte-identical (sha256) to the study's base, so
every statement, label, and matching result carries over unchanged. All
computation is from the datasets themselves, in Polars; no library code is
used or changed. Scripts: `values.py` (the per-family computation),
`separation.py` (the separation tests and correlations), `load.py`
(dataset loading through the corpus and gate registries), kept with this
measurement's working papers alongside the study's scripts.

## 1. The question and the baselines

The study established two negative results. Compression gain per
character — how much a statement compresses the table, priced per
character it costs to print — separates must from should weakly: AUC
0.610 pooled, 0.552 within datasets. Membership in an analysis-changing
action class does not separate at all: 68% of must statements and 67% of
should statements are analysis-changing. The direction recorded on issue
#347 after those results: actionability is not a class property, because
every statement licenses some action; the research object is the *value*
of the enabled actions — what goes wrong, or what is lost, if the
statement is ignored. This document computes that value for each
statement and runs the same separation test a third time.

## 2. Method

### 2.1 Population and labels

The 89 gate reference statements over 15 datasets, with the study's
family classification (its Appendix C) as the label of record: specify
29, identify 16, partition 13, simplify 13, escalate 9, admit or refuse
5, re-express 1, action-free 3. Tiers: 28 must, 61 should. The primary
test population is the 65 statements matched in some report scope (21
must, 44 should), the population both baselines were computed on;
statements whose value cannot be priced are excluded from the value
ranking and counted, and the compression baseline is recomputed on the
identical priced subset so the comparison is cell for cell.

### 2.2 The proxy for each family

Each proxy is the one recorded on issue #347, made concrete. Where a
proxy needs a quantity the statement does not name, the quantity is
derived from the statement's check expression; the derivation is stated
with the proxy.

- **Admit or refuse — the distortion prevented.** The refused object
  would have entered the mean of its column. The value is the shift in
  that mean between admitting the object as data and refusing it:
  naive mean over all rows against the mean with the code rows recoded
  and excluded (`capital_gain` with the 99999 rows removed;
  `days_since_previous_campaign_contact` with the −1 rows removed;
  parsed `amount` with the `-999` rows removed; `income_reported` with
  the −999 rows removed). The missing-month statement refuses a period,
  not a value: October 2025 is null for all 51 states, and ignoring the
  statement means a row-position monthly difference across the gap
  reports a two-month movement as one month's. Its value is the share of
  that gap-spanning movement attributable to the hidden month, from the
  mean absolute September→November 2025 change and the mean absolute
  one-month change over the panel. The October values are unobservable,
  so this prices the misreading, not a recoverable error.
- **Partition — the heterogeneity the pooled number conceals.** For the
  quantity the statement partitions and the segments its check defines,
  the value is η²: the mass-weighted between-segment variance of the
  quantity as a share of its total variance. The partitioned quantity is
  the dataset's outcome where the check is about a cell
  (`subscribed_term_deposit` for the February recency cell, approval for
  the source-system split), the concentrated column where the statement
  is a concentration (`cnt`, `payout`), the series level for time splits
  (`sessions`, `visits`), `log10|value|` for the concept split in
  sec-edgar-company-facts (the statement is that concept sets the scale,
  so the quantity is priced on the scale axis), and
  |visits − median(visits)| for the variance break, whose segments differ
  in spread rather than level. The covid-spike segments are the
  contiguous months from April 2020 whose cross-state mean exceeds 1.5×
  the 2019 mean (April 2020 to April 2021 on this data).
- **Specify — the held-out gain from the directed bend.** Five-fold
  cross-validation (seed 0, folds assigned by a fixed permutation). The
  model that follows the statement adds the directed term to an
  intercept-only model; the model that ignores it does not. Terms are
  group means for categorical predictors, quantile-binned group means
  (10 bins, edges from the training fold) for numeric predictors,
  additive on the logit scale for binary outcomes with Laplace-smoothed
  group rates, and additive group means with backfitting for numeric
  outcomes. Loss is mean squared error for numeric outcomes and log loss
  for binary outcomes. The value is the relative loss reduction,
  (loss without − loss with) / loss without. For the three null
  statements (noise-independent, low-mid-independent,
  inner-wide-independent) the direction is to omit the term, and the
  value is the proportional held-out worsening the omission avoids. For
  state-comovement the baseline carries state means and the term adds
  the binned national-rate effect on the residuals. The one two-term
  statements (income-age-hours, subscription-month-contact) add both
  named terms.
- **Simplify — the description removed at zero loss.** Column
  description cost: n × empirical entropy for discrete-valued columns,
  Gaussian code (n/2)·log2(2πeσ²) for continuous columns, nulls counted
  as a symbol. Dropping a column that is exactly determined by others
  removes its full cost (`cnt`, `total_cost`, `fee`). Dropping one of a
  near-collinear numeric pair removes (n/2)·log2(σ²_dropped /
  σ²_residual), maximized over the two directions. Pooling levels
  removes the cost difference between the column as-is and the column
  with the pooled levels merged. The share form divides by the table's
  total description cost D(T), the sum over its columns.
- **Identify — the mass double-counted.** Two-encodings statements: the
  shared information counted twice, n × H of the encoded variable
  (`education` in adult-census-income; `geo_accession` in
  gse15258-expression, which is in bijection with `sample_id`), as a
  share of D(T). The non-unique filing key: the excess rows, n −
  distinct keys, as a share of rows — the rows a uniqueness assumption
  would misattribute or multiply. Key and axis certifications and the
  marginal band relations are unpriced; see section 3.
- **Re-express — the value of what becomes computable.** The single
  re-express statement (das28-na) unlocks exactly one downstream
  statement in the gate set, das28-improvement, whose specify value is
  computable only after the parse. The re-expression's value is that
  statement's value.
- **Escalate — the divergence between the analyses under each
  resolution.** For each statement a headline quantity and the two
  resolutions its check implies. Missingness statements: the affected
  segment's quantity against the rest (income rate for the
  jointly-missing `occupation` rows; mean `price` by `hoa_fee`
  presence; mean parsed `amount` by `notes` presence; mean
  log10|value| by `fiscal_period` presence, log scale because concept
  sets the scale of `value`). Validity statements: the quantity with
  the flagged rows kept against excluded (mean `whole_weight`; mean
  `height`), and for the spike day, June 2023's mean `visits` as-is
  against the spike day replaced by the month's median. The native
  value is the divergence in the quantity's own units; the share form
  standardizes it by the quantity's spread (its standard deviation;
  for the spike day, the standard deviation of the monthly-mean
  series). This family's share is an effect size, not a fraction of a
  whole — the one family where the fraction form does not apply,
  because the two resolutions differ in an estimate, not in a mass.
  Two statements price at exactly 0: the balance-sheet and
  filed-after-period-end regularities have no exceptions on this data,
  so no analysis of the table takes two different values under the two
  resolutions. The proxy prices realized divergence, not the insurance
  value of a validation rule.
- **Action-free — 0 by definition.** The taxonomy licenses no action
  for the three action-free statements, so there is no action value to
  measure.

### 2.3 Normalization

Values are reported first in native units per family (section 4). The
one normalization used for the cross-family test is the share form
defined with each proxy above: distortion as a fraction of the corrected
mean, concealed heterogeneity as a fraction of total variance, held-out
gain as a fraction of the intercept-only loss, removed or double-counted
description as a fraction of the table's total description, excess rows
as a fraction of rows, and for escalate the standardized divergence.
Each share is the value as a fraction of the corresponding whole-table
quantity for its family. The within-dataset test additionally converts
each statement's share to a percentile within its own dataset, which is
the comparison briefing selection actually faces and does not depend on
cross-family currency at all.

### 2.4 The test

Tier separation is measured as in the study: AUC of must above should
(P(must > should) + ½P(tie)), pooled and over within-dataset
percentiles, and median ranks by tier in the ranking by the measure.
Replication check: on the 65 matched statements the compression
baseline's pooled AUC and median ranks reproduce exactly (0.610; must
23, should 37.5). Its within-dataset AUC computes to 0.557 under the
percentile convention stated here against 0.552 as recorded — a
percentile-convention difference of 0.005 that changes no comparison
below; all within-dataset figures in this document use the one
convention.

## 3. Coverage

76 of 89 statements are priced. The 13 unpriced statements are all in
the identify family:

- Ten key and axis certifications: `instant` in bike-sharing-hourly,
  the row keys of column_order_stress, dominant_driver, marginal_ranges,
  messy_intake and planted_mix, `sample_id` in gse15258-expression, the
  complete daily series of daily_level_collapse and
  daily_regime_break, and the complete state × month grid of
  state-unemployment-long. The double-counted mass is not defined for
  them: nothing in the table is counted twice while the certification is
  ignored. The double-counting a wrong key assumption causes arises in
  joins or merges with other tables, which a single-table statement set
  does not contain.
- The three marginal band relations (low-mid-disjoint,
  mid-high-disjoint, inner-nested): the bands are separate measurements
  whose ranges are disjoint or nested; no information in the table is
  carried twice.

The coverage hole is concentrated: 10 of the 11 matched identify
statements are unpriced, so the value ranking on the matched population
covers 55 of 65 statements (21 must, 34 should — every matched must
statement is priced). Seven priced statements have value exactly 0: the
three action-free statements, the two exception-free escalate
regularities, the constant `disease_state` column (a constant costs 0
bits per row under the empirical code; its removal saves computation,
not description), and the missing month (section 4).

## 4. Values in native units

**Admit or refuse.** The two large distortions are the two sentinel
codes. Admitting the 99999 pile in `capital_gain` as amounts moves the
column mean from 582.41 to 1,079.07, a distortion of 0.853; the 244 code
rows are 46.3% of the column's total. Admitting −1 in
`days_since_previous_campaign_contact` moves its mean from 224.58 to
40.20, a distortion of 0.821 over 36,954 code rows. The planted −999 in
`income_reported` distorts its mean by 0.080 and the messy-intake −999
in `amount` by 0.044. The missing month prices at 0: the mean absolute
September→November 2025 change is 0.071 percentage points against a mean
absolute one-month change of 0.110 over the panel, so none of the
gap-spanning movement is attributable to the hidden month beyond a
normal month's movement — the gap spans a quiet period, and the
misreading it invites is smaller than the table's ordinary monthly
noise.

**Partition.** Median η² is 0.102. The largest value in the family is
the planted hotspot: the kiosk × age 18–24 cell's mean `payout` is
896.46 against 99.77 for the rest, η² = 0.929 — the cell holds 24.3% of
total payout on 3.45% of rows, and payout varies little within each
segment. The collapse window in daily_level_collapse holds η² = 0.691
of `sessions`; concept holds 0.471 of log10|value|. At the bottom, the
February recency cell in bank-marketing prices at 0.0004: its
subscription rate is 6.1% against 11.8% for the rest, but the cell is
1.2% of rows, and the mass-weighted heterogeneity it represents is
negligible. The covid window prices at 0.018: thirteen months at a
cross-state mean of 7.68 against 5.65 elsewhere is a large level shift,
but the window is 663 of 31,059 state-months, and fifty years of
within-state variation dominate the variance it is priced against.

**Specify.** Median relative held-out loss reduction is 0.192. The
largest values are near-deterministic relations in the synthetic
tables: `a3` on `a1` in column_order_stress 0.944, `conversions` on
`sessions` 0.907, `signups` on `visits` 0.894, `price` on `size_sqft`
0.778, das28-improvement by `response_to_therapy` 0.674 (mean squared
error 1.885 without the term, 0.615 with it, on 75 rows). The three
null statements price at 0.006, 0.003 and 0.001 — the foreclosed terms
genuinely do not help held out, and following the statements avoids a
worsening of that size. `age_years` on `price` prices at 0.006, the
weak term its statement says it is.

**Simplify.** Median share of the table description removed is 0.021.
In bits: dropping `fee` in messy_intake removes 26,754.0 bits (0.230 of
that table's 116,137.4-bit description; `fee` is 2% of `amount` to the
cent on every parseable row); dropping `cnt` in bike-sharing-hourly
removes 154,397.6 bits (0.105); dropping `total_cost` in planted_mix
removes 55,007.6 bits (0.126). The near-collinear pair removals are
smaller: `length`/`diameter` in abalone 0.042 of the table, `temp`/
`atemp` 0.032. Pooling `native_country` below United-States removes
0.008 — the column is already cheap, because 91.35% of its mass is one
level before pooling.

**Identify.** The education encoding double-counts 143,139.9 bits
(0.061 of adult-census-income's description): every row's education is
carried twice. The sec-edgar filing key leaves 2,162 excess rows of
11,543 (0.187) beyond the first per key. The geo accession restates the
sample in gse15258-expression for 552.6 bits — a negligible share
(0.0002) of a 5,006-column table's description, although it is the
whole identity of every one of the 86 samples.

**Escalate.** Median standardized divergence is 0.006. The two largest
are the spike day (June 2023's mean `visits` is 947.4 as-is against
865.7 with the day smoothed, 0.417 of the monthly-mean standard
deviation of 196.1) and the jointly-missing `occupation` rows (income
rate 9.4% on the affected rows against 24.8% on the rest, 0.361 of the
outcome's standard deviation). `fiscal_period` presence separates mean
log10|value| by 0.269 standard deviations. The remaining divergences
are small: whether `hoa_fee` is missing separates mean `price` by 0.021
standard deviations, and the abalone validity questions move their
column means by 0.006 and 0.002.

**Re-express.** das28-na takes the value of the statement it unlocks:
0.674, the specify value of das28-improvement.

## 5. The separation test

| Measure | Population | AUC pooled | AUC within-dataset | Median rank, must / should |
| --- | --- | --- | --- | --- |
| Compression gain per character | 65 matched | 0.610 | 0.552 recorded (0.557 here) | 23 / 37.5 of 65 |
| Compression gain per character | 55 priced matched | 0.524 | 0.511 | 23 / 30 of 55 |
| Action class (analysis-changing share) | 89 | — (68% of must, 67% of should) | — | — |
| Action value (share form) | 55 priced matched | 0.500 | 0.545 | 26 / 28.5 of 55 |
| Action value (share form) | 76 priced | 0.493 | 0.478 | 39 / 38 of 76 |

Action value does not separate the tiers. Pooled over the comparable
population it is exactly chance: a randomly chosen must statement is as
likely to be worth less than a randomly chosen should statement as
more. Within datasets it reaches 0.545 — above compression on the same
55 statements (0.511) and below compression on the full matched
population (0.552–0.557); on the wider priced set it falls below chance
on both views. No reading of the table supports value as the quantity
the tiers track.

Per family (pooled AUC, priced matched / all priced; families with
fewer than two statements in a tier have no AUC): simplify 0.778 /
0.818, on two must statements (`cnt`-identity and `size-collinear`)
against nine to eleven should statements — the one family above chance,
on the smallest must count in the test. Specify 0.435 / 0.535.
Partition 0.500 / 0.455. Escalate 0.312 / 0.425 — the direction
reverses: the family's two largest values (the spike day, the
jointly-missing `occupation` rows) are should statements, and its must
statements include the zero-priced balance-sheet rule. Admit or refuse,
identify, re-express and action-free have at most one statement in one
tier on the matched population and cannot be tested there; on all
priced statements admit or refuse has four musts against one should,
still too few to test.

## 6. Correlation with compression gain

Over the 55 priced matched statements, the Spearman correlation
between action value and compression gain per character is 0.249
(p = 0.066) — weak, and short of significance. Per family the picture
splits by mechanism. Simplify correlates at 0.917: both measures price
description, and for the exact relations they coincide numerically
(the `cnt`-identity's removed description, 154,397.6 bits, is its
compression gain by construction). Specify correlates at 0.758: both
measures grow with the strength of the association the statement
reports, one as held-out loss reduction and the other as
log-likelihood ratio. Partition (−0.012) and escalate (−0.146) are
uncorrelated with gain: the heterogeneity a pooled number conceals and
the divergence between two resolutions are not what compression
measures. So value is not a re-measurement of gain in general; the two
agree where they share a mechanism and are independent elsewhere —
which makes the joint failure informative rather than redundant. Two
quantities that mostly measure different things both fail to track the
tiers.

## 7. The ranking's top and bottom

By action value over the 55 priced matched statements:

Top ten: `a3-negative` in column_order_stress (0.944, specify,
should); `conversions-track` in daily_level_collapse (0.907, specify,
should); `signups-track` in daily_regime_break (0.894, specify,
should); `capital-gain-99999` in adult-census-income (0.853, admit or
refuse, should); `days-since-sentinel` in bank-marketing (0.821, admit
or refuse, must); `size-dominates` in dominant_driver (0.778, specify,
must); `collapse-sustained` in daily_level_collapse (0.691, partition,
should); `labor-total` in planted_mix (0.672, specify, should);
`cnt-hour` in bike-sharing-hourly (0.500, specify, must);
`concept-scale` in sec-edgar-company-facts (0.471, partition, must).

Bottom ten: `native-country-concentration` (0.008, simplify, should);
`height-extreme` in abalone (0.006, escalate, should); `age-weak` in
dominant_driver (0.006, specify, should); `foreign-worker-concentration`
in south-german-credit (0.005, simplify, should); `notes-blank` in
messy_intake (0.005, escalate, must); `components-exceed-whole` in
abalone (0.002, escalate, should); `previous-contact-february-concentration`
in bank-marketing (0.0004, partition, should); `disease-constant` in
gse15258-expression (0, simplify, should); `missing-month` in
state-unemployment-long (0, admit or refuse, must);
`balance-sheet-never-negative` in sec-edgar-company-facts (0,
escalate, must).

As a sanity read the ranking is defensible at both ends: the top is
dominated by near-deterministic relations and by the two sentinel
distortions, and the bottom holds the statements whose questions never
bite on their own data. What it does not do is sort by tier: six of
the top ten and seven of the bottom ten are should statements, and the
single largest distortion in the gate (`capital_gain`) belongs to a
should statement while its bank-marketing counterpart of the same size
is a must.

## 8. What this means for the lexicographic rule

The study's surviving selection rule orders statements lexicographically:
admit-or-refuse statements that qualify an object another statement uses
come first; analysis-changing statements (partition, specify, simplify)
fill greedily by marginal gain per character; escalate statements follow
by the same efficiency; bookkeeping renders as account content;
action-free statements never render.

Action value leaves that rule unchanged. It does not replace the
within-class ordering: pooled, it separates the tiers at chance, so an
ordering by value has no measured claim to track what the gate's
reference statements reward. It does not refine the ordering either.
Within the two families where value and gain can be compared as
orderings, value would reproduce gain's ordering (Spearman 0.917 in
simplify, 0.758 in specify); within the families where the two orderings
differ (partition, escalate), value tracks the tiers no better than gain
does (partition AUC 0.500, escalate 0.312). The concrete statement for a
later implementation: make no change — steps 2 and 3 of the rule keep
marginal gain per character as the within-class ordering key, and action
value is not adopted as an ordering key at any step.

The falsification is of value as a *selection signal*. It is not a
finding that the values are wrong as measurements: the distortion the
99999 pile causes, the payout heterogeneity, and the held-out gain of
the response term are real quantities, computable from the data, and
they are the quantities an analyst ignoring the statements would lose.
What the test shows is that the gate's must tier is not the set of
statements with the largest such quantities. The tiers appear to encode
something the three failed measures do not — plausibly what a briefing
must not omit for a reader to trust and correctly read the rest of it,
which is a property of the briefing as a document rather than of the
statement's consequences for an analysis. That reading is consistent
with the study's displaced-statement result and is stated here as the
open question the next measurement would have to price, not as a
conclusion of this one.

## 9. The second factor: linkage to a business action

The values measured here are the first of two factors. The direction
recorded on issue #347 alongside this measurement: the value that
ultimately counts is a statement's linkage to a business action, and
only data actions are measurable from the data, so the eventual ranking
quantity is the measured data-action value multiplied by a
business-linkage weight carried by a structured catalog of actions —
default weights per data action with their reasoning stated,
overridable at runtime through the declared-metadata channel, and
conditionable on declared purpose. That catalog is the layer that
follows this measurement; it is not attempted here.

The per-family magnitudes are part of the evidence its default weights
will be set from. Median share-form values over the priced statements:
specify 0.192, partition 0.102, admit or refuse 0.080, identify 0.061,
simplify 0.021, escalate 0.006, re-express 0.674 on a single statement.
A catalog that weights families by their typical measured consequence
would start from roughly this ordering of the data-action factor. One
caution follows from section 5: because the first factor does not track
the must/should tiers, tier separation cannot be used to calibrate or
validate the catalog's weights. The weights will have to stand on the
stated reasoning per action and be checked some other way — for
example, where a weight is itself measurable one level down, as the
held-out gain of a specify action already is here.

## 10. Limitations

- The escalate proxy prices realized divergence on the statement's own
  data. A validation rule whose question never arises on its table
  (the two sec-edgar regularities) and a refusal whose gap spans a
  quiet period (the missing month) price at 0 even though the
  statements protect against configurations the data happens not to
  contain. Insurance value is not measured anywhere in this document.
- The partition proxy is mass-weighted by the definition recorded on
  the issue. It prices a regime by its row mass: the covid window's
  η² of 0.018 and the February cell's 0.0004 follow from the
  definition, and a definition that priced level shift alone would
  rank both higher.
- The identify coverage hole (section 3) means the value ranking is
  silent on most key and axis statements. If those certifications
  carry value, it is of a kind that materializes across tables and is
  outside every proxy defined here.
- Specify values are relative to the uniform model forms of section
  2.2. A richer directed model would price some terms higher; the
  forms were kept uniform so that values compare across statements,
  and the null statements' near-zero values show the forms are not
  finding gains that are not there.
- The admit-or-refuse proxy prices distortion of the column mean. A
  refused object can distort other quantities (a trend, a regression
  coefficient) by more or less; the mean is the quantity every refused
  object in the set would have entered.

## Appendix A. All 89 statements

Native units are per family as defined in section 2.2: admit or refuse,
the absolute shift in the column mean (for missing-month, the mean
absolute gap-spanning change in percentage points); partition, the
mass-weighted standard deviation of segment means in the quantity's
units; specify and re-express, the relative held-out loss reduction
(the share form itself); simplify and the two-encodings identify
statements, description bits; filing-key-not-unique, excess rows;
escalate, the divergence in the headline quantity's units. Matched
scopes are the report scopes in which the study's matching found the
statement.

| Statement | Tier | Family | Matched | Native value | Share |
| --- | --- | --- | --- | --- | --- |
| abalone|components-exceed-whole | should | escalate | 8k,full | 0.0008 | 0.0017 |
| abalone|height-extreme | should | escalate | 4k,8k,full | 0.0003 | 0.0062 |
| abalone|rings-sex | must | specify | 4k,8k,full | 0.1924 | 0.1924 |
| abalone|rings-size | must | specify | 4k,8k,full | 0.3993 | 0.3993 |
| abalone|size-collinear | must | simplify | 4k,8k,full | 10,973.3 | 0.0420 |
| adult-census-income|capital-gain-99999 | should | admit_refuse | 4k,8k,full | 496.7 | 0.8528 |
| adult-census-income|education-encoding | must | identify | 4k,8k,full | 143,139.9 | 0.0614 |
| adult-census-income|income-age-hours | should | specify | 4k,8k,full | 0.1494 | 0.1494 |
| adult-census-income|income-capital-gain | must | specify | 4k,8k,full | 0.0538 | 0.0538 |
| adult-census-income|income-education | must | specify | 4k,8k,full | 0.1153 | 0.1153 |
| adult-census-income|income-marital | must | specify | 4k,8k,full | 0.1976 | 0.1976 |
| adult-census-income|native-country-concentration | should | simplify | 4k,8k,full | 18,915.5 | 0.0081 |
| adult-census-income|occupation-workclass-missing | should | escalate | 8k,full | 0.1538 | 0.3605 |
| bank-marketing|days-since-sentinel | must | admit_refuse | 4k,8k,full | 184.4 | 0.8210 |
| bank-marketing|previous-contact-february-concentration | should | partition | 8k,full | 0.0062 | 0.0004 |
| bank-marketing|subscription-duration | must | specify | 4k,8k,full | 0.1907 | 0.1907 |
| bank-marketing|subscription-housing-loan | should | specify | 8k | 0.0265 | 0.0265 |
| bank-marketing|subscription-month-contact | should | specify | full | 0.0779 | 0.0779 |
| bank-marketing|subscription-previous | should | specify | 8k,full | 0.0813 | 0.0813 |
| bike-sharing-hourly|cnt-hour | must | specify | 4k,8k,full | 0.4997 | 0.4997 |
| bike-sharing-hourly|cnt-identity | must | simplify | 4k,8k,full | 154,397.6 | 0.1052 |
| bike-sharing-hourly|cnt-rush-hour-concentration | should | partition | - | 94.0189 | 0.2687 |
| bike-sharing-hourly|cnt-temperature | should | specify | - | 0.1690 | 0.1690 |
| bike-sharing-hourly|cnt-year | should | partition | 8k,full | 45.4354 | 0.0627 |
| bike-sharing-hourly|instant-index | should | identify | 4k,8k,full | unpriced | - |
| bike-sharing-hourly|temp-atemp | should | simplify | 4k,8k,full | 46,496.2 | 0.0317 |
| column_order_stress|a-collinearity | should | simplify | 4k,8k,full | 16,617.5 | 0.0231 |
| column_order_stress|a3-negative | should | specify | 4k,8k,full | 0.9436 | 0.9436 |
| column_order_stress|b-collinearity | should | simplify | 4k,8k,full | 12,729.7 | 0.0177 |
| column_order_stress|key | should | identify | 4k,8k,full | unpriced | - |
| column_order_stress|mix1-relates | should | specify | 4k,8k,full | 0.4599 | 0.4599 |
| column_order_stress|noise-independent | should | specify | - | 0.0025 | 0.0025 |
| daily_level_collapse|collapse-2023 | should | partition | 4k,8k,full | 66.3316 | 0.0680 |
| daily_level_collapse|collapse-sustained | should | partition | 4k,8k,full | 211.4 | 0.6912 |
| daily_level_collapse|complete-series | should | identify | full | unpriced | - |
| daily_level_collapse|conversions-track | should | specify | 4k,8k,full | 0.9071 | 0.9071 |
| daily_level_collapse|no-missing | must | action_free | - | 0.0000 | 0.0000 |
| daily_level_collapse|recovery | should | partition | 4k,8k,full | 80.5762 | 0.1004 |
| daily_level_collapse|weekly-cycle | should | specify | 4k,8k,full | 0.2405 | 0.2405 |
| daily_regime_break|complete-series | should | identify | full | unpriced | - |
| daily_regime_break|level-shift-2024 | should | partition | 4k,8k,full | 190.9 | 0.4345 |
| daily_regime_break|no-missing | must | action_free | - | 0.0000 | 0.0000 |
| daily_regime_break|signups-track | should | specify | 4k,8k,full | 0.8939 | 0.8939 |
| daily_regime_break|spike-day | should | escalate | 4k,8k,full | 81.7200 | 0.4167 |
| daily_regime_break|variance-break-2022 | should | partition | 4k,8k,full | 64.0182 | 0.1021 |
| daily_regime_break|weekly-cycle | should | specify | 4k,8k,full | 0.2287 | 0.2287 |
| dominant_driver|age-weak | should | specify | 4k,8k,full | 0.0062 | 0.0062 |
| dominant_driver|hoa-missingness | must | escalate | 4k,8k,full | 4,378.7 | 0.0212 |
| dominant_driver|key | should | identify | 4k,8k,full | unpriced | - |
| dominant_driver|rooms-collinear | should | simplify | 4k,8k,full | 7,402.4 | 0.0205 |
| dominant_driver|size-dominates | must | specify | 4k,8k,full | 0.7779 | 0.7779 |
| dominant_driver|suburb-concentration | should | simplify | - | 7,717.9 | 0.0213 |
| gse15258-expression|das28-improvement | must | specify | - | 0.6737 | 0.6737 |
| gse15258-expression|das28-na | must | reexpress | - | 0.6737 | 0.6737 |
| gse15258-expression|disease-constant | should | simplify | 4k,8k,full | 0.0000 | 0.0000 |
| gse15258-expression|geo-restates-sample | must | identify | - | 552.7 | 0.0002 |
| gse15258-expression|probe-1556053-response | must | specify | - | 0.0955 | 0.0955 |
| gse15258-expression|sample-key | should | identify | 4k,8k,full | unpriced | - |
| marginal_ranges|inner-nested | should | identify | - | unpriced | - |
| marginal_ranges|inner-wide-independent | should | specify | - | 0.0006 | 0.0006 |
| marginal_ranges|key | should | identify | 4k,8k,full | unpriced | - |
| marginal_ranges|level-spread | should | action_free | - | 0.0000 | 0.0000 |
| marginal_ranges|low-mid-disjoint | should | identify | - | unpriced | - |
| marginal_ranges|low-mid-independent | should | specify | - | 0.0056 | 0.0056 |
| marginal_ranges|mid-high-disjoint | should | identify | - | unpriced | - |
| messy_intake|amount-sentinel | must | admit_refuse | - | 1,984.1 | 0.0440 |
| messy_intake|code-flip | should | partition | - | 0.1738 | 0.1217 |
| messy_intake|fee-is-2pct | should | simplify | 4k,8k,full | 26,754.0 | 0.2304 |
| messy_intake|key | should | identify | 4k,8k,full | unpriced | - |
| messy_intake|notes-blank | must | escalate | 4k,8k,full | 120.0 | 0.0045 |
| messy_intake|web-majority | should | simplify | - | 1,095.5 | 0.0094 |
| planted_mix|affine-identity | should | simplify | 4k,8k,full | 55,007.6 | 0.1259 |
| planted_mix|channel-driver | must | specify | full | 0.0715 | 0.0715 |
| planted_mix|followup-missingness | must | partition | 4k,8k,full | 0.0517 | 0.0109 |
| planted_mix|hotspot-payout | should | partition | - | 145.4 | 0.9295 |
| planted_mix|key | should | identify | 4k,8k,full | unpriced | - |
| planted_mix|labor-total | should | specify | 4k,8k,full | 0.6724 | 0.6724 |
| planted_mix|sentinel-subpopulation | must | admit_refuse | 4k,8k,full | 5,797.1 | 0.0804 |
| sec-edgar-company-facts|balance-sheet-never-negative | must | escalate | 4k,8k,full | 0.0000 | 0.0000 |
| sec-edgar-company-facts|concept-scale | must | partition | 4k,8k,full | 0.6563 | 0.4707 |
| sec-edgar-company-facts|filed-after-period-end | should | escalate | - | 0.0000 | 0.0000 |
| sec-edgar-company-facts|filing-key-not-unique | should | identify | - | 2,162.0 | 0.1873 |
| sec-edgar-company-facts|fiscal-period-missing | must | escalate | 4k,8k,full | 0.2575 | 0.2692 |
| south-german-credit|foreign-worker-concentration | should | simplify | 4k,8k,full | 228.4 | 0.0052 |
| state-unemployment-long|complete-grid | should | identify | 4k,8k,full | unpriced | - |
| state-unemployment-long|covid-spike | should | partition | 4k,8k,full | 0.2937 | 0.0182 |
| state-unemployment-long|missing-month | must | admit_refuse | 4k,8k,full | 0.0706 | 0.0000 |
| state-unemployment-long|state-comovement | should | specify | - | 0.6551 | 0.6551 |
| state-unemployment-long|state-spread | should | specify | - | 0.2266 | 0.2266 |
