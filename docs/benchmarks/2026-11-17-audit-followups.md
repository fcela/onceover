# Corpus regression after the audit follow-ups

2026-11-17, refreshed after merging `origin/main` at `3889755`.
Branch `fix/audit-followups-2026-11-17` (v0.1.6) carries the
structural-correctness fixes for budgeted JSON, deadline validation,
U64 ordering axes, null-only duplicate columns, coverage labels,
HTML escaping, CLI inference, and the Lanczos coefficient. The audit
fixes add, remove, and promote no detector. The merge brought the
already-reviewed pointed-mode/outcome-contrast work and upstream
registry/gate updates; those are not part of this fix set.

## Protocol

After merging `origin/main`, rebuilt the native extension in release
mode with `maturin develop --release` in the repo's virtualenv, then
ran:

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
  deleted or promoted on this branch; the upstream expectation
  updates in `main` are part of that merge, not this fix set.

The tallies are unchanged from the audit baseline, but the
post-merge record differs in per-dataset finding counts and category
mixes: `main` advanced with pointed-mode/outcome-contrast work and
its registry updates. The audit fixes alter evidence values and
coverage wording (for example corrected categorical-tail
significance and `prism_rules` instead of the internal detector id
`covering_rules`); no registry expectation was edited on this
branch.

## Supporting validation

- Python: `python -m pytest tests/ -q` — 838 passed.
- Rust: `cargo test --workspace` — all unit, integration, and
  doc-test targets passed (490 tests).
- Lint: `cargo clippy --workspace --all-targets --locked -- -D
  warnings` and `cargo fmt --all -- --check` — clean.
- Book: `myst clean --execute --templates --yes` followed by
  `bash docs/book/build-html.sh` — strict executed build, 62 pages,
  including the new `outcome_contrasts` page from `main`. The MyST
  site template is pinned to the `myst-to-react@1.4.1` theme archive
  because the moving `book-theme` alias now resolves to a release
  that the installed MyST 1.10.1 CLI cannot render.
