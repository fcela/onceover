# Findings-document generation and the AGENTS.md reduction (#317, phase 4)

## Problem

Issue #317's last phase asks for the surface to shrink:
`docs/findings.md` (115,168 bytes on main) generated from the
registry, and the repo-root `AGENTS.md` reduced to the packaged
agent guide that already ships (`onceover.agent_guide()`, 3,987
characters, added under #295). The taxonomy application (#314)
already landed; what remained was generation and the reduction.

This change was prepared against an earlier main and rebased
twice as the phase-3 merges landed: first onto the main the
five merges produced (the per-detector audit's move of three
detectors to on-request, the order-basis and slice-outlier
changes), then onto the main the axis-gated-breaks merge
produced. The registry counts below are the current ones (29
of the 43 detectors in the default set; the axis merge did not
change membership), the audit's paragraph in the findings
document and the axis merge's along-axis sentence in
`AGENTS.md` are preserved, the generator was re-run against the
current registry on the final base, and the fact comparison
was re-run against the current file.

The risk in generating a reader-facing reference is silent loss:
the hand file mixes registry facts with explanation, worked
interpretation, and measured history. This change generates only
what the registry and the taxonomy own, keeps every other block
as hand-maintained prose the generator preserves verbatim, and
proves by an automated comparison that no stated fact about
detectors, types, or forms was lost.

## Inventory of docs/findings.md (main, 115,168 bytes)

| Section | Content | Disposition |
|---|---|---|
| Schema history and report contract (introduction) | prose: what a finding is, the 0.8 field list, the schema-change history | hand prose, kept |
| Categories and statement definitions | a definitions table mixing category-level rows with per-type rows for a minority of types; the taxonomy has no complete listing anywhere in the file | the definitions table stays hand prose; a generated taxonomy index (every category with every type under it) is added above it |
| Facets | two tables (the facet fields and their values; the facets of each type) plus prose | both tables generated; prose kept |
| Selection and ranking | prose: interestingness, demotion, consolidation | hand prose, kept |
| Public structure | prose: subject, specification, evidence | hand prose, kept |
| Report outputs | prose: the digest, budgeted JSON, HTML | hand prose, kept |
| Tested associations; Group contrast; Per-level contrasts; Slice shifts; Model failure slices | prose: what each target-relative statement family means and how to read it | hand prose, kept |
| Consolidation and selection | prose: statement classes, the caps, the hygiene reserve | hand prose, kept |
| Verified relationship families | prose: the correspondence advisory and its verifier | hand prose, kept |
| Declared expectations | prose: the expectations pass per hint key | hand prose, kept |
| Detectors and extensions | a usage snippet (`available_detectors()`, a `detectors=[...]` call); a paragraph stating the membership counts and the on-request list; per-detector deep dives (discontinuities, drift, duplicate encodings, row clusters); the extension API | the snippet, the deep dives, and the extension API stay hand prose; the counts and lists are generated as a detector table (categories, membership, trigger, requires, purpose), with the battery rationale and the trigger notes kept as hand prose around it |
| Migration from schema 0.1 | a historical field-mapping table | hand, kept (a record of a past schema, not a registry fact) |

Measured coverage of the hand file, against the taxonomy's 64
types: the definitions table gives per-type rows for 17 types;
the facets-by-type table assigns facets to 37 types; the
registered form vocabulary (49 forms) is stated nowhere as a
list; per-detector categories, requirements, and purposes live
only in the capability manifest. The strictly mechanical share
of the old file — the two facet tables and the membership facts —
is about 13 KB of its 115,168 bytes; the rest is explanation
that generation must not touch.

## What is generated, and from what

The taxonomy lived only in Rust (`Pattern::ALL`, the facet
assignment in `Facets::for_pattern`, the `FORMS` vocabulary) and
was not exposed to Python. The report crate now exports it:

- `onceover_report::taxonomy_json()` renders every pattern with
  its type, its derived category, and its facets, from the same
  lists validation enforces. The facets are the canonical
  evaluation the taxonomy's own type lists use; where a
  statement's facets vary with its record (support with a
  violations metric, scope on a one-column subject,
  conditionality on a conditioned specification), the export
  carries the varied value beside the canonical one. A
  report-crate test pins the export against the taxonomy it
  describes.
