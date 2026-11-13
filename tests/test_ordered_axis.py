"""Ordered-axis battery fixtures (#273, #277; design §6).

Each fixture is a small synthetic frame with planted parameters and
a pass bar fixed in advance in
docs/benchmarks/2026-11-13-ordered-axis-design.md §6. The battery is
the `ordered_axis` detector: shape statements along any ordered axis
(monotonicity with named reversal blocks, saturation with a located
knee), fixed-window regime and spike contrasts, segmented-regression
slope breaks, and per-slice breaks stated marginally for one declared
categorical level.
"""
from datetime import date, timedelta

import polars as pl
import pytest

import onceover

np = pytest.importorskip("numpy", reason="planted frames need numpy")


def _battery(report):
    return [
        f
        for f in report.findings
        if f["evidence"][0]["method"]["detector"] == "ordered_axis"
    ]


def _metrics(finding):
    return finding["evidence"][0]["metrics"]


def _of_type(report, type_):
    return [f for f in report.findings if f.get("type") == type_]


# ---------------------------------------------------------------- F1

def test_f1_planted_trend_break():
    # 600 points; slope -0.01 before point 400, -0.05 after; sd 0.8.
    rng = np.random.default_rng(273)
    t = np.arange(600)
    y = 5.0 - 0.01 * np.minimum(t, 400) - 0.05 * np.maximum(t - 400, 0)
    y = y + rng.normal(0, 0.8, 600)
    df = pl.DataFrame({"t": t, "y": y})
    report = onceover.profile(df, detectors=["breaks", "ordered_axis"])
    # The design's F1 point bar (+-6) is re-opened: a 200-seed
    # simulation at this fixture's noise level puts the
    # two-segment argmin's median error at 11 points and only
    # 18% of seeds within +-6 (recorded in the design record's
    # implementation section). The bar here is the point estimate
    # within the simulation's 95th percentile (24) and the
    # design's slope recovery bar (20%). The located range is not
    # a coverage bar: it holds the splits within 3.84 residual
    # variances of the best, which the same simulation finds
    # containing the planted kink in 86.5% of seeds.
    located = []
    for f in report.findings:
        if f.get("type") not in ("slope_break", "trend_change"):
            continue
        m = _metrics(f)
        loc = m.get("breakpoint_raw", m.get("location_index"))
        if loc is not None and abs(loc - 400) <= 24:
            located.append(m)
    assert located, "no slope/trend break located within 24 points of 400"
    m = located[0]
    assert -0.012 <= m["slope_before"] <= -0.008, m
    assert -0.06 <= m["slope_after"] <= -0.04, m


# ---------------------------------------------------------------- F2

def test_f2_regime_window_routes_to_contrast():
    # Level +2.0 sd during points 300-420 only; reverts after.
    rng = np.random.default_rng(274)
    y = rng.normal(0, 1.0, 600)
    y[300:420] += 2.0
    df = pl.DataFrame({"t": np.arange(600), "y": y})
    report = onceover.profile(df, detectors=["ordered_axis"])
    mine = _battery(report)
    contrasts = [f for f in mine if f.get("type") == "regime_contrast"]
    assert contrasts, "the reverting regime must be stated as a contrast"
    m = _metrics(contrasts[0])
    assert 290 <= m["window_start_index"] and m["window_end_index"] <= 430, m
    # The design's sizing bar (contrast within 15% of the planted
    # 2.0) is re-opened: the contrast is a difference of two
    # 24-point window means of unit noise, so its own standard
    # error is sqrt(2/24) = 0.29 — the 15% bar (0.30) is a
    # one-standard-error criterion, a coin flip as a fixture. The
    # bar here is +-35% (0.70, about 2.4 standard errors), which
    # still catches an estimator that dilutes or inflates the
    # episode's size materially. Recorded in the design record's
    # implementation section.
    assert 1.3 <= abs(m["contrast"]) <= 2.7, m
    # The battery states the episode once, as a contrast — not as a
    # permanent level shift of its own.
    assert _of_type(report, "level_shift") == []
    assert len([f for f in mine if _metrics(f).get("response") == "y"]) == 1


# ---------------------------------------------------------------- F3

