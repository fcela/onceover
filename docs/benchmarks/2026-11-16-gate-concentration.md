# Gate concentration statements and the hotspots restore — 2026-11-16

Part of #317. This record covers three changes: the gate's reference
set gains concentration statements (and one completeness statement) so
the families the maintainer named as priorities are measured; the
statement-kind vocabulary gains the one kind those statements needed
that it lacked; and the `hotspots` detector returns to the default
set, reversing the hotspots half of the 2026-11-16 per-detector audit
(`docs/benchmarks/2026-11-16-detector-audit.md`), on the maintainer's
instruction.

## Why the gate could not see concentration

The audit moved `hotspots` to on-request because none of its 79
findings across the gate tables matched a reference statement. That
zero was structural, in two parts:

1. The reference set contained almost no concentration statements.
   Of the 85 statements on main before this change, one used the
   `concentration` kind (planted_mix `hotspot-payout`, added after the
   audit ran). A detector cannot match statements that do not exist.
2. The one concentration shape the panel data needed had no kind at
   all. The statement kinds map to finding categories; no kind reached
   the `gaps` category, so the panel's partial-period fact — stated by
   the `panel_coverage` detector as a `partial_period` finding,
   category `gaps` — could not be credited: its statement declared
   kind `missingness`, which requires the `missingness` category, and
   the panel's default profile contains no missingness-category
   findings. The audit record noted this verbatim.

Direct tests on the merged build showed the matching machinery itself
(`finding_fits`: statement columns a subset of the finding's columns,
kind matched on category or type) already credits both concentration
shapes — a single-column `value_concentration` finding and a
two-column `hotspot` finding both fit a `kind: concentration`
statement. The vocabulary change is therefore one line: a
`completeness` kind mapping to the `gaps` category, mirroring the
`concentration` kind's shape. `missingness` keeps its per-column
meaning (null shares); `completeness` is about periods or rows a
frame should have and does not.

Equivalence check: with only the vocabulary change applied (the kind
added, no statements changed), a reference-component gate run
reproduces the baseline run's reference totals and every dataset's
per-statement detail exactly (timing fields aside). An unused kind
changes nothing.

## Statements added

Each statement's check expression was evaluated against the table
before the statement was accepted; all checks pass
(`check_failed: 0` in the runs below).

| Dataset | Statement | Tier | Kind | Verified fact |
|---|---|---|---|---|
| planted_mix | `hotspot-payout` (already on main) | should | concentration | cell channel = kiosk × applicant_age 18–24: 207 of 6,000 rows (3.45%) holding 24.30% of total payout, 7.04× its row share |
| bank-marketing | `previous-contact-february-concentration` (new) | should | concentration | cell days_since_previous_campaign_contact in (190, 278] × month = feb: 542 of 45,211 rows against 83.5 expected under independence, 6.49× (the pass states the cell on its encoded sample: 236 against 36.6, 6.44×) |
| bike-sharing-hourly | `cnt-rush-hour-concentration` (new) | should | concentration | cell workingday = 1 × hr in {7,8,9,17,18,19}: 2,983 of 17,379 rows (17.16%) holding 1,181,285 of 3,292,679 rentals (35.88%), 2.09× its row share |
| adult-census-income | `native-country-concentration` (new) | should | concentration | `United-States` holds 43,832 of 47,985 non-null `native_country` rows (91.35%); next level Mexico 951; 857 nulls |
| south-german-credit (held out) | `foreign-worker-concentration` (new file) | should | concentration | `foreign_worker` = 'no' holds 963 of 1,000 rows (96.3%); 'yes' 37 |
| state-unemployment-long | `missing-month` (re-kinded) | must | completeness (was missingness) | exactly 51 null `unemployment_rate` values, all dated 2025-10-01 — one per state, the partial period |

