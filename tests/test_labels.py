"""Column labels (issue #212): `hints.labels` maps a column name to a
display label used in claim strings and interpretations. Labels are
presentation, never identity: evidence metrics, subjects, and column
cards keep the raw column names, and finding ids are unchanged.
Colliding or unusable labels fall back to the raw name and are stated
in the `labels` coverage entry, never raised."""

import polars as pl

import onceover


def _counts_df():
    # acc <= gen on every row: an exact order constraint between two
    # cryptically named count columns.
    acc = [(k % 500) * 0.1 for k in range(2000)]
    gen = [a + (k % 41) for k, a in enumerate(acc)]
    return pl.DataFrame({"acc_cnt": acc, "gen_cnt": gen})


def _order_finding(report):
    hits = [
        f
        for f in report.findings
        if f.get("category") == "dependencies" and f.get("type") == "order_constraint"
    ]
    assert len(hits) == 1, [f.get("claim") for f in report.findings]
    return hits[0]


def test_labels_render_in_claims_raw_names_stay_in_evidence():
    df = _counts_df()
    plain = onceover.profile(df)
    labelled = onceover.profile(
        df, hints={"labels": {"acc_cnt": "accepted count", "gen_cnt": "generated count"}}
    )
    raw = _order_finding(plain)
    assert raw["claim"].startswith("acc_cnt <= gen_cnt")
    hit = _order_finding(labelled)
    # The claim speaks in labels...
    assert hit["claim"].startswith("accepted count <= generated count")
    assert "acc_cnt" not in hit["claim"]
    assert "gen_cnt" not in hit["claim"]
    # ...the identity does not move: same id, raw names in the
    # subject and in the evidence metrics.
    assert hit["id"] == raw["id"]
    assert hit["subject"]["columns"] == raw["subject"]["columns"]
    metrics = hit["evidence"][0]["metrics"]
    assert metrics["lower"] == "acc_cnt"
    assert metrics["upper"] == "gen_cnt"
    # The application is disclosed in coverage.
    info = labelled.coverage["labels"]
    assert info["status"] == "ran_exact"
    assert "accepted count" in info["detail"]


def test_labels_reach_rendered_reports_and_findings_frame():
    df = _counts_df()
    report = onceover.profile(
        df, hints={"labels": {"acc_cnt": "accepted count", "gen_cnt": "generated count"}}
    )
    assert "accepted count <= generated count" in report.to_digest(8000)
    assert any(
        "accepted count" in claim for claim in report.data["overview"]["top_findings"]
    )
    frame = report.findings_frame()
    claims = frame.get_column("claim").to_list()
    assert any("accepted count <= generated count" in c for c in claims)
    # The frame's identity columns keep the raw names.
    columns = frame.get_column("columns").to_list()
    assert any("acc_cnt" in (c or []) for c in columns)


def test_label_naming_another_column_falls_back():
    df = _counts_df()
    report = onceover.profile(df, hints={"labels": {"acc_cnt": "gen_cnt"}})
    hit = _order_finding(report)
    # The colliding label is not used: the claim keeps the raw name,
    # and the collision is stated in coverage.
    assert hit["claim"].startswith("acc_cnt <= gen_cnt")
    detail = report.coverage["labels"]["detail"]
    assert "acc_cnt" in detail and "gen_cnt" in detail and "collid" in detail


def test_two_columns_sharing_one_label_fall_back():
    df = _counts_df()
    report = onceover.profile(
        df, hints={"labels": {"acc_cnt": "count", "gen_cnt": "count"}}
    )
    hit = _order_finding(report)
    assert hit["claim"].startswith("acc_cnt <= gen_cnt")
    detail = report.coverage["labels"]["detail"]
    assert "count" in detail and "collid" in detail


def test_label_for_unknown_column_is_ignored_and_stated():
    df = _counts_df()
    report = onceover.profile(df, hints={"labels": {"nope": "nothing"}})
    hit = _order_finding(report)
    assert hit["claim"].startswith("acc_cnt <= gen_cnt")
    detail = report.coverage["labels"]["detail"]
    assert "nope" in detail and "ignored" in detail


def test_empty_label_is_ignored_and_stated():
    df = _counts_df()
    report = onceover.profile(df, hints={"labels": {"acc_cnt": "  "}})
    hit = _order_finding(report)
    assert hit["claim"].startswith("acc_cnt <= gen_cnt")
    detail = report.coverage["labels"]["detail"]
    assert "acc_cnt" in detail and "ignored" in detail


def test_no_labels_no_labels_coverage_entry():
    report = onceover.profile(_counts_df())
    assert "labels" not in report.coverage