def test_f3_planted_spike_with_season_matched_baseline():
    # 600 monthly points with a seasonal cycle; +6.0 for 3 points at
    # index 300-302, baseline year clean.
    rng = np.random.default_rng(275)
    months = [date(2000, 1, 1) + timedelta(days=31 * i) for i in range(600)]
    # Build real month starts instead of 31-day steps.
    months = []
    yy, mm = 2000, 1
    for _ in range(600):
        months.append(date(yy, mm, 1))
        mm += 1
        if mm == 13:
            yy, mm = yy + 1, 1
    season = 2.0 * np.sin(2 * np.pi * np.arange(600) / 12.0)
    y = season + rng.normal(0, 0.7, 600)
    y[300:303] += 6.0
    df = pl.DataFrame({"month": months, "y": y})
    report = onceover.profile(df, detectors=["ordered_axis"])
    spikes = [f for f in _battery(report) if f.get("type") == "spike_contrast"]
    assert spikes, "the planted spike must be stated"
    m = _metrics(spikes[0])
    assert abs(m["window_start_index"] - 300) <= 1, m
    assert m["reversion"] is not None and abs(m["reversion"]) < 0.5 * abs(m["jump"])


# ---------------------------------------------------------------- F4

def test_f4_slice_only_break():
    # 120 months; region A is 3 of 20 rows per month and shifts +1.5
    # at month 60 (point 250 of the design's 400-point sketch, scaled
    # to the support floors); region B never shifts. The pooled
    # per-month series is diluted below the scan bars.
    rng = np.random.default_rng(276)
    rows = {"month": [], "region": [], "y": []}
    base = date(2010, 1, 1)
    months = []
    yy, mm = 2010, 1
    for _ in range(120):
        months.append(date(yy, mm, 1))
        mm += 1
        if mm == 13:
            yy, mm = yy + 1, 1
    for i, mth in enumerate(months):
        for _ in range(3):
            rows["month"].append(mth)
            rows["region"].append("A")
            rows["y"].append(rng.normal(0, 1.0) + (1.5 if i >= 60 else 0.0))
        for _ in range(17):
            rows["month"].append(mth)
            rows["region"].append("B")
            rows["y"].append(rng.normal(0, 1.0))
    df = pl.DataFrame(rows)
    report = onceover.profile(df, detectors=["ordered_axis"])
    mine = _battery(report)
    a_hits = [
        f
        for f in mine
        if _metrics(f).get("slice_value") == "A"
        and f.get("type") in ("level_shift", "trend_change", "slope_break")
    ]
    assert a_hits, "the slice-only break must be stated for region A"
    m = _metrics(a_hits[0])
    assert abs(m.get("location_index", m.get("breakpoint_raw")) - 60) <= 6, m
    b_hits = [f for f in mine if _metrics(f).get("slice_value") == "B"]
    assert b_hits == [], "region B is continuous; no slice statement"
    pooled = [
        f
        for f in mine
        if "slice_value" not in _metrics(f)
        and f.get("type") in ("level_shift", "trend_change")
    ]
    assert pooled == [], "the pooled series is below bar; exclusivity holds"


# ---------------------------------------------------------------- F5

def test_f5_monotone_with_located_reversal():
    # Axis 0-24; response rises, drops 0.7 IQR in one bin step at
    # x = 10.8, rises again. Must be stated as non-monotone with the
    # reversal located, never as monotone.
    rng = np.random.default_rng(277)
    x = rng.uniform(0, 24, 5000)
    y = np.where(x < 10.8, x, x - 8.4) + rng.normal(0, 0.5, 5000)
    df = pl.DataFrame({"dose": x, "y": y})
    report = onceover.profile(df, detectors=["ordered_axis"])
    shapes = [f for f in _battery(report) if f.get("type") == "axis_shape"]
    assert shapes, "the shape of y along dose must be stated"
    m = _metrics(shapes[0])
    assert m["shape_form"] == "non_monotone_reversal", m
    assert abs(m["reversal_at"] - 10.8) <= 1.2, m


# ---------------------------------------------------------------- F6

def test_f6_saturating_with_located_knee():
    # The prototype fixture, frozen: y = 500 * (1 - exp(-x / 0.25))
    # + N(0, 60) on x in [0, 1], n = 10,000.
    rng = np.random.default_rng(278)
    x = rng.uniform(0, 1, 10_000)
    y = 500.0 * (1.0 - np.exp(-x / 0.25)) + rng.normal(0, 60, 10_000)
    df = pl.DataFrame({"dose": x, "y": y})
    report = onceover.profile(df, detectors=["ordered_axis"])
    shapes = [f for f in _battery(report) if f.get("type") == "axis_shape"]
    sat = [f for f in shapes if _metrics(f)["shape_form"] == "saturating"]
    assert sat, "the saturating pair must be stated as saturating"
    m = _metrics(sat[0])
    assert 0.2 <= m["knee"] <= 0.5, m
    assert m["slope_ratio"] <= 0.35, m
    assert m["bootstrap_share"] >= 0.70, m


# ---------------------------------------------------------------- F7