Tier reasoning: the two value-concentration statements state facts
the column profile already states as findings, so they are expected
matches; they are `should`, not `must`, because they are new
provisional statements in the files' standing convention. The bike
statement is `should` for a stronger reason: the concentration is
measure-weighted — it lives in the `cnt` values over the cell, not in
the cell's row count — and no current pass aggregates a measure over
categorical cells outside panel structure (issue #329). Its miss is
the measurement, and the statement's note says so. `hotspot-payout`
stays `should` as merged. `missing-month` keeps its `must` tier and
its check; only its kind changed.

## Match results on the enlarged set

Measured on the five affected datasets (reference component, final
build of this change); the full enlarged-set run follows in this same
record.

- `native-country-concentration` (adult): matched at every scope in
  both modes, credited to the column profile's `value_concentration`
  finding.
- `foreign-worker-concentration` (south-german-credit, held out):
  matched at every scope in both modes, same credit.
- `missing-month` (state-unemployment-long), re-kinded to
  `completeness`: matched at every scope in both modes, credited to
  `panel_coverage`'s `partial_period` finding. Under the old
  `missingness` kind it matched nothing; this is a must-tier
  statement the gate could not credit before.
- `previous-contact-february-concentration` (bank-marketing): a
  `hotspot` finding fits the statement in the default full report —
  the first matched credit the `hotspots` detector has earned in the
  gate (the audit recorded zero). Digest scopes are reported with the
  full run below.
- `cnt-rush-hour-concentration` (bike-sharing-hourly): matched
  nowhere. The pass ran (28 combinations, 1,144 cells examined) and
  the cell's row counts track independence; the concentration is in
  the `cnt` values. Measure-weighted — issue #329.
- `hotspot-payout` (planted_mix): matched nowhere, for the same
  reason plus one more. The pass ran (6 combinations, 61 cells) and
  the cell's 207 rows sit at their independence expectation (~196);
  the concentration is in payout per row, not in rows. And
  `applicant_age` is numeric, outside the columns the pass scans.
  Measure-weighted — issue #329.

<FILL: full-run totals before/after and per-scope digest results for
the bank statement.>

## The hotspots restore

The maintainer's instruction: "Put them in and rerun. Include
hotspots in the gating." The registry change is one word
(`OnRequest` → `DefaultSet` on the `hotspots` descriptor): 30
default / 13 on-request of 43, triggers unchanged. Every surface the
audit's move touched is reversed: the `profile()` docstring counts
and on-request list, the packaged agent guide count, the root
AGENTS.md detector paragraph and `docs/findings.md` (membership
counts and lists only in those two files), the book's api and
capabilities pages (counts and the on-request enumeration — both
also carried a stale eleven-detector list that omitted
`decision_tables` and `interactions`; corrected to the thirteen),
the hotspots methodology page (it stated the detector is on
request), the capability manifest (regenerated), and the tests that
pinned the move: the registry partition and default-count assertions,
the hotspots test docstring, and the Rust deadline integration test,
which again asserts the pass's capped status in a default run
instead of asserting the on-request skip on a named run. Book
notebooks whose stored outputs embed the default set are
re-executed.

Re-measurement, against the audit's zero: with hotspots in the
default set, the gate's own profiles contain 25 `hotspot` findings
across the reference tables, and the detector earns a matched credit
on the bank-marketing statement above. The two cell statements it
does not earn are the two measure-weighted ones (issue #329): the
pass counts rows, and on those two tables the rows do not
concentrate. The membership implication is the maintainer's call,
already made: hotspots is in the gating. What the enlarged gate adds
is that the family's remaining gap is now a measured miss with a
named cause, not an absence of statements.

<FILL: attribution table (credits per detector for the new
statements, as-gated, per mode) and any digest displacement the
restore causes, per dataset, from the full run.>

## Remaining uncreditable families

The `duplicates` category still has no kind reaching it; formats
types beyond `sentinel_values`/`boundary_mass` and anomalies types
beyond the outlier kind's three are reachable only kind-less (too
broad to attribute a statement to a family) or through an explicit
`types` list. These are the next gaps of the same shape, named here
rather than fixed: each needs its own statements first, the same
order this change followed for concentration.

## Legs

<FILL after the full run: test_gate.py, full pytest, corpus harness,
cargo tests, strict book build.>
