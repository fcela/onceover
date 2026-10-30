"""Bespoke evidence payloads (issue #11): finding types carry a
type-specific table in their evidence metrics so a claim can be
inspected without a follow-up query."""
import polars as pl

import onceover


def _finding(report, type_):
    for f in report.findings:
        if f.get("type") == type_:
            return f
    raise AssertionError(f"no {type_} finding in {[(f.get('category'), f.get('type')) for f in report.findings]}")


def test_fd_mapping_shows_ambiguous_keys():
    # city -> state holds on 99.4% of rows; only Springfield is
    # ambiguous. The mapping payload must name exactly that key with
    # both values and their row counts.
    cities, states = [], []
    for _ in range(1400):
        cities.append("Chicago")
        states.append("IL")
        cities.append("Austin")
        states.append("TX")
    for i in range(200):
        cities.append("Springfield")
        states.append("IL" if i < 180 else "MO")
        cities.append("Portland")
        states.append("OR")
    df = pl.DataFrame({"city": cities, "state": states})
    report = onceover.profile(df)
    fd = next(
        f for f in report.findings
        if "mapping" in f["evidence"][0]["metrics"]
    )
    metrics = fd["evidence"][0]["metrics"]
    mapping = metrics["mapping"]
    assert {row["key"] for row in mapping} == {"Springfield"}
    springfield = {row["value"]: row["rows"] for row in mapping if row["key"] == "Springfield"}
    assert springfield == {"IL": 180, "MO": 20}
    assert metrics["mapping_ambiguous_keys"] == 1
    assert metrics["mapping_keys"] == 4


def test_duplicate_groups_render_values():
    rows = {"a": [], "b": []}
    for i in range(100):
        rows["a"].append(f"k{i % 40}")
        rows["b"].append(i % 7)
    # Plant 5 extra copies of one exact row.
    for _ in range(5):
        rows["a"].append("k5")
        rows["b"].append(5)
    df = pl.DataFrame(rows)
    report = onceover.profile(df)
    dup = next(
        f for f in report.findings
        if f.get("category") == "duplicates" and "repeat earlier values" in f["claim"]
    )
    groups = dup["evidence"][0]["metrics"]["duplicate_groups"]
    assert groups, "expected duplicate groups payload"
    top = max(groups, key=lambda g: g["rows"])
    assert top["values"] == "k5 | 5"
    assert top["rows"] == 6
    assert top["first_row"] == 5


def test_breaks_segments_table():
    # The issue #6 panel shape (40 entities x 15 years, South +4 from
    # 2008, integer years): the level-shift finding's segments
    # payload carries the two segments with their means.
    regions = ["North", "South", "East", "West"]
    entity, region, year, life = [], [], [], []
    i = 0
    for e in range(40):
        for yi in range(15):
            entity.append(f"E{e:02d}")
            region.append(regions[e // 10])
            year.append(2000 + yi)
            shift = 4.0 if regions[e // 10] == "South" and yi >= 8 else 0.0
            life.append(70.0 + (((i * 37) % 11) - 5) * 0.3 + shift)
            i += 1
    df = pl.DataFrame(
        {"entity": entity, "region": region, "year": year, "life_exp": life}
    )
    report = onceover.profile(df, detectors=["breaks"])
    f = _finding(report, "level_shift")
    metrics = f["evidence"][0]["metrics"]
    assert metrics["group_value"] == "South"
    segments = metrics["segments"]
    assert len(segments) == 2
    assert segments[0]["rows"] > 0 and segments[1]["rows"] > 0
    means = sorted(s["mean"] for s in segments)
    assert abs(means[0] - 70.0) < 1.0 and abs(means[1] - 74.0) < 1.0
    md = report.to_markdown()
    assert "| from | mean | rows | segment | to |" in md
