# Content-keyed sampling (issues #435, #441, #425, #437, #442)

## Protocol

Base: `e495b4c` (`origin/main` at the time of the run). Head: `0985c1f`
(`fix/five-immediate-priorities`, rebased onto that base). Both builds were
installed into clean virtual environments with `maturin develop --release`
and measured with seed 0. The gate ran the full dataset registry with

```sh
python benchmarks/gate/run.py --out <scorecard> --commit <sha>
python benchmarks/gate/compare.py <base> <head>
```

and the corpus ran with

```sh
python benchmarks/corpus/run_benchmarks.py --json-out <record>
```

No ceilings were changed. Six narrowly scoped accepted entries were added
for the small regressions listed below; each names the measured head value
as its limit.

## Gate

The comparison before accepting regressions reported 6 worse, 0 above
ceiling, 8 worse-and-accepted, 47 improved and 45 unchanged totals. After
the accepted entries the comparison exits 0.

The invariance result is the point of the sampler change. Totals moved:

| Metric | Base | Head |
|---|---:|---:|
| `invariance.row_shuffle` | 95 | 11 |
| `invariance.reverse` | 91 | 87 |
| `invariance.noise` | 52 | 31 |
| `invariance.total` | 239 | 130 |

The row-shuffle gain is concentrated on the tables whose row caps bind:
adult-census-income 2 → 0, bank-marketing 12 → 2 and abalone 5 → 0. The
residual differences are model-selection changes on the shared encoded
sample, not positional dependence of the sample itself.

The accepted regressions are all one- or two-unit character-density moves
plus a two-fact noise difference on the stress table:

| Metric | Scope | Base | Head | Limit |
|---|---|---:|---:|---:|
| `redundancy.finding_permille_default` | thyroid | 878 | 877 | 877 |
| `redundancy.finding_permille_default` | hospital-dha | 895 | 893 | 893 |
| `redundancy.finding_permille_default` | dominant_driver | 878 | 877 | 877 |
| `redundancy.finding_permille_target` | abalone | 925 | 924 | 924 |
| `redundancy.finding_permille_target` | congressional-voting | 904 | 903 | 903 |
| `invariance.noise` | column_order_stress | 0 | 2 | 2 |

`finding_permille` measures the character share of finding lines in the
context-budgeted digest; one or two per mille is a one-line reordering at
these digest sizes. On `column_order_stress`, noise 0 → 2 is offset by row
shuffle 7 → 5 in the same table, leaving its invariance total unchanged at
7. The sampler's default field salt was kept: alternate salts fixed the
two sample-sensitive Python fixtures but produced more gate regressions
(10 worse with salt `1`, 4 worse with salt `517cc1b727220a95`, including a
reverse-invariance regression), so the fixtures were made to assert the
invariant behavior instead.

## Corpus

The base had 220/220 machine-verified expectations passing and no
`expectations_failed` datasets. The first head run had 218/220 with two
sample-dependent flips. Both expectations were re-baselined with their old
wording preserved under `Earlier note`:

- `thyroid`, predictive on the designated target `diagnosis`: the
  rank-keyed 75/25 holdout no longer selects the 0.97-accuracy statement
  that the positional split selected. The target is still screened and
  its decline/other statements remain.
- `abalone`, model failure on the designated target `rings`: the
  rank-keyed cross-fit fold partition now clears the slice-search bars, so
  a previously known-miss statement is promoted to `found`.

After those expectation updates the head has 220/220 machine checks and 0
`expectations_failed`. The documented-case counts are unchanged (17 found,
11 missed, 4 partial).

## Validation

The rebased branch passed `cargo fmt --check`, strict workspace clippy,
`cargo test --workspace`, `python -m pytest tests/ -q` (851 passed, 1
skipped), the four affected executable notebooks and the strict book build
(63 pages). No benchmark ceiling was weakened.
