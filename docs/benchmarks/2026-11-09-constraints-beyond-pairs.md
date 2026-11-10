# Constraints beyond pairs: wider identities, systematic order violations, one-way flags (2026-11-09)

## Question

Issue #206 reports three constraint shapes the shipped passes miss,
all from one real analysis: a total that is the exact sum of four
component columns (the multi-term identity scan enumerates subsets
of two and three predictors only); a count column that exceeds the
count bounding it on 2,408 rows (order constraints certify a
direction only at zero violations or at most 1% of evaluated rows,
and are silent past that share in both directions); and one-way
flags on an entity × time panel (no absorbing-state check exists).
This run records the before/after behaviour of the three additions
on synthetic cases built to the reported shapes, and the corpus
contract under the change.

## Protocol

- Builds: the unmodified 0.1.4 tree (baseline) and the issue-#206
  branch build, each installed with `maturin develop --release`
  into its own virtual environment; nothing else differed.
- Entry point: `onceover.profile(df)` at the default budget unless
  a case says otherwise.
- Case 1 (identity): a 240 × 50 person-week panel (12,000 rows).
  A total column is split into four components by Dirichlet shares
  of a near-constant budget, the fourth component the exact
  remainder, so the total is the sum of the four on every row; a
  scaled copy of the total (× 1.07, rounded) and two small decoy
  counts are present. Control: the same panel with one component
  raised by 50 on 5% of rows (a near-sum).
- Case 2 (order): 10,000 paired count rows constructed so the
  accepted count exceeds the generated count on exactly 2,408
  rows, falls below it on 6,500, and ties on 1,092. Controls: two
  unrelated counts crossing both ways at similar rates (3,144 /
  3,214 / 3,642 ties), and a different-scales pair (an age in
  years against a duration in seconds; the age exceeds the
  duration on 12.5% of rows, the reverse fails on 87.5%).
- Case 3 (flags): a 120 × 52 person-week panel. One flag switches
  on for 60 of the 120 entities partway through and never off; an
  activity measure is zero before each onset and first turns
  positive exactly 2 periods after it (with variation across
  entities). A second flag switches both ways within entities; a
  third is on from the first period for every entity. Control: a
  flat frame with no time axis.
- Corpus: `python benchmarks/corpus/run_benchmarks.py` on both
  builds; per-dataset finding claims were also diffed directly on
  the three datasets whose counts moved during development.

## Results

Case 1: the baseline reports no multi-term identity on the total
(its coverage records 448 multi-term candidates screened and one
identity, elsewhere). The branch build reports
`total_tokens = chat_tokens + completion_tokens + embedding_tokens + image_tokens`,
exact on all 12,000 rows, and still reports nothing on the
near-sum control. The widened search is a components-first chase:
candidate parts are the columns lying between zero and the target
on at least 99% of sampled rows (at most 24, by mean share of the
target), parts are subtracted from the target on the 512-row
sample until the residual vanishes (up to 8 parts, no fitted
coefficients), and the assembled form is verified on all rows
under the pass's existing exactness standard.

Case 2: the baseline reports nothing on the pair. The branch
build reports one `order_violation`: 2,408 of 10,000 evaluated
rows (24.1%), the reverse order violated on 6,500 rows (65.0%),
with the violating rows in the evidence. The rule: the
less-violated direction fails on at least 20 rows and at least 5%
of evaluated rows, the reverse direction fails at least twice as
often, and the pair tracks its diagonal on a comparable scale —
median |a − b| / max(|a|, |b|) over sampled complete rows at most
0.2 (this case: 0.038) and column medians within a factor of 2.5
when both are non-zero. Both controls stay silent.

The scale terms are load-bearing, and the corpus is the evidence.
With the count, share, and asymmetry terms alone, the rule also
fired on 16 pairs across three corpus datasets — an age in years
against a contact duration in seconds, a days-since-contact
column carrying a 999 sentinel against a day-of-week column, and
abalone size measurements in different units — with pair
distances of 0.247 to 0.491. Those pairs cross asymmetrically but
do not track a diagonal; no bound between them is being broken.
With the scale terms at the stated bounds, all three datasets
return to their baseline finding sets exactly, and the canonical
case above still fires.

Case 3: the baseline has no one-way-flag statement (its frame
account does factor the panel as 120 person × 52 week). The
branch build reports one `one_way_flag`: the flag switches on
and never off within person, 60 of 120 entities show an onset, no
on-to-off transition occurs, and the associated activity measure
gives a lag of median 2 periods (middle half 1 to 3). The
both-ways flag and the always-on flag produce no finding; the
coverage record states why the both-ways flag is not one-way.
On the flat control frame the coverage record for the check is
`skipped`: no entity × time grain.

Corpus: 93/93 machine-verified expectations pass on the branch
build, and the documented cases are unchanged from the baseline
run (found 18, missed 10, partial 4; the run's exit code is 2 in
both runs — machine checks clean, documented misses on record).
No registry expectation was updated: no documented case's
findings changed. Direct claim diffs on abalone,
south-german-credit, and bank-marketing — the datasets whose
counts moved before the scale terms were added — show zero new
and zero lost findings against the baseline build in the final
build.

## Cost

The identity chase adds one bounded pass per target column at
the tiers where the multi-term scan already runs (it stays off at
`quick`): a pool of at most 24 candidate parts and at most 8
subtraction steps on the 512-row sample, then the usual all-rows
verification of any assembled form. The order scale gate is one
stride sample of at most about 1,024 complete rows per verified
pair. The flag scan walks each flag column once per candidate
entity column (at most three) on frames with a time axis.