- `onceover.taxonomy()` exposes it in Python, beside
  `available_detectors()`.

`scripts/findings_doc.py` renders five blocks of
`docs/findings.md` from `onceover.taxonomy()` and
`onceover.available_detectors()`, between
`<!-- generated:NAME -->` markers:

- `taxonomy-index` — every category with every type under it.
- `forms` — the registered form vocabulary (form, type,
  definition).
- `facet-fields` — the facet fields and their values.
- `facets-by-type` — the facets of each finding type.
- `detectors` — the membership counts and a table of every
  detector: categories, membership, trigger, requires, purpose.

Everything outside the markers is preserved byte for byte; a
comment at the head of the file states the split. The one piece
of presentation the generator itself owns is the facet glosses
(the parenthetical readings in the facet-fields table) and the
phrasing that merges a type's varied facet values into its row;
a facet value with no gloss decision fails the render, so
extending the vocabulary forces the documentation decision. One
merge is a named override, not a rule: an order violation's
support renders `approximate`, because the statement exists only
where an order fails — the canonical evaluation reads `exact`
for the pattern only because support keys on the violations
metric, which the canonical (empty) record does not carry.

Freshness is enforced the same way as the capability manifest:
`test_findings_doc_is_fresh` in `tests/test_registration.py`
re-renders the committed file and fails on any difference, and
`scripts/findings_doc.py --check` reports the same from the
command line. The render also fails if any of the five blocks
is missing from the document, so deleting a marker cannot make
a stale file pass as fresh.

The rendered file is 129,608 bytes: 20,491 bytes of generated
blocks stating the complete taxonomy, forms, and detector facts
the old file omitted, and 109,117 bytes of hand-maintained prose
(down from 115,168). The file grows because its coverage
completes; the maintained surface — text a person must keep in
sync with the registry by hand — is what shrinks.

## Fact comparison

The fact-bearing tables of the old and new files were compared
mechanically before this change was published:

- Facet fields: all seven fields with identical value lists and
  glosses, cell for cell.
- Facets by type: every one of the 37 types the old table
  covered has identical facet values in the generated table
  (the generated table backticks the values, as the facet-fields
  table above it already did; the old table left them bare).
  The remaining 27 types, which the old table never assigned,
  are covered from the taxonomy.
- Detectors: the on-request set is identical (the same 14
  detectors), and the counts agree (29 of 43). The generated
  table's categories, requirements, and purposes are the
  registry's — the same values the capability manifest freezes.
- Definitions: all 17 per-type (category, type) pairs the hand
  definitions table states appear in the generated index; the
  index adds the other 47 types. The definitions table itself
  stays as hand prose, so its meanings are untouched.

No fact stated by the old file is absent from the new one.

## What generation and the inventory exposed

- The selection caps in `SelectionPolicy` are keyed to two
  detector ids the schema 0.8 rename retired (`gbm`, `views`),
  so neither cap can bind; the document states the predictability
  cap the code does not apply. Filed as issue #343 with the fix
  requirements. The caps prose in "Consolidation and selection"
  is hand text and is not corrected here: today it describes the
  policy map as coded for the pressure cap and as intended for
  the standing cap, so the prose correction belongs with the
  code fix in #343.
- The row-clusters paragraph in "Detectors and extensions" told
  readers to request the experimental `views` detector, an id
  that no longer exists; corrected to `row_clusters` in this
  change (the paragraph sits in the section this change
  restructures, and the statement it makes — excluded from the
  default selection, request it explicitly — is true of
  `row_clusters`).
- The 2026-11-13 selection-study sentences in "Consolidation
  and selection" name `views` as the study measured it, under
  the id the detector carried on that date; left as the
  historical record.

## AGENTS.md reduction

The repo-root `AGENTS.md` went from 13,459 to 7,942 bytes. What
the packaged guide already states was removed: the budget
definitions, the reading-the-report walkthrough (findings,
evidence, interestingness, coverage, source), the annotated
hints block (the guide states every hint key, the roles
vocabulary, and the cadence values), and the covered report
methods (`finding`, `select`, `to_digest`, plain `to_json`,
`findings_frame`, `verify`, `rows`). `AGENTS.md` now opens with
the quickstart and the pointer to `agent_guide()` and
`capabilities()`, and keeps only what has no equivalent in the
guide:

