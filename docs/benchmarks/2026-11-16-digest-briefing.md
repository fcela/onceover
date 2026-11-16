# The digest as a briefing: gate measurements for the phase 2 rebuild (2026-11-16)

Issue #317, phase 2. The digest (`Report.to_digest`) was rebuilt as a
briefing organised by reader question, and report selection changed
with it: the hygiene reserve is now a floor as well as a ceiling.
This record holds the before/after measurements. Base is main at
daf7a790 (0.1.5); the head build is this change. All runs use the
repository's own instruments: the gate in `benchmarks/gate/`
(seed 0, full runs), the corpus in `benchmarks/corpus/`, and a
stricter local statement harness (36 issue facts over 12 tables,
strict id-based matching) kept outside the repository.

## What changed

- `python/onceover/__init__.py`: `to_digest` renders sections by
  reader question (what a row is; what is wrong; what is redundant;
  what relates to the target; other relationships; anomalies; not
  examined), each with its own character allowance. Findings merge
  into facts — one line per distinct set of stated columns, a
  model-based finding stating its target and credited drivers —
  and the exact form of a fact speaks for it. Lines are plain
  words, numbers at three significant figures, one finding id per
  line. The "organises the frame" account lines, the headline, the
  coverage list, and the category counts leave the header.
- `crates/onceover-report/src/ledger.rs`: under cap pressure, the
  hygiene statements the reserve protects sequence ahead of
  structure and shape. The reserve was a ceiling only; sequenced
  last, hygiene was displaced entirely whenever structure and
  shape filled the cap.
- `benchmarks/gate/gatelib.py`: the digest parser reads the
  briefing line format alongside the rolled-up format (a gate
  comparison runs the head harness against both builds' digests).
  Statements, thresholds, and components are unchanged.
- `benchmarks/corpus/registry.json`: 14 expectations updated from
  the measured run (below).

## The gate, before and after

| metric | default before | default after | target before | target after |
|---|---:|---:|---:|---:|
| reference must at 4k (of 11) | 7 | 8 | 7 | 11 |
| reference should at 4k (of 12) | 7 | 6 | 8 | 8 |
| reference must at 8k | 11 | 11 | 11 | 11 |
| repeated facts at 4k | 4 | 0 | 3 | 0 |
| long decimals at 4k | 53 | 0 | 41 | 0 |
| permuted relationship lines at 4k | 136 | 77 | 67 | 42 |
| planted facts found at 4k (of 46) | 20 | 28 | — | — |
| planted facts found in full reports | 34 | 33 | — | — |
| truth mismatches | 0 | 0 | — | — |
| invariance violations (total) | 408 | 430 | — | — |

Finding-line share per digest on the four reference tables after
the change: 839–919 per mille (adult 889/880, bank 890/879,
abalone 919/909, bike 839/835, default/target). Before, the summed
share across all gate tables was 7118 (default) and 4531 (target);
after, 10622 and 7001.

`compare.py` on the two scorecards: **FAIL — 32 worse, 94
improved, 24 unchanged.** The worse side, by cluster:

- Invariance, +22 in total (reverse +10, noise +8, scale +3,
  row_shuffle +1). The briefing states more findings per digest
  (235 finding lines across the reference modes, against 111), so
  more of its lines sit near a taking boundary and swap under
  perturbation.
- Planted facts in full reports, 34 → 33: on two tables a planted
  statement is displaced from the 100-finding report by the
  hygiene floor's admissions; on one table a statement the floor
  restored nets the total to −1.
- Reference should at 4k in default mode, 7 → 6 (one bank
  statement), and the repeated-fact count is 0 in both modes
  after, against 4 and 3 before.

## Exit criteria (issue #317 phase 2), as measured

- **Must recall at 4k ≥ 90% on non-held-out tables.** Target mode
  11/11 (100%) — met. Default mode 8/11 (73%) — **not met**. The
  three misses (bank subscription-duration, abalone rings-sex,
  bike cnt-hour) are each carried by a level-contrast fact that
  ranks 21st–29th by score in a 28–44-fact relationship section;
  the budget is fully spent in every case. In default mode the
  digest cannot know which column is the table's outcome; the
  reference statements were written with that knowledge. At 8k
  the default digest states all 11.
- **Zero repeated facts.** Met: 0 in both modes (gate), and 0 on
  every table of the strict harness.
- **≥ 80% of characters on finding lines.** Met on the reference
  tables (84–92% per digest). The strict harness measures 83–94%
  on its tables except the 19-finding panel table (56%), where
  the fixed blocks dominate a small report.
- **Zero relationship statements on controls.** Met on the strict
  harness's fully permuted controls (0 in both runs' modes). The
  gate's permuted component, which counts relationship-category
  lines in partially permuted tables' digests, falls 136 → 77
  (default) and 67 → 42 (target) but is not zero: the profiler
  still states weak relationships in permuted data, and the
  briefing now carries more of them per digest.
