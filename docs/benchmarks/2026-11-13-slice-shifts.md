# Slice shifts: acceptance against the measured prototype protocol (2026-11-13)

## Question

No pass stated how a column behaves inside a subset of the rows
that nobody declared: the share of extreme values, the median,
or a level's share inside a conjunction of conditions, against
the table as a whole. The group contrast compares a subset the
caller declared; the pair screens relate two columns over all
rows. Issue #272 adds the searched form — a conjunction of
categorical levels and numeric thresholds, found by the pass and
stated only when the shift is confirmed on held-out data — as
the opt-in `slice_shifts` detector (findings of type
`slice_shift`, category `associations`).

The design was measured before it was built, as a Python
prototype over six datasets (the protocol and its results are
summarised below from the measurement record). This record is
the acceptance run of the Rust implementation against that
protocol: the same six datasets, the same preparation, the same
bars where the design carries over, a permutation control per
dataset, and the differences between the two search spaces and
control stacks stated explicitly.

## The prototype protocol and its results

Slices: a categorical level (depth 1), or two categorical
levels, or one level and one numeric threshold — the value above
the column's 75th or 90th percentile (depth 2). Minimum support
max(50, 1% of rows). Tail shares use all-rows denominators (a
null response counts as not extreme). Slices conditioning on the
response are excluded. Per dataset the prototype searched a
curated set of condition columns and responses, plus one binary
target per dataset where one exists.

Pattern 1 (numeric responses): a hit is a slice whose share of
rows above the response's global 95th percentile is at least
0.15 (base rate 0.05), or whose median differs from the global
median by at least 0.75 IQR. Across the six datasets: 5,907
slice × response candidates, 332 hits, 0 hits under permutation
of the responses. Split-half holdout of each dataset's top 10
hits (held-out tail share at least 0.10 and at least half the
discovery share) confirmed 59 of 60. 310 of the 332 hits are
depth-2; depth 1 finds 22, and on Appliances 0 of 23. The
strongest median-form hits on Bank are definitional — the
distribution of age within job = retired — true and replicable,
a ranking problem, not a detection failure.

Pattern 2 (binary target, level shares): a hit is a slice whose
rate is at least twice or at most half the global rate, differs
by at least 0.02, and has at least 5 expected positives. Across
the four datasets with a target: 554 hits, 0 under permutation.
Strict split-half confirmation (same direction, held-out
|log ratio| at least half the discovery |log ratio|, held-out
ratio at least 1.4 or at most 0.71) confirmed 26 of 40 top hits;
the 14 failures were slices whose discovery rate was exactly 0,
and under a zero-tolerant criterion (held-out ratio at most
0.08) all 40 confirmed.

Per-dataset prototype counts (candidates / pattern-1 hits /
pattern-2 hits): Airbnb 978 / 97 / 42; Appliances 144 / 23 / —;
Bike 1,473 / 89 / 148; Bank 1,515 / 47 / 77; Adult 1,692 / 66 /
287; Panel 105 / 10 / —.

## The shipped design, and where it departs from the prototype

The bars carry over unchanged: support max(50, 1% of the
sample) with at least 25 rows in each half; tail share at least
0.15 or median shift at least 0.75 IQR on the search half; level
shares at ratio at least 2 or at most 0.5, difference at least
0.02, expected positives at least 5. Confirmation bars: tail
share at least 0.10 and at least half the search-half share (the
prototype's holdout criterion); median shift at least 0.5 IQR in
the same direction (the prototype's holdout checked the tail
form only; the shipped pass confirms each form in its own
terms); level-share ratio at least 1.5 or at most 0.5 with
difference at least 0.01, and a search rate of exactly 0
confirmed when the held-out ratio is at most 0.25 — the
zero-tolerant rule the prototype's strict run motivated (its
bar was 0.08 on the top-10 check; 0.25 is looser, and the
Benjamini–Hochberg term below is the discipline that replaces
the tighter bar).

Four departures, each deliberate:

1. **Confirmation is exhaustive, not top-10, and carries a
   multiplicity term.** The prototype confirmed its top 10 hits
   per dataset. The shipped pass confirms every candidate that
   clears the search bars, on a deterministic split of the shared
   sample, and requires a Benjamini–Hochberg q ≤ 0.05 over that
   family, from held-out p-values (Mann–Whitney for numeric
   targets, two-proportion z for rates). The candidate count is
   stated in the coverage entry and in each finding's evidence.
2. **The search space is the whole eligible table, not a
   curated subset.** Every bounded-cardinality categorical
   column (at most 25 distinct values on the sample) contributes
   levels, every numeric column contributes thresholds at its
   75th and 90th percentiles, every numeric column is a target,
   and every eligible categorical column's levels are targets —
   under caps of 12 condition columns, 12 threshold columns, 24
   numeric targets, and 8 categorical target columns, on the
   shared sample (20,000 rows at the standard budget, 50,000 at
   deep). Candidate counts are therefore one to two orders of
   magnitude larger than the prototype's and are not expected to
   reconcile with them; what reconciles is the protocol outcome
   (next section). One consequence runs the other way: the
   prototype allowed 60 levels for the panel's state column,
   while the shipped level cap of 25 excludes a 51-level column
   from conditions and targets, so on the panel only era and
   threshold slices are searched.
3. **Selection is part of the pass, because the yield is.**
   The prototype reported every hit (332 and 554 — its own
   record calls the rate-form yield the binding problem).
   The shipped pass deduplicates slices with identical row sets
   (one statement, shallowest description), demotes a deeper
   slice whose row set is at least 90% a shallower kept slice's
   for the same target, ranks by absolute effect with support —
   a rate effect counts the absolute difference equally with the
   log ratio, so a rate of exactly 0 against a small global rate
   does not outrank a large shift on a common level — and states
   at most 20 findings, at most 8 per target column. The
   per-target cap was added during acceptance: on a frame with
   two identical columns, 83 confirmed candidates were mostly
   the rate-0 restatements of that one duplication, and they
   filled the statement cap ahead of the planted finding.
4. **Depth 3 exists at the deep budget**, as a beam: depth-2
   slices that cleared the search bars are extended by one
   further categorical level. The prototype searched depth at
   most 2. The standard-budget record below does not exercise
   depth 3; the shipped tests exercise the beam path on a
   fixture (33 slice definitions examined against 28 at depth at
   most 2).

Findings are `not_checkable` for `report.verify`: the statistics
are measured on the pass's sample and split, which a recount on
the full frame reproduces neither. The slice definition is
carried as structured predicates in the statement definition.
Where a numeric slice concentrates the target's top-1% values
(at least 3 times its row share, at least 8 such values), the
claim adds that share — the share-of-extremes statement as an
evidence form inside the slice statement.

Placement is opt-in, following the project's convention for this
cost and inference class (views, changeforest): the pass is the
heaviest in the registry on wide frames and its statements are
the only ones whose selection is itself the risk the evidence
has to answer for.

## Acceptance run: the Rust implementation on the same datasets

Protocol: `profile(df, detectors=["slice_shifts"])` at the
standard budget on each dataset prepared as in the prototype
run (Airbnb: price parsed from its string form, `is_superhost`
derived; Appliances: space-padded numeric strings stripped and
cast, `daypart` and `is_weekend` derived from the timestamp;
Bike: calendar codes cast to strings, `peak_demand` derived at
the 75th percentile of `cnt`; Bank: `subscribed` derived;
Adult: strings stripped, `high_income` derived; Panel: `era`
derived from the date). The permutation control shuffles every
column independently — the prototype shuffled its responses and
target, which were its whole target set; the shipped target set
is every eligible column, so the null has to break every
column's association with every other. Wall times are the
end-to-end profile calls on this machine (release build) under
shared load; a second set of runs under heavier contention
measured up to 2.3× these times on the widest frame.