| Kept item | Why it has no guide equivalent |
|---|---|
| Deadline reach inside the five capped passes, with the coverage shares | the guide states that a deadline bounds wall time; it does not state which passes stop at internal quanta or how their coverage reads |
| Evidence payload examples (`mapping`, `segments`, `duplicate_groups`, `periods`) | the guide does not name the payload keys |
| Sampled-source reading rule | the guide states the source section's contents, not the reading rule for sampled numbers |
| Report contract pointer (`docs/report-schema.json`, `docs/report-schema.md`) | not named in the guide |
| Bare-string hints raise `ValueError` | not stated in the guide |
| Multiple tables (`profile_many`) | the guide covers the single-table report only |
| Command line | not covered by the guide |
| Report methods `to_html`, budgeted `to_json(max_chars)`, `associations()`, `diff()`, `to_checks()` | not covered by the guide |
| The caps remedy under "Empty and partial results" (raise `max_pairs=`, `max_predictors=`, `sample_rows=`, or narrow with `detectors=`) | the guide's closing note says to check coverage, not how to widen the examination |
| The detector-count statement and the skipped-in-coverage fact | the count statement is pinned by `test_detector_counts_in_prose_match_the_registry`, which reads this file; the guide carries the same counts but the test's surface is this file |
| The slice-outlier rarity rule (a default run states a `categorical_outlier` finding only when every breaking level is at or below 1% of its column; naming `outliers` states them) | added to `AGENTS.md` by the slice-outlier change after the guide shipped; the guide does not mention the `outliers` pass |
| The along-axis gating (the `breaks`, `ordered_partitions`, `ordered_axis`, and `discontinuities` passes scan only along the time axis or a declared `ordered_axis`; on a frame with neither, a default run records them skipped) | added to `AGENTS.md` by the axis-gated-breaks change after the guide shipped; the guide names the `ordered_axis` hint but does not state the gating |

`docs/agents.md` gains the parallel line for the findings
document, so its instruction to regenerate the manifest when
detectors change now covers both generated surfaces. The book's
taxonomy chapter gains one sentence where it points at the
tabulated assignment: the tables are generated from the taxonomy
the library exports as `onceover.taxonomy()`.

## Gate

Run in CI order on the final tree (current main plus this
change); every leg completed:

- `scripts/release.py check` passes (release metadata matches
  0.1.5) and the release tests pass (6).
- `cargo fmt --all -- --check` is clean.
- Clippy under the Rust 1.100 toolchain (`--workspace
  --all-targets --locked -D warnings`): exit 0.
- `cargo test --workspace --locked`: every suite passes,
  including the report crate's 69 tests and the
  `onceover-deps` library's 60. On this machine the run had to
  be split in two: in the combined run the `onceover-deps`
  test binary's link exceeded the disk quota (the linker died
  with the quota's signal-7 bus error, and a retry named the
  cause directly: "Disk quota exceeded"). With regenerable
  build artifacts freed, the workspace run excluding
  `onceover-deps` exits 0 and `cargo test -p onceover-deps
  --lib` passes on its own. No test failed in any run; the
  failures were the linker and the quota, not assertions.
- `python -m pytest tests/ -q`: 766 passed, including the
  registration tests (5) with `test_findings_doc_is_fresh` and
  the prose-count test over the reduced `AGENTS.md` and the
  generated findings document.
- Corpus runner: 218/218 machine-verified expectations pass;
  the runner exits 2 for the documented misses on record,
  which is its standing state on main.
- Digest gate (`benchmarks/gate/run.py`): exit 0;
  `check_failed` is 0 on every dataset, in the totals, and in
  the held-out totals.
- Strict book build (`docs/book/build-html.sh`, executed):
  exit 0.

This change alters no report output, so the corpus, gate, and
book results are main's own expectations met on the merged
tree.

This completes the phases of #317. The name freeze is a standing
rule from here, not a change in this record.
