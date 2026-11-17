"""Registration is single-point: the Rust detector registry is the
one declaration of each detector's name, membership, trigger, and purpose,
and the finding taxonomy's type strings live in one list in the
report crate. These tests pin the surfaces that used to be
hand-maintained copies — the capability manifest, the generated
blocks of the findings document, the detector counts stated in
prose, and the registry's purpose text — to the registry itself,
so a missed registration fails the suite instead of drifting
silently.
"""

import json
import re
from pathlib import Path

import polars as pl

import onceover

ROOT = Path(__file__).resolve().parent.parent

COUNT_PATTERN = re.compile(r"(\d+)\s+of\s+the\s+(\d+)\s+registered\s+detectors")


def _registry():
    return onceover.available_detectors()


def _counts():
    registry = _registry()
    default = sum(1 for d in registry if d["membership"] == "default_set")
    return default, len(registry)


def test_every_detector_declares_a_purpose():
    for descriptor in _registry():
        assert descriptor["purpose"].strip(), descriptor["id"]


def test_capability_manifest_is_fresh():
    committed = json.loads((ROOT / "docs" / "capability-manifest.json").read_text(encoding="utf-8"))
    expected = {
        "library": "onceover",
        "version": onceover.__version__,
        "generated_by": "scripts/capability_manifest.py",
        "detectors": onceover.available_detectors(),
    }
    assert committed == expected, (
        "docs/capability-manifest.json is stale; regenerate it with "
        "python scripts/capability_manifest.py"
    )


def test_findings_doc_is_fresh():
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "findings_doc", ROOT / "scripts" / "findings_doc.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    committed = (ROOT / "docs" / "findings.md").read_text(encoding="utf-8")
    assert module.render(committed) == committed, (
        "docs/findings.md is stale; regenerate it with "
        "python scripts/findings_doc.py"
    )


def test_detector_counts_in_prose_match_the_registry():
    default, registered = _counts()
    notebook = json.loads(
        (ROOT / "docs" / "book" / "usage" / "capabilities.ipynb").read_text(encoding="utf-8")
    )
    texts = {
        "profile() docstring": onceover.profile.__doc__,
        "AGENTS.md": (ROOT / "AGENTS.md").read_text(encoding="utf-8"),
        "docs/findings.md": (ROOT / "docs" / "findings.md").read_text(encoding="utf-8"),
        "capabilities.ipynb": "\n".join(
            "".join(cell["source"]) for cell in notebook["cells"]
        ),
    }
    for name, text in texts.items():
        matches = COUNT_PATTERN.findall(text)
        assert matches, f"no detector-count statement found in {name}"
        for stated_default, stated_registered in matches:
            assert (int(stated_default), int(stated_registered)) == (
                default,
                registered,
            ), (
                f"{name} says {stated_default} of {stated_registered} "
                f"registered detectors; the registry has {default} of "
                f"{registered}"
            )


def test_profile_coverage_records_no_dropped_findings():
    df = pl.DataFrame(
        {
            "region": ["north", "south", "north", "south"] * 25,
            "revenue": [float(i % 7) for i in range(100)],
        }
    )
    report = onceover.profile(df)
    for name, info in report.data["coverage"].items():
        assert "failed validation" not in info["detail"], (
            f"coverage entry {name} records findings dropped by "
            f"validation: {info['detail']}"
        )
