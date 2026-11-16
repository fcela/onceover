# Default max_findings: measured at 100 / 75 / 50 / 25 — no lower cap holds

Part of issue #317, phase 3: "Default report size. Lower the default
`max_findings` so the default JSON for a 15-column table is in the tens
of thousands of characters. The full set remains available by raising
the cap."

The default stays at 100. The mildest reduction measured, 75, already
removes must-tier statements the gate credits, and the cap turns out
to be a weak lever on JSON size: the 15-column reference table enters
the plan's size band only at a cap of 25, where must-tier recall has
fallen by half. This record is the measurement behind that decision.
No code changes.

## Protocol

- Tree: main `064132d8` (onceover 0.1.5, report schema 0.8), release
  build, CPython 3.12.3, Polars 2.1.0, NumPy 2.5.5, seed 0. The
  cap-100 baseline below reproduces the merged phase-2 record's final
  gate totals (must 10/12 at 4k default and 11/12 in target mode,
  planted 29/46 at 4k, repeated facts 0), so the environment measures
  the same library the gate history records.
- Where the cap binds: every gate dataset profiled through the gate's
  own Context (default mode, and target mode where the registry
  designates a target), recording the report's selection record —
  candidates, verified candidates, distinct statements (what the
  passes generated, before the cap), selected (what the body holds) —
  and the JSON size. The corpus tables were profiled the same way,
  with the corpus runner's call shape (registry budget, detectors,
  hints, target).
- Candidate caps: the gate run four times with `max_findings` set to
  100, 75, 50, 25 for the run — the value passed as the profile
  call's argument, which no gate caller passes itself, so each run
  profiles exactly as a library default of that value would.
  Components: reference, planted, redundancy, cost.
- Decision rule: the lowest cap at which no must-tier reference
  recall (4k digest or full report, default or target mode, gated or
  held out) and no planted-at-4k count falls against the cap-100
  baseline. Should-tier movements are reported with their causes.

## Where the cap binds at 100

The cap binds on most real tables. Across the 30 gate profiles
(18 datasets, default and target modes), 23 generate more distinct
statements than the body holds; 15 bodies sit exactly at the cap.
Default mode, per gate dataset:

| dataset (columns) | distinct generated | in body | JSON chars |
|---|---|---|---|
| adult-census-income (15) | 132 | 100 | 259,208 |
| bank-marketing (17) | 128 | 100 | 274,964 |
| abalone (9) | 124 | 90 | 203,582 |
| bike-sharing-hourly (17) | 279 | 100 | 297,003 |
| metro-interstate-traffic (9) | 91 | 90 | 227,407 |
| thyroid (32) | 178 | 100 | 317,419 |
| absenteeism-at-work (21) | 373 | 100 | 299,715 |
| hospital-dha (64) | 179 | 89 | 391,685 |
| congressional-voting (17) | 107 | 66 | 160,878 |
| adventureworks (26) | 255 | 100 | 303,055 |
| nab-nyc-taxi (2) | 35 | 35 | 105,886 |
| state-unemployment (52) | 525 | 100 | 453,221 |
| state-unemployment-long (3) | 19 | 19 | 72,329 |
| south-german-credit (21, held out) | 77 | 77 | 225,660 |
| soybean (36, held out) | 165 | 80 | 277,643 |
| lace-satellites (21, held out) | 288 | 100 | 270,601 |
| sec-edgar-company-facts (10, held out) | 150 | 100 | 258,774 |
| gse15258-expression (5,006, held out) | 44 | 44 | 6,666,369 |

Two properties matter for what follows. First, the body is often
short of the cap even under pressure (congressional-voting 66,
soybean 80, hospital-dha 89): the hygiene reserve is a floor as well
as a ceiling, so when the other classes run out the report is shorter
than the cap rather than padded. Second, the JSON size is not mostly
findings — the wide expression table holds 44 findings in 6.7M
characters of column cards, and even with its body cut to 25
findings the 15-column table's JSON still totals 95,457 characters
(see its cap-25 row below).

