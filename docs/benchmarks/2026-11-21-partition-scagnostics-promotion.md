# Partitioned scagnostics promotion trial

Partitioned scagnostics is available through `profile(df, detectors=["partition_scagnostics"])`. The trial below evaluated adding it to the recommended set. The promotion trial did not add any shuffled-data findings on the tested seed. It made report contents less stable under changes to the input layout and added several calendar and row-index relationships to short digests. Keep the search on request for now.

## Protocol

Run the full `benchmarks/gate/run.py` battery with seed 0 on three local release builds of onceover 0.1.7. The baseline was the worktree at `a0527ef` before the detector registration. The opt-in build registers `partition_scagnostics` as `OnRequest`. The promotion build uses the same source in a scratch copy with that one membership set to `DefaultSet`. This lets the trial use the regular profile path, including automatic target activation and default axis eligibility. All builds used Rust's release profile, Python 3.13.7, Polars 2.0.0, and NumPy 2.5.4 on arm64 macOS 26.3. The benchmark used the existing corpus inputs and registry. Gated totals cover 20 tables, with seven additional held-out tables reported separately.

```sh
python benchmarks/gate/run.py --out base.json --commit baseline
python benchmarks/gate/run.py --out opt-in.json --commit opt-in
python benchmarks/gate/run.py --out promotion.json --commit promotion-trial
python benchmarks/gate/compare.py base.json opt-in.json
python benchmarks/gate/compare.py base.json promotion.json
```

For a diagnostic audit of output the existing reference statements do not cover, run the promotion build's default profile on each original gate table. Count `partitioned_shape` findings in the full report and in `to_digest(4_000)`. Inspect their named columns and rules against the table's recorded context. The iris case is checked separately using the book fixture.

## Results

| Measure over gated tables | Baseline | Opt-in registration | Promotion trial |
| --- | ---: | ---: | ---: |
| False findings on shuffled data, full default report | 38 | 38 | 38 |
| False findings on shuffled data, 4k default digest | 21 | 21 | 21 |
| False findings on shuffled data, full target report | 16 | 16 | 16 |
| False findings on shuffled data, 4k target digest | 9 | 9 | 9 |
| Required reference findings, 4k default digest | 15 | 15 | 15 |
| Required reference findings, 4k target digest | 17 | 17 | 17 |
| Unwanted reference findings, full default report | 0 | 0 | 0 |
| Existing planted findings, 4k default digest | 59 | 59 | 59 |
| Facts lost or gained across layout transformations | 130 | 130 | 154 |
| Sum of median profile times | 9.148 s | 9.281 s | 9.307 s |

The opt-in registration passed the comparison. The default corpus run produced the same 228 of 228 machine-verified expectations, with the same documented misses and skips as the baseline. The promotion comparison failed on layout sensitivity and reduced digest space for finding lines. Row shuffling accounted for 13 additional changed facts, adding an independent column accounted for nine, and reversing columns accounted for two. The detector samples by row position and considers a capped set of axes and descriptors by name. Those choices can change the tested rows or hypotheses under benign transformations. All tested precision ceilings were met. Runtime stayed within the comparison tolerance.

The promotion build surfaced 32 partition findings in full recommended profiles on the 20 gated tables, with nine visible in 4,000-character digests. Capital Bikeshare findings included `atemp × mnth` split on `season`, and `atemp × instant` split on `mnth`. The calendar variables encode related aspects of the same calendar, and `instant` is a row counter. These are legitimate score gains that explain less than the separate workday shape question in the case study. The trial also recovered the setosa partition on iris, with held-out Spearman ρ of −0.17 pooled and +0.76 and +0.57 within groups. That finding appeared in the full ordinary profile. This audit characterizes what the added results say. The existing reference catalog does not independently score their usefulness.

On-request registration depends on controls relevant to the new detector and on preserving the recommended profile. A promotion decision needs a direct comparison like this trial, including a review of new real-data output. The current trial gives a concrete reason to keep scagnostics on request even though the independently shuffled controls showed no additional false findings. Use content-keyed sampling and avoid row identifiers and derived context as a first improvement before testing promotion again.
