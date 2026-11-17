"""The packaged agent guide and the capability manifest.

Pins: the guide stays within its size cap (the in-package
quickstart budget); the guide's named hint keys, budgets, and
detector counts match the implementation; the manifest covers
exactly the registered detectors; the book chapter's reference
lists match the guide's; and the caller-facing error paths state
the valid values or the accepted shape.
"""

import json
import re
from datetime import date
from pathlib import Path

import polars as pl
import pytest

import onceover
from onceover._native import hint_keys as _native_hint_keys

DF = pl.DataFrame(
    {
        "d": [date(2024, 1, 1), date(2024, 1, 2), date(2024, 1, 3)],
        "a": [1, 2, 3],
        "b": ["x", "y", "x"],
    }
)

COUNT_PATTERN = re.compile(r"(\d+) of (?:the )?(\d+) (?:registered )?detectors")


def _reference_line(text, prefix):
    for line in text.splitlines():
        if line.startswith(prefix):
            return re.findall(r"`([^`]+)`", line)
    raise AssertionError(f"no line starting with {prefix!r}")


def _registry():
    return {d["id"]: d for d in onceover.available_detectors()}


# --- the guide -------------------------------------------------------


def test_agent_guide_within_the_size_cap():
    guide = onceover.agent_guide()
    assert len(guide) <= 4000
    assert len(guide.encode("utf-8")) <= 4000


def test_agent_guide_teaches_in_order():
    guide = onceover.agent_guide()
    sections = [
        guide.index("## 1. Hand over"),
        guide.index("## 2. The call"),
        guide.index("## 3. Reading"),
        guide.index("## 4. Detector selection"),
    ]
    assert sections == sorted(sections)
    # The handover comes first: the outcome is declared in section 1.
    assert "target=" in guide[: sections[1]]


def test_guide_hint_keys_match_the_implementation():
    named = _reference_line(onceover.agent_guide(), "Hint keys:")
    assert named == onceover.capabilities()["hint_keys"]
    assert named == _native_hint_keys()


def test_guide_budgets_match_the_implementation():
    named = _reference_line(onceover.agent_guide(), "Budgets:")
    assert named == onceover.capabilities()["budgets"]


def test_guide_detector_counts_match_the_registry():
    registry = _registry()
    default = sum(1 for d in registry.values() if d["membership"] == "default_set")
    match = COUNT_PATTERN.search(onceover.agent_guide())
    assert match is not None
    assert (int(match.group(1)), int(match.group(2))) == (default, len(registry))


def test_guide_names_the_target_triggered_passes():
    # The guide's first section promises three target-triggered
    # passes by name; a fourth registering fails here until the
    # guide is updated.
    registry = _registry()
    target_triggered = {i for i, d in registry.items() if d["trigger"] == "target"}
    assert target_triggered == {"target_shapes", "leakage", "model_failures"}
    guide = onceover.agent_guide()
    for detector_id in target_triggered:
        assert f"`{detector_id}`" in guide


def test_hints_triggered_passes_are_the_two_documented_ones():
    registry = _registry()
    hints_triggered = {i for i, d in registry.items() if d["trigger"] == "hints"}
    assert hints_triggered == {"expectations", "group_contrast"}
    assert "`expectations`" in onceover.agent_guide() or "expectations pass" in (
        onceover.agent_guide()
    )


# --- the manifest ----------------------------------------------------


def test_capabilities_cover_exactly_the_registered_detectors():
    manifest = onceover.capabilities()
    registry = onceover.available_detectors()
    assert [d["id"] for d in manifest["detectors"]] == [d["id"] for d in registry]
    for entry, record in zip(manifest["detectors"], registry):
        assert entry["membership"] == record["membership"]
        assert entry["trigger"] == record["trigger"]
        assert entry["purpose"] == record["purpose"]
        assert entry.get("requires", []) == record["requires"]
        assert set(entry) <= {"id", "membership", "trigger", "purpose", "requires"}


def test_capabilities_header_matches_the_package():
    manifest = onceover.capabilities()
    assert manifest["version"] == onceover.__version__
    assert manifest["budgets"] == ["quick", "standard", "deep"]
    assert manifest["hint_keys"] == _native_hint_keys()


def test_every_advertised_hint_key_is_accepted():
    benign = {
        "time": "d",
        "ordered_axis": "a",
        "level_order": {"b": ["x", "y"]},
        "order_by": ["d"],
        "no_gaps": ["d"],
        "cadence": {"d": 1},
        "dimensions": ["b"],
        "measures": ["a"],
        "weights": ["a"],
        "roles": {"a": "measure"},
        "hierarchies": [["b", "a"]],
        "exclude_columns": ["not_a_column"],
        "labels": {"a": "A"},
        "contrast": {"column": "b", "value": "x"},
    }
    assert set(benign) == set(onceover.capabilities()["hint_keys"])
    for key, value in benign.items():
        onceover.profile(DF, hints={key: value})


