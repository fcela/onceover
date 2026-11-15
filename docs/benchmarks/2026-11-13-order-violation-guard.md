# Order violations on unordered pairs — 2026-11-13

The order pass stated `order_violation` findings for column
pairs that have no order in either direction. This record is
the before/after measurement for the guard fix: the
containment route to a part-against-whole statement now
applies the systematic statement's asymmetry term, and the
part-against-whole share band narrows from 5–40% to 5–25% of
the evaluated rows.

## The defect

The systematic form of the statement requires the reverse
direction's violations to number at least twice the stated
direction's (`SYSTEMATIC_ASYMMETRY`). The part/whole form
dropped that term on both of its routes. The identity route
certifies the pair directly — the whole minus the part is an
exact sum of other columns — so it needs no such term. The
containment route certifies much less: both columns respect
a shared zero floor on at least 99% of rows, and the slack
(the whole minus the part) reaches the floor on at least 5%
of the held rows but on fewer than half of them. Two
independent columns over {0, …, 4} satisfy both conditions
by accident: they share the floor 0, and they are equal —
slack zero — on about a fifth of rows. With no asymmetry
term, either direction of such a pair could be, and was,
stated as a broken bound.

## Protocol

Two fixtures of independent uniform columns over {0, …, 4},
profiled with the default budget; the count is the number of
findings whose type is `order_violation` or
`order_constraint`.

1. Three columns `a`, `b`, `c`, 5,000 rows
   (`numpy.random.default_rng(1)`).
2. Thirty-two columns, 5,000 rows
   (`numpy.random.default_rng(7)`).

## Results

Fixture 1, before the fix: two `order_violation` findings.
`a <= c` violated on 1,936 of 5,000 rows (38.7%), reverse
2,022 (40.4%); `b <= c` violated on 1,949 (39.0%), reverse
2,034 (40.7%). Both directions of each stated pair fail at
nearly the same rate; neither is an order.

Fixture 2, before the fix: 25 `order_violation` findings
among 57 findings in total.

After the fix: zero order findings on both fixtures (fixture
2 reports 32 findings, none of them order findings). The
fixtures are regression tests in
`tests/test_order_unordered_pairs.py`, with a scan-level
counterpart in the `onceover-deps` order tests.

Genuine statements are unchanged: the order test suites pass
unmodified apart from the new tests (Rust: the contained-part
containment statement at 24% violations with the reverse at
2.85×, the identity-route statements, and the systematic
statements; Python: the part/whole and order-constraint
suites). The corpus run on the fixed tree exits 1 with
exactly the three machine-verified failures already on
record (thyroid, adventureworks, bike-sharing-hourly) and no
others: 90 of 93 machine-verified expectations pass, as on
the base tree.

## What the fix does not cover

A pair can pass the asymmetry term and still have no real
order: a column pair ordered by construction in one
direction only in a loose sense (the issue's example pairs
two unrelated measurement columns) needs a declared-order
resolver, which is separate work. This fix removes the
symmetric case — pairs whose two directions fail comparably
— and narrows the share band so a stated direction fails on
at most a quarter of the evaluated rows.
