"""Ordered partitions (MOB) against the bundled state-unemployment
panel: the design expectations, pinned exactly.

Design record: the pass fits a level+trend regime tree per series.
On this panel (609 months x 51 series, BLS via FRED) the pinned
expectations are: a tree for every series; a first (root) split
in 2008-08..2008-12 for at least 30 of 51 series (the Great
Recession was a persistent level shift; the COVID spike is not
what a segment model prices); median tree relative SSE gain at
least 0.45 (the two-break prototype benchmark); silence on the
same panel with each series' time order shuffled; and a
not-applicable coverage record when no ordering column exists.
"""
import statistics
import zlib
from pathlib import Path

import polars as pl

import onceover

PANEL = (
    Path(__file__).resolve().parent.parent
    / "benchmarks/corpus/data/crosscat/state-unemployment/data.csv"
)


def _trees(report):
    out = []
    for f in report.findings:
        if f.get("type") == "ordered_partition":
            m = f["evidence"][0]["metrics"]
            out.append((m["response"], m["splits"], m["relative_gain"]))
    return out


def test_panel_regime_trees_match_design_expectations():
    df = pl.read_csv(PANEL, try_parse_dates=True)
    report = onceover.profile(df, budget="deep", detectors=["ordered_partitions"])
    trees = _trees(report)
    assert len(trees) == 51, f"{len(trees)} trees for 51 series"
    in_window = [
        r for r, splits, _ in trees if "2008-08-01" <= splits[0]["at"] <= "2008-12-31"
    ]
    assert len(in_window) >= 30, f"{len(in_window)} first splits in 2008-08..2008-12"
    median_gain = statistics.median(g for _, _, g in trees)
    assert median_gain >= 0.45, f"median relative gain {median_gain:.3f}"


PANEL_LONG = (
    Path(__file__).resolve().parent.parent
    / "benchmarks/corpus/data/crosscat/state-unemployment-long/data.csv"
)

ENERGY = {"WY", "OK", "ND", "AK", "TX", "LA", "NM"}


def test_long_form_tree_mixes_time_and_state_splits():
    df = pl.read_csv(PANEL_LONG, try_parse_dates=True)
    report = onceover.profile(df, budget="deep", detectors=["ordered_partitions"])
    trees = _trees(report)
    assert len(trees) == 1, f"{len(trees)} trees for the pooled response"
    _, splits, gain = trees[0]
    assert gain >= 0.40, f"pooled relative gain {gain:.3f}"
    # The root split is the Great Recession break.
    assert splits[0].get("at") == "2008-11-01", splits[0]
    # State bipartitions compete in the same tree.
    state_splits = [s for s in splits if s.get("on") == "state"]
    assert state_splits, "no dimension split in the pooled tree"
    for s in state_splits:
        assert s["left_levels"] and s["right_levels"]
        assert set(s["left_levels"]).isdisjoint(s["right_levels"])
    # The first state split sits under time splits only, so its
    # two sides partition all 51 states; deeper state splits
    # partition the level subset of their parent node.
    first = state_splits[0]
    assert len(first["left_levels"]) + len(first["right_levels"]) == 51


def test_energy_state_split_is_a_known_miss():
    # Known-miss tripwire. The design's preanalysis measured a
    # 2014->2016 rise contrast topped by the energy states
    # (WY, OK, ND), a localized slope pattern. The shipped split
    # search orders a dimension's levels by fitted level, and the
    # greedy tree prices the panel's large level contrasts first;
    # within the depth cap it never isolates a mid-2010s node in
    # which the energy grouping is the best split. If a future
    # search (e.g. slope-ordered dimension scans) finds it, this
    # test fails and the expectation must be promoted, not the
    # test deleted.
    df = pl.read_csv(PANEL_LONG, try_parse_dates=True)
    report = onceover.profile(df, budget="deep", detectors=["ordered_partitions"])
    _, splits, _ = _trees(report)[0]
    for s in splits:
        if s.get("on") != "state":
            continue
        for side in (s["left_levels"], s["right_levels"]):
            energy_share = len(ENERGY.intersection(side)) / max(len(side), 1)
            assert not (len(side) <= 12 and energy_share >= 0.5), (
                f"energy-dominated split appeared: {side}"
            )


def test_shuffled_panel_produces_no_partition_findings():
    df = pl.read_csv(PANEL, try_parse_dates=True)
    shuffled = df.with_columns(
        [
            pl.col(c).shuffle(seed=zlib.crc32(c.encode()))
            for c in df.columns
            if c != "date"
        ]
    )
    report = onceover.profile(shuffled, budget="deep", detectors=["ordered_partitions"])
    assert _trees(report) == []


def test_no_ordering_column_is_not_applicable():
    df = pl.read_csv(PANEL, try_parse_dates=True).drop("date")
    report = onceover.profile(df, budget="standard", detectors=["ordered_partitions"])
    cov = report.coverage["ordered_partitions"]
    assert cov["status"] == "not_applicable"
    assert _trees(report) == []
