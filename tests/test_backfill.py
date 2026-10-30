"""Out-of-order appended blocks (backfills) in an ordered column
(issue #57): the sortedness pass reports the late block whose
values return to earlier territory."""

import polars as pl

import onceover


def _backfill_frame() -> pl.DataFrame:
    months = list(range(120))
    months = months + months[10:20]
    return pl.DataFrame(
        {"month": months, "value": [float(i % 9) for i in range(len(months))]}
    )


def test_backfill_block_is_reported():
    report = onceover.profile(_backfill_frame())
    hits = [
        f
        for f in report.findings
        if f["subject"]["columns"] == ["month"] and "late block" in f["claim"]
    ]
    assert len(hits) == 1
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["descents"] == 1
    assert metrics["first_descent_row"] == 120
    assert metrics["tail_rows"] == 10


def test_sorted_column_gets_only_the_positive_claim():
    df = pl.DataFrame({"month": list(range(130)), "value": [1.0] * 130})
    report = onceover.profile(df)
    claims = [
        f["claim"]
        for f in report.findings
        if f["subject"]["columns"] == ["month"]
    ]
    assert any("non-decreasing" in c or "strictly increasing" in c for c in claims)
    assert not any("late block" in c for c in claims)


def test_noisy_dip_is_not_a_backfill():
    vals = list(range(130))
    vals[60] = 5  # one dip, then new highs: not an appended block
    df = pl.DataFrame({"month": vals, "value": [1.0] * 130})
    report = onceover.profile(df)
    assert not any(
        "late block" in f["claim"]
        for f in report.findings
        if f["subject"]["columns"] == ["month"]
    )