# --- error paths -------------------------------------------------------


def test_unknown_budget_error_lists_the_budgets():
    with pytest.raises(ValueError) as excinfo:
        onceover.profile(DF, budget="bogus")
    assert str(excinfo.value) == (
        "unknown budget 'bogus' (expected quick | standard | deep)"
    )


def test_budget_type_error_names_the_budgets():
    with pytest.raises(TypeError) as excinfo:
        onceover.profile(DF, budget=3)
    message = str(excinfo.value)
    assert "quick" in message and "deep" in message


def test_unknown_detector_error_lists_the_registered_ids():
    with pytest.raises(ValueError) as excinfo:
        onceover.profile(DF, detectors=["bogus"])
    message = str(excinfo.value)
    assert "unknown detector: bogus" in message
    for detector_id in _registry():
        assert detector_id in message


def test_detectors_bare_string_is_a_type_error():
    with pytest.raises(TypeError) as excinfo:
        onceover.profile(DF, detectors="leakage")
    assert "list of detector IDs" in str(excinfo.value)


def test_detectors_non_string_entry_is_a_type_error():
    with pytest.raises(TypeError) as excinfo:
        onceover.profile(DF, detectors=[3])
    assert "detector ID strings" in str(excinfo.value)


def test_unknown_hint_key_error_lists_the_keys():
    with pytest.raises(ValueError) as excinfo:
        onceover.profile(DF, hints={"bogus": 1})
    message = str(excinfo.value)
    for key in onceover.capabilities()["hint_keys"]:
        assert key in message


def test_hints_must_be_a_dict():
    with pytest.raises(TypeError) as excinfo:
        onceover.profile(DF, hints=["time"])
    assert "dict" in str(excinfo.value)


def test_cadence_shape_error_states_the_accepted_values():
    with pytest.raises(ValueError) as excinfo:
        onceover.profile(DF, hints={"cadence": {"a": [1]}})
    message = str(excinfo.value)
    assert "cadence must be" in message
    assert "business_day" in message


def test_contrast_unknown_field_is_an_error():
    with pytest.raises(ValueError) as excinfo:
        onceover.profile(DF, hints={"contrast": {"column": "b", "valu": 1}})
    assert "valu" in str(excinfo.value)


def test_target_type_error():
    with pytest.raises(TypeError) as excinfo:
        onceover.profile(DF, target=3)
    assert "column name" in str(excinfo.value)


def test_deadline_type_error():
    with pytest.raises(TypeError):
        onceover.profile(DF, deadline_seconds="soon")


def test_cap_type_and_sign_errors():
    with pytest.raises(TypeError) as excinfo:
        onceover.profile(DF, sample_rows="many")
    assert "must be an integer" in str(excinfo.value)
    with pytest.raises(ValueError) as excinfo:
        onceover.profile(DF, sample_rows=-1)
    assert "sample_rows must be positive" in str(excinfo.value)
    with pytest.raises(ValueError) as excinfo:
        onceover.profile(DF, max_findings=0)
    assert "max_findings must be positive" in str(excinfo.value)


# --- the book chapter ---------------------------------------------------

CHAPTER = (
    Path(__file__).resolve().parents[1]
    / "docs"
    / "book"
    / "usage"
    / "talking_to_the_profiler.ipynb"
)


def _chapter_sources():
    notebook = json.loads(CHAPTER.read_text(encoding="utf-8"))
    markdown = "\n".join(
        "".join(cell["source"])
        for cell in notebook["cells"]
        if cell["cell_type"] == "markdown"
    )
    outputs = []
    for cell in notebook["cells"]:
        if cell["cell_type"] != "code":
            continue
        for output in cell.get("outputs", []):
            if "text" in output:
                outputs.append("".join(output["text"]))
    return markdown, outputs


def test_book_chapter_reference_lists_match_the_guide():
    markdown, _ = _chapter_sources()
    guide = onceover.agent_guide()
    assert _reference_line(markdown, "Hint keys:") == _reference_line(
        guide, "Hint keys:"
    )
    assert _reference_line(markdown, "Budgets:") == _reference_line(guide, "Budgets:")


def test_book_chapter_capabilities_output_matches_the_registry():
    _, outputs = _chapter_sources()
    printed = [o for o in outputs if '"detectors"' in o]
    assert printed, "the chapter prints no capabilities() output"
    manifest = json.loads(printed[0])
    assert [d["id"] for d in manifest["detectors"]] == list(_registry())
    assert manifest["hint_keys"] == _native_hint_keys()
