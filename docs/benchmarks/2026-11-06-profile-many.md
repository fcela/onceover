# profile_many: candidate foreign keys across tables

Date: 2026-11-06 (implementation and measurement)
Issue: #136 item 2 (multi-table inputs: candidate foreign keys through
inclusion dependencies, orphan rows, and value overlap). Item 1
(LazyFrame and file inputs to `profile`) shipped separately; item 3
(CLI and HTML report) is not part of this change.

## What was built

`onceover.profile_many({"orders": a, "customers": b})` examines every
ordered pair of columns from different tables and reports:

- **Candidate foreign keys**: pairs whose distinct non-null values are
  fully contained in a parent column that is unique over its non-null
  values. Candidates are reported in two tiers and never suppressed:
  *strong* when the column names agree after normalisation (case
  folded, underscores and spaces removed), and a lower tier for
  contained-only candidates, each carrying a **chance score** — the
  probability that a value set of the same size would land inside the
  parent's values by accident. Integer columns are priced against the
  child's own span (the dense-range accident: every `1..k` column sits
  inside every larger id column); other families are priced against
  every distinct value those columns use across the input tables.
  Both forms are hypergeometric ratios of binomial coefficients in
  log space, the same form as the FD chance correction in the native
  dependency pass; the universe differs because the accident differs.
- **Overlaps**: pairs into a unique parent whose shared distinct-value
  share reaches `overlap_threshold` (default 0.9) without full
  containment, with exact orphan counts (distinct orphan values and
  the non-null child rows holding them; nulls are never orphans) and
  example orphan values.

The result is a separate `MultiTableReport` object (`candidates`,
`overlaps`, `coverage`, `to_digest()`, `to_json()`,
`candidates_frame()`); the single-table report contract (schema 0.7)
is untouched, and `profile_many` does not profile the individual
tables. Candidate generation is exact verification behind pruning:
value-family match, unique parents only, a reachability prune on
distinct counts, then a strided probe of up to 4,096 child values
against a cached set of the parent's distinct values (a probe miss
proves non-containment; pairs probing more than 0.05 under the
overlap threshold are screened out). Reported shares and counts are
always exact. Coverage states the pair funnel and which columns were
not examined (same-table pairs, boolean columns, and columns of
unexamined types are out of scope).

## Protocol

`benchmarks/profile_many.py` runs four evaluations and exits nonzero
if the Chinook shape assertions fail:

1. **Chinook** (real database, original table and column names):
   11 tables, 64 columns, 15,607 rows.
2. **Planted orphans**: the Chinook Invoice table with three rows'
   `CustomerId` set to 9999, run against Customer.
3. **Planted chance containment**: a unique `grade` column holding
   1..50 and an unrelated `tier_code` column ranging over 1..40.
4. **Scale cases** (synthetic, seeded): random values (6 tables x 10
   integer columns, largest table 1M rows, 500k-value key spaces);
   a shared ID space (6 x 10 unique columns, each an 800k subset of
   1..1M, so every cross-table pair shares ~80% of its values); and
   true keys at scale (a 1M-key parent, five 900k-row children whose
   key columns sample the parent's keys).

Timings are single `perf_counter` runs around the `profile_many`
call on the project VM (4 cores), data generation excluded.

## Results

Chinook, 0.05 s: **112 candidates — 9 strong, 103 lower-tier — and no
overlaps.** The strong tier is exactly the 9 name-matched declared
foreign keys (precision 9/9), and all 10 declared cross-table foreign
keys are found. The tenth, `Customer.SupportRepId ->
Employee.EmployeeId`, sits in the lower tier with chance score 1.000:
its values {1, 2, 3} are a dense prefix of the employee ids, which
chance fully explains — the name disagreement is precisely why the
tier exists. 101 of the 103 lower-tier candidates score chance
>= 0.99 (dense integer ranges containing each other). The two
exceptions are string pairs that chance cannot explain:
`Invoice.BillingAddress -> Customer.Address` (chance 2.2e-141) and
`Invoice.BillingPostalCode -> Customer.PostalCode` (chance 1.9e-133)
— billing values copied from the customer record, real structure
under differing names. Pair funnel: 3,486 ordered cross-table pairs
considered, 1,540 sharing a value family, 712 with a unique parent,
370 size-pruned, 342 probe-screened, 0 needing verification beyond
the probe (every Chinook column has at most 3,503 distinct values,
under the 4,096 probe limit, so every probe was exact).

Two readings of the chance score matter. First, it prices the
containment, not the key: the strong `Album.ArtistId ->
Artist.ArtistId` also scores 1.000, because a dense 1..204 range
inside a dense 1..275 range is what chance produces; the values alone
carry no evidence, and the name agreement is the evidence. Second, a
score near 0 in the lower tier is a finding in its own right, as the
billing-address pairs show.

Planted orphans: the planted pair leaves the candidate list and
appears as an overlap with orphan_rows = 3, orphan_distinct = 1,
example [9999] — the planted values, exactly. Planted chance
containment: reported in the lower tier with chance score 1.0.

Scale: random-values case 2.0 s (no column is unique, so no pair is
ever probed); shared-space case 25.7 s with 0 candidates and 0 exact
verifications — the probe screen rejects every pair (true share ~0.8,
below the 0.85 screen gate), and the time is distinct-set extraction
and probe-set construction over 60 columns of 800k distinct values;
true-keys case 3.1 s, recovering all 5 planted keys in the strong
tier. For reference, an earlier prototype that probed pairs with
per-pair anti-joins instead of the cached-set probe screen took
~310 s on the shared-space case, and the scoping analysis measured
~30 s for a naive all-pairs scan of the random-values shape.

## Provenance

Chinook: the Chinook sample database (lerocha/chinook-database,
MIT), file `Chinook_Sqlite.sqlite`, sha256
`7651ba378ac2fcd0dfc3c66fb101f7a7eed3ba39a612ec642b96e20702061f15`
— the same pin as `benchmarks/corpus/data/business/chinook/
prepare.py`. Tables are loaded whole with original names; no values
altered. The declared foreign keys used as ground truth are the
database's own schema constraints (10 cross-table keys;
`Employee.ReportsTo -> Employee.EmployeeId` is an eleventh, same-table
key, out of scope by design and disclosed in coverage). Planted and
scale fixtures are generated in the script with fixed seeds; no
planted data is committed.
