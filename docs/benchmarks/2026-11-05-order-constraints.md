# Order constraints: the `order_constraints` detector (issue #132)

Sortedness states the order of one column against the row
sequence; functional dependencies state equality mappings.
Neither states the row-level order between two columns — that an
end precedes its start, a part outweighs the whole, a discharge
predates its admission. Those are among the most common real
data errors, and they are invisible to the association screens:
a 0.5% violation rate barely dents a correlation. This detector
states the relation directly, exactly or with its exceptions
counted.

## The rule

Candidate pairs share a value kind: numeric with numeric, dates
with dates, datetimes with datetimes of the same unit. A pair is
a candidate only when all of these hold:

- each column carries at least 100 non-null values and at least
  5 distinct values (below that the pair is a levels question
  for the dependency passes, and a constant column would make
  the statement one about a single column's range);
- the marginal ranges are compatible: they overlap by at least
  25% of the narrower range, and their widths are within 100x.
  When one column's maximum does not exceed the other's
  minimum, `a <= b` is a fact about the two marginal ranges,
  not about how values pair up in rows; when widths differ by
  orders of magnitude the columns measure different quantities
  and any near-universal order is a scale artifact;
- comparisons are exact: integers compare as integers, floats
  as floats, and a mixed integer/float pair compares as floats
  only when the integer column is exactly representable in f64
  (|v| <= 2^53).

Each direction is screened on a deterministic sample of 4,096
rows when the table is large (survival bar: violations at most
2% of the sample's complete rows) and verified on all rows
before it is reported. A direction is reported when its
violations are zero or at most 1% of the rows where both
columns are present (at least 100 such rows). At most one
direction per pair is reported — the one with fewer violations.
A direction must also beat random pairing of the two columns:
the violations that pairing would give, computed from the two
sorted columns and stated in the evidence as
`violations_if_shuffled`, must be at least 20 and at least 4
times those observed. A heavy-tailed column against a
non-negative one has a compatible range and almost no crossings
under any pairing; before this rule, independent columns of that
kind filled the 25 finding cap on null frames.
When both directions pass with equal violations the pair states
no order: zero violations in both directions means the columns
are equal wherever both are present (a duplicate-encoding
statement), and equal nonzero counts mean the columns cross as
often as they agree.

Pairs are evaluated in column order on the shared pair budget
(`max_pairs`, the budget that also caps the dependence screen:
496 pairs at standard, 2,016 at deep, 0 at quick, where the
pass records itself skipped). Coverage states how many of the
eligible pairs were evaluated, how many directions were
reportable, and how many findings were emitted, and is marked
capped when the budget or the findings cap binds. At most 25
findings are emitted, ranked:

1. directions with violations (the data-error signal), by
   violation share, then rows evaluated;
2. exact orders between columns with no functional relation;
3. exact orders between columns an exact single-column
   functional dependency ties in either direction in the same
   run — the order may follow from the derivation, so the
   evidence carries `functionally_related: true` and the
   finding scores below the other two classes (0.80 / 0.62 /
   0.45).

The evidence states the direction, the evaluated rows, the rows
where either column is missing, the violation count and share,
the tied rows, both columns' observed ranges over the evaluated
rows, and for a direction with violations the first ten
violating rows, the first and last violating rows, and the
worst violation with its row and both values.

## Evaluation

Baseline, measured before the change on the issue's protocol
plants (5,000 rows, 3 seeds, the pair in both column positions,
against the library as it stood): no pass stated any of the
three planted relations — `lo <= hi` exact, `lo <= hi` violated
on 0.5% of rows, `end >= start` violated on 0.5% of rows — 0 of
3 plants in every seed and position.

Unit-level results (Rust, this change): the pass's tests cover
an exact order firing with zero violations; a 0.5%-violation
order firing with its violating rows retrievable; identical
columns stating no order; independent columns staying quiet; a
range-disjoint pair rejected as a marginal fact; a functionally
related pair scored below an unrelated exact order and below a
violations finding; the pair budget bounding evaluation with
the bound visible in the outcome counts; and a datetime pair
with violations firing with its worst violation stated. Full
crate results: onceover-core 15 passed, onceover-deps 35
passed, onceover-report 28 passed, 0 failed.

Remaining measurements — the planted protocol and controls
through the Python API, the corpus run with every fire
recomputed independently, runtime on the widest corpus frame,
and the activation decision those measurements determine — are
recorded here as they land.
