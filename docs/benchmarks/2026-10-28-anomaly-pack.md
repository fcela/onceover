# Benchmark corpus: anomaly pack seed (2026-10-28)

Protocol: same harness, standard budget, onceover 0.1.2 at main
1cd09b28. Run record: `benchmarks/corpus/runs/2026-10-28-anomaly.json`.

## The scoring mapping (issue #24's design problem, v1)

Findings are not anomaly scores, so the mapping has to be explicit.
For labeled time-series streams the harness implements
`anomaly_coverage`: a labeled point is *covered* when a finding in
the changes, gaps, or intervals categories carries a timestamp in
its evidence within +/-24 hours of the point. The tolerance and the
category set are part of the expectation, stated in the registry.

Known limits of v1, recorded rather than hidden: NAB labels are
points standing for windows; evidence timestamps are extracted from
the finding JSON, so a finding that localizes an event only by row
position does not count; and nothing in v1 scores tabular
(binary-label) datasets yet — that half of the mapping waits for
ADBench members to clear the per-dataset license audit.

## First subjects: two NAB streams (bundled, MIT)

| stream | points | findings | labeled coverage |
|---|---|---|---|
| nyc_taxi (10,320 rows) | 5 | 3 | 0 / 5 |
| ec2_request_latency_system_failure (4,032 rows) | 3 | 3 | 0 / 3 |

The breaks pass scanned both series (coverage `ran_sampled`) and
reported nothing; the only findings are uniqueness/column-role/
ordering observations. The labeled events in nyc_taxi are transient
demand collapses around known events (marathon, Thanksgiving,
Christmas, New Year, blizzard) — hours-long dips, not sustained
level shifts, which is what the current changes machinery looks
for. Both expectations are registered with `min_covered: 0` as
known-miss baselines: the day a detector starts covering these
windows, the run record shows it and the baseline gets raised.

**Update 2026-10-28:** that day came with the transient-excursion
scan in the breaks pass (issue #32). Coverage is now 5/5 on
nyc_taxi (10 excursion findings; the unlabeled ones are the
July 4 and Labor Day collapses and a Dec 12 spike) and 3/3 on
ec2_request_latency (exactly 3 findings). Both baselines are
raised to the measured values. See
2026-10-28-transient-events.md.

This is the second measured instance of the corpus's recurring
shape (the first was the GritBot pack): broad structure is found;
localized events inside the structure are not.

## Suites registered, not wired

ADBench, ODDS, and macrOData have registry entries with status
notes. ADBench's datasets are distributed outside its repository
tree and aggregate many original sources, so members clear the
license gate individually; ODDS's original distribution point is
moribund and its members are mostly re-derivations of primary
sources this corpus prefers directly; macrOData admission waits on
the mapping proving out beyond two streams.
