# Categorical distribution slices

The categorical comparison uses total variation over discovery level
shares. The levels whose slice shares exceed their complement shares
form the set that attains this gap. The detector ranks discovery
proposals by the squared gap times the product of the two group shares.
It checks the same level set on holdout and includes categorical and
numeric proposals in one Holm correction. [Total variation, Proposition 4.2](https://pages.uoregon.edu/dlevin/MARKOV/markovmixing.pdf)

## Protocol

The [distribution slices benchmark](../../benchmarks/distribution_slices.py)
runs each detector on the same fixtures. This run compared the numeric-only
and categorical-enabled builds of the on-request `distribution_slices`
detector with `slice_shifts`.

```sh
OPENBLAS_NUM_THREADS=1 VECLIB_MAXIMUM_THREADS=1 python benchmarks/distribution_slices.py --stage detectors
```

Each plant uses 16 seeds. A hit requires a reported rule whose row set has
Jaccard similarity of at least 0.75 with the planted categorical region.
For the binary plants the reported target event must include the positive
level, `yes`. The binary and eight-level plants use 3,200 rows. The
rare-level plant uses 12,000 rows to give the older detector enough
observations of the rare level for its support rules.

## Results

| Categorical plant | `distribution_slices` (numeric-only build) | `distribution_slices` (categorical-enabled build) | `slice_shifts` |
| --- | ---: | ---: | ---: |
| Binary shift from 8% to 55% | 0/16 | **16/16** | 16/16 |
| Eight-level mix, each of four levels rises from 12.5% to 18% | 0/16 | **16/16** | 0/16 |
| Rare level rises from 1.2% to 7% | 0/16 | 0/16 | **13/16** |

In the eight-level plant, the four increases amount to a 22 percentage
point combined shift. At the time of this run, `distribution_slices`
required a 12-point gap, so only `slice_shifts` recovered the rare-level
plant's 5.8-point gap.

The numeric cases recovered 159 of 160 planted regions before and after
the categorical change. Both versions of `distribution_slices` were
silent on all 64 shuffled categorical tables. The new build reported
one finding on one of 64 ordinary numeric nulls and one on one of 64
wider nulls, for a total of two null tables out of 192. The older build
reported two on the ordinary nulls and none on the wider nulls.
`slice_shifts` reported one finding on a wider null and one on a
categorical null.

All 61 corpus records matched before and after apart from timings.
The corpus retained its five existing expectation failures across four
datasets.

## Repeat on the final release build

The local onceover 0.1.9 release build was measured on arm64 macOS
with the same 208 planted tables and 192 shuffled controls. Both
detector orders produced identical recovery and finding counts.

```sh
OPENBLAS_NUM_THREADS=1 VECLIB_MAXIMUM_THREADS=1 python benchmarks/distribution_slices.py --stage detectors --detectors slice_shifts,distribution_slices
OPENBLAS_NUM_THREADS=1 VECLIB_MAXIMUM_THREADS=1 python benchmarks/distribution_slices.py --stage detectors --detectors distribution_slices,slice_shifts
```

| Measure | `distribution_slices` | `slice_shifts` |
| --- | ---: | ---: |
| Planted numeric regions recovered | 159/160 | 43/160 |
| Binary categorical regions recovered | 16/16 | 16/16 |
| Eight-level categorical regions recovered | 16/16 | 0/16 |
| Rare-level categorical regions recovered | 0/16 | 13/16 |
| Ordinary numeric null tables with a finding | 1/64 | 0/64 |
| Wider null tables with a finding | 1/64 | 1/64 |
| Categorical null tables with a finding | 0/64 | 1/64 |
| Numeric case runtime | 12.95 ms | 30.98 ms |
| Wider null runtime | 12.00 ms | 5.12 ms |

Runtime measures the full `profile()` call. The numeric figure is the
median of the ten numeric fixture medians from each of the two orders.
The wider null figure is the median of that fixture's two medians.

## Rare-level follow-up

The same detector-stage command on the local 0.1.9 release build now runs
224 planted tables and 256 shuffled controls. The added depletion case
uses 12,000 rows with a categorical level that occurs in 2.6% of the
remaining rows and disappears in the planted region. The added null
shuffles the response of the 12,000-row enrichment case.

| Measure | `distribution_slices` before | `distribution_slices` with rare-level search | `slice_shifts` |
| --- | ---: | ---: | ---: |
| Enrichment from 1.2% to 7% | 0/16 | 16/16 | 13/16 |
| Depletion from 2.6% to zero | 0/16 | 16/16 | 3/16 |
| Shuffled rare-level tables with a finding | 0/64 | 1/64 | 0/64 |
| All shuffled tables with a finding | 2/256 | 3/256 | 2/256 |
| Numeric regions recovered | 159/160 | 159/160 | 43/160 |

Both binary and eight-level categorical recovery stayed at 16/16 for
`distribution_slices`. The rare-level check advances at most four
extra proposals per categorical target, keeps the level and
direction fixed on holdout, and includes its p-values in the same
Holm family as all other slice proposals. All 61 corpus records
matched before and after apart from timings.
