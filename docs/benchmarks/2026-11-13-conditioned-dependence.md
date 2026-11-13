# Conditioned dependence: standalone statements and the vanishing form (2026-11-13)

## Question

The Simpson check (issue #134) tested whether a strong pair's
pooled direction reverses inside the levels of a third column,
and recorded the outcome only as a clause on the pair's own
claim. Two gaps followed. First, the conditioned fact was a
footnote: a reader scanning the report's findings never met it
as a statement in its own right. Second, the check had no way
to state the mirror-image pattern — a pooled association that
is absent inside every level, present only because the levels
differ in their means. That pattern is the standard signature
of an association produced by mixing unlike groups, and it is
the one a reader most needs named, because the pooled claim on
its own reads as a property of the variables.

This change adds a finding type, `conditioned_association`
(category `associations`), emitted by the two dependence
screens (`numeric_dependence`, `association_rules`) alongside
the existing clause and payload on the pair's claim. It has
two forms:

- **reversal** — the #134 outcome, unchanged bars: within every
  usable stratum the effect has the sign opposite to the pooled
  effect, with a measured magnitude.
- **vanishing** — new: the pooled effect is material, every
  usable stratum's effect is small, and the combined
  within-stratum effect is certified near zero by an
  equivalence bound.

## Bars for the vanishing form

Frozen before the real-data sweep, and implemented in
`crates/onceover-mines/src/simpson.rs`:

- Pooled materiality: |pooled effect| at least 0.30 for
  correlations (Pearson or Spearman, the statistic the pair's
  claim uses), or |pooled log odds ratio| at least 0.69 for a
  nominal 2×2 pair.
- Per-stratum absence: every usable stratum's |effect| at most
  0.15 (correlations) or 0.35 (log odds ratio).
- Equivalence bound on the combined within effect: |mean| +
  1.96 × standard error at most 0.10 on the Fisher-z scale
  (correlations) or 0.22 on the log-odds scale. The combined
  effect is the precision-weighted mean of the per-stratum
  effects.
- Usability, shared with the reversal check: at least 2 strata,
  at least 30 rows per stratum, the strata cover at least half
  the pair's rows. The two forms share the check's existing
  screen budget (400 stratum evaluations per screen run); the
  vanishing test runs only for a pair the reversal test did not
  already qualify.
- Statement score: 0.45 + 0.3 × |pooled effect| for the
  vanishing form, 0.45 + 0.4 × |within effect| for the
  reversal form, capped at 0.95. Both forms rest on estimates,
  so the findings are marked not checkable by the executable
  verifier, like the pair claims they qualify.

## Planted fixtures

The shipped tests (`tests/test_conditioned_association.py`,
plus the Simpson unit tests in `simpson.rs`) pin three
constructions:

- A planted reversal (two levels, within-level correlation
  exactly −1 in each level, pooled correlation strongly
  positive) produces a `conditioned_association` finding of
  form `reversal` in addition to the clause on the pair claim.
- A group-means-only construction (two levels whose means
  differ on both variables, no within-level association, 3,000
  rows per level) produces a finding of form `vanishing` and
  no reversal anywhere in the report.
- A homogeneous negative control produces neither form.

Two pre-existing tests in `tests/test_associations.py` encoded
the old emission shape and were updated to the new contract:
the group-mean guard now asserts that no *reversal* is stated
(the vanishing form is the designed outcome on that fixture),
and the 2×2 qualification test now counts the pair claim and
its standalone finding as the two records of one reversal.

## Coverage wording

The screens' coverage entries previously mentioned the Simpson
check only when a reversal had qualified. They now state the
check whenever at least one strong pair was examined: how many
strong pairs were checked, how many reversals and vanishings
were found, and whether the evaluation bound was reached. A
report that says "checked, found nothing" is distinguishable
from one where the check never ran.

## Real-data sweep

The eight evaluation frames, profiled at the standard budget
with the shipped build. The counts are the screens' own
coverage entries after the wording change above; runtimes are
end-to-end `profile()` calls on this machine.

| frame | strong numeric pairs checked | categorical pairs checked | reversals | vanishings | profile time |
|---|---|---|---|---|---|
| Airbnb NYC listings | 20 | 1 | 0 | 0 | 45.7 s |
| Appliances Energy, raw | 20 | 0 | 0 | 0 | 7.3 s |
| Appliances Energy, typed | 20 | 0 | 0 | 0 | 8.9 s |
| Adult | 0 | 0 | 0 | 0 | 2.7 s |
| Bike Sharing hourly | 9 | 0 | 0 | 0 | 2.8 s |
| Taiwanese Bankruptcy | 20 | 0 | 0 | 0 | 15.1 s |
| State unemployment panel | 0 | 0 | 0 | 0 | 0.1 s |
| Hotel bookings | 4 | 1 | 0 | 0 | 24.8 s |

Adult has no strong numeric pair (13 weak), and its one
strong nominal pair (relationship × sex, 6 × 2 levels) is not
a 2×2 pair, so the log-odds form of the check has nothing to
examine there. The panel has a single numeric column. Across
the remaining frames the check examined 93 numeric pairs and
2 categorical pairs and qualified none of them.

That is the result the bars produce on these frames, stated
plainly: the type fires on the planted constructions and is
silent on this real-data set. The strong pairs in these frames
are mostly direct measurement relations (one sensor reading
against another, a count against its total), where a third
categorical column does not manufacture or mask the pooled
effect. The vanishing bars are also deliberately hard to meet:
the equivalence bound requires the combined within effect to
be certified below 0.10 with the frame's own sample sizes,
which rules out "too small to see" as well as "absent". A
wider claim — that the type earns its place on confounded
observational data generally — is not established by this
sweep and is not made here. What the sweep establishes is the
negative control at scale: 95 pair examinations on real frames
produced no false statement of either form.

## Provenance

Frames: Inside Airbnb New York listings (snapshot 14
September 2026, CC BY 4.0); UCI Appliances Energy Prediction
(the UCI repository copy used by the evaluation set); UCI
Adult (DOI 10.24432/C5XW20); UCI Bike
Sharing (DOI 10.24432/C5W894); UCI Taiwanese Bankruptcy
Prediction (DOI 10.24432/C5004D); the state unemployment panel
(BLS LAUS, corpus copy); hotel bookings (TidyTuesday mirror,
2020-02-11, of Antonio, de Almeida & Nunes, "Hotel Booking
Demand Datasets", Data in Brief 24 (2019), CC BY 4.0).
