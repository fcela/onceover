# Detector battery after frame-model step 2: selection mix and a discontinuities cap

This pass re-measured the default detector battery on the full
bundled corpus with frame-model step 2 in place (schema 0.5): what
each of the 22 default-active detectors contributes to the
*selected* reports, how much of that contribution step 2's
restatement demotion marks down, and where the selection cap binds.
One change follows from the numbers:

- `discontinuities` gains a per-detector report cap of 25 selected
  statements, joining `functional_dependencies` (25) and `gbm` (20).
  Its statements score in a narrow high band (0.83–0.93), so score
  ranking cannot ration them against other detectors; on the
  state-unemployment panel they took 81 of the 100 report slots and
  crowded out every gbm and numeric-dependence statement. The cap is
  disclosed in coverage with counts, like the two existing caps.

Everything else the battery flagged is recorded below as a
recommendation with its numbers; no other default, cap, or parameter
changes in this pass.

## Protocol

All 22 bundled corpus datasets (registry at the run's base commit;
the 23rd entry, Adult, was still in an open PR and is not in this
pass), standard budget, registry hints and date parsing kept. Three
runs per dataset, all with the full default detector set — the
registry's per-entry detector restrictions, which exist to pin
expectation runs, were set aside, because the question is what the
default set does:

- **A (default, `timing=True`)**: the report a caller gets —
  selection totals, per-pass wall time from coverage `elapsed_ms`
  (findings are identical with timing on), per-finding attribution,
  report characters.
- **B (uncapped, `max_findings`/`max_rules` = 100000)**: the
  statement universe the ledger ranked — each statement's
  demotion-adjusted score, attribution, and demotion reasons, and
  whether the default run selected it. Statements consumed by
  production-stage caps (the FD and gbm report caps apply inside
  selection even here, prism's per-outcome rule caps inside
  production) do not appear; their counts come from run A's coverage
  disclosures and the solo runs.
- **S (solo)**: one run per default-active detector
  (`detectors=[d]`, caps raised) giving that detector's own
  candidate and distinct-statement production with no other detector
  selected.

A statement is attributed to the detector of its first evidence
record — the ledger's own attribution for the per-detector caps.
Character counts are per-finding JSON lengths summed by attribution;
report characters are the full `to_json()` length. Findings are
identical across the before/after sweeps except where the cap change
itself moves the mix (see Measured effect).

## The aggregate picture

Across the 22 datasets: 2,637 candidates, 2,482 distinct statements,
1,423 selected findings. Nine datasets saturate the 100-finding cap
(absenteeism-at-work, adventureworks, bike-sharing-hourly,
lace-animals, lace-satellites, soybean, state-unemployment,
taiwanese-bankruptcy, thyroid). 2,024 statements are visible in the
uncapped runs; 601 of them lose selection. Step 2's demotion marked
53 candidates (25 `calendar_derivation`, 28 `scaffolding_subject`);
at statement level 78 of the 2,024 visible statements carry a
demotion, and on the most cap-pressured frame (bike-sharing-hourly)
every one of the 21 demoted statements ranks below the cut — the
demotion mechanism is doing the work it was built for, and what
still crowds reports is not restatement but undifferentiated score
mass (below).

Selected findings total 2,592,048 characters. Whole reports total
8,528,191 characters, of which 5,318,204 are gse15258-expression's
column cards (54,675 columns; its 45 findings are ordinary-sized) —
on that frame the cards, not the findings, are the report-size
story, and no detector change addresses it.

At the selection margin, scores tie in blocks: on bike-sharing the
statements ranked 96–106 score 0.75–0.77, on soybean a wall of
column_profile statements scores 0.64, on state-unemployment the
discontinuities statements score ≈0.897 almost uniformly. Among
ties the canonical-key order decides. Score tweaks cannot ration
one detector against another inside a tie block; the structural
caps are the instrument that works, which is what the one change
uses.

## Per-detector results

Candidates and statements are solo-run production; *universe* is
the statement count visible in the uncapped full run (lower where a
production or report cap consumes statements first); *selected* is
the default run's count; *demoted* counts demoted statements in the
universe; *cut* counts universe statements the default run did not
select; time is the detector's passes' share of the 104.4 s of
coverage-attributed pass time across all 22 timed runs.

