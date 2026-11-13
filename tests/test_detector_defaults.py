"""The default detector set and its coverage disclosure.

The 2026-11-02 detector battery (docs/benchmarks/2026-11-02-detector-
opt-in.md) measured five detectors producing zero findings in 40 runs
over 20 datasets; they are opt-in, like the experimental views pass.
A default run must say so in coverage, per detector, and an explicit
selection must still run them. The per-detector report caps on
functional_dependencies, gbm, and discontinuities (the last added by
the 2026-11-04 battery) must likewise be disclosed with
counts, never applied silently.
"""

import datetime as dt

import polars as pl

import onceover

OPT_IN = [
    "views",
    "dates",
    "pivot",
    "period_associations",
    "drift",
    "granularity",
    "changeforest",
]


def _frame() -> pl.DataFrame:
    days = [dt.date(2024, 1, 1) + dt.timedelta(days=i) for i in range(300)]
    return pl.DataFrame(
        {
            "day": days,
            "region": [["emea", "amer", "apac"][i % 3] for i in range(300)],
            "amount": [float(i % 17) + 0.5 for i in range(300)],
        }
    )


def test_registry_activation_partition():
    registry = {d["id"]: d for d in onceover.available_detectors()}
    assert len(registry) == 39
    by_activation = {}
    for d in registry.values():
        by_activation.setdefault(d["activation"], []).append(d["id"])
    assert sorted(by_activation["opt_in"]) == sorted(OPT_IN)
    assert by_activation["hint_gated"] == ["expectations", "group_contrast"]
    assert by_activation["target_gated"] == ["target_shapes"]
    assert len(by_activation["default"]) == 29


def test_default_run_discloses_opt_in_passes_as_skipped():
    report = onceover.profile(_frame())
    for detector in OPT_IN:
        entry = report.coverage[detector]
        assert entry["status"] == "skipped", (detector, entry)
        assert "opt-in" in entry["detail"], (detector, entry)
        assert detector in entry["detail"], (detector, entry)
    entry = report.coverage["target_shapes"]
    assert entry["status"] == "skipped"
    assert "target" in entry["detail"]
    # No per-detector cap binds on a frame this small, so there is no
    # selection entry: the disclosure appears only when a cap binds.
    assert "selection" not in report.coverage


def test_opt_in_detector_runs_when_selected():
    days = [dt.date(2024, 1, 1) + dt.timedelta(days=i) for i in range(100)]
    days = [d for d in days if d.weekday() < 5 and d != dt.date(2024, 2, 7)]
    report = onceover.profile(pl.DataFrame({"date": days}), detectors=["dates"])
    assert report.coverage["dates"]["status"] == "ran_exact"
    assert report.select(category="gaps")


def test_functional_dependency_cap_is_disclosed_with_counts():
    # Lace satellites: the functional-dependency detector produces
    # more statements than the per-detector report cap of 25
    # (74 statements past the cap in the measured default run; see
    # the benchmark record).
    path = "benchmarks/corpus/data/crosscat/lace-satellites/data.csv"
    df = pl.read_csv(path, infer_schema_length=None)
    report = onceover.profile(df)
    fd = [
        f
        for f in report.findings
        if f["category"] == "dependencies"
        and f["evidence"][0]["method"]["detector"] == "functional_dependencies"
    ]
    assert 0 < len(fd) <= 25
    entry = report.coverage["selection"]
    assert entry["status"] == "capped"
    assert "functional_dependencies" in entry["detail"]


def test_discontinuities_cap_is_disclosed_with_counts():
    # State unemployment (wide panel): the discontinuities detector
    # produced 81 selected statements in the 2026-11-04 battery's
    # default run, crowding out every gbm and numeric-dependence
    # statement; the per-detector report cap of 25 now bounds it and
    # must be disclosed the same way.
    path = "benchmarks/corpus/data/crosscat/state-unemployment/data.csv"
    df = pl.read_csv(path, infer_schema_length=None, try_parse_dates=True)
    report = onceover.profile(df)
    disc = [
        f
        for f in report.findings
        if f["evidence"][0]["method"]["detector"] == "discontinuities"
    ]
    assert 0 < len(disc) <= 25
    entry = report.coverage["selection"]
    assert entry["status"] == "capped"
    assert "discontinuities" in entry["detail"]
