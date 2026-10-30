"""The published report JSON Schema (issue #28): every live report
must validate against docs/report-schema.json, so an agent can code
against the contract instead of inferring shape from examples."""
import json
from pathlib import Path

import polars as pl
import pytest

jsonschema = pytest.importorskip("jsonschema")

import onceover

SCHEMA = json.loads(
    (Path(__file__).resolve().parent.parent / "docs" / "report-schema.json").read_text()
)


def _validate(report):
    validator = jsonschema.Draft202012Validator(SCHEMA)
    errors = sorted(validator.iter_errors(report.data), key=lambda e: list(e.path))
    assert not errors, "\n".join(
        f"{'.'.join(str(p) for p in e.path)}: {e.message}" for e in errors[:10]
    )


def test_schema_is_well_formed():
    jsonschema.Draft202012Validator.check_schema(SCHEMA)
    assert SCHEMA["properties"]["schema_version"] == {"const": "0.2"}
    assert "type" in SCHEMA["$defs"]["finding"]["required"]


def test_every_finding_carries_a_type():
    # The type field is regular: no finding serializes without one,
    # so consumers (and DataFrame outputs) never special-case it.
    df = pl.DataFrame(
        {
            "row_id": list(range(300)),
            "amt": [10.0 + (i % 41) * 0.37 for i in range(300)],
            "city": [f"city{i % 7}" for i in range(300)],
        }
    )
    report = onceover.profile(df)
    assert report.findings, "expected findings on this frame"
    for f in report.findings:
        assert isinstance(f.get("type"), str) and f["type"], f
    frame = report.findings_frame()
    assert frame["type"].null_count() == 0


def test_mixed_frame_report_validates():
    df = pl.DataFrame(
        {
            "row_id": list(range(300)),
            "flag": [i % 3 == 0 for i in range(300)],
            "amt": [10.0 + (i % 41) * 0.37 for i in range(300)],
            "city": [f"city{i % 7}" for i in range(300)],
            "when": [f"2024-02-{(i % 28) + 1:02d}" for i in range(300)],
        }
    )
    _validate(onceover.profile(df))


def test_panel_report_validates():
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
    _validate(onceover.profile(df, hints={"time": "year"}))


def test_long_format_report_validates():
    entity, indicator, value = [], [], []
    for e in range(60):
        entity.append(f"E{e:02d}")
        indicator.append("gdp")
        value.append(100.0 + (e % 50))
        entity.append(f"E{e:02d}")
        indicator.append("inflation")
        value.append(2.0 + (e % 8) * 0.5)
    df = pl.DataFrame({"entity": entity, "indicator": indicator, "value": value})
    _validate(onceover.profile(df))
