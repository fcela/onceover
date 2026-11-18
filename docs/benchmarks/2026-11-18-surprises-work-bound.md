# Surprises work bound

Issue #437. The categorical half of `outliers` now builds each
column's item bitsets once and shares them across targets, and the
context search stops at a deterministic work budget. A stop marks
only the `outliers` coverage entry `capped`, states the work spent
and the contexts and targets examined, and never suppresses later
passes.

## Protocol

Synthetic pure-noise categorical frames, 20,000 rows × 400 columns,
one independent level draw per column with 5–49 levels, seed 437,
`RAYON_NUM_THREADS=4`, standard tier, release build on
macOS-26.3-arm64, Python 3.13.7, Polars 1.38.1, onceover 0.1.6.
The head is branch `fix/437-surprises-bound` at `7cc00d4` plus this
change. Timings are wall-clock from one run, not medians.

| Run | Head | Baseline from issue #437 |
|---|---:|---:|
| `outliers` only | 3.3 s | 367.6 s |
| Whole profile | 49.0 s | 544.9 s |
| `outliers` findings | 0 | 0 |

The baseline times were measured on main `114fff67` with the
issue's generator; this is not a same-commit A/B. The head run's
coverage entry reports `work budget bound: 299999651 of 300000000
work units spent, 486740 contexts evaluated, 7 of 251 categorical
targets fully screened, 1 stopped mid-scan, 0 not started; the
examined contexts are a prefix of the deterministic enumeration,
so the negative is partial`. The whole profile still reported 12
findings from other passes.

## Verification

- Rust: `cargo fmt --check`, `cargo test --workspace --locked`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`.
  The new tests cover a binding work cap and an ordinary frame
  whose findings and coverage are unchanged with a generous budget.
- Python: `python -m pytest tests/test_surprises.py
  tests/test_deadline.py -q` — 24 passed.
- Gate files: `benchmarks/gate/ceilings.json` and
  `benchmarks/gate/accepted.json` are unchanged. No gate metric
  reads coverage detail; on uncapped frames the findings, digest
  and report schema are unchanged, and the new text appears only
  when the pass is capped.

## Reproduce

```sh
source .venv/bin/activate && maturin develop --release
python - <<'PY'
import os, time
os.environ["RAYON_NUM_THREADS"] = "4"
import numpy as np, polars as pl, onceover
rng = np.random.default_rng(437)
data = {}
for i in range(400):
    levels = int(rng.integers(5, 50))
    data[f"cat_{i:03d}"] = rng.integers(0, levels, 20_000).astype(str)
df = pl.DataFrame(data)
for detectors in (None, ["outliers"]):
    kwargs = {} if detectors is None else {"detectors": detectors}
    started = time.perf_counter()
    report = onceover.profile(df, budget="standard", **kwargs)
    print(detectors, round(time.perf_counter() - started, 1))
    print(report.coverage["outliers"])
PY
```
