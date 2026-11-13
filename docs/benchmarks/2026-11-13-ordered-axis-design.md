# Ordered-axis battery: design record for the shape and break statements (#273, #277)

Benchmark/design record, design phase only — no detector code. Prototype scripts and full results live with the study record; the measured claims below are from the panel and Bike Sharing runs summarised in §1.2 and §3.


Design phase. This document settles the statistic battery, the frozen
bars, the axis-eligibility rules, the deduplication rule, and the
fixture plan for the two ordered-axis enhancement issues. It is backed
by a Python + Polars prototype (`ordered_axis_prototype.py`, results in
`ordered_axis_prototype.json`, same directory) run on the
state-unemployment panel and Bike Sharing in this session. No detector
code is specified to implementation level here; the phase after this
one implements against the fixtures in §6.

## 0. What the measurements force

Four measured facts from the empirical phase (state-unemployment panel,
51 states × 609 months, 1976-01–2026-09) drive every choice below:

1. A global single level-break scan puts **0 of 51** states' best break
   in the 2008 or 2020 windows. The scan optimises level SSE over 50
   years, so it finds the largest persistent level difference (1980s
   declines), never the recent event. A level scan alone cannot be the
   battery.
2. The national **trend** break lands at **2008-11** (two-segment /
   one-segment SSE ratio **0.602**).
3. A **fixed-window regime contrast** (mean 2009–10 vs mean 2006–07)
   fires in **48 of 51** states (national contrast +4.2 points,
   shuffle z = 7.9); a **spike contrast** (Apr–Jun 2020 vs the same
   months of 2019) fires in **51 of 51** states (NV +20.6, national
   +8.21 points).
4. Per-slice timing differs genuinely: NV's best level break is at
   **2008-07**, CA's at **2015-04**, TX's at **2014-04** (recomputed in
   this session's prototype: shifts +1.96 / −1.90 / −1.64 points,
   |t| = 8.9 / 9.7 / 13.5).

And one hazard: on the 4.5-month Appliances window, a level scan mostly
detects the winter→spring transition — on a short axis a scan cannot
distinguish seasonality from a regime change (§2.3).

Conclusion: the ordered-axis pass is a **battery of statistics with
different event shapes**, plus a deduplication rule so one event is
stated once. No single scan is defensible.

## 1. The battery

Common conventions. The *family* for multiplicity is all statistics ×
responses × axes evaluated in one profile. Within a scan, multiplicity
over candidate split points is handled by calibrating the scan's
maximum statistic against a shuffled-axis null (the same device the
drift detector already uses: permute, re-run the whole scan, compare
maxima). Across the battery, candidate statements that survive their
own statistic's bar go through Benjamini–Hochberg over the family
before the dedup rule (§1.2) clusters them; dedup runs last so the
report never pays for the same event twice, and the per-(response,
axis) statement cap is **3 events**.

Axis-point floors below are stated for a series aggregated to the axis
grain (e.g. one value per month, per dose, per hour). "Segment floor"
is the minimum number of axis points on each side of a candidate break.

