"""Schema 0.3 facets: every finding carries them, the category is the
one the facets derive, and consumers can filter on them."""
from pathlib import Path

import polars as pl

import onceover

SOYBEAN = (
    Path(__file__).resolve().parent.parent
    / "benchmarks"
    / "corpus"
    / "data"
    / "gritbot"
    / "soybean"
    / "data.csv"
)


def _soybean_report():
    return onceover.profile(pl.read_csv(SOYBEAN), budget="standard")


def test_every_finding_carries_facets():
    report = _soybean_report()
    assert report.data["schema_version"] == "0.6"
    assert report.findings
    for f in report.findings:
        facets = f["facets"]
        assert facets["scope"] in {"column", "columns", "rows", "sequence", "frame"}
        assert facets["object"] in {
            "distribution",
            "relation",
            "structure",
            "temporal_behaviour",
            "role",
        }
        assert facets["evidence"] in {"exact", "approximate", "model_based", "inferred"}
        assert facets["conditionality"] in {
            "unconditional",
            "value_conditioned",
            "segment_conditioned",
        }
        assert facets["temporality"] in {"atemporal", "ordered", "change_across_segments"}
        assert facets["status"] in {"regularity", "deviation"}


def test_dependency_and_rule_differ_only_in_conditionality():
    report = _soybean_report()
    fd = report.select(category="dependencies", type="functional")[0]
    rule = report.select(category="conditional_rules", type="if_then")[0]
    a, b = fd["facets"], rule["facets"]
    for field in ("scope", "object", "temporality", "status"):
        assert a[field] == b[field]
    assert a["conditionality"] == "unconditional"
    assert b["conditionality"] == "value_conditioned"


def test_select_filters_on_facets():
    report = _soybean_report()
    relations = report.select(object="relation")
    assert relations
    assert all(f["facets"]["object"] == "relation" for f in relations)
    deviations = report.select(status="deviation")
    assert all(f["facets"]["status"] == "deviation" for f in deviations)


def test_findings_frame_carries_facet_columns():
    report = _soybean_report()
    frame = report.findings_frame()
    for column in (
        "scope",
        "object",
        "evidence_kind",
        "conditionality",
        "temporality",
        "quality_dimension",
        "status",
    ):
        assert column in frame.columns
    assert frame.height == len(report.findings)
