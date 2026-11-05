# Verified relationship families: selection consolidation and top-k recall (issue #116)

Date: 2026-11-04. Change: schema 0.5 → 0.6. Base: main
`6744b05169492c5dd60e68caaa1b2377da149ad7`. Harness:
`benchmarks/family_evaluation.py` (this change), run before and after
on identical inputs; corpus: `benchmarks/corpus/run_benchmarks.py`.
The branch was later rebased onto main
`657123061d7e8ae6dd209774060ffddedf45c8ae` (which merged the #117
stage-1 streaming change and the wheel-CI test fixes); the evaluation
and the full corpus run were repeated on the rebased tree and every
number below was identical, per dataset and in aggregate.

## What the change does

Selection now consolidates statements that restate one exact relation
into a verified relationship family (the semantics are defined in
`docs/findings.md`, "Verified relationship families"). One lead
statement stays in `findings`; the restatements move to
`grouped_findings` as full records carrying `grouped_under`, spend no
selection slot and no digest slot, and remain retrievable by id
(`Report.finding(id)` resolves them). `selection` records `families`
and `grouped_statements`; the digest header states the grouped count
and the lead's rolled line carries the grouped ids.

Families form only over exact, unconditional statements (the producing
pass recorded zero violations; no conditions). Seeds: an exact
duplicate encoding gathers the exact FDs over its pair; an exact
rollup gathers the FD in its direction; exact identities and affine
dependencies over one column set are partitioned into rotation classes
by proportionality of their recorded coefficient vectors (weights and
intercept), and the FDs over the set attach to the first class family;
a set of exact FDs whose directions cover every column forms a family
on its own. Predictions are never members; scoped statements are never
members; distributional statements over a family's columns (a hotspot,
a column's role classification) are not implied by the exact relation
and stay independent.

## Protocol

