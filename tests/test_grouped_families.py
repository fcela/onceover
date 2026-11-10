"""Verified relationship families (issue #116, schema 0.6).

An exact bijection between two columns produces, on the same
columns, the duplicate-encoding statement and the two functional
dependencies. The family consolidates them under one lead: the
FDs stay in the report as full records under `grouped_findings`,
spend no selection or digest slot, and resolve by id.
"""
import json
from pathlib import Path

import polars as pl
import pytest

import onceover


def _bijection_report():
    levels = ["primary", "secondary", "bachelor", "master"]
    codes = [1, 2, 3, 5]
    n = 400
    df = pl.DataFrame(
        {
            "level": [levels[i % 4] for i in range(n)],
            "level_code": [codes[i % 4] for i in range(n)],
            "score": [50.0 + ((i * 37) % 43) for i in range(n)],
        }
    )
    return onceover.profile(df)


def test_bijection_family_shape():
    report = _bijection_report()
    assert report.data["schema_version"] == "0.7"
    selection = report.data["selection"]
    assert selection["families"] == 1
    assert selection["grouped_statements"] == 2
    leads = [f for f in report.findings if f.get("grouped_members")]
    assert len(leads) == 1
    lead = leads[0]
    assert lead["type"] == "duplicate_encoding"
    assert len(lead["grouped_members"]) == 2
    assert len(report.grouped_findings) == 2
    for member in report.grouped_findings:
        assert member["grouped_under"] == lead["id"]
        assert member["id"] in lead["grouped_members"]
        # Retrievable by id through the report's own lookup.
        assert report.finding(member["id"]) is member
    # The grouped FDs spend no slot in the findings ledger.
    fd_ids = {f["id"] for f in report.findings if f.get("type") == "functional"}
    assert not fd_ids & {m["id"] for m in report.grouped_findings}
    # The selection coverage entry discloses the grouping.
    entry = report.data["coverage"]["selection"]
    assert "grouped under 1 verified relationship" in entry["detail"]


def test_select_include_grouped():
    report = _bijection_report()
    plain = report.select(type="functional")
    with_grouped = report.select(type="functional", include_grouped=True)
    assert len(with_grouped) == len(plain) + 2
    grouped_ids = {m["id"] for m in report.grouped_findings}
    assert grouped_ids <= {f["id"] for f in with_grouped}
    assert not grouped_ids & {f["id"] for f in plain}


def test_digest_discloses_grouping():
    report = _bijection_report()
    digest = report.to_digest(4000)
    assert len(digest) <= 4000
    assert "grouped: 2 statement(s) grouped under 1 verified" in digest
    assert "[grouped: " in digest
    for member in report.grouped_findings:
        assert report.finding(member["id"])["claim"]


def test_identity_family_retrievable():
    n = 1000
    a = [(i * 13) % 29 for i in range(n)]
    b = [(i * 7) % 11 for i in range(n)]
    df = pl.DataFrame({"a": a, "b": b, "c": [x + y for x, y in zip(a, b)]})
    report = onceover.profile(df)
    selection = report.data["selection"]
    # The identity scan states the relation once; the three functional
    # dependencies over the same columns restate it and group under it.
    assert selection["families"] == 1
    assert selection["grouped_statements"] == 3
    leads = {m["grouped_under"] for m in report.grouped_findings}
    assert len(leads) == 1
    assert report.finding(leads.pop())["claim"].startswith("c = a + b")
    for member in report.grouped_findings:
        assert report.finding(member["id"]) is member
        lead = report.finding(member["grouped_under"])
        assert lead is not None
        assert member["id"] in lead["grouped_members"]


def test_budgeted_json_keeps_grouped_records():
    report = _bijection_report()
    view = json.loads(report.to_json(max_chars=200_000))
    assert len(view["grouped_findings"]) == 2
    assert view["budget"]["full_grouped_findings"] == 2


def test_grouped_report_validates_against_schema():
    jsonschema = pytest.importorskip("jsonschema")
    schema = json.loads(
        (Path(__file__).resolve().parent.parent / "docs" / "report-schema.json")
        .read_text()
    )
    report = _bijection_report()
    assert report.grouped_findings, "expected a family on this frame"
    validator = jsonschema.Draft202012Validator(schema)
    errors = sorted(validator.iter_errors(report.data), key=lambda e: list(e.path))
    assert not errors, "\n".join(
        f"{'.'.join(str(p) for p in e.path)}: {e.message}" for e in errors[:10]
    )


def _hierarchy_df():
    rows = []
    for u in range(24):
        team = u // 3
        division = team // 2
        company = division // 2
        for r in range(100):
            rows.append(
                {
                    "unit": f"unit_{u:02d}",
                    "team": f"team_{team}",
                    "division": f"division_{division}",
                    "company": f"company_{company}",
                    "revenue": 100.0 + ((u * 37 + r * 11) % 89),
                    "channel": ["web", "store", "partner"][r % 3],
                }
            )
    return pl.DataFrame(rows)