- **Every target-mode digest names the target's strongest driver
  in a direct statement.** Met on all four reference tables: the
  predictability line for the target leads its section (adult,
  bank, abalone), and bike's hour contrast is stated directly.
- **Held-out tables within 10 points.** The gate registry's
  held-out tables carry no reference statements, so the criterion
  is vacuous there. On the strict harness's held-out split it is
  **not met**: bank must recall 1/5 default and 2/5 target against
  12/23 and 15/23 on the non-held-out tables.

## Strict harness (secondary reading)

The local harness scores 41 must+should statements over 7
non-held-out tables with strict matching (a statement counts only
when a digest line resolves, by finding id, to a finding whose
declared columns and relation kind fit). Its parser was adapted
to the briefing format the same way the gate's was; statements
and the matching rule are unchanged.

| | must default | must target | must+should default | must+should target |
|---|---:|---:|---:|---:|
| before | 2/23 | 1/23 | 11/41 | 9/41 |
| after | 12/23 | 15/23 | 21/41 | 25/41 |

## Break and hotspot families in the digest (default mode, 4k)

Counts of findings of each family that reach the digest, as
members of stated facts, against their counts in the full report
(the report counts are the after build's; selection changed them
on three tables). Before this change, hotspot/concentration
findings reached the digest on **no** table.

| table | breaks in report | breaks in digest, before → after | hotspots in report | hotspots in digest, before → after |
|---|---:|---:|---:|---:|
| abalone | 24 | 6 → 12 | 1 | 0 → 0 |
| absenteeism-at-work | 0 | 0 → 0 | 11 | 0 → 6 |
| adult-census-income | 5 | 5 → 5 | 3 | 0 → 3 |
| adventureworks | 16 | 6 → 12 | 14 | 0 → 4 |
| bank-marketing | 3 | 3 → 2 | 6 | 0 → 3 |
| bike-sharing-hourly | 42 | 9 → 31 | 8 | 0 → 2 |
| congressional-voting | 0 | 0 → 0 | 2 | 0 → 1 |
| hospital-dha | 13 | 4 → 2 | 0 | 0 → 0 |
| lace-satellites | 0 | 0 → 0 | 0 | 0 → 0 |
| metro-interstate-traffic | 17 | 9 → 0 | 5 | 0 → 5 |
| nab-nyc-taxi | 11 | 4 → 11 | 0 | 0 → 0 |
| south-german-credit | 1 | 1 → 1 | 9 | 0 → 3 |
| soybean | 0 | 0 → 0 | 8 | 0 → 5 |
| state-unemployment | 54 | 9 → 11 | 0 | 0 → 0 |
| state-unemployment-long | 6 | 6 → 6 | 0 | 0 → 0 |
| thyroid | 16 | 7 → 5 | 12 | 0 → 8 |

Two starvations to name plainly:

- **metro-interstate-traffic: 17 break findings in the report, 0
  in the digest.** Every break fact on this table shares its
  column set ({date_time, one weather column}) with a functional
  dependency finding; the exact form speaks for the merged fact
  and routes it to "What is redundant", and that section's lines
  did not fit its allowance. The breaks are members of stated
  facts nowhere in the digest.
- **abalone: the table's one hotspot finding does not reach the
  digest.** Its fact's line fell outside the allowances.

Break counts also fall on hospital-dha (4 → 2) and thyroid
(7 → 5): on both, break facts merged with higher-scored facts
whose lines the sections took instead.

## Corpus

First run on this branch: 159/173 machine-verified expectations,
with 14 failures in 6 datasets — every one an expectation that
asserted a hygiene statement's absence under the old admission
order, plus one displacement the floor caused. The thirteen
absence assertions were promoted to found (the statements are
selected again); the predictability statement on bike's cnt,
displaced by the floor's squeeze on the structure class (85
candidates capped; the statement is present at max_findings=1000),
was re-baselined to missed. Each note records its mechanism.
Second run: **173/173 machine-verified expectations pass**;
documented cases unchanged (found 17, missed 11, partial 4).