| Detector | Cand. | Stmts | Universe | Selected | Demoted | Cut | Sel. chars | Datasets sel./prod. | Time |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| discontinuities | 225 | 225 | 225 | 224 | 0 | 1 | 440,313 | 12/12 | 56.8% |
| functional_dependencies | 512 | 512 | 196 | 188 | 8 | 8 | 340,794 | 14/14 | 8.2% |
| gbm | 307 | 307 | 190 | 152 | 1 | 38 | 380,326 | 16/18 | 6.7% |
| surprises | 195 | 177 | 177 | 169 | 0 | 8 | 253,176 | 17/17 | 1.5% |
| breaks | 244 | 244 | 244 | 138 | 0 | 106 | 201,092 | 8/8 | 0.8% |
| column_profile | 316 | 310 | 310 | 136 | 50 | 174 | 137,884 | 20/21 | — |
| numeric_dependence | 197 | 197 | 197 | 93 | 2 | 104 | 116,437 | 10/14 | 8.4% |
| hotspots | 129 | 95 | 95 | 51 | 0 | 44 | 117,659 | 8/13 | 10.8% |
| association_rules | 72 | 72 | 58 | 48 | 0 | 10 | 124,075 | 7/7 | 0.0% |
| identities | 63 | 63 | 63 | 63 | 0 | 0 | 92,995 | 3/3 | 2.2% |
| prism | 145 | 55 | 44 | 27 | 8 | 17 | 60,992 | 9/9 | 0.0% |
| ordered_partitions | 45 | 45 | 45 | 26 | 0 | 19 | 107,241 | 8/8 | 0.1% |
| digits | 57 | 57 | 57 | 20 | 0 | 37 | 34,502 | 7/11 | 0.1% |
| decision_tables | 22 | 22 | 22 | 22 | 0 | 0 | 29,721 | 8/8 | 0.3% |
| duplicate_encodings | 44 | 44 | 44 | 21 | 0 | 23 | 31,578 | 5/8 | 0.3% |
| keys | 20 | 20 | 20 | 19 | 1 | 1 | 22,512 | 14/14 | 0.0% |
| regimes | 11 | 11 | 11 | 9 | 0 | 2 | 59,960 | 6/7 | 0.1% |
| sortedness | 12 | 12 | 12 | 4 | 7 | 8 | 4,265 | 4/8 | 0.1% |
| interactions | 7 | 7 | 5 | 5 | 0 | 0 | 11,739 | 2/2 | 1.7% |
| row_duplicates | 5 | 5 | 5 | 5 | 0 | 0 | 14,599 | 5/5 | 0.1% |
| sentinels | 9 | 4 | 4 | 3 | 0 | 1 | 10,188 | 2/3 | 0.1% |
| expectations | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0/0 | 0.0% |

(column_profile's passes share the column-cards stage with the
pipeline's extraction, so no separate pass-time share is attributed
to it.)

Reading notes the table needs:

- **functional_dependencies** produces 512 statements; its own cap
  of 25 admits 196 to the ranking and the global cap then cuts only
  8 — all 8 demoted calendar derivations. On absenteeism-at-work it
  produces 214 statements and the cap turns 178 away (coverage
  discloses the count). The cap, not the score, is what stands
  between that frame and a single-detector report.
- **prism**'s drop from 145 candidates to 55 statements happens
  inside production (six targets, two rules per outcome); the
  ledger's 20-rule cap then binds only on congressional-voting,
  which selects exactly 20 rules.
- **association_rules** shows 1–5 fewer statements in full runs than
  solo on six datasets: the same rule statement produced by both
  prism and association mining consolidates to one statement.
- **column_profile**'s 50 demoted statements are 30 of its own
  declared triviality (`continuous_measure_key_candidate` on
  identifier-role statements), 15 identifier-role and 5
  format-variation statements about scaffolding columns. That is
  the demotion machinery working on the detector that produces the
  most raw statements, not a defect in it.
- **expectations** produced zero because the corpus barely declares
  checkable structure: only the two NAB entries pass hints, and they
  declare a time axis only — its coverage there reads "0 checks run,
  0 findings". Zero production here is a corpus artifact, not the
  2026-11-02 pattern (five detectors at zero in every configuration,
  hints included).
- **hotspots**' 10.8% time share is one dataset: 11.2 s of its
  11.3 s total is gse15258-expression (86 rows × 54,675 columns);
  on every other dataset it costs at most 44 ms. The dependence
  screen shows the same concentration (6.2 s of 8.8 s there).
  Wide-table cost lives in those two passes on that one frame;
  both are capped and coverage-disclosed, and neither produces a
  selection problem (51 and 93 statements selected corpus-wide
  against 44 and 104 cut — ordinary cap victims).

