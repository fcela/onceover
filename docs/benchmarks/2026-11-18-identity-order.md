# Identities before predictability

Issue #374, step 4.

## Changes

- **Order.** The pairwise dependence screen and the exact identity scans (pairwise affine and arithmetic, and multi-term) now run before the predictability screen. Before, they ran after it. Neither depends on the screen's output.
- **Exclusion.** When the screen models a target, it withholds every column that an exact identity ties to that target. Identities that share a column are merged into one group, so an affine copy of a part (double_quantity = 2 × quantity) counts as a part too. The rule is symmetric: a part is not predicted from its total either.
- **Seeds.** The screen's holdout split, the missing-indicator screen, the cross-fit folds and the restricted refit were seeded from the target's column position. They are now seeded from the target's own values: an FNV-1a hash of its encoded bin histogram. Reordering or renaming columns no longer changes any model.

## Why

Bike sharing states `cnt = casual + registered`, then stated "cnt is predictable from registered (94% of gain), R² 0.94" and "registered is predictable from cnt". Those are the identity again. The statement a reader needs, what explains demand, never appeared. Now:

- cnt is predictable from hr (62% of gain), temp (11%), yr (9%), workingday (8%): holdout R² 0.81 with only these drivers.
- casual is predictable from hr (34%), workingday (24%), temp (17%), atemp (12%), hum (7%).

In the README example (`notebooks/onceover_analysis.ipynb`), `line_total = quantity · unit_price` is stated as an identity. The previous report also stated "line_total follows local linear regimes" and "line_total is predictable from double_quantity". Both restate the product, and neither is stated now. The notebook's Table 5 (local models of line_total) is removed. Table 4 now credits the profiler's identity finding; the old text said the library does not infer the multiplication.

Column-position seeding made results depend on column order. On the gate's reversal test, 191 stated facts changed when the columns were reversed; now 87 change. Seeding by column name instead would have made results depend on names (rename test: 114 changes). Seeding from values gives 0 changes under rename.

## Results

Full gate, seed 0, against step 3:

| metric | step 3 | step 4 |
|---|---:|---:|
| `invariance.reverse` | 191 | 87 |
| `invariance.rename` | 0 | 0 |
| `invariance.row_shuffle` | 99 | 106 |
| `invariance.noise` | 28 | 44 |
| `invariance.total` | 319 | 238 |
| `planted.found_full` | 58 | 59 |
| `planted.found_4k` | 56 | 57 |
| `reference.should_4k_default` | 30 | 31 |
| `permuted.false_*` | 39 / 22 / 17 / 10 | 39 / 22 / 17 / 10 |
| `reference.must_not_*` | 0 | 0 |
| `reference.must_*` | unchanged | unchanged |

### Accepted differences

- **Row shuffle and an added noise column move more facts.** Driver lists are the drivers above 5% of gain, and they shift when the model gets one more predictor or a different row sample. The new seeds draw a different realization of that pre-existing instability, so these two transforms move up while reversal and rename move down. Making driver attribution stable is a separate problem from this step.
- **bike `weighted.must_8k` 1 → 0.** In the weights-hinted run, the rush-hour concentration (still in the full report) leaves the 8k digest, because the new statements explaining cnt, registered and casual take that space.
- **`redundancy.finding_permille_default`** moves by 1–3 per mille on five datasets.

### Corpus

220/220 machine expectations pass. Three outcomes flipped with the seed change:

| dataset | statement | before | after |
|---|---|---|---|
| abalone | model failure on `rings` | found | missed |
| absenteeism-at-work | model failure on `absenteeism_time_hours` | missed | found |
| south-german-credit | `credit_risk` predictive | missed | found |

For abalone, under the new seed 0 of 90 slice × form candidates clear the search bars. These statements depend on which split the cross-fit draws. The registry notes record this as evidence of their fragility, not as a gain or a loss.

### Tests changed

- `tests/test_notebook_contract.py` reads the identity in place of the piecewise model, and asserts that no predictive statement targets line_total.
- `tests/test_profile_integrity.py` pivot fixture: b now differs from a on every 20th row. Two identical columns are one identity, whose rules restate it.
- `tests/test_report_access.py`: the budgeted JSON test's 9,000-character limit was 25 characters under the report's minimal skeleton, so it is now 10,000. The full JSON is about 34k characters, not the 19k its comment stated.
- `docs/book/usage/findings.ipynb`: the conditional-rule example ("double_quantity ≤ 2 implies quantity = 1") restated the affine identity. It is replaced by a subscriptions example with a rule between two columns that are related, not derived.

## Reproduce

```sh
source .venv/bin/activate && maturin develop --release
python benchmarks/gate/run.py --out head.json
python benchmarks/gate/compare.py base.json head.json
python benchmarks/corpus/run_benchmarks.py --json-out corpus.json
python scripts/check_notebook.py
bash docs/book/build-html.sh
```