| dataset | rows | candidates | cleared search | confirmed | stated |
| --- | ---: | ---: | ---: | ---: | ---: |
| Airbnb | 29,918 | 173,477 | 35,947 | 28,362 | 20 (11 median, 9 level share) |
| Appliances | 19,735 | 4,542 | 1,252 | 1,198 | 20 (20 tail) |
| Bike | 17,379 | 60,111 | 13,403 | 12,246 | 20 (12 tail, 8 level share) |
| Bank | 45,211 | 30,155 | 3,846 | 3,279 | 20 (12 level share, 8 tail) |
| Adult | 48,842 | 52,655 | 13,028 | 11,288 | 20 (20 level share) |
| Panel | 31,059 | 9 | 1 | 1 | 1 (1 level share) |

Permutation control, same runs: Airbnb 169,962 candidates,
1,929 cleared the search bars, **0 confirmed, 0 stated**;
Appliances 4,590 candidates, 0 cleared; Bike 63,067 candidates,
858 cleared, **0 confirmed**; Bank 30,502 candidates, 97
cleared, **0 confirmed**; Adult 54,394 candidates, 490 cleared,
**0 confirmed**; Panel 9 candidates, 0 cleared. The search bars
alone pass hundreds to thousands of slice × target pairs on
structureless data; the split-sample confirmation and the BH
term together pass none of them, on any of the six frames — the
prototype's permutation outcome (0 hits), under a search space
roughly 50 times wider.

Wall times (real runs): Airbnb 18.7 s, Appliances 1.7 s, Bike
4.8 s, Bank 2.7 s, Adult 5.1 s, Panel 0.07 s. Cost tracks the
candidate count, not the row count: the panel's 31,059 rows
admit 9 candidates inside the level cap and finish immediately,
while Airbnb's 90 columns produce 5,159 slice definitions and
173,477 candidates on the 20,000-row sample. The work budget
(300 million cell visits) did not bind on any of the six; a
stopped run would report `capped` coverage with the examined
share of the slice definitions.

Reading the confirmed counts: confirmation is not the binding
constraint on real data (79–96% of search candidates confirm) —
the statement caps are, by design. The strongest statements are
the expected ones. On Bank, the leading statements are the
retired slices — the share of job = retired inside older rows
without a housing loan (48.9% against 5.1%), and the age
distribution inside job = retired, the prototype's definitional
case, stated and left to the reader. On Adult, the
top statements are the education ↔ education_num pair (100%
against 5.2%): an exact encoding, which the functional
dependency detector states in its own terms in a full profile.
On Airbnb, the top statement is a median shift of estimated
occupancy inside high-revenue listings (median 255 against 0,
4.25 IQR). The prototype's headline slices are inside the
confirmed families; whether each reaches the stated 20 depends
on the ranking, which is the recorded trade of departure 3.

## Fixtures

The shipped regression tests (`tests/test_slice_shifts.py`)
plant each form at a known strength on 4,000-row frames: a
depth-2 tail slice (segment B ∧ store channel, 40% extreme
rows), a depth-1 median shift built so the tail form cannot
present it (another segment carries the frame's extremes), a
level share (45% against 8% elsewhere), and a rate of exactly 0
in both halves by construction, confirmed under the
zero-tolerant rule. A shuffled-target frame states nothing. Two
identical condition columns produce exactly one statement about
the planted row set, at depth 1. Every stated finding's evidence
shows the diagnostic-holdout validation with both halves'
row counts, and `report.verify` marks every slice statement
`not_checkable`.

## Provenance

Datasets: Inside Airbnb NYC listings, snapshot 2026-09-14
(CC BY 4.0), local copy; UCI Appliances Energy Prediction,
local copy; the corpus copies in `benchmarks/corpus/data/` of
UCI Bike Sharing (275; DOI 10.24432/C5W894), UCI Bank Marketing
(222; DOI 10.24432/C5K306), UCI Adult / Census Income (2; DOI
10.24432/C5XW20), and the state unemployment panel (BLS LAUS
via FRED, long form). Prototype protocol and results: the
2026-11-12 practitioner-EDA measurement record (patterns 1 and
2), summarised in the second section; the shipped bars and
departures are recorded in `docs/findings.md` ("Slice shifts")
and the methodology chapter of the same name.