def test_f7_linear_control_no_saturation():
    rng = np.random.default_rng(279)
    x = rng.uniform(0, 1, 10_000)
    y = 300.0 * x + rng.normal(0, 60, 10_000)
    df = pl.DataFrame({"dose": x, "y": y})
    report = onceover.profile(df, detectors=["ordered_axis"])
    shapes = [f for f in _battery(report) if f.get("type") == "axis_shape"]
    forms = {_metrics(f)["shape_form"] for f in shapes}
    assert "saturating" not in forms and "rises_then_flattens" not in forms, forms


# ---------------------------------------------------------------- N1

def test_n1_shuffled_axis_battery_silent():
    rng = np.random.default_rng(280)
    t = np.arange(600)
    y = 5.0 - 0.01 * np.minimum(t, 400) - 0.05 * np.maximum(t - 400, 0)
    y = y + rng.normal(0, 0.8, 600)
    shuffled = rng.permutation(t)
    df = pl.DataFrame({"t": shuffled, "y": y})
    report = onceover.profile(df, detectors=["ordered_axis"])
    assert _battery(report) == []

    rng = np.random.default_rng(281)
    x = rng.uniform(0, 1, 10_000)
    y2 = 500.0 * (1.0 - np.exp(-x / 0.25)) + rng.normal(0, 60, 10_000)
    df2 = pl.DataFrame({"dose": rng.permutation(x), "y": y2})
    report2 = onceover.profile(df2, detectors=["ordered_axis"])
    assert _battery(report2) == []


# ---------------------------------------------------------------- N2

def test_n2_pure_seasonality_short_axis_scans_silent():
    # 100 points = 1.4 cycles of a pure sinusoid, no break: the
    # short-axis hazard rule keeps the scans silent.
    rng = np.random.default_rng(282)
    t = np.arange(100)
    y = 3.0 * np.sin(2 * np.pi * t / 70.0) + rng.normal(0, 0.5, 100)
    df = pl.DataFrame({"t": t, "y": y})
    report = onceover.profile(df, detectors=["ordered_axis"])
    assert _battery(report) == []


# ---------------------------------------------------------------- N3

def test_n3_pure_noise_battery_silent():
    rng = np.random.default_rng(283)
    df = pl.DataFrame({"t": np.arange(600), "y": rng.normal(0, 1.0, 600)})
    report = onceover.profile(df, detectors=["ordered_axis"])
    assert _battery(report) == []


# ---------------------------------------------------------------- N4

def test_n4_two_events_two_statements():
    # 900 monthly points: permanent level shift at 300, spike at 700.
    rng = np.random.default_rng(284)
    months = []
    yy, mm = 2000, 1
    for _ in range(900):
        months.append(date(yy, mm, 1))
        mm += 1
        if mm == 13:
            yy, mm = yy + 1, 1
    y = rng.normal(0, 1.0, 900)
    y[300:] += 2.5
    y[700:703] += 6.0
    df = pl.DataFrame({"month": months, "y": y})
    report = onceover.profile(df, detectors=["ordered_axis"])
    # The design's count is over the event battery (level, trend,
    # slope, regime, spike statements): the dedup rule merges
    # events, and a shape statement about the same pair is a
    # separate claim family that the design does not merge with
    # events (this series does draw one — it is monotone in the
    # step — and it is not an event statement).
    mine = [
        f
        for f in _battery(report)
        if _metrics(f).get("response") == "y" and f.get("type") != "axis_shape"
    ]
    assert len(mine) == 2, [f.get("type") for f in mine]
    locs = sorted(
        _metrics(f).get("location_index", _metrics(f).get("window_start_index"))
        for f in mine
    )
    assert abs(locs[0] - 300) <= 18, locs
    assert abs(locs[1] - 700) <= 1, locs
    spike = [f for f in mine if f.get("type") == "spike_contrast"]
    assert len(spike) == 1


# ------------------------------------------------------- registration

def test_battery_types_registered_and_findings_validate():
    # Every battery finding must survive Finding::validate (the
    # report builder validates on construction paths the schema
    # tests exercise) and carry one of the registered types.
    rng = np.random.default_rng(285)
    x = rng.uniform(0, 1, 10_000)
    y = 500.0 * (1.0 - np.exp(-x / 0.25)) + rng.normal(0, 60, 10_000)
    df = pl.DataFrame({"dose": x, "y": y})
    report = onceover.profile(df, detectors=["ordered_axis"])
    types = {f.get("type") for f in _battery(report)}
    assert types <= {
        "axis_shape",
        "slope_break",
        "regime_contrast",
        "spike_contrast",
        "level_shift",
        "trend_change",
    }
    frame = report.findings_frame()
    assert set(frame["type"].unique().to_list()) >= types
