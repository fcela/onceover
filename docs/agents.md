# Agent guide

This file covers working *on* the repository. The root
`AGENTS.md` is the brief for *using* the library; keep the two in
sync when the public surface changes.

## What this project is

onceover profiles Polars DataFrames and reports ranked,
evidence-backed findings. All core computation lives in the Rust
workspace (`crates/`) so Python (`python/onceover/`, PyO3/maturin)
and any future host-language bindings (e.g. R) remain thin wrappers
over one implementation:

- `crates/onceover-core`: typed column extraction, streaming cached
  `RowKeys`, column cards, exact duplicate rows, elementary stats.
- `crates/onceover-deps`: keys, FDs, duplicate encodings, order
  constraints, boundary mass, sentinels, digits, content, pivot.
- `crates/onceover-mines`: predictability screen (GBM), dependence,
  level contrasts, breaks, ordered-axis battery, panels, surprises,
  hotspots, regimes, views, slices, partition scagnostics.
- `crates/onceover-report`: report schema (`0.8`), ledger selection
  and family grouping, Markdown rendering, briefing digest & compact
  brief (`digest.rs`), compact & size-budgeted JSON (`compact.rs`),
  and finding lookup/select/diff (`diff.rs`).
- `crates/onceover`: modular pipeline (`src/pipeline/`), frame model,
  panel coverage, multi-table profiling (`many.rs`), and frame spec /
  hierarchy discovery (`spec.rs`).
- `crates/onceover-py` & `python/onceover/`: thin PyO3 binding layer,
  Polars `LazyFrame`/file intake (`_source.py`), Polars expression /
  Great Expectations checks (`_checks.py`), optional Altair HTML
  rendering (`_html.py`), and CLI (`_cli.py`).

## Build and test

- Python: use an activated virtualenv, then
  `maturin develop --release` (the venv must be active — a bare
  `VIRTUAL_ENV=` prefix does not install the editable package).
  Then `python -m pytest tests/ -q`.
- Rust: `cargo test --workspace`, `cargo clippy --workspace`,
  `cargo fmt --check`.
- Notebooks committed to git are committed executed; the notebook
  check (`scripts/check_notebook.py`) fails otherwise.
- The book (`docs/book/`) is executable too, but neither the gate
  nor PR CI builds it: the Docs workflow runs only on pushes to
  `main`, so a broken page surfaces only after merge. A change
  that alters report output (schema version, the frame section,
  digest shape, finding text) must run
  `bash docs/book/build-html.sh` (strict, executed) before merge
  and update every page that renders the changed output.

## Benchmarks

Register every shipped profiling detector so callers can select it by
ID. Shared resources and subchecks belong to their parent detector.
Recommendation is a separate, evidence-based decision.

- Corpus: `benchmarks/corpus/` — run
  `python benchmarks/corpus/run_benchmarks.py` before and after
  detector changes. Registry expectations are the contract; a
  failing known-miss tripwire means a capability arrived — promote
  the expectation, do not delete it.
- Gate: `benchmarks/gate/` (README there). `compare.py` must pass
  against the base build, including the absolute limits in
  `benchmarks/gate/ceilings.json` for changes to the recommended set.
  A new on-request detector can enter the registry with documented
  evidence, targeted positive and negative controls, and coverage for
  its bounds. Choose controls for the claim it makes. Its registration
  must preserve the recommended findings.
  To promote a detector, benchmark the proposed recommended set against
  the current one, including false findings on shuffled data, useful
  findings retained in short digests, and runtime. Record what the new
  detector adds on real or planted data. A change that brings a metric
  below its ceiling lowers the ceiling in the same pull request.
- Every benchmark result gets a write-up in `docs/benchmarks/`
  (protocol, numbers, provenance). If it is not in the docs, it
  does not exist.
- Data license gate: bundled datasets must be compatible with the
  package's MIT license (public domain, CC BY 4.0). Anything else
  is test-only — attribution and expectations in the registry,
  data never committed. Every dataset carries an `ATTRIBUTION.md`
  and a `prepare.py`; preprocessing is decode/rename only, logged.

## Reports

- The JSON report is versioned (`schema_version` in the report);
  the machine-readable contract is `docs/report-schema.json`
  (validated against live reports in the test suite), with a
  human-readable inventory in `docs/report-schema.md`.
- Pull findings instead of rendering the whole report:
  `report.finding(id)` returns one finding by its stable id (or
  None); `report.select(category=, type=, column=, target=)`
  returns the findings matching every given filter, in report
  order. Every finding carries a `type` and a `facets` object; the
  (category, type) values and the facet vocabularies are listed in
  `docs/report-schema.md`, the per-type facet assignment in
  `docs/findings.md`, and the derivation table in
  `docs/taxonomy-v1-design.md`.
- `report.to_digest(chars)` renders a context-budgeted digest
  (a briefing in sections by reader question, one line per fact,
  with a stated omission count when the budget truncates). Reach
  for it when the full JSON will not fit
  the reading budget; the digest is an index, the JSON is the
  record. `report.to_json(max_chars=N)` renders a size-budgeted
  JSON view instead: findings collapse to an index, then column
  cards to names, then coverage details — with a `budget` record
  stating what was cut.
- Detector capabilities: `docs/capability-manifest.json`,
  regenerated by `scripts/capability_manifest.py` — rerun it when
  detectors change; do not edit the manifest by hand.
- The registry-owned blocks of `docs/findings.md` (the taxonomy
  index, the form list, the facet tables, the detector table) are
  generated by `scripts/findings_doc.py` from the registry and
  the taxonomy — rerun it when detectors, finding types, or
  forms change; do not edit the generated blocks by hand. The
  rest of the file is hand-maintained prose the generator
  preserves.
