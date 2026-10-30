# ADBench Classical: findings-to-labels scoring

Date: 2026-10-30. Issue #24's deliverable: a documented mapping
from onceover findings to ADBench's row labels, a run over a
rule-chosen subset, per-dataset counts broken out by the quirk
catalog, a macrOData sampling plan, and NAB's scoping. Harness:
`benchmarks/suites/adbench_scoring.py`.

## The mapping, and what it loses

ADBench scores row-level anomaly labels; onceover produces
findings. The mapping used here is deliberately strict:

> A labeled anomaly row counts as **detected** when some finding's
> evidence explicitly implicates that row — a metrics payload
> carrying a list of row indices (a key ending in `_rows` holding
> in-range integers). Segment and range findings do not count:
> crediting every row inside a segment would inflate detection
> beyond what the finding states. Implicated normal rows are
> reported alongside as the mapping's precision side.

What is lost in translation, measured rather than asserted: in
the default detector set exactly **one** mechanism produces
row-shaped evidence — the surprises pass's `extreme_rows` on
extreme-value findings. Every implicated row in the 37-dataset
run (63 rows total) came from it. The same profiles produce up
to 100 findings per dataset about dependence, concentration,
duplicates, and structure; none of them name rows, so none of
them can score under any row-level mapping. Detection under
this mapping therefore measures the profiler's row-implicating
surface, not its structural sensitivity, and the counts below
should be read that way.

## Subset rule