| # | Statistic | Statement form (example, real names) | Null / control | Floors and bars (frozen) |
|---|---|---|---|---|
| S1 | Level scan (exists; widened per §2) | "unemployment_rate in Nevada steps up after 2008-07: mean 5.86% before, 7.82% after." | Shuffled-axis max-\|t\| null; statement only if the observed max clears the null max | axis ≥ 48 points; segment floor max(12, 10% of axis); effect ≥ 0.5 pooled sd **and** \|t\| ≥ 8 on long axes (prototype bar) |
| S2 | Trend scan (exists as two-segment fit; restated as a battery member) | "Nationally, unemployment_rate's trend steepens after 2008-11: slope −0.008 → −0.024 points/month." | Shuffled-axis null on the SSE ratio | axis ≥ 48; segment floor max(12, 10%); SSE ratio ≤ 0.90 and slope change ≥ 0.25 × residual sd per axis step |
| S3 | Fixed-window regime contrast (new) | "unemployment_rate averaged 4.2 points higher in 2009–2010 than in 2006–2007 (48 of 51 states show the same contrast ≥ +1.0)." | Shuffle null on the same two windows (z from ≥ 20 shuffles) | windows pre-registered by the axis calendar (e.g. 24-month windows either side of a candidate boundary), **not** searched; contrast ≥ materiality bar (response-specific, 1.0 point on the panel) and z ≥ 4 |
| S4 | Spike contrast (new) | "unemployment_rate spiked in Apr–Jun 2020: +20.6 points in Nevada against the same months of 2019." | Season-matched baseline (same calendar window, prior cycle) plus shuffle z | spike window ≤ 6 axis points (months) / one season; jump ≥ 3 points on the panel (scale-free form: ≥ 1.5 × the series' IQR) and z ≥ 4; requires the post-window mean to revert toward baseline for the word "spike" — otherwise the event is a level shift and S1 owns it |
| S5 | Monotonicity with named reversal blocks (#273) | "cnt rises with hour through the morning peak, then reverses at 08:30 (bin medians drop 169 rentals, 0.70 IQR) — the two-peak day." | Shuffled-axis run must produce no monotone statement; reversal sizes calibrated against bin-median noise | ≥ 200 rows, ≥ 12 populated bins; "monotone" licensed at \|Spearman ρ\| ≥ 0.5 with isotonic improvement ≥ 0.5; a reversal block is named when an adjacent bin-median move against the trend exceeds 0.25 IQR; a reversal > 0.5 IQR reclassifies the pair as non-monotone with a located reversal |
| S6 | Saturation with located knee (#273) | "casual rentals rise with temp and saturate above temp ≈ 0.66: beyond the knee the slope is under a quarter of the rise." | Shuffled axis silent; near-linear pairs rejected by the slope-ratio bar (§3) | §3 bars, all of them, including bootstrap stability |
| S7 | Segmented-regression slope break (#277) | "The slope of unemployment_rate on time within California changed at 2015-04" — general form: "the slope of price on volume halved after 2019-03 (0.84 before, 0.39 after; breakpoint range 2019-03–2019-07)." | Shuffled-axis null on the segmented-fit SSE gain; breakpoint *range* = all splits within 3.84 residual variances of the best (the existing breaks convention) | axis ≥ 48; segment floor 15% of axis (Bai–Perron trimming h = 0.15), min 12; single break in v1; break count by BIC only in the multi-break stretch; slope change material: \|slope_after − slope_before\| ≥ 0.5 × the pooled slope's standard error × 4 (frozen as: the two segment slopes' confidence intervals must not overlap) |
| S8 | Per-slice break (#277) | "The 2020-04 level break in unemployment_rate exists in region = north only; the pooled series and region = south are continuous through it." | Shuffle null inside each slice; BH across the level × response family | §4: slice support ≥ 200 axis-rows and ≥ 48 axis points inside the slice; the break must be absent (below bar) in the pooled series, or located > 18 axis points away from the pooled break |

Notes on individual members:

- **S3's windows are not searched.** A regime contrast with searched
  windows is a level scan with extra steps and inherits the winner's
  curse. The windows come from the axis calendar (fixed widths anchored
  at candidate boundaries proposed by S1/S2, or at calendar boundaries)
  — in the prototype the 2006–07 / 2009–10 windows are the pre-registered
  pair around the 2008 event, and the statistic's job is confirmation
  and effect size, not discovery.
- **S4 vs `transient_excursion`.** The existing detector states short
  events against a local baseline with a binomial cluster test. S4 is
  the same event shape at the *series* level with a season-matched
  baseline and a reversion requirement; if both fire on the same
  window, dedup (§1.2) keeps one statement.
- **S5's reversal is the finding.** On Bike Sharing, cnt ~ hour has
  ρ = 0.51 but the quotable fact is the located reversal at 08:30
  (0.70 IQR). A bare "monotone" claim on that pair would be wrong in
  the way that matters; the grammar is monotone-with-named-blocks or
  non-monotone-with-located-reversal, never a bare ρ.
- **S7 is S2 generalised**: S2 is the special case where the regressor
  is the axis itself (trend). S7 runs the segmented fit with an
  arbitrary screened regressor. They share code and the dedup cluster.

### 1.2 Deduplication: one event, one statement

Firings from different statistics that describe the same event must
collapse. Rule (frozen):

1. Each firing carries a location (break date, knee, or window start)
   and, for window statistics, a window.
2. Sort firings by location; cluster by chaining: two firings are in
   the same cluster when their locations are within the **merging
   radius** (18 axis points on a monthly axis; in general
   max(6 points, 3% of the axis length)) or their windows overlap.
3. One statement per cluster. The primary form is chosen by fixed
   priority — **spike (S4) > regime contrast (S3) > trend / slope
   break (S2/S7) > level scan (S1)** — because the earlier forms are
   the more specific descriptions of an episode (a spike is not a
   level shift; a sustained regime contrast is a better description
   than a bare slope change). The non-primary firings attach to the
   statement as corroborating evidence, named in the evidence block.
4. A trend/slope firing inside a cluster is promoted to primary when
   no level-form statistic (S1/S3/S4) fires in the cluster — a pure
   slope change with no level event is its own statement.
5. Dedup is within (response, axis, slice). Per-slice firings in
   different slices are different statements (capped by the family
   cap), not duplicates of each other.
6. Cross-detector: clusters are computed after the existing breaks /
   changeforest / ordered_partition findings for the same
   (response, axis) are known; a battery cluster whose location falls
   inside an existing finding's breakpoint range attaches as
   corroboration instead of restating. (Implementation must thread
   this through the report's existing finding-key dedup; the exact
   keying is an implementation decision, the radius rule above is the
   semantic one.)

**Prototype validation (this session).** Battery = S1 + S2 + S3 + S4
at the frozen bars, run on the national series and all 51 state series
(`ordered_axis_prototype.py`):

- National series: 4 firings (level scan best at 1987-06, shift
  −1.83 points, t = −13.4; trend break at 2008-11, SSE ratio 0.602;
  regime contrast; spike contrast) → **3 statements** after dedup: the
  2008 cluster (regime contrast primary, trend break as corroboration),
  the 2020 spike, and the 1987 level shift.
- All states: **197 firings → 149 statements**. Firings by statistic:
  trend break 51, level scan 50, spike contrast 49, regime contrast
  47. 47 multi-member clusters in total; **in 39 of 51 states** a
  multi-member cluster sits in the 2007–2011 window — the same event
  described by two or more statistics, stated once. Primaries chosen:
  spike 49, regime 47, level 42, trend 11 (the 11 trend primaries are
  clusters where no level-form statistic fired, per rule 4).
- Shuffled-axis control (one full shuffle per state series, same
  battery, same bars, same dedup): **0 firings, 0 statements** across
  all 51 series. The battery is silent on unordered data at these bars.

## 2. Axis eligibility

### 2.1 Recognition, in precedence order

1. **Hint.** A declared axis/order hint always wins (the existing
   `time` hint; #273 adds an `axis`/`order` hint naming any column).
2. **Datetime / date dtype.**
3. **Integer-year-like** columns (existing rule: integer dtype, values
   in a plausible year range, near-complete coverage of the range).
4. **Sequence columns** (new under #273): integer columns that are
   strictly increasing in row order (≥ 99% of adjacent pairs) — treated
   as an axis for shape statements (S5) and for drift-along-the-sequence
   statements; break scans (S1/S2/S7) run on them only when the axis
   has ≥ 48 distinct values.
5. **Declared ordinal** (new): a categorical column becomes an ordered
   axis only through a hint carrying its level order. Alphabetical or
   frequency order is never inferred as semantics.
6. **Any numeric column as a shape axis** (new, #273's core widening):
   S5/S6 (shape statements) run over numeric axis × measure pairs
   screened the way associations are screened — shape is a property of
   the pair, so no semantic inference is needed. Dose, age, income,
   mileage, temp all enter here without being declared.

Break scans (S1/S2/S7/S8) widen beyond time axes as follows: they run
on axes of kinds 1–4, and on kind-6 numeric axes when the response can
be aggregated to ≥ 48 distinct axis values with ≥ 5 rows per value at
the median (a dose grid, an age range). Below that support the axis is
shape-only.

### 2.2 Which existing passes widen

| Pass | Widens to | Note |
|---|---|---|
| breaks (level/trend/variance) | axis kinds 1–4 + supported kind-6 | currently kinds 1–3 (date/datetime/integer-year) |
| series_structure (trend, cycle) | kinds 1–4 | cycle stays time-only (a cycle needs a calendar period) |
| ordered_partition | kinds 1–3, then 4 | node model is level+trend in the axis; non-time axes admitted once the breaks widening is proven on fixtures |
| drift | stays time/declared-axis only | its statement is about extracts over time; widening it to dose makes no sense |
| shape (new, S5/S6) | kind 6 by construction | — |
| step_discontinuity | already any numeric axis | unchanged; it is the cross-sectional sibling, and dedup (§1.2 rule 6) must not merge its statements with S1's — different claim (jump across a threshold in another column vs break along an axis). Same (response, threshold-location) pairs are attached as corroboration only when the axis *is* the discontinuity column. |

### 2.3 Short-axis seasonality hazard rule (frozen)

If the axis is time-like and its span covers **fewer than 2 full
cycles** of the dominant seasonal period (detected period, or the
calendar period implied by the grain: 12 for monthly, 7 for daily,
24 for hourly), then:

- S1/S2/S7 (scans) are **silent** on that axis. On the Appliances
  window (4.5 months of 10-minute data) the level scan's best break is
  the winter→spring transition; two cycles is the minimum span at
  which a scan can separate a level shift from a seasonal swing.
- S3/S4 (contrasts) may still fire, because their baselines are
  season-matched by construction (same calendar window, adjacent
  cycle) — a spike against the same months last year is not confounded
  by the season.
- Shape statements (S5/S6) on sub-cycle axes are unaffected (they are
  not time claims), but a monotone claim along a sub-cycle *time* axis
  must carry the span in its evidence.

## 3. Saturation: method and the wording bars for "saturates"

**Knee location.** Equal-frequency binning of the axis (20 bins,
minimum 10 rows per bin), bin medians of the response; a continuous
two-segment linear fit on the bin medians (count-weighted), knee
searched over interior bin centres with ≥ 3 bins on each side; the
knee is the SSE-minimising join point. The fit is on bin medians
because the statement is about the typical response, and binning is
what makes the fit robust at profiler scale; the empirical phase's
isotonic figures (computed the same way) are the precedent.

**The word "saturates" is licensed only when all of these hold**
(frozen):

1. Direction and association: Spearman ρ ≥ 0.30 in the stated
   direction (mirrored for decreasing saturation).
2. Shape gain: the two-segment fit improves on the one-line fit by
   ≥ 10% of SSE on the binned medians.
3. Relative flatness: post-knee slope ≤ 0.35 × pre-knee slope. (An
   absolute flatness bar — total post-knee rise ≤ 0.25 IQR — was
   prototyped first and **rejected**: genuinely saturating curves
   whose plateau still creeps, e.g. an exponential approach, fail it
   at every knee. Flatness is a ratio claim; the plateau's absolute
   rise is reported in the evidence, in IQRs, so a reader can see how
   flat "flat" is.)
4. Interior knee: the knee lies within 10–90% of the observed axis
   range, with ≥ 3 populated bins beyond it.
5. Stability under resampling (100 row bootstraps): ≥ 70% of
   bootstraps classify the pair as saturating under bars 1–4, **and**
   the interquartile range of bootstrap knee locations is ≤ 15% of
   the axis range.

If bars 1–4 hold but 5 fails, the licensed wording is the weaker
"rises, then flattens" with no located knee. If bar 3 fails, the pair
is not a saturation candidate at all, whatever the knee's stability —
location stability alone does not license the word (see the control
below).

**Prototype validation (this session).** A corpus scan for a real
saturating pair came up empty, and that is a result: among Bike
Sharing's temperature responses, bin-median slopes over the last 40%
of bins *exceed* those over the first 60% (slope ratios: cnt 1.56,
casual 3.19, registered 1.25 — the response accelerates into the warm
tail over the observed range, it does not plateau), and the Appliances
temperature responses are near-linear (T1 slope ratio 2.14, T2 2.12 on
the same split). Validation therefore pairs a planted fixture with
real negative controls:

- **Planted fixture** (Bike Sharing scale): x resampled from the real
  `temp` distribution (n = 17,379), y = 500·(1 − exp(−temp/0.25)) +
  N(0, 60). The bars accept: ρ = 0.693, improvement 0.937, slope ratio
  0.24 (slopes 752 → 180 per unit temp), knee located at temp = 0.40
  against the curve's tangent-at-origin knee of 0.25 — the method
  locates the bend region, not the analytic point, and the design
  wording is "saturates above ≈ 0.40", a range claim. Stability:
  100/100 bootstraps classify saturating; bootstrap knee IQR = 4.1%
  of the axis range (bar: ≤ 15%). **Pass.**
- **Real near-linear control**: Appliances T1 ~ T_out (ρ = 0.649). The
  knee fit is *stable* (bootstrap knee IQR = 2.0% of range) but the
  slope ratio is 2.16 — the "plateau" is steeper than the rise — so
  bar 3 rejects it in 100/100 bootstraps. **Silent, as required**, and
  the case that proves bar 3, not bar 5, is the load-bearing one for
  linear data.
- **Shuffled-axis control**: Bike Sharing cnt ~ shuffled temp:
  ρ = 0.007, improvement 0.047, knee IQR = 24.5% of range. Silent on
  every bar. **Pass.**

## 4. Per-slice breaks (#277, second half) and the boundary with ordered_partition

**Slice family (frozen).** One categorical column at a time, one level
at a time; levels with ≥ 200 rows and ≥ 48 axis points inside the
level; cardinality ≤ 25 levels (the screen's existing cap). No pairs
of dimensions, no bipartitions, no searched subsets of levels.

**Statistic.** Inside each level's own series (response aggregated to
the axis grain within the level): the S1 level scan and S2 trend scan,
same bars, shuffle null computed inside the slice. A slice statement
additionally requires the *exclusivity* condition: the pooled series'
scan does not fire at a location within the merging radius (§1.2) of
the slice's break. If the pooled series fires at the same location,
the event is global and the slice statement would restate it.

**Multiplicity.** BH across the level × response family actually
evaluated, with the candidate count in coverage — the enumerable-family
bookkeeping the group-contrast patterns already use.

**Boundary with ordered_partition (precise).**

- ordered_partition owns **joint** statements: a recursive tree over
  axis × dimension-level *bipartitions*, fitted on the pooled frame,
  node model level + trend; a leaf is a path of conditions, and the
  finding answers "which partition of dimensions × periods best
  explains the response".
- Per-slice breaks own **marginal** statements: one pre-declared
  categorical column, one level, that level's own series against the
  pooled series' break profile. No tree, no bipartitions, no fitted
  partition of the frame.
- Suppression rule: if ordered_partition splits on the same dimension
  and a leaf boundary lies within the merging radius of a per-slice
  break on that dimension, the per-slice statement is suppressed as a
  restatement of the leaf. The converse does not hold (a marginal
  slice break never suppresses a partition leaf — the leaf is a joint
  claim).
- Panel entities are excluded from the slice family: the entity key
  belongs to `entity_break`. Per-slice applies to ordinary categorical
  columns, including non-entity dimension columns of a panel.
- Worked separation: "the 2020 spike in unemployment_rate is a Nevada
  statement" can arise three ways — as an entity_break (NV is the
  entity), as a per-slice break (state treated as an ordinary
  categorical on a non-panel extract), or as a partition leaf
  ({NV, HI, MI} × post-2020 period). On a panel grain the first owns
  it; on a flat table the second; the third is stated only when the
  partition's joint fit is itself the finding. The implementation must
  pick by grain, not by which pass runs first.

## 5. What is deliberately out of scope for v1

- Joint multi-break segmentation (gap G5) beyond the single-break
  forms: the battery states one event per cluster; a jointly estimated
  segmentation as one statement is the stretch goal named in #277,
  sequenced after the battery exists.
- Calendar-part contrasts (G6) and per-group shape (G7): both ride on
  machinery specified here (S3 windows / S5 per slice) but are separate
  issues' scope, not #273/#277.
- Variance-along-axis (G8): `variance_change` widened by §2.2 covers
  the time-axis form; the fan-out statement on arbitrary axes is not
  in either issue's filed scope.

## 6. Fixtures to build before implementation

All fixtures are small synthetic frames with planted parameters; each
has a pass bar that the implementation must meet before the detector
ships. Negative controls are fixtures, not afterthoughts.

| Fixture | Shape | Planted parameters | Pass bar |
|---|---|---|---|
| F1 planted trend break | 1 series, 600 monthly points; response = 5 + trend | slope −0.01/month before point 400, −0.05/month after; noise sd 0.8 | S2/S7 fires; located break within ±6 points of 400; slopes recovered within 20% |
| F2 planted regime window | 1 series, 600 points | level +2.0 sd during points 300–420 only, reverts after | S3 fires with the window contrast within 15% of planted; S1's best break is *not* stated as a permanent level shift (reversion routes it to S3/S4 grammar) |
| F3 planted spike | 1 series, 600 points, monthly with a seasonal cycle | +4 sd for 3 consecutive points at a known location, season-matched baseline clean | S4 fires at the planted window (±1 point); the word "spike" used (reversion holds) |
| F4 slice-only break | 2 slices × 400 points on one axis | level break +1.5 sd at point 250 in slice A only; pooled series diluted below bar | S8 fires for slice A, silent for slice B, silent pooled (exclusivity condition exercised) |
| F5 monotone with reversal | axis 0–24 (hour-like), 5,000 rows | response rises to a peak at 40% of the axis, drops 0.7 IQR in one bin step, rises again | S5 states non-monotone with the reversal located within 1 bin of the planted drop; not stated as monotone |
| F6 saturating | axis 0–1, 10,000 rows | y = 500·(1 − exp(−x/0.25)) + noise sd 60 (the prototype fixture, frozen as a fixture spec) | S6 says "saturates", knee within the bend region (0.2–0.5), stability bars pass |
| F7 linear control | axis 0–1, 10,000 rows | y linear in x, slope ratio of any two-segment fit ≈ 1, noise matched to F6 | S6 silent (bar 3 rejects) |
| N1 shuffled axis | F1/F6 frames with the axis values permuted | — | entire battery silent |
| N2 pure seasonality, short axis | 100 points = 1.4 seasonal cycles, no break | sinusoid + noise | scans silent per §2.3; no level/trend statement |
| N3 pure noise | 600 points, iid noise | — | entire battery silent |
| N4 two events, one series | 900 points | level shift at 300, spike at 700 | exactly 2 statements after dedup, one per event, correct primary forms |

## 7. Prototype summary and limits

Validated in this session (`ordered_axis_prototype.py`,
`ordered_axis_prototype.json`): the dedup rule and battery silence
(§1.2: 197 → 149 statements on the panel, 0 under shuffle), and the
saturation bars (§3: planted fixture accepted and stable at knee IQR
4.1% of range; near-linear and shuffled controls silent).

Not validated here, by design: S7's regressor form on a real
non-time regressor pair (the prototype's trend scan is the
axis-as-regressor special case; the fixture F1 is where the general
form gets proven); S3 with searched candidate boundaries (the design
forbids searched windows — S3 confirms boundaries proposed by scans or
the calendar, and only the pre-registered form was prototyped); the
cross-detector attachment in §1.2 rule 6 (an implementation-phase
integration, specified but not prototyped). Bars marked "frozen" are
the values the fixtures in §6 are judged against; if a fixture fails
at a frozen bar, the bar is re-opened explicitly in the implementation
record, not silently tuned.
