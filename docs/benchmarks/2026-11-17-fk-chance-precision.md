# Stable foreign-key chance scores (#365)

Base: main `7d27ac8`, v0.1.6. Python 3.13, macOS arm64.

The integer-span chance model subtracts two log binomial coefficients.
Its former implementation subtracted large `lgamma` values within each
coefficient, losing the entire result for sparse, widely separated keys.
For `{0, 2**60}` on both sides of a weak-tier candidate, the old score
was 1.0; the model's exact probability is about 1.5046e-36.

The replacement uses symmetry, a log-product sum for k <= 128, and a
Stirling expansion rearranged around the binomial entropy above that
threshold. The latter uses additive terms and log1p instead of subtracting
nearly equal log-factorials. Work is bounded independently of the span.
Containment and candidate eligibility are unchanged.

## Protocol and results

`tests/test_profile_many.py` compares log coefficients against
`log(math.comb(n, k))` at 11 points, including k=128/129 across the
algorithm boundary, balanced coefficients, spans through 2**64 and
k through 10,000. Relative tolerance is 2e-14 (absolute 1e-12 near zero).
An end-to-end regression checks both sparse-key candidates against the
exact probability with relative tolerance 1e-13 and no absolute tolerance
that could accidentally accept zero.

- Targeted multi-table suite: **30 passed**.
- Full Python suite: **787 passed** (775 baseline plus 12 regressions).
- Strict executed book: **61 pages built**, static HTML produced.

Commands: `PYTHONPATH=python python -m pytest tests/test_profile_many.py -q`,
`PYTHONPATH=python python -m pytest tests/ -q`, and
`bash docs/book/build-html.sh`. The checkout used the baseline release
native extension; this change is entirely in the Python multi-table code.

The installed MyST engine (1.10.1) cannot serve the latest unpinned
book theme. The initial book run executed all pages but failed at theme
startup. The successful run temporarily selected MyST's recommended
compatible book theme 1.4.1 and restored `myst.yml` afterward. No theme
change is included in the PR. Network access was needed to fetch the theme.

This is a numerical correctness comparison, not a runtime benchmark.
The single-table corpus does not exercise `profile_many`; its baseline
220/220 machine expectations do not test this failure.
