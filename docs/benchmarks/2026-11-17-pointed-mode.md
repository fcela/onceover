# Pointed mode, first increment: the metadata handover and target-designated diagnosis

Issue #310 asks for two modes under one entry point: a bare pass
that states a table's structure, and a pointed pass that, given a
designated outcome and the metadata only the caller holds, states
the outcome's diagnosis. The shipped library had the second mode's
machinery scattered — target-relative passes behind an explicit
target, no channel for meaning or purpose, and no statement of the
outcome itself. On the design's worked example (the
state-unemployment panel, target `unemployment_rate`) the shipped
library stated 1 of the design's 7 statement classes; on the
Chicago inspections file (issue #308) it stated none of the three
binding statements: the fail-rate contrast by inspection type, the
results distribution, and the risk-class null.

This increment builds the handover and the first two pointed
statement families. It does not build the inference machinery
(deferred by the design), conditional extremes beyond what the
existing passes state, or the digest's full lexicographic
re-ranking (the interestingness study's order governs what the
pointed briefing shows first through the digest's existing
precedence for target-relative statements; see "Selection"
below).

## What was built

1. **The metadata channel.** Seven hint keys join the existing
   fourteen — the design's six metadata keys, with
   purpose/questions as two surface keys: `gloss` (per-column
   text, or per-level text for a categorical column), `units`,
   `event`, `entity`, `recorded_after`, `purpose`, and
   `questions`.
   Declared metadata is carried into the report's frame account
   (`frame.declarations`, additive in schema 0.8) and accounted
   for in a `declarations` coverage entry: what was checked (the
   entity is a column; the event names a level of the target;
   recorded-after names are columns), what was carried unchecked
   (gloss, units, purpose, questions), and what was refused, with
   the reason. Unknown keys keep the existing rejection.
   Declared leakage suspects ride the existing leakage pass: a
   `recorded_after` column is checked first, stated as a declared
   suspect when it determines the target or clears the strength
   bar, and every declared column's measured strength is stated
   in the pass's coverage. Declared suspects and declared
   recorded-after columns are excluded from the predictor pools
   of the model-failure and predictability screens. A declared
   ordered axis continues to ride `ordered_axis`; nothing
   parallel was added.
2. **The outcome account** (detector `outcome_account`, type
   `outcome_distribution`). A designated target states its own
   distribution: numeric — median, mean, middle half, range,
   in the declared unit; categorical — the share of each level,
   and the declared event's share of all rows and its rate on
   the event basis.
3. **Per-level outcome contrasts with stated nulls** (detector
   `outcome_contrasts`, type `outcome_contrast`). For a
   categorical outcome with a declared event: each categorical
   grouping's per-level event rates against the pooled rate on
   the event basis (rows carrying the event level or the
   reference level — the target's modal other level), omnibus
   chi-square plus per-level two-proportion tests, two
   Benjamini–Hochberg families. Without an event, the weaker
   per-level share form. For a numeric outcome: per-level
   medians against the pooled median, rank-sum tests, the level
   contrasts' half-IQR entry bar. Every form sits behind a
   grouping-level omnibus corrected across groupings as its own
   family (chi-square for the categorical forms, Kruskal-Wallis
   for the numeric form): entry never rests on a single level
   or cell. Measured absences are stated
   as nulls at the same prominence — only when metadata was
   declared (the pointed mode): a categorical null requires
   omnibus p ≥ 0.10, every level's 95% interval containing the
   pooled rate, and half-widths ≤ 5 points; a numeric null
   requires every level's median within a quarter of the pooled
   IQR. The scoping is deliberate: the digest gate's permuted
   component counts any multi-column relationship statement on a
   column-shuffled table as false, and a null fires on shuffled
   data by construction, so nulls are the pointed mode's
   statements, not the bare pass's. When a numeric target is
   designated, the `level_contrasts` scan no longer also states
   it as a measure (the contrasts state it against the pooled
   outcome); the withholding is stated in that pass's coverage.
4. **The handover taught.** The packaged agent guide
   (`python/onceover/agent_guide.md`, 3,982 of its 4,000
   characters) now teaches declaring the outcome and the
   metadata first; its book chapter
   (`usage/talking_to_the_profiler.ipynb`) carries the same
   lists verbatim and its worked call declares the event, units,
   and a question. A new methodology chapter
   (`methodology/outcome_contrasts.ipynb`) states the basis
   rule, the entry bars, and the null discipline, with a worked
   example whose null disappears when the declaration is
   removed. `docs/findings.md` gains the two statement types
   (generated blocks regenerated by `scripts/findings_doc.py`)
   and the metadata keys in "Declared expectations"; the report
   schema gains `frame.declarations`.

## Selection

Findings answering a declared question (matched by the columns
the question names) are admitted ahead of unmatched findings of
equal selection score — the design's rule, and the only
selection change. The `questions` coverage entry states the
answer count per question, including zero. In the digest, the
outcome account leads the target section (after leakage) and
outcome contrasts form their own per-grouping buckets at the
same precedence tier as rules, ahead of statistical
restatements — the interestingness study's lexicographic order
(refusals first, then analysis-changing statements) applied
through the digest's existing precedence machinery rather than
a second ranking.

## Evaluation

### The state-unemployment panel

Handover, exactly as the design's worked example declares it:
target `unemployment_rate`; `time` = `date`; `entity` = `state`;
gloss for `state`, `date`, `unemployment_rate`; units
`unemployment_rate` = percent; `recorded_after` empty; purpose
"monthly monitoring of state labour markets"; the design's three
questions. Baseline (shipped 0.1.6): 1 of 7 classes stated.

| Class | Verdict | Evidence |
|---|---|---|
| 1. The outcome's own distribution | stated | "The designated outcome 'unemployment_rate' is numeric (in percent): median 5.3, mean 5.69, middle half 4.1 to 6.9, range 1.7 to 30.5, over 31008 values (51 missing)." Polars on the raw file: median 5.3, mean 5.6908, quartiles 4.1 and 6.9, min 1.7, max 30.5. The 51 missing values are October 2025 for every state; the digest also states the interior period (2025-10-01) as a gap. |
| 2. Drivers with measured strength | stated | The per-state contrasts below are the panel's driver statements (the frame has no other predictor: the date is the axis, the state is the entity), and the ordered partition's splits state the time drivers (first split at 2008-10-01, 41.7% of SSE). |
| 3. Per-level contrasts with stated nulls | stated | Twelve state medians against the pooled 5.3 (the stated cap; coverage states 12 statements): NE 3.0, SD 3.3, ND 3.4, NH 3.7, DC 7.3, AK 7.2, MI 7.1, WV 6.8, CA 6.7 as measured differences; IA 4.1, MS 6.5, NM 6.6 beside the pooled median. Every stated median equals the Polars group median. A month-of-year null is also stated (every month's median within the band, on a seasonally adjusted series); no state null arises, because the state grouping genuinely differs. |
| 4. Conditional extremes | not stated | The hotspots pass found no eligible columns: state × date cells are the grain itself, one row per cell, so no cell can concentrate. Nevada's 30.5 in April 2020 is carried by the account's range and the axis spike statements, not singled out as a conditional extreme. |
| 5. Slice failures | not stated | The model-failure pass ran and its coverage states the reason: no predictor columns are available for the target (the date is the axis, the state is the entity). |
| 6. Leakage, declared suspects first | discharged in coverage | No suspects were declared (the design's handover declares `recorded_after` empty). The screen ran: coverage states the strongest candidate, 'date' at 0.340, below the 0.80 bar — a stated negative. The declared-suspect path is exercised by the unit and Python tests. |
| 7. Breaks and shifts along the declared axis | stated | The baseline's one class, still stated: the ordered partition (8 regimes), two level shifts, a trend change, and the April/May 2020 transient excursions; the April spike reaches the 4,000-character digest. |

Four of seven classes are now stated as findings (baseline: one),
one is discharged as a stated negative in coverage, and two are
not stated for structural reasons the coverage itself states.
The 4,000-character digest's target section carries the account,
the April 2020 spike, the Nebraska contrast, and the
month-of-year null. Declared-question accounting: each of the
three questions matched 23 findings — the matcher works by the
columns a question names, and all three questions name the
panel's columns, so the counts coincide; the mechanism's
coarseness is recorded here rather than hidden. Validation
drops in this run: none.

### The Chicago inspections file

`~/workspace/demo-food-inspections/inspections_raw.csv`
(50,000 rows), target `results`, event `Fail`, purpose and one
question declared. The three binding statements of issue #308:

| Binding statement | Verdict | Evidence |
|---|---|---|
| The results distribution | stated, in the report and the digest | "Pass 53.4% (26,676 rows), Fail 17.7% (8,840), Pass w/ Conditions 12.5% (6,262), No Entry 7.2% (3,595), Out of Business 6.7% (3,361), Not Ready 2.5% (1,256)" — the answer key's 53.35 / 17.68 / 12.52 / 7.19 / 6.72 / 2.51, and the event: Fail on 24.9% of the 35,516 Pass/Fail rows (Polars: pooled 0.2489). |
| The fail-rate contrast by inspection type | stated in the report; its strongest line (Complaint) in the 4,000-character digest | "inspection_type = 'Complaint': the 'Fail' rate is 45.2% of 3563 rows carrying 'Fail' or 'Pass', against the pooled 24.9%" and "'Canvass': 27.3% of 17,441 rows" — the answer key's 45.2% and 27.3% (Polars: 0.4516 and 0.2726). |
| The risk-class null | stated as a null, in the report and the digest | "No difference in the 'Fail' rate across 'risk' was measured: 'Risk 1 (High)' 24.8% of 28,872, 'Risk 3 (Low)' 24.9% of 1,741, 'Risk 2 (Medium)' 25.3% of 4,901, against the pooled 24.9%" — the answer key's 24.8 / 25.3 / 24.9. |

Validation drops in this run: none. The digest result required
two digest changes, both recorded under "Selection" below: the
outcome contrasts initially reached the 8,000-character digest
but not the 4,000-character one.

### Classes still unstated, and why

Classes 4 and 5 of the design remain partly unstated, as the
brief expected. Conditional extremes (class 4) are stated only
by the existing hotspots and slice machinery where it applies;
nothing in this increment looks for an outcome's extreme values
under a conjunction of conditions. Slice failures (class 5) are
what the model-failure pass already states; the pointed mode
adds only the recorded-after exclusions to its inputs. Class
2's driver statements come from the existing screens
(association, predictability, target shapes) plus the outcome
contrasts; a numeric driver's strength against a categorical
outcome is still stated by those screens, not by the outcome
screen itself.

Two design effects were deferred, stated here so they are not
mistaken for done. The `units` declaration's second effect — a
constant-multiple contrast between two columns of declared-equal
units, stated as a possible unit error — is not built; exact
multiples are already stated as affine dependencies, and the
approximate-multiple screen does not exist. Gloss prose is the
sentence-append mechanism described above; per-level glosses
are applied by the outcome passes where they name levels.

## Gate

All legs on the final tree:

- `scripts/release.py check`: "Release metadata matches 0.1.6".
  `pytest scripts/test_release.py`: 6 passed.
- `cargo fmt --all -- --check`: clean.
- `cargo test --workspace --locked`: exit 0 (486 tests).
- Clippy with the 1.100 toolchain at `~/workspace/rustup-1100`
  (`cargo clippy --workspace --all-targets --locked -- -D warnings`):
  one error, in pre-existing test code this change does not
  touch (`hotspots.rs`, needless_range_loop). The identical
  command on a pristine reconstruction of main 526507a4 produces
  the identical single error, so it is an artifact of this
  toolchain bundle (its clippy reports 0.1.99 against rustc
  1.100.0); no file this change touches raises a clippy error.
- `python -m pytest tests/ -q`: 786 passed, 1 failed — the book
  chapter's stored `capabilities()` output, fixed by
  re-executing the chapter (below); `tests/test_agent_guide.py`
  then passes 24/24. New tests: 12 in `tests/test_pointed_mode.py`
  plus the updated registry pins.
- Corpus runner: exit 2, 220/220 machine-verified expectations
  (one re-baseline, below).
- Digest gate, `run.py` + `compare.py` against a base scorecard
  built from current main (526507a4) for this comparison — the
  scorecard recorded in the repo is 0.1.5-era and predates the
  briefing rebuild, so it cannot attribute deltas: **PASS**, 0
  worse, 23 improved, 9 accepted (below). Must-tier totals are
  unchanged or improved at every scope (must at 8k in target
  mode 16 -> 17); should-tier totals improve (target mode: 4k
  16 -> 18, 8k 21 -> 23, full 22 -> 24). Planted recovery at 4k
  improves 55 -> 56. Permuted false statements in target mode
  are unchanged in total (the omnibus gates below removed the
  new screen's contribution: on shuffled absenteeism-at-work,
  false relationship statements went 31 -> 8 with zero from the
  outcome screen, matching the base's 8). Cost total improves
  (31.073 -> 27.313 profile-seconds; an earlier head run shared
  the machine with a cargo build and read +10.8s — contention,
  not cost, as the idle rerun shows).
- Strict book build (`bash docs/book/build-html.sh`): exit 0.
  The notebooks this change touches were re-executed in place
  and pass `scripts/check_notebook.py`.

Two defects the gate exposed were fixed in this change rather
than accepted. The outcome screen's numeric and weaker forms
entered groupings on single levels or cells with no
grouping-level gate; both now sit behind an omnibus corrected
across groupings (Kruskal-Wallis for a numeric outcome,
chi-square for the weaker form), as the event form already
did. And the digest's target section let every grouping's
outcome contrast into its lead, crowding out the
model-failure slices (a must-tier statement on bank-marketing
regressed at 4k); the lead now takes two groupings' contrasts
and one level statement, the model-failure head competes in
the lead tier, and calendar-part contrasts key their own
digest facts instead of merging with the axis statements on
the same columns.

## Re-baselined expectations

Corpus registry (`benchmarks/corpus/registry.json`),
metro-interstate-traffic — the entry designates
`traffic_volume` as target, so its per-level contrasts moved
vocabularies:

- The `level_contrast` finding expectation (associations)
  becomes an `outcome_contrast` expectation (dependencies),
  same columns. The verified medians are unchanged (03:00 ->
  362, 16:00 -> 6,167); the basis changes from the pooled rest
  (3,559 / 3,221) to the pooled median (3,380), which the new
  claims state. The note records the move.
- The `max_findings` ballpark guard rises 89 -> 100: the
  outcome screen adds its account and twelve hour-of-day
  contrasts, and the report reaches the 100-finding cap. The
  note records the raise.

Digest gate (`benchmarks/gate/accepted.json`), nine entries,
each with its reason in the file:

- `planted.found_full` on absenteeism-at-work (limit 3) and in
  total (limit 58), and `planted.found_4k` on
  absenteeism-at-work (limit 2): the target_driver shape
  statement (interestingness 0.6289) falls just below the head
  selection cut (0.6387) on a report saturated at the
  100-finding cap, after the outcome statements on the planted
  target joined it. A marginal displacement at a saturated
  cap, not a screening loss: the statement's detector still
  produces it (it is in the base report at the same score),
  and found_4k in total improves.
- `redundancy.finding_permille_target` on six datasets (limits
  at the head values, dips of 0.3%-1.7%): the target sections'
  line composition changed — the outcome account and contrast
  lines are longer prose — while the metric's total rose.

Test pins moved with the registry, each the registry's own
growth: detectors 43 -> 45 (two target-triggered, on-request),
patterns 65 -> 67, (category, type) pairs 64 -> 66, forms
49 -> 56, hint keys 14 -> 21, triggers 38/2/3 -> 38/2/5. The
pins live in `crates/onceover-report/src/tests.rs`,
`tests/test_detector_defaults.py`, `tests/test_agent_guide.py`
(the target-triggered set and the benign hint-key dict), and
the prose counts in the guide, the `profile()` docstring,
`AGENTS.md`, and the book's usage chapters.
