# Thread scaling on the large-table fixture

Profile time on the 2,000,000 × 200 fixture falls from 150 s at one thread to 35 s at
ten. Running the `surprises` pass in parallel by column accounts for most of the gain
over `main` above two threads, because it was the largest pass that did not scale.

## Protocol

Same host, macOS arm64, 24 GiB, 10 cores, Python 3.12.11, Polars 2.1.0, full release
builds, one fresh process per thread count and nothing else running. The fixture
matches `benchmarks/scale.py`. Times come from `profile(..., timing=True)`. Each cell is
one run, and repeated runs on this host have differed by several seconds.

## Results

Total wall time, seconds:

| Threads | `main` at `8321d14` | With parallel `surprises` |
|---:|---:|---:|
| 1 | 147.3 | 149.8 |
| 2 | 90.1 | 80.5 |
| 4 | 66.4 | 53.0 |
| 8 | 52.3 | 36.3 |
| 10 | 47.0 | 34.9 |

Per pass, with parallel `surprises`, seconds:

| Pass | 1 | 2 | 4 | 8 | 10 |
|---|---:|---:|---:|---:|---:|
| `discontinuities` | 48.0 | 25.2 | 15.7 | 9.8 | 9.1 |
| `boundaries` | 31.7 | 15.5 | 9.4 | 6.4 | 6.2 |
| `surprises` | 18.8 | 10.0 | 7.3 | 5.5 | 5.4 |
| `digits` | 17.9 | 9.3 | 5.7 | 3.8 | 3.7 |
| `column_cards` | 13.0 | 6.9 | 4.1 | 2.8 | 2.4 |
| `content` | 10.5 | 5.1 | 3.0 | 2.0 | 1.8 |
| `multiterm_identities` | 3.7 | 3.5 | 3.5 | 2.1 | 2.1 |
| `sortedness` | 1.7 | 1.9 | 1.9 | 1.9 | 1.9 |

On `main`, `surprises` took between 18.6 and 19.4 s at every thread count.

`benchmarks/scale.py` at four threads on this change: 56.96 s, peak process RSS
4.42 GiB, an increase of 1.35 GiB from pre-profile RSS, all eight recovery and
cardinality checks passed, and zero noise targets flagged
([record](../../benchmarks/results/2m-200-parallel-surprises-2026-11-05.json)). On `main`
the same run took 70.4 s with a peak of 5.03 GiB. Each is a single run, so the memory
difference may be noise, but this change did not raise the peak.

## Reports are unchanged

SHA-256 of `to_json()` over all 23 bundled corpus tables is identical before and after
at four threads, and identical at one and four threads after. Columns are scanned in
parallel, and the statement budget is applied afterwards in column order.

A subset scan now runs for every column, where the old loop stopped once the budget was
spent. That is more work on tables that reach the budget, though it runs in parallel.

## Running passes concurrently

Not done. Almost all time is inside the passes: their sum matches wall time at every
thread count, so the pipeline's own serial work is negligible. Passes already use all
cores, and most gain little from 8 to 10 threads, which suggests memory bandwidth
rather than core count is the limit. Running passes at once would compete for the same
resources, raise peak memory, and require restructuring a 1,500-line function whose
coverage entries depend on pass order.

## Regression tracking

`benchmarks/timing_gate.py --output timing.json` records wall time and per-pass times for
each case, and CI uploads the file as an artifact. The gate's own thresholds still catch
only large regressions, so a smaller slowdown has to be read from these records.