## Recommendations

Implemented in this change:

1. **Cap `discontinuities` at 25 selected statements.** It is the
   only uncapped detector that produces more than 25 statements on
   multiple datasets (81 state-unemployment, 31 absenteeism-at-work,
   28 bike-sharing-hourly, 28 taiwanese-bankruptcy, 26
   adventureworks), holds the largest selected-character share
   (440,313), and — unlike FD and gbm before their caps — cannot be
   rationed by score, because its statements cluster at 0.83–0.93
   while the detectors it crowds out score 0.71–0.82. On
   state-unemployment it crowded out 20 of 20 gbm statements, 20 of
   20 numeric-dependence statements, 62 breaks statements, and 13
   ordered-partition statements. The 2026-11-02 change documented
   this flood and deferred it to the selection-mix question; this
   is that measurement. The production cost is unchanged (the cap
   acts at selection; the pass's 56.8% time share buys the 156
   statements that survive corpus-wide, and the standard tier's
   diagonal tiering from 2026-11-02 already bounds the search).

Recorded, no change:

2. **Keep the FD cap at 25 and the gbm cap at 20.** Both bind
   exactly where they were aimed (FD: 316 statements turned away
   pre-ranking; gbm: 117, and its state-unemployment coverage line
   shows 31 further statements past the cap). Raising either
   re-floods saturated reports with the corpus's two heaviest
   statement producers; lowering either cuts statements the ranking
   currently admits on unsaturated frames. Two-sided on this
   evidence.
3. **Keep `breaks` and `numeric_dependence` uncapped and default.**
   They are the cap's victims, not its problem: 106 of 244 breaks
   statements and 104 of 197 dependence statements are cut, and
   they are the first statements admitted when slots free (see
   Measured effect). Capping a victim would only move the loss.
4. **Keep `expectations` hint-gated in the default set.** Its zero
   is explained by the corpus's hints, and a caller who declares
   `no_gaps`, `cadence`, or `order_by` expects the checks to run
   without also naming the detector.
5. **Keep `sentinels` and `interactions` default.** Neither meets
   the zero bar that moved the 2026-11-02 five: sentinels selected
   3 statements on the only datasets whose data carries
   sentinel-shaped values (bank-marketing 2, metro-interstate-traffic
   1) at 0.1% of pass time; interactions selected all 5 of its
   statements and is the only interaction screen. Interactions'
   1.7% time share (1.8 s corpus-wide) is the price of the
   H-statistic screen, recorded here so a future battery can compare.
6. **Keep prism's production caps and the association pass cap.**
   The per-outcome and per-target caps bind inside production where
   the rule volume is generated; the surviving rules select at
   ordinary rates and no dataset's rule section is starved (the
   ledger's 20-rule cap binds once, exactly at 20).

## Measured effect

The after sweep (same protocol) changes the selected mix on four
datasets and nothing else; every other dataset's mix is identical.

| Dataset | Lost | Gained | Report chars |
|---|---|---|---:|
| state-unemployment | 56 discontinuities | 20 gbm, 18 breaks, 13 ordered_partitions, 4 numeric_dependence, 1 regimes | 269,490 → 355,485 |
| absenteeism-at-work | 6 discontinuities | 3 surprises, 2 numeric_dependence, 1 prism | 223,381 → 221,195 |
| bike-sharing-hourly | 3 discontinuities | 2 surprises, 1 breaks | 199,552 → 198,346 |
| taiwanese-bankruptcy | 3 discontinuities | 3 numeric_dependence | 353,347 → 351,184 |

Corpus-wide, discontinuities selections fall 224 → 156 and the
freed slots go to the next-ranked statements on the saturated
frames. The state-unemployment report grows by 85,995 characters:
the statements the cap admits there (gbm predictability and
dependence statements with full evidence payloads) are heavier than
the step findings they replace. The cap trades size for mix on
that frame; the trade is stated, not hidden — the alternative the
battery measured was a 100-finding report that is 81% one
detector's step statements and contains no predictability or
dependence statement at all.

The corpus run after the change: 22 ok / 0 fail / 11 skipped, with
no findings-count change on any registry-configured run and no
expectation edited. The step-discontinuity expectations (nine
datasets) still pass: the cap keeps each dataset's top-ranked step
findings, and the two saturated default-config datasets
(bike-sharing-hourly, taiwanese-bankruptcy) keep their full 100
findings with the freed slots refilled.
