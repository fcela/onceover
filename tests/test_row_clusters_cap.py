"""The row-clusters column cap is stated in coverage, not as a finding.

Columns the pass did not examine are a statement about the
examination, not about the data: the pass's coverage entry names
them under the reason that dropped them (the column cap, or
extraction), and no finding carries the statement.
"""

import json
import random

import polars as pl

import onceover


def _wide_frame():
    # 68 eligible columns at the 64-column cap: 64 constants, two
    # noise columns, and a dependent pair the pre-screen must keep.
    # Four columns drop past the cap.
    rng = random.Random(9)
    n = 600
    x = [rng.gauss(0, 1) for _ in range(n)]
    data = {f"constant_{i}": [float(i)] * n for i in range(64)}
    data.update({
        "noise_a": [rng.gauss(0, 1) for _ in range(n)],
        "noise_b": [rng.gauss(0, 1) for _ in range(n)],
        "late_x": x,
        "late_y": x,
    })
    return pl.DataFrame(data)


def test_views_cap_drop_is_stated_in_coverage():
    report = onceover.profile(_wide_frame(), detectors=["row_clusters"], max_findings=500)
    assert not report.select(category="gaps", type="unexamined_columns")
    assert all(
        f.get("type") != "unexamined_columns" for f in report.findings
    )
    detail = report.coverage["row_clusters"]["detail"]
    assert "dropped 4 column(s) past the 64-column cap" in detail
    segment = detail.split("dropped 4 column(s) past the 64-column cap: ", 1)[1]
    dropped = [name.strip() for name in segment.split("; ", 1)[0].split(",")]
    assert len(dropped) == 4
    # The pre-screen keeps the dependent pair.
    assert "late_x" not in dropped and "late_y" not in dropped


def test_views_cap_statement_reaches_json():
    report = onceover.profile(_wide_frame(), detectors=["row_clusters"], max_findings=500)
    parsed = json.loads(report.to_json())
    assert not [
        f for f in parsed["findings"] if f.get("type") == "unexamined_columns"
    ]
    assert "dropped 4 column(s) past the 64-column cap" in parsed["coverage"][
        "row_clusters"
    ]["detail"]


def test_views_at_cap_produces_no_cap_statement():
    rng = random.Random(4)
    n = 300
    data = {f"c{i}": [rng.gauss(0, 1) for _ in range(n)] for i in range(10)}
    report = onceover.profile(pl.DataFrame(data), detectors=["row_clusters"], max_findings=500)
    assert not report.select(category="gaps", type="unexamined_columns")
    assert "past the 64-column cap" not in report.coverage["row_clusters"]["detail"]


def test_default_run_has_no_views_cap_statement():
    # Row clusters is on request: a default run records it skipped
    # and gains no cap statement.
    report = onceover.profile(_wide_frame(), max_findings=500)
    assert not report.select(category="gaps", type="unexamined_columns")
    assert report.coverage["row_clusters"]["status"] == "skipped"


def _drop_segments(detail):
    # The coverage detail states each drop reason as its own
    # "; dropped ..." segment; map the segment's opening wording
    # to the names it lists.
    segments = {}
    for part in detail.split("; dropped ")[1:]:
        wording, _, names = part.partition(": ")
        segments[wording] = [name.strip() for name in names.split(",")]
    return segments


def test_views_coverage_separates_cap_and_extraction_drops():
    # The wide frame (four cap drops) plus one list column. A list
    # column is dropped at extraction for its unsupported type: it
    # was never eligible, so it was never subject to the cap, and
    # the coverage entry must not attribute it to the cap.
    frame = _wide_frame().with_columns(
        pl.Series("payload_list", [[1, 2] for _ in range(600)], dtype=pl.List(pl.Int64))
    )
    report = onceover.profile(frame, detectors=["row_clusters"], max_findings=500)
    detail = report.coverage["row_clusters"]["detail"]
    assert "dropped 4 column(s) past the 64-column cap" in detail
    assert (
        "dropped 1 column(s) at extraction "
        "(unsupported type or no non-null values): payload_list"
    ) in detail
    # The old wording counted every drop as a cap drop.
    assert "dropped 5 column(s)" not in detail

    segments = _drop_segments(detail)
    cap_segment = segments["4 column(s) past the 64-column cap"]
    assert len(cap_segment) == 4
    assert "payload_list" not in cap_segment
    extraction_segment = segments[
        "1 column(s) at extraction (unsupported type or no non-null values)"
    ]
    assert extraction_segment == ["payload_list"]


def test_views_coverage_extraction_drop_only_states_no_cap_group():
    # Only one drop reason present (extraction, on a narrow frame):
    # the entry states that group alone, with no cap group.
    rng = random.Random(4)
    n = 300
    data = {f"c{i}": [rng.gauss(0, 1) for _ in range(n)] for i in range(6)}
    frame = pl.DataFrame(data).with_columns(
        pl.Series("payload_list", [[1] for _ in range(n)], dtype=pl.List(pl.Int64))
    )
    report = onceover.profile(frame, detectors=["row_clusters"], max_findings=500)
    detail = report.coverage["row_clusters"]["detail"]
    assert (
        "dropped 1 column(s) at extraction "
        "(unsupported type or no non-null values): payload_list"
    ) in detail
    assert "past the 64-column cap" not in detail


def test_views_coverage_cap_drop_only_states_no_extraction_group():
    # Only one drop reason present (the cap, on the wide frame):
    # the entry states that group alone, with no extraction group.
    report = onceover.profile(_wide_frame(), detectors=["row_clusters"], max_findings=500)
    detail = report.coverage["row_clusters"]["detail"]
    assert "dropped 4 column(s) past the 64-column cap" in detail
    assert "at extraction" not in detail
