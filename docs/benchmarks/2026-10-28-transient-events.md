# Transient events in time series (issue #32)

Date: 2026-10-28. Library change: the `breaks` detector gains a
transient-excursion scan; new finding type `transient_excursion`
(category `changes`, form `excursion`). No new detector id — the
scan runs inside the existing breaks pass, over the same
time-ordered series.

## Why

The anomaly pack (2026-10-28-anomaly-pack.md) scored the breaks
pass at 0 of 5 labeled events on NAB nyc_taxi and 0 of 3 on NAB
ec2_request_latency. The labeled events are transient — hours to a
couple of days — and the changes machinery targeted sustained
level shifts, so the events fell between passes. The taxonomy
(family 2) ranked this the second detector gap.

## Method

For each time-ordered numeric series the breaks pass already
scans:

1. Baseline. When the median interval between timestamps divides
   a week evenly (at least 48 steps per week) and the series spans
   at least 2.5 weeks of periods, the baseline for a point is the
   median of the values at the same time of week in the three
   neighbouring weeks on each side. Neighbours are matched by
   timestamp, not by index offset: the pass works on a stride
   sample of long series, and an index offset drifts out of phase
   as soon as one grid point is missing. Otherwise the baseline
   is a centered rolling median over 97 points.
2. Deviation is scaled by the median absolute residual
   (× 1.4826). When at least half the series sits exactly on its
   baseline (quantized series), the scale falls back to 1% of the
   series' p01–p99 range; a constant series has range 0 and is
   skipped.
3. Points beyond 8 robust z are grouped into events, bridging
   gaps of up to 12 unflagged points. An event's span is capped
   at one week of points (seasonal baseline) or half the rolling
   window (local baseline); a sustained level shift produces a
   longer run than the cap and is left to the level-shift scan.
   At most 10 events per series are reported, ranked by peak
   deviation.

Each finding carries the event start, end, and peak timestamps,
the peak value, and the peak deviation in robust z.

## Measured effect

NAB nyc_taxi (10,320 half-hourly points): coverage 0/5 → 5/5,
from 10 excursion findings. The five labeled events are the
Christmas demand collapse (peak z −18.7), the New Year spike
(+23.8), the blizzard collapse (−20.1), the marathon spike
(+15.5), and Thanksgiving (two findings, −14.4 and −14.2). The
remaining findings are the July 4 collapse (−14.6), a Dec 12
spike (+12.7), the Jan 2 dip (−12.5), and the Labor Day collapse
(−12.3) — real calendar events the label set does not mark.

NAB ec2_request_latency (4,032 points at 5-minute cadence):
coverage 0/3 → 3/3, from exactly 3 excursion findings, one per
labeled failure (peak z +31.1, −12.8, −8.0). Two of the three
events are latency drops, not rises: during the failures the
metric falls away from its baseline, and the scan is two-sided.

Controls. Thyroid has no time column; nothing fires. On the
state-unemployment panel (51 monthly series, 16 scanned under the
pass's series cap), every excursion finding is the COVID shock:
onset 2020-04-01 in each reported state, peak z 17–33, durations
of 3–13 months against the panel's 51 years. No excursion fires
on a stationary stretch of any corpus series.

Registry change: both NAB `anomaly_coverage` expectations move
from the `min_covered: 0` baseline to the measured values
(5 and 3). Full corpus run after the change: every bundled
dataset's expectations pass.

## Limits

- Series under ~100 points are not scanned (the breaks pass's
  existing minimum), and the pass scans at most 16 series per
  table at the standard budget.
- Events longer than the span cap are treated as regime changes,
  not excursions; at monthly cadence the cap is 48 points, so a
  multi-year elevation would be split between this scan and the
  level-shift scan rather than reported once.
- The seasonal baseline needs a cadence that divides a week;
  daily series (7 points per week) use the rolling median and
  keep their day-of-week structure in the residuals.
- One series at a time: an excursion visible only in a ratio or
  difference of two series is out of reach.
