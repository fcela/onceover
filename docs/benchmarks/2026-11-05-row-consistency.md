# Row consistency: near duplicates, swapped values, rows off a line (issue #132)

The `row_consistency` detector states three things about individual rows that the
column and pair passes do not. It adds no statement on the 30 null frames, and on the
corpus it states the weight-only repeats in Adult and the restatements in SEC EDGAR,
both previously on record as misses.

## The rules

**Near-duplicate rows.** Each row gets a wrapping sum of salted per-column value hashes.
Subtracting one column's hash gives the row's key without that column, so all columns
can be tested in one pass over the rows per column. A statement names the column whose
removal makes rows repeat. It requires at least 5 extra rows and 0.1% of the table, at
least 3 other columns, and at least half the rows distinct once the column is removed,
which keeps coarse tables where almost every row repeats out. Exact repeats are
subtracted, so the count is the rows that differ only in the named column. At most 3
statements per table. Tables above 50,000,000 cells are not checked, and coverage says
so.

**Rows off a line.** Pairs with |Pearson r| of at least 0.9 (256 at most, tightest
first) get a least-squares line, refitted after trimming at 3 robust scales. Rows are
flagged at a residual of 8 robust scales or more, at most 5 rows or 0.5% of the table.
The largest ordinary residual must be at most 0.6 of the smallest flagged one, so that
the flagged rows are separated from the rest and do not continue a fat tail.

**Swapped values.** For each pair of measures, a column's usual range is its middle 90%
on a sample, widened by half its width on each side. Pairs with disjoint usual ranges
are candidates (5,000 at most). A row counts when each value lies in the other column's
range. A statement requires at least 2 such rows, at most 5% of complete rows, and at
least 80% of the rows where either value moved into the other's range. A tail value of
one column can reach the other's range by chance, and then the other column stays put;
that last bar is what separates this from a swap.

**Three-column keys.** The functional-dependency pass now searches composite keys when
a unique float is present, because a measured float that happens to be unique says
nothing about the row's identity. Floats are left out of composite keys, and a pair or
triple is accepted only when its cells number at most n²/20, so random columns do not
form a key by chance.

## Corpus

Machine-verified expectations: 89 of 89. Promoted from misses to found:

| Table | Statement |
|---|---|
| Adult census income | 6,322 rows repeat another row on every column except `final_weight` (12.94%). The documented 6,374 excess rows, less the 52 exact repeats. |
| SEC EDGAR company facts | 1,970 rows repeat another row on every column except `value` (17.07%), the restatements. |

Other statements, reviewed by hand:

| Table | Statement | Reading |
|---|---|---|
| Metro traffic | 217 rows repeat except `weather_description` | One hour recorded under two descriptions (`light rain` and `moderate rain`). |
| Satellites | 79 rows repeat except `ID` | Mostly satellites of one constellation, identical on every attribute (Iridium 39 and Iridium 43). |
| Splice | 172 rows repeat except `instance_name` | The same base sequence under two names (`HUMTNFA` and `HUMTNFX`). |
| Bike sharing | 24 rows sit far from `atemp = 0.8841 * temp + 0.0372` | All 24 rows are 2012-08-17, where `atemp` stays at 0.2424 while `temp` reaches 0.86. |
| Absenteeism, voting, soybean | Rows repeat except one column | True. These tables have few levels per column, so the statements carry less information. |

`swapped_values` makes no statement on the corpus. Before the 80% bar it made 74 on
Taiwanese Bankruptcy and others on hospital-dha, thyroid and Metro, all unrelated
columns with a tail reaching the other's range. In tests it recovers planted swaps
within one row of the planted count: a swapped value in the outer part of a column's
distribution can fall outside the widened range.

## Order constraints beat random pairing

The null control for this work found `order_constraints` stating `c15 <= c18 on 1999 of
2000 evaluated rows` between independent columns. A heavy-tailed column against a
non-negative one has a compatible range and almost no crossings whichever way the rows
are paired. A direction now has to beat random pairing: the violations random pairing
would give, computed from the sorted columns and stated as `violations_if_shuffled`,
must be at least 20 and at least 4 times those observed.

Null frames (5 seeds per shape, 30 frames):

| Frame | Mean findings before | Mean findings after |
|---|---:|---:|
| 300 × 12 | 9.0 | 4.2 |
| 2,000 × 30 | 33.6 | 9.0 |
| 20,000 × 30 | 32.6 | 8.0 |
| 5,000 × 100 | 52.2 | 27.2 |
| 2,000 × 30, dated | 36.0 | 11.4 |
| 5,000 × 30, dated | 33.8 | 10.6 |

`order_constraint` findings over the 30 frames fell from 634 to 0. `row_consistency`
made none before or after.

On the corpus the statements a reader would expect remain: `casual <= cnt`,
`registered <= cnt`, `Perigee_km <= Apogee_km`, `Dry_Mass_kg <= Launch_Mass_kg`,
`period_end <= filed`, `freight <= subtotal`, and the abalone measurements. Tables of
strongly correlated columns still fill the cap of 25 with orders that hold on nearly all
rows, such as `ks_unemployment_rate <= ak_unemployment_rate` on 607 of 608 rows in state
unemployment. Those are true and far from random pairing, and they say less than a
constraint between a start and an end does.

## Order constraints on large tables

The pass read every value of every numeric column into memory and evaluated pairs in
column order until the budget of 496 ran out. On the 2,000,000 × 200 fixture that took
17.8 s, reached only the first 32 columns, and accounted for most of the memory peak. It
now keeps a 4,096-row sample of each column, screens every eligible pair on that sample
in parallel, nearest columns first, and loads whole columns only for the pairs that
survive. The screen covers up to 64 times the pair budget (31,744 pairs at standard),
and at most 496 survivors are verified on all rows.

2,000,000 × 200, 4 threads, full release build, one run each:

| | Before | After |
|---|---:|---:|
| `order_constraints` | 17.82 s | 0.45 s |
| `row_consistency` | 0.67 s | not re-read; outside the top 8 passes |
| Whole profile | 63.0 s | 44.2 s |
| Peak memory added by profiling | 5.24 GiB | 1.53 GiB |
| Pairs screened | 496 of 19,110 | 19,110 of 19,110 |
| `late_x <= late_affine` (columns past 190) | not stated | stated |

The benchmark's existing checks all pass, including the plants past column 64, and
`late_order` is added to them. `row_consistency` does not check near-duplicate rows on
this fixture, because 400,000,000 cells is above its 50,000,000 limit, and coverage says
so. On 1,000,000 × 40, which is within the limit, it takes 0.54 s of a 5.35 s profile.