On the corpus side, 21 of the 25 entries that run in this environment
generate more than the body holds (9 entries skip here, as in CI).
The incident-event-log entry generates 231 distinct statements for a
100-finding body. Its `impact` value-concentration statement — the
subject of issue #325 — is in the body on current main; see the
corpus section below.

## Candidate caps

Gated totals (13 datasets; must of 12, should of 16, planted of 46):

| metric | cap 100 | cap 75 | cap 50 | cap 25 |
|---|---|---|---|---|
| must, 4k, default | 10 | 10 | 8 | 5 |
| must, 4k, target | 11 | 10 | 9 | 7 |
| must, full, default | 11 | 11 | 9 | 5 |
| must, full, target | 11 | 10 | 9 | 7 |
| should, 4k, default | 6 | 7 | 8 | 11 |
| should, 4k, target | 10 | 9 | 8 | 12 |
| should, full, default | 13 | 13 | 12 | 12 |
| should, full, target | 14 | 14 | 14 | 13 |
| planted, 4k | 29 | 29 | 28 | 22 |
| planted, full | 33 | 30 | 23 | 15 |
| repeated facts, default / target | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| findings in body (cost total) | 1,089 | 870 | 604 | 319 |
| JSON chars (cost total) | 3,366,352 | 2,920,197 | 2,318,771 | 1,616,879 |

Held-out totals (5 datasets; must of 7, should of 4, planted of 11):

| metric | cap 100 | cap 75 | cap 50 | cap 25 |
|---|---|---|---|---|
| must, 4k, default | 3 | 3 | 1 | 1 |
| must, 4k, target | 3 | 1 | 3 | 3 |
| must, full, default | 3 | 3 | 3 | 1 |
| must, full, target | 3 | 3 | 3 | 3 |
| should, all scopes and modes | 2 | 2 | 2 | 2 |
| planted, 4k | 6 | 6 | 7 | 7 |
| planted, full | 7 | 5 | 4 | 4 |
| findings in body (cost total) | 357 | 300 | 200 | 100 |
| JSON chars (cost total) | 1,032,678 | 903,060 | 693,058 | 460,423 |

JSON size per table (default mode), against the plan's aim of tens of
thousands of characters for a 15-column table:

| table (columns) | cap 100 | cap 75 | cap 50 | cap 25 |
|---|---|---|---|---|
| adult-census-income (15) | 259,208 | 222,004 | 169,754 | 95,457 |
| bank-marketing (17) | 274,964 | 235,097 | 177,654 | 104,656 |
| abalone (9) | 203,582 | 181,545 | 127,606 | 80,206 |
| bike-sharing-hourly (17) | 297,003 | 248,632 | 192,662 | 122,449 |

