# Semantics application: vocabulary renames, one category path, registered forms (2026-11-15)

Applies the semantics review (issue #314) on top of the single-point
registration change. Report schema 0.7 becomes 0.8: the schema's own
history bumps the minor number for every contract revision (0.3 the
facets, 0.6 grouped findings, 0.7 the associations table), and the
renames change the report JSON, so they take the next number. No
statement changes meaning; every rename below is a rename of a name,
verified by the corpus run at the end of this record.

## The rename mapping

Roles and cards:

- Column card field `role` -> `card_role`; its value `identifier` ->
  `unique`.
- Semantic role `index` -> `row_number` (frame model, hints `roles`,
  coverage details that print role names).

Hints:

- `axis` -> `ordered_axis`, `ordinal` -> `level_order`. The old keys
  are accepted for one release; a report built with one states the
  rename in a `hints` coverage entry.
- Every one of the 13 keys now counts in the hints emptiness check;
  before, an axis-only or ordinal-only declaration read as "no hints
  provided" in the expectations coverage entry.

Activation, split into two fields in `available_detectors()`, the
registry, and the capability manifest:

- `membership`: `default_set` (32 detectors) / `on_request` (11).
- `trigger`: `always` (38) / `hints` (2: expectations, group_contrast)
  / `target` (3: target_shapes, leakage, model_failures).
- The old classes map as Default -> (default_set, always),
  OptIn -> (on_request, always), HintGated -> (default_set, hints),
  TargetGated -> (on_request, target).

Detectors:

- `gbm` -> `predictability`, `prism` -> `covering_rules`,
  `changeforest` -> `distribution_change`, `views` -> `row_clusters`,
  `content` -> `content_anomalies`, `boundaries` -> `boundary_mass`,
  `surprises` -> `outliers`, `digits` -> `digit_fingerprints`,
  `dates` -> `date_gaps`.
- Evidence method names follow the detector id where the method is
  the pass (the distribution-change pass stamped the old name, which
  the explicit-selection retain filter reads; the algorithm names in
  method descriptions are unchanged).
- Pass coverage keys follow the detector id, with one exception kept:
  `predictability` still reports as `predictability_screen`.
  `covering_rules` takes its own id (the old `prism_rules` key named
  the algorithm).

Taxonomy:

- Category `intervals` -> `anomalies`.
- Type `functional` splits by what the statement says:
  `functional` (value mappings), `affine` (pairwise affine fits),
  `arithmetic_identity` (multi-term identities and decompositions).
- `residual_level` -> `residual_shift`, `distribution_shift` ->
  `predictor_drift`, `distribution_change` (type) ->
  `joint_distribution_change`.
- Facet field `evidence` -> `support`.
- Types `unexamined_columns` and `unscanned_series` are removed from
  the findings. Both described the examination rather than the data;
  what a pass did not examine stays in its coverage entry (the
  pipeline names cap-dropped columns, the row-clusters coverage
  names the columns outside its search and extraction, the breaks
  coverage names prescored-but-unscanned series with prescores).

## One category path

`Pattern::category()` is deleted. A finding's category is derived
from its facets at construction, and validation checks the stated
category against the same derivation. Before the pattern-level path
was removed, the two paths were compared on all 65 patterns with
canonical fixtures (directed patterns exercised with a named target,
as production findings always carry one) and agreed everywhere; the
report test suite now pins the derived category for every pattern in
an explicit table, so a facet change that moves a category fails a
test that names the pattern.

## The form registry

Statement `form` values are now a registered vocabulary in the
report crate: each form is listed with the finding type it belongs
to and a one-line definition, validation rejects a finding whose
form is not registered for its type, and a source scan in the report
tests checks the form literals the producers assign against the
registry. Two consequences worth recording:

- A plugin (`from_statement`) finding carrying a novel form is now
  dropped at selection, with the drop recorded in coverage, where
  before it passed through. Registering the form is the fix.
- The scan has to know each producer's assignment shape. The slice
  pass computes its form in an if/else over an internal candidate
  kind; the first version of the scan missed that shape and the
  registry briefly carried `tail_share` — a metric name — where the
  code assigns `tail_shift`. The full Python gate caught the dropped
  findings; the scan now covers the if/else shape, and the registry
  carries `tail_shift`.

## Corpus verification

Registry expectations naming renamed detectors, categories, or
types were renamed, not re-baselined. The one judgement call: the
three taiwanese-bankruptcy expectations written against `functional`
describe affine relations (their notes state the fitted equations)
and are now `affine`; bike-sharing's `cnt` expectation describes the
sum identity and is now `arithmetic_identity`; the other 16
`functional` expectations still describe value mappings and are
unchanged. No expectation referenced the two removed types, so none
was deleted.

Result on the changed tree, `benchmarks/corpus/run_benchmarks.py`:

- machine-verified expectations: 95/95 passed (base: 95/95)
- documented cases: found 17, missed 11, partial 4 (base: identical)

No verdict moved.

## Gate

In CI order on the changed tree (base counts from the registration
change's verification on the same base):

- `python scripts/release.py check` — passes ("Release metadata matches 0.1.5"); `pytest scripts/test_release.py -q` — 6 passed.
- `cargo fmt --all -- --check` — clean (six touched files needed
  wrapping).
- `cargo clippy --workspace --all-targets --locked -- -D warnings` —
  clean.
- `cargo test --workspace --locked --release` — 451 passed, 0 failed
  (base 448; +3 net new tests: the per-pattern derivation table, the
  form-registry checks, the hint-emptiness test).
- `python -m pytest tests/ -q` — 693 passed, 0 failed (base 683;
  +10 net new tests, including the schema-0.8 vocabulary tests and
  the row-clusters cap tests that replace the views cap tests).
- Corpus — above.
