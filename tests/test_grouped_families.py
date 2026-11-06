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