Individual statements move non-monotonically as the cap falls
(held-out must at 4k in target mode reads 3, 1, 3, 3 across the four
caps; should-tier recall at 4k rises at cap 25, to 11 and 12, because
the digest's sections refill from a different body). Class rationing
rebalances under pressure, so single statements can return at a lower
cap. The protected totals, though, move one way: every candidate
below 100 loses must-tier recall somewhere, and the losses grow as
the cap falls.

## What cap 75 loses, by name

- adult-census-income, must, `income-capital-gain` ("income depends
  on capital_gain"): matched at every scope in target mode at cap
  100, matched nowhere in target mode at cap 75 — the finding leaves
  the report body. Mechanism, measured on the same profile: the
  target-mode run generates 150 distinct statements at both caps. At
  cap 100 the selection record admits 75 structure-class statements;
  at cap 75 it admits 50, while the hygiene class keeps its 20 slots
  and shape its 5 — the whole reduction lands on the structure class.
  The statement's carrier at cap 100 is the predictive finding
  ("income is predictable from marital_status (51% of gain),
  education_num (17% of gain), capital_gain (12% of gain)"), which
  sits in the displaced structure tail at cap 75. The statement is
  still generated; this is cap displacement, the mechanism of issue
  #325, removing a must-tier driver statement.
- sec-edgar-company-facts (held out), must, `concept-scale` and
  `balance-sheet-never-negative`: matched at 4k in target mode at cap
  100, unmatched at 4k at cap 75, still matched in the full report —
  digest displacement under the smaller body, not body displacement.
- adult-census-income, should, `income-age-hours`: matched at 4k in
  target mode at cap 100, unmatched at cap 75 (the one should-tier
  loss at 75; gated should at 4k in default mode gains one, 6 to 7,
  and full-report should recall is unchanged in both modes).
- Planted statements in the full report fall at 75 (33 to 30 gated,
  7 to 5 held out) while planted at 4k holds (29 and 6).

At cap 50 the losses widen: three adult musts (income-marital at 4k,
income-education and income-capital-gain at every scope in default
mode), bike-sharing's cnt-hour must in target mode, both sec-edgar
musts at 4k in default mode, and planted at 4k falls (29 to 28
gated). At cap 25, gated must recall at 4k is 5 of 12 (default) and
7 of 12 (target), and planted at 4k is 22 of 46.

## Decision

The default `max_findings` stays at 100, for two independent reasons,
each sufficient on the measurements above.

1. Recall. The decision rule admits no candidate: 75, the mildest
   reduction, already costs must-tier recall in both scopes (gated
   target mode at 4k and in the full report; held out at 4k), and
   50 and 25 cost more, including planted statements at 4k. A
   smaller default report would state less of what the gate requires
   the report to state.
2. Size. The plan's aim — a 15-column table's default JSON in the
   tens of thousands of characters — is not reachable through this
   cap at any value the gate tolerates. Cutting the findings body by
   a quarter cuts the 15-column table's JSON by 14% (259,208 to
   222,004), because column cards, the frame account, and coverage do
   not scale with the cap. The table enters the size band only at
   cap 25 (95,457), where gated must recall at 4k stands at 5–7 of
   12 and planted at 4k at 22 of 46. The size aim, if pursued, has to
   come from the report's non-findings bulk or from selection that
   protects the statements the gate credits under pressure — the
   displaced statements sit in the structure class's tail — not from
   a smaller cap.

What would change this answer: a selection change that keeps
credited driver statements in the body under cap pressure, re-measured
with this same protocol. Until then, the full generated set remains
available exactly as the plan provides — by raising the cap (on
adult, a cap of 1,000 admits 146 of the 150 distinct statements).

## Corpus consequences

At the shipped default, the corpus run in this environment — both
test-only data files present locally, so all 25 runnable entries run
and 9 skip — passes 190 of 190 machine-verified expectations, with
the documented-case tallies unchanged (found 18, missed 11, partial
4). At cap 75 the same run passes 186 of 190: four expectations on
three entries flip to failed, every one a dependency statement
displaced from the smaller body — two affine expectations on
taiwanese-bankruptcy, a conditional-rules expectation on
sec-edgar-company-facts, and the adult-census-income predictive
expectation, whose finding is the same one that carries the gate's
capital-gain must statement above.

Issue #325's expectation — incident-event-log, `impact`
value-concentration, expected found — passes on current main at cap
100 and at cap 75, verified directly: the profile generates 231
distinct statements, and the statement ("column impact is highly
concentrated: 94.8% 'Medium'") is among the selected findings at
both caps. The failure #325 records was measured on `ca275fdf`,
before the phase-2 selection rebuild; that rebuild changed the
body's composition, and the failure does not reproduce on `064132d8`.
The issue's disposition is separate from this change, which does not
touch it.

No corpus registry entry changes: the default does not change, so no
expectation's outcome changes.

## Provenance

Measurements run on a hash-verified reconstruction of main
`064132d8` (514 blobs, no mismatches), built with
`maturin develop --release` under the workspace build lock. Gate
scorecards and run logs are working papers of this change; the
numbers above are taken from them directly. The shipped change is
this record only — no library code, no registry, no defaults.