Every ADBench Classical dataset whose `.npz` in the suite
repository is at most 1 MB: **37 of 47**. The ten excluded by
size alone are ALOI, backdoor, celeba, census, cover, donors,
fraud, http, magic.gamma, and speech. (Fraud is profiled
separately as a corpus reference; census and donors are the
notable coverage gaps the rule accepts.) Datasets are test-only:
fetched and scored locally, never committed — suite membership
does not settle redistribution, as the datasets aggregate many
original sources. Profiles are the standard budget under the
default detector set with a 300 s per-dataset deadline; feature
matrices are cast to Float64 first (one suite file stores its
matrix as UInt8, which the profiler's ingestion rejects — filed
as issue #75; the cast is value-preserving).

## Results

37 datasets scored, 0 errored, 70,155 labeled anomalies,
**25 detected (0.04%)**, 38 implicated normal rows, total
profile time 933 s (median 2.1 s per dataset).

| dataset | rows | cols | anomalies | share | detected | impl. normal | dup rows | s |
|---|---|---|---|---|---|---|---|---|
| fault | 1,941 | 27 | 673 | 34.7% | 2 | 1 | 0 | 7.0 |
| glass | 214 | 7 | 9 | 4.2% | 0 | 3 | 1 | 0.1 |
| Hepatitis | 80 | 19 | 13 | 16.3% | 0 | 0 | 0 | 0.0 |
| InternetAds | 1,966 | 1,555 | 368 | 18.7% | 0 | 0 | 0 | 358.9 |
| Ionosphere | 351 | 32 | 126 | 35.9% | 0 | 0 | 1 | 1.9 |
| landsat | 6,435 | 36 | 1,333 | 20.7% | 0 | 0 | 0 | 28.2 |
| letter | 1,600 | 32 | 100 | 6.3% | 0 | 0 | 2 | 6.9 |
| Lymphography | 148 | 18 | 6 | 4.1% | 2 | 1 | 0 | 0.7 |
| mammography | 11,183 | 6 | 260 | 2.3% | 0 | 3 | 3,335 | 1.9 |
| mnist | 7,603 | 100 | 700 | 9.2% | 0 | 0 | 0 | 55.5 |
| musk | 3,062 | 166 | 97 | 3.2% | 0 | 0 | 18 | 108.2 |
| optdigits | 5,216 | 64 | 150 | 2.9% | 0 | 0 | 18 | 35.4 |
| PageBlocks | 5,393 | 10 | 510 | 9.5% | 5 | 0 | 0 | 5.3 |
| pendigits | 6,870 | 16 | 156 | 2.3% | 0 | 0 | 0 | 31.2 |
| Pima | 768 | 8 | 268 | 34.9% | 0 | 0 | 0 | 0.5 |
| annthyroid | 7,200 | 6 | 534 | 7.4% | 3 | 0 | 138 | 1.5 |
| satellite | 6,435 | 36 | 2,036 | 31.6% | 0 | 0 | 0 | 30.6 |
| satimage-2 | 5,803 | 36 | 71 | 1.2% | 0 | 0 | 2 | 27.1 |
| shuttle | 49,097 | 9 | 3,511 | 7.1% | 4 | 7 | 0 | 12.0 |
| skin | 245,057 | 3 | 50,859 | 20.8% | 0 | 0 | 193,624 | 0.7 |
| smtp | 95,156 | 3 | 30 | 0.03% | 0 | 1 | 23,926 | 0.5 |
| SpamBase | 4,207 | 57 | 1,679 | 39.9% | 7 | 6 | 0 | 24.7 |
| Stamps | 340 | 9 | 31 | 9.1% | 0 | 3 | 0 | 0.3 |
| thyroid | 3,772 | 6 | 93 | 2.5% | 2 | 2 | 116 | 0.8 |
| vertebral | 240 | 6 | 30 | 12.5% | 0 | 1 | 0 | 0.1 |
| vowels | 1,456 | 12 | 50 | 3.4% | 0 | 0 | 4 | 3.0 |
| Waveform | 3,443 | 21 | 100 | 2.9% | 0 | 0 | 0 | 28.4 |
| WBC | 223 | 9 | 10 | 4.5% | 0 | 0 | 0 | 0.2 |
| WDBC | 367 | 30 | 10 | 2.7% | 0 | 2 | 0 | 2.1 |
| Wilt | 4,819 | 5 | 257 | 5.3% | 0 | 3 | 0 | 0.6 |
| wine | 129 | 13 | 10 | 7.8% | 0 | 0 | 0 | 0.5 |
| WPBC | 198 | 33 | 47 | 23.7% | 0 | 2 | 0 | 1.0 |
| yeast | 1,484 | 8 | 507 | 34.2% | 0 | 1 | 31 | 0.5 |
| breastw | 683 | 9 | 239 | 35.0% | 0 | 0 | 234 | 0.5 |
| campaign | 41,188 | 62 | 4,640 | 11.3% | 0 | 0 | 12 | 121.9 |
| cardio | 1,831 | 21 | 176 | 9.6% | 0 | 0 | 9 | 16.0 |
| Cardiotocography | 2,114 | 21 | 466 | 22.0% | 0 | 2 | 4 | 18.7 |

## Quirk catalog breakout

Documented quirks (sourced): the Classical sets are repurposed
classification tables — the construction the DAMI critique
(Campos et al. 2016, doi:10.1007/s10618-015-0444-8) documents,
one class downsampled as the outlier class — and DAMI warns
explicitly that duplicates distort density-based detection,
shipping with/without-duplicate variants for that reason.
Measured per dataset in the table: anomaly share and exact
duplicate-row counts.

- **By anomaly share.** At most 5% (13 datasets, 1,042
  anomalies): 4 detected (0.38%). 5–15% (11 datasets, 10,499):
  12 (0.11%). Above 15% (13 datasets, 58,614): 9 (0.02%). The
  high-share band is a class-imbalance regime, not a
  needle-hunting one, and detection falls as share rises — the
  labeled class is a coherent population, so its members are
  rarely isolated extremes in any single column. Even the
  needle band barely registers: smtp's 30 anomalies in 95,156
  rows produced one implicated row, and it was normal.
- **Duplicates.** skin is 79% duplicate rows (193,624 of
  245,057), smtp 25%, mammography 30%, breastw 34% — detection
  is 0 in all four, consistent with DAMI's warning applying to
  the row-implication surface as well: duplicated normals make
  genuinely isolated rows rarer, not more visible.
- **Where detection does occur** it is the designed case:
  Lymphography (2 of 6 — a tiny table where the anomalous class
  does sit at isolated extremes), SpamBase (7 of 1,679),
  PageBlocks (5 of 510), shuttle (4 of 3,511).

## Runtime is part of the result

Cost tracks width, not rows: skin (245k rows, 3 cols) profiles
in 0.7 s while musk (166 cols) takes 108 s and InternetAds
(1,555 cols) 359 s — past its 300 s deadline, because the
deadline is checked between passes and the width-quadratic
dependence screen (issue #66) overruns inside one pass.
campaign (41k rows, 62 cols) takes 122 s on the discontinuities
screen's cost (issue #65).

## macrOData sampling plan

The macrOData suites (per issue #24: OddBench 790 datasets,
OvRBench 856, SynBench 800) cannot be run whole at the observed
costs — a single wide dataset can take six minutes. Plan: a
stratified random sample of 40 datasets per suite, fixed seed,
strata by column count (at most 16 / 17–64 / over 64) drawn in
proportion to each suite's width distribution, scored with this
same harness and mapping. Projected from this run's band
medians (narrow ~2 s, mid ~30 s, wide ~110–360 s): roughly
25–40 minutes per suite. SynBench goes first: its anomalies are
planted synthetics, the one suite design where row-level
detection is the intended target and the mapping's strictness
is a fair test. OddBench's semantic anomalies are the case the
mapping is expected to fail informatively, for the same
repurposing reason as the Classical sets.

## NAB scoping

NAB stays a follow-up, partially landed: two streams
(nab-nyc-taxi, nab-ec2-latency) are already in the corpus with
`anomaly_coverage` expectations — a window-tolerance variant of
this mapping (a labeled window counts as covered when a
changes/gaps/intervals finding lands within 24 h). Extending to
the full stream set is a data-conversion exercise on the same
terms, not new methodology.
