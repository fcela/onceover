# Corpus cap displacement: three expectations diagnosed and resolved

Date: 2026-11-14. Change: report selection walk and corpus registry
(issue #294). Base: main at ad0a0fe3 (version 0.1.5).

## The failures

The corpus on the base build exits 1 with three machine-verified
expectation failures (90 of 93 machine-verified expectations
passing):

- thyroid: a `concentration` finding naming `pregnant` (98.6%
  false) is not in the report body.
- adventureworks: a `value_concentration` finding on
  `revision_number` (99.9% '10') is not in the report body.
- bike-sharing-hourly: the `(cnt, casual) -> registered`
  functional direction, expected as a grouped member of the
  verified relationship family over `casual`, `cnt`, and
  `registered`, is not found.

## Method

For each case the dataset was profiled at the default cap
(`max_findings=100`) and at `max_findings=5000`, which admits
every candidate. A statement present in the wide report but
absent from the capped one is displaced or dropped by
selection; a statement absent from both is lost in a pass. The
selection record's per-class counts (candidates, selected,
turned away) and the family counts locate where each statement
stops.

## Thyroid and adventureworks: one mechanism, selection as designed

Both statements are generated. The `pregnant` concentration is
present in the wide report at interestingness 0.83; the
`revision_number` concentration at 0.88. Both are hygiene-class
statements (concentration), and under cap pressure admission is
class-major: account first, then structure and shape, then
hygiene. On these tables the earlier classes fill the report
before any hygiene statement is offered:

| dataset | candidates (account/structure/shape/hygiene) | selected |
|---|---|---|
| thyroid | 1 / 97 / 17 / 43 | 1 / 82 / 17 / 0 |
| adventureworks | 1 / 120 / 72 / 42 | 1 / 70 / 29 / 0 |

The hygiene reserve does not bind in either case: it caps
hygiene admissions at 20, but hygiene is never reached, so the
reserve guarantees nothing. This is the selection design
operating as specified in
`docs/benchmarks/2026-11-13-report-selection.md`. Whether the
reserve should also guarantee hygiene a floor is the open
design question in issue #280; this change does not decide it.

Resolution: the two expectations are re-baselined to
`expect: "missed"` with the mechanism stated in each note, the
corpus's standing practice for cap displacement. No score was
changed and no selection rule was changed for these cases. The
assertions act as tripwires: if hygiene admission changes and
either statement reaches a capped report, the assertion fails
and the expectation is promoted back to found.

## Bike-sharing-hourly: a selection bug, fixed

The discriminating case has a different mechanism. The
`(cnt, casual) -> registered` statement is generated and forms
a verified relationship family under the identity lead
(`cnt = casual + registered`); the lead is selected, third in
the capped report. Grouped members spend no selection slot by
design. But the selection walk stopped outright when the cap
filled (`selected.len() >= max_findings`), and the family's
members rank far below the statements that fill the cap
(detector interestingness 0.17), so the walk never reached
them. At the default cap the report recorded families 0 and
grouped_statements 0, against families 4 and grouped 6 in the
wide report: the lead stood in the report with none of its
members grouped under it.

Resolution: the walk no longer stops at the cap. Past a full
cap it collects only members of families whose lead is already
selected — the keys that spend no slot — and skips every other
unreached statement, which the shadow admission pass continues
to measure exactly as before. A selection-level regression
test, `family_members_ranked_past_the_cap_still_group`, pins
the case: an identity lead selected first, filler statements
filling the remaining slots, the family's members ranked below
all of them; it fails on the old walk and passes on the new
one.

After the fix, at the default cap: families 4,
grouped_statements 6, and the `(cnt, casual) -> registered`
statement is in `grouped_findings` under the identity lead.
The structure class's turned-away count drops from 54 to 48:
the six grouped members are retained, not refused. The
expectation stands at found, satisfied by the grouped record
as its note describes, and the note records the repair.

## Hotel-bookings measurement (report only, no bar)

The hotel-bookings table (119,390 rows, standard budget,
default cap) was profiled before and after the change. The
report is unchanged: 100 findings, no grouped statements,
before and after. The predictable-missingness statements
remain outside the capped report — 58 hygiene candidates, none
selected — displaced by the same hygiene-phase mechanism as
the thyroid and adventureworks cases. This change neither
admits them nor claims to; their admission waits on the
reserve question named above.

## Corpus, before and after

Before: exit 1; machine-verified expectations 90/93, the three
failures above.

After: machine-verified expectations 93/93 across the 23
datasets that run (11 test-only datasets skip for missing
local data, as before). The runner exits 2, its standing
signal that documented cases — reviewed records, not machine
checks — stand at missed: 10 missed of 32 documented cases, in
thyroid, adventureworks, metro-interstate-traffic,
bike-sharing-hourly, and adult-census-income. Those records
are unchanged by this change; the three datasets above carry
the same documented tallies before and after.

## Gate

`scripts/release.py check` passes; `scripts/test_release.py`
6 passed; `cargo fmt --all -- --check` clean; `cargo clippy
--workspace --all-targets --locked -- -D warnings` clean;
`cargo test --workspace --locked` 418 passed, 0 failed
(including the new regression test); `pytest tests/ -q` 642
passed. The strict book build (`docs/book/build-html.sh`)
exits 0.