The evaluation annotates the useful statements of three datasets from
records that predate this change — the Adult case study
(`~/workspace/unseen-test-adult/`, iteration 3 ground truth), the Bike
Sharing case study (`~/workspace/unseen-test-bike/`), and the
state-unemployment target account
(`~/workspace/research_notes/onceover-beijing-test-20261102/`) — and
ranks them in the reader's stream: the frame account statements first
(grain, then dominant structures, as the digest states them), then the
selected findings in report order. A useful item counts at rank k
when a matching statement appears within the first k stream
positions; grouped statements are not in the stream. The top-k
convention is the one `benchmarks/quality/battery.py` (issue #115)
uses. Runs use the corpus registry's own invocation per dataset
(Adult and Bike: standard budget, default detectors; unemployment:
the registry's declared detector subset with date parsing).

## Results

Selection accounting (candidates → distinct statements → selected +
grouped):

| dataset | candidates | distinct | before: selected | after: selected | after: grouped (families) |
|---|---|---|---|---|---|
| Adult | 73 | 64 | 62 | 60 | 2 (1) |
| Bike Sharing | 197 | 193 | 100 | 100 | 2 (1) |
| State unemployment (long) | 1 | 1 | 1 | 1 | 0 (0) |

Useful recall:

| dataset | useful items | top-10 before | top-10 after | top-25 before | top-25 after | stated before | stated after |
|---|---|---|---|---|---|---|---|
| Adult | 10 | 3 | 3 | 7 | 7 | 8 | 8 |
| Bike Sharing | 12 | 3 | 3 | 4 | 5 | 4 | 5 |
| State unemployment (long) | 5 | 4 | 4 | 4 | 4 | 4 | 4 |

Digest size at the 4,000-character budget:

| dataset | before: chars (lines) | after: chars (lines) |
|---|---|---|
| Adult | 3,874 (21) | 3,936 (21) |
| Bike Sharing | 3,925 (24) | 3,866 (23) |
| State unemployment (long) | 1,402 (10) | 1,402 (10) |

Recall is maintained on all three datasets and improves on Bike
Sharing. The acceptance bar — maintain or improve top-10 and top-25
on all three while the Adult encoding collapses to one digest family
with all supporting evidence retrievable — is met.

### Adult

The education family forms exactly as the issue describes: the
duplicate-encoding statement leads, and both functional dependencies
(`education → education_num` and `education_num → education`) are
grouped under it, retrievable by id. The lead presents at rank 8
(before: the encoding statement presented at rank 9, behind both FDs
at ranks 7–8, because the column-roles band follows the relations
band; the lead now presents at the family's earliest band). Two
selection slots are freed; the report holds 60 findings and 2 grouped
statements. The digest grows by 62 characters: the grouped-count
header line and the lead's `[grouped: …]` suffix are disclosure, and
the total stays inside the budget.

Not grouped, deliberately: the education hotspot (a distributional
statement about one cell's concentration, not implied by the
bijection), the `education_num` categorical-role statement (a
per-column classification that never mentions the partner column),
and the two prediction statements over the pair (a prediction is not
an exact mapping). The two useful items the report still does not
state are the ones the case study already records: per-column
missingness and the capital-gain top-code at 99,999 (issue #119
covers the top-code).

### Bike Sharing

The three FDs over `{cnt, casual, registered}` form an FD rotation
family (each column is the target of an FD on the other two); one
leads and two are grouped. The identity itself is not among the
candidates on this build — the multiterm scan does not reach the
triple within its candidate budget (issue #125) — so the family is
the FD signature of the relation, which is what the issue's
follow-up comment anticipates. The cap stays saturated at 100
selected, so the two freed slots admit the next two statements in
selection order; the first of them, the temp/atemp statistical
association, enters the reader's top 25 (rank 21), which is the
top-25 and stated improvement in the table.

`dteday → holiday` is still not selected, and consolidation is not
the reason. In an uncapped diagnostic run (selection cap raised to
100,000; 182 of 193 distinct statements selected), the six
`dteday → …` FDs sit at selection positions ~169–174: frame-model
step 2 demotes calendar derivations by a declared 0.6 factor, and
168 statements outrank them. The three exact rollups over `dteday`
(→ `mnth`, `season`, `weekday`, score 0.7063) also sit below the
capped cutoff (~0.76 at position 100), so their families resolve as
lead-not-admitted and their FDs remain independent — and unselected,
as before. The registry's documented case for `dteday → holiday`
keeps its reviewed status "missed" ("generated by the dependencies
pass but not selected"), which remains accurate. Whether calendar
derivations should rank higher is the step-2 demotion policy's
question, not a consolidation question; this change neither caused
nor cures it.

### State unemployment

The registry entry profiles with a declared detector subset
(`column_profile`, `ordered_partitions`) that produces no FD,
identity, or encoding statements, so grouping cannot engage; the
report is unchanged apart from the schema version and the two new
zero counts on `selection`.

## After merging main (2026-11-05)

The branch was merged with main `fa59cbe`, which added the identity search
rework (#125), the false-alarm fixes (#123, #124), missingness (#100), and boundary
mass (#119). Two statements above no longer describe the merged tree: the identity
over `{cnt, casual, registered}` is now among the candidates, so it leads its family
at rank 6 and all three FDs group under it, and the identity search states each
relation once, so identity rotations rarely reach this pass.

`benchmarks/family_evaluation.py` now also counts an item that matches a grouped
statement, at its lead's rank. Without this, `fd_triangle` read as absent once all
three of its statements were grouped.

| dataset | findings | grouped (families) | top-10 | top-25 | stated | digest chars (lines) |
|---|---:|---:|---:|---:|---:|---:|
| Adult | 69 | 2 (1) | 4/10 | 7/10 | 10/10 | 3,827 (20) |
| Bike Sharing | 100 | 3 (1) | 4/12 | 5/12 | 6/12 | 3,859 (23) |
| State unemployment (long) | 1 | 0 (0) | 4/5 | 4/5 | 4/5 | 1,402 (10) |

These numbers combine this change with the merged work, so they are not this
change's effect alone. Adult's `max_findings` guard moves to 69, and
`test_identity_family_retrievable` now builds a frame where FDs restate the identity,
since the scan no longer produces rotations to group.

## Corpus-wide outcomes

The corpus harness now records an explicit selection outcome for
every generated known-answer expectation (issue #116 acceptance):
`selected`, or `grouped` with the lead ids, alongside the standing
matched counts. A grouped match counts as found — the statement is
in the report, retrievable — and is never reported as selected; a
known-miss tripwire now requires absence from both `findings` and
`grouped_findings`.

Full corpus run (34 registered entries, 23 run, 11 skipped for
license gating), before and after: 23 ok / 0 fail both times;
machine-verified expectations 84/84 both times; documented cases
unchanged at 11 found / 19 missed / 5 partial; exit code 2 both
times (machine checks clean, documented misses on record). No
expectation's `ok` value changed anywhere. Eleven of the 23 datasets
form families, grouping 36 statements in total:

| dataset | findings before | findings after | grouped |
|---|---|---|---|
| chinook | 40 | 24 | 16 |
| adult-census-income | 62 | 60 | 2 |
| sec-edgar-company-facts | 75 | 73 | 2 |
| metro-interstate-traffic | 38 | 37 | 1 |
| soybean | 100 | 100 | 2 |
| taiwanese-bankruptcy | 100 | 100 | 3 |
| absenteeism-at-work | 100 | 100 | 2 |
| adventureworks | 100 | 100 | 3 |
| lace-animals | 100 | 100 | 1 |
| lace-satellites | 100 | 100 | 2 |
| bike-sharing-hourly | 100 | 100 | 2 |

On cap-saturated datasets the selected count holds at 100: the freed
slots admit the next statements in selection order. Chinook's 16
grouped statements are its one-to-one code/name column pairs
(customer, invoice, and track relations), each stated by a
duplicate-encoding lead with its FDs beneath it.

Expectation outcome changes — all of them `selected` → `grouped`,
none to missing:

| dataset | expectation | outcome after |
|---|---|---|
| adult-census-income | FD, target `education_num` | grouped under the duplicate-encoding lead |
| adult-census-income | FD, target `education` | grouped under the same lead |
| bike-sharing-hourly | FD, target `cnt` | grouped under the rotation family's FD lead |
| chinook | FD, target `billing_address` | grouped (both directions, under two pair leads) |
| metro-interstate-traffic | FD, target `weather_main` | grouped under the `weather_main`/`weather_description` encoding lead |

Registry changes: one. Adult's `max_findings` guard moves 62 → 60
with its note extended: the guard was tight by construction (actual
equalled the value), and the two grouped FDs are the whole delta, so
the tightened value preserves the guard's intent — a widening
selection is re-baselined deliberately. No expectation was weakened;
every other expectation's outcome is unchanged except the
selected → grouped moves the table records.

## Method note: rotation is tested on coefficients, not form labels

The multiterm scan labels the rotations of one relation
inconsistently: on a synthetic frame with `c = a + b` it reports a
`sum` toward `c` and fitted `linear_combination` statements toward
`a` and `b` (coefficients [intercept, −1, +1]). Form-label classes
would split that family. The ledger therefore tests rotation on the
recorded coefficient vectors: two linear statements over one column
set are rotations of one relation exactly when their (weights,
intercept) vectors are proportional (relative tolerance 1e-6, far
above the ~1e-12 fit noise the passes record). A relation with
different coefficients or a different intercept — and a product that
happens to hold alongside a sum — lands in a different class and
stays a separate statement; both directions of that rule are pinned
by unit tests (`linear_combination_rotations_group_with_the_sum`,
`distinct_relations_over_one_column_set_do_not_merge`).
