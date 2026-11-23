# Detector access and recommendation audit

The public registry has 47 detectors. Its 31 recommendations remain the default; `onceover.ALL_DETECTORS` provides the whole set for explicit selection. The remaining 16 include five target-triggered detectors. The recommendation and the ability to select a detector are different decisions.

## Protocol

On onceover 0.1.7 in the worktree at `a0527ef` with the partition-scagnostics opt-in registration, use the release extension with Python 3.13.7, Polars 2.0.0, and NumPy 2.5.4 on arm64 macOS 26.3. Load the 27 inputs in `benchmarks/gate/registry.json` through `gatelib.build_context(entry, [0])`, preserving its data, hints, and detector selections. Run `context.profile()` and run `context.profile(target=context.target)` wherever a target is designated. This includes seven held-out tables.

For each mode, count the detector IDs in the selected and grouped findings' evidence. Attribute must and should reference matches through `reference.finding_fits`, the gate's existing structural matcher. Count detector evidence in the 4,000-character digest through `Profiled.lines(4_000)`. These are observational credits. They do not measure what removing a detector would do to the ranking or give every finding type an equal opportunity to match. Keep text-only matches out of detector attribution.

Check access separately with `pytest tests/test_detector_defaults.py`. The detector-access test builds a panel with 36 dates and 24 entities, supplies a target, a contrast, and an ordering expectation, and checks the coverage of all 47 single-detector selections. It then requests all IDs together. The panel computations under `series` remain part of that detector and identify their individual work in coverage.

## Recommendation decision

The 27 tables offered 145 must or should reference statements across the applicable dataset and mode pairs. Of the 31 recommended detectors, 21 were credited on a structural reference match. `hotspots` had two matches for which it was the only credited detector, and its evidence appeared on 22 digest lines. The seven detectors with findings but no reference credit cover file integrity, recording patterns, or calendar gaps: `row_duplicates`, `sortedness`, `content_anomalies`, `series`, `row_consistency`, `digit_fingerprints`, and `date_gaps`. The historical audit records why the reference catalog misses several of these, and the calendar-gaps report documents a 218-business-day gap that another default pass did not state. [Earlier attribution audit](2026-11-16-detector-audit.md) · [Calendar-gap evidence](2026-11-18-defaults-framing.md)

The three recommendations with no findings in this audit were `expectations`, `group_contrast`, and `discontinuities`. Their recommendation is conditional. The gate inputs do not exercise the corresponding declarations in their default runs. In the ordered-axis fixture from `tests/test_axis_gate.py`, the default report skipped `discontinuities` without a declaration. Declaring the dose axis found a jump of 1.90 near dose 0.608 on held-out rows. Omitting `discontinuities` from an explicit recommended list on that same hinted input removed the step finding. Selecting it explicitly recovered the same step. Blanket demotion would remove this tested benefit while ordinary unhinted profiles already skip the search.

The measured opt-in registration preserved every baseline gate total for recommended profiles. The separate trial that recommended partitioned scagnostics increased the number of facts gained or lost after layout changes from 130 to 154. It stays selectable on request. [Promotion trial](2026-11-21-partition-scagnostics-promotion.md)