def test_hierarchy_chain_collapses_to_one_lead():
    # Issue #207: on the unmodified build this table's hierarchy is
    # stated nine times — six exact statements selected, three
    # grouped under rollup leads. After the change, one constructed
    # lead states the chain and all nine group under it.
    report = onceover.profile(_hierarchy_df())
    hier_cols = {"unit", "team", "division", "company"}
    leads = [
        f
        for f in report.findings
        if (f.get("specification") or {}).get("form") == "hierarchy"
    ]
    assert len(leads) == 1
    lead = leads[0]
    assert set(lead["subject"]["columns"]) == hier_cols
    metrics = lead["evidence"][0]["metrics"]
    assert metrics["hierarchy_levels"] == [
        ["unit"],
        ["team"],
        ["division"],
        ["company"],
    ]
    assert [(m["finer"], m["coarser"]) for m in metrics["hierarchy_mappings"]] == [
        (["unit"], ["team"]),
        (["team"], ["division"]),
        (["division"], ["company"]),
    ]
    assert all(m["rows_evaluated"] == 2400 for m in metrics["hierarchy_mappings"])
    # The member relations ride in the lead's evidence, one per
    # grouped statement.
    assert len(metrics["member_relations"]) == 9
    assert len(lead["grouped_members"]) == 9
    assert len(report.grouped_findings) == 9
    for member in report.grouped_findings:
        assert set(member["subject"]["columns"]) <= hier_cols
        assert member["grouped_under"] == lead["id"]
        assert report.finding(member["id"]) is member
    # No exact pairwise statement over the hierarchy columns is
    # selected any more.
    stray = [
        f
        for f in report.findings
        if f["id"] != lead["id"]
        and set(f["subject"]["columns"]) <= hier_cols
        and f.get("type") in ("functional", "duplicate_encoding")
    ]
    assert stray == []
    assert report.data["selection"]["families"] == 1
    assert report.data["selection"]["grouped_statements"] == 9


def _hierarchy_lead(report):
    leads = [
        f
        for f in report.findings
        if (f.get("specification") or {}).get("form") == "hierarchy"
    ]
    assert len(leads) == 1
    return leads[0]


def test_hierarchy_lead_verifies_against_the_data():
    df = _hierarchy_df()
    report = onceover.profile(df)
    lead = _hierarchy_lead(report)
    rows = report.verify(df).filter(pl.col("id") == lead["id"])
    assert rows.height > 0
    assert set(rows["status"].unique()) == {"ok"}


def test_hierarchy_lead_fails_verification_when_the_chain_breaks():
    df = _hierarchy_df()
    report = onceover.profile(df)
    lead = _hierarchy_lead(report)
    # Move unit_0's rows to a new division: team_0 then maps to two
    # divisions, so the team → division mapping no longer holds.
    broken = df.with_columns(
        pl.when(pl.int_range(pl.len()) < 100)
        .then(pl.lit("division_elsewhere"))
        .otherwise(pl.col("division"))
        .alias("division")
    )
    rows = report.verify(broken).filter(
        (pl.col("id") == lead["id"]) & (pl.col("status") == "mismatch")
    )
    assert rows.height > 0


def test_two_independent_hierarchies_stay_two_leads():
    rows = []
    for i in range(600):
        a2 = (i // 5) % 6
        b2 = (i // 5) % 5
        rows.append(
            {
                "a1": f"a1_{i % 30}",
                "a2": f"a2_{a2}",
                "a3": f"a3_{a2 // 2}",
                "b1": f"b1_{i % 25}",
                "b2": f"b2_{b2}",
                "b3": f"b3_{b2 // 2}",
                "x": float((i * 13) % 17),
            }
        )
    report = onceover.profile(pl.DataFrame(rows))
    leads = [
        f
        for f in report.findings
        if (f.get("specification") or {}).get("form") == "hierarchy"
    ]
    assert len(leads) == 2
    col_sets = {tuple(sorted(f["subject"]["columns"])) for f in leads}
    assert col_sets == {("a1", "a2", "a3"), ("b1", "b2", "b3")}


def test_hierarchy_report_validates_against_schema():
    jsonschema = pytest.importorskip("jsonschema")
    schema = json.loads(
        (Path(__file__).resolve().parent.parent / "docs" / "report-schema.json")
        .read_text()
    )
    report = onceover.profile(_hierarchy_df())
    assert report.grouped_findings, "expected the chain family on this frame"
    validator = jsonschema.Draft202012Validator(schema)
    errors = sorted(validator.iter_errors(report.data), key=lambda e: list(e.path))
    assert not errors, "\n".join(
        f"{'.'.join(str(p) for p in e.path)}: {e.message}" for e in errors[:10]
    )
