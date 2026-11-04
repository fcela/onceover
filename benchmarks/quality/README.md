# Discovery-quality battery

A corpus of real tables shows what the profiler says about data
whose truth is only partly documented. This battery measures the
complementary quantity: on synthetic frames whose ground truth is
known by construction, how often does the profiler report the
planted signal, where does the report rank it, and how often does
it report signals that are not there?

## Running

    python benchmarks/quality/battery.py --fast
    python benchmarks/quality/battery.py --full --commit <sha> \
        --out benchmarks/quality/results/<date>-full.json

`--fast` uses seeds 0–1 and is the configuration the CI subset
(`tests/test_quality_battery.py`) samples from. `--full` uses
seeds 0–11 for the base plants and every variant. Every run writes a
machine-readable JSON record with provenance: library version,
the commit passed with `--commit`, package versions, platform,
and the full configuration.

## Plants

Each plant (plants.py) builds a frame with one planted signal plus
independent noise columns, and its defining property is re-verified
with NumPy on the generated frame before any profiling — the truth
never comes from the library under test.

- `affine_identity` — y = 3x + 2 exactly (dependencies)
- `categorical_fd` — label is an exact function of code (dependencies)
- `weak_association` — bivariate normal, rho = 0.15 (dependence)
- `association_moderate` — bivariate normal, rho = 0.4 (dependence)
- `nonlinear_only` — y = x² on symmetric x; linear correlation ≈ 0 (dependence)
- `interaction_only` — y marks sign(x1·x2); both marginals uninformative (interactions)
- `conditional_extremes` — six extreme values inside one slice (surprises)
- `rare_slice_outlier` — three exception rows inside one slice (surprises)
- `level_shift` — a +2.5σ level shift in a daily series (breaks)

Variants: the response column permuted (the signal is destroyed;
the match rate must collapse), 5% MCAR missingness in the signal
columns, column order reversed, and a wide frame with the signal
beyond column 64. Null frames contain independent columns only:
any family-matching finding there is a false positive by
construction.

## Metrics

Per plant and variant, over seeds (mean, minimum, n):

- **detector recall** — a profile restricted to the family's
  detectors reports a finding whose type belongs to the family and
  whose subject involves all the planted signal columns;
- **post-selection recall** — the same test on the default
  profile's selected findings;
- **top-10 / top-25 recall** — the first matching finding's rank
  when selected findings are ordered by the report's
  `interestingness` score, descending (the findings list itself is
  not in rank order; the digest ranks by this score);
- **target recall** (secondary) — a family-type finding reports
  the planted response column in any context: distinguishes "the
  signal's values were reported under another grouping" from
  "nothing about the signal was reported";
- **stray family findings** — family-type findings on a planted
  frame that involve no signal column (they can only concern noise
  columns).

Unknown claims on real data are not scored here at all: the
battery's frames are the ones whose truth is defined. Real-data
claims the corpus cannot verify stay "unreviewed" in the corpus
accounting, not false positives.

Baseline numbers and the regression tolerances proposed from them
are recorded in `docs/benchmarks/2026-11-04-quality-battery.md`
and in the checked-in run record under `results/`.
