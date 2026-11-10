"""Series past the breaks series cap are stated as a finding (#219).

The breaks pass prescores every eligible series and scans at most
`series_cap` (16) in depth. The series past the cap used to be
named only in the coverage entry, so a reader of the findings
alone could not tell which series were not scanned in depth. The
pass now emits one `unscanned_series` finding (category `gaps`)
naming them, with each series' prescore in the evidence — the
prescore is the partial examination those series received, so the
finding states what it saw, not only what it skipped.
"""
import json
from datetime import date, timedelta

import polars as pl

import onceover

N = 400
DAYS = [date(2021, 1, 1) + timedelta(days=i) for i in range(N)]
SHIFTED = [f"shift_{k:02d}" for k in range(16)]
FLAT = [f"flat_{k:02d}" for k in range(4)]


def _frame(n_shifted: int, n_flat: int) -> pl.DataFrame:
    cols: dict[str, list] = {"day": DAYS}
    for k in range(n_shifted):
        cols[f"shift_{k:02d}"] = [0.0] * (N // 2) + [10.0] * (N // 2)
    for k in range(n_flat):
        cols[f"flat_{k:02d}"] = [1.0] * N
    return pl.DataFrame(cols)


def _unscanned(report) -> list:
    return [f for f in report.findings if f.get("type") == "unscanned_series"]


def _metrics(finding) -> dict:
    return finding["evidence"][0]["metrics"]


def test_cap_binds_states_unscanned_series_with_prescores():
    df = _frame(16, 4)
    report = onceover.profile(df, detectors=["breaks"])
    hits = _unscanned(report)
    assert len(hits) == 1
    f = hits[0]
    assert f["category"] == "gaps"
    assert f["facets"]["scope"] == "frame"
    assert f["facets"]["quality_dimension"] == "completeness"
    assert "4 of 20 eligible series" in f["claim"]
    assert "16-series cap" in f["claim"]
    m = _metrics(f)
    assert m["pass"] == "breaks"
    assert m["reason"] == "series_cap"
    assert m["cap"] == 16
    assert m["n_eligible"] == 20
    assert m["n_examined"] == 16
    assert m["n_unscanned"] == 4
    payload = m["unscanned_series"]
    assert [row["series"] for row in payload] == FLAT
    assert all(row["prescore"] == 0.0 for row in payload)
    # The statement definition and the subject name the same series.
    assert f["specification"]["definition"]["absent_values"] == FLAT
    assert sorted(f["subject"]["columns"]) == FLAT
    # Coverage names the same set under "not scanned:".
    detail = report.data["coverage"]["breaks"]["detail"]
    tail = detail.split("not scanned: ", 1)[1]
    assert all(name in tail for name in FLAT)


def test_unscanned_series_survives_serialization_and_verify():
    df = _frame(16, 4)
    report = onceover.profile(df, detectors=["breaks"])
    f = _unscanned(report)[0]
    data = json.loads(report.to_json())
    back = [x for x in data["findings"] if x.get("type") == "unscanned_series"]
    assert len(back) == 1
    assert back[0]["claim"] == f["claim"]
    assert back[0]["id"] == f["id"]
    # The verifier has no exact recount for a prescore statement;
    # the finding must come back not_checkable, never an error or
    # a mismatch.
    checked = report.verify(df)
    rows = checked.filter(pl.col("id") == f["id"])
    assert rows.height >= 1
    assert set(rows["status"].to_list()) == {"not_checkable"}


def test_cap_not_reached_states_nothing():
    df = _frame(12, 4)
    report = onceover.profile(df, detectors=["breaks"])
    assert _unscanned(report) == []
    detail = report.data["coverage"]["breaks"]["detail"]
    assert "series cap" not in detail
