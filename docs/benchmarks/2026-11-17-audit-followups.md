# Corpus regression after the audit follow-ups

2026-11-17. Branch `fix/audit-followups-2026-11-17`, based on
`34718c6` (v0.1.6). Working tree: the structural-correctness fixes
for budgeted JSON, deadline validation, U64 ordering axes,
null-only duplicate columns, coverage labels, HTML escaping, CLI
inference, and the Lanczos coefficient. No detector was added,
removed, or promoted; the registry expectations are unchanged.

## Protocol

Built the native extension in release mode with
`maturin develop --release` in the repo's virtualenv, then ran:

```sh
python benchmarks/corpus/run_benchmarks.py \
  --json-out benchmarks/results/audit-followups-2026-11-17.json
```

Python 3.13.7, macOS 26.3 arm64, package version 0.1.6. The harness
verifies the registry's pinned content hashes before profiling, uses
the standard budget and each entry's declared detector set, and
profiles only bundled datasets or test-only datasets whose prepared
file is present. Full run record:
`benchmarks/results/audit-followups-2026-11-17.json`.

## Results

- 32 datasets `ok`, 25 skipped (absent test-only data, unverifiable
  or non-redistributable sources, or suites not wired).
- Machine-verified expectations: **220/220 passed**. No dataset
  entered `expectations_failed` or `error`.
- Reviewed cases: 17 found, 11 missed, 4 partial. The misses are
  the same six known-miss tripwire datasets recorded in the prior
  audit: thyroid, adventureworks, sec-edgar-company-facts,
  metro-interstate-traffic, bike-sharing-hourly, and
  adult-census-income. They are documented absences, not new
  regressions. The harness therefore exits 2: machine checks are
  clean, but documented misses remain on record. No tripwire was
  deleted or promoted because no new capability arrived.

The numbers are unchanged from the audit baseline. The fixes alter
evidence values and coverage wording (for example corrected
categorical-tail significance and `prism_rules` instead of the
internal detector id `covering_rules`), not detector coverage or
expectation predicates.

## Supporting validation

- Python: `python -m pytest tests/ -q` — 824 passed.
- Rust: `cargo test --workspace` — all unit, integration, and
  doc-test targets passed (480 tests).
- Lint: `cargo clippy --workspace --all-targets --locked -- -D
  warnings` and `cargo fmt --all -- --check` — clean.
- Book: `bash docs/book/build-html.sh` — strict executed build,
  61 pages. The MyST site template is pinned to the
  `myst-to-react@1.4.1` theme archive because the moving
  `book-theme` alias now resolves to a release that the installed
  MyST 1.10.1 CLI cannot render.
