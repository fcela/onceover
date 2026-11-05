# Parallel boundaries and digits passes

On the 2,000,000 × 200 fixture, the `boundaries` and `digits` passes went from
49 s to about 17 s at four threads, and total profile time went from about 100 s
to about 68 s. Peak process memory rose by about 0.5 GiB. Reports are unchanged.

## What changed

Both passes looped over columns on one thread. Each now handles columns in
parallel and collects results in column order. `boundaries` also returns early
when no value in a column repeats at least 10 times, before sorting the column.
On continuous columns that sort produced nothing.

## Protocol

Same host, macOS arm64, 24 GiB, Python 3.12.11, Polars 2.1.0, four Rayon threads,
full release builds, nothing else running. The fixture matches
`benchmarks/scale.py` with seed 20241024. Per-pass times come from
`profile(..., timing=True)`.

## Results

| Pass | `270b122`, 2 runs | Before this change, 2 runs | After, 2 runs |
|---|---:|---:|---:|
| `boundaries` | 33.8, 33.6 | 33.6, 33.8 | 10.4, 11.9 |
| `digits` | 15.4, 15.4 | 15.3, 15.3 | 6.3, 7.2 |
| `surprises` | 18.6, 18.4 | 18.4, 21.5 | 19.4, 19.4 |
| `discontinuities` | 17.5, 17.5 | 18.9, 19.9 | 17.9, 20.1 |
| Total wall time | 93.2, 93.1 | 97.1, 103.3 | 65.1, 71.3 |

"Before this change" is `270b122` with the identity, false-alarm and missingness
branches merged locally, which is close to the base of this change. Run-to-run
differences of several seconds are visible, so two runs establish the size of the
change but not a precise figure.

`benchmarks/scale.py` on this change: 73.14 s, all eight recovery and cardinality
checks passed, zero noise targets flagged
([record](../../benchmarks/results/2m-200-parallel-2026-11-05.json)).

| Memory | Before | After |
|---|---:|---:|
| Peak process RSS | 3.57 GiB | 4.08 GiB |
| Increase from pre-profile RSS | 0.50 GiB | 1.00 GiB |

The increase follows from holding several columns' value vectors at once, one per
active thread. Each RSS figure is from a single run.

## Reports are unchanged

SHA-256 of `to_json()` over all 23 bundled corpus tables is identical before and
after at four threads, and identical at one and four threads after.

## Remaining serial cost

`surprises` is now the largest serial pass, at 19 s of about 68 s. `boundaries`
still hashes every value of every column; a faster hash or an early exit on a
sample could reduce it further. See issue #137.
