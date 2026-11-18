# The frame spec: first step

Issue #449.

## What it does

`onceover.infer_spec(df, hints=None, target=None)` returns one document holding the frame's resolved interpretation, without profiling the table:

- every column's role, with its `source` and `evidence`;
- the time axis (with its parts when assembled from parts);
- the entity;
- the target and the weights.

It runs the frame model with only the keys and duplicate-encodings passes (which the grain and the recodings need), at the quick budget: 0.02 s on bike sharing.

`profile(df, spec=...)` runs on the document. It compares each entry with what inference resolves on its own:
- an entry that matches is left to inference, so an unedited spec reproduces the plain run exactly (verified on bike sharing, Adult and thyroid: identical finding ids);
- an entry that differs becomes a declaration (a role, `time`, `entity`, `weights`);
- because declaring one role can move another column's inferred role, the comparison repeats until every role the run resolves is the spec's, or raises.

A spec that does not describe the frame raises `ValueError`. That covers missing or extra columns, an unknown role, a wrong `spec_version`, an axis or entity that is not a column, and a target that conflicts with `target=`. Hint keys the spec carries (`roles`, `time`, `entity`, `weights`) are not also accepted as hints.

## Not yet

Left for later steps of #449:
- a report section showing the resolved spec whole;
- one coverage account of the inferences, alongside `declarations`;
- making the derived-column decisions (calendar parts, sub-periods, recodings, missingness indicators) editable entries of their own. Today they appear as roles with their evidence.

Inference does not choose a target or weights; the spec states them as declared or none.

## Validation

`tests/test_frame_spec.py`: 10 tests covering the round trip, edits held, the target and every refusal. The full suites pass. Default runs are unchanged (a spec is opt-in), so the gate is not affected.
